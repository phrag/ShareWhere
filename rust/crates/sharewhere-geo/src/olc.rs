//! Open Location Code (Plus Codes).
//!
//! An original implementation of the published specification, which is frozen.
//! The two crates on crates.io are stale (`open-location-code` 0.1.0 from 2018,
//! `pluscodes` 0.5.0 from 2022) and the algorithm is ~200 lines, so carrying it
//! ourselves costs less than depending on either.
//!
//! Correctness is verified against the upstream reference vectors in
//! `testdata/olc_encoding.csv` — see `tests/olc_vectors.rs`. That matters more
//! than usual here: this is a navigation format, and a subtly wrong code sends
//! someone to the wrong place.

/// The OLC digit alphabet. Deliberately excludes vowels and easily-confused
/// characters so codes cannot spell words and are hard to mis-transcribe.
const ALPHABET: &[u8] = b"23456789CFGHJMPQRVWX";

const SEPARATOR: char = '+';
const SEPARATOR_POSITION: usize = 8;
const PADDING: char = '0';

/// Digits before the grid refinement stage.
const PAIR_CODE_LENGTH: usize = 10;
/// Digits in a maximum-precision code.
const MAX_DIGIT_COUNT: usize = 15;

const GRID_ROWS: i64 = 5;
const GRID_COLUMNS: i64 = 4;

const LATITUDE_MAX: i64 = 90;
const LONGITUDE_MAX: i64 = 180;

/// Integer multipliers for a full-precision code: `8000 * 5^5` for latitude and
/// `8000 * 4^5` for longitude. Working in integers avoids the floating-point
/// drift that plagues naive implementations near cell boundaries.
const LAT_MULTIPLIER: i64 = 8000 * 3125;
const LNG_MULTIPLIER: i64 = 8000 * 1024;

/// The length of a code shown to a user, e.g. `8FVC9G8F+6W` (~14 m).
pub const DEFAULT_CODE_LENGTH: usize = 10;

/// Encode a coordinate as a Plus Code.
///
/// `code_length` is clamped to the valid range and, below
/// [`PAIR_CODE_LENGTH`], rounded down to an even number, since the pair stage
/// encodes latitude and longitude together.
pub fn encode(lat: f64, lon: f64, code_length: usize) -> String {
    let (lat_value, lng_value) = location_to_integers(lat, lon);
    encode_integers(lat_value, lng_value, code_length)
}

/// Convert degrees to the integer grid coordinates the codec works in.
///
/// `floor`, per the reference implementation — but see [`snapping_floor`] for
/// why a plain `floor` is not quite enough.
pub fn location_to_integers(lat: f64, lon: f64) -> (i64, i64) {
    let mut lat_value =
        snapping_floor(clip_latitude(lat) * LAT_MULTIPLIER as f64) + LATITUDE_MAX * LAT_MULTIPLIER;
    let lat_ceiling = 2 * LATITUDE_MAX * LAT_MULTIPLIER;
    // Latitude 90 would fall outside the last cell, so pull it back inside.
    lat_value = lat_value.clamp(0, lat_ceiling - 1);

    let lng_span = 2 * LONGITUDE_MAX * LNG_MULTIPLIER;
    let lng_value = (snapping_floor(normalise_longitude(lon) * LNG_MULTIPLIER as f64)
        + LONGITUDE_MAX * LNG_MULTIPLIER)
        .rem_euclid(lng_span);

    (lat_value, lng_value)
}

/// `floor`, except that a value sitting within a few ulps of an integer is
/// taken to be that integer.
///
/// Necessary because round coordinates land exactly on cell boundaries and
/// binary floating point cannot represent them: `129.7 * 8_192_000` evaluates
/// to `1062502399.9999999`, so a plain `floor` drops a whole cell and produces
/// a visibly wrong Plus Code. The reference implementations inherit the same
/// flaw — they disagree with their own published test vectors on 17 of 302
/// cases, which is why upstream verifies the codec against integer columns
/// rather than degrees.
///
/// A tolerance of a few ulps fixes every one of those without ever pulling a
/// genuinely mid-cell value across a boundary; the nearest counter-example in
/// the reference set sits half a cell away, six orders of magnitude clear.
fn snapping_floor(value: f64) -> i64 {
    let nearest = value.round();
    let tolerance = value.abs().max(1.0) * f64::EPSILON * 4.0;
    if (value - nearest).abs() < tolerance {
        nearest as i64
    } else {
        value.floor() as i64
    }
}

/// The codec proper, on integer grid coordinates.
///
/// Exposed so it can be tested directly against the reference vectors without
/// the float conversion in the way.
pub fn encode_integers(mut lat_value: i64, mut lng_value: i64, code_length: usize) -> String {
    let code_length = normalise_length(code_length);
    let mut digits = [b'0'; MAX_DIGIT_COUNT];

    if code_length > PAIR_CODE_LENGTH {
        // Grid stage: each digit refines latitude and longitude together.
        for i in (0..MAX_DIGIT_COUNT - PAIR_CODE_LENGTH).rev() {
            let lat_digit = lat_value % GRID_ROWS;
            let lng_digit = lng_value % GRID_COLUMNS;
            digits[PAIR_CODE_LENGTH + i] =
                ALPHABET[(lat_digit * GRID_COLUMNS + lng_digit) as usize];
            lat_value /= GRID_ROWS;
            lng_value /= GRID_COLUMNS;
        }
    } else {
        lat_value /= GRID_ROWS.pow(5);
        lng_value /= GRID_COLUMNS.pow(5);
    }

    // Pair stage, from the least significant digit back.
    for i in (0..PAIR_CODE_LENGTH / 2).rev() {
        digits[i * 2] = ALPHABET[(lat_value % 20) as usize];
        digits[i * 2 + 1] = ALPHABET[(lng_value % 20) as usize];
        lat_value /= 20;
        lng_value /= 20;
    }

    let digits = std::str::from_utf8(&digits).expect("alphabet is ASCII");

    let mut code = String::with_capacity(MAX_DIGIT_COUNT + 1);
    if code_length < SEPARATOR_POSITION {
        code.push_str(&digits[..code_length]);
        for _ in code_length..SEPARATOR_POSITION {
            code.push(PADDING);
        }
        code.push(SEPARATOR);
    } else {
        code.push_str(&digits[..SEPARATOR_POSITION]);
        code.push(SEPARATOR);
        code.push_str(&digits[SEPARATOR_POSITION..code_length]);
    }
    code
}

/// Decode a full Plus Code to the centre of the area it names.
///
/// Short codes (those with `0` padding, or relative codes like `9G8F+6W`) are
/// rejected: resolving them needs a reference location, and guessing one would
/// silently move the pin.
pub fn decode(code: &str) -> Option<(f64, f64)> {
    let code = code.trim();
    let separator = code.find(SEPARATOR)?;
    if separator != SEPARATOR_POSITION || code.contains(PADDING) {
        return None;
    }

    let digits: Vec<usize> = code
        .chars()
        .filter(|c| *c != SEPARATOR)
        .map(|c| {
            ALPHABET
                .iter()
                .position(|a| a.eq_ignore_ascii_case(&(c as u8)))
        })
        .collect::<Option<Vec<_>>>()?;

    // The pair stage consumes digits two at a time, so anything up to
    // PAIR_CODE_LENGTH must be even. Beyond that each grid digit stands alone,
    // so an 11-digit code is perfectly valid.
    if digits.is_empty()
        || digits.len() > MAX_DIGIT_COUNT
        || (digits.len() <= PAIR_CODE_LENGTH && digits.len() % 2 != 0)
    {
        return None;
    }

    let mut lat = -(LATITUDE_MAX as f64);
    let mut lon = -(LONGITUDE_MAX as f64);

    // Each pair refines by a factor of 20, starting at 20 degrees:
    // 20, 1, 1/20, 1/400, 1/8000.
    let mut resolution = 20.0_f64;
    let pair_digits = digits.len().min(PAIR_CODE_LENGTH);
    for i in (0..pair_digits).step_by(2) {
        lat += digits[i] as f64 * resolution;
        lon += digits[i + 1] as f64 * resolution;
        if i + 2 < pair_digits {
            resolution /= 20.0;
        }
    }

    // The last pair's resolution is the size of the cell we have landed in.
    let mut lat_size = resolution;
    let mut lon_size = resolution;

    for &digit in digits.iter().skip(PAIR_CODE_LENGTH) {
        lat_size /= GRID_ROWS as f64;
        lon_size /= GRID_COLUMNS as f64;
        let row = digit as i64 / GRID_COLUMNS;
        let col = digit as i64 % GRID_COLUMNS;
        lat += row as f64 * lat_size;
        lon += col as f64 * lon_size;
    }

    // Name the centre of the area, not its corner.
    Some((lat + lat_size / 2.0, lon + lon_size / 2.0))
}

/// Is this a well-formed full Plus Code?
pub fn is_full_code(candidate: &str) -> bool {
    decode(candidate).is_some()
}

fn normalise_length(requested: usize) -> usize {
    let capped = requested.clamp(2, MAX_DIGIT_COUNT);
    if capped < PAIR_CODE_LENGTH && capped % 2 != 0 {
        capped - 1
    } else {
        capped
    }
}

fn clip_latitude(lat: f64) -> f64 {
    lat.clamp(-(LATITUDE_MAX as f64), LATITUDE_MAX as f64)
}

fn normalise_longitude(lon: f64) -> f64 {
    let mut lon = lon;
    while lon < -(LONGITUDE_MAX as f64) {
        lon += 360.0;
    }
    while lon >= LONGITUDE_MAX as f64 {
        lon -= 360.0;
    }
    lon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_canonical_example() {
        assert_eq!(encode(20.3701125, 2.782234375, 11), "7FG49QCJ+2VX");
    }

    #[test]
    fn short_lengths_are_padded() {
        assert_eq!(encode(20.375, 2.775, 6), "7FG49Q00+");
    }

    #[test]
    fn round_trips_within_one_cell() {
        for (lat, lon) in [
            (51.5007292, -0.1246254),
            (-33.8688, 151.2093),
            (0.0, 0.0),
            (-89.9, -179.9),
        ] {
            let code = encode(lat, lon, 11);
            let (dlat, dlon) = decode(&code).expect("full code must decode");
            assert!(
                (dlat - lat).abs() < 0.0002 && (dlon - lon).abs() < 0.0002,
                "{code} decoded to {dlat},{dlon}, expected {lat},{lon}"
            );
        }
    }

    #[test]
    fn padded_and_relative_codes_are_rejected() {
        assert!(decode("7FG49Q00+").is_none(), "padded code");
        assert!(
            decode("9G8F+6W").is_none(),
            "relative code needs a locality"
        );
        assert!(decode("not a code").is_none());
    }

    #[test]
    fn longitude_wraps_rather_than_clamping() {
        assert_eq!(encode(0.0, 190.0, 10), encode(0.0, -170.0, 10));
    }
}
