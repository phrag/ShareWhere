//! Development tool. Not shipped in the app.
//!
//!   cargo run -p sharewhere-cli -- clean '<url>'
//!   cargo run -p sharewhere-cli -- text '<free text with links>'
//!   cargo run -p sharewhere-cli -- corpus [path]

use std::process::ExitCode;

use sharewhere_url::{sanitize_text, sanitize_url, SanitizeOptions};

const USAGE: &str = "\
usage:
  sharewhere-cli clean  <url>       clean one URL and show what was removed
  sharewhere-cli text   <text>      clean every URL inside free text
  sharewhere-cli corpus [path]      run the golden corpus (default: testdata/dirty_urls.jsonl)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = SanitizeOptions::default();

    match args.first().map(String::as_str) {
        Some("clean") => {
            let Some(input) = args.get(1) else {
                eprint!("{USAGE}");
                return ExitCode::FAILURE;
            };
            match sanitize_url(input, &opts) {
                Ok(result) => {
                    println!("in   {}", result.original);
                    println!("out  {}", result.cleaned);
                    println!("changed: {}", result.changed);
                    if result.complete_provider {
                        println!("WARNING: this URL is entirely a tracker");
                    }
                    if result.unwrapped_redirect {
                        println!("note: unwrapped a redirect, so the host changed");
                    }
                    if !result.providers.is_empty() {
                        println!("providers: {}", result.providers.join(", "));
                    }
                    for removed in &result.removed {
                        println!("  - [{}] {} = {}", removed.kind, removed.key, removed.value);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Some("text") => {
            let Some(input) = args.get(1) else {
                eprint!("{USAGE}");
                return ExitCode::FAILURE;
            };
            match sanitize_text(input, &opts) {
                Ok(result) => {
                    println!("{}", result.cleaned_text);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Some("corpus") => {
            let path = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| "testdata/dirty_urls.jsonl".to_string());
            run_corpus(&path)
        }

        _ => {
            eprint!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run_corpus(path: &str) -> ExitCode {
    let Ok(contents) = std::fs::read_to_string(path) else {
        eprintln!("error: cannot read {path}");
        return ExitCode::FAILURE;
    };

    let opts = SanitizeOptions::default();
    let mut passed = 0usize;
    let mut failed = 0usize;

    for (n, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let case: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("line {}: bad JSON: {e}", n + 1);
                failed += 1;
                continue;
            }
        };
        let input = case["input"].as_str().unwrap_or_default();
        let expected = case["expected"].as_str().unwrap_or_default();
        let note = case["note"].as_str().unwrap_or_default();

        match sanitize_url(input, &opts) {
            Ok(result) if result.cleaned == expected => passed += 1,
            Ok(result) => {
                failed += 1;
                println!("FAIL line {} ({note})", n + 1);
                println!("  in       {input}");
                println!("  expected {expected}");
                println!("  actual   {}", result.cleaned);
            }
            Err(e) => {
                failed += 1;
                println!("FAIL line {} ({note}): {e}", n + 1);
            }
        }
    }

    println!("\n{passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
