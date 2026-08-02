//! Generating a location link for every map app, entirely offline.
//!
//! Nothing here contacts anything. Every format below is constructible from the
//! coordinates alone, which is the whole reason the location feature can work
//! with the network switched off.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::ge0;
use crate::olc;
use crate::point::{GeoPoint, Precision};

/// Characters escaped in a place name. Deliberately broad — these strings end
/// up inside query parameters and path segments of several different apps'
/// URLs, and over-escaping is harmless where under-escaping breaks the link.
const NAME_ESCAPES: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'#')
    .add(b'%')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^')
    .add(b'~')
    .add(b'[')
    .add(b']')
    .add(b'`')
    .add(b'?')
    .add(b'&')
    .add(b'=')
    .add(b'+')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'@')
    .add(b'$')
    .add(b',')
    .add(b'(')
    .add(b')');

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkId {
    /// RFC 5870. The interoperable one: any installed maps app can open it.
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

impl LinkId {
    pub fn label(self) -> &'static str {
        match self {
            LinkId::Geo => "geo: link (any maps app)",
            LinkId::GoogleMaps => "Google Maps",
            LinkId::OrganicMaps => "Organic Maps",
            LinkId::OrganicMapsShort => "Organic Maps (short)",
            LinkId::OrganicMapsScheme => "Organic Maps (app link)",
            LinkId::AppleMaps => "Apple Maps",
            LinkId::OpenStreetMap => "OpenStreetMap",
            LinkId::PlusCode => "Plus Code",
            LinkId::PlusCodeUrl => "Plus Code link",
            LinkId::DecimalDegrees => "Coordinates",
            LinkId::Dms => "Coordinates (DMS)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoLink {
    pub id: LinkId,
    pub label: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    pub precision: Precision,
    /// Include the place name. Off means share the coordinate without saying
    /// that it is "Home".
    pub include_label: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            precision: Precision::default(),
            include_label: true,
        }
    }
}

/// Default zoom when the source link did not carry one. 17 is roughly
/// street-level, which is what someone sharing a place usually means.
const DEFAULT_ZOOM: f64 = 17.0;

/// Build every link format for a point.
pub fn render_links(point: &GeoPoint, opts: &RenderOptions) -> Vec<GeoLink> {
    let point = point.at_precision(opts.precision, opts.include_label);
    let (lat, lon) = point.format(opts.precision);
    let label = point.label.as_deref().filter(|l| !l.trim().is_empty());
    let zoom = point.zoom.unwrap_or(DEFAULT_ZOOM);

    let encoded_name = label.map(|l| utf8_percent_encode(l, NAME_ESCAPES).to_string());
    let ge0_payload = ge0::encode(point.lat, point.lon, zoom, ge0::DEFAULT_POINT_BYTES);
    let plus_code = olc::encode(point.lat, point.lon, olc::DEFAULT_CODE_LENGTH);

    let mut links = Vec::with_capacity(11);
    let mut push = |id: LinkId, value: String| {
        links.push(GeoLink {
            id,
            label: id.label(),
            value,
        });
    };

    // geo: first — it is the one that works with whatever the recipient has
    // installed, rather than assuming they use a particular app.
    let mut geo = format!("geo:{lat},{lon}?z={zoom:.0}&q={lat},{lon}");
    if let Some(name) = &encoded_name {
        geo.push_str(&format!("({name})"));
    }
    push(LinkId::Geo, geo);

    // Google's documented Maps URL API: stable, and carries no tracking.
    push(
        LinkId::GoogleMaps,
        format!("https://www.google.com/maps/search/?api=1&query={lat},{lon}"),
    );

    // Organic Maps reads spaces in a place name as underscores.
    let om_name =
        label.map(|l| utf8_percent_encode(&l.replace(' ', "_"), NAME_ESCAPES).to_string());
    push(
        LinkId::OrganicMaps,
        match &om_name {
            Some(name) => format!("https://omaps.app/{lat},{lon}/{name}"),
            None => format!("https://omaps.app/{lat},{lon}"),
        },
    );
    push(
        LinkId::OrganicMapsShort,
        match &om_name {
            Some(name) => format!("https://omaps.app/{ge0_payload}/{name}"),
            None => format!("https://omaps.app/{ge0_payload}"),
        },
    );
    push(
        LinkId::OrganicMapsScheme,
        match &om_name {
            Some(name) => format!("om://{ge0_payload}/{name}"),
            None => format!("om://{ge0_payload}"),
        },
    );

    push(
        LinkId::AppleMaps,
        match &encoded_name {
            Some(name) => format!("https://maps.apple.com/?ll={lat},{lon}&q={name}"),
            None => format!("https://maps.apple.com/?ll={lat},{lon}&q={lat},{lon}"),
        },
    );

    push(
        LinkId::OpenStreetMap,
        format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map={zoom:.0}/{lat}/{lon}"),
    );

    push(LinkId::PlusCode, plus_code.clone());
    push(
        LinkId::PlusCodeUrl,
        format!("https://plus.codes/{plus_code}"),
    );

    push(LinkId::DecimalDegrees, format!("{lat}, {lon}"));
    push(LinkId::Dms, format_dms(point.lat, point.lon));

    links
}

/// A single block of text with every format, for "copy all".
pub fn render_all_text(point: &GeoPoint, opts: &RenderOptions) -> String {
    let links = render_links(point, opts);
    let mut out = String::new();

    if opts.include_label {
        if let Some(label) = point.label.as_deref().filter(|l| !l.trim().is_empty()) {
            out.push_str(label);
            out.push('\n');
        }
    }
    for link in links {
        out.push_str(&format!("{}: {}\n", link.label, link.value));
    }
    out
}

/// Degrees/minutes/seconds, the form people read aloud.
pub fn format_dms(lat: f64, lon: f64) -> String {
    fn component(value: f64, positive: char, negative: char) -> String {
        let hemisphere = if value >= 0.0 { positive } else { negative };
        let value = value.abs();
        let degrees = value.trunc();
        let minutes_full = (value - degrees) * 60.0;
        let minutes = minutes_full.trunc();
        let seconds = (minutes_full - minutes) * 60.0;
        format!("{degrees:.0}°{minutes:.0}'{seconds:.1}\"{hemisphere}")
    }
    format!("{} {}", component(lat, 'N', 'S'), component(lon, 'E', 'W'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big_ben() -> GeoPoint {
        GeoPoint::new(51.5007292, -0.1246254).with_label(Some("Big Ben".into()))
    }

    fn value(links: &[GeoLink], id: LinkId) -> &str {
        &links
            .iter()
            .find(|l| l.id == id)
            .expect("link present")
            .value
    }

    #[test]
    fn emits_every_format() {
        let links = render_links(&big_ben(), &RenderOptions::default());
        assert_eq!(links.len(), 11);

        assert_eq!(
            value(&links, LinkId::GoogleMaps),
            "https://www.google.com/maps/search/?api=1&query=51.500729,-0.124625"
        );
        assert_eq!(
            value(&links, LinkId::OrganicMaps),
            "https://omaps.app/51.500729,-0.124625/Big_Ben"
        );
        assert!(value(&links, LinkId::Geo).starts_with("geo:51.500729,-0.124625"));
        assert!(value(&links, LinkId::Geo).contains("(Big%20Ben)"));
        assert!(value(&links, LinkId::OrganicMapsScheme).starts_with("om://"));
    }

    #[test]
    fn dropping_the_label_leaves_no_trace_of_it() {
        let opts = RenderOptions {
            include_label: false,
            ..Default::default()
        };
        let links = render_links(&big_ben(), &opts);
        for link in &links {
            assert!(
                !link.value.contains("Big") && !link.value.contains("Ben"),
                "label leaked into {:?}: {}",
                link.id,
                link.value
            );
        }
    }

    #[test]
    fn blurring_reduces_precision_in_every_link() {
        let opts = RenderOptions {
            precision: Precision::Coarse,
            include_label: false,
        };
        let links = render_links(&big_ben(), &opts);
        assert_eq!(
            value(&links, LinkId::GoogleMaps),
            "https://www.google.com/maps/search/?api=1&query=51.5,-0.12"
        );
    }

    #[test]
    fn dms_matches_the_conventional_rendering() {
        assert_eq!(
            format_dms(51.5007292, -0.1246254),
            "51°30'2.6\"N 0°7'28.7\"W"
        );
    }

    #[test]
    fn google_maps_output_is_parseable_by_our_own_parser() {
        // Whatever we emit, we must be able to read back. Otherwise sharing
        // from ShareWhere into ShareWhere silently loses the location.
        let links = render_links(&big_ben(), &RenderOptions::default());
        for link in links {
            if !link.value.starts_with("http") && !link.value.starts_with("geo:") {
                continue;
            }
            if link.id == LinkId::PlusCodeUrl {
                continue; // plus.codes is a web page, not a coordinate link
            }
            let parsed = crate::parse::parse_location(&link.value);
            assert!(
                matches!(parsed, crate::parse::LocationParse::Point { .. }),
                "could not re-parse our own {:?}: {} -> {parsed:?}",
                link.id,
                link.value
            );
        }
    }
}
