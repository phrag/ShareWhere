//! Rule catalogs for ShareWhere.
//!
//! Two catalogs are embedded in the binary:
//!
//! * `assets/clearurls.min.json` — vendored from the ClearURLs project,
//!   **LGPL-3.0**. ~106 KB, 206 providers. See `NOTICE` at the repo root.
//! * `assets/sharewhere.rules.json` — our own layer, GPL-3.0-or-later. Covers
//!   gaps upstream has (Google Maps most of all) and carries the `preserve`
//!   safelist.
//!
//! Both are parsed lazily on first use: the parse is 1-3 ms and is not the
//! bottleneck. Regex compilation is, which is why [`candidates`] exists — see
//! `build.rs` for how the tier-1 index is produced.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use indexmap::IndexMap;
use serde::Deserialize;

include!(concat!(env!("OUT_DIR"), "/host_index.rs"));

/// One provider's rules. Field names mirror the ClearURLs catalog schema so a
/// single engine can evaluate both catalogs.
#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    #[serde(rename = "urlPattern")]
    pub url_pattern: String,

    /// The whole URL is a tracker, not a link with tracking bolted on. We do
    /// not try to "clean" these; we warn instead.
    #[serde(rename = "completeProvider", default)]
    pub complete_provider: bool,

    #[serde(rename = "forceRedirection", default)]
    pub force_redirection: bool,

    /// Query/fragment parameter name patterns to drop.
    #[serde(default)]
    pub rules: Vec<String>,

    /// Whole-URL regex replacements. This is how Amazon's `/ref=…` path
    /// segment gets removed.
    #[serde(rename = "rawRules", default)]
    pub raw_rules: Vec<String>,

    /// Affiliate parameters. Dropped separately so the user can choose to keep
    /// them and not cost a creator their commission.
    #[serde(rename = "referralMarketing", default)]
    pub referral_marketing: Vec<String>,

    /// If any of these match the URL, the provider is skipped entirely.
    #[serde(default)]
    pub exceptions: Vec<String>,

    /// Patterns whose first capture group is a wrapped destination URL.
    #[serde(default)]
    pub redirections: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub version: String,
    pub providers: IndexMap<String, Provider>,
    /// Host suffix -> parameter names that must never be removed.
    #[serde(default)]
    pub preserve: BTreeMap<String, Vec<String>>,
}

impl Catalog {
    pub fn get(&self, name: &str) -> Option<&Provider> {
        self.providers.get(name)
    }
}

const CLEARURLS_JSON: &str = include_str!("../assets/clearurls.min.json");
const CUSTOM_JSON: &str = include_str!("../assets/sharewhere.rules.json");

static CLEARURLS: OnceLock<Catalog> = OnceLock::new();
static CUSTOM: OnceLock<Catalog> = OnceLock::new();

/// The vendored ClearURLs catalog (LGPL-3.0).
pub fn clearurls() -> &'static Catalog {
    CLEARURLS.get_or_init(|| {
        serde_json::from_str(CLEARURLS_JSON).expect("vendored clearurls catalog must parse")
    })
}

/// ShareWhere's own rule layer, applied after the vendored catalog.
pub fn custom() -> &'static Catalog {
    CUSTOM.get_or_init(|| serde_json::from_str(CUSTOM_JSON).expect("sharewhere rules must parse"))
}

/// Which catalog a candidate provider came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    ClearUrls,
    Custom,
}

/// A provider that might apply to a URL. The engine still has to verify it
/// against the provider's real `urlPattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub source: Source,
    pub name: &'static str,
    /// True when the provider applies to every URL (`urlPattern` of `.*`), so
    /// the engine can skip the verification regex.
    pub global: bool,
}

fn lookup(
    index: &'static [(&'static str, &'static [&'static str])],
    label: &str,
) -> &'static [&'static str] {
    match index.binary_search_by(|(k, _)| (*k).cmp(label)) {
        Ok(i) => index[i].1,
        Err(_) => &[],
    }
}

fn collect(
    out: &mut Vec<Candidate>,
    source: Source,
    index: &'static [(&'static str, &'static [&'static str])],
    residue: &'static [&'static str],
    global: &'static [&'static str],
    host: &str,
) {
    for name in global {
        out.push(Candidate {
            source,
            name,
            global: true,
        });
    }
    let host = host.to_ascii_lowercase();
    for label in host.split('.') {
        if label.is_empty() {
            continue;
        }
        for name in lookup(index, label) {
            if !out.iter().any(|c| c.source == source && c.name == *name) {
                out.push(Candidate {
                    source,
                    name,
                    global: false,
                });
            }
        }
    }
    for name in residue {
        out.push(Candidate {
            source,
            name,
            global: false,
        });
    }
}

/// Tier-1 prefilter: the providers that could possibly apply to `host`.
///
/// Deliberately over-selects. Typically returns 1-3 host-specific providers
/// plus `globalRules` plus the small residue set, instead of all 206.
pub fn candidates(host: &str) -> Vec<Candidate> {
    let mut out = Vec::with_capacity(12);
    collect(
        &mut out,
        Source::ClearUrls,
        CLEARURLS_INDEX,
        CLEARURLS_RESIDUE,
        CLEARURLS_GLOBAL,
        host,
    );
    collect(
        &mut out,
        Source::Custom,
        CUSTOM_INDEX,
        CUSTOM_RESIDUE,
        CUSTOM_GLOBAL,
        host,
    );
    out
}

/// Parameter names that must survive sanitisation for this host.
///
/// Matched by host suffix, so `www.youtube.com` picks up the `youtube.com`
/// entry. Suffix matching is on label boundaries: `notyoutube.com` does not
/// match `youtube.com`.
pub fn preserved_params(host: &str) -> Vec<&'static str> {
    let host = host.to_ascii_lowercase();
    let mut out = Vec::new();
    for (suffix, params) in &custom().preserve {
        let matches = host == *suffix
            || (host.len() > suffix.len()
                && host.ends_with(suffix.as_str())
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.');
        if matches {
            out.extend(params.iter().map(|s| s.as_str()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_parse() {
        assert!(clearurls().providers.len() > 150);
        assert!(custom().providers.contains_key("googlemaps"));
    }

    #[test]
    fn global_rules_are_recognised_as_global() {
        assert!(CLEARURLS_GLOBAL.contains(&"globalRules"));
    }

    #[test]
    fn candidates_are_narrow_but_include_globals() {
        let c = candidates("www.amazon.co.uk");
        let names: Vec<_> = c.iter().map(|c| c.name).collect();
        assert!(names.contains(&"amazon"), "got {names:?}");
        assert!(names.contains(&"globalRules"), "got {names:?}");
        // The whole point: nothing like the full 206-provider catalog.
        assert!(c.len() < 30, "prefilter selected {} providers", c.len());
    }

    #[test]
    fn candidates_cover_our_custom_providers() {
        let names: Vec<_> = candidates("www.tiktok.com")
            .iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&"tiktok"), "got {names:?}");
    }

    #[test]
    fn preserve_matches_on_label_boundaries() {
        assert!(preserved_params("www.youtube.com").contains(&"v"));
        assert!(preserved_params("youtube.com").contains(&"t"));
        assert!(preserved_params("notyoutube.com").is_empty());
    }

    /// Guards the finding the whole design rests on: every catalog regex has to
    /// compile under the Rust `regex` crate with the feature set we ship. If
    /// upstream ever introduces a lookahead or backreference this fails loudly
    /// rather than silently dropping a rule at runtime.
    ///
    /// Lives in the engine crate's tests too, where `regex` is a dependency.
    #[test]
    fn every_provider_has_a_url_pattern() {
        for (name, p) in &clearurls().providers {
            assert!(!p.url_pattern.is_empty(), "{name} has an empty urlPattern");
        }
    }
}
