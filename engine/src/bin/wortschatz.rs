//! `wortschatz` CLI: validate a lexicon or print cards the way a shell would receive them.
//!
//!   wortschatz validate core/data/lexicon.json
//!   wortschatz next core/data/lexicon.json [count]
//!   wortschatz stats core/data/lexicon.json
use std::path::Path;
use wortschatz_core::{Engine, Lexicon};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: wortschatz <validate|next|stats> <lexicon.json> [count]";
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
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}
