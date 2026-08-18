//! Regras próprias de Commander (CR 903), contra o motor de verdade.
//!
//! Nada aqui usa `if let Some(..)` para escapar de um caminho feliz que não
//! aconteceu: quando o motor não faz o que deveria, o teste entra em pânico.
mod common;

use common::*;

use mtg_core::action::{Action, Request};
use mtg_core::engine::{cast, combat, commander, turn, Agent, Game};
use mtg_core::event::{Defender, Step};
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::mana::{Color, ColorSet};
use mtg_core::types::{CardType, Supertype};
use mtg_core::zone::ZoneId;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const COMMANDER_A: &str = "Test Commander A";
const COMMANDER_B: &str = "Test Commander B";
const FILLER: &str = "Test Filler";
/// Cópias de enchimento no deck: o motor recusa deck vazio.
const FILLER_COPIES: usize = 20;

// ---------------------------------------------------------------------------
// Montagem
// ---------------------------------------------------------------------------

/// Partida de Commander em que só P0 tem comandante, com o P/T e o custo
/// pedidos. O comandante entra no deck de P0 — é de lá que `Game::new` o tira.
fn one_commander_game(cost: &str, power: i32, toughness: i32, agents: Vec<Box<dyn Agent>>) -> Game {
    let mut setup = Setup::empty();
    let mut def = creature_def_costed(COMMANDER_A, cost, power, toughness, &[]);
    def.type_line.supertypes.push(Supertype::Legendary);
    let commander_id = setup.add_card(def);
    let filler = setup.add_card(creature_def_costed(FILLER, "{1}", 1, 1, &[]));

    setup.deck(P0, &[commander_id]);
    setup.fill(P0, filler, FILLER_COPIES);
    setup.fill(P1, filler, FILLER_COPIES);
    setup.commander(P0, commander_id);
    setup.build(agents)
}

/// Partida em que os dois jogadores têm comandante — é o que permite pôr dois
/// comandantes diferentes batendo na mesma vítima.
fn two_commander_game() -> Game {
    let mut setup = Setup::empty();
    let mut def_a = creature_def_costed(COMMANDER_A, "{1}", 2, 2, &[]);
    def_a.type_line.supertypes.push(Supertype::Legendary);
    let mut def_b = creature_def_costed(COMMANDER_B, "{1}", 2, 2, &[]);
    def_b.type_line.supertypes.push(Supertype::Legendary);
    let a = setup.add_card(def_a);
    let b = setup.add_card(def_b);
    let filler = setup.add_card(creature_def_costed(FILLER, "{1}", 1, 1, &[]));

    setup.deck(P0, &[a]);
    setup.fill(P0, filler, FILLER_COPIES);
    setup.deck(P1, &[b]);
    setup.fill(P1, filler, FILLER_COPIES);
    setup.commander(P0, a);
    setup.commander(P1, b);
    setup.build_passing()
}

/// Único objeto na zona de comando do jogador. Pânico se ela estiver vazia: um
/// teste que siga sem comandante não está testando CR 903 coisa nenhuma.
fn commander_object(game: &Game, player: PlayerId) -> ObjectId {
    match game.state.command(player).objects.first() {
        Some(id) => *id,
        None => panic!("zona de comando de {player} está vazia"),
    }
}

/// Valor de mana do custo total da mágica no estado atual (CR 202.3).
fn total_mana_value(game: &Game, object: ObjectId, controller: PlayerId) -> u32 {
    cast::spell_total_cost(game, object, controller)
        .iter()
        .map(|s| s.mana_value())
        .sum()
}

/// Lança o comandante pela `cast::execute` de verdade — custo pago inclusive.
fn cast_commander(game: &mut Game, object: ObjectId, player: PlayerId) {
    set_active(game, player);
    goto_step(game, Step::PrecombatMain);
    fill_mana_pool(game, player, 20);
    let action = Action::CastSpell {
        object,
        targets: Vec::new(),
        x: 0,
        modes: Vec::new(),
        mana_plan: Vec::new(),
    };
    if let Err(err) = cast::execute(game, player, action) {
        panic!("lançar o comandante da zona de comando falhou: {err}");
    }
    resolve_stack(game);
}

/// Agente que responde uma escolha fixa a todo `ConfirmOptional` e passa
/// prioridade no resto — é como CR 903.9 é exercitada nos dois sentidos.
fn confirming_agent(name: &'static str, yes: bool) -> Box<dyn Agent> {
    Box::new(ScriptedAgent::new(name, move |request, legal| {
        match request {
            Request::ConfirmOptional { .. } => Action::Confirm { yes },
            _ => default_action(legal),
        }
    }))
}

fn agents_confirming(yes: bool) -> Vec<Box<dyn Agent>> {
    vec![confirming_agent("A", yes), confirming_agent("B", yes)]
}

// ---------------------------------------------------------------------------
// CR 903.6
// ---------------------------------------------------------------------------

#[test]
fn comandante_comeca_na_zona_de_comando() {
    // CR 903.6 — o comandante começa a partida na zona de comando, não no deck.
    let game = one_commander_game("{1}", 2, 2, passing_agents());
    let commander_id = commander_object(&game, P0);

    let Some(state) = game.state.object(commander_id) else {
        panic!("{commander_id} não existe depois da montagem")
    };
    assert_eq!(
        state.zone,
        ZoneId::command(P0),
        "CR 903.6 — o comandante tem de estar na zona de comando"
    );
    assert!(
        state.is_commander,
        "CR 903.3 — o objeto precisa carregar a designação de comandante"
    );
    assert_eq!(state.owner, P0, "o comandante é do jogador que o declarou");

    let Some(card) = game.db.get(state.card) else {
        panic!("a carta do comandante sumiu do catálogo")
    };
    assert_eq!(card.name, COMMANDER_A, "foi o comandante certo que subiu");

    assert!(
        !game.state.library(P0).contains(commander_id),
        "CR 903.6 — o comandante sai da biblioteca; deixá-lo lá o faria comprável"
    );
    assert_eq!(
        game.state.player(P0).commander,
        Some(commander_id),
        "o jogador tem de saber qual objeto é o comandante dele"
    );
    assert_eq!(
        game.state.command(P1).len(),
        0,
        "jogador que não declarou comandante não ganha um de brinde"
    );
    assert_zone_bookkeeping(&game);
}

// ---------------------------------------------------------------------------
// CR 903.8
// ---------------------------------------------------------------------------

#[test]
fn taxa_sobe_dois_por_lancamento_da_zona_de_comando() {
    // CR 903.8 — cada lançamento anterior da zona de comando encarece {2}.
    let mut game = one_commander_game("{1}", 2, 2, agents_confirming(true));
    let commander_id = commander_object(&game, P0);

    assert_eq!(
        total_mana_value(&game, commander_id, P0),
        1,
        "o primeiro lançamento sai pelo custo impresso"
    );

    cast_commander(&mut game, commander_id, P0);
    assert!(
        on_battlefield(&game, commander_id),
        "o comandante tinha de ter resolvido no campo de batalha"
    );
    assert_eq!(
        game.state.player(P0).commander_casts,
        1,
        "o lançamento que aconteceu tem de contar"
    );

    // CR 903.9 leva o comandante de volta para a zona de comando; é lá que a
    // taxa do próximo lançamento é medida.
    turn::move_object(&mut game, commander_id, ZoneId::graveyard(P0));
    assert_eq!(
        zone_of(&game, commander_id),
        ZoneId::command(P0),
        "o agente confirmou o retorno à zona de comando"
    );
    assert_eq!(
        total_mana_value(&game, commander_id, P0),
        3,
        "CR 903.8 — {{1}} impresso + {{2}} de taxa depois de um lançamento"
    );

    cast_commander(&mut game, commander_id, P0);
    assert_eq!(game.state.player(P0).commander_casts, 2);
    turn::move_object(&mut game, commander_id, ZoneId::graveyard(P0));
    assert_eq!(
        total_mana_value(&game, commander_id, P0),
        5,
        "CR 903.8 — a taxa é cumulativa: {{1}} + {{2}} + {{2}}"
    );
}

#[test]
fn taxa_nao_vale_para_lancamento_fora_da_zona_de_comando() {
    // CR 903.8 — a taxa é da *zona de comando*. Comandante na mão custa o
    // impresso, por mais vezes que já tenha sido lançado de casa.
    let mut game = one_commander_game("{1}", 2, 2, agents_confirming(true));
    let commander_id = commander_object(&game, P0);
    cast_commander(&mut game, commander_id, P0);
    assert_eq!(game.state.player(P0).commander_casts, 1);

    turn::move_object(&mut game, commander_id, ZoneId::hand(P0));
    assert_eq!(
        zone_of(&game, commander_id),
        ZoneId::command(P0),
        "o agente confirmou: CR 903.9 vale também para a mão"
    );

    let mut game = one_commander_game("{1}", 2, 2, agents_confirming(false));
    let commander_id = commander_object(&game, P0);
    cast_commander(&mut game, commander_id, P0);
    turn::move_object(&mut game, commander_id, ZoneId::hand(P0));
    assert_eq!(
        zone_of(&game, commander_id),
        ZoneId::hand(P0),
        "o agente recusou: o comandante fica onde iria"
    );
    assert_eq!(
        total_mana_value(&game, commander_id, P0),
        1,
        "CR 903.8 — da mão o comandante custa o impresso, sem taxa"
    );
}

// ---------------------------------------------------------------------------
// CR 903.9
// ---------------------------------------------------------------------------

#[test]
fn comandante_morto_pode_voltar_a_zona_de_comando() {
    // CR 903.9 — indo para o cemitério, o dono pode mandá-lo para a zona de
    // comando. É escolha: os dois ramos têm de funcionar.
    let mut aceita = one_commander_game("{1}", 2, 2, agents_confirming(true));
    let commander_id = commander_object(&aceita, P0);
    turn::move_object(&mut aceita, commander_id, ZoneId::BATTLEFIELD);
    assert!(
        on_battlefield(&aceita, commander_id),
        "o comandante precisa estar em campo para poder morrer"
    );

    turn::move_object(&mut aceita, commander_id, ZoneId::graveyard(P0));
    assert_eq!(
        zone_of(&aceita, commander_id),
        ZoneId::command(P0),
        "CR 903.9 — dono aceitou: o comandante vai para a zona de comando"
    );
    assert!(
        !aceita.state.graveyard(P0).contains(commander_id),
        "o cemitério não pode ficar com uma cópia fantasma"
    );
    assert_zone_bookkeeping(&aceita);

    let mut recusa = one_commander_game("{1}", 2, 2, agents_confirming(false));
    let commander_id = commander_object(&recusa, P0);
    turn::move_object(&mut recusa, commander_id, ZoneId::BATTLEFIELD);
    turn::move_object(&mut recusa, commander_id, ZoneId::graveyard(P0));
    assert_eq!(
        zone_of(&recusa, commander_id),
        ZoneId::graveyard(P0),
        "CR 903.9 — a substituição é opcional; recusada, o comandante morre mesmo"
    );
    assert_zone_bookkeeping(&recusa);
}

#[test]
fn comandante_exilado_tambem_pode_voltar() {
    // CR 903.9 — exílio, mão e biblioteca contam junto com o cemitério.
    for destino in [ZoneId::EXILE, ZoneId::library(P0)] {
        let mut game = one_commander_game("{1}", 2, 2, agents_confirming(true));
        let commander_id = commander_object(&game, P0);
        turn::move_object(&mut game, commander_id, ZoneId::BATTLEFIELD);
        turn::move_object(&mut game, commander_id, destino);
        assert_eq!(
            zone_of(&game, commander_id),
            ZoneId::command(P0),
            "CR 903.9 — destino {:?} também é resgatável",
            destino.kind
        );
    }
}

// ---------------------------------------------------------------------------
// CR 903.10
// ---------------------------------------------------------------------------

#[test]
fn vinte_e_um_de_dano_do_mesmo_comandante_mata() {
    // CR 903.10 — 21 de dano de combate de um mesmo comandante derrota quem o
    // recebeu. O limiar é 21: 20 não basta.
    let mut game = one_commander_game("{1}", 20, 20, passing_agents());
    let commander_id = commander_object(&game, P0);
    turn::move_object(&mut game, commander_id, ZoneId::BATTLEFIELD);
    clear_summoning_sickness(&mut game, commander_id);
    // Vida alta de propósito: quem tem de matar aqui é CR 903.10, não CR 704.5a.
    set_life(&mut game, P1, 60);

    set_active(&mut game, P0);
    goto_step(&mut game, Step::DeclareAttackers);
    combat::declare_attackers(&mut game, &[(commander_id, Defender::Player(P1))]);
    combat::combat_damage_step(&mut game, false);

    assert_eq!(
        game.state.player(P1).commander_damage_from(commander_id),
        20,
        "o dano de combate do comandante tem de ser creditado na matriz"
    );
    assert_eq!(
        life(&game, P1),
        40,
        "CR 119.3 — o dano continua sendo perda de vida normal, além da matriz"
    );
    assert!(
        commander::lethal_commander_damage(&game.state).is_empty(),
        "CR 903.10 — 20 não é 21; ninguém morre ainda"
    );

    commander::note_combat_damage(&mut game.state, commander_id, P1, 1);
    assert_eq!(
        commander::lethal_commander_damage(&game.state),
        vec![P1],
        "CR 903.10 — no vigésimo primeiro ponto o jogador é derrotado"
    );
}

#[test]
fn dano_de_dois_comandantes_diferentes_nao_soma() {
    // CR 903.10 — a contagem é por comandante. Vinte de um e vinte de outro
    // somam quarenta de vida perdida e zero derrota por dano de comandante.
    let mut game = two_commander_game();
    let a = commander_object(&game, P0);
    let b = commander_object(&game, P1);
    assert_ne!(a, b, "os dois comandantes têm de ser objetos distintos");

    commander::note_combat_damage(&mut game.state, a, P1, 20);
    commander::note_combat_damage(&mut game.state, b, P1, 20);

    assert_eq!(game.state.player(P1).commander_damage_from(a), 20);
    assert_eq!(game.state.player(P1).commander_damage_from(b), 20);
    assert!(
        commander::lethal_commander_damage(&game.state).is_empty(),
        "CR 903.10 — 20 + 20 de comandantes diferentes não derrota ninguém"
    );

    commander::note_combat_damage(&mut game.state, a, P1, 1);
    assert_eq!(
        commander::lethal_commander_damage(&game.state),
        vec![P1],
        "CR 903.10 — o vigésimo primeiro ponto do *mesmo* comandante derrota"
    );
}

#[test]
fn dano_de_quem_nao_e_comandante_nao_entra_na_matriz() {
    // CR 903.10 — só comandante alimenta a matriz dos 21.
    let mut game = one_commander_game("{1}", 2, 2, passing_agents());
    let bicho = put_ready(&mut game, FILLER, P0);
    commander::note_combat_damage(&mut game.state, bicho, P1, 21);

    assert_eq!(
        game.state.player(P1).commander_damage_from(bicho),
        0,
        "criatura comum não é comandante e não credita nada"
    );
    assert!(
        commander::lethal_commander_damage(&game.state).is_empty(),
        "CR 903.10 — 21 de uma criatura qualquer não derrota"
    );
}

// ---------------------------------------------------------------------------
// CR 903.4
// ---------------------------------------------------------------------------

#[test]
fn identidade_de_cor_inclui_simbolo_no_texto() {
    // CR 903.4 — a identidade soma custo, indicador de cor e todo símbolo de
    // mana colorido no texto de regras.
    let mut casca = blank_card("Colorless Shell", "{2}", vec![CardType::Artifact]);
    casca.oracle_text = "{T}: Adicione {G}.".to_string();

    assert!(
        casca.colors().is_colorless(),
        "a carta em si continua incolor: identidade não é cor (CR 202.2)"
    );
    let identidade = commander::color_identity(&casca);
    assert!(
        identidade.contains(Color::Green),
        "CR 903.4 — o {{G}} do texto entra na identidade"
    );
    assert_eq!(
        identidade.count(),
        1,
        "só o verde: {{2}} e {{T}} não são cor"
    );

    let mut hibrida = blank_card("Hybrid Talk", "{1}", vec![CardType::Enchantment]);
    hibrida.oracle_text = "{W/U}: nada. {2/R}: nada. {B/P}: nada.".to_string();
    let identidade = commander::color_identity(&hibrida);
    for color in [Color::White, Color::Blue, Color::Red, Color::Black] {
        assert!(
            identidade.contains(color),
            "CR 903.4 — símbolo híbrido/monohíbrido/fírexiano entra com a cor {color:?}"
        );
    }
    assert!(
        !identidade.contains(Color::Green),
        "nada no texto é verde: o parser não pode inventar cor"
    );

    let mut indicada = blank_card("Indicated Face", "", vec![CardType::Creature]);
    indicada.color_override = Some(ColorSet::single(Color::Blue));
    assert!(
        commander::color_identity(&indicada).contains(Color::Blue),
        "CR 903.4 — o indicador de cor entra na identidade mesmo sem custo de mana"
    );

    let mut do_custo = blank_card("Costed", "{W}{B}", vec![CardType::Creature]);
    do_custo.oracle_text = "Sem símbolos aqui.".to_string();
    let identidade = commander::color_identity(&do_custo);
    assert!(
        identidade.contains(Color::White) && identidade.contains(Color::Black),
        "CR 903.4 — o custo de mana continua sendo a base da identidade"
    );
    assert_eq!(identidade.count(), 2);
}

// ---------------------------------------------------------------------------
// Formato
// ---------------------------------------------------------------------------

#[test]
fn formato_construido_nao_tem_zona_de_comando_ocupada() {
    // O caminho de duas cartas do motor não pode mudar por causa de CR 903:
    // sem `PlayerConfig::commander`, nada entra na zona de comando.
    let game = game_with_defs(
        vec![creature_def_costed(FILLER, "{1}", 1, 1, &[])],
        passing_agents(),
    );
    for player in [P0, P1] {
        assert_eq!(
            game.state.command(player).len(),
            0,
            "partida sem comandante não pode povoar a zona de comando"
        );
        assert_eq!(game.state.player(player).commander, None);
        assert_eq!(game.state.player(player).commander_casts, 0);
    }
}
