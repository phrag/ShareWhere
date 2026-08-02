//! Runs the golden corpus as part of `cargo test`.
//!
//! The corpus lives at `rust/testdata/dirty_urls.jsonl` so the CLI and the test
//! suite share one file — a case added while debugging with the CLI is a case
//! CI enforces from then on.

use sharewhere_url::{sanitize_url, SanitizeOptions};

#[derive(serde::Deserialize)]
struct Case {
    input: String,
    expected: String,
    #[serde(default)]
    note: String,
}

fn corpus() -> Vec<Case> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/dirty_urls.jsonl"
    );
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read corpus at {path}: {e}"));

    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| panic!("bad corpus line {line}: {e}"))
        })
        .collect()
}

#[test]
fn golden_corpus_matches_exactly() {
    let opts = SanitizeOptions::default();
    let mut failures = Vec::new();

    for case in corpus() {
        match sanitize_url(&case.input, &opts) {
            Ok(result) if result.cleaned == case.expected => {}
            Ok(result) => failures.push(format!(
                "{}\n  in       {}\n  expected {}\n  actual   {}",
                case.note, case.input, case.expected, result.cleaned
            )),
            Err(e) => failures.push(format!("{}\n  in {}\n  error: {e}", case.note, case.input)),
        }
    }

    assert!(
        failures.is_empty(),
        "{} corpus case(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Cleaning an already-clean URL must be a no-op. Without this the app would
/// report "cleaned!" on links it did not touch, and users would stop believing
/// it.
#[test]
fn corpus_outputs_are_stable_under_a_second_pass() {
    let opts = SanitizeOptions::default();

    for case in corpus() {
        let once = sanitize_url(&case.expected, &opts).expect("expected output must parse");
        assert_eq!(
            once.cleaned, case.expected,
            "second pass changed a corpus output ({})",
            case.note
        );
    }
}

#[test]
fn corpus_is_not_trivially_small() {
    // Guards against the file being emptied or the path silently breaking.
    assert!(corpus().len() >= 30, "corpus has shrunk unexpectedly");
}
