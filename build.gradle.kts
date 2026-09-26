// Declares every plugin version once, at the root, with `apply false`.
//
// Without this, subprojects each specify their own version and the Kotlin
// Gradle plugin gets loaded multiple times in one build. Gradle warns that this
// "is not supported and may break the build" -- and it is the kind of warning
// that turns into a real failure on some later upgrade rather than today.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
