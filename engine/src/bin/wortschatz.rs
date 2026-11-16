//! `wortschatz` CLI: validate a lexicon or print cards the way a shell would receive them.
//!
//!   wortschatz validate core/data/lexicon.json
//!   wortschatz next core/data/lexicon.json [count]
//!   wortschatz stats core/data/lexicon.json
//!   wortschatz card core/data/lexicon.json <id|word>...
//!   wortschatz cards core/data/lexicon.json        (every card, one JSON per line)
use std::path::Path;
use wortschatz_core::{card_for, Engine, Lexicon};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: wortschatz <validate|next|stats|card|cards> <lexicon.json> [count|id...]";
    if args.len() < 3 {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    let lex = match Lexicon::load(Path::new(&args[2])) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    match args[1].as_str() {
        "validate" => {
            let problems = lex.validate();
            for p in &problems {
                println!("{p}");
            }
            println!("{} entries, {} problems", lex.entries.len(), problems.len());
            std::process::exit(if problems.is_empty() { 0 } else { 1 });
        }
        "next" => {
            let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
            let mut eng = Engine::new(lex, None);
            for _ in 0..n {
                println!("{}", serde_json::to_string(&eng.next_card()).unwrap());
            }
        }
        "stats" => {
            let mut by_pos = std::collections::BTreeMap::new();
            for e in &lex.entries {
                *by_pos.entry(e.pos.as_str()).or_insert(0usize) += 1;
            }
            println!("{} entries {:?}", lex.entries.len(), by_pos);
            let pct = |a: usize, b: usize| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
            let nouns: Vec<_> = lex.entries.iter().filter(|e| e.pos == "noun").collect();
            let with_plural = nouns.iter().filter(|e| e.number.as_deref() != Some("plural_only") && e.forms.contains_key("plural")).count();
            let sg = nouns.iter().filter(|e| e.number.as_deref() == Some("singular_only")).count();
            let pl = nouns.iter().filter(|e| e.number.as_deref() == Some("plural_only")).count();
            println!(
                "nouns: {with_plural}/{} with plural ({:.1}%), {sg} singular-only, {pl} plural-only, {} unmarked",
                nouns.len(),
                pct(with_plural, nouns.len()),
                nouns.len() - with_plural - sg - pl
            );
            let verbs: Vec<_> = lex.entries.iter().filter(|e| e.pos == "verb").collect();
            let full = verbs.iter().filter(|e| wortschatz_core::principal_parts(e).is_some()).count();
            let sep = verbs.iter().filter(|e| e.separable.is_some()).count();
            let refl = verbs.iter().filter(|e| e.reflexive).count();
            println!("verbs: {full}/{} with principal parts ({:.1}%), {sep} separable, {refl} reflexive", verbs.len(), pct(full, verbs.len()));
            let ex = lex.entries.iter().filter(|e| e.example.is_some()).count();
            println!("examples: {ex}/{} ({:.1}%)", lex.entries.len(), pct(ex, lex.entries.len()));
        }
        "cards" => {
            // Every entry's card (serial 1), one JSON object per line: tools/check-parity.mjs
            // compares these with the JavaScript mirror in core/app.js.
            for e in &lex.entries {
                println!("{}", serde_json::to_string(&card_for(e, 1)).unwrap());
            }
        }
        "card" => {
            for q in &args[3..] {
                match lex.entries.iter().find(|e| &e.id == q || &e.word == q) {
                    Some(e) => println!("{}", serde_json::to_string(&card_for(e, 1)).unwrap()),
                    None => eprintln!("{q}: not found"),
                }
            }
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}
