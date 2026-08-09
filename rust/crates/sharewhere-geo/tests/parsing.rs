//! Parsing real links, in the shapes the apps themselves produce.
//!
//! Every string here is the kind of thing that actually lands in
//! `Intent.EXTRA_TEXT` — usually with prose wrapped around it, because that is
//! what a share sheet sends.

use sharewhere_geo::{
    parse_location, render_links, GeoPoint, LinkId, LocationParse, LocationSource, NetworkNeed,
    RenderOptions, Unsupported,
};

/// Assert a parse produced a point near the expected coordinates.
#[track_caller]
fn expect_point(input: &str, lat: f64, lon: f64, source: LocationSource) -> GeoPoint {
    match parse_location(input) {
        LocationParse::Point { point, source: got } => {
            assert_eq!(got, source, "wrong source for {input}");
            assert!(
                (point.lat - lat).abs() < 0.001 && (point.lon - lon).abs() < 0.001,
                "{input}\n  parsed as {},{}\n  expected {lat},{lon}",
                point.lat,
                point.lon
            );
            point
        }
        other => panic!("{input} parsed as {other:?}, expected a point"),
    }
}

#[test]
fn geo_uris() {
    expect_point(
        "geo:51.5007,-0.1246",
        51.5007,
        -0.1246,
        LocationSource::GeoUri,
    );
    // The conventional Android form: `0,0` is a placeholder and the real
    // location lives in `q`. Taking the `0,0` literally would drop the pin in
    // the Atlantic.
    expect_point(
        "geo:0,0?q=51.5007,-0.1246(Big%20Ben)",
        51.5007,
        -0.1246,
        LocationSource::GeoUri,
    );

    let point = expect_point(
        "geo:51.5007,-0.1246?z=17&q=51.5007,-0.1246(Big%20Ben)",
        51.5007,
        -0.1246,
        LocationSource::GeoUri,
    );
    assert_eq!(point.label.as_deref(), Some("Big Ben"));
    assert_eq!(point.zoom, Some(17.0));

    let point = expect_point(
        "geo:51.5007,-0.1246;u=35",
        51.5007,
        -0.1246,
        LocationSource::GeoUri,
    );
    assert_eq!(point.accuracy_m, Some(35.0));
}

#[test]
fn google_maps_links() {
    // The `!3d/!4d` pair is the place; `@` is only where the camera was. When
    // both are present the place must win.
    let point = expect_point(
        "https://www.google.com/maps/place/Big+Ben/@51.4,-0.2,17z/data=!3m1!4b1!4m6!3m5!1s0x487604c38c8cd1d9:0xb78f2474b9a45aa9!8m2!3d51.5007292!4d-0.1246254",
        51.5007292,
        -0.1246254,
        LocationSource::GoogleMaps,
    );
    assert_eq!(point.label.as_deref(), Some("Big Ben"));

    expect_point(
        "https://www.google.com/maps/@51.5007,-0.1246,15z",
        51.5007,
        -0.1246,
        LocationSource::GoogleMaps,
    );
    expect_point(
        "https://www.google.com/maps/search/?api=1&query=51.5007,-0.1246",
        51.5007,
        -0.1246,
        LocationSource::GoogleMaps,
    );
}

/// What Google Maps actually puts on the clipboard. It carries no coordinates
/// at all, so the only honest answer is to ask before contacting anyone.
#[test]
fn google_short_links_ask_before_resolving() {
    match parse_location("https://maps.app.goo.gl/AbCdEfGhIjK") {
        LocationParse::NeedsNetwork { host, need, .. } => {
            assert_eq!(host, "maps.app.goo.gl");
            assert_eq!(need, NetworkNeed::ShortLink);
        }
        other => panic!("expected NeedsNetwork, got {other:?}"),
    }
}

/// A shared *place*, as opposed to a shared pin, names the place by Google's
/// own id and states no coordinate anywhere. It used to come back
/// `NotALocation`, which gave up on one of the commonest links there is.
#[test]
fn google_place_ids_offer_to_resolve_rather_than_giving_up() {
    for link in [
        // Feature id in a `data=` blob — what sharing a place produces.
        "https://www.google.com/maps/place/Bar+Raval/data=!4m2!3m1!\
         1s0x41652398b4d869f7:0x97d977a1ec83f74e!18m1!1e1",
        // Place id as a query parameter.
        "https://maps.google.com/?cid=97d977a1ec83f74e",
        // A named place with neither, which still needs Google to place it.
        "https://www.google.com/maps/place/Big+Ben",
    ] {
        match parse_location(link) {
            LocationParse::NeedsNetwork { host, need, url } => {
                assert_eq!(need, NetworkNeed::PlaceId, "{link}");
                // The full host, not the `www.`-stripped form: it is what the
                // consent dialog names and what the fetch is allow-listed to.
                assert!(host.ends_with("google.com"), "{link} -> {host}");
                assert_eq!(url, link);
            }
            other => panic!("expected NeedsNetwork for {link}, got {other:?}"),
        }
    }
}

/// The offline path must not regress: anything carrying a coordinate is still
/// answered without asking for the network.
#[test]
fn google_links_with_coordinates_stay_offline() {
    expect_point(
        "https://www.google.com/maps/place/Big+Ben/@51.5007,-0.1246,17z\
         /data=!3m1!4b1!4m5!3d51.5007!4d-0.1246",
        51.5007,
        -0.1246,
        LocationSource::GoogleMaps,
    );
    expect_point(
        "https://www.google.com/maps/@51.5007,-0.1246,17z",
        51.5007,
        -0.1246,
        LocationSource::GoogleMaps,
    );
}

#[test]
fn organic_maps_links() {
    // The name is percent-encoded, as the app emits it — a raw space would end
    // the token, since no URL parser accepts one.
    let point = expect_point(
        "om://map?v=1&ll=51.5007,-0.1246&n=Big%20Ben",
        51.5007,
        -0.1246,
        LocationSource::OrganicMaps,
    );
    assert_eq!(point.label.as_deref(), Some("Big Ben"));

    expect_point(
        "https://omaps.app/map?v=1&ll=51.5007,-0.1246",
        51.5007,
        -0.1246,
        LocationSource::OrganicMaps,
    );

    // The newer human-readable form.
    let point = expect_point(
        "https://omaps.app/51.5007,-0.1246/Big_Ben",
        51.5007,
        -0.1246,
        LocationSource::OrganicMaps,
    );
    assert_eq!(point.label.as_deref(), Some("Big Ben"));
}

#[test]
fn organic_maps_ge0_short_links() {
    // Generated by our own encoder, which is verified against Organic Maps'
    // reference vectors in ge0.rs.
    let payload = sharewhere_geo::ge0::encode(51.5007, -0.1246, 17.0, 9);
    let link = format!("https://omaps.app/{payload}/Big_Ben");
    let point = expect_point(&link, 51.5007, -0.1246, LocationSource::OrganicMaps);
    assert_eq!(point.label.as_deref(), Some("Big Ben"));
}

#[test]
fn apple_and_openstreetmap_links() {
    expect_point(
        "https://maps.apple.com/?ll=51.5007,-0.1246&q=Big%20Ben",
        51.5007,
        -0.1246,
        LocationSource::AppleMaps,
    );
    expect_point(
        "https://www.openstreetmap.org/?mlat=51.5007&mlon=-0.1246#map=17/51.5007/-0.1246",
        51.5007,
        -0.1246,
        LocationSource::OpenStreetMap,
    );
    expect_point(
        "https://www.openstreetmap.org/#map=17/51.5007/-0.1246",
        51.5007,
        -0.1246,
        LocationSource::OpenStreetMap,
    );
}

#[test]
fn plain_text_forms() {
    expect_point(
        "51.5007, -0.1246",
        51.5007,
        -0.1246,
        LocationSource::PlainCoordinates,
    );
    expect_point(
        "51°30'2.6\"N 0°7'28.7\"W",
        51.5007,
        -0.1246,
        LocationSource::PlainCoordinates,
    );
    // A full Plus Code. ~14 m of granularity, hence the looser tolerance.
    let point = expect_point("9C3XGV2G+75", 51.5007, -0.1246, LocationSource::PlusCode);
    assert!(
        (point.lat - 51.5007).abs() < 0.0002 && (point.lon - -0.1246).abs() < 0.0002,
        "plus code decoded to {},{}",
        point.lat,
        point.lon
    );
}

#[test]
fn links_embedded_in_prose_still_parse() {
    expect_point(
        "Meet me here: https://www.google.com/maps/search/?api=1&query=51.5007,-0.1246 at seven",
        51.5007,
        -0.1246,
        LocationSource::GoogleMaps,
    );
}

/// what3words needs their API, a key, and handing over the coordinates, an IP
/// address and a timestamp. ShareWhere does not do that — but saying so beats
/// failing silently.
#[test]
fn what3words_is_recognised_and_explained_rather_than_converted() {
    match parse_location("///filled.count.soap") {
        LocationParse::Unsupported(Unsupported::What3Words(words)) => {
            assert_eq!(words, "filled.count.soap");
        }
        other => panic!("expected an unsupported what3words result, got {other:?}"),
    }
}

#[test]
fn non_locations_are_rejected() {
    for input in [
        "",
        "just some text",
        "https://example.com/article",
        "1000,2000",           // out of range
        "version 1.2.3 notes", // three dotted words, but no /// marker
    ] {
        assert!(
            matches!(parse_location(input), LocationParse::NotALocation),
            "{input:?} should not have parsed as a location"
        );
    }
}

/// Everything we emit, we must be able to read back. Otherwise sharing from
/// ShareWhere into ShareWhere silently loses the location.
#[test]
fn every_emitted_link_round_trips_through_the_parser() {
    let point = GeoPoint::new(51.5007292, -0.1246254).with_label(Some("Big Ben".into()));
    let links = render_links(&point, &RenderOptions::default());

    for link in links {
        // plus.codes is a web page rather than a coordinate-bearing link, and
        // the two plain-text renderings are covered by the plain-text tests.
        if matches!(link.id, LinkId::PlusCodeUrl | LinkId::Dms) {
            continue;
        }
        match parse_location(&link.value) {
            LocationParse::Point { point: parsed, .. } => {
                assert!(
                    (parsed.lat - point.lat).abs() < 0.001
                        && (parsed.lon - point.lon).abs() < 0.001,
                    "{:?} round-tripped to {},{}: {}",
                    link.id,
                    parsed.lat,
                    parsed.lon,
                    link.value
                );
            }
            other => panic!(
                "{:?} ({}) did not round trip: {other:?}",
                link.id, link.value
            ),
        }
    }
}
