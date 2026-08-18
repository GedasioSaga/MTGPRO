//! Açúcar sintático para escrever cartas.
//!
//! Uma `CardDef` escrita à mão tem ~25 campos e três níveis de enum aninhado.
//! Multiplicado por 150 cartas, isso vira ruído que esconde erro. Aqui ficam os
//! atalhos: `mana("2RR")`, `tl("Creature — Human Soldier")`, `creature(...)`.
//! A regra do módulo é que quem lê a entrada de uma carta entende a carta.

use mtg_core::card::{
    Ability, ActivatedAbility, CardDef, ManaAbility, ManaProduction, ReplacementAbility,
    ReplacementEvent, StaticAbility, StaticMod, TriggerCondition, TriggeredAbility,
};
use mtg_core::ids::CardDefId;
use mtg_core::ir::{
    Condition, Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind,
    TargetSpec, TimingRestriction, TokenSpec, Value, ZoneScope,
};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Rarity, Supertype, TypeLine};

// ---------------------------------------------------------------------------
// Parsers de notação curta
// ---------------------------------------------------------------------------

/// `mana("2RR")` → `{2}{R}{R}`. Dígitos consecutivos formam um único símbolo
/// genérico, `X` é o símbolo variável e `C` é o incolor de Eldrazi.
pub fn mana(spec: &str) -> ManaCost {
    let mut symbols = Vec::new();
    let mut generic: Option<u32> = None;
    for ch in spec.chars() {
        if let Some(d) = ch.to_digit(10) {
            generic = Some(generic.unwrap_or(0) * 10 + d);
            continue;
        }
        if let Some(n) = generic.take() {
            symbols.push(ManaSymbol::Generic(n.min(u8::MAX as u32) as u8));
        }
        match ch {
            'X' | 'x' => symbols.push(ManaSymbol::X),
            'C' | 'c' => symbols.push(ManaSymbol::Colorless),
            ' ' | '{' | '}' => {}
            other => {
                if let Some(color) = Color::from_letter(other) {
                    symbols.push(ManaSymbol::Colored(color));
                }
            }
        }
    }
    if let Some(n) = generic {
        symbols.push(ManaSymbol::Generic(n.min(u8::MAX as u32) as u8));
    }
    ManaCost { symbols }
}

/// `tl("Legendary Creature — Human Soldier")`. Aceita travessão (—) ou hífen.
pub fn tl(spec: &str) -> TypeLine {
    let (head, tail) = match spec.split_once('—').or_else(|| spec.split_once(" - ")) {
        Some((h, t)) => (h, Some(t)),
        None => (spec, None),
    };
    let mut line = TypeLine::default();
    for word in head.split_whitespace() {
        match word {
            "Basic" => line.supertypes.push(Supertype::Basic),
            "Legendary" => line.supertypes.push(Supertype::Legendary),
            "Snow" => line.supertypes.push(Supertype::Snow),
            "World" => line.supertypes.push(Supertype::World),
            "Artifact" => line.types.push(CardType::Artifact),
            "Battle" => line.types.push(CardType::Battle),
            "Creature" => line.types.push(CardType::Creature),
            "Enchantment" => line.types.push(CardType::Enchantment),
            "Instant" => line.types.push(CardType::Instant),
            "Land" => line.types.push(CardType::Land),
            "Planeswalker" => line.types.push(CardType::Planeswalker),
            "Sorcery" => line.types.push(CardType::Sorcery),
            "Kindred" | "Tribal" => line.types.push(CardType::Kindred),
            _ => {}
        }
    }
    if let Some(subs) = tail {
        line.subtypes = subs.split_whitespace().map(|s| s.to_string()).collect();
    }
    line
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Constrói uma `CardDef` por encadeamento. `id` sai como 0 — quem monta o
/// catálogo chama `CardDatabase::reindex` e os ids passam a ser a posição.
pub struct CardBuilder {
    def: CardDef,
}

impl CardBuilder {
    pub fn new(name: &str, cost: &str, type_line: &str) -> Self {
        CardBuilder {
            def: CardDef {
                id: CardDefId(0),
                name: name.to_string(),
                mana_cost: mana(cost),
                type_line: tl(type_line),
                color_override: None,
                power: None,
                toughness: None,
                loyalty: None,
                abilities: Vec::new(),
                spell_effect: None,
                spell_targets: Vec::new(),
                oracle_text: String::new(),
                flavor_text: None,
                rarity: Rarity::Common,
                set_code: String::new(),
                collector_number: String::new(),
                artist: None,
                // A UI monta a URL da arte no Scryfall a partir do nome exato.
                art_key: Some(name.to_string()),
            },
        }
    }

    pub fn pt(mut self, power: i32, toughness: i32) -> Self {
        self.def.power = Some(power);
        self.def.toughness = Some(toughness);
        self
    }

    /// Palavra-chave impressa (voadora, atropelar, ...).
    pub fn kw(mut self, keyword: Keyword) -> Self {
        self.def.abilities.push(Ability::Keyword(keyword));
        self
    }

    pub fn kws(mut self, keywords: impl IntoIterator<Item = Keyword>) -> Self {
        self.def
            .abilities
            .extend(keywords.into_iter().map(Ability::Keyword));
        self
    }

    pub fn ability(mut self, ability: Ability) -> Self {
        self.def.abilities.push(ability);
        self
    }

    /// Efeito de instantâneo/feitiço ao resolver.
    pub fn spell(mut self, effect: Effect) -> Self {
        self.def.spell_effect = Some(effect);
        self
    }

    pub fn target(mut self, spec: TargetSpec) -> Self {
        self.def.spell_targets.push(spec);
        self
    }

    pub fn oracle(mut self, text: &str) -> Self {
        self.def.oracle_text = text.to_string();
        self
    }

    pub fn flavor(mut self, text: &str) -> Self {
        self.def.flavor_text = Some(text.to_string());
        self
    }

    /// Metadados de impressão. Coleção e número alimentam só a UI; `art_key`
    /// continua sendo o nome da carta.
    pub fn meta(mut self, rarity: Rarity, set_code: &str, number: &str, artist: &str) -> Self {
        self.def.rarity = rarity;
        self.def.set_code = set_code.to_string();
        self.def.collector_number = number.to_string();
        self.def.artist = Some(artist.to_string());
        self
    }

    pub fn build(self) -> CardDef {
        self.def
    }
}

// ---------------------------------------------------------------------------
// Atalhos por tipo de carta
// ---------------------------------------------------------------------------

pub fn creature(name: &str, cost: &str, type_line: &str, power: i32, toughness: i32) -> CardBuilder {
    CardBuilder::new(name, cost, type_line).pt(power, toughness)
}

pub fn instant(name: &str, cost: &str) -> CardBuilder {
    CardBuilder::new(name, cost, "Instant")
}

pub fn sorcery(name: &str, cost: &str) -> CardBuilder {
    CardBuilder::new(name, cost, "Sorcery")
}

pub fn enchantment(name: &str, cost: &str) -> CardBuilder {
    CardBuilder::new(name, cost, "Enchantment")
}

/// Aura: o alvo do encantamento é declarado como alvo da mágica e o motor
/// anexa a permanente ao resolver (CR 303.4).
pub fn aura(name: &str, cost: &str, target: TargetSpec) -> CardBuilder {
    CardBuilder::new(name, cost, "Enchantment — Aura").target(target)
}

pub fn artifact(name: &str, cost: &str) -> CardBuilder {
    CardBuilder::new(name, cost, "Artifact")
}

pub fn basic_land(name: &str, subtype: &str, color: Color, oracle: &str) -> CardBuilder {
    CardBuilder::new(name, "", &format!("Basic Land — {subtype}"))
        .ability(mana_ability(&[ManaSymbol::Colored(color)], oracle))
        .oracle(oracle)
}

// ---------------------------------------------------------------------------
// Seletores prontos
// ---------------------------------------------------------------------------

pub fn sel_self() -> Selector {
    Selector::battlefield(Filter::IsSelf)
}

pub fn sel_creatures() -> Selector {
    Selector::creatures()
}

pub fn sel_your_creatures() -> Selector {
    Selector::creatures().yours()
}

pub fn sel_opponent_creatures() -> Selector {
    Selector::creatures().opponents()
}

pub fn sel_other_creatures_you_control() -> Selector {
    Selector::battlefield(Filter::all([Filter::creature(), Filter::IsOther])).yours()
}

pub fn sel_subtype(subtype: &str) -> Selector {
    Selector::battlefield(Filter::all([
        Filter::creature(),
        Filter::HasSubtype(subtype.to_string()),
    ]))
}

/// Mágica na pilha que casa com o filtro — para contramágicas.
pub fn sel_stack(filter: Filter) -> Selector {
    Selector {
        zone: ZoneScope::Stack,
        filter,
        owner_scope: None,
        max: None,
    }
}

pub fn f_type(t: CardType) -> Filter {
    Filter::HasType(t)
}

pub fn f_noncreature_nonland() -> Filter {
    Filter::Not(Box::new(Filter::Or(vec![
        Filter::HasType(CardType::Creature),
        Filter::HasType(CardType::Land),
    ])))
}

pub fn f_artifact_or_enchantment() -> Filter {
    Filter::Or(vec![
        Filter::HasType(CardType::Artifact),
        Filter::HasType(CardType::Enchantment),
    ])
}

// ---------------------------------------------------------------------------
// Alvos
// ---------------------------------------------------------------------------

pub fn spec(kind: TargetKind, description: &str) -> TargetSpec {
    TargetSpec {
        kind,
        description: description.to_string(),
    }
}

/// "any target" — criatura, planeswalker ou jogador (CR 115.4). O motor decide
/// se a escolha virou `TargetChoice::Object` ou `TargetChoice::Player`; o efeito
/// de dano referencia o índice do alvo nos dois casos.
pub fn t_any() -> TargetSpec {
    spec(
        TargetKind::ObjectOrPlayer(Selector::creatures(), PlayerRef::Each),
        "qualquer alvo",
    )
}

pub fn t_creature() -> TargetSpec {
    spec(TargetKind::Object(Selector::creatures()), "alvo de criatura")
}

pub fn t_creature_filtered(filter: Filter, description: &str) -> TargetSpec {
    spec(
        TargetKind::Object(Selector::battlefield(Filter::all([
            Filter::creature(),
            filter,
        ]))),
        description,
    )
}

pub fn t_creature_opponent() -> TargetSpec {
    spec(
        TargetKind::Object(Selector::creatures().opponents()),
        "alvo de criatura que um oponente controla",
    )
}

pub fn t_creature_yours() -> TargetSpec {
    spec(
        TargetKind::Object(Selector::creatures().yours()),
        "alvo de criatura que você controla",
    )
}

pub fn t_permanent() -> TargetSpec {
    spec(
        TargetKind::Object(Selector::battlefield(Filter::Any)),
        "alvo de permanente",
    )
}

pub fn t_permanent_yours() -> TargetSpec {
    spec(
        TargetKind::Object(Selector::battlefield(Filter::Any).yours()),
        "alvo de permanente que você controla",
    )
}

pub fn t_object(filter: Filter, description: &str) -> TargetSpec {
    spec(
        TargetKind::Object(Selector::battlefield(filter)),
        description,
    )
}

pub fn t_player() -> TargetSpec {
    spec(TargetKind::Player(PlayerRef::Each), "alvo de jogador")
}

pub fn t_opponent() -> TargetSpec {
    spec(TargetKind::Player(PlayerRef::Opponents), "alvo de oponente")
}

pub fn t_spell(filter: Filter, description: &str) -> TargetSpec {
    spec(TargetKind::SpellOnStack(filter), description)
}

// ---------------------------------------------------------------------------
// Efeitos frequentes
// ---------------------------------------------------------------------------

pub fn target0() -> ObjRef {
    ObjRef::Target(0)
}

pub fn dmg(amount: i32, target: ObjRef) -> Effect {
    Effect::DealDamage {
        amount: Value::c(amount),
        target,
    }
}

/// Dano a "any target": o alvo 0 pode ser objeto ou jogador.
pub fn dmg_any(amount: i32) -> Effect {
    dmg(amount, ObjRef::Target(0))
}

pub fn dmg_player(amount: i32, player: PlayerRef) -> Effect {
    Effect::DealDamageToPlayer {
        amount: Value::c(amount),
        player,
    }
}

pub fn draw(count: i32, player: PlayerRef) -> Effect {
    Effect::DrawCards {
        count: Value::c(count),
        player,
    }
}

pub fn gain_life(amount: i32, player: PlayerRef) -> Effect {
    Effect::GainLife {
        amount: Value::c(amount),
        player,
    }
}

pub fn lose_life(amount: i32, player: PlayerRef) -> Effect {
    Effect::LoseLife {
        amount: Value::c(amount),
        player,
    }
}

pub fn destroy(target: ObjRef) -> Effect {
    Effect::Destroy {
        target,
        no_regeneration: false,
    }
}

pub fn destroy_hard(target: ObjRef) -> Effect {
    Effect::Destroy {
        target,
        no_regeneration: true,
    }
}

pub fn bounce(target: ObjRef) -> Effect {
    Effect::ReturnToHand { target }
}

pub fn pump(target: ObjRef, power: i32, toughness: i32, duration: Duration) -> Effect {
    Effect::ModifyPT {
        target,
        power: Value::c(power),
        toughness: Value::c(toughness),
        duration,
    }
}

pub fn grant(target: ObjRef, keywords: Vec<Keyword>, duration: Duration) -> Effect {
    Effect::GrantKeywords {
        target,
        keywords,
        duration,
    }
}

pub fn counters(target: ObjRef, kind: CounterKind, count: i32) -> Effect {
    Effect::AddCounters {
        target,
        kind,
        count: Value::c(count),
    }
}

/// Aplica um efeito a cada objeto do seletor — a forma de escrever varredura
/// ("destrua todas as criaturas") sem alvo.
pub fn for_each(over: Selector, do_: Effect) -> Effect {
    Effect::ForEach {
        over,
        do_: Box::new(do_),
    }
}

pub fn token(name: &str, type_line: &str, colors: &[Color], power: i32, toughness: i32) -> TokenSpec {
    TokenSpec {
        name: name.to_string(),
        type_line: tl(type_line),
        colors: colors.to_vec(),
        power,
        toughness,
        keywords: Vec::new(),
        art_key: Some(name.to_string()),
    }
}

pub fn token_kw(
    name: &str,
    type_line: &str,
    colors: &[Color],
    power: i32,
    toughness: i32,
    keywords: Vec<Keyword>,
) -> TokenSpec {
    let mut spec = token(name, type_line, colors, power, toughness);
    spec.keywords = keywords;
    spec
}

pub fn create_tokens(spec: TokenSpec, count: i32) -> Effect {
    Effect::CreateToken {
        spec,
        count: Value::c(count),
        controller: PlayerRef::You,
    }
}

// ---------------------------------------------------------------------------
// Habilidades
// ---------------------------------------------------------------------------

/// Gatilho de entrada da própria permanente ("When ~ enters, ...").
pub fn etb(text: &str, effect: Effect) -> Ability {
    Ability::Triggered(TriggeredAbility {
        trigger: TriggerCondition::EntersBattlefield(sel_self()),
        intervening_if: Condition::Always,
        targets: Vec::new(),
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: text.to_string(),
    })
}

pub fn etb_targeted(text: &str, targets: Vec<TargetSpec>, effect: Effect) -> Ability {
    Ability::Triggered(TriggeredAbility {
        trigger: TriggerCondition::EntersBattlefield(sel_self()),
        intervening_if: Condition::Always,
        targets,
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: text.to_string(),
    })
}

/// Gatilho genérico sem alvo.
pub fn trigger(condition: TriggerCondition, text: &str, effect: Effect) -> Ability {
    Ability::Triggered(TriggeredAbility {
        trigger: condition,
        intervening_if: Condition::Always,
        targets: Vec::new(),
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: text.to_string(),
    })
}

pub fn trigger_targeted(
    condition: TriggerCondition,
    text: &str,
    targets: Vec<TargetSpec>,
    effect: Effect,
) -> Ability {
    Ability::Triggered(TriggeredAbility {
        trigger: condition,
        intervening_if: Condition::Always,
        targets,
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: text.to_string(),
    })
}

/// Gatilho de morte que dispara já do cemitério (CR 603.6c/603.10).
pub fn dies_trigger(text: &str, effect: Effect) -> Ability {
    Ability::Triggered(TriggeredAbility {
        trigger: TriggerCondition::Dies(sel_self()),
        intervening_if: Condition::Always,
        targets: Vec::new(),
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: true,
        text: text.to_string(),
    })
}

pub fn activated(cost: Cost, effect: Effect, timing: TimingRestriction, text: &str) -> Ability {
    Ability::Activated(ActivatedAbility {
        cost,
        targets: Vec::new(),
        effect,
        timing,
        restriction: Condition::Always,
        uses_per_turn: None,
        loyalty_change: None,
        text: text.to_string(),
    })
}

pub fn activated_targeted(
    cost: Cost,
    targets: Vec<TargetSpec>,
    effect: Effect,
    timing: TimingRestriction,
    text: &str,
) -> Ability {
    Ability::Activated(ActivatedAbility {
        cost,
        targets,
        effect,
        timing,
        restriction: Condition::Always,
        uses_per_turn: None,
        loyalty_change: None,
        text: text.to_string(),
    })
}

pub fn static_ability(affects: Selector, modification: StaticMod, text: &str) -> Ability {
    Ability::Static(StaticAbility {
        condition: Condition::Always,
        affects,
        modification,
        text: text.to_string(),
    })
}

/// Anthem: "criaturas que você controla recebem +x/+y".
pub fn anthem(affects: Selector, power: i32, toughness: i32, text: &str) -> Ability {
    static_ability(
        affects,
        StaticMod::ModifyPT(Value::c(power), Value::c(toughness)),
        text,
    )
}

pub fn mana_ability(symbols: &[ManaSymbol], text: &str) -> Ability {
    Ability::Mana(ManaAbility {
        cost: Cost::Tap,
        production: ManaProduction::Fixed(symbols.to_vec()),
        restriction: Condition::Always,
        text: text.to_string(),
    })
}

pub fn mana_any_color(cost: Cost, text: &str) -> Ability {
    Ability::Mana(ManaAbility {
        cost,
        production: ManaProduction::AnyColor(1),
        restriction: Condition::Always,
        text: text.to_string(),
    })
}

pub fn enters_tapped() -> Ability {
    Ability::Replacement(ReplacementAbility {
        event: ReplacementEvent::EntersTapped,
        replacement: Effect::Nothing,
        text: "enters tapped".to_string(),
    })
}

/// `{T}` puro, e `{T}` somado a um custo de mana — os dois formatos que
/// aparecem em praticamente toda habilidade ativada de permanente.
pub fn tap_cost() -> Cost {
    Cost::Tap
}

pub fn tap_mana_cost(symbols: &str) -> Cost {
    Cost::tap_and(mana(symbols).symbols)
}

pub fn mana_cost(symbols: &str) -> Cost {
    Cost::Mana(mana(symbols).symbols)
}
