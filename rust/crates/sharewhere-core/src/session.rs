//! The resolve state machine.
//!
//! ShareWhere's core never opens a socket. When something genuinely cannot be
//! answered offline — `maps.app.goo.gl` carries no coordinates, and that is
//! precisely what Google Maps puts on the clipboard — the core says *what* to
//! fetch and *how to interpret the answer*, and the platform layer moves the
//! bytes.
//!
//! This is a resumable session rather than a single plan/apply pair because
//! short links cascade: `maps.app.goo.gl` → `consent.google.com` →
//! `google.com/maps/…`, and the last hop sometimes hides the coordinates in
//! the page body rather than the URL. Each [`ResolveSession::advance`] call is
//! a pure state transition that either finishes or asks for one more fetch.
//!
//! ```text
//!   new(input, opts)
//!        │
//!        ▼
//!   advance() ──► Done(outcome)          nothing else needed
//!        │
//!        └─────► Fetch(request) ──► caller performs it ──► supply(response)
//!                     ▲                                          │
//!                     └──────────────── advance() ◄──────────────┘
//! ```

use std::sync::Mutex;

use sharewhere_geo::{
    extract_location_from_html, parse_location, GeoPoint, LocationParse, LocationSource,
};
use sharewhere_url::{sanitize_text, Sanitized};

use crate::options::Options;

/// Why the core wants to make a request. Shown to the user verbatim, so it has
/// to be something a person can actually act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchReason {
    /// Follow a short link far enough to learn where it points.
    ShortLinkExpansion,
    /// The final page's URL had no coordinates; look in the body.
    HtmlCoordinateExtraction,
}

impl FetchReason {
    pub fn explanation(self) -> &'static str {
        match self {
            FetchReason::ShortLinkExpansion => {
                "This link is a shortener and hides where it points. \
                 Following it contacts the host below."
            }
            FetchReason::HtmlCoordinateExtraction => {
                "The link resolved but carries no coordinates. \
                 Reading the page contacts the host below."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Head,
    Get,
}

/// A request the core wants made, carrying its own policy.
///
/// The policy travels with the request rather than living in the platform layer
/// so there is exactly one place to audit it. The caller is expected to enforce
/// every field — belt and braces, because the redirect target is
/// attacker-controlled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: String,
    pub method: HttpMethod,
    /// The caller MUST reject a URL whose host is not in this list, both before
    /// the request and after any redirect.
    pub allowed_hosts: Vec<String>,
    /// Always false: the core walks the hops itself so it can count them and
    /// re-check the host at every step.
    pub follow_redirects: bool,
    pub max_body_bytes: u32,
    pub timeout_ms: u32,
    /// Always false. A cookie jar would turn link-cleaning into tracked
    /// browsing, which is the opposite of the point.
    pub send_cookies: bool,
    /// Fixed and boring, so the request does not become a fingerprint.
    pub user_agent: String,
    pub reason: FetchReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub final_url: String,
    pub location_header: Option<String>,
    pub body: Option<String>,
}

/// What the caller should do next.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Done(Outcome),
    Fetch(FetchRequest),
}

/// The end state of a resolve.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Shared text with its URLs cleaned.
    Text {
        cleaned: String,
        urls: Vec<Sanitized>,
        changed: bool,
    },
    /// A location we can offer in every format.
    Location {
        point: GeoPoint,
        source: LocationSource,
    },
    /// A location that needs one network request, and the user has not agreed
    /// to it. This is the default path: nothing leaves the device until they
    /// tap the button this drives.
    NeedsConsent {
        url: String,
        host: String,
        reason: FetchReason,
    },
    /// Understood, but not something we will convert.
    Unsupported { detail: String, explanation: String },
    /// Nothing usable in the input.
    Nothing,
}

const MAX_BODY_BYTES: u32 = 64 * 1024;
const TIMEOUT_MS: u32 = 8_000;
/// Deliberately generic. A distinctive agent string is a fingerprint.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; ShareWhere)";

/// Hosts a short-link expansion is allowed to touch, including the consent and
/// regional hosts Google bounces through.
fn allowed_hosts_for(host: &str) -> Vec<String> {
    let mut hosts = vec![host.to_string()];
    if host.ends_with("goo.gl") || host == "g.co" {
        hosts.extend(
            [
                "maps.app.goo.gl",
                "goo.gl",
                "g.co",
                "www.google.com",
                "maps.google.com",
                "consent.google.com",
                "google.com",
            ]
            .iter()
            .map(|h| h.to_string()),
        );
    }
    hosts.sort();
    hosts.dedup();
    hosts
}

#[derive(Debug)]
enum Phase {
    Start,
    /// Waiting for the caller to supply a response.
    AwaitingFetch {
        request: FetchRequest,
    },
    /// A response arrived and is ready to be interpreted.
    Supplied {
        request: FetchRequest,
        response: FetchResponse,
    },
    Finished(Outcome),
}

struct State {
    input: String,
    hops: u8,
    visited: Vec<String>,
    phase: Phase,
}

/// A resumable resolve. Cheap to create; holds no resources.
pub struct ResolveSession {
    options: Options,
    state: Mutex<State>,
}

impl ResolveSession {
    pub fn new(input: impl Into<String>, options: Options) -> Self {
        Self {
            options,
            state: Mutex::new(State {
                input: input.into(),
                hops: 0,
                visited: Vec::new(),
                phase: Phase::Start,
            }),
        }
    }

    /// Advance one pure step.
    pub fn advance(&self) -> Step {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        // Terminal phases are idempotent: asking again returns the same answer
        // rather than starting a second resolve.
        let step = match std::mem::replace(&mut state.phase, Phase::Start) {
            Phase::Finished(outcome) => Step::Done(outcome),
            Phase::AwaitingFetch { request } => Step::Fetch(request),
            Phase::Start => {
                let input = state.input.clone();
                self.begin(&input, &mut state)
            }
            Phase::Supplied { request, response } => {
                self.interpret(&request, &response, &mut state)
            }
        };

        state.phase = match &step {
            Step::Done(outcome) => Phase::Finished(outcome.clone()),
            Step::Fetch(request) => Phase::AwaitingFetch {
                request: request.clone(),
            },
        };
        step
    }

    /// Hand back the result of the request [`advance`] asked for.
    pub fn supply(&self, response: FetchResponse) -> Result<(), SessionError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match std::mem::replace(&mut state.phase, Phase::Start) {
            Phase::AwaitingFetch { request } => {
                state.phase = Phase::Supplied { request, response };
                Ok(())
            }
            other => {
                state.phase = other;
                Err(SessionError::NotAwaitingFetch)
            }
        }
    }

    /// First step: try to answer entirely offline.
    fn begin(&self, input: &str, state: &mut State) -> Step {
        match parse_location(input) {
            LocationParse::Point { point, source } => {
                Step::Done(Outcome::Location { point, source })
            }

            LocationParse::NeedsNetwork { url, host } => {
                if !self.options.allow_network {
                    // The default path. Nothing has left the device.
                    return Step::Done(Outcome::NeedsConsent {
                        url,
                        host,
                        reason: FetchReason::ShortLinkExpansion,
                    });
                }
                state.visited.push(host.clone());
                Step::Fetch(FetchRequest {
                    allowed_hosts: allowed_hosts_for(&host),
                    url,
                    method: HttpMethod::Head,
                    follow_redirects: false,
                    max_body_bytes: MAX_BODY_BYTES,
                    timeout_ms: TIMEOUT_MS,
                    send_cookies: false,
                    user_agent: USER_AGENT.to_string(),
                    reason: FetchReason::ShortLinkExpansion,
                })
            }

            LocationParse::Unsupported(unsupported) => {
                let sharewhere_geo::Unsupported::What3Words(words) = unsupported;
                Step::Done(Outcome::Unsupported {
                    detail: words,
                    explanation: "what3words addresses can only be converted by \
                                  what3words' own service, which would mean sending \
                                  them the location, your IP address and a timestamp. \
                                  ShareWhere does not do that."
                        .to_string(),
                })
            }

            // Not a location, so treat it as text with links in it.
            LocationParse::NotALocation => match sanitize_text(input, &self.options.sanitize) {
                Ok(result) if result.urls.is_empty() && !result.changed => {
                    Step::Done(Outcome::Nothing)
                }
                Ok(result) => Step::Done(Outcome::Text {
                    cleaned: result.cleaned_text,
                    urls: result.urls,
                    changed: result.changed,
                }),
                Err(_) => Step::Done(Outcome::Nothing),
            },
        }
    }

    /// Interpret a response and decide whether another hop is needed.
    fn interpret(
        &self,
        request: &FetchRequest,
        response: &FetchResponse,
        state: &mut State,
    ) -> Step {
        state.hops += 1;

        // Follow one redirect hop at a time so the host is re-checked each time
        // and the hop count cannot be bypassed.
        if (300..400).contains(&response.status) {
            if let Some(location) = &response.location_header {
                let next = resolve_relative(&response.final_url, location);
                if let Some(next) = next {
                    if state.hops < self.options.sanitize.max_hops {
                        if let Some(host) = host_of(&next) {
                            if !state.visited.contains(&host) {
                                state.visited.push(host.clone());
                                let mut allowed = request.allowed_hosts.clone();
                                if !allowed.contains(&host) {
                                    allowed.push(host);
                                }
                                return Step::Fetch(FetchRequest {
                                    url: next,
                                    allowed_hosts: allowed,
                                    ..request.clone()
                                });
                            }
                        }
                    }
                }
            }
        }

        // Try the URL we landed on.
        if let LocationParse::Point { point, source } = parse_location(&response.final_url) {
            return Step::Done(Outcome::Location { point, source });
        }

        // Then the body, if we asked for one.
        if let Some(body) = &response.body {
            if let Some(point) = extract_location_from_html(body) {
                return Step::Done(Outcome::Location {
                    point,
                    source: LocationSource::GoogleMaps,
                });
            }
        }

        // Escalate from HEAD to GET once, to look inside the page.
        if request.method == HttpMethod::Head && state.hops < self.options.sanitize.max_hops {
            if let Some(host) = host_of(&response.final_url) {
                let mut allowed = request.allowed_hosts.clone();
                if !allowed.contains(&host) {
                    allowed.push(host);
                }
                return Step::Fetch(FetchRequest {
                    url: response.final_url.clone(),
                    method: HttpMethod::Get,
                    allowed_hosts: allowed,
                    reason: FetchReason::HtmlCoordinateExtraction,
                    ..request.clone()
                });
            }
        }

        // No location anywhere, but the URL we ended on is still worth cleaning.
        match sanitize_text(&response.final_url, &self.options.sanitize) {
            Ok(result) if !result.urls.is_empty() => Step::Done(Outcome::Text {
                cleaned: result.cleaned_text,
                urls: result.urls,
                changed: result.changed,
            }),
            _ => Step::Done(Outcome::Nothing),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    /// `supply` was called when the session was not waiting for a response.
    NotAwaitingFetch,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::NotAwaitingFetch => {
                write!(f, "the session is not waiting for a fetch response")
            }
        }
    }
}

impl std::error::Error for SessionError {}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(|h| h.to_ascii_lowercase())
}

/// Resolve a `Location` header, which may be relative, against the URL it came
/// from — and refuse anything that is not https.
///
/// SECURITY: the header is chosen by whoever controls the redirect. Allowing
/// `file:`, `data:` or `intent:` through here would let a hostile short link
/// steer the platform layer somewhere it should never go.
fn resolve_relative(base: &str, location: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    let resolved = base.join(location).ok()?;
    (resolved.scheme() == "https").then(|| resolved.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(allow_network: bool) -> Options {
        Options {
            allow_network,
            ..Options::default()
        }
    }

    #[test]
    fn an_offline_location_never_asks_for_the_network() {
        let session = ResolveSession::new("geo:51.5007,-0.1246", options(false));
        match session.advance() {
            Step::Done(Outcome::Location { point, .. }) => {
                assert!((point.lat - 51.5007).abs() < 0.001);
            }
            other => panic!("expected a location, got {other:?}"),
        }
    }

    #[test]
    fn a_short_link_asks_for_consent_by_default() {
        let session = ResolveSession::new("https://maps.app.goo.gl/AbCdEf", options(false));
        match session.advance() {
            Step::Done(Outcome::NeedsConsent { host, reason, .. }) => {
                assert_eq!(host, "maps.app.goo.gl");
                assert_eq!(reason, FetchReason::ShortLinkExpansion);
            }
            other => panic!("expected consent to be requested, got {other:?}"),
        }
    }

    #[test]
    fn with_consent_it_requests_a_locked_down_fetch() {
        let session = ResolveSession::new("https://maps.app.goo.gl/AbCdEf", options(true));
        let Step::Fetch(request) = session.advance() else {
            panic!("expected a fetch");
        };

        assert_eq!(request.method, HttpMethod::Head);
        assert!(!request.follow_redirects, "the core walks hops itself");
        assert!(!request.send_cookies, "a cookie jar would defeat the point");
        assert!(request
            .allowed_hosts
            .contains(&"maps.app.goo.gl".to_string()));
        assert!(request.max_body_bytes <= 64 * 1024);

        // Asking again must be idempotent, not start a second resolve.
        assert_eq!(session.advance(), Step::Fetch(request));
    }

    #[test]
    fn a_redirect_chain_resolves_to_a_location() {
        let session = ResolveSession::new("https://maps.app.goo.gl/AbCdEf", options(true));
        let Step::Fetch(_) = session.advance() else {
            panic!("expected a fetch");
        };

        session
            .supply(FetchResponse {
                status: 302,
                final_url: "https://maps.app.goo.gl/AbCdEf".into(),
                location_header: Some(
                    "https://www.google.com/maps/place/Big+Ben/@51.5,-0.12,17z/data=!3m1!4b1!4m6!3m5!8m2!3d51.5007292!4d-0.1246254".into(),
                ),
                body: None,
            })
            .unwrap();

        match session.advance() {
            Step::Fetch(request) => {
                assert!(request.url.contains("google.com/maps"));
                session
                    .supply(FetchResponse {
                        status: 200,
                        final_url: request.url.clone(),
                        location_header: None,
                        body: None,
                    })
                    .unwrap();
            }
            other => panic!("expected a second fetch, got {other:?}"),
        }

        match session.advance() {
            Step::Done(Outcome::Location { point, .. }) => {
                assert!((point.lat - 51.5007292).abs() < 0.0001);
                assert!((point.lon - -0.1246254).abs() < 0.0001);
            }
            other => panic!("expected a location, got {other:?}"),
        }
    }

    #[test]
    fn a_non_https_redirect_target_is_refused() {
        assert_eq!(
            resolve_relative("https://maps.app.goo.gl/x", "intent://scan/#Intent;end"),
            None
        );
        assert_eq!(
            resolve_relative("https://maps.app.goo.gl/x", "http://example.com/"),
            None,
            "downgrade to http must not be followed"
        );
        assert!(resolve_relative("https://maps.app.goo.gl/x", "/maps/place/Foo").is_some());
    }

    #[test]
    fn plain_text_falls_through_to_url_cleaning() {
        let session = ResolveSession::new(
            "Look at this https://example.com/a?utm_source=news",
            options(false),
        );
        match session.advance() {
            Step::Done(Outcome::Text {
                cleaned, changed, ..
            }) => {
                assert!(changed);
                assert_eq!(cleaned, "Look at this https://example.com/a");
            }
            other => panic!("expected cleaned text, got {other:?}"),
        }
    }

    #[test]
    fn what3words_is_explained_rather_than_converted() {
        let session = ResolveSession::new("///filled.count.soap", options(true));
        match session.advance() {
            Step::Done(Outcome::Unsupported {
                detail,
                explanation,
            }) => {
                assert_eq!(detail, "filled.count.soap");
                assert!(explanation.contains("what3words"));
            }
            other => panic!("expected an explanation, got {other:?}"),
        }
    }

    #[test]
    fn supplying_a_response_when_none_was_asked_for_is_an_error() {
        let session = ResolveSession::new("geo:51.5,-0.1", options(false));
        let error = session.supply(FetchResponse {
            status: 200,
            final_url: "https://example.com".into(),
            location_header: None,
            body: None,
        });
        assert_eq!(error, Err(SessionError::NotAwaitingFetch));
    }

    #[test]
    fn hops_are_capped() {
        let options = Options {
            allow_network: true,
            ..Options::default()
        };
        let max = options.sanitize.max_hops;
        let session = ResolveSession::new("https://maps.app.goo.gl/AbCdEf", options);

        // Bounce it around a loop of distinct hosts and check it stops.
        let mut fetches = 0;
        while let Step::Fetch(request) = session.advance() {
            fetches += 1;
            assert!(fetches <= max as usize + 1, "hop cap was not enforced");
            session
                .supply(FetchResponse {
                    status: 302,
                    final_url: request.url.clone(),
                    location_header: Some(format!("https://hop{fetches}.example.com/")),
                    body: None,
                })
                .unwrap();
        }
        assert!(
            fetches > 0,
            "the loop should have made at least one request"
        );
    }
}
