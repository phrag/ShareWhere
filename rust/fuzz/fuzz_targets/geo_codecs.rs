//! Round-trips the two codecs we implement ourselves.
//!
//! Open Location Code and ge0 are the parts of `sharewhere-geo` with no
//! upstream crate behind them — they are our arithmetic, and a bug in either
//! produces a link that opens somewhere plausible but wrong. The reference
//! vectors pin down the values upstream publishes; this pins down everything
//! between them.
//!
//! Structured input rather than raw bytes: `decode(encode(x)) ≈ x` needs a
//! valid `x`, and finding one by mutating bytes would waste the whole run.
//! The second half of the target still feeds arbitrary text to the decoders,
//! which is the side an attacker actually controls.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sharewhere_geo::{ge0, olc};

#[derive(Arbitrary, Debug)]
struct Input {
    lat_millionths: i32,
    lon_millionths: i32,
    code_length: u8,
    zoom_tenths: u8,
    point_bytes: u8,
    /// Fed to the decoders as-is. This is the untrusted direction: a ge0 or
    /// Plus Code arriving in a shared link is whatever the sender wrote.
    text: String,
}

fuzz_target!(|input: Input| {
    // Map the integers onto the globe rather than rejecting most of the space.
    let lat = (input.lat_millionths % 90_000_001) as f64 / 1_000_000.0;
    let lon = (input.lon_millionths % 180_000_001) as f64 / 1_000_000.0;

    // --- Open Location Code -------------------------------------------------
    //
    // Lengths below 8 are *short* codes: the spec pads them with `0` to the
    // separator, and a padded code cannot be resolved without a reference
    // location, so `decode` rightly refuses them. Round-tripping therefore only
    // makes sense from 8 up — 8 and 10 in the pair stage, 11..=15 in the grid.
    let code_length = match input.code_length % 7 {
        0 => 8,
        n => 9 + n as usize, // 10..=15
    };
    let code = olc::encode(lat, lon, code_length);
    assert!(
        olc::is_full_code(&code),
        "encoded {lat},{lon} at length {code_length} into {code:?}, which is not a full code",
    );

    let (dec_lat, dec_lon) =
        olc::decode(&code).unwrap_or_else(|| panic!("our own code {code:?} failed to decode"));

    // A code names an area and `decode` returns its centre, so the point is
    // within half a cell. Cell size: the pair stage starts at 20° and divides
    // by 20 per pair; each grid digit then divides latitude by 5 and longitude
    // by 4.
    let pairs = code_length.min(10) / 2;
    let mut lat_size = 20.0 * 20.0_f64.powi(1 - pairs as i32);
    let mut lon_size = lat_size;
    for _ in 10..code_length {
        lat_size /= 5.0;
        lon_size /= 4.0;
    }
    assert!(
        (dec_lat - lat).abs() <= lat_size / 2.0 + 1e-9,
        "OLC moved latitude {lat} to {dec_lat} (code {code:?}, cell {lat_size})",
    );
    assert!(
        angular_delta(dec_lon, lon) <= lon_size / 2.0 + 1e-9,
        "OLC moved longitude {lon} to {dec_lon} (code {code:?}, cell {lon_size})",
    );

    // --- ge0 ----------------------------------------------------------------
    //
    // The Organic Maps short-link codec. 1..=10 payload bytes; zoom is clamped
    // by the encoder to the range it can represent.
    let point_bytes = 1 + (input.point_bytes as usize % 10);
    let zoom = input.zoom_tenths as f64 / 10.0;
    let payload = ge0::encode(lat, lon, zoom, point_bytes);
    assert_eq!(
        payload.chars().count(),
        point_bytes + 1,
        "ge0 payload {payload:?} is the wrong length for {point_bytes} point bytes",
    );

    let (ge0_lat, ge0_lon, _) = ge0::decode(&payload)
        .unwrap_or_else(|| panic!("our own ge0 payload {payload:?} failed to decode"));

    // Each payload character carries 6 bits, split 3 lat / 3 lon, so precision
    // halves three times per character. Start from the full 180°/360° range.
    let bits_per_axis = point_bytes * 3;
    let lat_tolerance = 180.0 / (1u64 << bits_per_axis.min(30)) as f64 + 1e-6;
    let lon_tolerance = 360.0 / (1u64 << bits_per_axis.min(30)) as f64 + 1e-6;
    assert!(
        (ge0_lat - lat).abs() <= lat_tolerance,
        "ge0 moved latitude {lat} to {ge0_lat} (payload {payload:?}, tolerance {lat_tolerance})",
    );
    assert!(
        angular_delta(ge0_lon, lon) <= lon_tolerance,
        "ge0 moved longitude {lon} to {ge0_lon} (payload {payload:?}, tolerance {lon_tolerance})",
    );

    // --- the untrusted direction -------------------------------------------
    //
    // Anything the decoders accept must still be a point on the globe; anything
    // else must be `None`. Neither may panic.
    if let Some((lat, lon)) = olc::decode(&input.text) {
        assert!(
            (-90.0..=90.0).contains(&lat),
            "olc::decode produced lat {lat}"
        );
        assert!(
            (-180.0..=180.0).contains(&lon),
            "olc::decode produced lon {lon}"
        );
    }
    if let Some((lat, lon, zoom)) = ge0::decode(&input.text) {
        assert!(
            (-90.0..=90.0).contains(&lat),
            "ge0::decode produced lat {lat}"
        );
        assert!(
            (-180.0..=180.0).contains(&lon),
            "ge0::decode produced lon {lon}"
        );
        assert!(zoom.is_finite(), "ge0::decode produced zoom {zoom}");
    }
});

/// Longitude difference, taking the short way round so 179.9 and -179.9 are
/// close rather than 359.8 apart.
fn angular_delta(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs();
    raw.min(360.0 - raw)
}
