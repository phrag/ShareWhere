//! Location parsing and link generation for ShareWhere.
//!
//! Pure and offline. The one thing that cannot be done offline —
//! `maps.app.goo.gl` short links, which carry no coordinates and are exactly
//! what Google Maps puts on the clipboard — is surfaced as
//! [`LocationParse::NeedsNetwork`] rather than silently fetched.
//!
//! ```
//! use sharewhere_geo::{parse_location, render_links, LocationParse, RenderOptions};
//!
//! let LocationParse::Point { point, .. } = parse_location("geo:51.5007,-0.1246") else {
//!     panic!("expected a point");
//! };
//! let links = render_links(&point, &RenderOptions::default());
//! assert!(links.iter().any(|l| l.value.starts_with("https://omaps.app/")));
//! ```

pub mod ge0;
pub mod olc;

mod emit;
mod parse;
mod point;

pub use emit::{format_dms, render_all_text, render_links, GeoLink, LinkId, RenderOptions};
pub use parse::{
    extract_location_from_html, parse_location, LocationParse, LocationSource, Unsupported,
};
pub use point::{GeoPoint, Precision};
