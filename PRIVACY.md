# Privacy

## The short version

ShareWhere collects nothing and stores nothing but your settings. It asks
Android for exactly one capability — internet — and does not use it until you
tap through a dialog naming the host it would contact. No location, no storage,
no contacts. You can check that yourself without trusting this document.

## What leaves your device

**By default: nothing.** ShareWhere holds `INTERNET`, but the Rust core will
not emit a request until `allowNetwork` is set, and that is only ever set for a
single resolve, from the consent dialog.

Worth being precise, since it is a real limitation rather than a design choice:
`INTERNET` is a *normal* Android permission, granted at install time. The
platform offers **no runtime prompt** for it, so no app can make the system ask
you. ShareWhere's dialog is an in-app gate, not a system permission dialog. If
you would rather not rely on that, turn "Offer to resolve short links" off in
Settings and it will never ask or connect.

**When you do tap: one request.** Some links
genuinely cannot be resolved offline — `maps.app.goo.gl/…`, which is what Google
Maps puts on the clipboard, carries no coordinates whatsoever. When you share
one, ShareWhere stops and shows you the host it would contact. Nothing happens
until you tap.

That request:

- goes to https only, to a host from a list the core fixed in advance
- sends no cookies and stores none
- sends a fixed, unremarkable `User-Agent`, so the request is not a fingerprint
- does not follow redirects on its own; each hop is re-checked and counted
- reads at most 64 KB and times out after 8 seconds
- is refused if the host resolves to a private, loopback or link-local address

There is no "always allow" setting. Consent is per link, every time.

## What is stored

Your settings, in app-private storage: whether referral parameters are removed,
coordinate precision, and which link handlers you have enabled. Nothing is
included in cloud backup or device transfer.

No history of what you have shared is kept. There is nowhere for it to go.

## The clipboard

ShareWhere **never reads your clipboard**. There is no call to `getPrimaryClip`
anywhere in the app. Text arrives because you shared it or pasted it.

Locations copied to the clipboard are marked sensitive, so the system preview on
Android 13+ masks them rather than displaying your coordinates on screen.

## Logging

Release builds compile URL logging out entirely. A shared link never reaches
logcat, where any app with the right permission could read it.

## Verifying this yourself

```bash
./gradlew assembleRelease
apkanalyzer manifest permissions app/build/outputs/apk/release/*.apk
```

That prints two lines:

```
android.permission.INTERNET
app.sharewhere.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
```

The second is not a capability. androidx defines it under ShareWhere's own
application id at `signature` protection level, so no other app can ever be
granted it, and it exists only to stop other apps reaching a broadcast receiver
androidx registers internally on pre-Android-13 devices.

Location, storage and contacts are absent, and CI fails the build if anything
beyond those two lines appears — so a dependency cannot quietly introduce one.

Two earlier versions of this document were wrong and are worth naming. The first
said the app declares no permissions at all; the merged manifest disproved it.
The second described an `offline` build with no `INTERNET` permission, which was
true at the time but no longer exists — there is one APK now.

The request policy is in
`rust/crates/sharewhere-core/src/session.rs` and its enforcement is in
`core-net/src/main/kotlin/app/sharewhere/net/ResolveCoordinator.kt`. Both are
short and deliberately boring to read.

## What ShareWhere does not protect you from

Worth stating plainly, because a privacy tool that oversells itself is worse
than one that does not exist.

**The person you send the link to.** Stripping a tracking parameter stops
*third parties* correlating the click back to you. It does not hide the
destination from the recipient — that is the point of sending it.

**A location is a location.** A map link you share, in any format, tells the
recipient exactly where you mean. If that is more than you intended, use the
precision setting to blur it, or drop the place name: "Home" often says more
than the coordinates do.

**Your network.** ShareWhere does not tunnel anything. Your ISP sees the same
traffic it always did.

**Links that are entirely a tracker.** Some URLs have no destination to clean
out of them; the whole thing exists to record that you followed it. ShareWhere
flags these rather than pretending it cleaned something.

## Third-party data

The bundled ClearURLs rule catalog is data, not a service — it ships inside the
app and is never fetched at runtime in this version. Updating it is a code
change that goes through review.

## Contact

Security issues and privacy concerns: open an issue at
https://github.com/phrag/ShareWhere/issues, or use GitHub's private
vulnerability reporting for anything you would rather not disclose publicly.
