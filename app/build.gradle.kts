plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "app.sharewhere"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.sharewhere"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    flavorDimensions += "network"
    productFlavors {
        /**
         * The recommended download. Declares no permissions at all -- not even
         * INTERNET -- and does not link an HTTP client. Short links cannot be
         * resolved, and the app says so rather than offering a dead button.
         *
         * The point is that the privacy claim is checkable by a stranger with
         * apkanalyzer instead of taken on trust. `verifyOfflineFlavorHasNoPermissions`
         * below fails the build if that ever stops being true.
         */
        create("offline") {
            dimension = "network"
            isDefault = true
            buildConfigField("boolean", "NETWORK_AVAILABLE", "false")
        }

        /**
         * Adds INTERNET so short links can be expanded -- but only after the
         * user taps through a per-link consent prompt. There is deliberately no
         * "always resolve" setting in v1.
         */
        create("standard") {
            dimension = "network"
            applicationIdSuffix = ".standard"
            versionNameSuffix = "-standard"
            buildConfigField("boolean", "NETWORK_AVAILABLE", "true")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(project(":core-rust"))
    // Only the standard flavor gets an HTTP client.
    "standardImplementation"(project(":core-net"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.datastore.preferences)
    implementation(libs.kotlinx.coroutines.android)

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)
}

/**
 * The privacy regression test.
 *
 * "We ask for no permissions" is the app's central claim, and the kind of thing
 * a stray library or a merged manifest can quietly break. Asserting it against
 * the *merged* manifest means it cannot regress by accident.
 */
val verifyOfflineFlavorHasNoPermissions by tasks.registering {
    group = "verification"
    description = "Fails if the offline flavor's merged manifest declares any permission."

    val manifests = layout.buildDirectory.dir("intermediates/merged_manifest/offline")
    inputs.dir(manifests).optional(true)

    doLast {
        val files = manifests.get().asFile.walkTopDown().filter { it.name == "AndroidManifest.xml" }
        var checked = 0
        files.forEach { manifest ->
            checked++
            val declared = Regex("""<uses-permission[^>]*android:name="([^"]+)"""")
                .findAll(manifest.readText())
                .map { it.groupValues[1] }
                .toList()
            check(declared.isEmpty()) {
                "The offline flavor must declare no permissions, but ${manifest.name} has: $declared"
            }
        }
        check(checked > 0) { "No merged manifest found to verify -- assemble the offline flavor first." }
        logger.lifecycle("offline flavor declares no permissions ($checked manifest(s) checked)")
    }
}

tasks.matching { it.name.startsWith("assembleOffline") }.configureEach {
    finalizedBy(verifyOfflineFlavorHasNoPermissions)
}
