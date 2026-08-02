//! Public data types for the sanitizer.

use std::fmt;

/// How aggressive to be, and what the user has consented to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// Strip affiliate/referral parameters as well as pure tracking ones.
    ///
    /// On by default, but separated out because removing them costs a creator
    /// their commission, which some users would rather not do.
    pub remove_referral: bool,

    /// Unwrap URLs that merely wrap a destination (`google.com/url?q=…`).
    pub follow_redirect_params: bool,

    /// Cap on redirect unwrapping across the whole recursion, not per level.
    pub max_hops: u8,
}

impl Default for SanitizeOptions {
    fn default() -> Self {
        Self {
            remove_referral: true,
            follow_redirect_params: true,
            max_hops: 5,
        }
    }
}

/// Why a parameter was removed. Drives the wording in the preview sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalKind {
    /// Analytics/tracking (`utm_*`, `fbclid`, `igshid`).
    Tracking,
    /// Affiliate/referral (`tag`, `ref`).
    Referral,
    /// A whole-URL rewrite, e.g. Amazon's `/ref=…` path segment.
    RawRule,
}

impl fmt::Display for RemovalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemovalKind::Tracking => write!(f, "tracking"),
            RemovalKind::Referral => write!(f, "referral"),
            RemovalKind::RawRule => write!(f, "rewrite"),
        }
    }
}

/// One thing that was taken out, for the "here's what we removed" UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedParam {
    pub key: String,
    pub value: String,
    pub kind: RemovalKind,
    /// Catalog provider that matched, e.g. `amazon` or `globalRules`.
    pub provider: String,
}

/// The result of cleaning a single URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized {
    pub original: String,
    pub cleaned: String,
    pub changed: bool,
    pub removed: Vec<RemovedParam>,
    /// Providers that fired, in the order they did.
    pub providers: Vec<String>,
    /// A wrapped destination URL was extracted, so the host legitimately
    /// changed. This is the one case where the host-preservation invariant is
    /// allowed not to hold.
    pub unwrapped_redirect: bool,
    /// The URL is *entirely* a tracker rather than a link with tracking added.
    /// Nothing was cleaned; the UI should say so rather than pretend.
    pub complete_provider: bool,
}

/// The result of cleaning free text that happens to contain URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextResult {
    /// The input with every URL replaced by its cleaned form; surrounding
    /// prose is left alone.
    pub cleaned_text: String,
    pub urls: Vec<Sanitized>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not parseable as a URL at all.
    InvalidUrl(String),
    /// Parsed, but not `http`/`https`. Refusing these is a security control,
    /// not tidiness: see the note on redirect targets in `engine.rs`.
    UnsupportedScheme(String),
    /// Beyond [`MAX_INPUT_LEN`](crate::MAX_INPUT_LEN). Bounds the cost of
    /// whole-URL regex replacement on adversarial input.
    InputTooLong { len: usize, max: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidUrl(s) => write!(f, "not a valid URL: {s}"),
            Error::UnsupportedScheme(s) => write!(f, "unsupported scheme: {s}"),
            Error::InputTooLong { len, max } => {
                write!(f, "input is {len} bytes, maximum is {max}")
            }
        }
    }
}

impl std::error::Error for Error {}
