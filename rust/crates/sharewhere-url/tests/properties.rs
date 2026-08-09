//! Invariants that must hold for every input, not just the ones we thought of.
//!
//! The corpus proves the engine does the right thing on links we have seen.
//! These prove it cannot do certain wrong things on links we have not.

use proptest::prelude::*;
use sharewhere_url::{sanitize_url, SanitizeOptions, MAX_INPUT_LEN};
use url::Url;

/// Plausible-looking URLs, so the generator spends its time in the space the
/// engine actually operates on rather than rejecting garbage at the parse step.
fn url_strategy() -> impl Strategy<Value = String> {
    let scheme = prop_oneof!["http", "https"];
    let host = prop_oneof![
        Just("example.com".to_string()),
        Just("www.amazon.co.uk".to_string()),
        Just("www.youtube.com".to_string()),
        Just("www.instagram.com".to_string()),
        Just("www.google.com".to_string()),
        Just("open.spotify.com".to_string()),
        "[a-z]{3,10}\\.(com|org|net|co\\.uk)",
    ];
    let path = prop::collection::vec("[a-zA-Z0-9_-]{1,12}", 0..4).prop_map(|segments| {
        if segments.is_empty() {
            String::new()
        } else {
            format!("/{}", segments.join("/"))
        }
    });
    let params = prop::collection::vec(
        (
            prop_oneof![
                Just("utm_source".to_string()),
                Just("utm_medium".to_string()),
                Just("fbclid".to_string()),
                Just("igshid".to_string()),
                Just("gclid".to_string()),
                Just("tag".to_string()),
                Just("v".to_string()),
                Just("id".to_string()),
                Just("q".to_string()),
                "[a-z_]{1,10}",
            ],
            "[a-zA-Z0-9._-]{0,16}",
        ),
        0..6,
    )
    .prop_map(|pairs| {
        if pairs.is_empty() {
            String::new()
        } else {
            let joined = pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");
            format!("?{joined}")
        }
    });

    (scheme, host, path, params).prop_map(|(s, h, p, q)| format!("{s}://{h}{p}{q}"))
}

/// Parameter keys present in a URL string, without normalising anything.
fn param_keys(url: &str) -> Vec<String> {
    let before_fragment = url.split('#').next().unwrap_or_default();
    let Some((_, query)) = before_fragment.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|s| s.split_once('=').map_or(s, |(k, _)| k).to_string())
        .collect()
}

proptest! {
    /// The single most important safety invariant. If the host changes without
    /// `unwrapped_redirect` being set, the app has silently sent the user
    /// somewhere other than where they were going.
    #[test]
    fn host_is_preserved_unless_a_redirect_was_unwrapped(input in url_strategy()) {
        let opts = SanitizeOptions::default();
        let result = sanitize_url(&input, &opts).unwrap();

        if !result.unwrapped_redirect {
            let before = Url::parse(&input).unwrap();
            let after = Url::parse(&result.cleaned).unwrap();
            prop_assert_eq!(before.host_str(), after.host_str());
        }
    }

    #[test]
    fn scheme_is_preserved(input in url_strategy()) {
        let opts = SanitizeOptions::default();
        let result = sanitize_url(&input, &opts).unwrap();

        let before = Url::parse(&input).unwrap();
        let after = Url::parse(&result.cleaned).unwrap();
        prop_assert_eq!(before.scheme(), after.scheme());
    }

    /// Sanitising is subtractive. No rule may ever invent a parameter.
    #[test]
    fn parameters_are_only_ever_removed(input in url_strategy()) {
        let opts = SanitizeOptions::default();
        let result = sanitize_url(&input, &opts).unwrap();
        prop_assume!(!result.unwrapped_redirect);

        let before = param_keys(&input);
        for key in param_keys(&result.cleaned) {
            prop_assert!(
                before.contains(&key),
                "parameter {key:?} appeared out of nowhere"
            );
        }
    }

    /// `sanitize(sanitize(u)) == sanitize(u)`. The fixed-point loop is supposed
    /// to give this structurally; this is what catches a catalog update that
    /// introduces a non-idempotent rawRule.
    #[test]
    fn sanitising_is_idempotent(input in url_strategy()) {
        let opts = SanitizeOptions::default();
        let once = sanitize_url(&input, &opts).unwrap();
        let twice = sanitize_url(&once.cleaned, &opts).unwrap();

        prop_assert_eq!(&once.cleaned, &twice.cleaned);
        prop_assert!(!twice.changed, "second pass reported a change");
    }

    /// Whatever arbitrary text is thrown at it, the engine returns a result or
    /// an error — never a panic, which across the FFI boundary would take the
    /// Android process with it.
    #[test]
    fn never_panics_on_arbitrary_input(input in ".{0,300}") {
        let opts = SanitizeOptions::default();
        let _ = sanitize_url(&input, &opts);
    }

    #[test]
    fn never_panics_on_arbitrary_text(input in ".{0,300}") {
        let opts = SanitizeOptions::default();
        let _ = sharewhere_url::sanitize_text(&input, &opts);
    }
}

#[test]
fn oversized_input_is_rejected_rather_than_processed() {
    let opts = SanitizeOptions::default();
    let long = format!("https://example.com/?a={}", "x".repeat(MAX_INPUT_LEN));
    assert!(matches!(
        sanitize_url(&long, &opts),
        Err(sharewhere_url::Error::InputTooLong { .. })
    ));
}

/// Regression, found by `cargo fuzz run sanitize_url`.
///
/// `rawRules` are regexes over the whole URL string. Amazon's `\/ref=[^/?]*`
/// matched a literal `/ref=` inside a *query value* and then ran greedily
/// through every `&` to the end of the query, so five parameters vanished and
/// the key `sp.co.uk/dp/B08N5WRWNW/ref` was spliced down to `sp.co.uk/dp/…`.
///
/// The path rewrite must still happen; only the match inside the query is
/// refused.
#[test]
fn a_raw_rule_may_not_splice_query_parameters() {
    let opts = SanitizeOptions::default();
    let input = "https://www.amazon.co.uk/dp/B08N5WRWNW/ref=sr_0_3\
                 ?crid=2ABCDEF&sp.co.uk/dp/B08N5WRWNW/ref=srefix=usb&tag=someaffiliate-21";
    let result = sanitize_url(input, &opts).unwrap();

    assert!(
        !result.cleaned.contains("/ref=sr_0_3"),
        "the path rewrite stopped working: {}",
        result.cleaned,
    );
    assert!(
        result
            .cleaned
            .contains("sp.co.uk/dp/B08N5WRWNW/ref=srefix=usb"),
        "a query parameter was spliced by a rawRule: {}",
        result.cleaned,
    );
    for key in param_keys(&result.cleaned) {
        assert!(
            param_keys(input).contains(&key),
            "parameter {key:?} appeared out of nowhere",
        );
    }
}

#[test]
fn non_web_schemes_are_rejected() {
    let opts = SanitizeOptions::default();
    for input in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "intent://scan/#Intent;scheme=zxing;end",
        "data:text/html,<script>",
    ] {
        assert!(
            sanitize_url(input, &opts).is_err(),
            "{input} should have been rejected"
        );
    }
}

/// The finding the whole two-tier design rests on: every catalog pattern
/// compiles under the Rust `regex` crate with the feature set we ship. If
/// upstream ever adds a lookahead or backreference, this fails loudly instead
/// of that rule silently disappearing at runtime.
#[test]
fn every_catalog_regex_compiles() {
    let mut failures = Vec::new();

    for catalog in [sharewhere_rules::clearurls(), sharewhere_rules::custom()] {
        for (name, provider) in &catalog.providers {
            let mut patterns = vec![provider.url_pattern.clone()];
            patterns.extend(provider.rules.iter().cloned());
            patterns.extend(provider.raw_rules.iter().cloned());
            patterns.extend(provider.referral_marketing.iter().cloned());
            patterns.extend(provider.exceptions.iter().cloned());
            patterns.extend(provider.redirections.iter().cloned());

            for pattern in patterns {
                if regex::Regex::new(&format!("(?i){pattern}")).is_err() {
                    failures.push(format!("{name}: {pattern}"));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} catalog pattern(s) do not compile under the Rust regex crate:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
