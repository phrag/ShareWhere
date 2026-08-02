//! URL sanitiser for ShareWhere.
//!
//! Pure and deterministic: no I/O, no networking, no clock, no randomness.
//! Everything network-shaped lives in `sharewhere-core`, which is what makes
//! this crate cheap to fuzz and property-test.
//!
//! ```
//! use sharewhere_url::{sanitize_url, SanitizeOptions};
//!
//! let opts = SanitizeOptions::default();
//! let out = sanitize_url("https://example.com/p?utm_source=news&id=7", &opts).unwrap();
//! assert_eq!(out.cleaned, "https://example.com/p?id=7");
//! assert_eq!(out.removed.len(), 1);
//! ```

mod compiled;
mod engine;
mod text;
mod types;

pub use compiled::warm;
pub use engine::{sanitize_url, MAX_INPUT_LEN};
pub use text::{find_urls, sanitize_text};
pub use types::{Error, RemovalKind, RemovedParam, SanitizeOptions, Sanitized, TextResult};
