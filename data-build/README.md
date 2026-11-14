# Building the lexicon

`core/data/lexicon.json` (and the identical `lexicon.js` used by the browser preview) is
generated, never hand-edited.

```sh
python3 -m pip install wn            # Open English WordNet reader
./fetch_sources.sh /tmp/wz-raw       # ~10 min, several GB
python3 build_lexicon.py --raw /tmp/wz-raw --out ../core/data --limit 1300 --top 40000
cargo run --release --manifest-path ../engine/Cargo.toml -- validate ../core/data/lexicon.json
```

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
   is set when the present reads *… sich* or the chosen German sense is tagged reflexive only.
   A second perfect auxiliary is kept only when English Wiktionary lists both, unqualified, in
   the same order (so *anfangen* is *hat angefangen*, not the regional *ist angefangen*).
7. **Example.** The first English-Wiktionary usage example (not a quotation) of the sense whose
   gloss contains the card's English equivalent: a full sentence of at most 90 characters, with
   a modern English translation (no "literally" notes, no fragments like *to accept an apology*).
   `example_exclude.tsv` drops individual examples found wrong in review.
8. **Manual review**: every built entry was read; entries whose English and German senses
   disagree (or are too specialised, e.g. card-game senses) are listed in `exclude.tsv` and
   removed. Nothing is rewritten by hand.

`--why Word1,Word2` prints why specific words were kept or dropped.

To extend: raise `--limit`, rebuild, review the new tail (`entries[N:]`), add mismatches to
`exclude.tsv`.
