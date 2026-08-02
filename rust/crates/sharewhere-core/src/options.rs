//! Everything the user gets to decide.
//!
//! Policy lives here, in one place, rather than being scattered across the
//! platform layer. The defaults are the privacy defaults: no network until
//! asked, exact coordinates only because that is what the user typed, and
//! affiliate parameters removed.

use sharewhere_geo::{Precision, RenderOptions};
use sharewhere_url::SanitizeOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub sanitize: SanitizeOptions,

    /// The master switch. False means the core will never emit a
    /// [`FetchRequest`](crate::FetchRequest) — it returns
    /// [`Outcome::NeedsConsent`](crate::Outcome::NeedsConsent) instead, and
    /// nothing leaves the device.
    ///
    /// Default false. The app turns it on for a single resolve after the user
    /// taps the button, not globally.
    pub allow_network: bool,

    /// How precisely to share a location.
    pub precision: Precision,

    /// Include the place name in generated links. "Home" often says more than
    /// the coordinates do.
    pub include_label: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            sanitize: SanitizeOptions::default(),
            allow_network: false,
            precision: Precision::default(),
            include_label: true,
        }
    }
}

impl Options {
    /// A one-shot copy with the network permitted, for use after the user has
    /// agreed to a specific request.
    pub fn with_consent(&self) -> Self {
        Self {
            allow_network: true,
            ..self.clone()
        }
    }

    pub fn render(&self) -> RenderOptions {
        RenderOptions {
            precision: self.precision,
            include_label: self.include_label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_offline() {
        assert!(
            !Options::default().allow_network,
            "network access must be opt-in"
        );
    }

    #[test]
    fn consent_is_granted_per_use_and_does_not_mutate_the_original() {
        let options = Options::default();
        let consented = options.with_consent();
        assert!(consented.allow_network);
        assert!(!options.allow_network);
    }
}
