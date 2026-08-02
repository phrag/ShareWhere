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
*Clean & copy* finishes without showing a screen, *Clean & share…* shows exactly
what was removed first.

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

**No silent network access.** The default build declares **no permissions at
all** — not even `INTERNET`. That is verified in CI against the merged manifest,
so it is checkable rather than promised:

```
./gradlew assembleOfflineRelease
apkanalyzer manifest permissions app-offline-release.apk   # prints nothing
```

Google Maps share links (`maps.app.goo.gl/…`) carry no coordinates at all, so
they genuinely cannot be resolved offline. The `standard` build can follow one,
but only after you tap a prompt naming the host it will contact. There is no
"always allow" setting.

**No analytics, no crash reporter, no Play Services.** URLs never reach logcat
in release builds.

## How it is put together

```
rust/crates/
  sharewhere-rules   vendored ClearURLs catalog + our own layer + build-time index
  sharewhere-url     the sanitiser engine          ─┐ pure, no I/O,
  sharewhere-geo     parsers, Plus Codes, ge0      ─┘ independently fuzzable
  sharewhere-core    policy + the resolve state machine
  sharewhere-ffi     UniFFI wrappers, nothing else
  sharewhere-cli     development tool

app/         Compose UI, share targets
core-rust/   cargo-ndk + uniffi-bindgen, JNA
core-net/    OkHttp — linked by the `standard` flavor only
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

Every push builds both flavors and attaches them to the run. To grab one:

1. Open the [Actions tab](https://github.com/phrag/ShareWhere/actions/workflows/android.yml)
   and pick the most recent green run — or trigger one yourself with **Run
   workflow**.
2. Download the `sharewhere-apks-<sha>` artifact at the bottom of the run page.
3. Unzip and install `app-offline-debug.apk`.

The run summary lists each APK's size and SHA-256 so you can check what you got.

**Install `offline` unless you specifically want short-link expansion.** It is
the build with no permissions at all. `standard` adds `INTERNET`, used only
behind the per-link consent prompt.

These are **debug-signed**, so they install without any keystore setup, but they
will not upgrade over a release-signed build later and are not suitable for
distribution. Signed release builds come with the F-Droid work in v1.0.

## Building

**The Rust core** needs nothing but a Rust toolchain:

```bash
cd rust
cargo test --workspace                                  # 75 tests
cargo run -p sharewhere-cli -- clean '<url>'            # try one link
cargo run -p sharewhere-cli -- corpus testdata/dirty_urls.jsonl
```

**The app** additionally needs the Android SDK, NDK r27+ (for 16 KB page
alignment, mandatory on Android 15+) and `cargo-ndk`:

```bash
cargo install cargo-ndk --locked
./gradlew assembleOfflineDebug
```

## Current state

| | |
|---|---|
| Rust core | complete — 75 tests, clippy and rustfmt clean |
| Plus Codes | matches all 302 upstream reference vectors exactly |
| `ge0` codec | matches Organic Maps' own test vectors |
| UniFFI bindings | verified to generate from the built library |
| Android app | **written, not yet compiled** — needs an SDK |

The Android half was authored without an SDK available, so expect the usual
first-build friction: a missing launcher icon, Gradle wrapper, and whatever the
compiler finds. Nothing there is load-bearing on the logic, which is all in Rust
and all tested.

### Roadmap

- **v0.2** — settings, per-domain toggles, precision blur in the UI, i18n
- **v0.3** — wire the consent prompt to `core-net` in the `standard` flavor
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
