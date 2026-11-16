#!/usr/bin/env bash
# Download and pre-filter the open-licensed sources used by build_lexicon.py.
# Needs ~8 GB of free disk while running (the English Wiktionary extract is streamed and
# filtered on the fly; the German ones are kept), python3, curl, and uv (for the `wn` package;
# without uv, `wn` must already be importable by python3).
#   ./fetch_sources.sh RAW_DIR
#
# The kaikki.org dumps are replaced in place as Wiktionary changes, so they cannot be pinned.
# Instead this writes RAW_DIR/sources.json: each source's URL, Last-Modified header, download
# date and the SHA-256 of the file build_lexicon.py reads. build_lexicon.py copies it into the
# lexicon's meta.source_snapshot, so a rebuild can be compared with the shipped data.
set -euo pipefail
RAW="${1:?usage: fetch_sources.sh RAW_DIR}"
mkdir -p "$RAW" && cd "$RAW"

# FrequencyWords is pinned to a commit; its 2018 lists do not change.
FREQ_COMMIT=525f9b560de45753a5ea01069454e72e9aa541c6
FREQ_URL="https://raw.githubusercontent.com/hermitdave/FrequencyWords/$FREQ_COMMIT/content/2018/de/de_50k.txt"
DEWIKT_URL=https://kaikki.org/dewiktionary/raw-wiktextract-data.jsonl.gz
EN_GERMAN_URL=https://kaikki.org/dictionary/German/kaikki.org-dictionary-German.jsonl
EN_ENGLISH_URL=https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl

last_modified() { curl -fsSI "$1" | tr -d '\r' | sed -n 's/^[Ll]ast-[Mm]odified: //p'; }
: > sources.tsv
record() {  # name url file
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$(last_modified "$2" || true)" "$3" >> sources.tsv
}

# 1. Frequency list (OpenSubtitles 2018), CC BY-SA 4.0
curl -fsSL -o de_50k.txt "$FREQ_URL"
record frequency "$FREQ_URL" de_50k.txt

# 2. German Wiktionary (wiktextract via kaikki.org), keep German-language entries only
curl -fsSL "$DEWIKT_URL" | gunzip | \
  python3 -c 'import sys,json
for l in sys.stdin:
    if "\"lang_code\": \"de\"" in l and json.loads(l).get("lang_code")=="de": sys.stdout.write(l)' > dewikt_de.jsonl
record de_wiktionary "$DEWIKT_URL" dewikt_de.jsonl

# 3. English Wiktionary, German entries
curl -fsSL -o en_german.jsonl "$EN_GERMAN_URL"
record en_wiktionary_german "$EN_GERMAN_URL" en_german.jsonl

# 4. English Wiktionary, English entries with German translations (streamed + filtered)
curl -fsSL "$EN_ENGLISH_URL" | python3 -c '
import sys, json
out = open("en_english_de2.jsonl", "w")
for line in sys.stdin:
    if "\"German\"" not in line: continue
    try: e = json.loads(line)
    except Exception: continue
    if e.get("pos") not in ("noun", "verb", "adj", "adv"): continue
    de = lambda t: t.get("lang") == "German" or t.get("code") == "de"
    tr = [t for t in e.get("translations", []) if de(t)]
    senses = [{"glosses": s.get("glosses", []), "tags": s.get("tags", []), "topics": s.get("topics", []),
               "raw_glosses": s.get("raw_glosses", []), "tr": [t for t in s.get("translations", []) if de(t)]}
              for s in e.get("senses", [])]
    if tr or any(x["tr"] for x in senses):
        out.write(json.dumps({"word": e["word"], "pos": e["pos"], "tr": tr, "senses": senses}) + "\n")
'
record en_wiktionary_english "$EN_ENGLISH_URL" en_english_de2.jsonl

# 5. Open English WordNet 2024 (CC BY 4.0) via the `wn` package
PY=(python3)
command -v uv >/dev/null && PY=(uv run --quiet --no-project --with wn python)
"${PY[@]}" - <<'PY'
import wn
wn.config.data_directory = "wn_data"
wn.download("oewn:2024")
PY

# sources.json: what this build directory contains.
python3 - <<'PY'
import datetime, hashlib, json
def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""): h.update(chunk)
    return h.hexdigest()
today = datetime.date.today().isoformat()
out = {}
for line in open("sources.tsv", encoding="utf-8"):
    name, url, modified, path = line.rstrip("\n").split("\t")
    out[name] = {"url": url, "last_modified": modified or None, "fetched": today, "file": path, "sha256": sha256(path)}
out["wordnet"] = {"package": "oewn:2024", "fetched": today}
json.dump(out, open("sources.json", "w"), indent=1)
PY
rm -f sources.tsv
echo "sources ready in $RAW (see $RAW/sources.json)"
