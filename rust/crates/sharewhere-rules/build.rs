//! Generates the tier-1 provider index at build time.
//!
//! Compiling all ~1100 catalog regexes eagerly costs 50-150ms, which is far too
//! slow for a share-sheet activity that has to feel instant. So we do the cheap
//! part here: pull the first literal domain label out of each provider's
//! `urlPattern` (e.g. `amazon` from
//! `^https?:\/\/(?:[a-z0-9-]+\.)*?amazon(?:\.[a-z]{2,}){1,}`) and emit a sorted
//! table from label -> provider names.
//!
//! At runtime we split the URL's host into labels, look each one up, and only
//! compile the handful of providers that could possibly match.
//!
//! This is a *prefilter*, not a matcher. It is allowed to over-select: the
//! engine still verifies each candidate against the provider's real
//! `urlPattern` regex. Correctness therefore never depends on the extraction
//! below being clever. Patterns it cannot reduce land in the residue list and
//! are checked on every call, so the worst case is slower, never wrong.

use std::collections::BTreeMap;
use std::path::Path;

/// Scheme prefixes we know how to strip, longest first so `https:\/\/` is not
/// shadowed by a shorter alternative.
const SCHEME_PREFIXES: &[&str] = &[
    r"^https?:\/\/",
    r"^https:\/\/",
    r"^http:\/\/",
    r"^wss?:\/\/",
    r"^https?://",
];

/// The "any subdomain" groups the catalog puts between scheme and domain.
const SUBDOMAIN_GROUPS: &[&str] = &[
    r"(?:[a-z0-9-]+\.)*?",
    r"(?:[a-z0-9-]+\.)*",
    r"(?:[a-z0-9-]+\.)+?",
    r"(?:[a-z0-9-]+\.)+",
    r"([a-z0-9-\.])*",
    r"([a-z0-9-.]*\.)",
];

/// Pull the first literal domain label out of a `urlPattern`.
///
/// Returns `None` when the pattern does not start with a recognisable scheme,
/// or when the domain position holds a metacharacter (an alternation such as
/// `(?:youtube\.com|youtu\.be)`) rather than a literal.
fn first_label(pattern: &str) -> Option<String> {
    let mut s = pattern;

    let mut matched_scheme = false;
    for prefix in SCHEME_PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            matched_scheme = true;
            break;
        }
    }
    if !matched_scheme {
        return None;
    }

    for group in SUBDOMAIN_GROUPS {
        if let Some(rest) = s.strip_prefix(group) {
            s = rest;
            break;
        }
    }

    // Walk the literal run. `\.` and `\-` are escaped literals; a bare `.` is
    // technically "any char" but every catalog pattern that uses one means a
    // dot, and treating it as a literal only ever widens the prefilter.
    let mut literal = String::new();
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '\\' && i + 1 < bytes.len() && (bytes[i + 1] == '.' || bytes[i + 1] == '-') {
            literal.push(bytes[i + 1]);
            i += 2;
            continue;
        }
        if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
            literal.push(c);
            i += 1;
            continue;
        }
        break;
    }

    let label = literal.split('.').next().unwrap_or_default();
    if label.is_empty() {
        None
    } else {
        Some(label.to_ascii_lowercase())
    }
}

/// A provider that applies to every URL regardless of host.
fn is_global(pattern: &str) -> bool {
    matches!(pattern, ".*" | "^.*" | "^.*$" | "")
}

struct Index {
    by_label: BTreeMap<String, Vec<String>>,
    residue: Vec<String>,
    global: Vec<String>,
}

fn build_index(json: &serde_json::Value) -> Index {
    let mut by_label: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut residue = Vec::new();
    let mut global = Vec::new();

    let providers = json
        .get("providers")
        .and_then(|p| p.as_object())
        .expect("catalog must have a `providers` object");

    for (name, provider) in providers {
        let pattern = provider
            .get("urlPattern")
            .and_then(|p| p.as_str())
            .unwrap_or_default();

        if is_global(pattern) {
            global.push(name.clone());
        } else if let Some(label) = first_label(pattern) {
            by_label.entry(label).or_default().push(name.clone());
        } else {
            residue.push(name.clone());
        }
    }

    Index {
        by_label,
        residue,
        global,
    }
}

fn emit(out: &mut String, prefix: &str, index: &Index) {
    out.push_str(&format!(
        "pub static {prefix}_INDEX: &[(&str, &[&str])] = &[\n"
    ));
    for (label, names) in &index.by_label {
        let list = names
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("    ({label:?}, &[{list}]),\n"));
    }
    out.push_str("];\n\n");

    let residue = index
        .residue
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        "pub static {prefix}_RESIDUE: &[&str] = &[{residue}];\n\n"
    ));

    let global = index
        .global
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        "pub static {prefix}_GLOBAL: &[&str] = &[{global}];\n\n"
    ));
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let clearurls_path = Path::new(&manifest).join("assets/clearurls.min.json");
    let custom_path = Path::new(&manifest).join("assets/sharewhere.rules.json");

    println!("cargo:rerun-if-changed={}", clearurls_path.display());
    println!("cargo:rerun-if-changed={}", custom_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let clearurls: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&clearurls_path).expect("read clearurls catalog"))
            .expect("parse clearurls catalog");
    let custom: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&custom_path).expect("read sharewhere rules"))
            .expect("parse sharewhere rules");

    let clearurls_index = build_index(&clearurls);
    let custom_index = build_index(&custom);

    // The prefilter is a performance feature, but a collapse in coverage would
    // silently turn every lookup into a full residue scan. Fail the build
    // instead of shipping that.
    let indexed: usize = clearurls_index.by_label.values().map(Vec::len).sum();
    let total = indexed + clearurls_index.residue.len() + clearurls_index.global.len();
    let coverage = indexed as f64 / total as f64;
    assert!(
        coverage >= 0.90,
        "host-index coverage fell to {:.1}% ({indexed}/{total}); \
         upstream urlPattern shapes have changed and first_label() needs updating",
        coverage * 100.0
    );

    let mut out = String::from("// @generated by build.rs - do not edit\n\n");
    emit(&mut out, "CLEARURLS", &clearurls_index);
    emit(&mut out, "CUSTOM", &custom_index);

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("host_index.rs");
    std::fs::write(dest, out).expect("write host_index.rs");
}
