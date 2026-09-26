# ShareBear

**ShareBear cleans tracking junk out of links.** Share any link to it —
from Instagram, Amazon, Google, wherever — the way you'd share to any other
app, and you get the same link back with the tracking stripped, either
copied to your clipboard or ready to share onward. It can also turn a
shared map pin into links for other map apps, offline.

Android app, Rust core, GPL-3.0-or-later.

> **Status:** the Rust core is complete and tested; the Android app builds
> green in CI but hasn't been run on a real phone yet. See
> [Current state](#current-state).

## How it works

1. Share a link from any app (or long-press a URL and pick ShareBear from
   the text-selection menu).
2. Pick **Clean Copy** to strip it and copy the result silently, or
   **Clean Share** to see exactly what got removed before re-sharing.

```
https://www.amazon.co.uk/dp/B08N5WRWNW/ref=sr_1_3?crid=2ABCDEF&keywords=usb+hub&qid=1712345678&tag=someaffiliate-21
→ https://www.amazon.co.uk/dp/B08N5WRWNW
```

A wrapped redirect (Google's `/url?q=…`, a `consent.google.com` wrapper) is
unwrapped and the real destination is cleaned too, all offline. Roughly 200
tracking providers are covered, plus Instagram's `igshid`/`stkn`, Amazon's
`tag`/`/ref=`, and Google Maps, which nothing else covers.

**Locations, as a bonus.** Share a pin from Google Maps, Organic Maps, or a
`geo:` link, and ShareBear hands back the same spot as a `geo:` link, a
clean Google Maps link, three Organic Maps forms, Apple Maps, OpenStreetMap,
a Plus Code, and plain coordinates — so whoever you send it to can open it
in whatever they use.

## Privacy

**One permission — `INTERNET` — and it's unused by default.** Cleaning a
link never touches the network. The only time ShareBear needs to, it's
because a link doesn't carry enough to work with offline (a `maps.app.goo.gl`
short link, or a Maps link that names a place by Google's internal id rather
than a coordinate) — and even then it asks first, naming the exact host,
every time. There's no "always allow"; each request is a one-off. Turn the
offer off in Settings and it never asks and never connects.

You can check this yourself:

```
./gradlew assembleRelease
apkanalyzer manifest permissions app-release.apk
```

Two lines, always: `android.permission.INTERNET`, and an androidx-internal
signature permission neither app nor anyone else can use. CI fails the
build if anything else shows up. No analytics, no crash reporter, no Play
Services, and URLs never reach logcat in release builds.

## How it's built

```
rust/crates/
  sharebear-rules   vendored ClearURLs catalog + our own rules + a build-time index
  sharebear-url     the sanitiser engine        ─┐ pure, no I/O,
  sharebear-geo     location parsers, codecs    ─┘ independently fuzzed
  sharebear-core    policy + the network-resolve state machine
  sharebear-ffi     UniFFI wrappers, nothing else
  sharebear-cli     a dev tool for trying links from the terminal

app/         Compose UI, share targets, settings
core-rust/   cargo-ndk + uniffi-bindgen, JNA
core-net/    OkHttp — the only module allowed to touch the network
```

The rules catalog is vendored and extended rather than pulled in as a
dependency (the crates.io one is stale), and nothing is compiled until a
link actually needs it — compiling the whole catalog eagerly would cost
50–150 ms, too slow for a share-sheet tap. The core itself never opens a
socket: when a request is unavoidable, it says exactly what to fetch and
how to read the answer, and `core-net` is the only thing that moves bytes,
enforcing that policy independently rather than trusting it.

## Getting a build

### [⬇ sharebear-dev.apk](https://github.com/phrag/ShareBear/releases/download/dev-build/sharebear-dev.apk)

That link always serves the newest build — bookmark it, no GitHub login
needed. It's rebuilt on every push to `main` or a `claude/**` branch; see
the [`dev-build` release](https://github.com/phrag/ShareBear/releases/tag/dev-build)
for which commit it's from. Tagging `vX.Y.Z` publishes a versioned release
with notes from [CHANGELOG.md](CHANGELOG.md) instead.

Builds are debug-signed, so they install with no keystore setup but won't
upgrade over a signed release later (that comes with F-Droid, in v1.0).

## Building it yourself

**Rust core**, needs only a Rust toolchain:

```bash
cd rust
cargo test --workspace                                # 82 tests
cargo run -p sharebear-cli -- clean '<url>'            # try one link
cargo run -p sharebear-cli -- corpus testdata/dirty_urls.jsonl
```

**Fuzzing**, needs nightly:

```bash
cargo install cargo-fuzz --locked
rust/fuzz/seed-corpus.sh
cd rust && cargo +nightly fuzz run sanitize_url -- -max_total_time=60
```

Four targets (`sanitize_url`, `sanitize_text`, `parse_location`,
`geo_codecs`) check safety invariants, not just crash-freedom — a
sanitiser that quietly rewrites a link to point somewhere else never
crashes. They run nightly in CI, and briefly on any PR that touches them.

**The app** additionally needs the Android SDK, NDK r27+, and `cargo-ndk`:

```bash
cargo install cargo-ndk --locked
./gradlew assembleDebug
```

Without an Android SDK, the generated UniFFI bindings and the OkHttp
transport still compile and test as a plain JVM project — see the Gradle
modules above for how they're kept separate from Android-only code.

## Current state

| | |
|---|---|
| Rust core | complete — 82 tests, clippy and rustfmt clean |
| Fuzzing | 4 targets, clean over ~4.7M combined executions |
| Android app | builds green in CI, APK published on every push |
| On a real device | **not yet tried** |

Everything above is machine-verified. Nobody has installed the APK and
shared a link into it yet, so whether the share targets actually appear,
the clipboard write lands, and the consent dialog reads sensibly in
practice is still unconfirmed — that's the next thing worth doing.

### Roadmap

- **v0.2** — per-domain toggles, i18n, a manual paste box
- **v1.0** — F-Droid, reproducible builds, accessibility pass
- **v2** — a map image, using offline map tiles rather than fetching them
  (a tile request tells a server exactly where you're looking)

## Contributing

The most useful contribution is a **real link ShareBear handles badly** —
one it fails to clean, or worse, breaks. Add it to
`rust/testdata/dirty_urls.jsonl` with a note, then fix the engine. A case
where the right answer is "change nothing" matters just as much as one
that strips something: a sanitiser that mangles working links is worse
than no sanitiser at all.

## Licence

GPL-3.0-or-later. The bundled ClearURLs catalog is LGPL-3.0 and stays
under that licence — see [NOTICE](NOTICE) for full attribution, including
the Open Location Code and Organic Maps specifications this implements.
