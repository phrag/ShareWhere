# ShareWhere

Share a link to ShareWhere and it comes back without the tracking. Share a
location and you get every map format at once.

Android app, Rust core, GPL-3.0-or-later.

> **Status: the Rust core is complete and tested. The Android layer builds in
> CI.** See [Current state](#current-state) and
> [Getting a build](#getting-a-build).

## What it does

**Clean a link.** Pick ShareWhere from the share sheet and the tracking comes
off — Instagram's `igshid`, Amazon's `/ref=` path segment and affiliate `tag`,
`utm_*`, `fbclid`, and about 200 other providers' worth. Two share targets:
**Clean Copy** finishes without showing a screen, with a brief popup naming what
it removed; **Clean Share** shows the full list first, then re-shares.

```
https://www.amazon.co.uk/dp/B08N5WRWNW/ref=sr_1_3?crid=2ABCDEF&keywords=usb+hub&qid=1712345678&tag=someaffiliate-21
→ https://www.amazon.co.uk/dp/B08N5WRWNW
```

Wrapped redirects are unwrapped *and* the destination is cleaned too:

```
https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fpage%3Futm_source%3Dnews&sa=D&ved=2ahUKEwi
→ https://example.com/page
```

**Share a location everywhere at once.** Share from Google Maps, Organic Maps, a
`geo:` link or plain coordinates, and get back all eleven formats: a `geo:` link
any maps app can open, a tracker-free Google Maps link, three Organic Maps forms
including the compact `ge0` short link, Apple Maps, OpenStreetMap, a Plus Code,
and the coordinates in decimal and DMS.

## What it doesn't do

**No what3words.** Converting an address needs their API, which means an API key
and handing them the coordinates, your IP address and a timestamp on every
lookup. Plus Codes do the same job entirely offline, so that is what ShareWhere
uses. A `///word.word.word` is recognised and explained rather than silently
failing.

**No silent network access.** One APK, one permission: `INTERNET`. Nothing
reaches the network until you tap through a dialog naming the exact host.

Being precise about the mechanism, because it is not what people assume:
`INTERNET` is a *normal* Android permission, granted at install time, and the
platform provides **no way to request it at runtime**. No app can put a system
permission dialog in front of you for it. So ShareWhere gates itself instead —
the Rust core refuses to emit a request until `allowNetwork` is set, and that is
only ever set for a single resolve, from the consent dialog. There is no "always
allow".

What you can still verify mechanically is that nothing *else* crept in:

```
./gradlew assembleRelease
apkanalyzer manifest permissions app-release.apk
```

Two lines: `android.permission.INTERNET`, and
`app.sharewhere.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` — the latter defined
by androidx under our own application id at `signature` level, so only
ShareWhere can hold it, guarding a receiver androidx registers internally. CI
fails the build if anything beyond those two appears.

Google Maps share links (`maps.app.goo.gl/…`) carry no coordinates at all, so
they genuinely cannot be resolved offline. ShareWhere offers to follow one, and
asks first — naming the host — every time. You can turn the offer off entirely
in Settings, in which case it never asks and never connects.

**No analytics, no crash reporter, no Play Services.** URLs never reach logcat
in release builds.

## How it is put together

```
rust/crates/
  sharewhere-rules   vendored ClearURLs catalog + our own layer + build-time index
  sharewhere-url     the sanitiser engine          ─┐ pure, no I/O,
  sharewhere-geo     parsers, Plus Codes, ge0      ─┘ independently fuzzed
  sharewhere-core    policy + the resolve state machine
  sharewhere-ffi     UniFFI wrappers, nothing else
  sharewhere-cli     development tool

app/         Compose UI, share targets, settings
core-rust/   cargo-ndk + uniffi-bindgen, JNA
core-net/    OkHttp transport — the only module that can reach the network
```

Three decisions worth knowing:

**The rules are vendored, and the engine is ours.** The `clearurls` crate on
crates.io is stale at 0.0.4 (September 2024) and would bake in two-year-old
rules. Vendoring the catalog also gets us the removed-parameter report the
preview needs, and lets us cover Google Maps, which ClearURLs does not touch at
all. All 1095 of its regexes were checked for lookaheads and backreferences —
there are none — so they compile as-is under the Rust `regex` crate.

**Nothing is compiled eagerly.** Compiling the whole catalog costs 50–150 ms,
far too slow for a share-sheet activity. A build-time index extracts the first
literal domain label from each provider's pattern (199 of 206 reduce cleanly),
so a URL compiles only the one or two providers that could match it.

**The core never opens a socket.** When a request is genuinely unavoidable, the
core says what to fetch and how to read the answer; `core-net` moves the bytes.
Each request carries its own policy — allowed hosts, no redirect following, no
cookies, a 64 KB cap — and both layers enforce it, because the redirect target
is chosen by whoever controls the link.

## Getting a build

### [⬇ sharewhere-dev.apk](https://github.com/phrag/ShareWhere/releases/download/dev-build/sharewhere-dev.apk)

That link always serves the newest build — bookmark it. Every push to `main` or
a `claude/**` branch replaces it, and it needs no GitHub login, so it opens
straight from a phone.

The [`dev-build` pre-release](https://github.com/phrag/ShareWhere/releases/tag/dev-build)
page shows which branch and commit it came from. Each run also uploads a
`sharewhere-apk-<sha>` workflow artifact, with the size and full SHA-256 printed
in the run summary, if you want a specific commit rather than the latest.

Tagging `v0.2.0` (or any `x.y.z`) publishes a versioned release instead, with
notes taken from the matching section of [CHANGELOG.md](CHANGELOG.md).

Builds are **debug-signed**, so they install without keystore setup but will not
upgrade over a release-signed build later. Signed release builds come with the
F-Droid work in v1.0. The Rust core inside them *is* built with the release
profile (`-PrustRelease`) — a debug core ships unstripped for three ABIs and
turns a small app into a ~95 MB download.

## Building

**The Rust core** needs nothing but a Rust toolchain:

```bash
cd rust
cargo test --workspace                                  # 78 tests
cargo run -p sharewhere-cli -- clean '<url>'            # try one link
cargo run -p sharewhere-cli -- corpus testdata/dirty_urls.jsonl
```

**Fuzzing** needs nightly, since `cargo fuzz` builds with `-Zsanitizer=address`:

```bash
cargo install cargo-fuzz --locked
rust/fuzz/seed-corpus.sh              # seeds generated from the golden corpus
cd rust
cargo +nightly fuzz run sanitize_url -- -max_total_time=60
```

Targets: `sanitize_url`, `sanitize_text`, `parse_location`, `geo_codecs`. They
assert the safety invariants, not just panic-freedom — a sanitiser that quietly
rewrites a link to point somewhere else never crashes. `.github/workflows/fuzz.yml`
runs all four nightly and for two minutes each on a pull request that touches
them.

**The app** additionally needs the Android SDK, NDK r27+ (for 16 KB page
alignment, mandatory on Android 15+) and `cargo-ndk`:

```bash
cargo install cargo-ndk --locked
./gradlew assembleDebug
```

**Without an Android SDK**, a useful amount is still checkable locally. The
generated UniFFI bindings and `core-net`'s `ResolveCoordinator` use no Android
APIs at all — only JNA, OkHttp, coroutines and the JDK — so they compile in a
plain JVM Gradle project.

Mirror the real module boundaries when you do this, with OkHttp as an
`implementation` dependency of the `net` module and absent from `app`. A single
flat module with every dependency on the classpath will compile code that then
fails in the real build: that is exactly how an `OkHttpClient` default argument
leaked into `ResolveCoordinator`'s public signature and broke the app module.

## Current state

| | |
|---|---|
| Rust core | complete — 78 tests, clippy and rustfmt clean |
| Fuzzing | four targets, clean over a combined ~4.7 M executions |
| Plus Codes | matches all 302 upstream reference vectors exactly |
| `ge0` codec | matches Organic Maps' own test vectors |
| UniFFI bindings | generate and compile against JNA on the JVM |
| Android app | builds green in CI, APK published per run |
| On a real device | **not yet tried** |

Everything above is verified by machine. What nobody has done yet is install the
APK and share a link into it, so the end-to-end behaviour — do both share
targets appear, does the clipboard write land, does the consent dialog read
sensibly — is still unconfirmed. That is the next thing worth doing, and the
most likely place for a surprise.

### Known gaps

- **Nothing is verified on a device.** Settings, the consent dialog and the
  removal popup all compile and pass CI, but none of them has been seen
  working.
- **Cleaning free text is not idempotent once a redirect is unwrapped.** The
  unwrapped target is percent-decoded out of the wrapper, so it can contain
  characters — `>`, for one — that the URL scanner treats as ending a URL, and
  the spliced text then tokenises differently. Cleaning always *settles*, and
  the fuzzer asserts that, but it can take two rounds. A single URL is
  unaffected. Fixing it means re-encoding unwrapped targets through the `Url`
  serialiser, which mangles links that were fine, so it is not obviously worth
  doing.
- **Redirect unwrapping is capped** at five hops, so a wrapper nested deeper
  comes back partly wrapped. Deliberate — the cap is what stops a crafted link
  costing unbounded work.

### Roadmap

- ~~**v0.3** — wire the consent prompt to `core-net`~~ — done. Tapping "Resolve
  this one link" now drives the Rust resolve session through OkHttp and replaces
  the prompt with the location behind the short link. Untried on a device.
- **v0.2** — per-domain toggles, i18n, a manual paste box
- **v1.0** — F-Droid, reproducible builds, accessibility pass
- **v2** — a map image; offline PMTiles regions preferred over fetching tiles,
  since a tile request tells a server exactly where you are looking

## Contributing

The most useful contribution is a **real dirty URL that ShareWhere handles
badly** — either one it fails to clean, or worse, one it breaks. Add it to
`rust/testdata/dirty_urls.jsonl` with a note, then fix the engine. Cases where
the right answer is "change nothing" matter as much as the ones that strip
something: a sanitiser that mangles working links is worse than no sanitiser.

## Licence

GPL-3.0-or-later. The bundled ClearURLs rule catalog is LGPL-3.0 and remains
available under that licence — see [NOTICE](NOTICE) for full attribution,
including the Open Location Code and Organic Maps specifications this
implements.
