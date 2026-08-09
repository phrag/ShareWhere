#!/usr/bin/env bash
#
# Seed the fuzz corpora from the golden test data.
#
# Generated rather than committed: the inputs already live in
# `testdata/dirty_urls.jsonl`, and having one copy means a case added there is
# automatically a fuzz seed too, instead of the two drifting apart.
#
# Safe to re-run. libFuzzer will add its own findings alongside these.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
testdata="$here/../testdata"

mkdir -p "$here/corpus/sanitize_url" "$here/corpus/sanitize_text" \
         "$here/corpus/parse_location"

# --- sanitize_url: every input and expected output from the golden corpus ----
#
# The expected outputs matter as much as the inputs: they are what the
# idempotence assertion re-feeds, so seeding them puts the fuzzer straight onto
# that path.
python3 - "$testdata/dirty_urls.jsonl" "$here/corpus/sanitize_url" <<'PY'
import hashlib, json, pathlib, sys

source, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
written = 0
for line in source.read_text().splitlines():
    line = line.strip()
    if not line or line.startswith("//"):
        continue
    case = json.loads(line)
    for key in ("input", "expected"):
        value = case.get(key)
        if not value:
            continue
        name = hashlib.sha1(value.encode()).hexdigest()[:16]
        (out / name).write_text(value)
        written += 1
print(f"sanitize_url: {written} seeds")
PY

# --- sanitize_text: the same URLs wrapped in the prose apps actually send ----
python3 - "$testdata/dirty_urls.jsonl" "$here/corpus/sanitize_text" <<'PY'
import hashlib, json, pathlib, sys

source, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
# Shapes taken from what Instagram, TikTok and Reddit put on the share sheet.
templates = [
    "{url}",
    "Check this out {url}",
    "{url} and also {url}",
    "look\n{url}\n-- sent from a phone",
]
written = 0
for line in source.read_text().splitlines():
    line = line.strip()
    if not line or line.startswith("//"):
        continue
    url = json.loads(line).get("input")
    if not url:
        continue
    for template in templates:
        text = template.format(url=url)
        name = hashlib.sha1(text.encode()).hexdigest()[:16]
        (out / name).write_text(text)
        written += 1
print(f"sanitize_text: {written} seeds")
PY

# --- parse_location: one seed per format the parser claims to handle ---------
#
# Hand-written, because the URL corpus is about tracking parameters and carries
# very few coordinates. Reaching the interesting code needs an input that at
# least resembles a location.
seed() { printf '%s' "$2" > "$here/corpus/parse_location/$1"; }
seed geo                'geo:51.5007,-0.1246'
seed geo_q              'geo:0,0?q=51.5007,-0.1246(Big%20Ben)'
seed gmaps_at           'https://www.google.com/maps/@51.5007,-0.1246,17z'
seed gmaps_place        'https://www.google.com/maps/place/Big+Ben/data=!3m1!4b1!4m5!3d51.5007!4d-0.1246'
seed gmaps_query        'https://www.google.com/maps/search/?api=1&query=51.5007,-0.1246'
seed gmaps_short        'https://maps.app.goo.gl/AbCdEfGhIjKlMnOp'
seed om_scheme          'om://map?v=1&ll=51.5007,-0.1246&n=Big%20Ben'
seed omaps_clear        'https://omaps.app/51.5007,-0.1246/Big_Ben'
seed omaps_ge0          'https://omaps.app/8wAAAAAAAA/Big_Ben'
seed apple              'https://maps.apple.com/?ll=51.5007,-0.1246'
seed osm_hash           'https://www.openstreetmap.org/#map=17/51.5007/-0.1246'
seed osm_marker         'https://www.openstreetmap.org/?mlat=51.5007&mlon=-0.1246'
seed pluscode           '9C3XGV4C+2X'
seed pluscode_url       'https://plus.codes/9C3XGV4C+2X'
seed decimal            '51.5007, -0.1246'
seed dms                '51°30'\''02.5"N 0°07'\''28.6"W'
seed what3words         '///filled.count.soap'
seed poles              'geo:90,-180'
seed antimeridian       'geo:0.0,179.9999999'
echo "parse_location: $(find "$here/corpus/parse_location" -type f | wc -l) seeds"

# geo_codecs takes structured input via `arbitrary`, so a hand-written seed
# corpus would just be bytes the deriver reinterprets. libFuzzer builds its own.
