//! Lazy regex compilation with a process-wide cache.
//!
//! Compiling the whole catalog eagerly costs 50-150 ms. We compile only the
//! providers the tier-1 index selected — typically one or two per URL, plus
//! `globalRules` — and keep them for the life of the process. A warm call is
//! then pure matching.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use regex::{Regex, RegexSet};
use sharewhere_rules::{Candidate, Provider, Source};

pub struct CompiledProvider {
    /// Verifies the tier-1 prefilter's guess. `None` for global providers,
    /// which apply unconditionally.
    pub url_pattern: Option<Regex>,
    pub exceptions: Option<RegexSet>,
    pub redirections: Vec<Regex>,
    pub raw_rules: Vec<Regex>,
    pub rules: Option<RegexSet>,
    pub referral: Option<RegexSet>,
    pub complete_provider: bool,
}

/// Compile a catalog pattern that matches against a whole URL.
///
/// Case-insensitive to match ClearURLs, which builds its regexes with the `gi`
/// flags. A pattern that fails to compile is dropped rather than panicking:
/// one bad upstream rule should degrade that rule, not break the app. The
/// conformance test in `tests/` is what makes sure this never happens quietly.
fn whole(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("(?i){pattern}")).ok()
}

/// Compile a catalog pattern that matches against a *parameter name*.
///
/// The catalog's rule patterns are written to match a complete key, so they get
/// anchored. Without the anchors, a rule like `s` would match every parameter
/// containing an "s".
fn param(pattern: &str) -> String {
    format!("(?i)^(?:{pattern})$")
}

fn set(patterns: &[String], f: impl Fn(&str) -> String) -> Option<RegexSet> {
    if patterns.is_empty() {
        return None;
    }
    let mapped: Vec<String> = patterns.iter().map(|p| f(p)).collect();
    match RegexSet::new(&mapped) {
        Ok(s) => Some(s),
        // One malformed pattern would poison the whole set, so fall back to
        // compiling them individually and keeping the ones that work.
        Err(_) => {
            let good: Vec<String> = mapped
                .into_iter()
                .filter(|p| Regex::new(p).is_ok())
                .collect();
            RegexSet::new(&good).ok()
        }
    }
}

fn compile(provider: &Provider, global: bool) -> CompiledProvider {
    CompiledProvider {
        url_pattern: if global {
            None
        } else {
            whole(&provider.url_pattern)
        },
        exceptions: set(&provider.exceptions, |p| format!("(?i){p}")),
        redirections: provider
            .redirections
            .iter()
            .filter_map(|p| whole(p))
            .collect(),
        raw_rules: provider.raw_rules.iter().filter_map(|p| whole(p)).collect(),
        rules: set(&provider.rules, param),
        referral: set(&provider.referral_marketing, param),
        complete_provider: provider.complete_provider,
    }
}

type Cache = Mutex<HashMap<(Source, &'static str), Arc<CompiledProvider>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fetch a compiled provider, compiling it on first use.
pub fn get(candidate: Candidate) -> Option<Arc<CompiledProvider>> {
    let key = (candidate.source, candidate.name);

    // A poisoned lock means another thread panicked mid-compile. Recover
    // rather than propagating: the cache holds no invariants worth protecting.
    if let Some(hit) = cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
    {
        return Some(hit);
    }

    let catalog = match candidate.source {
        Source::ClearUrls => sharewhere_rules::clearurls(),
        Source::Custom => sharewhere_rules::custom(),
    };
    let provider = catalog.get(candidate.name)?;
    let compiled = Arc::new(compile(provider, candidate.global));

    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, compiled.clone());
    Some(compiled)
}

/// Compile `globalRules` up front so the first share of a session is already
/// warm. Called from app start; cheap enough to be unconditional.
pub fn warm() {
    for candidate in sharewhere_rules::candidates("example.com") {
        if candidate.global {
            let _ = get(candidate);
        }
    }
}
