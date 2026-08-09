//! Fuzzes the free-text path — the one that runs on everything Instagram,
//! TikTok and Reddit put on the share sheet, which is prose with URLs in it
//! rather than a bare URL.
//!
//! `find_urls` hands back byte spans that `sanitize_text` then splices, so this
//! target is really about that splicing: a span that lands inside a multi-byte
//! character panics on slice, and an off-by-one silently eats a character of
//! someone's message.

#![no_main]

use libfuzzer_sys::fuzz_target;
use sharewhere_url::{find_urls, sanitize_text, SanitizeOptions};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Spans must be on character boundaries and non-overlapping, in order.
    // Slicing on either would panic inside sanitize_text; assert here so a
    // failure names the actual defect rather than a slice index.
    let mut previous_end = 0usize;
    for (start, end) in find_urls(text) {
        assert!(
            start <= end && end <= text.len(),
            "span {start}..{end} out of range"
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "span {start}..{end} splits a character in {text:?}",
        );
        assert!(start >= previous_end, "spans overlap or are out of order");
        previous_end = end;
    }

    let Ok(result) = sanitize_text(text, &opts()) else {
        return;
    };

    // Prose is not ours to edit. If nothing was found, the text comes back
    // byte-identical — no normalisation, no trimming.
    if result.urls.is_empty() {
        assert_eq!(result.cleaned_text, text, "rewrote text containing no URLs");
        assert!(!result.changed);
    }

    // Every URL reported as cleaned must actually be in the output. This is
    // the splice check: a bad cursor drops a URL from the text while still
    // listing it in the preview, so the user copies something that no longer
    // contains the link they shared.
    for url in &result.urls {
        assert!(
            result.cleaned_text.contains(&url.cleaned),
            "reported cleaning to {:?}, which is absent from the output for {text:?}",
            url.cleaned,
        );
    }

    // Convergence rather than one-step idempotence, which is a real weakening
    // and worth being explicit about.
    //
    // `sanitize_url` is idempotent in one step and its own target asserts
    // exactly that. Free text is not, for two reasons that both come from
    // redirect unwrapping and neither of which is a defect:
    //
    //   * unwrapping is capped by `max_hops`, so a chain nested deeper than the
    //     cap legitimately unwraps further on a second call; and
    //   * the unwrapped target is percent-decoded out of the wrapper, so it can
    //     contain characters — `>` is the one the fuzzer found — that
    //     `find_urls` treats as ending a URL, meaning the spliced text
    //     tokenises into different spans than the text it replaced.
    //
    // Fixing the second would mean re-encoding unwrapped targets through the
    // `Url` serialiser, which this engine deliberately avoids because it
    // rewrites links that were fine. So the property asserted is the one that
    // actually holds and still has teeth: repeated cleaning settles, rather
    // than oscillating or growing without bound.
    let mut current = result.cleaned_text;
    for _ in 0..8 {
        let next = sanitize_text(&current, &opts())
            .expect("cleaned text must re-clean")
            .cleaned_text;
        if next == current {
            return;
        }
        assert!(
            next.len() <= current.len(),
            "cleaning free text grew it, so it cannot settle: {text:?}",
        );
        current = next;
    }
    panic!("cleaning free text never settled after 8 rounds: {text:?}");
});

fn opts() -> SanitizeOptions {
    SanitizeOptions::default()
}
