//! Checks the Plus Code implementation against the upstream reference vectors.
//!
//! `testdata/olc_encoding.csv` is taken verbatim from
//! google/open-location-code. Round-tripping our own encoder against our own
//! decoder would prove only that we are consistently wrong; this proves we
//! agree with the reference.
//!
//! The file carries both degrees and the integer grid coordinates, and the
//! split matters. Upstream verifies the codec against the integer columns
//! because the degrees-to-integer step is floating point, and the reference
//! implementations disagree with their own published codes on 17 of these 302
//! cases. `snapping_floor` in `olc.rs` closes that gap, so both halves are
//! tested here — and both are held to exact agreement.

use sharewhere_geo::olc;

struct Vector {
    lat: f64,
    lon: f64,
    lat_int: i64,
    lng_int: i64,
    code_length: usize,
    expected: String,
}

fn vectors() -> Vec<Vector> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/olc_encoding.csv"
    );
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read OLC vectors at {path}: {e}"));

    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 6 {
                return None;
            }
            Some(Vector {
                lat: fields[0].parse().ok()?,
                lon: fields[1].parse().ok()?,
                lat_int: fields[2].parse().ok()?,
                lng_int: fields[3].parse().ok()?,
                code_length: fields[4].parse().ok()?,
                expected: fields[5].to_string(),
            })
        })
        .collect()
}

/// The real test: the codec must reproduce every reference code exactly.
#[test]
fn the_codec_matches_every_reference_vector_exactly() {
    let vectors = vectors();
    assert!(
        vectors.len() > 200,
        "only loaded {} vectors, the file looks wrong",
        vectors.len()
    );

    let mut failures = Vec::new();
    for vector in &vectors {
        let actual = olc::encode_integers(vector.lat_int, vector.lng_int, vector.code_length);
        if actual != vector.expected {
            failures.push(format!(
                "encode_integers({}, {}, {}) = {actual}, reference says {}",
                vector.lat_int, vector.lng_int, vector.code_length, vector.expected
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} reference vectors failed:\n{}",
        failures.len(),
        vectors.len(),
        failures.join("\n")
    );
}

/// The degrees-to-integer conversion must reproduce the reference integers
/// exactly.
///
/// The reference implementations do *not* manage this — a plain `floor` of the
/// float product disagrees on 17 of these 302 cases, because round coordinates
/// land exactly on cell boundaries that binary floating point cannot represent.
/// `snapping_floor` in `olc.rs` fixes those, so we hold ourselves to exact
/// agreement rather than to the reference's own tolerance.
#[test]
fn degrees_convert_to_the_reference_integers_exactly() {
    let mut failures = Vec::new();
    for vector in vectors() {
        let (lat_int, lng_int) = olc::location_to_integers(vector.lat, vector.lon);
        if lat_int != vector.lat_int || lng_int != vector.lng_int {
            failures.push(format!(
                "({}, {}) -> ({lat_int}, {lng_int}), reference says ({}, {})",
                vector.lat, vector.lon, vector.lat_int, vector.lng_int
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} conversion(s) disagreed with the reference:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// End to end, from degrees, at every length in the reference set.
#[test]
fn encoding_from_degrees_matches_every_reference_vector() {
    let mut failures = Vec::new();
    for vector in vectors() {
        let actual = olc::encode(vector.lat, vector.lon, vector.code_length);
        if actual != vector.expected {
            failures.push(format!(
                "encode({}, {}, {}) = {actual}, reference says {}",
                vector.lat, vector.lon, vector.code_length, vector.expected
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} vector(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn decoding_every_reference_code_lands_inside_its_own_cell() {
    for vector in vectors() {
        if vector.expected.contains('0') {
            continue; // padded codes name an area, not a point
        }
        let Some((lat, lon)) = olc::decode(&vector.expected) else {
            panic!("reference code {} failed to decode", vector.expected);
        };
        // Re-encoding the decoded centre must give the same code back.
        let reencoded = olc::encode(lat, lon, vector.code_length);
        assert_eq!(
            reencoded, vector.expected,
            "decode/encode round trip moved {} to {lat},{lon}",
            vector.expected
        );
    }
}
