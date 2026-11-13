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
    #[serde(default)]
    pub freq_rank: Option<u32>,
    #[serde(default)]
    pub source: BTreeMap<String, String>,
}

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
                    (e.gender.as_deref(), e.article.as_deref()),
                    (Some("masculine"), Some("der")) | (Some("feminine"), Some("die")) | (Some("neuter"), Some("das"))
                );
                if !ok {
                    problems.push(format!("{at}: noun without consistent gender/article"));
                }
            }
            if !(e.ipa.starts_with('[') || e.ipa.starts_with('/')) {
                problems.push(format!("{at}: IPA not bracketed: {}", e.ipa));
            }
        }
        problems
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
    let (forms_label, forms_line) = match e.pos.as_str() {
        "noun" => match f("plural") {
            Some(p) => (Some("Plural".to_string()), Some(format!("die {p}"))),
            None => (None, None),
        },
        "verb" => {
            let mut parts = Vec::new();
            if let Some(p3) = f("present3") {
                parts.push(format!("er {p3}"));
            }
            if let Some(past) = f("past") {
                parts.push(past);
            }
            if let Some(pp) = f("participle") {
                let aux = f("auxiliary").unwrap_or_default();
                let aux = aux
                    .split('/')
                    .map(|a| match a.trim() {
                        "haben" => "hat",
                        "sein" => "ist",
                        _ => "",
                    })
                    .filter(|a| !a.is_empty())
                    .collect::<Vec<_>>()
                    .join("/");
                parts.push(if aux.is_empty() { pp } else { format!("{aux} {pp}") });
            }
            if parts.is_empty() { (None, None) } else { (Some("Konjugation".into()), Some(parts.join(" · "))) }
        }
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
        source: e.source.clone(),
    }
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
            .filter(|d| d.len == n && d.order.len() == n)
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
    fn tiny_decks_work() {
        let mut d = Deck::new(1, Rng::new(3));
        assert_eq!(d.next_index(), 0);
        assert_eq!(d.next_index(), 0);
        let mut d = Deck::new(2, Rng::new(3));
        let a = d.next_index();
        let b = d.next_index();
        assert_ne!(a, b);
    }

    #[test]
    fn verb_forms_line() {
        let mut e = entry("gehen", "verb");
        e.forms.insert("present3".into(), "geht".into());
        e.forms.insert("past".into(), "ging".into());
        e.forms.insert("participle".into(), "gegangen".into());
        e.forms.insert("auxiliary".into(), "sein".into());
        let c = card_for(&e, 1);
        assert_eq!(c.forms_line.as_deref(), Some("er geht · ging · ist gegangen"));
        e.forms.insert("auxiliary".into(), "haben / sein".into());
        assert_eq!(card_for(&e, 1).forms_line.as_deref(), Some("er geht · ging · hat/ist gegangen"));
    }

    #[test]
    fn noun_plural_and_validation() {
        let mut e = entry("Haus", "noun");
        e.forms.insert("plural".into(), "Häuser".into());
        assert_eq!(card_for(&e, 1).forms_line.as_deref(), Some("die Häuser"));
        let lex = Lexicon { meta: serde_json::Value::Null, entries: vec![e.clone(), e] };
        assert!(lex.validate().iter().any(|p| p.contains("duplicate")));
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
