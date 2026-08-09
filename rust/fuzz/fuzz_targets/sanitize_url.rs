//! Fuzzes the sanitiser over arbitrary input, asserting the safety invariants
//! rather than only panic-freedom.
//!
//! Panic-freedom alone matters — a panic crosses the FFI boundary as an abort
//! and takes the Android process down — but it is the weaker property. The one
//! that could actually hurt someone is the engine quietly rewriting a link to
//! point somewhere else, and that is a *silent* failure a crash-only fuzzer
//! would never find.
//!
//! There is a proptest covering the same ground, but its generator only ever
//! emits well-formed URLs from a fixed host list. This sees genuinely arbitrary
//! bytes: unbalanced percent-escapes, embedded NULs, `?` and `#` in every
//! order, non-ASCII hosts, nested wrappers.

#![no_main]

use libfuzzer_sys::fuzz_target;
use sharewhere_url::{sanitize_url, SanitizeOptions};
use url::Url;

/// Parameter keys as they literally appear. Deliberately not routed through a
/// URL parser: comparing normalised forms would hide exactly the re-encoding
/// bugs worth finding.
///
/// The one normalisation applied is `trim`, because the engine trims its result
/// — a URL is never meant to carry bare leading or trailing whitespace — and
/// that can shave a space off the last key. Whitespace is all this hides: the
/// splice this assertion exists to catch turned `sp.co.uk/dp/X/ref` into
/// `sp.co.uk/dp/X`, which trimming does not make equal.
fn param_keys(url: &str) -> Vec<&str> {
    let before_fragment = url.split('#').next().unwrap_or_default();
    let Some((_, query)) = before_fragment.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|s| s.split_once('=').map_or(s, |(k, _)| k).trim())
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    let opts = SanitizeOptions::default();
    let Ok(once) = sanitize_url(input, &opts) else {
        // Rejecting an input is a correct outcome, not a finding.
        return;
    };

    // Compare against `once.original`, not `input`: the engine trims its input
    // before doing anything, so `input` still carries whitespace the result
    // never had. Comparing raw text against the untrimmed form reports a
    // trailing `\r` as a renamed parameter — a finding in the harness rather
    // than in the engine.
    let input = once.original.as_str();

    // The output must still be a URL. If cleaning produced something
    // unparseable, the app is about to hand garbage to `startActivity`.
    let after = Url::parse(&once.cleaned).expect("cleaned output must still parse as a URL");

    // SECURITY: the scheme is the difference between opening a web page and
    // launching an intent. It may never be widened by cleaning.
    assert!(
        matches!(after.scheme(), "http" | "https"),
        "cleaned {:?} into non-web scheme {:?}",
        input,
        after.scheme(),
    );

    // Both of the following are suspended when a redirect was unwrapped, and
    // only then: the result is a different URL by design, and an `https`
    // wrapper around an `http` destination legitimately changes both scheme
    // and host. `unwrapped_redirect` is the flag that makes that visible in
    // the preview, which is why every other case must hold it false.
    if !once.unwrapped_redirect {
        if let Ok(before) = Url::parse(input) {
            assert_eq!(
                before.scheme(),
                after.scheme(),
                "scheme changed cleaning {input:?} with no redirect unwrapped",
            );

            // The single most important invariant: a host change here means
            // the user was silently sent somewhere they did not ask to go.
            assert_eq!(
                before.host_str(),
                after.host_str(),
                "host changed cleaning {input:?} with no redirect unwrapped",
            );
        }
    }

    // Cleaning is subtractive. No rule may invent a parameter.
    if !once.unwrapped_redirect {
        let before = param_keys(input);
        for key in param_keys(&once.cleaned) {
            assert!(
                before.contains(&key),
                "parameter {key:?} appeared out of nowhere cleaning {input:?}",
            );
        }
    }

    // Idempotence. The fixed-point loop is supposed to give this structurally;
    // this is what catches a catalog update introducing a non-idempotent
    // rawRule, which would make the preview disagree with the copied result.
    //
    // Not asserted once a redirect has been unwrapped, and this is a real
    // limit rather than an oversight: `max_hops` deliberately caps how far a
    // chain of `google.com/url?q=…` wrappers is followed, so a URL nested
    // deeper than the cap comes back still wrapped — and a second call, with a
    // fresh budget, correctly unwraps further. The cap is a DoS control we want
    // to keep, so the two properties genuinely cannot both hold.
    if !once.unwrapped_redirect {
        let twice = sanitize_url(&once.cleaned, &opts).expect("cleaned output must re-sanitize");
        assert_eq!(
            once.cleaned, twice.cleaned,
            "not idempotent: {input:?} cleaned to {:?}, which cleaned again to {:?}",
            once.cleaned, twice.cleaned,
        );
    }
});
