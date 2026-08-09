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

    lint {
        // The generated bindings call java.lang.ref.Cleaner, which is API 33+,
        // and lint flags it against our minSdk of 26.
        //
        // It is a false positive. UniFFI guards the call with
        // `Class.forName("java.lang.ref.Cleaner")` inside a
        // `catch (ClassNotFoundException)`, so on API 26-32 the class is never
        // loaded and it falls back to a JNA-based cleaner. Lint cannot follow
        // reflection, so it sees only the call site.
        //
        // Scoped to this module deliberately: it contains no hand-written
        // Kotlin at all, only the bindings regenerated on every build, so
        // there is nothing here a NewApi check could usefully protect. The app
        // and core-net modules keep it enabled.
        disable += "NewApi"
    }
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

/**
 * Which Cargo profile goes *into the APK*.
 *
 * Worth being deliberate about: a debug Rust build ships unstripped, with full
 * debug info, for three ABIs — which is most of a 95 MB APK. The release
 * profile strips symbols and optimises for size, and is what any build someone
 * installs should carry. Pass `-PrustRelease`.
 *
 * This is separate from the *host* build below, which only ever feeds
 * uniffi-bindgen and the JVM tests and is never packaged, so it stays debug for
 * build speed. Conflating the two was a live bug: `generateBindings` used to
 * read this flag and would then look for a host library that `cargoBuildHost`
 * had never built.
 */
val androidRustProfile = if (project.hasProperty("rustRelease")) "release" else "debug"

/**
 * The host build is never shipped, so it is always debug.
 *
 * Plain `val`, not `const val`: a build script's body is not a top level in the
 * sense Kotlin means, so `const` there fails to compile the script.
 */
val hostRustProfile = "debug"

val cargoBuild by tasks.registering(Exec::class) {
    group = "rust"
    description = "Cross-compiles the Rust core for every Android ABI."
    workingDir = rustDir
    environment("RUSTFLAGS", rustFlags)

    val profile = androidRustProfile
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

/**
 * Builds the core for the host machine.
 *
 * Needed twice over: uniffi-bindgen has to dlopen a library built for THIS
 * machine to read the FFI metadata out of it, and the JVM unit tests link the
 * same library so they exercise the real bindings without an emulator.
 */
val cargoBuildHost by tasks.registering(Exec::class) {
    group = "rust"
    description = "Builds the Rust core for the host machine."
    workingDir = rustDir
    commandLine("cargo", "build", "-p", "sharewhere-ffi")

    inputs.dir(rustDir.resolve("crates"))
    inputs.file(rustDir.resolve("Cargo.lock"))
}

val generateBindings by tasks.registering(Exec::class) {
    group = "rust"
    description = "Generates Kotlin bindings from the built library."
    // cargoBuild cross-compiles for the Android ABIs, whose artifacts land under
    // target/<triple>/. uniffi-bindgen needs a library it can dlopen on THIS
    // machine, so the host build is a separate, mandatory prerequisite.
    dependsOn(cargoBuild, cargoBuildHost)
    workingDir = rustDir

    val libraryName = if (OperatingSystem.current().isMacOsX) "libsharewhere.dylib" else "libsharewhere.so"
    // Always the host profile, never androidRustProfile: this library is only
    // read for its FFI metadata, and it is cargoBuildHost that produces it.
    val library = rustDir.resolve("target/$hostRustProfile/$libraryName")
    val out = layout.buildDirectory.dir("generated/uniffi").get().asFile

    doFirst {
        out.mkdirs()
        // uniffi-bindgen reports a missing library as a bare
        // "No such file or directory (os error 2)" with no path, which is
        // genuinely hard to diagnose. Say which file, and say it here.
        check(library.isFile) {
            "uniffi-bindgen needs the host library at $library, which does not exist. " +
                "`cargo run --bin uniffi-bindgen` does NOT build the package's cdylib, " +
                "so cargoBuildHost has to have run first."
        }
    }

    commandLine(
        "cargo", "run", "-p", "sharewhere-ffi", "--features", "bindgen",
        "--bin", "uniffi-bindgen", "--",
        "generate", "--library", library.absolutePath,
        "--language", "kotlin", "--out-dir", out.absolutePath,
        // uniffi-bindgen otherwise shells out to ktlint to pretty-print its
        // output, which no CI runner has. Nothing reads this code and it is
        // regenerated every build, so the formatting is irrelevant.
        "--no-format",
    )

    outputs.dir(out)
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    dependsOn(generateBindings)
}
tasks.named("preBuild") { dependsOn(generateBindings) }

// JVM unit tests link the host library so they exercise the real bindings,
// catching binding drift -- a renamed record field, a changed enum variant --
// in seconds rather than at runtime on a device.
tasks.withType<Test>().configureEach {
    dependsOn(cargoBuildHost, generateBindings)
    systemProperty("jna.library.path", rustDir.resolve("target/debug").absolutePath)
}
