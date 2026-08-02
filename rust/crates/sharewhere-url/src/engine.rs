//! The sanitiser.
//!
//! Two things about the implementation are deliberate and worth knowing before
//! changing anything here.
//!
//! **We do not round-trip through `Url`'s serialiser.** Parsing and
//! re-serialising normalises: `https://example.com` becomes
//! `https://example.com/`, `%20` becomes `+`, and so on. That would make
//! `changed` true for URLs where we removed nothing, and an app that claims to
//! have cleaned a link it did not touch is an app you stop trusting. So the URL
//! is split on `?` and `#` as raw text, retained parameters are copied across
//! byte-for-byte, and `Url` is used only to extract the host and to validate
//! schemes.
//!
//! **Parameters are matched by key, not by string surgery.** ClearURLs' browser
//! extension regex-replaces against the raw URL with patterns like
//! `(?:&|[/?#])(?:%3F)?rule(?:=|\[\]=)[^&]*`. Splitting on `&` and testing the
//! decoded key against an anchored pattern is both simpler and more correct
//! around percent-encoding, repeated keys and empty values.

use percent_encoding::percent_decode_str;
use sharewhere_rules::Candidate;
use url::Url;

use crate::compiled::{self, CompiledProvider};
use crate::types::{Error, RemovalKind, RemovedParam, SanitizeOptions, Sanitized};

/// Inputs beyond this are rejected. Bounds the cost of whole-URL regex
/// replacement on adversarial input; the longest real share-sheet URLs are a
/// few hundred bytes.
pub const MAX_INPUT_LEN: usize = 8192;

/// Fixed-point iterations. A `rawRule` can expose a parameter that a `rule`
/// then matches, so one pass is not always enough; three is well past what any
/// real catalog rule needs and guarantees termination.
const MAX_PASSES: u8 = 3;

#[derive(Default)]
struct Acc {
    removed: Vec<RemovedParam>,
    providers: Vec<String>,
    unwrapped_redirect: bool,
    complete_provider: bool,
}

impl Acc {
    fn note(&mut self, provider: &str) {
        if !self.providers.iter().any(|p| p == provider) {
            self.providers.push(provider.to_string());
        }
    }
}

enum Step {
    Same,
    Changed(String),
    /// A wrapped destination URL was found; restart on it.
    Redirect(String),
}

/// Clean a single URL.
pub fn sanitize_url(input: &str, opts: &SanitizeOptions) -> Result<Sanitized, Error> {
    let input = input.trim();
    if input.len() > MAX_INPUT_LEN {
        return Err(Error::InputTooLong {
            len: input.len(),
            max: MAX_INPUT_LEN,
        });
    }

    let mut acc = Acc::default();
    let cleaned = run(input, opts, 0, &mut acc)?;

    Ok(Sanitized {
        changed: cleaned != input,
        original: input.to_string(),
        cleaned,
        removed: acc.removed,
        providers: acc.providers,
        unwrapped_redirect: acc.unwrapped_redirect,
        complete_provider: acc.complete_provider,
    })
}

fn run(current: &str, opts: &SanitizeOptions, hops: u8, acc: &mut Acc) -> Result<String, Error> {
    let parsed = Url::parse(current).map_err(|_| Error::InvalidUrl(current.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::UnsupportedScheme(parsed.scheme().to_string()));
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let preserved = sharewhere_rules::preserved_params(&host);

    let mut s = current.to_string();
    for _ in 0..MAX_PASSES {
        match apply_once(&s, &host, &preserved, opts, hops, acc)? {
            Step::Same => break,
            Step::Changed(next) => s = next,
            Step::Redirect(target) => {
                // Only commit to the unwrap if the target itself survives the
                // engine. If it does not, keep what we had rather than
                // returning something worse than the input.
                if let Ok(resolved) = run(&target, opts, hops + 1, acc) {
                    acc.unwrapped_redirect = true;
                    return Ok(resolved);
                }
                break;
            }
        }
    }
    Ok(s)
}

fn apply_once(
    s: &str,
    host: &str,
    preserved: &[&str],
    opts: &SanitizeOptions,
    hops: u8,
    acc: &mut Acc,
) -> Result<Step, Error> {
    let mut cur = s.to_string();
    let mut changed = false;

    for candidate in sharewhere_rules::candidates(host) {
        let Some(compiled) = compiled::get(candidate) else {
            continue;
        };

        // Verify the tier-1 prefilter's guess against the provider's real
        // pattern. Global providers skip this by construction.
        if let Some(pattern) = &compiled.url_pattern {
            if !pattern.is_match(&cur) {
                continue;
            }
        }
        if let Some(exceptions) = &compiled.exceptions {
            if exceptions.is_match(&cur) {
                continue;
            }
        }

        if compiled.complete_provider {
            acc.complete_provider = true;
            acc.note(candidate.name);
            continue;
        }

        if opts.follow_redirect_params && hops < opts.max_hops {
            if let Some(target) = find_redirect(&cur, &compiled) {
                acc.note(candidate.name);
                return Ok(Step::Redirect(target));
            }
        }

        if apply_raw_rules(&mut cur, &compiled, candidate, acc) {
            changed = true;
        }
        if apply_param_rules(&mut cur, &compiled, candidate, preserved, opts, acc) {
            changed = true;
        }
    }

    Ok(if changed {
        Step::Changed(cur)
    } else {
        Step::Same
    })
}

/// Whole-URL rewrites. This is what strips Amazon's `/ref=…` path segment.
fn apply_raw_rules(
    cur: &mut String,
    compiled: &CompiledProvider,
    candidate: Candidate,
    acc: &mut Acc,
) -> bool {
    let mut changed = false;
    for rule in &compiled.raw_rules {
        let hits: Vec<String> = rule
            .find_iter(cur)
            .map(|m| m.as_str().to_string())
            .filter(|h| !h.is_empty())
            .collect();
        if hits.is_empty() {
            continue;
        }
        let next = rule.replace_all(cur, "").into_owned();
        if next != *cur {
            for hit in hits {
                acc.removed.push(RemovedParam {
                    key: hit,
                    value: String::new(),
                    kind: RemovalKind::RawRule,
                    provider: candidate.name.to_string(),
                });
            }
            acc.note(candidate.name);
            *cur = next;
            changed = true;
        }
    }
    changed
}

/// Drop matching parameters from the query and, where it is query-shaped, the
/// fragment. ClearURLs applies field rules to both.
fn apply_param_rules(
    cur: &mut String,
    compiled: &CompiledProvider,
    candidate: Candidate,
    preserved: &[&str],
    opts: &SanitizeOptions,
    acc: &mut Acc,
) -> bool {
    if compiled.rules.is_none() && compiled.referral.is_none() {
        return false;
    }

    let parts = split_url(cur);
    let mut changed = false;

    let new_query = parts
        .query
        .and_then(|q| filter_params(q, compiled, candidate, preserved, opts, acc));
    // Only touch the fragment when it actually looks like `k=v`; a plain
    // anchor such as `#installation` must be left alone.
    let new_fragment = parts
        .fragment
        .filter(|f| is_query_shaped(f))
        .and_then(|f| filter_params(f, compiled, candidate, preserved, opts, acc));

    if new_query.is_none() && new_fragment.is_none() {
        return false;
    }

    let query = match &new_query {
        Some(q) => q.as_deref(),
        None => parts.query,
    };
    let fragment = match &new_fragment {
        Some(f) => f.as_deref(),
        None => parts.fragment,
    };

    let mut rebuilt = String::with_capacity(cur.len());
    rebuilt.push_str(parts.base);
    if let Some(q) = query {
        if !q.is_empty() {
            rebuilt.push('?');
            rebuilt.push_str(q);
        }
    }
    if let Some(f) = fragment {
        if !f.is_empty() {
            rebuilt.push('#');
            rebuilt.push_str(f);
        }
    }

    if rebuilt != *cur {
        *cur = rebuilt;
        changed = true;
    }
    changed
}

/// Returns `Some(new_value)` when at least one parameter was dropped. The inner
/// `Option<String>` is `None` when every parameter was dropped, so the caller
/// can omit the `?` or `#` entirely.
#[allow(clippy::option_option)]
fn filter_params(
    raw: &str,
    compiled: &CompiledProvider,
    candidate: Candidate,
    preserved: &[&str],
    opts: &SanitizeOptions,
    acc: &mut Acc,
) -> Option<Option<String>> {
    let mut kept: Vec<&str> = Vec::new();
    let mut dropped = false;

    for segment in raw.split('&') {
        if segment.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match segment.split_once('=') {
            Some((k, v)) => (k, v),
            None => (segment, ""),
        };
        let key = decode(raw_key);

        // The safelist wins over every rule. This is what stops us breaking a
        // link by removing something load-bearing.
        if preserved.iter().any(|p| p.eq_ignore_ascii_case(&key)) {
            kept.push(segment);
            continue;
        }

        let kind = if compiled
            .rules
            .as_ref()
            .is_some_and(|set| set.is_match(&key))
        {
            Some(RemovalKind::Tracking)
        } else if opts.remove_referral
            && compiled
                .referral
                .as_ref()
                .is_some_and(|set| set.is_match(&key))
        {
            Some(RemovalKind::Referral)
        } else {
            None
        };

        match kind {
            Some(kind) => {
                acc.removed.push(RemovedParam {
                    key,
                    value: decode(raw_value),
                    kind,
                    provider: candidate.name.to_string(),
                });
                acc.note(candidate.name);
                dropped = true;
            }
            None => kept.push(segment),
        }
    }

    if !dropped {
        return None;
    }
    Some(if kept.is_empty() {
        None
    } else {
        Some(kept.join("&"))
    })
}

struct Parts<'a> {
    base: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

/// Split a URL string on `?` and `#` without normalising anything.
///
/// Per URL syntax the fragment starts at the first `#`, and the query is the
/// first `?` *before* it — a `?` inside the fragment is part of the fragment.
fn split_url(s: &str) -> Parts<'_> {
    let (before_fragment, fragment) = match s.find('#') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let (base, query) = match before_fragment.find('?') {
        Some(i) => (&before_fragment[..i], Some(&before_fragment[i + 1..])),
        None => (before_fragment, None),
    };
    Parts {
        base,
        query,
        fragment,
    }
}

fn is_query_shaped(fragment: &str) -> bool {
    fragment.contains('=') && !fragment.starts_with('/')
}

fn decode(s: &str) -> String {
    percent_decode_str(s)
        .decode_utf8()
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

/// Pull a wrapped destination URL out of a redirector.
fn find_redirect(cur: &str, compiled: &CompiledProvider) -> Option<String> {
    for rule in &compiled.redirections {
        let captures = rule.captures(cur)?;
        if let Some(m) = captures.get(1) {
            if let Some(target) = extract_target(m.as_str()) {
                return Some(target);
            }
        }
    }
    None
}

/// Decode a captured redirect target and validate it.
///
/// SECURITY: the target is attacker-controlled — anyone can craft
/// `google.com/url?q=intent://…`. Returning a non-http(s) URL here would turn
/// ShareWhere into a scheme-laundering machine: the user asked us to clean a
/// web link and we would hand their launcher an arbitrary intent. Only `http`
/// and `https` ever come back out.
fn extract_target(raw: &str) -> Option<String> {
    let mut current = raw.to_string();

    for _ in 0..3 {
        if let Ok(parsed) = Url::parse(&current) {
            return matches!(parsed.scheme(), "http" | "https").then_some(current);
        }
        let decoded = decode(&current);
        if decoded == current {
            return None;
        }
        current = decoded;
    }
    None
}
