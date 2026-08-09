//! Fuzzes location parsing and the link emitters together.
//!
//! Parsing alone is not the interesting part. What matters is the pair: a
//! parser that returns a plausible-looking `GeoPoint` from nonsense, which the
//! emitters then turn into eleven links pointing at the wrong place, is a
//! silent failure no crash-only fuzzer catches. So this target parses, and then
//! renders whatever came out.

#![no_main]

use libfuzzer_sys::fuzz_target;
use sharewhere_geo::{
    parse_location, render_links, GeoPoint, LocationParse, Precision, RenderOptions,
};

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    let LocationParse::Point { point, .. } = parse_location(input) else {
        // NeedsNetwork / Unsupported / NotALocation are all correct outcomes.
        return;
    };

    // A point that escapes the parser must be on the globe. Everything
    // downstream — the OLC encoder's integer arithmetic, the ge0 bit packing —
    // assumes this, and both would produce a confident, wrong answer rather
    // than fail if it did not hold.
    assert!(
        point.lat.is_finite() && (-90.0..=90.0).contains(&point.lat),
        "parsed out-of-range latitude {} from {input:?}",
        point.lat,
    );
    assert!(
        point.lon.is_finite() && (-180.0..=180.0).contains(&point.lon),
        "parsed out-of-range longitude {} from {input:?}",
        point.lon,
    );

    for precision in [Precision::Exact, Precision::Approximate, Precision::Coarse] {
        for include_label in [true, false] {
            let opts = RenderOptions {
                precision,
                include_label,
            };
            let links = render_links(&point, &opts);
            assert!(!links.is_empty(), "no links rendered for {input:?}");

            for link in &links {
                assert!(
                    !link.value.is_empty(),
                    "empty {:?} link for {input:?}",
                    link.id,
                );
            }

            // The privacy control has to actually work: "strip place name" must
            // produce exactly what sharing an unnamed point would, in every
            // format. Stated as an equality rather than "the label does not
            // appear in the output" on purpose — substring search gives false
            // positives whenever the label happens to look like a coordinate or
            // like the base-64ish `ge0` payload, which the fuzzer finds in
            // seconds.
            if !include_label {
                let anonymous = GeoPoint {
                    label: None,
                    ..point.clone()
                };
                assert_eq!(
                    links,
                    render_links(&anonymous, &opts),
                    "include_label = false did not match sharing an unnamed point, for {input:?}",
                );
            }
        }
    }
});
