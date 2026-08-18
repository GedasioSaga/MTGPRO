//! Compilador de texto de oracle para o IR do motor.
//!
//! O Scryfall entrega TEXTO; o motor roda sobre `CardDef`. Importar 35 mil
//! cartas sem este passo daria 35 mil fichas com arte e zero comportamento.
//! Por isso o resultado é binário: ou a carta vira um `CardDef` fiel ao que
//! está escrito, ou vira `Unsupported` — e continua no catálogo, navegável e
//! buscável, mas fora de deck.
//!
//! # A regra que governa tudo aqui
//!
//! **Fidelidade acima de cobertura.** Todo reconhecedor casa a frase INTEIRA.
//! Sobrou texto que não entendemos, é `Unsupported`, nunca "quase". Carta que
//! joga diferente do texto impresso quebra a partida em silêncio, e um bug
//! silencioso custa mais caro que uma carta ausente.
//!
//! # Entrada
//!
//! [`OracleCard`] é uma struct própria, não o JSON do Scryfall: o compilador
//! não depende do crate de importação e roda inteiro em teste, sem rede.
#![forbid(unsafe_code)]

mod effects;
mod keywords;
pub mod layouts;
mod parse;
mod text;

pub use effects::Parsed;
pub use text::{normalize_lines, oracle_pattern, pattern_of, OracleLine};

use mtg_core::card::{
    Ability, CardDef, ManaAbility, ManaProduction, ReplacementAbility, ReplacementEvent,
    TriggerCondition, TriggeredAbility,
};
use mtg_core::ids::CardDefId;
use mtg_core::ir::{Condition, Cost, Effect, Filter, Selector, TargetSpec};
use mtg_core::mana::ManaSymbol;
use mtg_core::types::CardType;

/// Os campos do Scryfall de que o compilador precisa.
///
/// Struct própria, e não `serde_json::Value`, para que o compilador seja
/// testável sem rede e independente de mudanças no formato do bulk. O
/// importador é quem adapta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OracleCard {
    pub name: String,
    /// Custo na forma impressa: `"{1}{G}"`. Vazio é válido (terreno).
    pub mana_cost: String,
    /// `"Legendary Creature — Human Soldier"`.
    pub type_line: String,
    pub oracle_text: String,
    /// Vem como texto porque pode ser `"*"` ou `"1+*"`.
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    pub rarity: String,
    pub set_code: String,
    pub collector_number: String,
    pub artist: Option<String>,
    pub flavor_text: Option<String>,
    /// `art_key` do `CardDef`; `None` cai no nome da carta.
    pub art_key: Option<String>,
    /// `layout` do Scryfall. Vazio conta como `"normal"`.
    pub layout: String,
}

/// Nome usado na especificação da API para a entrada do compilador.
pub type ScryfallLike = OracleCard;

impl OracleCard {
    /// Construtor enxuto para teste e para o caminho comum do importador.
    pub fn new(name: &str, mana_cost: &str, type_line: &str, oracle_text: &str) -> OracleCard {
        OracleCard {
            name: name.to_string(),
            mana_cost: mana_cost.to_string(),
            type_line: type_line.to_string(),
            oracle_text: oracle_text.to_string(),
            rarity: "common".to_string(),
            set_code: "TEST".to_string(),
            ..OracleCard::default()
        }
    }

    pub fn with_pt(mut self, power: &str, toughness: &str) -> OracleCard {
        self.power = Some(power.to_string());
        self.toughness = Some(toughness.to_string());
        self
    }
}

/// Resultado da compilação de uma carta.
///
/// `Playable` é bem maior que `Unsupported`, mas o enum é criado uma vez por
/// carta num laço de streaming e morre em seguida — encaixotar só trocaria
/// bytes de pilha por uma alocação por carta.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileResult {
    Playable(CardDef),
    Unsupported {
        /// Por que não compilou, para o humano.
        reason: String,
        /// Texto normalizado (nome -> `~`, números -> `N`). Padrões iguais
        /// agrupam, então o importador consegue contar frequência e decidir
        /// que família de texto compensa implementar em seguida.
        ///
        /// Quando a falha é de uma linha específica, é o padrão daquela linha;
        /// quando é da carta inteira (layout, P/T, custo), é o texto todo.
        pattern: String,
    },
}

impl CompileResult {
    pub fn is_playable(&self) -> bool {
        matches!(self, CompileResult::Playable(_))
    }
    pub fn card(&self) -> Option<&CardDef> {
        match self {
            CompileResult::Playable(c) => Some(c),
            CompileResult::Unsupported { .. } => None,
        }
    }
}

fn unsupported(reason: impl Into<String>, normalized: &str) -> CompileResult {
    CompileResult::Unsupported {
        reason: reason.into(),
        pattern: text::pattern_of(normalized),
    }
}

/// Compila uma carta do Scryfall para o IR, ou explica por que não dá.
pub fn compile(card: &ScryfallLike) -> CompileResult {
    let whole = || {
        text::normalize_lines(&card.oracle_text, &card.name)
            .into_iter()
            .map(|l| l.norm)
            .collect::<Vec<_>>()
            .join("\n")
    };

    if !card.layout.is_empty() && card.layout != "normal" {
        return unsupported(format!("layout não modelado: {}", card.layout), &whole());
    }

    let Some(type_line) = parse::parse_type_line(&card.type_line) else {
        return unsupported(format!("linha de tipo não reconhecida: {}", card.type_line), &whole());
    };
    // Lealdade e contadores de defesa não são vocabulário do IR: um
    // planeswalker sem habilidade compilada seria uma carta que não faz nada.
    if type_line.has_type(CardType::Planeswalker) {
        return unsupported("planeswalker: habilidades de lealdade não são compiladas", &whole());
    }
    if type_line.has_type(CardType::Battle) {
        return unsupported("battle: contadores de defesa não são modelados", &whole());
    }

    let Some(mana_cost) = parse::parse_mana_cost(&card.mana_cost) else {
        return unsupported(format!("custo de mana não reconhecido: {}", card.mana_cost), &whole());
    };
    // {X} no custo sempre implica texto que fala de X, e o texto que fala de X
    // não é reconhecido por nenhum padrão daqui.
    if mana_cost.has_x() {
        return unsupported("custo com {X}", &whole());
    }

    let power = card.power.as_deref().and_then(parse_stat);
    let toughness = card.toughness.as_deref().and_then(parse_stat);
    if type_line.is_creature() && (power.is_none() || toughness.is_none()) {
        return unsupported("criatura com P/T variável ou ausente", &whole());
    }
    let loyalty = card.loyalty.as_deref().and_then(parse_stat);

    let lines = text::normalize_lines(&card.oracle_text, &card.name);
    let is_spell = type_line.types.iter().any(|t| t.is_spell_only());

    let mut abilities: Vec<Ability> = Vec::new();
    let mut spell_effect: Option<Effect> = None;
    let mut spell_targets: Vec<TargetSpec> = Vec::new();

    // Habilidade de mana intrínseca do subtipo de terreno (CR 305.6). Vale
    // para qualquer terreno com o subtipo, não só para o básico — é assim que
    // Tropical Island produz duas cores sem uma linha de texto sequer.
    if type_line.is_land() {
        for subtype in &type_line.subtypes {
            if let Some(symbol) = parse::land_subtype_mana(subtype) {
                abilities.push(Ability::Mana(intrinsic_mana(symbol)));
            }
        }
    }

    for line in &lines {
        if let Some(kws) = keywords::parse_keyword_line(&line.norm) {
            abilities.extend(kws.into_iter().map(Ability::Keyword));
            continue;
        }
        if let Some(mana) = parse_mana_ability(&line.norm, &line.raw) {
            abilities.push(Ability::Mana(mana));
            continue;
        }
        if let Some(trigger) = parse_etb(&line.norm, &line.raw) {
            abilities.push(Ability::Triggered(trigger));
            continue;
        }
        if is_enters_tapped(&line.norm) {
            abilities.push(Ability::Replacement(ReplacementAbility {
                event: ReplacementEvent::EntersTapped,
                replacement: Effect::Nothing,
                text: line.raw.clone(),
            }));
            continue;
        }
        if is_spell {
            if let Some(parsed) = effects::parse_effect(&line.norm) {
                if spell_effect.is_some() {
                    return unsupported("feitiço com mais de um efeito reconhecido", &line.norm);
                }
                spell_effect = Some(parsed.effect);
                spell_targets = parsed.targets;
                continue;
            }
        }
        return unsupported("linha de habilidade não reconhecida", &line.norm);
    }

    if is_spell && spell_effect.is_none() {
        return unsupported("feitiço sem efeito reconhecido", &whole());
    }

    CompileResult::Playable(CardDef {
        id: CardDefId(0),
        name: card.name.clone(),
        mana_cost,
        type_line,
        color_override: None,
        power,
        toughness,
        loyalty,
        abilities,
        spell_effect,
        spell_targets,
        oracle_text: card.oracle_text.clone(),
        flavor_text: card.flavor_text.clone(),
        rarity: parse::parse_rarity(&card.rarity),
        set_code: card.set_code.clone(),
        collector_number: card.collector_number.clone(),
        artist: card.artist.clone(),
        art_key: card.art_key.clone().or_else(|| Some(card.name.clone())),
    })
}

fn parse_stat(raw: &str) -> Option<i32> {
    raw.trim().parse::<i32>().ok()
}

fn intrinsic_mana(symbol: ManaSymbol) -> ManaAbility {
    ManaAbility {
        cost: Cost::Tap,
        production: ManaProduction::Fixed(vec![symbol]),
        restriction: Condition::Always,
        text: format!("{{T}}: Add {}.", parse::render_symbol(symbol)),
    }
}

/// `"{T}: Add {G}."`, `"{T}: Add {W} or {U}."`, `"{T}: Add one mana of any color."`
fn parse_mana_ability(norm: &str, raw: &str) -> Option<ManaAbility> {
    let body = norm.trim().trim_end_matches('.').trim();
    let rest = body.strip_prefix("{t}: add ")?.trim();

    let production = if rest == "one mana of any color" {
        ManaProduction::AnyColor(1)
    } else if rest.contains(" or ") {
        // "{w}, {u}, or {b}" e "{w} or {u}" viram a mesma lista.
        let flattened = rest.replace(", or ", " or ").replace(", ", " or ");
        let mut symbols = Vec::new();
        for part in flattened.split(" or ") {
            symbols.push(parse::parse_braced_symbol(part.trim())?);
        }
        ManaProduction::OneOf(symbols)
    } else {
        let cost = parse::parse_mana_cost(rest)?;
        if cost.symbols.is_empty() {
            return None;
        }
        ManaProduction::Fixed(cost.symbols)
    };

    Some(ManaAbility {
        cost: Cost::Tap,
        production,
        restriction: Condition::Always,
        text: raw.to_string(),
    })
}

/// `"~ enters tapped."` — e a redação antiga, `"~ enters the battlefield tapped."`
fn is_enters_tapped(norm: &str) -> bool {
    let body = norm.trim().trim_end_matches('.').trim();
    body == "~ enters tapped" || body == "~ enters the battlefield tapped"
}

/// `"When ~ enters the battlefield, <efeito>."` — e a redação nova,
/// `"When ~ enters, <efeito>."`, que o Scryfall passou a usar.
fn parse_etb(norm: &str, raw: &str) -> Option<TriggeredAbility> {
    let body = norm.trim().trim_end_matches('.').trim();
    let rest = body
        .strip_prefix("when ~ enters the battlefield, ")
        .or_else(|| body.strip_prefix("when ~ enters, "))?;
    let parsed = effects::parse_effect(rest)?;
    Some(TriggeredAbility {
        trigger: TriggerCondition::EntersBattlefield(Selector::battlefield(Filter::IsSelf)),
        intervening_if: Condition::Always,
        targets: parsed.targets,
        effect: parsed.effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ir::{Duration, Keyword, ObjRef, PlayerRef, TargetKind, Value};
    use mtg_core::mana::{Color, ManaSymbol};

    fn playable(card: &OracleCard) -> CardDef {
        match compile(card) {
            CompileResult::Playable(def) => def,
            CompileResult::Unsupported { reason, pattern } => {
                panic!("esperava jogável, veio Unsupported: {reason} / {pattern}")
            }
        }
    }

    fn rejected(card: &OracleCard) -> (String, String) {
        match compile(card) {
            CompileResult::Playable(def) => panic!("esperava Unsupported, compilou: {def:?}"),
            CompileResult::Unsupported { reason, pattern } => (reason, pattern),
        }
    }

    #[test]
    fn grizzly_bears_vira_criatura_2_2_sem_habilidade() {
        let def = playable(
            &OracleCard::new("Grizzly Bears", "{1}{G}", "Creature — Bear", "").with_pt("2", "2"),
        );
        assert_eq!(def.power, Some(2));
        assert_eq!(def.toughness, Some(2));
        assert!(def.abilities.is_empty());
        assert!(def.spell_effect.is_none());
        assert_eq!(def.mana_value(), 2);
    }

    #[test]
    fn serra_angel_ganha_flying_e_vigilance() {
        let def = playable(
            &OracleCard::new("Serra Angel", "{3}{W}{W}", "Creature — Angel", "Flying, vigilance")
                .with_pt("4", "4"),
        );
        assert_eq!(
            def.abilities,
            vec![
                Ability::Keyword(Keyword::Flying),
                Ability::Keyword(Keyword::Vigilance)
            ]
        );
    }

    #[test]
    fn french_vanilla_cobre_lista_com_virgula() {
        let def = playable(
            &OracleCard::new("Rager", "{2}{R}", "Creature — Goblin", "Trample, haste")
                .with_pt("3", "1"),
        );
        assert_eq!(
            def.abilities,
            vec![
                Ability::Keyword(Keyword::Trample),
                Ability::Keyword(Keyword::Haste)
            ]
        );
    }

    #[test]
    fn toda_palavra_chave_simples_do_ir_tem_mapeamento() {
        let esperado = [
            ("Flying", Keyword::Flying),
            ("Reach", Keyword::Reach),
            ("Trample", Keyword::Trample),
            ("First strike", Keyword::FirstStrike),
            ("Double strike", Keyword::DoubleStrike),
            ("Deathtouch", Keyword::Deathtouch),
            ("Lifelink", Keyword::Lifelink),
            ("Vigilance", Keyword::Vigilance),
            ("Haste", Keyword::Haste),
            ("Menace", Keyword::Menace),
            ("Defender", Keyword::Defender),
            ("Flash", Keyword::Flash),
            ("Hexproof", Keyword::Hexproof),
            ("Shroud", Keyword::Shroud),
            ("Indestructible", Keyword::Indestructible),
            ("Prowess", Keyword::Prowess),
            ("Intimidate", Keyword::Intimidate),
            ("Fear", Keyword::Fear),
            ("Skulk", Keyword::Skulk),
            ("Exalted", Keyword::Exalted),
            ("Riot", Keyword::Riot),
            ("Convoke", Keyword::Convoke),
            ("Delve", Keyword::Delve),
            ("Cascade", Keyword::Cascade),
            ("Storm", Keyword::Storm),
            ("Protection from red", Keyword::Protection(Color::Red)),
            ("Islandwalk", Keyword::Landwalk("Island".to_string())),
            ("Annihilator 2", Keyword::Annihilator(2)),
            ("Afflict 3", Keyword::Afflict(3)),
        ];
        for (texto, kw) in esperado {
            let def = playable(
                &OracleCard::new("Tester", "{1}", "Creature — Human", texto).with_pt("1", "1"),
            );
            assert_eq!(def.abilities, vec![Ability::Keyword(kw)], "falhou em {texto:?}");
        }
    }

    #[test]
    fn palavra_chave_com_custo_carrega_o_custo() {
        let def = playable(
            &OracleCard::new("Warder", "{2}{U}", "Creature — Merfolk", "Ward {2}")
                .with_pt("2", "2"),
        );
        assert_eq!(
            def.abilities,
            vec![Ability::Keyword(Keyword::Ward(Box::new(Cost::Mana(vec![
                ManaSymbol::Generic(2)
            ]))))]
        );
    }

    #[test]
    fn lightning_bolt_vira_dano_3_em_any_target() {
        let def = playable(&OracleCard::new(
            "Lightning Bolt",
            "{R}",
            "Instant",
            "Lightning Bolt deals 3 damage to any target.",
        ));
        assert_eq!(
            def.spell_effect,
            Some(Effect::DealDamage { amount: Value::Const(3), target: ObjRef::Target(0) })
        );
        assert_eq!(def.spell_targets.len(), 1);
        assert_eq!(def.spell_targets[0].description, "any target");
        assert!(matches!(def.spell_targets[0].kind, TargetKind::ObjectOrPlayer(_, PlayerRef::Each)));
    }

    #[test]
    fn dano_a_jogador_usa_efeito_de_jogador() {
        let def = playable(&OracleCard::new(
            "Lava Spike",
            "{R}",
            "Sorcery",
            "Lava Spike deals 3 damage to target player.",
        ));
        assert_eq!(
            def.spell_effect,
            Some(Effect::DealDamageToPlayer {
                amount: Value::Const(3),
                player: PlayerRef::Target(0)
            })
        );
        assert!(matches!(def.spell_targets[0].kind, TargetKind::Player(_)));
    }

    #[test]
    fn forest_ganha_habilidade_de_mana_verde() {
        let def = playable(&OracleCard::new(
            "Forest",
            "",
            "Basic Land — Forest",
            "({T}: Add {G}.)",
        ));
        assert_eq!(
            def.abilities,
            vec![Ability::Mana(ManaAbility {
                cost: Cost::Tap,
                production: ManaProduction::Fixed(vec![ManaSymbol::Colored(Color::Green)]),
                restriction: Condition::Always,
                text: "{T}: Add {G}.".to_string(),
            })]
        );
    }

    #[test]
    fn terreno_com_dois_subtipos_produz_duas_habilidades() {
        let def = playable(&OracleCard::new(
            "Tropical Island",
            "",
            "Land — Forest Island",
            "({T}: Add {G} or {U}.)",
        ));
        assert_eq!(def.abilities.len(), 2);
    }

    #[test]
    fn terreno_com_texto_escrito_de_mana() {
        let def = playable(&OracleCard::new(
            "Sol Land",
            "",
            "Land",
            "{T}: Add {W} or {U}.",
        ));
        assert_eq!(
            def.abilities,
            vec![Ability::Mana(ManaAbility {
                cost: Cost::Tap,
                production: ManaProduction::OneOf(vec![
                    ManaSymbol::Colored(Color::White),
                    ManaSymbol::Colored(Color::Blue),
                ]),
                restriction: Condition::Always,
                text: "{T}: Add {W} or {U}.".to_string(),
            })]
        );
    }

    #[test]
    fn giant_growth_vira_pump_3_3_ate_o_fim_do_turno() {
        let def = playable(&OracleCard::new(
            "Giant Growth",
            "{G}",
            "Instant",
            "Target creature gets +3/+3 until end of turn.",
        ));
        assert_eq!(
            def.spell_effect,
            Some(Effect::ModifyPT {
                target: ObjRef::Target(0),
                power: Value::Const(3),
                toughness: Value::Const(3),
                duration: Duration::EndOfTurn,
            })
        );
        assert_eq!(def.spell_targets.len(), 1);
    }

    #[test]
    fn pump_com_palavra_chave_concedida_vira_sequencia() {
        let def = playable(&OracleCard::new(
            "Uplift",
            "{1}{W}",
            "Instant",
            "Target creature gets +2/+2 and gains flying until end of turn.",
        ));
        let esperado = Effect::Sequence(vec![
            Effect::ModifyPT {
                target: ObjRef::Target(0),
                power: Value::Const(2),
                toughness: Value::Const(2),
                duration: Duration::EndOfTurn,
            },
            Effect::GrantKeywords {
                target: ObjRef::Target(0),
                keywords: vec![Keyword::Flying],
                duration: Duration::EndOfTurn,
            },
        ]);
        assert_eq!(def.spell_effect, Some(esperado));
    }

    #[test]
    fn wall_of_omens_compila_etb_que_compra_carta() {
        let def = playable(
            &OracleCard::new(
                "Wall of Omens",
                "{1}{W}",
                "Creature — Wall",
                "Defender\nWhen Wall of Omens enters, draw a card.",
            )
            .with_pt("0", "4"),
        );
        assert_eq!(def.abilities.len(), 2);
        assert_eq!(def.abilities[0], Ability::Keyword(Keyword::Defender));
        let Ability::Triggered(trigger) = &def.abilities[1] else {
            panic!("segunda habilidade deveria ser disparada: {:?}", def.abilities[1]);
        };
        assert_eq!(
            trigger.trigger,
            TriggerCondition::EntersBattlefield(Selector::battlefield(Filter::IsSelf))
        );
        assert_eq!(
            trigger.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
        assert!(trigger.targets.is_empty());
    }

    #[test]
    fn etb_na_redacao_antiga_tambem_compila() {
        let def = playable(
            &OracleCard::new(
                "Angel of Mercy",
                "{4}{W}",
                "Creature — Angel",
                "Flying\nWhen Angel of Mercy enters the battlefield, you gain 3 life.",
            )
            .with_pt("3", "3"),
        );
        let Ability::Triggered(trigger) = &def.abilities[1] else {
            panic!("esperava habilidade disparada");
        };
        assert_eq!(
            trigger.effect,
            Effect::GainLife { amount: Value::Const(3), player: PlayerRef::You }
        );
    }

    #[test]
    fn destroy_e_counter_compilam_com_o_filtro_certo() {
        let doom = playable(&OracleCard::new(
            "Murder",
            "{1}{B}{B}",
            "Instant",
            "Destroy target creature.",
        ));
        assert_eq!(
            doom.spell_effect,
            Some(Effect::Destroy { target: ObjRef::Target(0), no_regeneration: false })
        );

        let naturalize = playable(&OracleCard::new(
            "Naturalize",
            "{1}{G}",
            "Instant",
            "Destroy target artifact or enchantment.",
        ));
        assert_eq!(naturalize.spell_targets[0].description, "target artifact or enchantment");

        let counterspell = playable(&OracleCard::new(
            "Counterspell",
            "{U}{U}",
            "Instant",
            "Counter target spell.",
        ));
        assert_eq!(
            counterspell.spell_effect,
            Some(Effect::CounterSpell { target: ObjRef::Target(0), unless_pays: None })
        );
        assert!(matches!(
            counterspell.spell_targets[0].kind,
            TargetKind::SpellOnStack(Filter::Any)
        ));
    }

    #[test]
    fn compra_multipla_por_extenso() {
        let def = playable(&OracleCard::new("Divination", "{2}{U}", "Sorcery", "Draw two cards."));
        assert_eq!(
            def.spell_effect,
            Some(Effect::DrawCards { count: Value::Const(2), player: PlayerRef::You })
        );
    }

    // -----------------------------------------------------------------------
    // O que NÃO pode compilar
    // -----------------------------------------------------------------------

    #[test]
    fn carta_complexa_vira_unsupported_com_padrao_normalizado() {
        let (_, pattern) = rejected(
            &OracleCard::new(
                "Ajani's Pridemate",
                "{1}{W}",
                "Creature — Cat Soldier",
                "Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate.",
            )
            .with_pt("2", "2"),
        );
        assert_eq!(pattern, "whenever you gain life, put a +N/+N counter on ~.");
    }

    #[test]
    fn padrao_agrupa_cartas_da_mesma_familia() {
        let a = rejected(&OracleCard::new(
            "Fireball",
            "{R}",
            "Sorcery",
            "Fireball deals 4 damage divided as you choose among any number of targets.",
        ));
        let b = rejected(&OracleCard::new(
            "Rockslide",
            "{R}",
            "Sorcery",
            "Rockslide deals 7 damage divided as you choose among any number of targets.",
        ));
        assert_eq!(a.1, b.1);
    }

    #[test]
    fn texto_alem_do_padrao_nao_vira_ir_parcial() {
        // "unless" muda quem morre; compilar só o "destroy" seria carta errada.
        let (reason, _) = rejected(&OracleCard::new(
            "Grasp",
            "{1}{B}",
            "Instant",
            "Destroy target creature unless its controller pays 3 life.",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");

        rejected(&OracleCard::new(
            "Careful Study",
            "{U}",
            "Sorcery",
            "Draw two cards, then discard two cards.",
        ));
        rejected(&OracleCard::new(
            "Char",
            "{1}{R}{R}",
            "Instant",
            "Char deals 4 damage to any target and 2 damage to you.",
        ));
    }

    #[test]
    fn carta_de_duas_faces_e_recusada_pelo_layout() {
        let mut card = OracleCard::new("Delver of Secrets", "{U}", "Creature — Human Wizard", "")
            .with_pt("1", "1");
        card.layout = "transform".to_string();
        let (reason, _) = rejected(&card);
        assert!(reason.contains("transform"), "reason: {reason}");
    }

    #[test]
    fn criatura_com_pt_variavel_e_recusada() {
        let (reason, _) = rejected(
            &OracleCard::new("Tarmogoyf", "{1}{G}", "Creature — Lhurgoyf", "")
                .with_pt("*", "1+*"),
        );
        assert!(reason.contains("P/T"), "reason: {reason}");
    }

    #[test]
    fn custo_com_x_e_recusado() {
        let (reason, _) = rejected(&OracleCard::new(
            "Hydra",
            "{X}{G}",
            "Creature — Hydra",
            "",
        ));
        assert!(reason.contains("{X}"), "reason: {reason}");
    }

    #[test]
    fn planeswalker_e_recusado() {
        let mut card = OracleCard::new(
            "Chandra",
            "{2}{R}{R}",
            "Legendary Planeswalker — Chandra",
            "+1: Chandra deals 2 damage to any target.",
        );
        card.loyalty = Some("4".to_string());
        let (reason, _) = rejected(&card);
        assert!(reason.contains("planeswalker"), "reason: {reason}");
    }

    #[test]
    fn terreno_com_texto_extra_na_mesma_linha_e_recusado() {
        // "{T}: Add {C}{C}. ~ deals 1 damage to you." — compilar só a primeira
        // metade daria um terreno gratuito que não existe.
        let (reason, _) = rejected(&OracleCard::new(
            "Ancient Tomb",
            "",
            "Land",
            "{T}: Add {C}{C}. Ancient Tomb deals 1 damage to you.",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn feitico_sem_efeito_reconhecido_nao_fica_jogavel_vazio() {
        let (reason, _) = rejected(&OracleCard::new(
            "Weird Spell",
            "{U}",
            "Instant",
            "Untap all creatures you control.",
        ));
        assert!(!reason.is_empty());
    }

    #[test]
    fn nome_proprio_vira_til_no_padrao() {
        let lines = normalize_lines(
            "Jace, the Mind Sculptor draws a card. Jace loses 1 life.",
            "Jace, the Mind Sculptor",
        );
        assert_eq!(lines[0].norm, "~ draws a card. ~ loses 1 life.");
        assert_eq!(pattern_of(&lines[0].norm), "~ draws a card. ~ loses N life.");
    }

    #[test]
    fn redacao_nova_com_this_creature_e_autorreferencia() {
        // Desde 2024 o oracle troca o nome próprio por "this creature". Sem
        // reconhecer isso, toda reimpressão recente viraria Unsupported.
        let def = playable(
            &OracleCard::new(
                "Centaur Healer",
                "{1}{W}{G}",
                "Creature — Centaur Cleric",
                "When this creature enters, you gain 3 life.",
            )
            .with_pt("3", "3"),
        );
        let Ability::Triggered(trigger) = &def.abilities[0] else {
            panic!("esperava habilidade disparada: {:?}", def.abilities);
        };
        assert_eq!(
            trigger.effect,
            Effect::GainLife { amount: Value::Const(3), player: PlayerRef::You }
        );
    }

    #[test]
    fn terreno_que_entra_virado_ganha_o_efeito_de_substituicao() {
        let def = playable(&OracleCard::new(
            "Tranquil Cove",
            "",
            "Land",
            "This land enters tapped.\n{T}: Add {W} or {U}.",
        ));
        assert_eq!(def.abilities.len(), 2);
        assert!(matches!(
            &def.abilities[0],
            Ability::Replacement(r) if r.event == ReplacementEvent::EntersTapped
        ));
        assert!(matches!(&def.abilities[1], Ability::Mana(_)));
    }

    #[test]
    fn nome_so_e_trocado_como_palavra_inteira() {
        // Numa carta chamada "Fly", "Flying" não pode virar "~ing".
        let def = playable(
            &OracleCard::new("Fly", "{1}{U}", "Creature — Bird", "Flying").with_pt("1", "1"),
        );
        assert_eq!(def.abilities, vec![Ability::Keyword(Keyword::Flying)]);
    }
}
