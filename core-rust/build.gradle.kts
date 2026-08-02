import org.gradle.internal.os.OperatingSystem

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

/**
 * Wraps the Rust core: cross-compiles it with cargo-ndk, generates the Kotlin
 * bindings with uniffi-bindgen, and exposes both to the app.
 *
 * Everything Rust-shaped lives here so nothing else in the Gradle build has to
 * know about cargo, the NDK, or JNA.
 */

android {
    namespace = "app.sharewhere.core"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    sourceSets["main"].java.srcDir(layout.buildDirectory.dir("generated/uniffi"))
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("generated/jniLibs"))
}

dependencies {
    // MUST be the AAR. The plain JAR resolves and then fails on device.
    api("${libs.jna.get()}@aar")
    testImplementation(libs.junit)
}

val rustDir = rootProject.file("rust")
val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")

/**
 * 16 KB page alignment is mandatory on Android 15+. Without it the app will not
 * install on newer devices, and the failure gives no hint as to why.
 */
val rustFlags = "-C link-arg=-Wl,-z,max-page-size=16384"

val cargoBuild by tasks.registering(Exec::class) {
    group = "rust"
    description = "Cross-compiles the Rust core for every Android ABI."
    workingDir = rustDir
    environment("RUSTFLAGS", rustFlags)

    val profile = if (project.hasProperty("rustRelease")) "release" else "debug"
    val out = layout.buildDirectory.dir("generated/jniLibs").get().asFile

    commandLine(
        buildList {
            add("cargo")
            add("ndk")
            abis.forEach { add("-t"); add(it) }
            add("-o"); add(out.absolutePath)
            add("build")
            add("-p"); add("sharewhere-ffi")
            if (profile == "release") add("--release")
        },
    )

    inputs.dir(rustDir.resolve("crates"))
    inputs.file(rustDir.resolve("Cargo.lock"))
    outputs.dir(out)
}

val generateBindings by tasks.registering(Exec::class) {
    group = "rust"
    description = "Generates Kotlin bindings from the built library."
    dependsOn(cargoBuild)
    workingDir = rustDir

    val libraryName = if (OperatingSystem.current().isMacOsX) "libsharewhere.dylib" else "libsharewhere.so"
    val profile = if (project.hasProperty("rustRelease")) "release" else "debug"
    val library = rustDir.resolve("target/$profile/$libraryName")
    val out = layout.buildDirectory.dir("generated/uniffi").get().asFile

    commandLine(
        "cargo", "run", "-p", "sharewhere-ffi", "--features", "bindgen",
        "--bin", "uniffi-bindgen", "--",
        "generate", "--library", library.absolutePath,
        "--language", "kotlin", "--out-dir", out.absolutePath,
    )

    outputs.dir(out)
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    dependsOn(generateBindings)
}
tasks.named("preBuild") { dependsOn(generateBindings) }

/**
 * Builds the core for the host so JVM unit tests can exercise the real FFI
 * without an emulator. This catches binding drift -- a renamed record field, a
 * changed enum variant -- in seconds rather than at runtime on a device.
 */
val cargoBuildHost by tasks.registering(Exec::class) {
    group = "rust"
    workingDir = rustDir
    commandLine("cargo", "build", "-p", "sharewhere-ffi")
}

tasks.withType<Test>().configureEach {
    dependsOn(cargoBuildHost, generateBindings)
    systemProperty("jna.library.path", rustDir.resolve("target/debug").absolutePath)
}
