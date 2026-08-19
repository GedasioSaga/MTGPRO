//! Fumaça de multijogador: 100 mesas de Commander com quatro jogadores.
//!
//! O que este arquivo protege, e que nenhum teste de módulo isolado pega:
//!
//!   1. **Nada entra em pânico.** O motor foi escrito para dois e generalizado
//!      para quatro; índice de jogador, `next_player` e alvo de ataque são os
//!      lugares onde a passagem de 2 para 4 quebra em silêncio.
//!   2. **Toda partida termina.** CR 104.2a — acaba quando sobra um. Ganhar
//!      uma mesa de quatro exige eliminar três, então uma partida que não
//!      converge aparece aqui como `Ongoing` ou como estouro do teto.
//!   3. **Quem sai não deixa objeto órfão.** CR 800.4a — o jogador que deixa a
//!      partida leva junto tudo que possui e todo efeito de controle dele. Um
//!      permanente controlado por quem já saiu é o bug clássico do salto de
//!      duelo para mesa: o objeto continua atacando, bloqueando e contando
//!      para a regra da lenda de um jogador que não existe mais.
//!
//! Os bots são aleatórios de propósito: o objetivo é varrer caminho de regra,
//! não medir força de IA (isso é `mtg-ai/tests/multiplayer.rs`). Aleatório
//! semeado explora ramos que um bot bom nunca escolhe — exatamente onde o
//! pânico se esconde.
mod common;

use std::sync::Arc;

use mtg_core::action::{Action, Request};
use mtg_core::card::CardDatabase;
use mtg_core::engine::{
    commander, layers, sba, turn, Agent, Game, GameConfig, GameFormat, PlayerConfig,
};
use mtg_core::event::LossReason;
use mtg_core::ids::{CardDefId, ObjectId, PlayerId};
use mtg_core::ir::{Duration, StaticModRuntime};
use mtg_core::state::{ContinuousEffect, GameOutcome, GameState};
use mtg_core::zone::{ZoneId, ZoneKind};

/// Jogadores por mesa.
const SEATS: usize = 4;
/// Sementes varridas: 0..SEEDS. Semente é o identificador da partida — quando
/// uma falha, o número no pânico reproduz a mesa exata.
const SEEDS: u64 = 100;
/// Teto de turnos. Mesa de quatro precisa de três eliminações, e com bot
/// aleatório isso demora; com o teto de duelo (60) a medida seria do relógio,
/// não do motor.
const MAX_TURNS: u32 = 160;

// ---------------------------------------------------------------------------
// Montagem
// ---------------------------------------------------------------------------

fn database() -> Arc<CardDatabase> {
    match mtg_cards::build_database() {
        Ok(db) => Arc::new(db),
        Err(err) => panic!("catálogo de cartas não carregou: {err:?}"),
    }
}

/// Os decks de Commander do catálogo, já em ids, com o id do comandante.
///
/// `expand` devolve a **biblioteca** (99 cartas): o comandante começa na zona
/// de comando (CR 903.6) e por isso vem separado.
fn commander_lists(db: &CardDatabase) -> Vec<(Vec<CardDefId>, CardDefId)> {
    let lists = mtg_cards::commander_decks();
    assert!(!lists.is_empty(), "catálogo não trouxe deck de Commander");
    lists
        .iter()
        .map(|list| {
            let Some(cards) = list.expand(db) else {
                panic!("deck {} não expandiu: carta faltando no catálogo", list.name);
            };
            let Some(commander) = list.commander_id(db) else {
                panic!("deck {} não declara comandante (CR 903.3)", list.name);
            };
            (cards, commander)
        })
        .collect()
}

fn config() -> GameConfig {
    GameConfig {
        max_turns: MAX_TURNS,
        ..GameConfig::for_format(GameFormat::Commander)
    }
}

// ---------------------------------------------------------------------------
// O vigia do CR 800.4a
// ---------------------------------------------------------------------------

/// Objetos que um jogador que já saiu ainda tem em zona pública.
///
/// Vale para campo de batalha e pilha, que são as zonas onde um objeto órfão é
/// observável pelos que ficaram (CR 800.4a manda tirar tudo, mas mão e
/// biblioteca de quem saiu não influenciam mais nada). Conta tanto por
/// controlador quanto por dono: efeito de roubo de controle deixa o objeto no
/// campo com dono morto, e é bug igual.
fn orphans_of(state: &GameState, player: PlayerId) -> Vec<String> {
    let mut out = Vec::new();
    for object in state.battlefield().objects.iter().filter_map(|id| state.object(*id)) {
        if object.controller == player {
            out.push(format!("permanente {:?} controlado por {player:?}", object.id));
        } else if object.owner == player {
            out.push(format!("permanente {:?} de {player:?} no campo", object.id));
        }
    }
    for item in &state.stack {
        if item.controller == player {
            out.push(format!("item de pilha controlado por {player:?}"));
        }
    }
    out
}

/// Jogadores que já perderam e ainda têm objeto em zona pública.
fn players_with_orphans(state: &GameState) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .filter(|p| p.has_lost)
        .filter(|p| !orphans_of(state, p.id).is_empty())
        .map(|p| p.id)
        .collect()
}

/// Bot aleatório semeado que confere o CR 800.4a a cada decisão.
///
/// Por que aqui e não no fim da partida: `leave_game` roda no instante da
/// eliminação, e é durante o resto da partida que um objeto órfão faria
/// estrago. Checar só no estado final não veria nada — o jogo já teria acabado
/// e o resultado, já decidido.
struct WatchdogAgent {
    inner: common::RandomAgent,
    seed: u64,
}

impl Agent for WatchdogAgent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        // Com a partida encerrada a limpeza não é mais exigida: CR 800.4a é
        // sobre quem sai de uma partida que continua. O último eliminado
        // encerra o jogo e fica com as coisas dele onde estavam.
        if !game.state.is_over() {
            // Primeiro infrator basta: o pânico já carrega semente e turno, e é
            // com eles que se reproduz a mesa inteira.
            if let Some(player) = players_with_orphans(&game.state).first().copied() {
                let detail = orphans_of(&game.state, player).join("; ");
                panic!(
                    "semente {}: turno {}, {player:?} saiu e deixou órfão (CR 800.4a): {detail}",
                    self.seed, game.state.turn
                );
            }
        }
        self.inner.decide(game, request, legal)
    }
}

// ---------------------------------------------------------------------------
// Uma mesa
// ---------------------------------------------------------------------------

/// O que uma partida rendeu de mensurável.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableResult {
    outcome: GameOutcome,
    turns: u32,
    /// Quantos jogadores foram eliminados até o fim.
    eliminated: usize,
    /// Quantos jogadores eliminados ainda tinham objeto no fim. CR 800.4a só
    /// roda enquanto a partida continua, então o último eliminado conta aqui.
    leftover: usize,
}

fn play_table(
    db: Arc<CardDatabase>,
    lists: &[(Vec<CardDefId>, CardDefId)],
    seed: u64,
) -> TableResult {
    let mut players = Vec::with_capacity(SEATS);
    let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(SEATS);
    for seat in 0..SEATS {
        let (deck, commander) = &lists[seat % lists.len()];
        players.push(PlayerConfig {
            name: format!("P{seat}"),
            deck: deck.clone(),
            commander: Some(*commander),
        });
        // Semente por assento: quatro bots com a mesma semente sorteiam o
        // mesmo índice nas mesmas posições e a mesa vira espelho. Continua
        // função pura da semente da partida, então o replay não quebra.
        let bot_seed = seed ^ ((seat as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        agents.push(Box::new(WatchdogAgent {
            inner: common::RandomAgent::new(&format!("P{seat}"), bot_seed),
            seed,
        }));
    }

    let mut game = match Game::new(db, players, agents, config(), seed) {
        Ok(g) => g,
        Err(err) => panic!("semente {seed}: Game::new falhou: {err:?}"),
    };
    let outcome = game.run();
    let eliminated = game.state.players.iter().filter(|p| p.has_lost).count();
    TableResult {
        outcome,
        turns: game.state.turn,
        eliminated,
        leftover: players_with_orphans(&game.state).len(),
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[test]
fn cem_mesas_de_commander_com_quatro_jogadores_terminam_sem_panico_nem_orfao() {
    let db = database();
    let lists = commander_lists(&db);

    let mut winners = 0usize;
    let mut draws = 0usize;
    let mut total_turns = 0u64;
    let mut max_turns_seen = 0u32;
    let mut total_eliminated = 0usize;
    let mut by_seat = [0usize; SEATS];

    for seed in 0..SEEDS {
        let r = play_table(Arc::clone(&db), &lists, seed);

        // (2) A partida tem de ter um resultado. `Ongoing` no fim de `run`
        // significa que o laço de turnos saiu sem decidir nada — é bug, não
        // empate.
        assert_ne!(
            r.outcome,
            GameOutcome::Ongoing,
            "semente {seed}: partida terminou sem resultado"
        );
        assert!(
            r.turns <= MAX_TURNS,
            "semente {seed}: {} turnos passam do teto de {MAX_TURNS}",
            r.turns
        );

        match r.outcome {
            GameOutcome::Winner(w) => {
                winners += 1;
                let index = w.index();
                assert!(index < SEATS, "semente {seed}: vencedor {w:?} fora da mesa");
                by_seat[index] += 1;
                // CR 104.2a — vencer é sobrar; os outros três saíram.
                assert_eq!(
                    r.eliminated,
                    SEATS - 1,
                    "semente {seed}: venceu com {} eliminados",
                    r.eliminated
                );
            }
            GameOutcome::Draw => draws += 1,
            GameOutcome::Ongoing => unreachable!("checado acima"),
        }

        // (3) CR 800.4a — no máximo um eliminado pode ter sobrado com objeto:
        // aquele cuja saída encerrou a partida, quando a limpeza deixa de ser
        // exigida. Todos os outros já foram limpos, e o vigia em
        // `WatchdogAgent` provou isso a cada decisão enquanto o jogo corria.
        assert!(
            r.leftover <= 1,
            "semente {seed}: {} eliminados ainda tinham objeto no fim",
            r.leftover
        );

        total_turns += u64::from(r.turns);
        max_turns_seen = max_turns_seen.max(r.turns);
        total_eliminated += r.eliminated;
    }

    let n = SEEDS as usize;
    assert_eq!(winners + draws, n, "toda partida tem de cair num dos dois casos");
    println!(
        "mesas={n} vencedor={winners} empate={draws} \
         turnos(média)={:.1} turnos(máx)={max_turns_seen} \
         eliminados(média)={:.2} por assento={by_seat:?}",
        total_turns as f64 / n as f64,
        total_eliminated as f64 / n as f64
    );

    // Uma mesa em que ninguém nunca é eliminado não exercitaria o CR 800.4a, e
    // este teste estaria verde sem testar o que diz testar.
    assert!(
        total_eliminated > 0,
        "nenhuma eliminação em {n} mesas: o CR 800.4a não foi exercitado"
    );
}

#[test]
fn a_mesma_semente_repete_a_mesa_de_quatro() {
    // Determinismo é requisito do projeto: sem ele o pânico com número de
    // semente do teste acima não reproduz nada.
    let db = database();
    let lists = commander_lists(&db);
    for seed in [0u64, 13, 61] {
        let first = play_table(Arc::clone(&db), &lists, seed);
        let again = play_table(Arc::clone(&db), &lists, seed);
        assert_eq!(first, again, "semente {seed}: a mesa não repetiu");
    }
}

#[test]
fn cada_jogador_comeca_com_quarenta_de_vida_e_o_comandante_na_zona_de_comando() {
    // CR 903.6 e CR 903.7 — a montagem da mesa de quatro tem de valer para os
    // quatro assentos, não só para os dois primeiros.
    let db = database();
    let lists = commander_lists(&db);
    let mut players = Vec::with_capacity(SEATS);
    let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(SEATS);
    for seat in 0..SEATS {
        let (deck, commander) = &lists[seat % lists.len()];
        players.push(PlayerConfig {
            name: format!("P{seat}"),
            deck: deck.clone(),
            commander: Some(*commander),
        });
        agents.push(Box::new(common::RandomAgent::new(&format!("P{seat}"), seat as u64)));
    }
    let game = match Game::new(Arc::clone(&db), players, agents, config(), 3) {
        Ok(g) => g,
        Err(err) => panic!("Game::new falhou: {err:?}"),
    };

    assert_eq!(game.state.players.len(), SEATS);
    for seat in 0..SEATS {
        let player = PlayerId(seat as u8);
        let p = game.state.player(player);
        assert_eq!(p.life, 40, "{player:?} não começou com 40 (CR 903.7)");
        let Some(object) = p.commander else {
            panic!("{player:?} não tem comandante registrado (CR 903.6)");
        };
        let Some(state) = game.state.object(object) else {
            panic!("{player:?}: comandante {object:?} não existe no estado");
        };
        assert!(state.is_commander, "{player:?}: objeto não marcado como comandante (CR 903.3)");
        assert_eq!(
            state.zone.kind,
            ZoneKind::Command,
            "{player:?}: comandante não começou na zona de comando (CR 903.6)"
        );
        assert!(
            !game.state.library(player).objects.contains(&object),
            "{player:?}: comandante ficou na biblioteca e contaminaria o embaralho"
        );
    }
}

#[test]
fn eliminacao_no_meio_da_mesa_leva_junto_o_que_era_do_jogador() {
    // CR 800.4a, contra o motor de verdade e numa mesa de quatro: o jogador
    // que sai leva os permanentes dele, e o que ele controlava sem possuir
    // volta ao dono. O duelo nunca exercita isto — lá, quem perde encerra a
    // partida e não há "depois" para limpar.
    let db = database();
    let lists = commander_lists(&db);
    let mut game = build_table(db, &lists, 21);

    let victim = PlayerId(2);
    let lender = PlayerId(1);

    // Dois permanentes do próprio jogador que vai sair...
    let owned: Vec<ObjectId> = (0..2).map(|_| put_top_card_on_battlefield(&mut game, victim)).collect();
    // ...e um que ele controla mas não possui (efeito de roubo de controle).
    let borrowed = put_top_card_on_battlefield(&mut game, lender);
    match game.state.object_mut(borrowed) {
        Some(obj) => obj.controller = victim,
        None => panic!("{borrowed:?} sumiu logo após entrar no campo"),
    }

    let before = game.state.battlefield().objects.len();
    assert_eq!(before, 3, "montagem devia deixar três permanentes no campo");

    turn::lose_game(&mut game, victim, LossReason::Concede);

    // Três vivos: a partida continua, então a limpeza é obrigatória.
    assert_eq!(
        game.state.outcome,
        GameOutcome::Ongoing,
        "com três vivos a partida não podia acabar (CR 104.2a)"
    );
    assert!(game.state.player(victim).has_lost);

    let leftovers = orphans_of(&game.state, victim);
    assert!(
        leftovers.is_empty(),
        "quem saiu deixou para trás: {}",
        leftovers.join("; ")
    );
    for id in owned {
        assert!(
            !game.state.battlefield().objects.contains(&id),
            "{id:?} era do jogador que saiu e continua no campo (CR 800.4a)"
        );
    }

    // O emprestado não sai do jogo: volta ao dono, que ainda está na partida.
    assert!(
        game.state.battlefield().objects.contains(&borrowed),
        "o permanente emprestado saiu junto, e ele é de quem ficou"
    );
    let Some(obj) = game.state.object(borrowed) else {
        panic!("{borrowed:?} deixou de existir");
    };
    assert_eq!(obj.controller, lender, "o emprestado não voltou ao dono (CR 800.4a)");
}

#[test]
fn dano_de_comandante_derrota_um_so_e_a_mesa_continua() {
    // CR 704.5v numa mesa de quatro: 21 de um mesmo comandante derrota o alvo
    // pela SBA, e só ele. Num duelo isso encerraria a partida e esconderia o
    // caso interessante — aqui os outros três continuam jogando.
    let db = database();
    let lists = commander_lists(&db);
    let mut game = build_table(db, &lists, 5);

    let attacker = PlayerId(0);
    let victim = PlayerId(3);
    let bystander = PlayerId(1);
    let Some(commander_id) = game.state.player(attacker).commander else {
        panic!("{attacker:?} não tem comandante (CR 903.6)");
    };

    // Vinte não bastam: o limiar é 21 (CR 903.10).
    commander::note_combat_damage(&mut game.state, commander_id, victim, 20);
    sba::check_until_stable(&mut game);
    assert!(
        !game.state.player(victim).has_lost,
        "CR 704.5v — 20 de dano de comandante não derrota ninguém"
    );

    commander::note_combat_damage(&mut game.state, commander_id, victim, 1);
    sba::check_until_stable(&mut game);

    assert!(game.state.player(victim).has_lost, "CR 704.5v — 21 derrota");
    assert_eq!(
        game.state.player(victim).loss_reason,
        Some(LossReason::CommanderDamage)
    );
    assert_eq!(
        game.state.player(victim).life,
        40,
        "CR 903.10 — a derrota vem da matriz, não da vida"
    );
    assert!(
        !game.state.player(bystander).has_lost,
        "só quem levou os 21 sai; a matriz é por jogador"
    );
    assert_eq!(
        game.state.outcome,
        GameOutcome::Ongoing,
        "CR 104.2a — com três vivos a partida continua"
    );
    assert!(
        orphans_of(&game.state, victim).is_empty(),
        "CR 800.4a — quem saiu no meio da mesa não deixa objeto para trás"
    );
}

// ---------------------------------------------------------------------------
// Montagem manual, para os testes que precisam de um estado exato
// ---------------------------------------------------------------------------

/// Mesa de quatro pronta, com agentes que só sorteiam. Serve aos testes que
/// mexem no estado à mão e não deixam o motor jogar.
fn build_table(
    db: Arc<CardDatabase>,
    lists: &[(Vec<CardDefId>, CardDefId)],
    seed: u64,
) -> Game {
    let mut players = Vec::with_capacity(SEATS);
    let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(SEATS);
    for seat in 0..SEATS {
        let (deck, commander) = &lists[seat % lists.len()];
        players.push(PlayerConfig {
            name: format!("P{seat}"),
            deck: deck.clone(),
            commander: Some(*commander),
        });
        agents.push(Box::new(common::RandomAgent::new(
            &format!("P{seat}"),
            seed ^ seat as u64,
        )));
    }
    match Game::new(db, players, agents, config(), seed) {
        Ok(g) => g,
        Err(err) => panic!("semente {seed}: Game::new falhou: {err:?}"),
    }
}

/// Tira a carta do topo da biblioteca do jogador e a põe no campo sob o
/// controle dele. Pânico se a biblioteca estiver vazia — em teste, silêncio é
/// pior que falha.
fn put_top_card_on_battlefield(game: &mut Game, player: PlayerId) -> ObjectId {
    let Some(id) = game.state.library(player).objects.first().copied() else {
        panic!("biblioteca de {player:?} está vazia");
    };
    turn::move_object(game, id, ZoneId::BATTLEFIELD);
    id
}

#[test]
fn efeito_ate_o_seu_proximo_turno_expira_pulando_quem_ja_saiu() {
    // CR 800.4d — quem deixou a partida sai da ordem de turno. `advance_turn`
    // já respeita isso (usa `next_alive`), então a expiração de
    // `Duration::YourNextTurn` tem de olhar para o **mesmo** jogador que vai
    // de fato receber o próximo turno. Com o sucessor cru (módulo puro), os
    // dois discordam exatamente quando alguém foi eliminado — e o efeito de
    // quem herdou a vez nunca expira, virando permanente na prática.
    //
    // O duelo nunca pega isto: lá, se o outro jogador sai a partida acaba e
    // não existe "próximo turno" para comparar. Só numa mesa de três ou mais.
    let db = database();
    let lists = commander_lists(&db);
    let mut game = build_table(db, &lists, 7);

    let active = PlayerId(0);
    let dead = PlayerId(1);
    let heir = PlayerId(2);

    game.state.active_player = active;
    turn::lose_game(&mut game, dead, LossReason::Concede);
    assert!(game.state.player(dead).has_lost, "montagem: {dead:?} devia ter saído");
    assert_eq!(
        game.state.outcome,
        GameOutcome::Ongoing,
        "com três vivos a mesa continua (CR 104.2a)"
    );

    // Quem realmente joga depois de P0 é P2: o sucessor cru seria P1, que saiu.
    let source = put_top_card_on_battlefield(&mut game, heir);
    let created_turn = game.state.turn;
    let timestamp = game.state.next_timestamp();
    game.state.continuous.push(ContinuousEffect {
        id: 1,
        source,
        affected: vec![source],
        modification: StaticModRuntime::ModifyPT(2, 2),
        duration: Duration::YourNextTurn,
        timestamp,
        created_turn,
        controller: heir,
    });

    layers::expire_continuous_effects(&mut game);

    assert!(
        game.state.continuous.is_empty(),
        "CR 800.4d — o efeito de {heir:?} devia expirar: com {dead:?} fora, \
         é {heir:?} quem começa o próximo turno, não o sucessor cru {dead:?}"
    );
}

#[test]
fn efeito_ate_o_seu_proximo_turno_sobrevive_ao_turno_de_outro() {
    // Contraprova do teste acima: sem eliminação nenhuma, o efeito de quem
    // **não** é o próximo jogador ativo não pode expirar. Sem esta metade, o
    // teste anterior passaria com um `retain` que apaga tudo.
    let db = database();
    let lists = commander_lists(&db);
    let mut game = build_table(db, &lists, 7);

    let active = PlayerId(0);
    let distante = PlayerId(3);
    game.state.active_player = active;

    let source = put_top_card_on_battlefield(&mut game, distante);
    let created_turn = game.state.turn;
    let timestamp = game.state.next_timestamp();
    game.state.continuous.push(ContinuousEffect {
        id: 1,
        source,
        affected: vec![source],
        modification: StaticModRuntime::ModifyPT(2, 2),
        duration: Duration::YourNextTurn,
        timestamp,
        created_turn,
        controller: distante,
    });

    layers::expire_continuous_effects(&mut game);

    assert_eq!(
        game.state.continuous.len(),
        1,
        "o próximo turno é de P1, não de {distante:?}: o efeito dele ainda vale"
    );
}
