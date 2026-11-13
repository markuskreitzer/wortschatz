# Data sources and licences

| What | Source | Licence |
|---|---|---|
| German headwords, gender, IPA, forms, German definitions, per-sense English translations | [German Wiktionary](https://de.wiktionary.org) via [kaikki.org / wiktextract](https://kaikki.org/dewiktionary/) | CC BY-SA 4.0 (and GFDL) |
| English glosses of German words (cross-check), IPA fallback | [English Wiktionary](https://en.wiktionary.org) via [kaikki.org](https://kaikki.org/dictionary/German/) | CC BY-SA 4.0 (and GFDL) |
| Sense alignment (translation-table labels) | English Wiktionary, English entries, via [kaikki.org](https://kaikki.org/dictionary/English/) | CC BY-SA 4.0 (and GFDL) |
| English definitions | [Open English WordNet 2024](https://en-word.net) | CC BY 4.0 |
| Word frequency (selection of common words) | [hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) (OpenSubtitles 2018) | CC BY-SA 4.0 |

Kaikki/wiktextract: Tatu Ylonen, *Wiktextract: Wiktionary as Machine-Readable Structured Data*,
LREC 2022.

The compiled lexicon (`core/data/lexicon.json`, `lexicon.js`) is a derivative of the above and is
distributed under **CC BY-SA 4.0**. Each entry carries links to its Wiktionary pages and WordNet
lemma in `source`. Definitions are reproduced verbatim except that WordNet definitions get an
initial capital and a final full stop, and Wiktionary sense numbers / footnote markers are removed.

No CEFR level is shown: none of these sources provides one, and it is not inferred.

## Fonts (bundled in `core/fonts/`, all SIL Open Font License 1.1)

| Font | Use | Licence file |
|---|---|---|
| Cormorant Garamond (Christian Thalmann) | headword, English equivalent | `OFL-cormorantgaramond.txt` |
| Source Serif 4 (Adobe) | definitions | `OFL-sourceserif4.txt` |
| Inter (Rasmus Andersson) | labels | `OFL-inter.txt` |
| Gentium Plus (SIL International) | IPA | `OFL-gentiumplus.txt` |

Fonts were subset to Latin, Latin Extended, IPA and punctuation and converted to WOFF2 for
embedding, and are referenced in CSS under neutral family aliases ("Wz Display", "Wz Text", …).
The original OFL texts are shipped next to them. If you prefer to avoid any question about
modified versions under the OFL, replace the `.woff2` files with the unmodified upstream files.
