//! Recognising a location in whatever a maps app put on the clipboard.
//!
//! Everything here is offline and pure. One case genuinely cannot be:
//! `maps.app.goo.gl` short links carry no coordinates at all, and that is
//! exactly what Google Maps produces when you tap Share. Those return
//! [`LocationParse::NeedsNetwork`] so the caller can ask the user before
//! contacting anyone.

use std::sync::OnceLock;

use percent_encoding::percent_decode_str;
use regex::Regex;

use crate::olc;
use crate::point::GeoPoint;

/// Where a location came from. Shown in the UI so the user can see we
/// understood their link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationSource {
    GeoUri,
    GoogleMaps,
    OrganicMaps,
    AppleMaps,
    OpenStreetMap,
    PlusCode,
    PlainCoordinates,
}

/// Why a location could not be resolved offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// A what3words address. Converting one requires their API — a key, plus
    /// handing them the coordinates, an IP address and a timestamp. ShareWhere
    /// does not do that, so we say so rather than failing silently.
    What3Words(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocationParse {
    Point {
        point: GeoPoint,
        source: LocationSource,
    },
    /// A short link that has to be followed to learn anything.
    NeedsNetwork {
        url: String,
        host: String,
    },
    Unsupported(Unsupported),
    NotALocation,
}

fn re(cache: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cache.get_or_init(|| Regex::new(pattern).expect("static pattern must compile"))
}

macro_rules! pattern {
    ($name:ident, $pattern:literal) => {
        fn $name() -> &'static Regex {
            static CACHE: OnceLock<Regex> = OnceLock::new();
            re(&CACHE, $pattern)
        }
    };
}

// `!3d<lat>!4d<lon>` inside a Google Maps `data=` blob. This is the place
// itself, as opposed to `@` which is only where the camera happened to be, so
// it is preferred when both are present.
pattern!(google_data, r"!3d(-?\d+\.?\d*)!4d(-?\d+\.?\d*)");
pattern!(google_at, r"@(-?\d+\.?\d*),(-?\d+\.?\d*)(?:,(\d+\.?\d*)z)?");
pattern!(google_place, r"/maps/place/([^/@?]+)");
pattern!(
    plain_pair,
    r"^\s*(-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)\s*$"
);
pattern!(
    dms,
    // NOTE: `x` mode strips whitespace inside character classes too, so the
    // separator has to be written `[,\s]` rather than `[, ]` — the latter
    // silently becomes "comma only" and stops matching `51°N 0°W`.
    r#"(?ix)
    (\d{1,3})\s*[°d]\s*(\d{1,2})?\s*['′m]?\s*([\d.]+)?\s*["″s]?\s*([NS])
    \s*[,\s]\s*
    (\d{1,3})\s*[°d]\s*(\d{1,2})?\s*['′m]?\s*([\d.]+)?\s*["″s]?\s*([EW])
    "#
);
pattern!(
    what3words,
    r"(?:^|/|\s)/{0,3}([a-z]{3,}\.[a-z]{3,}\.[a-z]{3,})\s*$"
);

/// Hosts whose links are short and opaque, so resolving them needs one request.
const SHORT_LINK_HOSTS: &[&str] = &[
    "maps.app.goo.gl",
    "goo.gl",
    "g.co",
    "bit.ly",
    "tinyurl.com",
    "t.co",
];

/// Try to find a location in arbitrary shared text.
pub fn parse_location(input: &str) -> LocationParse {
    let text = input.trim();
    if text.is_empty() {
        return LocationParse::NotALocation;
    }

    // Scheme-carrying forms first, since they are unambiguous.
    if let Some(token) = first_token_with_scheme(text) {
        if let Some(parsed) = parse_uri(&token) {
            return parsed;
        }
    }

    // Then bare formats that can appear as plain text.
    if let Some(point) = parse_plain_coordinates(text) {
        return LocationParse::Point {
            point,
            source: LocationSource::PlainCoordinates,
        };
    }
    if let Some(point) = parse_dms(text) {
        return LocationParse::Point {
            point,
            source: LocationSource::PlainCoordinates,
        };
    }
    if let Some(point) = parse_plus_code(text) {
        return LocationParse::Point {
            point,
            source: LocationSource::PlusCode,
        };
    }
    if let Some(words) = parse_what3words(text) {
        return LocationParse::Unsupported(Unsupported::What3Words(words));
    }

    LocationParse::NotALocation
}

/// Dig a location out of an HTML page body.
///
/// The last resort when a short link resolves to a page whose URL still carries
/// no coordinates. Kept here, in Rust, so the caller only ever moves bytes and
/// never parses anything — the body is attacker-influenced content.
///
/// Deliberately pattern-matching rather than real HTML parsing: we are looking
/// for a coordinate pair in a `data=` blob, a canonical link or an `og:image`
/// URL, all of which are plain text. A DOM parser would be a large dependency
/// and a much larger attack surface for no gain.
pub fn extract_location_from_html(body: &str) -> Option<GeoPoint> {
    // Cap the scan: a hostile page could otherwise make us walk megabytes.
    let body = &body[..body.len().min(256 * 1024)];

    if let Some(captures) = google_data().captures(body) {
        let point = GeoPoint::new(
            captures.get(1)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
        );
        if point.is_valid() {
            return Some(point);
        }
    }
    if let Some(captures) = google_at().captures(body) {
        let point = GeoPoint::new(
            captures.get(1)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
        )
        .with_zoom(captures.get(3).and_then(|z| z.as_str().parse().ok()));
        if point.is_valid() {
            return Some(point);
        }
    }
    None
}

/// Pull the first `geo:`, `om:` or `http(s):` token out of free text.
fn first_token_with_scheme(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("geo:")
            || lower.starts_with("om://")
            || lower.starts_with("http://")
            || lower.starts_with("https://")
        {
            return Some(
                token
                    .trim_end_matches(['.', ',', ';', ')', ']', '!'])
                    .to_string(),
            );
        }
    }
    None
}

fn parse_uri(token: &str) -> Option<LocationParse> {
    let lower = token.to_ascii_lowercase();

    if lower.starts_with("geo:") {
        return parse_geo_uri(token).map(|point| LocationParse::Point {
            point,
            source: LocationSource::GeoUri,
        });
    }
    if lower.starts_with("om://") {
        return parse_organic_maps(token).map(|point| LocationParse::Point {
            point,
            source: LocationSource::OrganicMaps,
        });
    }

    let url = url::Url::parse(token).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();

    if SHORT_LINK_HOSTS.contains(&host.as_str()) {
        return Some(LocationParse::NeedsNetwork {
            url: token.to_string(),
            host,
        });
    }
    if host == "omaps.app" || host == "ge0.me" {
        return parse_organic_maps(token).map(|point| LocationParse::Point {
            point,
            source: LocationSource::OrganicMaps,
        });
    }
    if host.contains("google.") && token.contains("/maps") || host == "maps.google.com" {
        return parse_google_maps(token).map(|point| LocationParse::Point {
            point,
            source: LocationSource::GoogleMaps,
        });
    }
    if host == "maps.apple.com" {
        return parse_apple_maps(&url).map(|point| LocationParse::Point {
            point,
            source: LocationSource::AppleMaps,
        });
    }
    if host == "openstreetmap.org" {
        return parse_osm(&url).map(|point| LocationParse::Point {
            point,
            source: LocationSource::OpenStreetMap,
        });
    }
    if host == "what3words.com" || host == "w3w.co" {
        let words = url.path().trim_matches('/').to_string();
        if !words.is_empty() {
            return Some(LocationParse::Unsupported(Unsupported::What3Words(words)));
        }
    }
    None
}

/// RFC 5870 `geo:` URIs, including the `?q=` and `?z=` extensions Android and
/// Organic Maps both use.
fn parse_geo_uri(token: &str) -> Option<GeoPoint> {
    let rest = &token[4..];
    let (coords_part, query) = match rest.split_once('?') {
        Some((c, q)) => (c, Some(q)),
        None => (rest, None),
    };

    // `;u=` carries an accuracy in metres, and other `;` params may follow.
    let mut segments = coords_part.split(';');
    let coords = segments.next()?;
    let mut accuracy = None;
    for segment in segments {
        if let Some(value) = segment.strip_prefix("u=") {
            accuracy = value.parse::<f64>().ok();
        }
    }

    let mut point = parse_pair(coords);
    let mut label = None;
    let mut zoom = None;

    if let Some(query) = query {
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                // `q=lat,lon(Label)` — Google's addition, and what Organic Maps
                // emits so the pin is actually dropped rather than just
                // centring the map.
                //
                // `q` wins over the base coordinates: the conventional Android
                // form is `geo:0,0?q=<real location>`, where the `0,0` is a
                // placeholder and taking it literally would drop the pin in the
                // Atlantic.
                "q" => {
                    let decoded = decode(value);
                    let (coords, name) = split_label(&decoded);
                    label = name;
                    if let Some(from_query) = parse_pair(&coords) {
                        point = Some(from_query);
                    }
                }
                "z" => zoom = value.parse().ok(),
                _ => {}
            }
        }
    }

    let mut point = point?;
    point.label = label;
    point.zoom = zoom;
    point.accuracy_m = accuracy;
    point.is_valid().then_some(point)
}

fn parse_google_maps(token: &str) -> Option<GeoPoint> {
    let label = google_place()
        .captures(token)
        .and_then(|c| c.get(1))
        .map(|m| decode(m.as_str()).replace('+', " "));

    // Preference order matters: `!3d/!4d` is the place, `@` is only the camera.
    let from_data = google_data().captures(token).and_then(|c| {
        Some(GeoPoint::new(
            c.get(1)?.as_str().parse().ok()?,
            c.get(2)?.as_str().parse().ok()?,
        ))
    });

    let from_at = google_at().captures(token).and_then(|c| {
        let zoom = c.get(3).and_then(|z| z.as_str().parse::<f64>().ok());
        Some(
            GeoPoint::new(
                c.get(1)?.as_str().parse().ok()?,
                c.get(2)?.as_str().parse().ok()?,
            )
            .with_zoom(zoom),
        )
    });

    let from_query = url::Url::parse(token).ok().and_then(|url| {
        for (key, value) in url.query_pairs() {
            if matches!(key.as_ref(), "q" | "query" | "ll" | "daddr" | "center") {
                let (coords, _) = split_label(&value);
                if let Some(point) = parse_pair(&coords) {
                    return Some(point);
                }
            }
        }
        None
    });

    let zoom = from_at.as_ref().and_then(|p| p.zoom);
    let mut point = from_data.or(from_query).or(from_at)?;
    point.label = label;
    point.zoom = point.zoom.or(zoom);
    point.is_valid().then_some(point)
}

/// Every Organic Maps form: `om://map?ll=`, `omaps.app/map?ll=`,
/// `omaps.app/LAT,LON/Name`, and the compact ge0 payload.
fn parse_organic_maps(token: &str) -> Option<GeoPoint> {
    if let Ok(url) = url::Url::parse(token) {
        // `?v=1&ll=LAT,LON&n=NAME`
        let mut coords = None;
        let mut label = None;
        let mut zoom = None;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "ll" => coords = parse_pair(&value),
                "n" | "name" => label = Some(value.to_string()),
                "z" | "zoom" => zoom = value.parse().ok(),
                _ => {}
            }
        }
        if let Some(mut point) = coords {
            point.label = label;
            point.zoom = zoom;
            return point.is_valid().then_some(point);
        }
    }

    // Path forms. `om://PAYLOAD/Name` has no host — the payload is the first
    // path segment — whereas `https://omaps.app/PAYLOAD/Name` does, so only the
    // https form gets a host stripped off the front.
    let after_scheme = token
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(token);
    let path = if token.to_ascii_lowercase().starts_with("om://") {
        after_scheme
    } else {
        after_scheme.split_once('/').map(|(_, p)| p).unwrap_or("")
    };
    let path = path.split(['?', '#']).next().unwrap_or("");

    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let first = segments.next()?;
    let label = segments
        .next()
        .map(|name| decode(name).replace('_', " "))
        .filter(|l| !l.is_empty());

    // `omaps.app/51.5,-0.12/Name`
    if let Some(mut point) = parse_pair(&decode(first)) {
        point.label = label;
        return point.is_valid().then_some(point);
    }

    // Otherwise a ge0 payload.
    let (lat, lon, zoom) = crate::ge0::decode(first)?;
    let mut point = GeoPoint::new(lat, lon).with_zoom(Some(zoom));
    point.label = label;
    point.is_valid().then_some(point)
}

fn parse_apple_maps(url: &url::Url) -> Option<GeoPoint> {
    let mut coords = None;
    let mut label = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "ll" | "sll" | "daddr" => coords = coords.or_else(|| parse_pair(&value)),
            "q" => {
                if let Some(point) = parse_pair(&value) {
                    coords = coords.or(Some(point));
                } else {
                    label = Some(value.to_string());
                }
            }
            "address" | "name" => label = label.or_else(|| Some(value.to_string())),
            _ => {}
        }
    }
    let mut point = coords?;
    point.label = label;
    point.is_valid().then_some(point)
}

fn parse_osm(url: &url::Url) -> Option<GeoPoint> {
    let mut lat = None;
    let mut lon = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "mlat" => lat = value.parse::<f64>().ok(),
            "mlon" => lon = value.parse::<f64>().ok(),
            _ => {}
        }
    }

    // `#map=17/51.5007/-0.1246`
    let mut zoom = None;
    if let Some(fragment) = url.fragment() {
        if let Some(rest) = fragment.strip_prefix("map=") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 3 {
                zoom = parts[0].parse().ok();
                lat = lat.or_else(|| parts[1].parse().ok());
                lon = lon.or_else(|| parts[2].parse().ok());
            }
        }
    }

    let point = GeoPoint::new(lat?, lon?).with_zoom(zoom);
    point.is_valid().then_some(point)
}

fn parse_plain_coordinates(text: &str) -> Option<GeoPoint> {
    let captures = plain_pair().captures(text)?;
    let point = GeoPoint::new(
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
    );
    point.is_valid().then_some(point)
}

fn parse_dms(text: &str) -> Option<GeoPoint> {
    let captures = dms().captures(text)?;

    let component = |deg: usize, min: usize, sec: usize, hemi: usize| -> Option<f64> {
        let degrees: f64 = captures.get(deg)?.as_str().parse().ok()?;
        let minutes: f64 = captures
            .get(min)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0.0);
        let seconds: f64 = captures
            .get(sec)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0.0);
        let value = degrees + minutes / 60.0 + seconds / 3600.0;
        let hemisphere = captures.get(hemi)?.as_str().to_ascii_uppercase();
        Some(if hemisphere == "S" || hemisphere == "W" {
            -value
        } else {
            value
        })
    };

    let point = GeoPoint::new(component(1, 2, 3, 4)?, component(5, 6, 7, 8)?);
    point.is_valid().then_some(point)
}

fn parse_plus_code(text: &str) -> Option<GeoPoint> {
    let candidate = text.split_whitespace().find(|t| t.contains('+'))?;
    let candidate = candidate.trim_end_matches(['.', ',', ';']);
    let (lat, lon) = olc::decode(candidate)?;
    let point = GeoPoint::new(lat, lon);
    point.is_valid().then_some(point)
}

fn parse_what3words(text: &str) -> Option<String> {
    // Only treat this as an address when it is explicitly marked with the
    // `///` prefix; three dotted words are otherwise far too easy to hit by
    // accident on a filename or a domain.
    let trimmed = text.trim();
    if !trimmed.contains("///") {
        return None;
    }
    what3words()
        .captures(trimmed)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// `lat,lon` in whatever surrounding whitespace.
fn parse_pair(text: &str) -> Option<GeoPoint> {
    let captures = plain_pair().captures(text.trim())?;
    Some(GeoPoint::new(
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
    ))
}

/// Split `51.5,-0.12(Big Ben)` into coordinates and label.
fn split_label(value: &str) -> (String, Option<String>) {
    match value.split_once('(') {
        Some((coords, rest)) => (
            coords.trim().to_string(),
            Some(rest.trim_end_matches(')').trim().to_string()).filter(|l| !l.is_empty()),
        ),
        None => (value.trim().to_string(), None),
    }
}

fn decode(value: &str) -> String {
    percent_decode_str(value)
        .decode_utf8()
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| value.to_string())
}
