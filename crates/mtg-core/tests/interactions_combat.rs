//! Itens 42–60 de `docs/RULES_TESTS.md`: combate (CR 506–511) e mana/custos
//! (CR 107, 202, 601, 605).
//!
//! Todos os testes falam com o motor de verdade — `engine::combat`,
//! `engine::cast`, `engine::stack`, `engine::sba` — e não com a simulação
//! aproximada de `mtg-ai`. A diferença importa: aquela simulação não dispara
//! gatilho, não passa pelas camadas e não paga custo, então item "coberto" por
//! ela na verdade não está coberto.
//!
//! Convenção: cada `#[test]` tem o nome exato do item e cita a regra. Nenhum
//! teste usa `if let Some(..)` para se esquivar de um caminho feliz que não
//! aconteceu — quando o motor não faz o que deveria, o teste entra em pânico.
//!
//! O módulo `support` é local de propósito: os dois arquivos que este agente
//! entrega precisam compilar sozinhos, sem depender de `tests/common/mod.rs`,
//! que está sendo escrito em paralelo.

mod support;

use support::*;

use mtg_core::action::{Action, Request, TargetChoice};
use mtg_core::card::CardDef;
use mtg_core::engine::{cast, combat, sba, turn, Agent, Game};
use mtg_core::event::{Defender, Step};
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::{
    Cost, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec, Value,
};
use mtg_core::mana::{Color, ManaSymbol};
use mtg_core::types::CardType;
use mtg_core::zone::ZoneId;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

// ---------------------------------------------------------------------------
// Cenário base de combate
// ---------------------------------------------------------------------------

/// Elenco fixo dos testes de combate. Nomes de carta em inglês, como o resto do
/// catálogo.
fn combat_cards() -> Vec<CardDef> {
    vec![
        creature_def("Vanilla Bear", "{1}{G}", 2, 2, &[]),
        creature_def("Big Vanilla", "{3}{G}", 3, 3, &[]),
        creature_def("Hasty Raider", "{1}{R}", 2, 2, &[Keyword::Haste]),
        creature_def("Watchful Knight", "{2}{W}", 2, 2, &[Keyword::Vigilance]),
        creature_def("Sky Serpent", "{2}{U}", 2, 2, &[Keyword::Flying]),
        creature_def("Web Spinner", "{1}{G}", 1, 3, &[Keyword::Reach]),
        creature_def("Two-Headed Brute", "{2}{R}", 3, 3, &[Keyword::Menace]),
        creature_def("Stomping Ox", "{3}{G}", 5, 5, &[Keyword::Trample]),
        creature_def(
            "Venomous Charger",
            "{3}{B}",
            5,
            5,
            &[Keyword::Trample, Keyword::Deathtouch],
        ),
        creature_def("Duelist", "{1}{W}", 2, 2, &[Keyword::FirstStrike]),
        creature_def("Twin Blade", "{2}{W}", 2, 2, &[Keyword::DoubleStrike]),
        creature_def("Wall of Meat", "{2}{G}", 0, 5, &[]),
        creature_def("Bloodthirsty Cleric", "{2}{W}", 3, 3, &[Keyword::Lifelink]),
    ]
}

fn combat_game(agents: Vec<Box<dyn Agent>>) -> Game {
    let mut game = game_with_defs(combat_cards(), agents);
    goto_step(&mut game, Step::DeclareAttackers);
    game
}

/// Declara o ataque pelo motor e confere que ele foi de fato registrado.
fn attack_with(game: &mut Game, attacker: ObjectId) {
    combat::declare_attackers(game, &[(attacker, Defender::Player(P1))]);
    assert!(
        game.state
            .object(attacker)
            .is_some_and(|o| o.combat.is_attacking()),
        "o motor não registrou {attacker} como atacante"
    );
}

fn blockers_of(game: &Game, attacker: ObjectId) -> Vec<ObjectId> {
    game.state
        .object(attacker)
        .map(|o| o.combat.blocked_by.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 6. Combate (CR 506–511)
// ---------------------------------------------------------------------------

#[test]
fn criatura_com_enjoo_nao_pode_atacar() {
    // CR 302.6 — criatura que não está sob controle do jogador desde o começo
    // do turno dele não pode atacar nem usar habilidade com {T} no custo.
    let mut game = combat_game(passing_agents());
    let fresh = put_on_battlefield(&mut game, "Vanilla Bear", P0);

    assert!(
        game.state.object(fresh).is_some_and(|o| o.summoning_sick),
        "a montagem tem de deixar a criatura com enjoo, senão o teste não testa nada"
    );
    assert!(
        !combat::eligible_attackers(&game, P0).contains(&fresh),
        "CR 302.6 — criatura com enjoo apareceu entre os atacantes elegíveis"
    );

    // Contraprova: tirado o enjoo, a mesma criatura passa a ser elegível.
    clear_summoning_sickness(&mut game, fresh);
    assert!(
        combat::eligible_attackers(&game, P0).contains(&fresh),
        "sem enjoo a criatura tem de poder atacar — sem isto a asserção anterior seria vácua"
    );
}

#[test]
fn pressa_permite_atacar_no_turno_que_entrou() {
    // CR 702.10b — Ímpeto remove a restrição de enjoo para atacar e para {T}.
    let mut game = combat_game(passing_agents());
    let hasty = put_on_battlefield(&mut game, "Hasty Raider", P0);
    let slow = put_on_battlefield(&mut game, "Vanilla Bear", P0);

    assert!(
        game.state.object(hasty).is_some_and(|o| o.summoning_sick),
        "criatura com Ímpeto também entra com enjoo; o que muda é a restrição"
    );
    let eligible = combat::eligible_attackers(&game, P0);
    assert!(
        eligible.contains(&hasty),
        "CR 702.10b — Ímpeto tem de permitir o ataque no turno de entrada"
    );
    assert!(
        !eligible.contains(&slow),
        "a criatura sem Ímpeto, na mesma situação, não pode atacar"
    );
}

#[test]
fn vigilancia_ataca_sem_virar() {
    // CR 702.20b — atacar não vira a criatura com Vigilância (exceção a CR 508.1f).
    let mut game = combat_game(passing_agents());
    let knight = put_ready(&mut game, "Watchful Knight", P0);
    let bear = put_ready(&mut game, "Vanilla Bear", P0);

    combat::declare_attackers(
        &mut game,
        &[(knight, Defender::Player(P1)), (bear, Defender::Player(P1))],
    );

    assert!(
        game.state
            .object(knight)
            .is_some_and(|o| o.combat.is_attacking()),
        "o cavaleiro precisa estar atacando para o teste significar algo"
    );
    assert!(
        !is_tapped(&game, knight),
        "CR 702.20b — Vigilância ataca sem virar"
    );
    assert!(
        is_tapped(&game, bear),
        "CR 508.1f — sem Vigilância, atacar vira a criatura"
    );
}

#[test]
fn voar_so_e_bloqueado_por_voar_ou_alcance() {
    // CR 702.9b — criatura com Voar só pode ser bloqueada por criatura com Voar
    // ou com Alcance.
    let mut game = combat_game(passing_agents());
    let flyer = put_ready(&mut game, "Sky Serpent", P0);
    let ground = put_ready(&mut game, "Vanilla Bear", P1);
    let other_flyer = put_ready(&mut game, "Sky Serpent", P1);
    let reacher = put_ready(&mut game, "Web Spinner", P1);
    attack_with(&mut game, flyer);

    assert!(
        !combat::can_block(&game, ground, flyer),
        "CR 702.9b — criatura sem Voar nem Alcance não bloqueia quem voa"
    );
    assert!(
        combat::can_block(&game, other_flyer, flyer),
        "quem voa bloqueia quem voa"
    );
    assert!(
        combat::can_block(&game, reacher, flyer),
        "Alcance bloqueia quem voa"
    );

    // O motor precisa recusar a declaração ilegal, não só o predicado.
    combat::declare_blockers(&mut game, &[(ground, flyer)]);
    assert!(
        blockers_of(&game, flyer).is_empty(),
        "bloqueio ilegal por criatura terrestre foi aceito: {:?}",
        blockers_of(&game, flyer)
    );
    combat::declare_blockers(&mut game, &[(reacher, flyer)]);
    assert_eq!(
        blockers_of(&game, flyer),
        vec![reacher],
        "o bloqueio por Alcance tinha de ser aceito"
    );
}

#[test]
fn ameacar_exige_dois_bloqueadores() {
    // CR 702.110b — Ameaçar: a criatura não pode ser bloqueada exceto por duas
    // ou mais criaturas.
    let mut game = combat_game(passing_agents());
    let brute = put_ready(&mut game, "Two-Headed Brute", P0);
    let g1 = put_ready(&mut game, "Vanilla Bear", P1);
    let g2 = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, brute);

    combat::declare_blockers(&mut game, &[(g1, brute)]);
    assert!(
        blockers_of(&game, brute).is_empty(),
        "CR 702.110b — um bloqueador sozinho não bloqueia Ameaçar, mas o motor aceitou {:?}",
        blockers_of(&game, brute)
    );

    combat::declare_blockers(&mut game, &[(g1, brute), (g2, brute)]);
    assert_eq!(
        blockers_of(&game, brute).len(),
        2,
        "com dois bloqueadores o bloqueio de Ameaçar é legal"
    );
}

#[test]
fn atacante_bloqueado_cujo_bloqueador_sumiu_nao_causa_dano_ao_jogador() {
    // CR 509.1h — a criatura bloqueada continua bloqueada mesmo sem bloqueador
    // no campo; sem Atropelar ela não causa dano ao jogador defensor.
    let mut game = combat_game(passing_agents());
    let attacker = put_ready(&mut game, "Big Vanilla", P0);
    let blocker = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, attacker);
    combat::declare_blockers(&mut game, &[(blocker, attacker)]);
    assert_eq!(
        blockers_of(&game, attacker).len(),
        1,
        "o bloqueio precisa ter acontecido antes de o bloqueador sumir"
    );

    // O bloqueador sai do campo antes do passo de dano.
    turn::move_object(&mut game, blocker, ZoneId::graveyard(P1));
    combat::combat_damage_step(&mut game, false);

    assert_eq!(
        life(&game, P1),
        20,
        "CR 509.1h — atacante bloqueado sem bloqueadores vivos não fere o jogador"
    );
}

#[test]
fn atropelar_passa_o_excedente_ao_defensor() {
    // CR 702.19b — atribuído dano letal ao bloqueador, o excedente vai ao
    // jogador defensor.
    let mut game = combat_game(passing_agents());
    let ox = put_ready(&mut game, "Stomping Ox", P0);
    let chump = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, ox);
    combat::declare_blockers(&mut game, &[(chump, ox)]);

    combat::combat_damage_step(&mut game, false);

    assert_eq!(
        damage_on(&game, chump),
        2,
        "CR 510.1c — letal exato no bloqueador 2/2, nem mais nem menos"
    );
    assert_eq!(
        life(&game, P1),
        17,
        "CR 702.19b — os 3 de excedente do 5/5 atravessam"
    );
}

#[test]
fn atropelar_com_toque_mortal_so_precisa_atribuir_um() {
    // CR 702.2c — com Toque Mortal, 1 de dano já conta como letal na
    // atribuição, o que libera todo o resto para o Atropelar.
    let mut game = combat_game(passing_agents());
    let charger = put_ready(&mut game, "Venomous Charger", P0);
    let w1 = put_ready(&mut game, "Big Vanilla", P1);
    let w2 = put_ready(&mut game, "Big Vanilla", P1);
    attack_with(&mut game, charger);
    combat::declare_blockers(&mut game, &[(w1, charger), (w2, charger)]);
    assert_eq!(
        blockers_of(&game, charger).len(),
        2,
        "o bloqueio duplo precisa ter sido aceito"
    );

    combat::combat_damage_step(&mut game, false);

    assert_eq!(damage_on(&game, w1), 1, "CR 702.2c — 1 já é letal");
    assert_eq!(damage_on(&game, w2), 1, "CR 702.2c — 1 já é letal");
    assert_eq!(
        life(&game, P1),
        17,
        "5 de poder menos 1+1 atribuídos: 3 atropelam"
    );

    sba::check_until_stable(&mut game);
    assert!(
        in_graveyard(&game, w1) && in_graveyard(&game, w2),
        "CR 704.5h — 1 de dano de toque mortal destrói os dois bloqueadores 3/3"
    );
}

#[test]
fn primeiro_golpe_mata_antes_do_dano_normal() {
    // CR 510.4 — quem tem Primeiro Golpe causa dano num passo próprio; morto
    // ali, o oponente do combate nunca chega a revidar.
    let mut game = combat_game(passing_agents());
    let duelist = put_ready(&mut game, "Duelist", P0);
    let bear = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, duelist);
    combat::declare_blockers(&mut game, &[(bear, duelist)]);

    assert!(
        combat::has_first_strike_creatures(&game),
        "o passo de primeiro golpe precisa existir para este combate"
    );
    combat::combat_damage_step(&mut game, true);
    assert_eq!(damage_on(&game, bear), 2, "o primeiro golpe acerta");
    assert_eq!(
        damage_on(&game, duelist),
        0,
        "no passo de primeiro golpe o bloqueador comum ainda não bateu"
    );

    sba::check_until_stable(&mut game);
    assert!(
        in_graveyard(&game, bear),
        "CR 704.5g — o bloqueador 2/2 com 2 de dano morre antes do passo normal"
    );

    // CR 510.4 — o passo normal acontece com o bloqueador já morto.
    game.state.first_strike_done = true;
    combat::combat_damage_step(&mut game, false);
    assert_eq!(
        damage_on(&game, duelist),
        0,
        "CR 510.4 — morto no primeiro golpe, o bloqueador não causa dano nenhum"
    );
}

#[test]
fn golpe_duplo_causa_dano_nos_dois_passos() {
    // CR 702.4b — Golpe Duplo causa dano tanto no passo de primeiro golpe
    // quanto no passo de dano de combate normal.
    let mut game = combat_game(passing_agents());
    let twin = put_ready(&mut game, "Twin Blade", P0);
    let wall = put_ready(&mut game, "Wall of Meat", P1);
    attack_with(&mut game, twin);
    combat::declare_blockers(&mut game, &[(wall, twin)]);

    combat::combat_damage_step(&mut game, true);
    assert_eq!(
        damage_on(&game, wall),
        2,
        "o primeiro golpe do Golpe Duplo tem de acertar"
    );

    game.state.first_strike_done = true;
    combat::combat_damage_step(&mut game, false);
    assert_eq!(
        damage_on(&game, wall),
        4,
        "CR 702.4b — o mesmo 2/2 bate de novo no passo normal"
    );
}

#[test]
fn dano_de_combate_e_simultaneo_troca_mutua() {
    // CR 510.2 — todo o dano de combate é causado ao mesmo tempo. Aplicar em
    // sequência faria a primeira criatura a morrer parar de causar dano.
    let mut game = combat_game(passing_agents());
    let attacker = put_ready(&mut game, "Vanilla Bear", P0);
    let blocker = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, attacker);
    combat::declare_blockers(&mut game, &[(blocker, attacker)]);

    combat::combat_damage_step(&mut game, false);
    assert_eq!(damage_on(&game, attacker), 2, "o bloqueador revidou");
    assert_eq!(damage_on(&game, blocker), 2, "o atacante bateu");

    sba::check_until_stable(&mut game);
    assert!(
        in_graveyard(&game, attacker) && in_graveyard(&game, blocker),
        "CR 510.2 — as duas 2/2 morrem juntas; uma sobrevivente denuncia dano sequencial"
    );
}

#[test]
fn vinculo_com_a_vida_da_vida_ao_controlador() {
    // CR 702.15b — dano causado por fonte com Vínculo com a Vida faz o
    // controlador ganhar aquele tanto de vida.
    let mut game = combat_game(passing_agents());
    let cleric = put_ready(&mut game, "Bloodthirsty Cleric", P0);
    attack_with(&mut game, cleric);

    combat::combat_damage_step(&mut game, false);

    assert_eq!(life(&game, P1), 17, "3 de dano ao jogador defensor");
    assert_eq!(
        life(&game, P0),
        23,
        "CR 702.15b — o controlador ganha 3 de vida"
    );
}

#[test]
fn bloqueio_multiplo_respeita_a_ordem_de_dano() {
    // CR 510.1c — o atacante ordena os bloqueadores (CR 509.2) e só passa dano
    // ao seguinte depois de atribuir letal ao anterior.
    //
    // O agente do atacante inverte a ordem proposta: se o motor ignorasse a
    // escolha, o dano cairia na ordem de declaração e a asserção falharia.
    let attacker_agent = ScriptedAgent::new("reverse-order", |request, legal| match request {
        Request::OrderBlockers {
            attacker, blockers, ..
        } => {
            let mut order = blockers.clone();
            order.reverse();
            Action::OrderBlockers {
                attacker: *attacker,
                order,
            }
        }
        _ => default_action(legal),
    });
    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(attacker_agent),
        Box::new(FixedAgent::passing("B")),
    ];

    let mut game = combat_game(agents);
    let attacker = put_ready(&mut game, "Big Vanilla", P0);
    let b1 = put_ready(&mut game, "Vanilla Bear", P1);
    let b2 = put_ready(&mut game, "Vanilla Bear", P1);
    attack_with(&mut game, attacker);

    combat::declare_blockers(&mut game, &[(b1, attacker), (b2, attacker)]);
    assert_eq!(
        blockers_of(&game, attacker),
        vec![b2, b1],
        "CR 509.2 — a ordem escolhida pelo atacante tem de ser a gravada"
    );

    combat::combat_damage_step(&mut game, false);
    assert_eq!(
        damage_on(&game, b2),
        2,
        "CR 510.1c — o primeiro da ordem recebe o dano letal"
    );
    assert_eq!(
        damage_on(&game, b1),
        1,
        "CR 510.1c — só o que sobra vai para o segundo da ordem"
    );
}

// ---------------------------------------------------------------------------
// 7. Mana e custos (CR 107, 202, 601, 605)
// ---------------------------------------------------------------------------

/// Elenco dos testes de custo: terrenos monocromáticos e mágicas sintéticas com
/// cada forma de símbolo que o motor precisa saber pagar. O catálogo real não
/// tem híbrido nem phyrexiano, então eles têm de ser montados aqui.
fn cost_cards() -> Vec<CardDef> {
    vec![
        land_def("Plains", ManaSymbol::Colored(Color::White)),
        land_def("Island", ManaSymbol::Colored(Color::Blue)),
        land_def("Forest", ManaSymbol::Colored(Color::Green)),
        land_def("Mountain", ManaSymbol::Colored(Color::Red)),
        instant_def(
            "Hybrid Blessing",
            "{W/U}",
            Effect::GainLife {
                amount: Value::Const(3),
                player: PlayerRef::You,
            },
        ),
        instant_def(
            "Phyrexian Blessing",
            "{W/P}",
            Effect::GainLife {
                amount: Value::Const(3),
                player: PlayerRef::You,
            },
        ),
        instant_def(
            "Variable Bolt",
            "{X}{R}",
            Effect::DealDamageToPlayer {
                amount: Value::X,
                player: PlayerRef::Opponents,
            },
        ),
        instant_def(
            "Expensive Blessing",
            "{5}",
            Effect::GainLife {
                amount: Value::Const(1),
                player: PlayerRef::You,
            },
        ),
        creature_def("Sacrificial Goat", "{G}", 0, 1, &[]),
        artifact_with_activated(
            "Bone Altar",
            "{2}",
            activated_ability(
                // CR 601.2h — custo composto: mana mais o custo adicional de
                // sacrificar uma criatura.
                Cost::Composite(vec![
                    Cost::Mana(vec![ManaSymbol::Generic(1)]),
                    Cost::Sacrifice(1, Filter::HasType(CardType::Creature)),
                ]),
                Effect::DrawCards {
                    count: Value::Const(1),
                    player: PlayerRef::You,
                },
                "{1}, Sacrifice a creature: Draw a card.",
            ),
        ),
    ]
}

fn cost_game() -> Game {
    let mut game = game_with_defs(cost_cards(), passing_agents());
    goto_step(&mut game, Step::PrecombatMain);
    game
}

/// A mágica aparece entre as ações legais de prioridade?
fn castable(game: &Game, spell: ObjectId) -> bool {
    cast::priority_actions(game, P0)
        .iter()
        .any(|a| matches!(a, Action::CastSpell { object, .. } if *object == spell))
}

/// Ação de lançamento desta mágica com o X pedido. Pânico se não existir: quem
/// chama depende de ela estar lá.
fn cast_action(game: &Game, spell: ObjectId, x: u32) -> Action {
    let found = cast::priority_actions(game, P0).into_iter().find(
        |a| matches!(a, Action::CastSpell { object, x: ax, .. } if *object == spell && *ax == x),
    );
    match found {
        Some(a) => a,
        None => panic!("{spell} não aparece nas ações legais com X={x}"),
    }
}

#[test]
fn custo_hibrido_pode_ser_pago_com_qualquer_metade() {
    // CR 202.2f — um símbolo híbrido pode ser pago com qualquer uma das duas
    // cores que ele nomeia.
    for land in ["Plains", "Island"] {
        let mut game = cost_game();
        let source = put_on_battlefield(&mut game, land, P0);
        let spell = put_in_hand(&mut game, "Hybrid Blessing", P0);
        clear_pending_events(&mut game);

        assert!(
            castable(&game, spell),
            "CR 202.2f — {{W/U}} tem de ser pagável só com {land}"
        );
        let action = cast_action(&game, spell, 0);
        if let Err(err) = cast::execute(&mut game, P0, action) {
            panic!("lançar com {land} falhou: {err}");
        }
        assert!(
            is_tapped(&game, source),
            "a metade escolhida do híbrido tem de virar {land}"
        );
        assert_eq!(game.state.stack.len(), 1, "a mágica tem de estar na pilha");

        resolve_stack(&mut game);
        assert_eq!(
            life(&game, P0),
            23,
            "a mágica precisa resolver depois de paga com {land}"
        );
    }

    // Contraprova: cor que não é nenhuma das metades não paga o híbrido.
    let mut game = cost_game();
    put_on_battlefield(&mut game, "Forest", P0);
    let spell = put_in_hand(&mut game, "Hybrid Blessing", P0);
    clear_pending_events(&mut game);
    assert!(
        !castable(&game, spell),
        "CR 202.2f — {{W/U}} não pode ser pago com mana verde"
    );
}

#[test]
fn custo_phyrexiano_pode_ser_pago_com_dois_de_vida() {
    // CR 107.4f — o símbolo Phyrexiano pode ser pago com a cor que ele nomeia
    // ou com 2 de vida.
    let mut game = cost_game();
    let spell = put_in_hand(&mut game, "Phyrexian Blessing", P0);
    clear_pending_events(&mut game);
    assert_eq!(
        cast::available_mana(&game, P0).total,
        0,
        "sem nenhuma fonte de mana, o único pagamento possível é a vida"
    );

    assert!(
        castable(&game, spell),
        "CR 107.4f — {{W/P}} tem de ser pagável sem mana algum"
    );
    let action = cast_action(&game, spell, 0);
    if let Err(err) = cast::execute(&mut game, P0, action) {
        panic!("pagamento phyrexiano falhou: {err}");
    }
    assert_eq!(
        life(&game, P0),
        18,
        "CR 107.4f — o símbolo phyrexiano custa exatamente 2 de vida"
    );
    assert_eq!(game.state.stack.len(), 1, "a mágica tem de estar na pilha");

    resolve_stack(&mut game);
    assert_eq!(
        life(&game, P0),
        21,
        "18 de vida após o custo, mais os 3 que a mágica devolve"
    );

    // CR 118.4 — não se pode pagar vida que não se tem.
    let mut broke = cost_game();
    let spell = put_in_hand(&mut broke, "Phyrexian Blessing", P0);
    set_life(&mut broke, P0, 1);
    clear_pending_events(&mut broke);
    assert!(
        !castable(&broke, spell),
        "CR 118.4 — com 1 de vida não dá para pagar 2 pelo símbolo phyrexiano"
    );
}

#[test]
fn x_igual_a_zero_e_um_valor_legal() {
    // CR 601.2b — X é escolhido por quem lança, e 0 é um valor legal.
    let mut game = cost_game();
    let mountain = put_on_battlefield(&mut game, "Mountain", P0);
    let spell = put_in_hand(&mut game, "Variable Bolt", P0);
    clear_pending_events(&mut game);

    let with_zero = cast_action(&game, spell, 0);
    assert!(
        cast::priority_actions(&game, P0)
            .iter()
            .all(|a| !matches!(a, Action::CastSpell { object, x, .. } if *object == spell && *x > 0)),
        "com uma única Montanha só X=0 é pagável; X maior não pode aparecer"
    );

    if let Err(err) = cast::execute(&mut game, P0, with_zero) {
        panic!("lançar com X=0 falhou: {err}");
    }
    assert!(
        is_tapped(&game, mountain),
        "o pip {{R}} foi pago pela Montanha"
    );
    assert_eq!(
        game.state.stack.first().map(|i| i.x_value),
        Some(0),
        "o item de pilha tem de guardar X=0"
    );

    resolve_stack(&mut game);
    assert_eq!(
        life(&game, P1),
        20,
        "X=0 causa 0 de dano — legal e inofensivo"
    );
}

#[test]
fn habilidade_de_mana_nao_usa_a_pilha() {
    // CR 605.3a — habilidade de mana resolve imediatamente, sem ir para a pilha
    // e sem dar oportunidade de resposta. Daí ela também não ser oferecida como
    // ação de prioridade (CR 117.1).
    let mut game = cost_game();
    let forest = put_on_battlefield(&mut game, "Forest", P0);
    clear_pending_events(&mut game);
    assert!(
        has_mana_ability(&game, forest),
        "a montagem precisa de uma fonte com habilidade de mana"
    );

    let offered_as_ability = cast::priority_actions(&game, P0)
        .iter()
        .any(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == forest));
    assert!(
        !offered_as_ability,
        "CR 605.3a — habilidade de mana não é uma ação de prioridade"
    );

    assert!(game.state.stack.is_empty(), "pilha limpa antes do pagamento");
    let cost = Cost::Mana(vec![ManaSymbol::Colored(Color::Green)]);
    if let Err(err) = cast::pay_cost(&mut game, P0, &cost, &[]) {
        panic!("pagar {{G}} com uma Floresta falhou: {err}");
    }

    assert!(
        game.state.stack.is_empty(),
        "CR 605.3a — a habilidade de mana não pode ter entrado na pilha"
    );
    assert!(
        is_tapped(&game, forest),
        "a Floresta tem de ter sido virada para produzir o mana"
    );
    assert_eq!(
        game.state.player(P0).mana_pool.total(),
        0,
        "o mana produzido foi consumido pelo próprio custo"
    );
}

#[test]
fn custo_adicional_de_sacrificio_e_pago_antes_de_resolver() {
    // CR 601.2h — todos os custos são pagos ao lançar/ativar, não na resolução.
    // O sacrifício, portanto, já aconteceu enquanto a habilidade está na pilha.
    let mut game = cost_game();
    let altar = put_ready(&mut game, "Bone Altar", P0);
    let goat = put_ready(&mut game, "Sacrificial Goat", P0);
    put_on_battlefield(&mut game, "Forest", P0);
    clear_pending_events(&mut game);

    let index = first_activated_index(&game, altar);
    let action = Action::ActivateAbility {
        source: altar,
        index,
        targets: Vec::new(),
        x: 0,
        mana_plan: Vec::new(),
    };
    assert!(
        cast::priority_actions(&game, P0).contains(&action),
        "a habilidade com custo de sacrifício tem de ser ativável"
    );
    let hand_before = hand_size(&game, P0);

    if let Err(err) = cast::execute(&mut game, P0, action) {
        panic!("ativar a habilidade falhou: {err}");
    }

    assert!(
        in_graveyard(&game, goat),
        "CR 601.2h — o sacrifício é custo: acontece na ativação, não na resolução"
    );
    assert_eq!(
        game.state.stack.len(),
        1,
        "a habilidade tem de estar na pilha, ainda não resolvida"
    );
    assert_eq!(
        hand_size(&game, P0),
        hand_before,
        "o efeito (comprar) ainda não pode ter acontecido"
    );

    resolve_stack(&mut game);
    assert_eq!(
        hand_size(&game, P0),
        hand_before + 1,
        "resolvida a habilidade, a compra acontece"
    );
}

#[test]
fn mana_nao_pago_impede_a_magica_de_aparecer_nas_acoes_legais() {
    // CR 601.2 — mágica cujo custo total não pode ser pago não é lançável, e o
    // motor não pode oferecê-la ao agente.
    let mut game = cost_game();
    let spell = put_in_hand(&mut game, "Expensive Blessing", P0);
    put_on_battlefield(&mut game, "Forest", P0);
    clear_pending_events(&mut game);

    assert_eq!(
        cast::available_mana(&game, P0).total,
        1,
        "um terreno, um mana disponível"
    );
    assert!(
        !castable(&game, spell),
        "custo {{5}} com 1 mana disponível não pode aparecer nas ações legais"
    );

    // Contraprova: com mana suficiente a mesma mágica aparece e é lançável.
    for _ in 0..4 {
        put_on_battlefield(&mut game, "Forest", P0);
    }
    clear_pending_events(&mut game);
    assert!(
        castable(&game, spell),
        "com 5 terrenos a mágica de custo {{5}} tem de aparecer"
    );
    let action = cast_action(&game, spell, 0);
    if let Err(err) = cast::execute(&mut game, P0, action) {
        panic!("lançar com mana suficiente falhou: {err}");
    }
    resolve_stack(&mut game);
    assert_eq!(life(&game, P0), 21, "a mágica resolveu");
}

#[test]
fn alvo_ilegal_tambem_impede_a_magica_de_aparecer() {
    // Complemento do item 60: CR 601.2c — sem alvo legal a mágica não pode ser
    // lançada, ainda que o mana esteja disponível. Sem esta metade, "não
    // aparece nas ações legais" poderia estar acontecendo pelo motivo errado.
    let defs = vec![
        land_def("Mountain", ManaSymbol::Colored(Color::Red)),
        creature_def("Vanilla Bear", "{1}{G}", 2, 2, &[]),
        instant_def_targeted(
            "Sniper Shot",
            "{R}",
            vec![TargetSpec {
                kind: TargetKind::Object(Selector::creatures()),
                description: "alvo de criatura".to_string(),
            }],
            Effect::DealDamage {
                amount: Value::Const(2),
                target: ObjRef::Target(0),
            },
        ),
    ];
    let mut game = game_with_defs(defs, passing_agents());
    goto_step(&mut game, Step::PrecombatMain);
    put_on_battlefield(&mut game, "Mountain", P0);
    let spell = put_in_hand(&mut game, "Sniper Shot", P0);
    clear_pending_events(&mut game);

    assert!(
        !castable(&game, spell),
        "CR 601.2c — sem criatura no campo não existe alvo legal"
    );

    let victim = put_ready(&mut game, "Vanilla Bear", P1);
    clear_pending_events(&mut game);
    let action = cast::priority_actions(&game, P0).into_iter().find(|a| {
        matches!(a, Action::CastSpell { object, targets, .. }
            if *object == spell && targets.contains(&TargetChoice::Object(victim)))
    });
    let Some(action) = action else {
        panic!("nenhuma combinação de alvo apontou para {victim}");
    };
    if let Err(err) = cast::execute(&mut game, P0, action) {
        panic!("lançar com alvo legal falhou: {err}");
    }
    resolve_stack(&mut game);
    assert!(
        in_graveyard(&game, victim),
        "CR 704.5g — 2 de dano numa 2/2 é letal"
    );
}
