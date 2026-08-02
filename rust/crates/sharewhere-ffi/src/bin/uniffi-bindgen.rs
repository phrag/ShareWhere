//! Binding generator, invoked by the Gradle build.
//!
//! UniFFI wants the generator built from the same crate graph as the library so
//! the versions cannot drift. Generated Kotlin is never committed — see
//! `.gitignore`.

fn main() {
    uniffi::uniffi_bindgen_main()
}
