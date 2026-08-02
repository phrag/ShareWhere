plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

/**
 * The HTTP transport, and the ONLY module that links an HTTP client.
 *
 * Kept separate so the offline flavor -- which does not depend on it -- can be
 * shown to contain no networking code at all, rather than merely promising not
 * to use any.
 */
android {
    namespace = "app.sharewhere.net"
    compileSdk = 36
    defaultConfig { minSdk = 26 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(project(":core-rust"))
    implementation(libs.okhttp)
    testImplementation(libs.junit)
}
