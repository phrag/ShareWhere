//! The coordinate model.

/// A point on the earth, plus whatever context the source link carried.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
    /// Place name from the source link, if it had one.
    pub label: Option<String>,
    pub zoom: Option<f64>,
    pub accuracy_m: Option<f64>,
}

impl GeoPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            label: None,
            zoom: None,
            accuracy_m: None,
        }
    }

    pub fn with_label(mut self, label: Option<String>) -> Self {
        self.label = label.filter(|l| !l.trim().is_empty());
        self
    }

    pub fn with_zoom(mut self, zoom: Option<f64>) -> Self {
        self.zoom = zoom;
        self
    }

    /// Are these coordinates on the earth at all?
    ///
    /// Worth checking explicitly: several source formats put longitude first,
    /// and a swapped pair often lands outside the valid latitude range, which
    /// is the only automatic signal that something is wrong.
    pub fn is_valid(&self) -> bool {
        self.lat.is_finite()
            && self.lon.is_finite()
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
    }

    /// Round to `precision`, and optionally drop the place name.
    ///
    /// Sharing an exact coordinate is itself a privacy event, and a label like
    /// "Home" often leaks more than the numbers do. This app is unusually well
    /// placed to offer the blur, so it does.
    pub fn at_precision(&self, precision: Precision, keep_label: bool) -> GeoPoint {
        let decimals = precision.decimals();
        GeoPoint {
            lat: round_to(self.lat, decimals),
            lon: round_to(self.lon, decimals),
            label: if keep_label { self.label.clone() } else { None },
            zoom: self.zoom,
            accuracy_m: self.accuracy_m.or(precision.approximate_metres()),
        }
    }

    /// The point formatted for a link, trailing zeros trimmed.
    pub fn format(&self, precision: Precision) -> (String, String) {
        let decimals = precision.decimals();
        (
            format_coordinate(self.lat, decimals),
            format_coordinate(self.lon, decimals),
        )
    }
}

/// How precisely to share a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// ~11 cm. What every other maps app emits.
    #[default]
    Exact,
    /// ~100 m — the building or block, not the doorway.
    Approximate,
    /// ~1 km — the neighbourhood.
    Coarse,
}

impl Precision {
    pub fn decimals(self) -> usize {
        match self {
            Precision::Exact => 6,
            Precision::Approximate => 3,
            Precision::Coarse => 2,
        }
    }

    fn approximate_metres(self) -> Option<f64> {
        match self {
            Precision::Exact => None,
            Precision::Approximate => Some(100.0),
            Precision::Coarse => Some(1000.0),
        }
    }
}

fn round_to(value: f64, decimals: usize) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

/// Format with at most `decimals` places, trimming trailing zeros so links stay
/// short, but always keeping at least one decimal so the value still reads as a
/// coordinate rather than an integer.
fn format_coordinate(value: f64, decimals: usize) -> String {
    let mut s = format!("{value:.decimals$}");
    if s.contains('.') {
        s = s.trim_end_matches('0').to_string();
        if s.ends_with('.') {
            s.push('0');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity_catches_out_of_range_coordinates() {
        assert!(GeoPoint::new(51.5, -0.12).is_valid());
        assert!(!GeoPoint::new(151.2, 33.8).is_valid(), "swapped lat/lon");
        assert!(!GeoPoint::new(f64::NAN, 0.0).is_valid());
    }

    #[test]
    fn formatting_trims_trailing_zeros_but_stays_a_decimal() {
        assert_eq!(format_coordinate(51.5007292, 6), "51.500729");
        assert_eq!(format_coordinate(51.5, 6), "51.5");
        assert_eq!(format_coordinate(0.0, 6), "0.0");
        assert_eq!(format_coordinate(-0.1246254, 6), "-0.124625");
    }

    #[test]
    fn blurring_reduces_precision_and_can_drop_the_label() {
        let point = GeoPoint::new(51.5007292, -0.1246254).with_label(Some("Home".into()));

        let coarse = point.at_precision(Precision::Coarse, false);
        assert_eq!(coarse.lat, 51.5);
        assert_eq!(coarse.lon, -0.12);
        assert_eq!(coarse.label, None);

        let kept = point.at_precision(Precision::Approximate, true);
        assert_eq!(kept.label.as_deref(), Some("Home"));
        assert_eq!(kept.lat, 51.501);
    }
}
