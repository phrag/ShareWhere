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

        // Surfaced on the About screen so a user can say which build they have.
        buildConfigField("String", "PROJECT_URL", "\"https://github.com/phrag/ShareWhere\"")
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
    implementation(project(":core-net"))

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
 * Permissions the app is allowed to declare, and nothing else.
 *
 * There is one APK now, and it does declare INTERNET — short links genuinely
 * cannot be resolved without it. So the old "no permissions at all" claim is
 * gone, and this check has changed job accordingly: it no longer asserts an
 * empty list, it asserts *this exact list*. A dependency that quietly drags in
 * location, storage, contacts or anything else still fails the build.
 *
 * `DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` is defined by androidx under our
 * own application id at `signature` level, so only ShareWhere can hold it. It
 * guards a receiver androidx registers internally on pre-Android-13 devices and
 * grants nothing.
 */
private val allowedPermissions = setOf(
    "android.permission.INTERNET",
    "app.sharewhere.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
)

val verifyDeclaredPermissions by tasks.registering {
    group = "verification"
    description = "Fails if the merged manifest declares a permission outside the allowed set."

    // Discovered by walking rather than hard-coded: the intermediates layout is
    // an AGP implementation detail that has moved between versions.
    val buildDir = layout.buildDirectory
    doLast {
        val manifests = buildDir.get().asFile.resolve("intermediates")
            .walkTopDown()
            .filter { it.name == "AndroidManifest.xml" }
            .filter { it.path.contains("merged_manifest", ignoreCase = true) }
            .toList()

        check(manifests.isNotEmpty()) {
            "No merged manifest found under ${buildDir.get().asFile}/intermediates -- " +
                "assemble first, or AGP has moved the output again."
        }

        manifests.forEach { manifest ->
            val unexpected = Regex("""<uses-permission[^>]*android:name="([^"]+)"""")
                .findAll(manifest.readText())
                .map { it.groupValues[1] }
                .filterNot { it in allowedPermissions }
                .toList()
            check(unexpected.isEmpty()) {
                "${manifest.path} declares unexpected permission(s): $unexpected"
            }
        }
        logger.lifecycle(
            "manifest declares only the allowed permissions (${manifests.size} checked)",
        )
    }
}

// Deliberately NOT wired with finalizedBy(assemble*): a failing check would then
// also fail the assemble task, and CI would have no APK to publish. The build is
// more useful than the assertion is urgent, so CI runs this after uploading.
tasks.named("check") { dependsOn(verifyDeclaredPermissions) }
