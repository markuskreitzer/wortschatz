#!/usr/bin/env bash
# Download and pre-filter the open-licensed sources used by build_lexicon.py.
# Needs ~8 GB of free disk while running (the English Wiktionary extract is streamed and
# filtered on the fly; the German ones are kept). Run on any Linux/macOS box with python3.
#   ./fetch_sources.sh RAW_DIR
set -euo pipefail
RAW="${1:?usage: fetch_sources.sh RAW_DIR}"
mkdir -p "$RAW" && cd "$RAW"

# 1. Frequency list (OpenSubtitles 2018), CC BY-SA 4.0
curl -fsSL -o de_50k.txt https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/de/de_50k.txt

# 2. German Wiktionary (wiktextract via kaikki.org), keep German-language entries only
curl -fsSL https://kaikki.org/dewiktionary/raw-wiktextract-data.jsonl.gz | gunzip | \
  python3 -c 'import sys,json
for l in sys.stdin:
    if "\"lang_code\": \"de\"" in l and json.loads(l).get("lang_code")=="de": sys.stdout.write(l)' > dewikt_de.jsonl

# 3. English Wiktionary, German entries
curl -fsSL -o en_german.jsonl https://kaikki.org/dictionary/German/kaikki.org-dictionary-German.jsonl

# 4. English Wiktionary, English entries with German translations (streamed + filtered)
curl -fsSL https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl | python3 -c '
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

# 5. Open English WordNet 2024 (CC BY 4.0) via the `wn` package
python3 - <<'PY'
import wn
wn.config.data_directory = "wn_data"
wn.download("oewn:2024")
PY
echo "sources ready in $RAW"
