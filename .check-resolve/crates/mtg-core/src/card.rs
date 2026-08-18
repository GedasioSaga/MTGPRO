//! Definição estática de carta e suas habilidades.
//!
//! `CardDef` é imutável e compartilhado: é o que está impresso. O estado que
//! muda durante a partida vive em `state::ObjectState`.
use crate::ids::CardDefId;
use crate::ir::*;
use crate::mana::{ColorSet, ManaCost, ManaSymbol};
use crate::types::{CounterKind, Rarity, TypeLine};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Gatilhos
// ---------------------------------------------------------------------------

/// Condição que faz uma habilidade disparada disparar. Casada contra `GameEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerCondition {
    EntersBattlefield(Selector),
    LeavesBattlefield(Selector),
    Dies(Selector),
    Sacrificed(Selector),
    Exiled(Selector),
    SpellCast(Selector),
    AbilityActivated(Selector),
    Attacks(Selector),
    AttacksAlone(Selector),
    Blocks(Selector),
    BecomesBlocked(Selector),
    DealsCombatDamage(Selector),
    DealsCombatDamageToPlayer(Selector),
    DealtDamage(Selector),
    Taps(Selector),
    Untaps(Selector),
    CounterAdded(Selector, CounterKind),

    BeginningOfUpkeep(PlayerRef),
    BeginningOfDrawStep(PlayerRef),
    BeginningOfPrecombatMain(PlayerRef),
    BeginningOfCombat(PlayerRef),
    EndOfCombat(PlayerRef),
    BeginningOfEndStep(PlayerRef),
    /// Fim da limpeza — raro, mas existe.
    Cleanup(PlayerRef),

    LifeGained(PlayerRef),
    LifeLost(PlayerRef),
    CardDrawn(PlayerRef),
    CardDiscarded(PlayerRef),
    LandPlayed(PlayerRef),
    /// Disparo de "quando isto entra ou ataca", agregando várias condições.
    Any(Vec<TriggerCondition>),
}

// ---------------------------------------------------------------------------
// Habilidades
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggeredAbility {
    pub trigger: TriggerCondition,
    /// Condição de intervenção ("se você controlar um Elfo").
    #[serde(default = "Condition::always")]
    pub intervening_if: Condition,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    pub effect: Effect,
    /// "Você pode..." — o controlador escolhe se resolve.
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub once_per_turn: bool,
    /// Dispara mesmo estando fora do campo de batalha (ex.: gatilho de morte).
    #[serde(default)]
    pub triggers_from_graveyard: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatedAbility {
    pub cost: Cost,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    pub effect: Effect,
    pub timing: TimingRestriction,
    #[serde(default = "Condition::always")]
    pub restriction: Condition,
    #[serde(default)]
    pub uses_per_turn: Option<u8>,
    /// Habilidade de lealdade de planeswalker: custo em marcadores.
    #[serde(default)]
    pub loyalty_change: Option<i32>,
    pub text: String,
}

/// Habilidade de mana: resolve imediatamente, não usa a pilha (CR 605.3a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaAbility {
    pub cost: Cost,
    pub production: ManaProduction,
    #[serde(default = "Condition::always")]
    pub restriction: Condition,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManaProduction {
    Fixed(Vec<ManaSymbol>),
    /// N de mana de qualquer cor, escolhida na hora.
    AnyColor(u8),
    /// Uma entre as cores listadas.
    OneOf(Vec<ManaSymbol>),
    /// Quantidade dependente do estado do jogo.
    Dynamic { symbol: ManaSymbol, amount: Value },
}

/// Efeito contínuo enquanto a fonte está no campo (anthem, lord, restrição).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticAbility {
    #[serde(default = "Condition::always")]
    pub condition: Condition,
    pub affects: Selector,
    pub modification: StaticMod,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaticMod {
    /// Camada 7c — modificação de P/T.
    ModifyPT(Value, Value),
    /// Camada 7b — define P/T.
    SetPT(Value, Value),
    /// Camada 6 — concede habilidade.
    GrantKeywords(Vec<Keyword>),
    LoseKeywords(Vec<Keyword>),
    /// Camada 5 — muda cor.
    SetColors(Vec<crate::mana::Color>),
    /// Camada 4 — muda tipo.
    AddTypes(TypeLine),
    CantAttack,
    CantBlock,
    CantBeBlockedExceptBy(Filter),
    /// Reduz custo genérico de lançamento.
    CostReduction(Value, Filter),
    CostIncrease(Value, Filter),
    /// Substitui dano: previne todo dano ao alvo.
    PreventAllDamage,
}

/// Efeito de substituição (CR 614). Aplicado antes do evento acontecer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementAbility {
    pub event: ReplacementEvent,
    pub replacement: Effect,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplacementEvent {
    /// Entra no campo de batalha com marcadores.
    EntersWithCounters(CounterKind, Value),
    /// Entra virado.
    EntersTapped,
    /// Dano que seria causado ao alvo é prevenido/redirecionado.
    DamageWouldBeDealt(Selector),
    /// Em vez de ir pro cemitério, vai para o exílio.
    WouldDieReplace(Selector),
    /// Compra extra por evento de compra.
    IfWouldDraw(PlayerRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ability {
    Keyword(Keyword),
    Static(StaticAbility),
    Activated(ActivatedAbility),
    Triggered(TriggeredAbility),
    Mana(ManaAbility),
    Replacement(ReplacementAbility),
}

impl Ability {
    pub fn text(&self) -> String {
        match self {
            Ability::Keyword(k) => k.label(),
            Ability::Static(a) => a.text.clone(),
            Ability::Activated(a) => a.text.clone(),
            Ability::Triggered(a) => a.text.clone(),
            Ability::Mana(a) => a.text.clone(),
            Ability::Replacement(a) => a.text.clone(),
        }
    }
    pub fn as_keyword(&self) -> Option<&Keyword> {
        match self {
            Ability::Keyword(k) => Some(k),
            _ => None,
        }
    }
}

impl Condition {
    /// Default do serde para campos de condição omitidos.
    pub fn always() -> Condition {
        Condition::Always
    }
}

// ---------------------------------------------------------------------------
// Carta
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDef {
    pub id: CardDefId,
    pub name: String,
    pub mana_cost: ManaCost,
    pub type_line: TypeLine,
    /// Sobrescreve a cor derivada do custo (ex.: indicador de cor).
    #[serde(default)]
    pub color_override: Option<ColorSet>,
    #[serde(default)]
    pub power: Option<i32>,
    #[serde(default)]
    pub toughness: Option<i32>,
    #[serde(default)]
    pub loyalty: Option<i32>,
    #[serde(default)]
    pub abilities: Vec<Ability>,
    /// Efeito de instantâneo/feitiço ao resolver.
    #[serde(default)]
    pub spell_effect: Option<Effect>,
    #[serde(default)]
    pub spell_targets: Vec<TargetSpec>,
    pub oracle_text: String,
    #[serde(default)]
    pub flavor_text: Option<String>,
    pub rarity: Rarity,
    pub set_code: String,
    #[serde(default)]
    pub collector_number: String,
    #[serde(default)]
    pub artist: Option<String>,
    /// Chave da arte no object storage / CDN.
    #[serde(default)]
    pub art_key: Option<String>,
}

impl CardDef {
    pub fn colors(&self) -> ColorSet {
        self.color_override.unwrap_or_else(|| self.mana_cost.colors())
    }
    pub fn mana_value(&self) -> u32 {
        self.mana_cost.mana_value()
    }
    pub fn is_permanent(&self) -> bool {
        self.type_line.is_permanent()
    }
    pub fn keywords(&self) -> impl Iterator<Item = &Keyword> {
        self.abilities.iter().filter_map(|a| a.as_keyword())
    }
    pub fn has_keyword(&self, k: &Keyword) -> bool {
        self.keywords().any(|x| x == k)
    }
    pub fn triggered(&self) -> impl Iterator<Item = (usize, &TriggeredAbility)> {
        self.abilities.iter().enumerate().filter_map(|(i, a)| match a {
            Ability::Triggered(t) => Some((i, t)),
            _ => None,
        })
    }
    pub fn activated(&self) -> impl Iterator<Item = (usize, &ActivatedAbility)> {
        self.abilities.iter().enumerate().filter_map(|(i, a)| match a {
            Ability::Activated(t) => Some((i, t)),
            _ => None,
        })
    }
    pub fn mana_abilities(&self) -> impl Iterator<Item = (usize, &ManaAbility)> {
        self.abilities.iter().enumerate().filter_map(|(i, a)| match a {
            Ability::Mana(t) => Some((i, t)),
            _ => None,
        })
    }
    pub fn statics(&self) -> impl Iterator<Item = (usize, &StaticAbility)> {
        self.abilities.iter().enumerate().filter_map(|(i, a)| match a {
            Ability::Static(t) => Some((i, t)),
            _ => None,
        })
    }
    pub fn replacements(&self) -> impl Iterator<Item = (usize, &ReplacementAbility)> {
        self.abilities.iter().enumerate().filter_map(|(i, a)| match a {
            Ability::Replacement(t) => Some((i, t)),
            _ => None,
        })
    }
}

/// Catálogo carregado do banco. Indexado por `CardDefId`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CardDatabase {
    pub cards: Vec<CardDef>,
}

impl CardDatabase {
    pub fn get(&self, id: CardDefId) -> Option<&CardDef> {
        self.cards.get(id.0 as usize)
    }
    pub fn by_name(&self, name: &str) -> Option<&CardDef> {
        self.cards.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }
    pub fn id_by_name(&self, name: &str) -> Option<CardDefId> {
        self.by_name(name).map(|c| c.id)
    }
    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
    /// Reindexa os ids para casar com a posição no vetor.
    pub fn reindex(&mut self) {
        for (i, c) in self.cards.iter_mut().enumerate() {
            c.id = CardDefId(i as u32);
        }
    }
}
