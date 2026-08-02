//! The UniFFI boundary.
//!
//! Nothing but wrapper types and `#[uniffi::export]` shims. All the logic lives
//! in `sharewhere-core` and below, so a UniFFI upgrade or a binding-generator
//! quirk never forces a change to anything that matters.
//!
//! The wrappers exist because UniFFI's derives cannot be applied to types from
//! other crates. They are mechanical, and the conversions here are the only
//! place the FFI shape and the core shape are allowed to differ.
//!
//! Two things that break this at runtime rather than compile time, recorded
//! here because they cost hours otherwise:
//!
//! * The release profile must keep `panic = "unwind"`. UniFFI converts panics
//!   into FFI errors via `catch_unwind`; under `panic = "abort"` a bad input
//!   would take the whole Android process down instead.
//! * Kotlin needs the **AAR** build of JNA (>= 5.12). The plain JAR resolves
//!   fine and then fails on device.

use std::sync::Arc;

use sharewhere_core as core;

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------- options ---

#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// ~11 cm, what every other maps app emits.
    Exact,
    /// ~100 m — the building, not the doorway.
    Approximate,
    /// ~1 km — the neighbourhood.
    Coarse,
}

impl From<Precision> for core::Precision {
    fn from(value: Precision) -> Self {
        match value {
            Precision::Exact => core::Precision::Exact,
            Precision::Approximate => core::Precision::Approximate,
            Precision::Coarse => core::Precision::Coarse,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct Options {
    #[uniffi(default = true)]
    pub remove_referral: bool,
    #[uniffi(default = true)]
    pub follow_redirect_params: bool,
    #[uniffi(default = 5)]
    pub max_hops: u8,
    /// Off by default, and granted for a single resolve rather than globally.
    #[uniffi(default = false)]
    pub allow_network: bool,
    pub precision: Precision,
    #[uniffi(default = true)]
    pub include_label: bool,
}

impl From<Options> for core::Options {
    fn from(value: Options) -> Self {
        core::Options {
            sanitize: core::SanitizeOptions {
                remove_referral: value.remove_referral,
                follow_redirect_params: value.follow_redirect_params,
                max_hops: value.max_hops,
            },
            allow_network: value.allow_network,
            precision: value.precision.into(),
            include_label: value.include_label,
        }
    }
}

/// The privacy defaults, so callers do not have to reconstruct them and risk
/// getting one wrong.
#[uniffi::export]
pub fn default_options() -> Options {
    Options {
        remove_referral: true,
        follow_redirect_params: true,
        max_hops: 5,
        allow_network: false,
        precision: Precision::Exact,
        include_label: true,
    }
}

// ------------------------------------------------------------------- urls ---

#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq)]
pub enum RemovalKind {
    Tracking,
    Referral,
    Rewrite,
}

#[derive(uniffi::Record, Clone)]
pub struct RemovedParam {
    pub key: String,
    pub value: String,
    pub kind: RemovalKind,
    pub provider: String,
}

#[derive(uniffi::Record, Clone)]
pub struct Sanitized {
    pub original: String,
    pub cleaned: String,
    pub changed: bool,
    /// What was taken out, so the preview can show it rather than asking to be
    /// trusted.
    pub removed: Vec<RemovedParam>,
    pub providers: Vec<String>,
    /// The host legitimately changed because a wrapped destination was
    /// unwrapped.
    pub unwrapped_redirect: bool,
    /// The whole URL is a tracker; nothing was cleaned.
    pub complete_provider: bool,
}

#[derive(uniffi::Record, Clone)]
pub struct TextResult {
    pub cleaned_text: String,
    pub urls: Vec<Sanitized>,
    pub changed: bool,
}

fn map_sanitized(value: core::Sanitized) -> Sanitized {
    Sanitized {
        original: value.original,
        cleaned: value.cleaned,
        changed: value.changed,
        removed: value
            .removed
            .into_iter()
            .map(|r| RemovedParam {
                key: r.key,
                value: r.value,
                kind: match r.kind {
                    core::RemovalKind::Tracking => RemovalKind::Tracking,
                    core::RemovalKind::Referral => RemovalKind::Referral,
                    core::RemovalKind::RawRule => RemovalKind::Rewrite,
                },
                provider: r.provider,
            })
            .collect(),
        providers: value.providers,
        unwrapped_redirect: value.unwrapped_redirect,
        complete_provider: value.complete_provider,
    }
}

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum ShareWhereError {
    #[error("not a valid URL: {message}")]
    InvalidUrl { message: String },
    #[error("unsupported scheme: {scheme}")]
    UnsupportedScheme { scheme: String },
    #[error("input is {length} bytes, maximum is {maximum}")]
    InputTooLong { length: u32, maximum: u32 },
    #[error("the session is not waiting for a fetch response")]
    NotAwaitingFetch,
}

impl From<core::UrlError> for ShareWhereError {
    fn from(value: core::UrlError) -> Self {
        match value {
            core::UrlError::InvalidUrl(message) => ShareWhereError::InvalidUrl { message },
            core::UrlError::UnsupportedScheme(scheme) => {
                ShareWhereError::UnsupportedScheme { scheme }
            }
            core::UrlError::InputTooLong { len, max } => ShareWhereError::InputTooLong {
                length: len as u32,
                maximum: max as u32,
            },
        }
    }
}

/// Clean a single URL.
#[uniffi::export]
pub fn sanitize_url(url: String, options: Options) -> Result<Sanitized, ShareWhereError> {
    let options: core::Options = options.into();
    core::sanitize_url(&url, &options.sanitize)
        .map(map_sanitized)
        .map_err(Into::into)
}

/// Clean every URL inside shared text, leaving the prose alone.
///
/// This is the one the share sheet should call: `EXTRA_TEXT` is usually prose
/// wrapped around a link, not a bare URL.
#[uniffi::export]
pub fn sanitize_text(text: String, options: Options) -> Result<TextResult, ShareWhereError> {
    let options: core::Options = options.into();
    core::sanitize_text(&text, &options.sanitize)
        .map(|result| TextResult {
            cleaned_text: result.cleaned_text,
            urls: result.urls.into_iter().map(map_sanitized).collect(),
            changed: result.changed,
        })
        .map_err(Into::into)
}

// -------------------------------------------------------------- locations ---

#[derive(uniffi::Record, Clone)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
    pub label: Option<String>,
    pub zoom: Option<f64>,
    pub accuracy_m: Option<f64>,
}

impl From<core::GeoPoint> for GeoPoint {
    fn from(value: core::GeoPoint) -> Self {
        Self {
            lat: value.lat,
            lon: value.lon,
            label: value.label,
            zoom: value.zoom,
            accuracy_m: value.accuracy_m,
        }
    }
}

impl From<GeoPoint> for core::GeoPoint {
    fn from(value: GeoPoint) -> Self {
        core::GeoPoint {
            lat: value.lat,
            lon: value.lon,
            label: value.label,
            zoom: value.zoom,
            accuracy_m: value.accuracy_m,
        }
    }
}

#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq)]
pub enum LinkId {
    Geo,
    GoogleMaps,
    OrganicMaps,
    OrganicMapsShort,
    OrganicMapsScheme,
    AppleMaps,
    OpenStreetMap,
    PlusCode,
    PlusCodeUrl,
    DecimalDegrees,
    Dms,
}

#[derive(uniffi::Record, Clone)]
pub struct GeoLink {
    pub id: LinkId,
    pub label: String,
    pub value: String,
}

/// Every link format for a point. Entirely offline.
#[uniffi::export]
pub fn render_links(point: GeoPoint, options: Options) -> Vec<GeoLink> {
    let options: core::Options = options.into();
    let point: core::GeoPoint = point.into();
    core::render_links(&point, &options.render())
        .into_iter()
        .map(|link| GeoLink {
            id: match link.id {
                core::LinkId::Geo => LinkId::Geo,
                core::LinkId::GoogleMaps => LinkId::GoogleMaps,
                core::LinkId::OrganicMaps => LinkId::OrganicMaps,
                core::LinkId::OrganicMapsShort => LinkId::OrganicMapsShort,
                core::LinkId::OrganicMapsScheme => LinkId::OrganicMapsScheme,
                core::LinkId::AppleMaps => LinkId::AppleMaps,
                core::LinkId::OpenStreetMap => LinkId::OpenStreetMap,
                core::LinkId::PlusCode => LinkId::PlusCode,
                core::LinkId::PlusCodeUrl => LinkId::PlusCodeUrl,
                core::LinkId::DecimalDegrees => LinkId::DecimalDegrees,
                core::LinkId::Dms => LinkId::Dms,
            },
            label: link.label.to_string(),
            value: link.value,
        })
        .collect()
}

/// Every format as one block of text, for "copy all".
#[uniffi::export]
pub fn render_all_text(point: GeoPoint, options: Options) -> String {
    let options: core::Options = options.into();
    let point: core::GeoPoint = point.into();
    core::render_all_text(&point, &options.render())
}

// ---------------------------------------------------------------- resolve ---

#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Head,
    Get,
}

#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq)]
pub enum FetchReason {
    ShortLinkExpansion,
    HtmlCoordinateExtraction,
}

/// A request the core wants made, with the policy it must be made under.
///
/// The caller is expected to enforce every field, especially `allowed_hosts` —
/// re-check it after any redirect. The core checks too; this is deliberately
/// belt and braces, because the redirect target is attacker-controlled.
#[derive(uniffi::Record, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub method: HttpMethod,
    pub allowed_hosts: Vec<String>,
    pub follow_redirects: bool,
    pub max_body_bytes: u32,
    pub timeout_ms: u32,
    pub send_cookies: bool,
    pub user_agent: String,
    pub reason: FetchReason,
    /// Wording to show the user before making this request.
    pub explanation: String,
}

#[derive(uniffi::Record, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub final_url: String,
    pub location_header: Option<String>,
    pub body: Option<String>,
}

#[derive(uniffi::Enum, Clone)]
pub enum Outcome {
    Text {
        cleaned: String,
        urls: Vec<Sanitized>,
        changed: bool,
    },
    Location {
        point: GeoPoint,
        source: String,
    },
    /// A network request is needed and has not been agreed to. The default.
    NeedsConsent {
        url: String,
        host: String,
        explanation: String,
    },
    Unsupported {
        detail: String,
        explanation: String,
    },
    Nothing,
}

#[derive(uniffi::Enum, Clone)]
pub enum Step {
    Done { outcome: Outcome },
    Fetch { request: FetchRequest },
}

/// A resumable resolve. See `sharewhere-core` for the state machine.
#[derive(uniffi::Object)]
pub struct ResolveSession {
    inner: core::ResolveSession,
}

#[uniffi::export]
impl ResolveSession {
    #[uniffi::constructor]
    pub fn new(input: String, options: Options) -> Arc<Self> {
        Arc::new(Self {
            inner: core::ResolveSession::new(input, options.into()),
        })
    }

    /// Advance one step. Pure: it never performs I/O itself.
    pub fn advance(&self) -> Step {
        match self.inner.advance() {
            core::Step::Done(outcome) => Step::Done {
                outcome: map_outcome(outcome),
            },
            core::Step::Fetch(request) => Step::Fetch {
                request: FetchRequest {
                    url: request.url,
                    method: match request.method {
                        core::HttpMethod::Head => HttpMethod::Head,
                        core::HttpMethod::Get => HttpMethod::Get,
                    },
                    allowed_hosts: request.allowed_hosts,
                    follow_redirects: request.follow_redirects,
                    max_body_bytes: request.max_body_bytes,
                    timeout_ms: request.timeout_ms,
                    send_cookies: request.send_cookies,
                    user_agent: request.user_agent,
                    reason: match request.reason {
                        core::FetchReason::ShortLinkExpansion => FetchReason::ShortLinkExpansion,
                        core::FetchReason::HtmlCoordinateExtraction => {
                            FetchReason::HtmlCoordinateExtraction
                        }
                    },
                    explanation: request.reason.explanation().to_string(),
                },
            },
        }
    }

    /// Hand back the result of the request `advance` asked for.
    pub fn supply(&self, response: FetchResponse) -> Result<(), ShareWhereError> {
        self.inner
            .supply(core::FetchResponse {
                status: response.status,
                final_url: response.final_url,
                location_header: response.location_header,
                body: response.body,
            })
            .map_err(|_| ShareWhereError::NotAwaitingFetch)
    }
}

fn map_outcome(outcome: core::Outcome) -> Outcome {
    match outcome {
        core::Outcome::Text {
            cleaned,
            urls,
            changed,
        } => Outcome::Text {
            cleaned,
            urls: urls.into_iter().map(map_sanitized).collect(),
            changed,
        },
        core::Outcome::Location { point, source } => Outcome::Location {
            point: point.into(),
            source: format!("{source:?}"),
        },
        core::Outcome::NeedsConsent { url, host, reason } => Outcome::NeedsConsent {
            url,
            host,
            explanation: reason.explanation().to_string(),
        },
        core::Outcome::Unsupported {
            detail,
            explanation,
        } => Outcome::Unsupported {
            detail,
            explanation,
        },
        core::Outcome::Nothing => Outcome::Nothing,
    }
}

/// Compile the always-on rules ahead of the first share. Call from app start.
#[uniffi::export]
pub fn warm_up() {
    core::warm_up();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_options_are_the_privacy_defaults() {
        let options = default_options();
        assert!(!options.allow_network, "network must be opt-in");
        assert!(options.remove_referral);
        assert!(matches!(options.precision, Precision::Exact));
    }

    #[test]
    fn sanitizing_crosses_the_boundary_intact() {
        let result = sanitize_url(
            "https://example.com/p?utm_source=news&id=7".into(),
            default_options(),
        )
        .expect("should clean");
        assert_eq!(result.cleaned, "https://example.com/p?id=7");
        assert_eq!(result.removed.len(), 1);
        assert!(matches!(result.removed[0].kind, RemovalKind::Tracking));
    }

    #[test]
    fn a_session_reports_needing_consent_across_the_boundary() {
        let session =
            ResolveSession::new("https://maps.app.goo.gl/AbCdEf".into(), default_options());
        match session.advance() {
            Step::Done {
                outcome: Outcome::NeedsConsent { host, .. },
            } => assert_eq!(host, "maps.app.goo.gl"),
            _ => panic!("expected consent to be requested"),
        }
    }

    #[test]
    fn every_link_format_survives_the_boundary() {
        let point = GeoPoint {
            lat: 51.5007292,
            lon: -0.1246254,
            label: Some("Big Ben".into()),
            zoom: None,
            accuracy_m: None,
        };
        let links = render_links(point, default_options());
        assert_eq!(links.len(), 11);
        assert!(links.iter().any(|l| l.id == LinkId::PlusCode));
        assert!(links
            .iter()
            .any(|l| l.value.starts_with("https://omaps.app/")));
    }
}
