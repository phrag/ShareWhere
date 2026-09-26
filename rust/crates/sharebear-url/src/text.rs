//! Cleaning URLs out of free text.
//!
//! What actually arrives in `Intent.EXTRA_TEXT` is rarely a bare URL.
//! Instagram, TikTok and Reddit all send prose wrapped around the link
//! ("Check this out https://…"), sometimes with more than one URL, and often
//! with a trailing newline. Handling only bare URLs works right up until it
//! meets a real share sheet.

use crate::engine::{sanitize_url, MAX_INPUT_LEN};
use crate::types::{Error, SanitizeOptions, TextResult};

/// Characters that end a URL when scanning free text.
const TERMINATORS: &[char] = &['<', '>', '"', '\'', '`', '\\', '|', '^', '{', '}'];

/// Trailing punctuation that is almost always sentence punctuation rather than
/// part of the link.
const TRAILING_PUNCTUATION: &[char] = &['.', ',', ';', ':', '!', '?', '\u{2026}'];

/// Byte spans of the URLs in `text`.
pub fn find_urls(text: &str) -> Vec<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut cursor = 0usize;

    while cursor < lower.len() {
        let Some(offset) = lower[cursor..].find("http") else {
            break;
        };
        let start = cursor + offset;

        let is_url_start =
            lower[start..].starts_with("http://") || lower[start..].starts_with("https://");
        // Reject a match inside a longer word, e.g. `xhttps://`.
        let at_boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();

        if !is_url_start || !at_boundary {
            cursor = start + 4;
            continue;
        }

        let mut end = text.len();
        for (i, ch) in text[start..].char_indices() {
            if ch.is_whitespace() || TERMINATORS.contains(&ch) {
                end = start + i;
                break;
            }
        }

        end = trim_trailing(text, start, end);
        if end > start {
            spans.push((start, end));
        }
        cursor = end.max(start + 1);
    }

    spans
}

/// Walk back over punctuation that belongs to the sentence, not the URL.
fn trim_trailing(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let slice = &text[start..end];
        let Some(last) = slice.chars().next_back() else {
            break;
        };

        if TRAILING_PUNCTUATION.contains(&last) {
            end -= last.len_utf8();
            continue;
        }

        // A closing bracket is only sentence punctuation if the URL does not
        // open it — Wikipedia links legitimately contain balanced brackets.
        let (open, close) = match last {
            ')' => ('(', ')'),
            ']' => ('[', ']'),
            _ => break,
        };
        let opens = slice.matches(open).count();
        let closes = slice.matches(close).count();
        if closes > opens {
            end -= last.len_utf8();
            continue;
        }
        break;
    }
    end
}

/// Clean every URL in `text`, leaving the surrounding prose untouched.
pub fn sanitize_text(text: &str, opts: &SanitizeOptions) -> Result<TextResult, Error> {
    if text.len() > MAX_INPUT_LEN {
        return Err(Error::InputTooLong {
            len: text.len(),
            max: MAX_INPUT_LEN,
        });
    }

    let spans = find_urls(text);
    let mut cleaned_text = String::with_capacity(text.len());
    let mut urls = Vec::new();
    let mut changed = false;
    let mut cursor = 0usize;

    for (start, end) in spans {
        cleaned_text.push_str(&text[cursor..start]);
        let raw = &text[start..end];

        match sanitize_url(raw, opts) {
            Ok(result) => {
                cleaned_text.push_str(&result.cleaned);
                changed |= result.changed;
                urls.push(result);
            }
            // A URL we cannot parse is left exactly as the user wrote it.
            // Mangling text we did not understand would be worse than doing
            // nothing.
            Err(_) => cleaned_text.push_str(raw),
        }
        cursor = end;
    }
    cleaned_text.push_str(&text[cursor..]);

    Ok(TextResult {
        cleaned_text,
        urls,
        changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<&str> {
        find_urls(text)
            .into_iter()
            .map(|(s, e)| &text[s..e])
            .collect()
    }

    #[test]
    fn finds_a_bare_url() {
        assert_eq!(
            spans("https://example.com/a"),
            vec!["https://example.com/a"]
        );
    }

    #[test]
    fn finds_a_url_wrapped_in_prose() {
        assert_eq!(
            spans("Check this out https://example.com/a it's good"),
            vec!["https://example.com/a"]
        );
    }

    #[test]
    fn strips_sentence_punctuation_but_keeps_balanced_brackets() {
        assert_eq!(
            spans("see https://example.com/a."),
            vec!["https://example.com/a"]
        );
        assert_eq!(
            spans("see https://en.wikipedia.org/wiki/Foo_(bar)"),
            vec!["https://en.wikipedia.org/wiki/Foo_(bar)"]
        );
        assert_eq!(
            spans("see (https://example.com/a)"),
            vec!["https://example.com/a"]
        );
    }

    #[test]
    fn finds_multiple_urls() {
        assert_eq!(
            spans("one https://a.example/x two http://b.example/y"),
            vec!["https://a.example/x", "http://b.example/y"]
        );
    }

    #[test]
    fn ignores_a_match_inside_a_word() {
        assert!(spans("xhttps://example.com").is_empty());
    }

    #[test]
    fn preserves_surrounding_text() {
        let opts = SanitizeOptions::default();
        let out = sanitize_text("Look: https://example.com/a?utm_source=x — nice", &opts).unwrap();
        assert_eq!(out.cleaned_text, "Look: https://example.com/a — nice");
        assert!(out.changed);
    }

    #[test]
    fn text_without_urls_is_untouched() {
        let opts = SanitizeOptions::default();
        let out = sanitize_text("no links here", &opts).unwrap();
        assert_eq!(out.cleaned_text, "no links here");
        assert!(!out.changed);
    }
}
