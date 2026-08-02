//! The `ge0` short-link codec used by Organic Maps.
//!
//! Original implementation written from the format description in
//! `organicmaps/libs/ge0` (Apache-2.0). No upstream code was copied; the
//! reference unit-test vectors are reproduced below so this is verified against
//! the real encoder rather than only round-tripped against itself.
//!
//! Layout of `om://8wAAAAAAAA/Name`:
//!
//! ```text
//! om://8wAAAAAAAA/Name
//!      ^          ^
//!      |          `-- optional place name, spaces written as underscores
//!      |
//!      +-- 1 zoom byte, then 9 bytes of interleaved lat/lon bits
//! ```
//!
//! Each coordinate is mapped to a 30-bit integer; the two are then interleaved
//! three bits at a time and emitted six bits per character.

/// Base64-like alphabet, but in a different order to standard base64 — letters
/// first, then digits, then `-` and `_`.
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

const MAX_POINT_BYTES: usize = 10;
const MAX_COORD_BITS: u32 = (MAX_POINT_BYTES as u32) * 3;

/// Bytes Organic Maps itself emits: 1 zoom + 9 coordinate.
pub const DEFAULT_POINT_BYTES: usize = 9;

fn coord_max() -> i64 {
    (1i64 << MAX_COORD_BITS) - 1
}

fn base64_char(value: usize) -> char {
    ALPHABET[value] as char
}

fn base64_value(c: char) -> Option<i64> {
    ALPHABET
        .iter()
        .position(|a| *a == c as u8)
        .map(|v| v as i64)
}

/// Map latitude `[-90, 90]` onto `[0, max]`, clamping out-of-range input.
fn lat_to_int(lat: f64, max: i64) -> i64 {
    let x = (lat + 90.0) / 180.0 * max as f64;
    if x < 0.0 {
        0
    } else if x > max as f64 {
        max
    } else {
        x.round() as i64
    }
}

/// Fold an arbitrary longitude into `[-180, 180)`.
fn lon_in_180_180(lon: f64) -> f64 {
    if lon >= 0.0 {
        (lon + 180.0) % 360.0 - 180.0
    } else {
        let l = (lon - 180.0) % 360.0 + 180.0;
        if l < 180.0 {
            l
        } else {
            l - 360.0
        }
    }
}

/// Map longitude `[-180, 180)` onto `[0, max]`.
fn lon_to_int(lon: f64, max: i64) -> i64 {
    let x = (lon_in_180_180(lon) + 180.0) / 360.0 * (max as f64 + 1.0) + 0.5;
    if x <= 0.0 || x >= max as f64 + 1.0 {
        0
    } else {
        x as i64
    }
}

fn encode_zoom(zoom: f64) -> usize {
    if zoom <= 4.0 {
        0
    } else if zoom >= 19.75 {
        63
    } else {
        ((zoom - 4.0) * 4.0) as usize
    }
}

fn decode_zoom(byte: i64) -> f64 {
    byte as f64 / 4.0 + 4.0
}

/// Encode a point as the `ge0` payload — the zoom byte plus `point_bytes` of
/// interleaved coordinate, without any `om://` or `https://omaps.app/` prefix.
pub fn encode(lat: f64, lon: f64, zoom: f64, point_bytes: usize) -> String {
    let point_bytes = point_bytes.min(MAX_POINT_BYTES);
    let max = coord_max();
    let lat_i = lat_to_int(lat, max);
    let lon_i = lon_to_int(lon, max);

    let mut out = String::with_capacity(point_bytes + 1);
    out.push(base64_char(encode_zoom(zoom)));

    for i in 0..point_bytes {
        let shift = MAX_COORD_BITS as i32 - 3 - (i as i32) * 3;
        let lat_bits = (lat_i >> shift) & 7;
        let lon_bits = (lon_i >> shift) & 7;

        let byte = ((lat_bits >> 2) & 1) << 5
            | ((lon_bits >> 2) & 1) << 4
            | ((lat_bits >> 1) & 1) << 3
            | ((lon_bits >> 1) & 1) << 2
            | (lat_bits & 1) << 1
            | (lon_bits & 1);
        out.push(base64_char(byte as usize));
    }
    out
}

/// Decode a `ge0` payload back to `(lat, lon, zoom)`.
///
/// Codes shorter than the full 10 coordinate bytes name a cell rather than a
/// point, so the centre of that cell is returned.
pub fn decode(payload: &str) -> Option<(f64, f64, f64)> {
    let mut chars = payload.chars();
    let zoom = decode_zoom(base64_value(chars.next()?)?);

    let mut lat_i: i64 = 0;
    let mut lon_i: i64 = 0;
    let mut bits = 0u32;

    for c in chars.take(MAX_POINT_BYTES) {
        let byte = base64_value(c)?;
        let lat_bits = ((byte >> 5) & 1) << 2 | ((byte >> 3) & 1) << 1 | ((byte >> 1) & 1);
        let lon_bits = ((byte >> 4) & 1) << 2 | ((byte >> 2) & 1) << 1 | (byte & 1);
        lat_i = (lat_i << 3) | lat_bits;
        lon_i = (lon_i << 3) | lon_bits;
        bits += 3;
    }
    if bits == 0 {
        return None;
    }

    // Shift the partial value up to full width and land in the middle of the
    // cell the code actually names.
    let remaining = MAX_COORD_BITS - bits;
    lat_i <<= remaining;
    lon_i <<= remaining;
    if remaining > 0 {
        let half = 1i64 << (remaining - 1);
        lat_i += half;
        lon_i += half;
    }

    let max = coord_max();
    let lat = lat_i as f64 / max as f64 * 180.0 - 90.0;
    let lon = lon_i as f64 / (max as f64 + 1.0) * 360.0 - 180.0;
    Some((lat, lon, zoom))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors lifted from Organic Maps' own `url_generator_tests.cpp`, so this
    /// is checked against the real encoder rather than against itself.
    #[test]
    fn matches_organic_maps_reference_vectors() {
        assert_eq!(encode(0.0, 0.0, 19.0, 9), "8wAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, 2.0, 9), "AwAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, -5.0, 9), "AwAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, 20.0, 9), "_wAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, 2_000_000_000.0, 9), "_wAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, 8.25, 9), "RwAAAAAAAA");
        assert_eq!(encode(0.0, 0.0, 8.499, 9), "RwAAAAAAAA");
    }

    #[test]
    fn latitude_mapping_matches_reference() {
        assert_eq!(lat_to_int(0.0, 998), 499);
        assert_eq!(lat_to_int(0.0, 999), 500);
        assert_eq!(lat_to_int(0.0, 1000), 500);
        assert_eq!(lat_to_int(0.0, 1001), 501);
        assert_eq!(lat_to_int(90.0, 1000), 1000);
        assert_eq!(lat_to_int(-90.0, 1000), 0);
        // Out of range input clamps rather than wrapping.
        assert_eq!(lat_to_int(370.0, 1000), 1000);
        assert_eq!(lat_to_int(-370.0, 1000), 0);
    }

    #[test]
    fn round_trips_to_within_a_few_metres() {
        for (lat, lon) in [
            (51.5007292, -0.1246254),
            (-33.8688, 151.2093),
            (55.7558, 37.6173),
            (0.0, 0.0),
        ] {
            let payload = encode(lat, lon, 17.0, DEFAULT_POINT_BYTES);
            let (dlat, dlon, zoom) = decode(&payload).expect("must decode");
            assert!(
                (dlat - lat).abs() < 0.0001 && (dlon - lon).abs() < 0.0001,
                "{payload} -> {dlat},{dlon}, expected {lat},{lon}"
            );
            assert!((zoom - 17.0).abs() < 0.3, "zoom {zoom}");
        }
    }

    #[test]
    fn rejects_characters_outside_the_alphabet() {
        assert!(decode("8w!AAAAAAA").is_none());
        assert!(decode("").is_none());
    }
}
