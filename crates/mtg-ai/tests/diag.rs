//! Diagnóstico temporário: por que as mesas de quatro empatam.
use std::sync::Arc;
use mtg_core::card::CardDatabase;
use mtg_core::engine::{Agent, Game, GameConfig, PlayerConfig};
use mtg_core::ids::CardDefId;
use mtg_core::state::GameOutcome;

fn database() -> Arc<CardDatabase> {
    match mtg_cards::build_database() { Ok(db) => Arc::new(db), Err(e) => panic!("{e:?}") }
}

#[test]
#[ignore]
fn diag() {
    let db = database();
    let lists: Vec<Vec<CardDefId>> = mtg_cards::decks().iter().map(|d| d.expand(&db).unwrap_or_default()).collect();
    let mut draw_turns = Vec::new();
    let mut draw_alive = Vec::new();
    let mut win_turns = Vec::new();
    let mut all_random_draws = 0;
    for seed in 0..200u64 {
        let seat = (seed as usize) % 4;
        let list = &lists[(seed as usize / 4) % lists.len()];
        for all_random in [false, true] {
            let mut players = Vec::new();
            let mut agents: Vec<Box<dyn Agent>> = Vec::new();
            for s in 0..4usize {
                players.push(PlayerConfig::new(format!("P{s}"), list.clone()));
                let kind = if !all_random && s == seat { "heuristic" } else { "random" };
                let bs = seed ^ ((s as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15));
                agents.push(mtg_ai::bot_by_name(kind, bs).unwrap_or_else(|| panic!("bot")));
            }
            let cfg = GameConfig { max_turns: 80, ..GameConfig::default() };
            let mut game = match Game::new(Arc::clone(&db), players, agents, cfg, seed) { Ok(g)=>g, Err(e)=>panic!("{e:?}") };
            let out = game.run();
            let alive = game.state.players.iter().filter(|p| !p.has_lost).count();
            if all_random {
                if matches!(out, GameOutcome::Draw) { all_random_draws += 1; }
            } else {
                match out {
                    GameOutcome::Draw => { draw_turns.push(game.state.turn); draw_alive.push(alive); }
                    _ => win_turns.push(game.state.turn),
                }
            }
        }
    }
    let avg = |v: &Vec<u32>| if v.is_empty() {0.0} else { v.iter().map(|x| *x as f64).sum::<f64>() / v.len() as f64 };
    println!("empates={} turno medio={:.1} vivos medio={:.2}", draw_turns.len(), avg(&draw_turns),
        draw_alive.iter().map(|x| *x as f64).sum::<f64>() / draw_alive.len().max(1) as f64);
    println!("decididas={} turno medio={:.1}", win_turns.len(), avg(&win_turns));
    println!("mesa 100% aleatoria: {all_random_draws}/200 empates");
    let mut hist = [0usize; 5];
    for a in &draw_alive { hist[(*a).min(4)] += 1; }
    println!("vivos no empate: {hist:?}");
}
