# Changelog

Release notes are pulled from this file by the tag build — the section under
`## <version>` becomes the GitHub release body, so keep the headings exactly in
that form.

## Unreleased

Nothing has been tagged yet. The `dev-build` pre-release always carries the
newest build of the branch:
<https://github.com/phrag/ShareWhere/releases/download/dev-build/sharewhere-dev.apk>

### Added

- Link cleaning from the share sheet, over the vendored ClearURLs catalog plus
  ShareWhere's own rules — including Google Maps, which ClearURLs does not cover.
  Two entries: **Clean Copy** finishes without a screen and briefly says what it
  removed; **Clean Share** lists everything first, then re-shares.
- Clean-in-place via `ACTION_PROCESS_TEXT`: highlight a URL anywhere in the OS
  and replace it with the cleaned version.
- Location sharing in eleven formats at once — `geo:`, a tracker-free Google
  Maps link, three Organic Maps forms including the compact `ge0` short link,
  Apple Maps, OpenStreetMap, Plus Code, decimal degrees and DMS.
- Settings: affiliate-tag removal, redirect unwrapping, whether to offer
  short-link resolution at all, coordinate precision (exact / ~100 m / ~1 km),
  place-name inclusion, always-preview, and the `geo:`/`om:` link handlers.
- A consent dialog before any network request, naming the host it will contact.

### Notes

- Plus Codes match all 302 upstream reference vectors exactly; the `ge0` codec
  matches Organic Maps' own test vectors.
- what3words is deliberately absent: converting an address needs their API, a
  key, and handing them the coordinates, an IP address and a timestamp. Plus
  Codes do the same job offline.
- Nothing in this changelog has been exercised on a physical device yet.
