//! The ShareWhere core facade.
//!
//! Ties the URL sanitiser and the geo crate together behind one entry point,
//! and owns every policy decision — how precise a coordinate to share, whether
//! affiliate parameters go, and above all whether the network may be touched.
//!
//! Still contains no I/O. When a request is genuinely unavoidable, the core
//! describes it and the platform layer performs it; see [`ResolveSession`].
//!
//! ```
//! use sharewhere_core::{Options, Outcome, ResolveSession, Step};
//!
//! // Offline by default: nothing leaves the device.
//! let session = ResolveSession::new("geo:51.5007,-0.1246", Options::default());
//! let Step::Done(Outcome::Location { point, .. }) = session.advance() else {
//!     panic!("expected a location");
//! };
//! assert!((point.lat - 51.5007).abs() < 0.001);
//! ```

mod options;
mod session;

pub use options::Options;
pub use session::{
    FetchReason, FetchRequest, FetchResponse, HttpMethod, Outcome, ResolveSession, SessionError,
    Step,
};

// Re-exported so the platform layer has a single crate to depend on.
pub use sharewhere_geo::{
    format_dms, render_all_text, render_links, GeoLink, GeoPoint, LinkId, LocationSource,
    Precision, RenderOptions,
};
pub use sharewhere_url::{
    sanitize_text, sanitize_url, warm, Error as UrlError, RemovalKind, RemovedParam,
    SanitizeOptions, Sanitized, TextResult,
};

/// Compile the always-on rules ahead of the first share, so the share-sheet
/// activity feels instant rather than merely fast. Safe to call more than once.
pub fn warm_up() {
    warm();
}
