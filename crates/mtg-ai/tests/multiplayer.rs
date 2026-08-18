//! Prova numérica de que o bot joga mesa cheia, não só duelo.
//!
//! O teste que importa é `heuristico_vence_bem_acima_do_acaso_em_mesa_de_quatro`:
//! um `HeuristicBot` contra três `RandomBot` numa partida de quatro. O acaso
//! puro dá 25%; o piso exigido é 45%. Se o número cair, o remédio é melhorar o
//! bot — afrouxar o piso não prova nada.
//!
//! Roda com o catálogo em Lua carregado, então é `#[ignore]`:
//! `cargo test -p mtg-ai --test multiplayer -- --ignored --nocapture`
use std::sync::Arc;

use mtg_core::card::CardDatabase;
use mtg_core::engine::{Agent, Game, GameConfig, GameFormat, PlayerConfig};
use mtg_core::ids::{CardDefId, PlayerId};
use mtg_core::state::GameOutcome;
use mtg_core::zone::ZoneKind;

use mtg_ai::table;

/// Jogadores por mesa no teste de força.
const SEATS: usize = 4;
/// Partidas rodadas. Cada uma troca a cadeira do heurístico.
const MATCHES: u64 = 400;
/// Piso exigido. Acaso puro numa mesa de quatro é 25%.
const FLOOR: f64 = 0.45;

fn database() -> Arc<CardDatabase> {
    match mtg_cards::build_database() {
        Ok(db) => Arc::new(db),
        Err(err) => panic!("catálogo não carregou: {err:?}"),
    }
}

fn constructed_lists(db: &CardDatabase) -> Vec<Vec<CardDefId>> {
    mtg_cards::decks()
        .iter()
        .map(|d| match d.expand(db) {
            Some(cards) => cards,
            None => panic!("deck {} não expandiu", d.name),
        })
        .collect()
}

fn config() -> GameConfig {
    GameConfig {
        // Mesa de quatro demora mais que duelo: sem folga de turno, quase toda
        // partida terminaria em empate por limite e o teste mediria o limite.
        max_turns: 160,
        ..GameConfig::default()
    }
}

/// Monta e joga uma mesa de quatro. `heuristic_seat` diz em qual cadeira o
/// heurístico senta; as outras três são `RandomBot`.
fn table_of_four(
    db: Arc<CardDatabase>,
    list: &[CardDefId],
    heuristic_seat: usize,
    seed: u64,
) -> GameOutcome {
    let mut players = Vec::with_capacity(SEATS);
    let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(SEATS);
    for seat in 0..SEATS {
        players.push(PlayerConfig::new(format!("P{seat}"), list.to_vec()));
        let kind = if seat == heuristic_seat {
            "heuristic"
        } else {
            "random"
        };
        // Sementes derivadas distintas: três `RandomBot` com a mesma semente
        // sortariam o mesmo índice nas mesmas posições e a mesa viraria espelho.
        let bot_seed = seed ^ ((seat as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let Some(agent) = mtg_ai::bot_by_name(kind, bot_seed) else {
            panic!("bot {kind} não construiu");
        };
        agents.push(agent);
    }
    match Game::new(db, players, agents, config(), seed) {
        Ok(mut game) => game.run(),
        Err(err) => panic!("Game::new falhou com semente {seed}: {err:?}"),
    }
}

#[test]
fn mesa_de_quatro_e_deterministica() {
    // Determinismo é requisito duro: sem ele o número do teste de força não
    // significa nada, porque não daria para reproduzir a partida que gerou.
    let db = database();
    let lists = constructed_lists(&db);
    let Some(list) = lists.first() else {
        panic!("catálogo não trouxe nenhum deck construído");
    };
    for seed in [5u64, 17] {
        let first = table_of_four(Arc::clone(&db), list, 0, seed);
        let again = table_of_four(Arc::clone(&db), list, 0, seed);
        assert_eq!(first, again, "semente {seed}: mesa de quatro não repetiu");
    }
}

#[test]
fn mesa_de_quatro_termina_com_um_vivo_ou_empate() {
    // CR 104.2a — a partida acaba quando sobra um. Sem isso, o teste de força
    // estaria medindo partidas que o motor abandonou pela metade.
    let db = database();
    let lists = constructed_lists(&db);
    let Some(list) = lists.first() else {
        panic!("catálogo não trouxe nenhum deck construído");
    };
    let outcome = table_of_four(Arc::clone(&db), list, 0, 9);
    match outcome {
        GameOutcome::Winner(p) => {
            assert!(
                (p.0 as usize) < SEATS,
                "vencedor {p:?} fora das quatro cadeiras"
            );
        }
        GameOutcome::Draw => {}
        GameOutcome::Ongoing => panic!("partida de quatro devolveu Ongoing"),
    }
}

#[test]
#[ignore = "50 partidas de quatro jogadores: lento demais para a suíte padrão"]
fn heuristico_vence_bem_acima_do_acaso_em_mesa_de_quatro() {
    let db = database();
    let lists = constructed_lists(&db);
    assert!(!lists.is_empty(), "catálogo sem decks construídos");

    let mut wins = 0usize;
    let mut losses = 0usize;
    let mut draws = 0usize;
    for seed in 0..MATCHES {
        // A cadeira rotaciona: numa mesa de quatro a ordem de turno é vantagem
        // real, e sem rodízio o teste mediria a cadeira, não o bot.
        let seat = (seed as usize) % SEATS;
        // Os quatro jogadores usam o mesmo deck na mesma partida: o que sobra
        // de diferença entre eles é decisão, não lista de cartas.
        let list = &lists[(seed as usize / SEATS) % lists.len()];
        let outcome = table_of_four(Arc::clone(&db), list, seat, seed);
        match outcome {
            GameOutcome::Winner(p) if p.index() == seat => wins += 1,
            GameOutcome::Winner(_) => losses += 1,
            _ => draws += 1,
        }
    }

    let rate = wins as f64 / MATCHES as f64;
    // Impressa sempre: a taxa medida é o resultado do teste, não só o fato de
    // ele passar. `--ignored --nocapture` mostra.
    println!(
        "heurístico x 3 aleatórios em {MATCHES} mesas de quatro: {wins} vitórias ({:.0}%), \
         {losses} derrotas, {draws} empates",
        rate * 100.0
    );
    assert!(
        rate >= FLOOR,
        "heurístico venceu {wins}/{MATCHES} ({:.0}%), perdeu {losses}, empatou {draws} — \
         abaixo do piso de {:.0}% (acaso puro seria 25%). Melhore o bot, não o teste.",
        rate * 100.0,
        FLOOR * 100.0
    );
}

// ---------------------------------------------------------------------------
// CR 903.10 — relógio de comandante
// ---------------------------------------------------------------------------

/// Monta uma mesa de Commander parada no estado inicial, sem jogar. Basta para
/// exercitar a leitura do relógio: o dano é escrito à mão logo abaixo, porque
/// forçar 21 pontos de dano de comandante numa partida de verdade dependeria da
/// mão sorteada e o teste deixaria de afirmar o que se propõe.
fn commander_table(db: Arc<CardDatabase>) -> Game {
    let Some(list) = mtg_cards::commander_decks().first().cloned() else {
        panic!("catálogo não trouxe deck de Commander");
    };
    let Some(deck) = list.expand(&db) else {
        panic!("deck de Commander {} não expandiu", list.name);
    };
    let Some(commander) = list.commander_id(&db) else {
        panic!("deck de Commander {} não resolveu o comandante", list.name);
    };
    let players: Vec<PlayerConfig> = (0..SEATS)
        .map(|seat| PlayerConfig {
            name: format!("P{seat}"),
            deck: deck.clone(),
            commander: Some(commander),
        })
        .collect();
    let agents: Vec<Box<dyn Agent>> = (0..SEATS)
        .map(|seat| match mtg_ai::bot_by_name("heuristic", seat as u64 + 1) {
            Some(a) => a,
            None => panic!("bot heuristic não construiu"),
        })
        .collect();
    let config = GameConfig {
        max_turns: 160,
        ..GameConfig::for_format(GameFormat::Commander)
    };
    match Game::new(db, players, agents, config, 42) {
        Ok(game) => game,
        Err(err) => panic!("Game::new de Commander falhou: {err:?}"),
    }
}

#[test]
fn comandante_comeca_na_zona_de_comando_em_mesa_de_quatro() {
    // CR 903.6. É a pré-condição de tudo o que a heurística lê depois: sem
    // comandante designado, o relógio de `table` seria zero por acidente.
    let db = database();
    let game = commander_table(db);
    for seat in 0..SEATS {
        let player = PlayerId(seat as u8);
        let zone = mtg_core::zone::ZoneId {
            kind: ZoneKind::Command,
            owner: Some(player),
        };
        let objects = mtg_ai::eval::zone_objects(&game.state, zone);
        assert_eq!(
            objects.len(),
            1,
            "cadeira {seat} não tem exatamente um comandante na zona de comando"
        );
    }
}

#[test]
fn relogio_de_comandante_e_lido_por_dono_e_por_vitima() {
    // CR 903.10 — 21 de um **mesmo** comandante. A heurística pergunta "quanto
    // o jogador B já tirou de mim?", então a leitura tem de indexar por dono e
    // devolver o máximo, nunca a soma de comandantes diferentes.
    let db = database();
    let mut game = commander_table(db);
    let me = PlayerId(0);
    let b = PlayerId(1);
    let c = PlayerId(2);

    assert_eq!(
        table::commander_damage_between(&game.state, b, me),
        0,
        "relógio nasceu diferente de zero"
    );

    let Some(cmd_b) = game.state.player(b).commander else {
        panic!("jogador B não tem comandante designado");
    };
    let Some(cmd_c) = game.state.player(c).commander else {
        panic!("jogador C não tem comandante designado");
    };

    mtg_core::engine::commander::note_combat_damage(&mut game.state, cmd_b, me, 13);
    mtg_core::engine::commander::note_combat_damage(&mut game.state, cmd_c, me, 8);

    assert_eq!(
        table::commander_damage_between(&game.state, b, me),
        13,
        "dano do comandante de B contra mim saiu errado"
    );
    assert_eq!(
        table::commander_damage_between(&game.state, c, me),
        8,
        "dano do comandante de C contra mim saiu errado"
    );
    assert_eq!(
        table::commander_damage_between(&game.state, me, b),
        0,
        "relógio inverteu de direção"
    );

    // Fecha os 21 de B e confirma que o motor concorda que isso é letal.
    mtg_core::engine::commander::note_combat_damage(&mut game.state, cmd_b, me, 8);
    assert_eq!(table::commander_damage_between(&game.state, b, me), 21);
    assert_eq!(
        mtg_core::engine::commander::lethal_commander_damage(&game.state),
        vec![me],
        "21 de um mesmo comandante não foi reconhecido como letal"
    );
}

#[test]
fn heuristica_ve_o_relogio_de_comandante_no_retrato_da_mesa() {
    // O ponto de junção: o número do motor tem de chegar ao `Snapshot`, senão
    // a avaliação segue cega para o segundo relógio de vida.
    let db = database();
    let mut game = commander_table(db);
    let me = PlayerId(0);
    let b = PlayerId(1);
    let Some(cmd_b) = game.state.player(b).commander else {
        panic!("jogador B não tem comandante designado");
    };
    mtg_core::engine::commander::note_combat_damage(&mut game.state, cmd_b, me, 17);

    let snapshot = mtg_ai::eval::Snapshot::from_game(&game, me);
    let views = snapshot.opponents_view();
    let Some(view) = views.iter().find(|o| o.id == b) else {
        panic!("jogador B não apareceu no retrato da mesa");
    };
    assert_eq!(
        view.commander_damage_to_me, 17,
        "o retrato não trouxe o relógio de comandante de B"
    );
    assert_eq!(views.len(), SEATS - 1, "retrato perdeu algum oponente");
}
