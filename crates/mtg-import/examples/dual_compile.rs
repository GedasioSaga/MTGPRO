//! Cobertura por pool ANTES e DEPOIS da segunda passada, sobre o bulk real.
//!
//! "Antes" nao e uma execucao separada: e a mesma execucao contando so as
//! cartas cuja IR veio do compilador deste crate (`second_pass == false`). A
//! segunda passada so pode acrescentar, entao a diferenca e exatamente o que
//! ela comprou — e a comparacao nao depende de rebuildar o binario antigo.
use std::collections::HashSet;

use mtg_import::compile::compile_card;
use mtg_import::scryfall::{self, reject_reason, ScryfallCard};
use mtg_oracle::coverage::{self, Pool, PoolMask};

fn pool_mask_of(card: &ScryfallCard) -> PoolMask {
    let rarity = card.rarity.as_deref().unwrap_or_default();
    coverage::pools_of(rarity, |format| {
        card.legalities.as_ref().and_then(|m| m.get(format)).map(String::as_str)
    })
}

fn main() {
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
    let mut total = [0u64; 5];
    let mut before = [0u64; 5];
    let mut after = [0u64; 5];
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
        let mask = pool_mask_of(&card);
        for (i, pool) in Pool::ALL.iter().enumerate() {
            if !mask.contains(*pool) {
                continue;
            }
            total[i] += 1;
            if c.playable {
                after[i] += 1;
                if !c.second_pass {
                    before[i] += 1;
                }
            }
        }
    }
    println!("| Pool | No catalogo | Antes | Depois | % antes | % depois |");
    println!("|---|---|---|---|---|---|");
    for (i, pool) in Pool::ALL.iter().enumerate() {
        let pct = |n: u64| if total[i] == 0 { 0.0 } else { n as f64 * 100.0 / total[i] as f64 };
        println!(
            "| {} | {} | {} | {} | {:.1}% | {:.1}% |",
            pool.label(),
            total[i],
            before[i],
            after[i],
            pct(before[i]),
            pct(after[i])
        );
    }
}
