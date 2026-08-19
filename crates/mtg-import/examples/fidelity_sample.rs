//! Sorteia, com semente fixa, N cartas que so viraram jogaveis pela SEGUNDA
//! PASSADA e imprime o texto de oraculo ao lado do IR gerado, para conferencia
//! humana. Determinista: mesma entrada, mesma amostra.
use std::collections::HashSet;

use mtg_import::compile::compile_card;
use mtg_import::scryfall::{self, reject_reason};

/// SplitMix64 — gerador determinista, sem dependencia externa.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

const SEED: u64 = 2026_08_18;

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let path = std::path::Path::new(".cache/scryfall/oracle_cards.jsonl.gz");
    let stream = match scryfall::stream_cards(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sem bulk: {e}");
            return;
        }
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut index = 0u32;
    let mut pool: Vec<(String, String, String)> = Vec::new();
    for entry in stream {
        let Ok(card) = entry else { continue };
        if reject_reason(&card).is_some() {
            continue;
        }
        if !seen.insert(card.name.clone().unwrap_or_default()) {
            continue;
        }
        let c = compile_card(&card, index);
        index = index.saturating_add(1);
        if !c.second_pass {
            continue;
        }
        let ir = format!(
            "type_line={:?}\n  mana_cost={:?}\n  pt={:?}/{:?}\n  abilities={:#?}\n  spell_effect={:#?}\n  spell_targets={:#?}",
            c.def.type_line.to_string(),
            c.def.mana_cost,
            c.def.power,
            c.def.toughness,
            c.def.abilities,
            c.def.spell_effect,
            c.def.spell_targets
        );
        pool.push((c.def.name.clone(), c.def.oracle_text.clone(), ir));
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0));
    println!("universo de cartas novas na segunda passada: {}", pool.len());
    if pool.is_empty() {
        return;
    }
    let mut rng = Rng(SEED);
    let mut picked: Vec<usize> = Vec::new();
    while picked.len() < n.min(pool.len()) {
        let i = (rng.next() % pool.len() as u64) as usize;
        if !picked.contains(&i) {
            picked.push(i);
        }
    }
    picked.sort_unstable();
    for (k, i) in picked.iter().enumerate() {
        let (name, text, ir) = &pool[*i];
        println!("\n=== {:>2}. {name} ===", k + 1);
        println!("TEXTO: {}", text.replace('\n', "\nTEXTO: "));
        println!("IR:    {ir}");
    }
}
