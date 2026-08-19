//! Cada padrão é provado com uma carta REAL, campo a campo.
//!
//! O texto de cada carta é o oracle atual do Scryfall (bulk `oracle_cards` de
//! 2026-08-18), copiado como está — inclusive a redação nova, em que o nome
//! próprio virou "this creature". Teste que só confere que compilou não prova
//! fidelidade: o que interessa é que o IR gerado diga a mesma coisa que o
//! texto impresso.

use mtg_core::card::{Ability, CardDef};
use mtg_core::ir::{
    Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec,
    TokenSpec, Value, ZoneScope,
};
use mtg_core::mana::{Color, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Supertype, TypeLine};

use super::{parse_effect, Parsed};
use crate::{compile, CompileResult, OracleCard};

fn playable(card: &OracleCard) -> CardDef {
    match compile(card) {
        CompileResult::Playable(def) => def,
        CompileResult::Unsupported { reason, pattern } => {
            panic!("esperava jogável, veio Unsupported: {reason} / {pattern}")
        }
    }
}

fn rejected(card: &OracleCard) -> String {
    match compile(card) {
        CompileResult::Playable(def) => panic!("esperava Unsupported, compilou: {def:?}"),
        CompileResult::Unsupported { reason, .. } => reason,
    }
}

fn spell(name: &str, cost: &str, type_line: &str, text: &str) -> CardDef {
    playable(&OracleCard::new(name, cost, type_line, text))
}

fn creature(name: &str, cost: &str, type_line: &str, p: &str, t: &str, text: &str) -> CardDef {
    playable(&OracleCard::new(name, cost, type_line, text).with_pt(p, t))
}

/// O efeito da única habilidade disparada da carta.
fn trigger_effect(def: &CardDef) -> &Effect {
    let Some((_, trigger)) = def.triggered().next() else {
        panic!("esperava habilidade disparada em {}: {:?}", def.name, def.abilities);
    };
    &trigger.effect
}

fn trigger_targets(def: &CardDef) -> &[TargetSpec] {
    let Some((_, trigger)) = def.triggered().next() else {
        panic!("esperava habilidade disparada em {}", def.name);
    };
    &trigger.targets
}

fn sentence(text: &str) -> Parsed {
    match parse_effect(text) {
        Some(parsed) => parsed,
        None => panic!("frase não reconhecida: {text:?}"),
    }
}

const T0: ObjRef = ObjRef::Target(0);
const T1: ObjRef = ObjRef::Target(1);

// ---------------------------------------------------------------------------
// Exílio
// ---------------------------------------------------------------------------

#[test]
fn final_reward_exila_a_criatura_alvo() {
    let def = spell("Final Reward", "{4}{B}", "Instant", "Exile target creature.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::Exile { target: T0, until_source_leaves: false })
    );
    assert_eq!(def.spell_targets.len(), 1);
    assert_eq!(def.spell_targets[0].description, "target creature");
    assert_eq!(
        def.spell_targets[0].kind,
        TargetKind::Object(Selector::battlefield(Filter::HasType(CardType::Creature)))
    );
}

#[test]
fn devouring_light_exila_atacante_ou_bloqueadora() {
    // "attacking or blocking creature": o substantivo vale para as duas
    // alternativas, então o alvo é criatura atacando ou criatura bloqueando —
    // não "qualquer permanente atacando".
    let def = spell(
        "Devouring Light",
        "{1}{W}{W}",
        "Instant",
        "Convoke (Your creatures can help cast this spell.)\nExile target attacking or blocking creature.",
    );
    assert_eq!(def.abilities, vec![Ability::Keyword(Keyword::Convoke)]);
    assert_eq!(
        def.spell_effect,
        Some(Effect::Exile { target: T0, until_source_leaves: false })
    );
    let esperado = Selector::battlefield(Filter::Or(vec![
        Filter::And(vec![Filter::Attacking, Filter::HasType(CardType::Creature)]),
        Filter::And(vec![Filter::Blocking, Filter::HasType(CardType::Creature)]),
    ]));
    assert_eq!(def.spell_targets[0].kind, TargetKind::Object(esperado));
}

#[test]
fn banisher_priest_continua_fora_por_causa_do_retorno() {
    // "until this creature leaves the battlefield" devolve a criatura, e o
    // motor não rastreia essa volta. Compilar como exílio simples daria uma
    // carta melhor que a impressa.
    let reason = rejected(
        &OracleCard::new(
            "Banisher Priest",
            "{1}{W}{W}",
            "Creature — Human Cleric",
            "When this creature enters, exile target creature an opponent controls until this creature leaves the battlefield.",
        )
        .with_pt("2", "2"),
    );
    assert_eq!(reason, "linha de habilidade não reconhecida");
}

// ---------------------------------------------------------------------------
// Devolver à mão e reanimar
// ---------------------------------------------------------------------------

#[test]
fn unsummon_devolve_a_criatura_para_a_mao() {
    let def = spell("Unsummon", "{U}", "Instant", "Return target creature to its owner's hand.");
    assert_eq!(def.spell_effect, Some(Effect::ReturnToHand { target: T0 }));
    assert_eq!(def.spell_targets[0].description, "target creature");
}

#[test]
fn eye_of_nowhere_devolve_permanente() {
    let def = spell(
        "Eye of Nowhere",
        "{U}{U}",
        "Sorcery — Arcane",
        "Return target permanent to its owner's hand.",
    );
    assert_eq!(def.spell_effect, Some(Effect::ReturnToHand { target: T0 }));
    assert_eq!(
        def.spell_targets[0].kind,
        TargetKind::Object(Selector::battlefield(Filter::Any))
    );
}

#[test]
fn zombify_reanima_do_seu_cemiterio() {
    let def = spell(
        "Zombify",
        "{3}{B}",
        "Sorcery",
        "Return target creature card from your graveyard to the battlefield.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::ReturnFromGraveyardToBattlefield { target: T0 })
    );
    assert_eq!(
        def.spell_targets[0].kind,
        TargetKind::Object(Selector {
            zone: ZoneScope::Graveyard,
            filter: Filter::HasType(CardType::Creature),
            owner_scope: Some(PlayerRef::You),
            max: None,
        })
    );
}

#[test]
fn raise_dead_devolve_do_cemiterio_para_a_mao() {
    let def = spell(
        "Raise Dead",
        "{B}",
        "Sorcery",
        "Return target creature card from your graveyard to your hand.",
    );
    assert_eq!(def.spell_effect, Some(Effect::ReturnToHand { target: T0 }));
    let TargetKind::Object(selector) = &def.spell_targets[0].kind else {
        panic!("alvo deveria ser objeto: {:?}", def.spell_targets[0]);
    };
    assert_eq!(selector.zone, ZoneScope::Graveyard);
    assert_eq!(selector.owner_scope, Some(PlayerRef::You));
}

// ---------------------------------------------------------------------------
// Virar, desvirar, congelar
// ---------------------------------------------------------------------------

#[test]
fn heavy_infantry_vira_criatura_do_oponente() {
    let def = creature(
        "Heavy Infantry",
        "{4}{W}",
        "Creature — Human Soldier",
        "3",
        "4",
        "When this creature enters, tap target creature an opponent controls.",
    );
    assert_eq!(trigger_effect(&def), &Effect::Tap { target: T0 });
    assert_eq!(
        trigger_targets(&def)[0].kind,
        TargetKind::Object(Selector::creatures().opponents())
    );
}

#[test]
fn frost_lynx_vira_e_impede_o_desvirar() {
    // Duas frases, um alvo só: "that creature" é o alvo que a primeira
    // escolheu, não um alvo novo.
    let def = creature(
        "Frost Lynx",
        "{2}{U}",
        "Creature — Elemental Cat",
        "2",
        "2",
        "When this creature enters, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
    );
    assert_eq!(
        trigger_effect(&def),
        &Effect::Sequence(vec![Effect::Tap { target: T0 }, Effect::Freeze { target: T0 }])
    );
    assert_eq!(trigger_targets(&def).len(), 1);
}

#[test]
fn inspirit_desvira_e_engorda_o_mesmo_alvo() {
    let def = spell(
        "Inspirit",
        "{2}{W}",
        "Instant",
        "Untap target creature. It gets +2/+4 until end of turn.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::Untap { target: T0 },
            Effect::ModifyPT {
                target: T0,
                power: Value::Const(2),
                toughness: Value::Const(4),
                duration: Duration::EndOfTurn,
            },
        ]))
    );
    assert_eq!(def.spell_targets.len(), 1);
}

#[test]
fn burst_of_energy_desvira_permanente() {
    let def = spell("Burst of Energy", "{W}", "Instant", "Untap target permanent.");
    assert_eq!(def.spell_effect, Some(Effect::Untap { target: T0 }));
}

// ---------------------------------------------------------------------------
// Biblioteca
// ---------------------------------------------------------------------------

#[test]
fn tome_scour_moe_cinco_cartas_do_alvo() {
    let def = spell("Tome Scour", "{U}", "Sorcery", "Target player mills five cards.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::Mill { count: Value::Const(5), player: PlayerRef::Target(0) })
    );
    assert_eq!(def.spell_targets[0].kind, TargetKind::Player(PlayerRef::Each));
}

#[test]
fn omenspeaker_faz_scry_2_ao_entrar() {
    let def = creature(
        "Omenspeaker",
        "{1}{U}",
        "Creature — Human Wizard",
        "1",
        "3",
        "When this creature enters, scry 2. (Look at the top two cards of your library.)",
    );
    assert_eq!(
        trigger_effect(&def),
        &Effect::Scry { count: Value::Const(2), player: PlayerRef::You }
    );
}

#[test]
fn dimir_informant_faz_surveil_2_ao_entrar() {
    let def = creature(
        "Dimir Informant",
        "{2}{U}",
        "Creature — Human Rogue",
        "1",
        "4",
        "When this creature enters, surveil 2.",
    );
    assert_eq!(
        trigger_effect(&def),
        &Effect::Surveil { count: Value::Const(2), player: PlayerRef::You }
    );
}

#[test]
fn serum_visions_compra_e_depois_faz_scry() {
    // A ordem importa: comprar antes de olhar o topo é outra carta.
    let def = spell("Serum Visions", "{U}", "Sorcery", "Draw a card. Scry 2.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You },
            Effect::Scry { count: Value::Const(2), player: PlayerRef::You },
        ]))
    );
}

#[test]
fn lay_of_the_land_busca_terreno_basico_e_embaralha() {
    let def = spell(
        "Lay of the Land",
        "{G}",
        "Sorcery",
        "Search your library for a basic land card, reveal it, put it into your hand, then shuffle.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::SearchLibrary {
                count: Value::Const(1),
                filter: Filter::And(vec![
                    Filter::HasSupertype(Supertype::Basic),
                    Filter::HasType(CardType::Land),
                ]),
                player: PlayerRef::You,
                to_hand: true,
            },
            Effect::ShuffleLibrary { player: PlayerRef::You },
        ]))
    );
    assert!(def.spell_targets.is_empty(), "procurar não tem alvo");
}

#[test]
fn eladamris_call_busca_criatura() {
    let def = spell(
        "Eladamri's Call",
        "{G}{W}",
        "Instant",
        "Search your library for a creature card, reveal that card, put it into your hand, then shuffle.",
    );
    let Some(Effect::Sequence(steps)) = &def.spell_effect else {
        panic!("esperava sequência: {:?}", def.spell_effect);
    };
    assert_eq!(
        steps[0],
        Effect::SearchLibrary {
            count: Value::Const(1),
            filter: Filter::HasType(CardType::Creature),
            player: PlayerRef::You,
            to_hand: true,
        }
    );
}

#[test]
fn rampant_growth_continua_fora_porque_o_terreno_entra_virado() {
    // O IR não sabe dizer "entra virado" numa busca. Um Rampant Growth que
    // trouxesse o terreno desvirado seria uma carta melhor que a impressa.
    let reason = rejected(&OracleCard::new(
        "Rampant Growth",
        "{1}{G}",
        "Sorcery",
        "Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    ));
    assert_eq!(reason, "linha de habilidade não reconhecida");
}

#[test]
fn ponder_continua_fora_porque_olhar_o_topo_nao_esta_no_ir() {
    let reason = rejected(&OracleCard::new(
        "Ponder",
        "{U}",
        "Sorcery",
        "Look at the top three cards of your library, then put them back in any order. You may shuffle.\nDraw a card.",
    ));
    assert_eq!(reason, "linha de habilidade não reconhecida");
}

// ---------------------------------------------------------------------------
// Descarte e sacrifício
// ---------------------------------------------------------------------------

#[test]
fn mind_rot_faz_o_alvo_descartar_duas() {
    let def = spell("Mind Rot", "{2}{B}", "Sorcery", "Target player discards two cards.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::Discard {
            count: Value::Const(2),
            player: PlayerRef::Target(0),
            filter: Filter::Any,
            random: false,
        })
    );
}

#[test]
fn hymn_to_tourach_descarta_ao_acaso() {
    let def = spell(
        "Hymn to Tourach",
        "{B}{B}",
        "Sorcery",
        "Target player discards two cards at random.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Discard {
            count: Value::Const(2),
            player: PlayerRef::Target(0),
            filter: Filter::Any,
            random: true,
        })
    );
}

#[test]
fn unnerve_atinge_cada_oponente_sem_alvo() {
    let def = spell("Unnerve", "{3}{B}", "Sorcery", "Each opponent discards two cards.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::Discard {
            count: Value::Const(2),
            player: PlayerRef::Opponents,
            filter: Filter::Any,
            random: false,
        })
    );
    assert!(def.spell_targets.is_empty(), "\"cada oponente\" não é alvo");
}

#[test]
fn diabolic_edict_faz_o_alvo_sacrificar_criatura() {
    let def = spell(
        "Diabolic Edict",
        "{1}{B}",
        "Instant",
        "Target player sacrifices a creature of their choice.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sacrifice {
            player: PlayerRef::Target(0),
            count: Value::Const(1),
            filter: Filter::HasType(CardType::Creature),
        })
    );
}

#[test]
fn innocent_blood_atinge_cada_jogador() {
    let def = spell(
        "Innocent Blood",
        "{B}",
        "Sorcery",
        "Each player sacrifices a creature of their choice.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sacrifice {
            player: PlayerRef::Each,
            count: Value::Const(1),
            filter: Filter::HasType(CardType::Creature),
        })
    );
}

// ---------------------------------------------------------------------------
// Fichas
// ---------------------------------------------------------------------------

#[test]
fn krenkos_command_cria_duas_fichas_1_1_vermelhas() {
    let def = spell(
        "Krenko's Command",
        "{1}{R}",
        "Sorcery",
        "Create two 1/1 red Goblin creature tokens.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::CreateToken {
            spec: TokenSpec {
                name: "Goblin".to_string(),
                type_line: TypeLine {
                    supertypes: Vec::new(),
                    types: vec![CardType::Creature],
                    subtypes: vec!["Goblin".to_string()],
                },
                colors: vec![Color::Red],
                power: 1,
                toughness: 1,
                keywords: Vec::new(),
                art_key: None,
            },
            count: Value::Const(2),
            controller: PlayerRef::You,
        })
    );
}

#[test]
fn midnight_haunting_cria_fichas_com_flying() {
    let def = spell(
        "Midnight Haunting",
        "{2}{W}",
        "Instant",
        "Create two 1/1 white Spirit creature tokens with flying.",
    );
    let Some(Effect::CreateToken { spec, count, controller }) = &def.spell_effect else {
        panic!("esperava ficha: {:?}", def.spell_effect);
    };
    assert_eq!(spec.name, "Spirit");
    assert_eq!(spec.colors, vec![Color::White]);
    assert_eq!((spec.power, spec.toughness), (1, 1));
    assert_eq!(spec.keywords, vec![Keyword::Flying]);
    assert_eq!(count, &Value::Const(2));
    assert_eq!(controller, &PlayerRef::You);
}

#[test]
fn brazen_freebooter_continua_fora_porque_treasure_nao_tem_pt() {
    // Ficha predefinida tem comportamento que não está escrito na carta.
    let reason = rejected(
        &OracleCard::new(
            "Brazen Freebooter",
            "{3}{R}",
            "Creature — Human Pirate",
            "When this creature enters, create a Treasure token.",
        )
        .with_pt("2", "2"),
    );
    assert_eq!(reason, "linha de habilidade não reconhecida");
}

// ---------------------------------------------------------------------------
// Marcadores
// ---------------------------------------------------------------------------

#[test]
fn battlegrowth_poe_um_marcador_de_1_1() {
    let def = spell("Battlegrowth", "{G}", "Instant", "Put a +1/+1 counter on target creature.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::AddCounters {
            target: T0,
            kind: CounterKind::PlusOnePlusOne,
            count: Value::Const(1),
        })
    );
}

#[test]
fn bond_beetle_poe_marcador_ao_entrar() {
    let def = creature(
        "Bond Beetle",
        "{G}",
        "Creature — Insect",
        "0",
        "1",
        "When this creature enters, put a +1/+1 counter on target creature.",
    );
    assert_eq!(
        trigger_effect(&def),
        &Effect::AddCounters {
            target: T0,
            kind: CounterKind::PlusOnePlusOne,
            count: Value::Const(1),
        }
    );
}

#[test]
fn remover_marcador_e_a_frase_de_woeleecher() {
    // A frase vive numa habilidade ativada ("{W}, {T}: Remove a -1/-1 counter
    // from target creature."), que ainda não é compilada; o efeito em si já é.
    let parsed = sentence("remove a -1/-1 counter from target creature");
    assert_eq!(
        parsed.effect,
        Effect::RemoveCounters {
            target: T0,
            kind: CounterKind::MinusOneMinusOne,
            count: Value::Const(1),
        }
    );
    assert_eq!(parsed.targets.len(), 1);
}

// ---------------------------------------------------------------------------
// Luta, controle, mana
// ---------------------------------------------------------------------------

#[test]
fn prey_upon_luta_com_dois_alvos_distintos() {
    let def = spell(
        "Prey Upon",
        "{G}",
        "Sorcery",
        "Target creature you control fights target creature you don't control.",
    );
    assert_eq!(def.spell_effect, Some(Effect::Fight { a: T0, b: T1 }));
    assert_eq!(def.spell_targets.len(), 2);
    assert_eq!(
        def.spell_targets[0].kind,
        TargetKind::Object(Selector::creatures().yours())
    );
    assert_eq!(
        def.spell_targets[1].kind,
        TargetKind::Object(Selector::creatures().opponents())
    );
}

#[test]
fn act_of_treason_ganha_controle_desvira_e_da_haste() {
    let def = spell(
        "Act of Treason",
        "{2}{R}",
        "Sorcery",
        "Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::GainControl {
                target: T0,
                player: PlayerRef::You,
                duration: Duration::EndOfTurn,
            },
            Effect::Untap { target: T0 },
            Effect::GrantKeywords {
                target: T0,
                keywords: vec![Keyword::Haste],
                duration: Duration::EndOfTurn,
            },
        ]))
    );
    assert_eq!(def.spell_targets.len(), 1, "as três frases falam do mesmo alvo");
}

#[test]
fn traitorous_blood_concede_duas_palavras_chave() {
    let def = spell(
        "Traitorous Blood",
        "{1}{R}{R}",
        "Sorcery",
        "Gain control of target creature until end of turn. Untap it. It gains trample and haste until end of turn.",
    );
    let Some(Effect::Sequence(steps)) = &def.spell_effect else {
        panic!("esperava sequência: {:?}", def.spell_effect);
    };
    assert_eq!(
        steps[2],
        Effect::GrantKeywords {
            target: T0,
            keywords: vec![Keyword::Trample, Keyword::Haste],
            duration: Duration::EndOfTurn,
        }
    );
}

#[test]
fn dark_ritual_produz_tres_manas_pretos() {
    let def = spell("Dark Ritual", "{B}", "Instant", "Add {B}{B}{B}.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::AddMana {
            symbols: vec![
                ManaSymbol::Colored(Color::Black),
                ManaSymbol::Colored(Color::Black),
                ManaSymbol::Colored(Color::Black),
            ],
            player: PlayerRef::You,
        })
    );
}

// ---------------------------------------------------------------------------
// P/T, evasão e vida
// ---------------------------------------------------------------------------

#[test]
fn infiltrate_tira_o_bloqueio_ate_o_fim_do_turno() {
    let def = spell("Infiltrate", "{U}", "Instant", "Target creature can't be blocked this turn.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::CantBeBlocked { target: T0, duration: Duration::EndOfTurn })
    );
}

#[test]
fn off_balance_impede_ataque_e_bloqueio() {
    let def = spell(
        "Off Balance",
        "{W}",
        "Instant",
        "Target creature can't attack or block this turn.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::CantAttackOrBlock { target: T0, duration: Duration::EndOfTurn })
    );
}

#[test]
fn disfigure_encolhe_a_criatura_alvo() {
    let def = spell("Disfigure", "{B}", "Instant", "Target creature gets -2/-2 until end of turn.");
    assert_eq!(
        def.spell_effect,
        Some(Effect::ModifyPT {
            target: T0,
            power: Value::Const(-2),
            toughness: Value::Const(-2),
            duration: Duration::EndOfTurn,
        })
    );
}

#[test]
fn bump_in_the_night_faz_o_oponente_alvo_perder_vida() {
    let def = spell(
        "Bump in the Night",
        "{B}",
        "Sorcery",
        "Target opponent loses 3 life.\nFlashback {5}{R}",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::LoseLife { amount: Value::Const(3), player: PlayerRef::Target(0) })
    );
    assert_eq!(def.spell_targets[0].kind, TargetKind::Player(PlayerRef::Opponents));
    assert_eq!(
        def.abilities,
        vec![Ability::Keyword(Keyword::Flashback(Box::new(Cost::Mana(vec![
            ManaSymbol::Generic(5),
            ManaSymbol::Colored(Color::Red),
        ]))))]
    );
}

#[test]
fn terminate_marca_que_nao_pode_regenerar() {
    let def = spell(
        "Terminate",
        "{B}{R}",
        "Instant",
        "Destroy target creature. It can't be regenerated.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Destroy { target: T0, no_regeneration: true })
    );
    assert_eq!(def.spell_targets.len(), 1);
}

#[test]
fn doom_blade_so_mira_criatura_nao_preta() {
    let def = spell("Doom Blade", "{1}{B}", "Instant", "Destroy target nonblack creature.");
    assert_eq!(
        def.spell_targets[0].kind,
        TargetKind::Object(Selector::battlefield(Filter::And(vec![
            Filter::Not(Box::new(Filter::HasColor(Color::Black))),
            Filter::HasType(CardType::Creature),
        ])))
    );
}

// ---------------------------------------------------------------------------
// Fim de jogo
// ---------------------------------------------------------------------------

#[test]
fn ganhar_e_perder_a_partida_sao_as_frases_de_test_of_endurance() {
    // "At the beginning of your upkeep, if you have 50 or more life, you win
    // the game." — o gatilho ainda não compila, a frase final já.
    assert_eq!(
        sentence("you win the game").effect,
        Effect::WinGame { player: PlayerRef::You }
    );
    assert_eq!(
        sentence("you lose the game").effect,
        Effect::LoseGame { player: PlayerRef::You }
    );
}

// ---------------------------------------------------------------------------
// Composição e recusa
// ---------------------------------------------------------------------------

#[test]
fn lichs_caress_vira_sequencia_na_ordem_escrita() {
    let def = spell(
        "Lich's Caress",
        "{3}{B}{B}",
        "Sorcery",
        "Destroy target creature. You gain 3 life.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::Destroy { target: T0, no_regeneration: false },
            Effect::GainLife { amount: Value::Const(3), player: PlayerRef::You },
        ]))
    );
    assert_eq!(def.spell_targets.len(), 1);
}

#[test]
fn take_into_custody_usa_it_para_o_alvo_anterior() {
    let def = spell(
        "Take into Custody",
        "{U}",
        "Instant",
        "Tap target creature. It doesn't untap during its controller's next untap step.",
    );
    assert_eq!(
        def.spell_effect,
        Some(Effect::Sequence(vec![
            Effect::Tap { target: T0 },
            Effect::Freeze { target: T0 },
        ]))
    );
    assert_eq!(def.spell_targets.len(), 1);
}

#[test]
fn frase_com_pedaco_nao_entendido_nao_vira_ir_parcial() {
    // Cada uma tem uma metade que este compilador entende. Compilar só ela
    // daria uma carta diferente da impressa, então nenhuma pode passar.
    let casos = [
        // Reality Shift: o exílio compila, "manifests" não.
        ("Reality Shift", "{1}{U}", "Instant",
         "Exile target creature. Its controller manifests the top card of their library."),
        // Wrench Mind: o "unless" muda quem descarta o quê.
        ("Wrench Mind", "{B}{B}", "Sorcery",
         "Target player discards two cards unless they discard an artifact card."),
    ];
    for (name, cost, type_line, text) in casos {
        let card = OracleCard::new(name, cost, type_line, text);
        assert!(
            matches!(compile(&card), CompileResult::Unsupported { .. }),
            "não podia compilar: {name}"
        );
    }
    // Oceanus Dragon: virar compila, "goad" não.
    let dragon = OracleCard::new(
        "Oceanus Dragon",
        "{4}{U}{U}",
        "Creature — Dragon",
        "Flying
When this creature enters, tap target creature an opponent controls. Goad it.",
    )
    .with_pt("3", "5");
    assert!(matches!(compile(&dragon), CompileResult::Unsupported { .. }));
}

#[test]
fn alvo_de_frase_diferente_ganha_indice_proprio() {
    // Dois alvos em frases diferentes não podem virar o mesmo índice.
    let parsed = sentence("destroy target artifact. tap target creature");
    assert_eq!(
        parsed.effect,
        Effect::Sequence(vec![
            Effect::Destroy { target: T0, no_regeneration: false },
            Effect::Tap { target: T1 },
        ])
    );
    assert_eq!(parsed.targets.len(), 2);
    assert_eq!(parsed.targets[0].description, "target artifact");
    assert_eq!(parsed.targets[1].description, "target creature");
}

#[test]
fn ate_o_fim_do_turno_na_frente_e_a_mesma_frase() {
    let direto = sentence("target creature gets +2/+2 until end of turn");
    let invertido = sentence("until end of turn, target creature gets +2/+2");
    assert_eq!(direto, invertido);
}
