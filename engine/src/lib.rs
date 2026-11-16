//! Wortschatz engine.
//!
//! Everything in here is platform-agnostic: it knows nothing about macOS, Linux, WebKit or
//! screensavers. Native shells (the macOS `.saver`, a future Omarchy/Hyprland runner) link this
//! crate (as an `rlib` or through the C ABI in [`ffi`]) to pick the next card and hand its JSON to
//! the shared HTML renderer in `core/`.

pub mod ffi;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lexicon {
    pub meta: serde_json::Value,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub word: String,
    pub pos: String,
    pub gender: Option<String>,
    pub article: Option<String>,
    pub ipa: String,
    #[serde(default)]
    pub syllables: Option<String>,
    pub english: String,
    pub definition_en: String,
    pub definition_de: String,
    #[serde(default)]
    pub forms: BTreeMap<String, String>,
    /// Nouns: "singular_only" (kein Plural) or "plural_only" (Pluraletantum); absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// Verbs: the separable particle ("an" for anrufen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separable: Option<String>,
    /// Verbs: used with "sich" in the sense shown on the card.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reflexive: bool,
    /// One short usage example from English Wiktionary, with its English translation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<Example>,
    #[serde(default)]
    pub freq_rank: Option<u32>,
    #[serde(default)]
    pub source: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Example {
    pub de: String,
    pub en: String,
}

/// Longest German example sentence a card will show (characters).
pub const EXAMPLE_MAX_CHARS: usize = 90;

/// What a renderer needs to draw one card. Serialized to JSON for the WebView.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Card {
    pub serial: u64,
    pub id: String,
    pub word: String,
    pub article: Option<String>,
    pub gender: Option<String>,
    pub pos: String,
    pub pos_de: String,
    pub pos_en: String,
    pub ipa: String,
    pub syllables: Option<String>,
    pub english: String,
    pub definition_en: String,
    pub definition_de: String,
    pub forms_label: Option<String>,
    pub forms_line: Option<String>,
    /// Nouns: "singular_only" | "plural_only" | null.
    pub number: Option<String>,
    /// Nouns: the plural, split so renderers can colour the article ("die", "Häuser").
    pub plural_article: Option<String>,
    pub plural: Option<String>,
    /// Nouns without a plural form: "kein Plural" / "nur Plural".
    pub plural_note: Option<String>,
    /// Verbs.
    pub reflexive: bool,
    pub separable_prefix: Option<String>,
    /// Verbs: "fährt · fuhr · ist gefahren" (the present is only listed when it is irregular).
    pub principal_parts: Option<String>,
    pub example_de: Option<String>,
    pub example_en: Option<String>,
    pub source: BTreeMap<String, String>,
}

impl Lexicon {
    pub fn from_json(s: &str) -> Result<Self, String> {
        let lex: Lexicon = serde_json::from_str(s).map_err(|e| format!("lexicon parse error: {e}"))?;
        if lex.entries.is_empty() {
            return Err("lexicon has no entries".into());
        }
        Ok(lex)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_json(&s)
    }

    /// Structural checks; returns human-readable problems (empty = valid).
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, e) in self.entries.iter().enumerate() {
            let at = format!("#{i} {}", e.word);
            if !seen.insert(e.id.clone()) {
                problems.push(format!("{at}: duplicate id {}", e.id));
            }
            for (name, v) in [
                ("word", &e.word),
                ("ipa", &e.ipa),
                ("english", &e.english),
                ("definition_en", &e.definition_en),
                ("definition_de", &e.definition_de),
            ] {
                if v.trim().is_empty() {
                    problems.push(format!("{at}: empty {name}"));
                }
            }
            if !matches!(e.pos.as_str(), "noun" | "verb" | "adj" | "adv") {
                problems.push(format!("{at}: unexpected pos {}", e.pos));
            }
            if e.pos == "noun" {
                let ok = matches!(
                    (e.gender.as_deref(), e.article.as_deref(), e.number.as_deref()),
                    (Some("masculine"), Some("der"), n) | (Some("feminine"), Some("die"), n) | (Some("neuter"), Some("das"), n)
                        if n != Some("plural_only")
                ) || matches!(
                    (e.gender.as_deref(), e.article.as_deref(), e.number.as_deref()),
                    (None, Some("die"), Some("plural_only"))
                );
                if !ok {
                    problems.push(format!("{at}: noun without consistent gender/article"));
                }
                match e.number.as_deref() {
                    None | Some("plural_only") => {}
                    Some("singular_only") if e.forms.contains_key("plural") => {
                        problems.push(format!("{at}: singular_only noun has a plural"))
                    }
                    Some("singular_only") => {}
                    Some(other) => problems.push(format!("{at}: unexpected number {other}")),
                }
            } else if e.number.is_some() {
                problems.push(format!("{at}: number on a non-noun"));
            }
            if e.pos == "verb" {
                for k in ["present3", "past", "participle", "auxiliary"] {
                    if e.forms.get(k).is_none_or(|v| v.trim().is_empty()) {
                        problems.push(format!("{at}: verb missing {k}"));
                    }
                }
                if let Some(aux) = e.forms.get("auxiliary") {
                    if aux.split('/').any(|a| !matches!(a.trim(), "haben" | "sein")) {
                        problems.push(format!("{at}: unexpected auxiliary {aux}"));
                    }
                }
                if let Some(p) = &e.separable {
                    let p3 = e.forms.get("present3").map(String::as_str).unwrap_or("");
                    if !e.word.starts_with(p.as_str()) || p.len() >= e.word.len() || !p3.ends_with(&format!(" {p}")) {
                        problems.push(format!("{at}: separable prefix {p} does not match word/forms"));
                    }
                }
            } else if e.separable.is_some() || e.reflexive {
                problems.push(format!("{at}: separable/reflexive on a non-verb"));
            }
            if let Some(x) = &e.example {
                if x.de.trim().is_empty() || x.en.trim().is_empty() {
                    problems.push(format!("{at}: example without text or translation"));
                }
                if x.de.chars().count() > EXAMPLE_MAX_CHARS {
                    problems.push(format!("{at}: example longer than {EXAMPLE_MAX_CHARS} characters"));
                }
                if x.de.contains('\n') || x.en.contains('\n') {
                    problems.push(format!("{at}: example has a line break"));
                }
            }
            if !(e.ipa.starts_with('[') || e.ipa.starts_with('/')) {
                problems.push(format!("{at}: IPA not bracketed: {}", e.ipa));
            }
        }
        problems
    }
}

/// Card timing from `core/timing.js`, the one timing spec for every renderer (milliseconds).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Timing {
    pub fade_in_ms: f64,
    pub drift_in_ms: f64,
    pub progress_ms: f64,
    pub cycle_ms: f64,
    pub exit_lead_ms: f64,
    pub fade_out_ms: f64,
}

impl Default for Timing {
    /// The values in `core/timing.js`, used only if that file cannot be read.
    fn default() -> Self {
        Timing { fade_in_ms: 2600.0, drift_in_ms: 4160.0, progress_ms: 32000.0, cycle_ms: 34600.0, exit_lead_ms: 2210.0, fade_out_ms: 2080.0 }
    }
}

impl Timing {
    /// Marker that starts the object literal in `core/timing.js`, searched from the end of the
    /// file so comments that mention it do not match. The macOS saver does the same
    /// (macos/Sources/WortschatzView.swift).
    pub const MARKER: &'static str = "WORTSCHATZ_TIMING = {";

    /// Parses the strict-JSON object literal after [`Timing::MARKER`] in `core/timing.js`.
    pub fn from_js(js: &str) -> Result<Self, String> {
        let start = js.rfind(Self::MARKER).ok_or("timing.js: WORTSCHATZ_TIMING = { not found")? + Self::MARKER.len() - 1;
        let len = js[start..].find('}').ok_or("timing.js: unterminated object")? + 1;
        serde_json::from_str(&js[start..start + len]).map_err(|e| format!("timing.js: {e}"))
    }

    /// When a host that calls `Wortschatz.show()` should send the next card, counted from the
    /// moment the current card started to enter: the web renderer fades the old card out for
    /// `exit_lead_ms` before the new one enters, so the call comes that much before `cycle_ms`.
    pub fn host_show_after_ms(&self) -> f64 {
        self.cycle_ms - self.exit_lead_ms
    }
}

/// Formats a lexicon entry into a renderable card.
pub fn card_for(e: &Entry, serial: u64) -> Card {
    let (pos_de, pos_en) = match e.pos.as_str() {
        "noun" => ("Substantiv", "noun"),
        "verb" => ("Verb", "verb"),
        "adj" => ("Adjektiv", "adjective"),
        "adv" => ("Adverb", "adverb"),
        other => (other, other),
    };
    let f = |k: &str| e.forms.get(k).filter(|v| !v.trim().is_empty()).cloned();
    let plural = if e.pos == "noun" && e.number.as_deref() != Some("plural_only") { f("plural") } else { None };
    let plural_note = match (e.pos.as_str(), e.number.as_deref(), &plural) {
        ("noun", Some("plural_only"), _) => Some("nur Plural".to_string()),
        ("noun", Some("singular_only"), None) => Some("kein Plural".to_string()),
        _ => None,
    };
    let principal_parts = if e.pos == "verb" { principal_parts(e) } else { None };
    let (forms_label, forms_line) = match e.pos.as_str() {
        "noun" => match &plural {
            Some(p) => (Some("Plural".to_string()), Some(format!("die {p}"))),
            None => (None, None),
        },
        "verb" => match &principal_parts {
            Some(pp) => (Some("Stammformen".to_string()), Some(pp.clone())),
            None => (None, None),
        },
        "adj" => match (f("comparative"), f("superlative")) {
            (Some(c), Some(s)) => (Some("Steigerung".into()), Some(format!("{c} · {s}"))),
            _ => (None, None),
        },
        _ => (None, None),
    };
    Card {
        serial,
        id: e.id.clone(),
        word: e.word.clone(),
        article: e.article.clone(),
        gender: e.gender.clone(),
        pos: e.pos.clone(),
        pos_de: pos_de.into(),
        pos_en: pos_en.into(),
        ipa: e.ipa.clone(),
        syllables: e.syllables.clone(),
        english: e.english.clone(),
        definition_en: e.definition_en.clone(),
        definition_de: e.definition_de.clone(),
        forms_label,
        forms_line,
        number: e.number.clone(),
        plural_article: plural.as_ref().map(|_| "die".to_string()),
        plural,
        plural_note,
        reflexive: e.pos == "verb" && e.reflexive,
        separable_prefix: if e.pos == "verb" { e.separable.clone() } else { None },
        principal_parts,
        example_de: e.example.as_ref().map(|x| x.de.clone()),
        example_en: e.example.as_ref().map(|x| x.en.clone()),
        source: e.source.clone(),
    }
}

/// The 3rd person singular present a regular verb would have (`machen` -> `macht`,
/// `arbeiten` -> `arbeitet`, `sammeln` -> `sammelt`). Used only to decide whether the present
/// is worth showing; the forms themselves always come from Wiktionary.
pub fn regular_present3(infinitive: &str) -> String {
    let w = infinitive;
    let stem = if w.ends_with("eln") || w.ends_with("ern") {
        &w[..w.len() - 1]
    } else if let Some(s) = w.strip_suffix("en") {
        s
    } else if let Some(s) = w.strip_suffix('n') {
        s
    } else {
        w
    };
    let cs: Vec<char> = stem.chars().collect();
    let n = cs.len();
    let needs_e = match cs.last() {
        Some('t') | Some('d') => true,
        // atmen -> atmet, öffnen -> öffnet, rechnen -> rechnet; but lernt, kommt, wohnt.
        Some('m') | Some('n') if n >= 2 => {
            let p = cs[n - 2];
            let vowel = "aeiouäöüy".contains(p);
            !vowel && !matches!(p, 'l' | 'r' | 'm' | 'n' | 'h') || (p == 'h' && n >= 3 && cs[n - 3] == 'c')
        }
        _ => false,
    };
    format!("{stem}{}", if needs_e { "et" } else { "t" })
}

/// "fährt · fuhr · ist gefahren", "rief an · hat angerufen", "benimmt sich · benahm sich ·
/// hat sich benommen". Wiktionary lists the finite forms with the particle (and sometimes
/// "sich") already split off; they are normalised so every part reads the same way.
pub fn principal_parts(e: &Entry) -> Option<String> {
    let get = |k: &str| e.forms.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let (past, participle) = (get("past")?, get("participle")?);
    let particle = e.separable.as_deref();
    let finite = |form: &str| -> String {
        let mut core: Vec<&str> = form.split_whitespace().filter(|t| *t != "sich").collect();
        if let (Some(p), Some(last)) = (particle, core.last()) {
            if core.len() > 1 && *last == p {
                core.pop();
            }
        }
        let mut out = core.join(" ");
        if e.reflexive {
            out.push_str(" sich");
        }
        if let Some(p) = particle {
            out.push(' ');
            out.push_str(p);
        }
        out
    };
    let mut parts = Vec::new();
    if let Some(p3) = get("present3") {
        let base = particle.and_then(|p| e.word.strip_prefix(p)).unwrap_or(&e.word);
        let shown = finite(&p3);
        let core = shown.split_whitespace().next().unwrap_or("");
        if core != regular_present3(base) {
            parts.push(shown);
        }
    }
    parts.push(finite(&past));
    let aux: Vec<&str> = get("auxiliary")
        .as_deref()
        .unwrap_or("")
        .split('/')
        .filter_map(|a| match a.trim() {
            "sein" => Some("ist"),
            "haben" => Some("hat"),
            _ => None,
        })
        .collect::<Vec<_>>();
    let aux = aux.join(" / ");
    let refl = if e.reflexive { "sich " } else { "" };
    parts.push(if aux.is_empty() { format!("{refl}{participle}") } else { format!("{aux} {refl}{participle}") });
    Some(parts.join(" · "))
}

/// Small, dependency-free PRNG (xorshift64*). Quality is plenty for shuffling flashcards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1) ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn from_time() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Rng::new(t ^ (std::process::id() as u64).rotate_left(32))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// A "shuffle bag": every entry is shown exactly once per round, in random order; a new round
/// never starts with the card that ended the previous one, and recently seen ids are kept out of
/// the first few slots of the next round so nothing repeats back to back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deck {
    order: Vec<usize>,
    pos: usize,
    rng: Rng,
    serial: u64,
    #[serde(default)]
    recent: Vec<usize>,
    #[serde(default)]
    len: usize,
}

const RECENT: usize = 8;

impl Deck {
    pub fn new(len: usize, rng: Rng) -> Self {
        let mut d = Deck { order: Vec::new(), pos: 0, rng, serial: 0, recent: Vec::new(), len };
        d.reshuffle();
        d
    }

    fn reshuffle(&mut self) {
        let n = self.len;
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = self.rng.below(i + 1);
            order.swap(i, j);
        }
        // Push recently shown items out of the opening slots of the new round.
        let guard = RECENT.min(n.saturating_sub(1) / 2);
        for slot in 0..guard {
            if self.recent.contains(&order[slot]) {
                if let Some(k) = (guard..n).find(|&k| !self.recent.contains(&order[k])) {
                    order.swap(slot, k);
                }
            }
        }
        self.order = order;
        self.pos = 0;
    }

    /// Index of the next entry to show.
    pub fn next_index(&mut self) -> usize {
        if self.pos >= self.order.len() {
            self.reshuffle();
        }
        let i = self.order[self.pos];
        self.pos += 1;
        self.serial += 1;
        self.recent.push(i);
        if self.recent.len() > RECENT {
            self.recent.remove(0);
        }
        i
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when a deck restored from disk can be used for a lexicon of `n` entries: `order`
    /// is a permutation of `0..n`, the cursor is in range, recent ids are valid and the RNG is
    /// live. Anything else (a truncated file, a lexicon of a different size, hand edits) means
    /// the caller should start a fresh shuffle instead.
    pub fn is_consistent_with(&self, n: usize) -> bool {
        if n == 0 || self.len != n || self.order.len() != n || self.pos > n || self.rng.0 == 0 {
            return false;
        }
        let mut seen = vec![false; n];
        for &i in &self.order {
            if i >= n || std::mem::replace(&mut seen[i], true) {
                return false;
            }
        }
        self.recent.len() <= RECENT && self.recent.iter().all(|&i| i < n)
    }
}

/// Lexicon + deck, optionally persisted so each screensaver activation continues the round
/// instead of starting over.
pub struct Engine {
    pub lexicon: Lexicon,
    deck: Deck,
    state_path: Option<PathBuf>,
}

impl Engine {
    pub fn new(lexicon: Lexicon, state_path: Option<PathBuf>) -> Self {
        let n = lexicon.entries.len();
        let deck = state_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Deck>(&s).ok())
            .filter(|d| d.is_consistent_with(n))
            .unwrap_or_else(|| Deck::new(n, Rng::from_time()));
        Engine { lexicon, deck, state_path }
    }

    pub fn next_card(&mut self) -> Card {
        let i = self.deck.next_index();
        let card = card_for(&self.lexicon.entries[i], self.deck.serial());
        self.save();
        card
    }

    fn save(&self) {
        if let Some(p) = &self.state_path {
            if let Ok(s) = serde_json::to_string(&self.deck) {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let tmp = p.with_extension("tmp");
                if std::fs::write(&tmp, s).is_ok() {
                    let _ = std::fs::rename(&tmp, p);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(word: &str, pos: &str) -> Entry {
        Entry {
            id: format!("{word}#{pos}"),
            word: word.into(),
            pos: pos.into(),
            gender: if pos == "noun" { Some("neuter".into()) } else { None },
            article: if pos == "noun" { Some("das".into()) } else { None },
            ipa: "[x]".into(),
            syllables: None,
            english: "x".into(),
            definition_en: "x".into(),
            definition_de: "x".into(),
            forms: BTreeMap::new(),
            number: None,
            separable: None,
            reflexive: false,
            example: None,
            freq_rank: None,
            source: BTreeMap::new(),
        }
    }

    #[test]
    fn deck_shows_everything_once_per_round_without_back_to_back_repeats() {
        for seed in 1..200u64 {
            let n = 37;
            let mut d = Deck::new(n, Rng::new(seed));
            let mut last = usize::MAX;
            for round in 0..5 {
                let mut seen = vec![false; n];
                for _ in 0..n {
                    let i = d.next_index();
                    assert_ne!(i, last, "immediate repeat (seed {seed}, round {round})");
                    assert!(!seen[i]);
                    seen[i] = true;
                    last = i;
                }
                assert!(seen.iter().all(|s| *s));
            }
        }
    }

    #[test]
    fn bundled_timing_js_parses() {
        let t = Timing::from_js(include_str!("../../core/timing.js")).unwrap();
        assert_eq!(t, Timing::default(), "update Timing::default() to match core/timing.js");
        assert!(t.exit_lead_ms < t.cycle_ms && t.progress_ms <= t.cycle_ms);
        // A brace in a comment before the object must not confuse the parser.
        let js = "/* WORTSCHATZ_TIMING = {x} */\nwindow.WORTSCHATZ_TIMING = {\"fade_in_ms\": 1, \"drift_in_ms\": 2, \"progress_ms\": 3, \"cycle_ms\": 4, \"exit_lead_ms\": 1, \"fade_out_ms\": 1};";
        assert_eq!(Timing::from_js(js).unwrap().cycle_ms, 4.0);
        assert!(Timing::from_js("nothing here").is_err());
    }

    #[test]
    fn tiny_decks_work() {
        let mut d = Deck::new(1, Rng::new(3));
        assert_eq!(d.next_index(), 0);
        assert_eq!(d.next_index(), 0);
        let mut d = Deck::new(2, Rng::new(3));
        let a = d.next_index();
        let b = d.next_index();
        assert_ne!(a, b);
    }

    fn verb(word: &str, p3: &str, past: &str, pp: &str, aux: &str) -> Entry {
        let mut e = entry(word, "verb");
        for (k, v) in [("present3", p3), ("past", past), ("participle", pp), ("auxiliary", aux)] {
            e.forms.insert(k.into(), v.into());
        }
        e
    }

    #[test]
    fn principal_parts_show_irregular_present_aux_particle_and_sich() {
        let pp = |e: &Entry| card_for(e, 1).principal_parts.unwrap();
        assert_eq!(pp(&verb("gehen", "geht", "ging", "gegangen", "sein")), "ging · ist gegangen");
        assert_eq!(pp(&verb("fahren", "fährt", "fuhr", "gefahren", "sein / haben")), "fährt · fuhr · ist / hat gefahren");
        assert_eq!(pp(&verb("arbeiten", "arbeitet", "arbeitete", "gearbeitet", "haben")), "arbeitete · hat gearbeitet");
        assert_eq!(pp(&verb("sammeln", "sammelt", "sammelte", "gesammelt", "haben")), "sammelte · hat gesammelt");
        assert_eq!(pp(&verb("wissen", "weiß", "wusste", "gewusst", "haben")), "weiß · wusste · hat gewusst");
        let mut e = verb("anrufen", "ruft an", "rief an", "angerufen", "haben");
        e.separable = Some("an".into());
        let c = card_for(&e, 1);
        assert_eq!(c.principal_parts.as_deref(), Some("rief an · hat angerufen"));
        assert_eq!(c.separable_prefix.as_deref(), Some("an"));
        let mut e = verb("benehmen", "benimmt sich", "benahm", "benommen", "haben");
        e.reflexive = true;
        let c = card_for(&e, 1);
        assert_eq!(c.principal_parts.as_deref(), Some("benimmt sich · benahm sich · hat sich benommen"));
        assert!(c.reflexive);
        let mut e = verb("vorstellen", "stellt sich vor", "stellte vor", "vorgestellt", "haben");
        e.reflexive = true;
        e.separable = Some("vor".into());
        assert_eq!(pp(&e), "stellte sich vor · hat sich vorgestellt");
        assert_eq!(regular_present3("öffnen"), "öffnet");
        assert_eq!(regular_present3("rechnen"), "rechnet");
        assert_eq!(regular_present3("lernen"), "lernt");
        assert_eq!(regular_present3("tun"), "tut");
    }

    #[test]
    fn noun_plural_number_and_validation() {
        let mut e = entry("Haus", "noun");
        e.forms.insert("plural".into(), "Häuser".into());
        let c = card_for(&e, 1);
        assert_eq!(c.forms_line.as_deref(), Some("die Häuser"));
        assert_eq!((c.plural_article.as_deref(), c.plural.as_deref(), c.plural_note.as_deref()), (Some("die"), Some("Häuser"), None));
        let mut durst = entry("Durst", "noun");
        durst.number = Some("singular_only".into());
        let c = card_for(&durst, 1);
        assert_eq!((c.plural.as_deref(), c.plural_note.as_deref()), (None, Some("kein Plural")));
        let mut leute = entry("Leute", "noun");
        leute.gender = None;
        leute.article = Some("die".into());
        leute.number = Some("plural_only".into());
        leute.forms.insert("plural".into(), "Leute".into());
        let c = card_for(&leute, 1);
        assert_eq!((c.plural.as_deref(), c.plural_note.as_deref()), (None, Some("nur Plural")));
        let mut long = entry("Tag", "noun");
        long.example = Some(Example { de: "x".repeat(91), en: "y".into() });
        let lex = Lexicon { meta: serde_json::Value::Null, entries: vec![e.clone(), durst, leute, long] };
        let problems = lex.validate();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("example longer"));
        let lex = Lexicon { meta: serde_json::Value::Null, entries: vec![e.clone(), e] };
        assert!(lex.validate().iter().any(|p| p.contains("duplicate")));
    }

    #[test]
    fn plural_only_with_gender_is_rejected() {
        let mut daten = entry("Daten", "noun");
        daten.gender = None;
        daten.article = Some("die".into());
        daten.number = Some("plural_only".into());
        let ok = Lexicon { meta: serde_json::Value::Null, entries: vec![daten.clone()] };
        assert!(ok.validate().is_empty(), "{:?}", ok.validate());
        for (g, a) in [("masculine", "der"), ("feminine", "die"), ("neuter", "das")] {
            let mut bad = daten.clone();
            bad.gender = Some(g.into());
            bad.article = Some(a.into());
            let lex = Lexicon { meta: serde_json::Value::Null, entries: vec![bad] };
            assert!(lex.validate().iter().any(|p| p.contains("gender/article")), "{g}/{a} accepted");
        }
        let mut no_article = daten;
        no_article.article = None;
        let lex = Lexicon { meta: serde_json::Value::Null, entries: vec![no_article] };
        assert!(!lex.validate().is_empty());
    }

    fn lexicon(n: usize) -> Lexicon {
        Lexicon { meta: serde_json::Value::Null, entries: (0..n).map(|i| entry(&format!("W{i}"), "noun")).collect() }
    }

    #[test]
    fn corrupted_deck_state_falls_back_to_a_fresh_shuffle() {
        let dir = std::env::temp_dir().join(format!("wz-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = dir.join("deck.json");
        let n = 10;
        let good = serde_json::to_value(Deck::new(n, Rng::new(7))).unwrap();
        let mut cases: Vec<String> = vec![
            "".into(),
            "{".into(),
            "null".into(),
            "{\"order\":[0,1,2],\"pos\":0,\"rng\":5,\"serial\":0}".into(),
        ];
        let mutate = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            f(&mut v);
            v.to_string()
        };
        cases.push(mutate(&|v| v["order"][3] = 999.into()));                 // out of range
        cases.push(mutate(&|v| v["order"][3] = v["order"][4].clone()));      // duplicate
        cases.push(mutate(&|v| v["pos"] = 11.into()));                       // cursor past end
        cases.push(mutate(&|v| v["pos"] = (-1).into()));                     // wrong type
        cases.push(mutate(&|v| v["recent"] = serde_json::json!([3, 42]))); // bad recent id
        cases.push(mutate(&|v| v["rng"] = 0.into()));                        // dead RNG
        cases.push(mutate(&|v| v["len"] = 11.into()));                       // other lexicon size
        cases.push(mutate(&|v| v["order"].as_array_mut().unwrap().pop().map(|_| ()).unwrap()));
        for (k, c) in cases.iter().enumerate() {
            std::fs::write(&state, c).unwrap();
            let mut eng = Engine::new(lexicon(n), Some(state.clone()));
            let mut seen = std::collections::HashSet::new();
            for _ in 0..n {
                seen.insert(eng.next_card().id);
            }
            assert_eq!(seen.len(), n, "case {k}: deck not a fresh full round: {c}");
        }
        // A consistent state is still resumed.
        std::fs::write(&state, good.to_string()).unwrap();
        let restored: Deck = serde_json::from_value(good).unwrap();
        assert!(restored.is_consistent_with(n));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ffi_survives_null_and_bad_input() {
        use std::ffi::CString;
        unsafe {
            assert!(ffi::wz_engine_new(std::ptr::null(), std::ptr::null()).is_null());
            let missing = CString::new("/nonexistent/lexicon.json").unwrap();
            assert!(ffi::wz_engine_new(missing.as_ptr(), std::ptr::null()).is_null());
            assert!(ffi::wz_engine_next_card_json(std::ptr::null_mut()).is_null());
            assert_eq!(ffi::wz_engine_count(std::ptr::null()), 0);
            ffi::wz_string_free(std::ptr::null_mut());
            ffi::wz_engine_free(std::ptr::null_mut());
            assert!(ffi::guard(std::ptr::null_mut::<u8>(), || panic!("boom")).is_null());
        }
    }

    #[test]
    fn engine_persists_progress() {
        let dir = std::env::temp_dir().join(format!("wz-test-{}", std::process::id()));
        let state = dir.join("deck.json");
        let lex = Lexicon {
            meta: serde_json::Value::Null,
            entries: (0..10).map(|i| entry(&format!("W{i}"), "noun")).collect(),
        };
        let mut a = Engine::new(lex.clone(), Some(state.clone()));
        let first: Vec<String> = (0..4).map(|_| a.next_card().id).collect();
        let mut b = Engine::new(lex, Some(state.clone()));
        let next = b.next_card();
        assert!(!first.contains(&next.id), "resumed deck repeated a card from the same round");
        assert_eq!(next.serial, 5);
        let _ = std::fs::remove_dir_all(dir);
    }
}
