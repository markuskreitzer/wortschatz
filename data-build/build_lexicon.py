#!/usr/bin/env python3
"""
Build core/data/lexicon.json (and lexicon.js) for Wortschatz from open-licensed sources.

Sources (see DATA_SOURCES.md):
  1. German Wiktionary (de.wiktionary.org) via kaikki.org/wiktextract
       https://kaikki.org/dewiktionary/raw-wiktextract-data.jsonl.gz
     -> German definition, IPA, gender, plural / verb / adjective forms, hyphenation,
        and the per-sense English translations.
  2. English Wiktionary (en.wiktionary.org), German entries, via kaikki.org
       https://kaikki.org/dictionary/German/kaikki.org-dictionary-German.jsonl
     -> English glosses of the German word (used to CROSS-CHECK the translation) and IPA fallback.
  3. English Wiktionary, English entries, via kaikki.org
       https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl
     -> English definition of the English equivalent, choosing the sense whose
        translation table lists the German word.
  4. hermitdave/FrequencyWords, de_50k (OpenSubtitles 2018), CC BY-SA 4.0
     -> ranks everyday words so the lexicon favours common A1–B2 vocabulary.

Nothing is written by hand: an entry is only kept when every field comes from a source
and the English equivalent from German Wiktionary also appears in English Wiktionary's
glosses for the same German word (two independent sources agree).

Usage:
  python3 build_lexicon.py --raw DIR --out ../core/data [--limit 1200] [--top 25000]
  where DIR contains: dewikt_de.jsonl (German-language entries of the de extract),
  en_german.jsonl, en_english_de2.jsonl (filtered English extract), de_50k.txt.
  See fetch_sources.sh for how DIR is produced.
"""
import argparse, json, re, sys, collections

POS_OK = {"noun", "verb", "adj", "adv"}
POS_TITLE_OK = {"Substantiv", "Verb", "Adjektiv", "Adverb"}
BAD_TAGS = {"vulgar", "offensive", "derogatory", "slur", "pejorative", "obsolete", "archaic",
            "dated", "rare", "historical", "dialectal", "regional", "slang", "Austrian", "Swiss",
            "humorous", "euphemistic", "poetic", "figuratively", "figurative",
            "Southern-German", "Northern-German", "Bavarian", "Swabian", "Alemannic", "ironic",
            "Switzerland", "Austria", "nonstandard", "misspelling", "proscribed", "jargon"}
BAD_RAW = re.compile(r"vulgär|derb|abwertend|beleidigend|veraltet|historisch|landschaftlich|regional|"
                     r"salopp|scherzhaft|selten|Jägersprache|Seemannssprache|"
                     r"österreichisch|schweizerisch|süddeutsch|norddeutsch|Jugendsprache|ironisch|"
                     r"übertragen|gehoben|Fachsprache|Soldatensprache|Rotwelsch|DDR", re.I)
# Words that are frequent in subtitles but make poor or awkward cards.
SKIP = set("""
sein haben werden ja nein okay ok hey hallo tschüss na mal so dann also doch noch schon nur auch
nicht sehr ganz gar halt eben eigentlich wohl gerade wieder immer hier da dort jetzt nun heute
Scheiße Arsch Hure Fick Schwanz Titte Nutte Schlampe Mist verdammt Gott Jesus Christus
""".split())

def clean(t):
    t = re.sub(r"\[\d+[a-z]?\]", "", t or "")
    t = re.sub(r"\^\([^)]*\)|\^☆", "", t)   # footnote/marker residue from Wiktionary templates
    t = t.replace(";;", ";")
    t = re.sub(r"\s+", " ", t).strip(" ;:,")
    return t

def word_of(line):
    m = re.match(r'\{"word": "((?:[^"\\]|\\.)*)"', line)
    return json.loads('"%s"' % m.group(1)) if m else None

def load_freq(path, top):
    rank = {}
    with open(path, encoding="utf-8") as f:
        for i, l in enumerate(f):
            if i >= top: break
            w = l.split()[0]
            rank.setdefault(w.lower(), i + 1)
    return rank

def tagset(s):
    return set(s.get("tags") or [])

def sense_bad(s):
    if tagset(s) & BAD_TAGS: return True
    # "auch übertragen" = "also figurative": the literal sense is still primary.
    raw = " ".join(r for r in (s.get("raw_tags") or []) if not r.strip().startswith("auch "))
    return bool(BAD_RAW.search(raw))

WN = None

def main():
    global WN
    import wn
    ap = argparse.ArgumentParser()
    ap.add_argument("--raw", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=1200)
    ap.add_argument("--top", type=int, default=25000)
    ap.add_argument("--why", default="", help="comma-separated words: print why each was kept or dropped")
    a = ap.parse_args()
    raw = a.raw.rstrip("/")
    wn.config.data_directory = f"{raw}/wn_data"
    WN = wn.Wordnet("oewn:2024")
    freq = load_freq(f"{raw}/de_50k.txt", a.top)
    want = set(freq)

    # --- German Wiktionary
    de = collections.defaultdict(list)
    lower_heads = set()   # every lower-case German headword (any part of speech, incl. inflected forms)
    with open(f"{raw}/dewikt_de.jsonl", encoding="utf-8") as f:
        for line in f:
            w = word_of(line)
            if not w or w.lower() not in want: continue
            if w[:1].islower():
                # Remember lower-case words whose frequency would be credited to a capitalised noun
                # by the (lower-cased) frequency list: finite verb forms and function words.
                el = json.loads(line)
                pt = el.get("pos_title") or ""
                g0 = " ".join((el.get("senses") or [{}])[0].get("glosses") or [])
                m = re.search(r"des Verbs (\S+)", g0)
                lemma_rank = freq.get(m.group(1).lower(), 10**9) if m else 10**9
                if (pt == "Konjugierte Form" and re.search(r"Indikativ|Partizip|Konjunktiv", g0)
                        and lemma_rank < 3000) or pt in (
                        "Hilfsverb", "Modalverb", "Personalpronomen", "Possessivpronomen", "Demonstrativpronomen",
                        "Artikel", "Konjunktion", "Subjunktion", "Partikel", "Gradpartikel", "Fokuspartikel",
                        "Antwortpartikel", "Modalpartikel", "Interrogativpronomen", "Relativpronomen"):
                    lower_heads.add(w)
            if w in SKIP: continue
            e = json.loads(line)
            if e.get("lang_code") != "de" or e.get("pos") not in POS_OK: continue
            if e.get("pos_title") not in POS_TITLE_OK: continue
            if "form-of" in (e.get("tags") or []): continue
            de[(w, e["pos"])].append(e)
    print("de candidates", len(de), file=sys.stderr)

    # --- English Wiktionary, German entries
    en = collections.defaultdict(list)
    with open(f"{raw}/en_german.jsonl", encoding="utf-8") as f:
        for line in f:
            e = json.loads(line)
            w = e.get("word")
            if not w or w.lower() not in want: continue
            if e.get("pos") not in POS_OK: continue
            en[(w, e["pos"])].append(e)
    print("en-German candidates", len(en), file=sys.stderr)

    # --- English Wiktionary, English entries that have German translations
    eng = collections.defaultdict(list)
    with open(f"{raw}/en_english_de2.jsonl", encoding="utf-8") as f:
        for line in f:
            e = json.loads(line)
            if e.get("pos") in POS_OK: eng[(e["word"], e["pos"])].append(e)

    def en_gloss_text(key):
        out = []
        for e in en.get(key, []):
            for s in e.get("senses", []):
                if "form-of" in tagset(s) or "alt-of" in tagset(s): continue
                out.extend(s.get("glosses") or [])
        return out

    STOP = set("a an the of or and to in on for with by as at from that which who whom whose this these those "
               "be is are was were being been it its one ones someone something any some such especially usually "
               "often etc e g i not no".split())

    def stems(text):
        toks = re.findall(r"[a-z]+", (text or "").lower())
        return {t[:5] for t in toks if t not in STOP and len(t) > 2}

    WN_POS = {"noun": ("n",), "verb": ("v",), "adj": ("a", "s"), "adv": ("r",)}

    def match_count(label_tokens, text):
        toks = re.findall(r"[a-z]+", text.lower())
        toks = [t for t in toks if t not in STOP and len(t) > 2]
        n = 0
        for a in label_tokens:
            if any(b[:5] == a[:5] or (len(a) >= 4 and b.startswith(a)) or (len(b) >= 4 and a.startswith(b)) for b in toks):
                n += 1
        return n

    def labels_for(eword, pos, german):
        out = []
        for e in eng.get((eword, pos), []):
            for t in e.get("tr", []):
                if t.get("word") == german and t.get("sense"): out.append(t["sense"])
            for sns in e["senses"]:
                for t in sns.get("tr", []):
                    if t.get("word") == german and t.get("sense"): out.append(t["sense"])
        return list(dict.fromkeys(out))

    def in_first_table(eword, pos, german):
        for e in eng.get((eword, pos), [])[:1]:
            first = None
            if e["senses"] and e["senses"][0].get("tr"):
                first = e["senses"][0]["tr"][0].get("sense")
            elif e.get("tr"):
                first = e["tr"][0].get("sense")
            if first is None: return False
            pool = [t for t in e.get("tr", []) if t.get("sense") == first]
            pool += [t for sn in e["senses"] for t in sn.get("tr", []) if t.get("sense") == first]
            return any(t.get("word") == german for t in pool)
        return False

    def english_definition(eword, pos, german):
        """English definition of `eword` from Open English WordNet.

        WordNet lists a word's senses roughly by frequency, so sense 1 is a strong prior.
        English Wiktionary's translation tables tell us which English sense the German word
        translates (each table has a short label such as "line of connected cars or carriages");
        content-word overlap between those labels and each WordNet sense (definition + synonyms)
        moves the choice to the right sense when it is not the first one.
        Returns None (entry dropped) when the choice is not well supported.
        """
        senses = []
        for wp in WN_POS[pos]:
            for wd in WN.words(eword, pos=wp):
                if wd.lemma() != eword: continue
                for sn in wd.senses():
                    senses.append(sn.synset())
        if not senses: return None
        labels = labels_for(eword, pos, german)
        own = set(re.findall(r"[a-z]+", eword.lower()))
        ltoks = [[t for t in re.findall(r"[a-z]+", l.lower()) if t not in STOP and len(t) > 2 and t not in own] for l in labels]
        scored = []
        for rank, ss in enumerate(senses):
            d = ss.definition() or ""
            text = d + " " + " ".join(x for x in ss.lemmas() if x.lower() != eword.lower())
            ratio = 0.0
            for lt in ltoks:
                if lt: ratio = max(ratio, match_count(lt, text) / len(lt))
            scored.append((rank, ratio, d))
        if labels:
            # Sense 1 wins whenever a translation label supports it at all; otherwise a later
            # sense needs strong label support. No support -> no definition (entry dropped).
            if scored[0][1] >= 0.34:
                rank, ratio, d = scored[0]
            else:
                cands = [x for x in scored if x[1] >= 0.5]
                if cands:
                    rank, ratio, d = max(cands, key=lambda x: (x[1], -x[0]))
                elif in_first_table(eword, pos, german):
                    # The German word sits in the English entry's first translation table, i.e.
                    # it translates the primary sense: take WordNet's primary sense.
                    rank, ratio, d = scored[0]
                else:
                    return None
        else:
            if len(senses) > 2: return None
            rank, ratio, d = scored[0]
        if d.startswith("(") or "(abstract)" in d: return None
        if len(d) < 8 or len(d) > 170: return None
        d = d[0].upper() + d[1:]
        if not d.endswith((".", "!", "?", ")")): d += "."
        return d

    def forms(e, pos, gender):
        F = e.get("forms") or []
        def find(*need, exclude=()):
            for x in F:
                tg = set(x.get("tags") or [])
                if set(need) <= tg and not (tg & set(exclude)) and not x.get("raw_tags"):
                    return x["form"]
        if pos == "noun":
            pl = find("nominative", "plural")
            return {"plural": pl} if pl and pl not in ("—", "-") else {}
        if pos == "verb":
            d = {}
            for x in F:
                tg = set(x.get("tags") or [])
                if {"third-person", "singular", "present", "active", "indicative"} <= tg and x["form"].startswith("er/sie/es "):
                    d["present3"] = x["form"][len("er/sie/es "):]; break
            past = find("past")
            pps = [x["form"] for x in F if {"participle-2", "perfect"} <= set(x.get("tags") or [])]
            pp = next((x for x in pps if x != e["word"]), pps[0] if pps else None)
            aux = [x["form"] for x in F if set(x.get("tags") or []) == {"auxiliary", "perfect"}]
            if past: d["past"] = past
            if pp: d["participle"] = pp
            if aux: d["auxiliary"] = " / ".join(dict.fromkeys(aux))
            return d
        if pos == "adj":
            d = {}
            c = find("comparative"); s = find("superlative")
            if c and c != "—": d["comparative"] = c
            if s and s != "—": d["superlative"] = s
            return d
        return {}

    def en_ipa(key):
        for e in en.get(key, []):
            for s in e.get("sounds", []):
                if s.get("ipa") and not s.get("tags"): return s["ipa"]
        return None

    entries, seen_en = [], set()
    reasons = collections.Counter()
    why = {}
    keys = sorted(de, key=lambda k: (freq[k[0].lower()], k[1]))
    for key in keys:
        w, pos = key
        # nouns must be capitalised, other parts of speech lower-case
        if pos == "noun" and not w[:1].isupper(): reasons["case"] += 1; why[w] = "case"; continue
        if pos != "noun" and not w[:1].islower(): reasons["case"] += 1; why[w] = "case"; continue
        if len(w) < 3 or " " in w or "-" in w: reasons["shape"] += 1; why[w] = "shape"; continue
        if pos == "noun" and w.lower() in lower_heads:
            # e.g. "Hast" ranks high only because of "du hast"; "Sein" because of "sein".
            reasons["noun-shadowed"] += 1; why[w] = "noun-shadowed"; continue
        if any(set(s.get("tags") or []) & {"vulgar", "offensive", "derogatory", "slur"}
               for x in en.get(key, []) for s in x.get("senses", [])) or \
           any(re.search(r"vulgär|derb|abwertend|beleidigend", " ".join(x.get("raw_tags") or []))
               or set(x.get("tags") or []) & {"vulgar", "offensive", "derogatory"}
               for x in de[key][0].get("senses", [])[:1]):
            reasons["register"] += 1; why[w] = "register"; continue
        e = de[key][0]
        gender = None
        if pos == "noun":
            g = [t for t in (e.get("tags") or []) if t in ("masculine", "feminine", "neuter")]
            if len(g) != 1: reasons["gender"] += 1; why[w] = "gender"; continue
            gender = g[0]
        ipa = None
        for s in e.get("sounds", []):
            if s.get("ipa"): ipa = s["ipa"]; break
        ipa = ipa or en_ipa(key)
        if not ipa: reasons["ipa"] += 1; why[w] = "ipa"; continue
        glosses_en = en_gloss_text(key)
        if not glosses_en: reasons["no-enwikt"] += 1; why[w] = "no-enwikt"; continue
        gl_blob = " ; ".join(glosses_en).lower()
        trans = collections.defaultdict(list)
        for t in e.get("translations", []):
            if t.get("lang_code") == "en" and t.get("word"):
                trans[t.get("sense_index") or ""].append(t)
        chosen = None
        for s in e.get("senses", []):
            idx = s.get("sense_index")
            g = clean((s.get("glosses") or [""])[0])
            if not g or sense_bad(s) or len(g) < 4 or len(g) > 200: continue
            if re.search(r"Interrogativ|Nebensatz|Komparativ|Adverbiale|Konjunktion|Partikel|Hilfsverb|Modalverb|Pronomen|Präposition", g): continue
            if g.lower().startswith(("kurz für", "kurzform", "variante", "siehe", "veraltet")): continue
            words = []
            for t in trans.get(idx, []):
                tw = t["word"].strip()
                if t.get("raw_tags") or t.get("tags"):
                    if set(t.get("tags") or []) & BAD_TAGS or any(BAD_RAW.search(r) for r in t.get("raw_tags") or []): continue
                    if any(re.search(r"amerikan|brit|Großbritannien|USA|Schottland|Austral", r) for r in t.get("raw_tags") or []): continue
                if pos == "verb" and tw.startswith("to "): tw = tw[3:]
                if pos == "verb" and tw in ("can", "must", "may", "shall", "will", "should", "might", "would", "could"): continue
                if tw and tw not in words: words.append(tw)
            # cross-check with English Wiktionary
            agreed = [x for x in words if re.search(r"(^|[^a-z])(to )?" + re.escape(x.lower()) + r"($|[^a-z])", gl_blob)]
            if agreed:
                chosen = (s, g, agreed); break
        if not chosen: reasons["no-agreement"] += 1; why[w] = "no-agreement"; continue
        s, gdef, agreed = chosen
        primary, edef = agreed[0], None
        for alt in agreed:
            edef = english_definition(alt, pos, w)
            if edef: primary = alt; break
        if not edef: reasons["no-endef"] += 1; why[w] = "no-endef"; continue
        eq = [primary] + [x for x in agreed if x.lower() != primary.lower()][:1]
        english = ", ".join(("to " + x) if pos == "verb" else x for x in eq)
        if english.lower() == english.lower().upper(): pass
        if english.lower() in seen_en and pos != "noun": reasons["dup-en"] += 1; why[w] = "dup-en"
        seen_en.add(english.lower())
        hyph = None
        if e.get("hyphenations"):
            parts = e["hyphenations"][0].get("parts") or []
            if len(parts) > 1: hyph = "·".join(parts)
        entry = {
            "id": f"{w}#{pos}",
            "word": w,
            "pos": pos,
            "gender": gender,
            "article": {"masculine": "der", "feminine": "die", "neuter": "das"}.get(gender),
            "ipa": ipa,
            "syllables": hyph,
            "english": english,
            "definition_en": edef,
            "definition_de": gdef,
            "forms": forms(e, pos, gender),
            "freq_rank": freq[w.lower()],
            "source": {"de": f"https://de.wiktionary.org/wiki/{w}#{s.get('sense_index') or ''}",
                       "en": f"https://en.wiktionary.org/wiki/{w}#German",
                       "en_def": f"https://en-word.net/lemma/{primary.replace(' ', '%20')}"},
        }
        entries.append(entry)
        if len(entries) >= a.limit: break
    # Post-selection curation: drop entries flagged in manual review (data-build/exclude.tsv)
    # and senses that are clearly specialised (games, slang street names).
    import os
    excl = set()
    xp = os.path.join(os.path.dirname(os.path.abspath(__file__)), "exclude.tsv")
    if os.path.exists(xp):
        for line in open(xp, encoding="utf-8"):
            if line.startswith("#") or not line.strip(): continue
            excl.add(line.split("\t")[0].strip())
    SPECIAL = re.compile(r"playing card|card game|billiard|chess|deck of cards|Street names? for|\bbees\b", re.I)
    before = len(entries)
    entries = [e for e in entries if e["id"] not in excl and not SPECIAL.search(e["definition_en"])
               and "^" not in e["definition_de"] and not e["definition_de"].endswith(" von")]
    reasons["review-excluded"] = before - len(entries)
    print("kept", len(entries), dict(reasons), file=sys.stderr)
    if a.why:
        for x in a.why.split(","): print("why", x, why.get(x, "kept" if any(e["word"] == x for e in entries) else "not a candidate"), file=sys.stderr)
    meta = {
        "name": "Wortschatz lexicon",
        "count": len(entries),
        "built": __import__("datetime").date.today().isoformat(),
        "review": "first 1300 candidates manually reviewed; mismatches listed in data-build/exclude.tsv",
        "license": "CC BY-SA 4.0 (derived from Wiktionary, CC BY-SA 4.0 / GFDL; frequency ranks from hermitdave/FrequencyWords, CC BY-SA 4.0)",
        "sources": ["https://de.wiktionary.org", "https://en.wiktionary.org", "https://kaikki.org",
                    "https://github.com/hermitdave/FrequencyWords"],
    }
    doc = {"meta": meta, "entries": entries}
    with open(f"{a.out}/lexicon.json", "w", encoding="utf-8") as f:
        json.dump(doc, f, ensure_ascii=False, indent=1)
    with open(f"{a.out}/lexicon.js", "w", encoding="utf-8") as f:
        f.write("// Generated by data-build/build_lexicon.py. Do not edit by hand.\n")
        f.write("window.WORTSCHATZ_LEXICON = ")
        json.dump(doc, f, ensure_ascii=False)
        f.write(";\n")

if __name__ == "__main__":
    main()
