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
 * Permissions the build tooling injects, which grant ShareWhere nothing.
 *
 * `DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` is defined by androidx.core and
 * namespaced under our own application id. It is `signature` protection level,
 * so only ShareWhere itself can ever hold it, and it exists solely so
 * androidx can guard a broadcast receiver it registers at runtime against
 * other apps on pre-Android-13 devices. It asks the system for no capability
 * and never appears in the permission list a user sees.
 *
 * It is allow-listed rather than stripped with `tools:node="remove"` because
 * removing it would break any dependency that does register such a receiver,
 * and that failure would only show up at runtime on a device -- which is not
 * something this project can currently test.
 */
private val autoInjected = setOf(
    "app.sharewhere.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
    "app.sharewhere.standard.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
)

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

    // Discovered by walking rather than hard-coded, because the intermediates
    // layout is an AGP implementation detail that moves between versions
    // (merged_manifest vs merged_manifests, and the task-name subdirectory).
    val buildDir = layout.buildDirectory
    doLast {
        val manifests = buildDir.get().asFile.resolve("intermediates")
            .walkTopDown()
            .filter { it.name == "AndroidManifest.xml" }
            .filter { it.path.contains("offline", ignoreCase = true) }
            .filter { it.path.contains("merged_manifest", ignoreCase = true) }
            .toList()

        check(manifests.isNotEmpty()) {
            "No merged offline manifest found under ${buildDir.get().asFile}/intermediates -- " +
                "assemble the offline flavor first, or AGP has moved the output again."
        }

        manifests.forEach { manifest ->
            val declared = Regex("""<uses-permission[^>]*android:name="([^"]+)"""")
                .findAll(manifest.readText())
                .map { it.groupValues[1] }
                .filterNot { it in autoInjected }
                .toList()
            check(declared.isEmpty()) {
                "The offline flavor must declare no capability-granting permissions, " +
                    "but ${manifest.path} has: $declared"
            }
        }
        logger.lifecycle(
            "offline flavor grants itself no capabilities (${manifests.size} manifest(s) checked)",
        )
    }
}

// Deliberately NOT wired with finalizedBy(assembleOffline*): a failing check
// would then also fail the assemble task, and CI would have no APK to publish.
// The build is more useful than the assertion is urgent, so CI runs this as its
// own step after the artifact has been uploaded.
tasks.named("check") { dependsOn(verifyOfflineFlavorHasNoPermissions) }
