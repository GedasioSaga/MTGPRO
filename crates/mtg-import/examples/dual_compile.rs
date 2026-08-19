//! Mede, sobre o bulk real, quantas cartas cada um dos DOIS compiladores do
//! repositório aceita: o do importador (`mtg_import::compile`) e o do crate
//! `mtg-oracle` (`mtg_oracle::compile`). Existe para responder, com número, se
//! o segundo acrescenta cobertura ao primeiro — e nao para entrar no produto.
use std::collections::HashSet;

use mtg_import::compile::compile_card;
use mtg_import::scryfall::{self, reject_reason, ScryfallCard};
use mtg_oracle::{CompileResult, OracleCard};

fn to_oracle(card: &ScryfallCard) -> OracleCard {
    OracleCard {
        name: card.face_name(None).to_string(),
        mana_cost: card.face_mana_cost(None).unwrap_or("").to_string(),
        type_line: card.face_type_line(None).unwrap_or("").to_string(),
        oracle_text: card.face_oracle_text(None).to_string(),
        power: card.face_power(None).map(str::to_string),
        toughness: card.face_toughness(None).map(str::to_string),
        loyalty: card.face_loyalty(None).map(str::to_string),
        rarity: card.rarity.clone().unwrap_or_default(),
        set_code: card.set.clone().unwrap_or_default(),
        collector_number: card.collector_number.clone().unwrap_or_default(),
        artist: card.face_artist(None).map(str::to_string),
        flavor_text: None,
        art_key: None,
        layout: card.layout.clone().unwrap_or_default(),
    }
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
    let (mut only_imp, mut only_ora, mut both, mut neither) = (0u64, 0u64, 0u64, 0u64);
    let mut seen: HashSet<String> = HashSet::new();
    let mut index = 0u32;
    let mut samples_only_ora: Vec<String> = Vec::new();
    for entry in stream {
        let Ok(card) = entry else { continue };
        if reject_reason(&card).is_some() {
            continue;
        }
        if !seen.insert(card.name.clone().unwrap_or_default()) {
            continue;
        }
        let imp = compile_card(&card, index).playable;
        index = index.saturating_add(1);
        let ora = matches!(mtg_oracle::compile(&to_oracle(&card)), CompileResult::Playable(_));
        match (imp, ora) {
            (true, true) => both += 1,
            (true, false) => only_imp += 1,
            (false, true) => {
                only_ora += 1;
                if samples_only_ora.len() < 40 {
                    samples_only_ora.push(card.name.clone().unwrap_or_default());
                }
            }
            (false, false) => neither += 1,
        }
    }
    println!("so importador ... {only_imp}");
    println!("so oracle ....... {only_ora}");
    println!("ambos ........... {both}");
    println!("nenhum .......... {neither}");
    println!("importador total  {}", only_imp + both);
    println!("oracle total .... {}", only_ora + both);
    println!("amostra so-oracle: {samples_only_ora:?}");
}
