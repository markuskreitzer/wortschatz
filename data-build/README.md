# Building the lexicon

`core/data/lexicon.json` (and the identical `lexicon.js` used by the browser preview) is
generated, never hand-edited.

Needs python3, curl and [uv](https://docs.astral.sh/uv/) (uv supplies the `wn` WordNet
reader; Arch and Ubuntu 24.04 block `pip install` into the system Python).

```sh
./fetch_sources.sh ~/wz-raw          # ~10 min, several GB; writes ~/wz-raw/sources.json
uv run --no-project --with wn build_lexicon.py --raw ~/wz-raw --out ../core/data   # defaults: --limit 1300 --top 40000
cargo run --release -p wortschatz-core --bin wortschatz -- validate ../core/data/lexicon.json
node ../tools/check-parity.mjs       # the browser's JS card formatting still matches the engine
```

**Reproducibility.** FrequencyWords is pinned to a commit. The kaikki.org Wiktionary dumps are
replaced in place as Wiktionary changes and cannot be pinned, so `fetch_sources.sh` records each
download's URL, `Last-Modified` date and SHA-256 in `sources.json`, and `build_lexicon.py` copies
that into the lexicon's `meta.source_snapshot`. A rebuild from newer dumps produces a different
candidate list: review the changed entries and update `exclude.tsv` before shipping it. (The
lexicon shipped today was built on 2026-11-14, before the snapshot was recorded.)

## How an entry is made

1. **Candidates** come from the 40 000 most frequent tokens in German subtitles
   (FrequencyWords). Nouns whose frequency is really a verb form or function word
   (*Hast* ← *du hast*, *Sein* ← *sein*) are skipped.
2. **German Wiktionary** gives the lemma, part of speech, gender, IPA, hyphenation, plural /
   conjugation / comparison forms, and its numbered senses with per-sense English translations.
   Senses marked archaic, regional, vulgar, figurative, etc. are skipped.
3. The **English equivalent** must be confirmed by two independent sources: a translation of
   the chosen German-Wiktionary sense that *also* appears in English Wiktionary's glosses for the
   same German word.
4. The **English definition** is the Open English WordNet definition of that English word.
   WordNet orders senses by frequency; English Wiktionary's translation tables (whose labels say
   which English sense the German word translates) move the choice to a later sense when needed.
   If neither signal supports a sense, the word is dropped.
5. The **German definition** is the German-Wiktionary gloss of the chosen sense.
6. **Grammar.** Nouns: the plural comes from German Wiktionary; `number` is `singular_only` when
   it has no plural and English Wiktionary marks the noun *no plural*/*uncountable* (or German
   Wiktionary gives "—"), and `plural_only` for pluralia tantum (*die Daten*, article "die", no
   gender). Verbs: `separable` is the particle split off in the present (*ruft an*), `reflexive`
   is set when the present reads *… sich*, the chosen German sense is tagged reflexive only, or
   (untagged sense) most of its examples use *sich*. When German Wiktionary gives two perfect
   auxiliaries, a transitive or reflexive chosen sense (tags from German Wiktionary, else from
   the matching English senses) keeps only *haben* (*hat geheilt*). Otherwise the second
   auxiliary is kept only when English Wiktionary lists both, unqualified, in the same order (so
   *anfangen* is *hat angefangen*, not the regional *ist angefangen*); what remains are true
   doubles such as *stehen* (*hat* in the north, *ist* in the south).
7. **Example.** The first English-Wiktionary usage example (not a quotation) of the sense whose
   own head gloss names the card's displayed English equivalent (form-of senses skipped). For
   verbs the sense must also agree with the chosen German sense's valency (transitive,
   intransitive, reflexive), and an example built with an auxiliary the card does not show is
   skipped; better no example than one for another sense. It must be a full sentence of at most
   90 characters, with a modern English translation (no "literally" notes, no fragments like
   *to accept an apology*).
   `example_exclude.tsv` drops individual examples found wrong in review.
8. **Manual review**: every built entry was read; entries whose English and German senses
   disagree (or are too specialised, e.g. card-game senses) are listed in `exclude.tsv` and
   removed. Nothing is rewritten by hand.

`--why Word1,Word2` prints why specific words were kept or dropped.

To extend: raise `--limit`, rebuild, review the new tail (`entries[N:]`), add mismatches to
`exclude.tsv`.
