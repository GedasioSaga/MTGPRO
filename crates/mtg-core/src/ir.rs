//! IR de efeitos: a linguagem em que as cartas são escritas.
//!
//! Carta não é código Rust — é uma árvore de dados serializável, guardada no
//! banco e interpretada pelo motor. Adicionar carta nova é inserir linha, não
//! recompilar. O preço é que o interpretador precisa cobrir o vocabulário.
use crate::ids::PlayerId;
use crate::mana::{Color, ManaSymbol};
use crate::types::{CardType, CounterKind, Supertype, TypeLine};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Referências
// ---------------------------------------------------------------------------

/// A quem um efeito se refere quando fala de jogador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerRef {
    /// Controlador da fonte do efeito.
    You,
    /// Cada oponente do controlador.
    Opponents,
    /// Todos os jogadores, na ordem de turno a partir do jogador ativo.
    Each,
    /// N-ésimo alvo escolhido, que precisa ser jogador.
    Target(u8),
    ControllerOf(Box<ObjRef>),
    OwnerOf(Box<ObjRef>),
    ActivePlayer,
}

/// A que objeto um efeito se refere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjRef {
    /// A própria fonte do efeito.
    SelfObject,
    /// N-ésimo alvo escolhido na hora de colocar na pilha.
    Target(u8),
    /// Item corrente dentro de `ForEach`.
    Selected,
    /// O que esta aura/equipamento está anexado.
    Attached,
    /// Objeto que disparou o gatilho (ex.: a criatura que morreu).
    TriggerObject,
    /// Objeto secundário do evento (ex.: quem causou o dano).
    TriggerSource,
    /// Todos os objetos que casam com o seletor.
    All(Selector),
    /// Objeto lembrado por efeito anterior no mesmo Sequence.
    Remembered(u8),
}

// ---------------------------------------------------------------------------
// Seleção e filtros
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneScope {
    Battlefield,
    Graveyard,
    Hand,
    Library,
    Exile,
    Stack,
    Anywhere,
}

/// Conjunto de objetos descrito declarativamente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selector {
    pub zone: ZoneScope,
    pub filter: Filter,
    /// Limita a objetos controlados/possuídos por estes jogadores.
    #[serde(default)]
    pub owner_scope: Option<PlayerRef>,
    /// `None` = sem limite de quantidade.
    #[serde(default)]
    pub max: Option<u8>,
}

impl Selector {
    pub fn battlefield(filter: Filter) -> Self {
        Selector { zone: ZoneScope::Battlefield, filter, owner_scope: None, max: None }
    }
    pub fn creatures() -> Self {
        Selector::battlefield(Filter::HasType(CardType::Creature))
    }
    pub fn yours(mut self) -> Self {
        self.owner_scope = Some(PlayerRef::You);
        self
    }
    pub fn opponents(mut self) -> Self {
        self.owner_scope = Some(PlayerRef::Opponents);
        self
    }
}

/// Predicado composicional sobre um objeto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    Any,
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),

    HasType(CardType),
    HasSubtype(String),
    HasSupertype(Supertype),
    HasName(String),
    HasColor(Color),
    Colorless,
    Multicolored,
    HasKeyword(Box<Keyword>),
    HasCounter(CounterKind),

    Tapped,
    Untapped,
    Attacking,
    Blocking,
    Blocked,
    Unblocked,
    Token,
    NonToken,
    /// Entrou no campo de batalha neste turno (enjoo de invocação).
    SummoningSick,

    PowerAtLeast(i32),
    PowerAtMost(i32),
    ToughnessAtLeast(i32),
    ToughnessAtMost(i32),
    ManaValueAtLeast(u32),
    ManaValueAtMost(u32),
    ManaValueExactly(u32),

    ControlledBy(PlayerRef),
    OwnedBy(PlayerRef),
    /// É a própria fonte do efeito.
    IsSelf,
    /// É outro objeto que não a fonte ("outra criatura").
    IsOther,
    /// Pode ser alvo legal (checa hexproof/shroud/proteção/ward).
    Targetable,
}

impl Filter {
    pub fn all(fs: impl IntoIterator<Item = Filter>) -> Filter {
        Filter::And(fs.into_iter().collect())
    }
    pub fn creature() -> Filter {
        Filter::HasType(CardType::Creature)
    }
}

// ---------------------------------------------------------------------------
// Valores numéricos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Const(i32),
    /// Valor de X escolhido ao lançar/ativar.
    X,
    /// Quantidade de objetos que casam com o seletor.
    Count(Selector),
    PowerOf(ObjRef),
    ToughnessOf(ObjRef),
    ManaValueOf(ObjRef),
    LifeOf(PlayerRef),
    CardsInHandOf(PlayerRef),
    CountersOn(ObjRef, CounterKind),
    /// Número escolhido por efeito anterior.
    ChosenNumber,
    Add(Box<Value>, Box<Value>),
    Sub(Box<Value>, Box<Value>),
    Mul(Box<Value>, Box<Value>),
    Neg(Box<Value>),
    Max(Box<Value>, Box<Value>),
    Min(Box<Value>, Box<Value>),
}

impl Value {
    pub fn c(n: i32) -> Value {
        Value::Const(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Value {
        Value::Const(n)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    Always,
    Never,
    Compare(Value, Cmp, Value),
    /// Existe ao menos um objeto que casa.
    Exists(Selector),
    /// O objeto referenciado casa com o filtro.
    Matches(ObjRef, Filter),
    IsYourTurn,
    IsMainPhase,
    /// Você controla ao menos N objetos do filtro.
    YouControlAtLeast(u8, Filter),
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),
}

// ---------------------------------------------------------------------------
// Palavras-chave
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Keyword {
    Flying,
    Reach,
    Trample,
    FirstStrike,
    DoubleStrike,
    Deathtouch,
    Lifelink,
    Vigilance,
    Haste,
    Menace,
    Defender,
    Flash,
    Hexproof,
    Shroud,
    Indestructible,
    Prowess,
    Intimidate,
    Fear,
    Skulk,
    Exalted,
    Riot,
    Convoke,
    Delve,
    Cascade,
    Storm,
    Protection(Color),
    Ward(Box<Cost>),
    Flashback(Box<Cost>),
    Kicker(Box<Cost>),
    Cycling(Box<Cost>),
    Equip(Box<Cost>),
    Enchant(Box<Filter>),
    Landwalk(String),
    Annihilator(u8),
    Afflict(u8),
    Split(u8),
}

impl Keyword {
    /// Palavras-chave que só alteram evasão em combate.
    pub fn is_evasion(&self) -> bool {
        matches!(
            self,
            Keyword::Flying
                | Keyword::Menace
                | Keyword::Intimidate
                | Keyword::Fear
                | Keyword::Skulk
                | Keyword::Landwalk(_)
        )
    }
    /// Nome curto para exibição na UI.
    pub fn label(&self) -> String {
        match self {
            Keyword::Protection(c) => format!("Protection from {c:?}"),
            Keyword::Landwalk(s) => format!("{s}walk"),
            Keyword::Annihilator(n) => format!("Annihilator {n}"),
            Keyword::Afflict(n) => format!("Afflict {n}"),
            Keyword::Split(n) => format!("Split {n}"),
            other => format!("{other:?}")
                .split('(')
                .next()
                .unwrap_or("")
                .to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Custos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    Mana(Vec<ManaSymbol>),
    Tap,
    Untap,
    PayLife(Value),
    Discard(u8, Filter),
    Sacrifice(u8, Filter),
    ExileFromGraveyard(u8, Filter),
    RemoveCounters(u8, CounterKind),
    AddCountersToSelf(u8, CounterKind),
    /// Vira criaturas desviradas que você controla.
    TapOther(u8, Filter),
    Composite(Vec<Cost>),
    Free,
}

impl Cost {
    pub fn mana(symbols: impl IntoIterator<Item = ManaSymbol>) -> Cost {
        Cost::Mana(symbols.into_iter().collect())
    }
    pub fn tap_and(mana: Vec<ManaSymbol>) -> Cost {
        Cost::Composite(vec![Cost::Mana(mana), Cost::Tap])
    }
}

// ---------------------------------------------------------------------------
// Efeitos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    /// Efeito único, sem continuidade.
    Instant,
    EndOfTurn,
    /// Enquanto a fonte permanecer no campo de batalha.
    WhileSourcePresent,
    /// Até o começo do seu próximo turno.
    YourNextTurn,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSpec {
    pub name: String,
    pub type_line: TypeLine,
    pub colors: Vec<Color>,
    pub power: i32,
    pub toughness: i32,
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    #[serde(default)]
    pub art_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Nothing,
    Sequence(Vec<Effect>),

    // --- dano e vida ---
    DealDamage { amount: Value, target: ObjRef },
    DealDamageToPlayer { amount: Value, player: PlayerRef },
    /// Divide N de dano entre os alvos escolhidos.
    DivideDamage { total: Value, targets: Vec<u8> },
    GainLife { amount: Value, player: PlayerRef },
    LoseLife { amount: Value, player: PlayerRef },
    SetLife { amount: Value, player: PlayerRef },

    // --- cartas ---
    DrawCards { count: Value, player: PlayerRef },
    Discard { count: Value, player: PlayerRef, filter: Filter, random: bool },
    Mill { count: Value, player: PlayerRef },
    Scry { count: Value, player: PlayerRef },
    Surveil { count: Value, player: PlayerRef },
    SearchLibrary { count: Value, filter: Filter, player: PlayerRef, to_hand: bool },
    ShuffleLibrary { player: PlayerRef },
    PutOnTopOfLibrary { target: ObjRef },
    PutOnBottomOfLibrary { target: ObjRef },
    ReturnToHand { target: ObjRef },
    ReturnFromGraveyardToBattlefield { target: ObjRef },

    // --- permanentes ---
    Destroy { target: ObjRef, no_regeneration: bool },
    Exile { target: ObjRef, until_source_leaves: bool },
    Sacrifice { player: PlayerRef, count: Value, filter: Filter },
    Tap { target: ObjRef },
    Untap { target: ObjRef },
    /// Não desvira no próximo passo de desvirar do controlador.
    Freeze { target: ObjRef },
    GainControl { target: ObjRef, player: PlayerRef, duration: Duration },
    AttachTo { equipment: ObjRef, target: ObjRef },
    Unattach { equipment: ObjRef },
    Transform { target: ObjRef },
    CreateToken { spec: TokenSpec, count: Value, controller: PlayerRef },

    // --- modificações contínuas ---
    ModifyPT { target: ObjRef, power: Value, toughness: Value, duration: Duration },
    SetPT { target: ObjRef, power: Value, toughness: Value, duration: Duration },
    GrantKeywords { target: ObjRef, keywords: Vec<Keyword>, duration: Duration },
    LoseKeywords { target: ObjRef, keywords: Vec<Keyword>, duration: Duration },
    AddCounters { target: ObjRef, kind: CounterKind, count: Value },
    RemoveCounters { target: ObjRef, kind: CounterKind, count: Value },
    CantBeBlocked { target: ObjRef, duration: Duration },
    CantAttackOrBlock { target: ObjRef, duration: Duration },

    // --- pilha ---
    CounterSpell { target: ObjRef, unless_pays: Option<Cost> },
    CopySpell { target: ObjRef, count: Value, may_choose_new_targets: bool },

    // --- mana ---
    AddMana { symbols: Vec<ManaSymbol>, player: PlayerRef },
    AddManaAnyColor { count: Value, player: PlayerRef },

    // --- combate ---
    Fight { a: ObjRef, b: ObjRef },
    PutOntoBattlefieldAttacking { spec: TokenSpec, controller: PlayerRef },
    ExtraCombatPhase { player: PlayerRef },
    ExtraTurn { player: PlayerRef },

    // --- controle de fluxo ---
    Conditional { cond: Condition, then_do: Box<Effect>, else_do: Option<Box<Effect>> },
    ForEach { over: Selector, do_: Box<Effect> },
    Repeat { times: Value, do_: Box<Effect> },
    /// Opcional: o controlador escolhe se executa.
    May { do_: Box<Effect>, prompt: String },
    /// Modal — o controlador escolhe `choose` opções entre as disponíveis.
    Modal { choose: u8, options: Vec<(String, Effect)> },

    // --- fim de jogo ---
    WinGame { player: PlayerRef },
    LoseGame { player: PlayerRef },
}

impl Effect {
    pub fn seq(effects: impl IntoIterator<Item = Effect>) -> Effect {
        Effect::Sequence(effects.into_iter().collect())
    }

    /// Maior índice de alvo referenciado — usado para validar a definição da carta.
    pub fn max_target_index(&self) -> Option<u8> {
        let mut max: Option<u8> = None;
        self.walk(&mut |e| {
            for r in e.direct_refs() {
                if let ObjRef::Target(i) = r {
                    max = Some(max.map_or(i, |m: u8| m.max(i)));
                }
            }
            if let Effect::DivideDamage { targets, .. } = e {
                for i in targets {
                    max = Some(max.map_or(*i, |m: u8| m.max(*i)));
                }
            }
        });
        max
    }

    fn direct_refs(&self) -> Vec<ObjRef> {
        use Effect::*;
        match self {
            DealDamage { target, .. }
            | Destroy { target, .. }
            | Exile { target, .. }
            | Tap { target }
            | Untap { target }
            | Freeze { target }
            | ReturnToHand { target }
            | ReturnFromGraveyardToBattlefield { target }
            | PutOnTopOfLibrary { target }
            | PutOnBottomOfLibrary { target }
            | Transform { target }
            | ModifyPT { target, .. }
            | SetPT { target, .. }
            | GrantKeywords { target, .. }
            | LoseKeywords { target, .. }
            | AddCounters { target, .. }
            | RemoveCounters { target, .. }
            | CantBeBlocked { target, .. }
            | CantAttackOrBlock { target, .. }
            | CounterSpell { target, .. }
            | CopySpell { target, .. }
            | GainControl { target, .. } => vec![target.clone()],
            Fight { a, b } => vec![a.clone(), b.clone()],
            AttachTo { equipment, target } => vec![equipment.clone(), target.clone()],
            _ => Vec::new(),
        }
    }

    /// Percorre a árvore aplicando `f` em cada nó.
    pub fn walk(&self, f: &mut impl FnMut(&Effect)) {
        f(self);
        match self {
            Effect::Sequence(v) => v.iter().for_each(|e| e.walk(f)),
            Effect::Conditional { then_do, else_do, .. } => {
                then_do.walk(f);
                if let Some(e) = else_do {
                    e.walk(f);
                }
            }
            Effect::ForEach { do_, .. } | Effect::Repeat { do_, .. } | Effect::May { do_, .. } => {
                do_.walk(f)
            }
            Effect::Modal { options, .. } => options.iter().for_each(|(_, e)| e.walk(f)),
            _ => {}
        }
    }
}

/// Especificação de um alvo exigido por uma habilidade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSpec {
    pub kind: TargetKind,
    /// Texto para a UI ("alvo de criatura que um oponente controla").
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    Object(Selector),
    Player(PlayerRef),
    /// Criatura ou jogador — dano tipo Shock.
    ObjectOrPlayer(Selector, PlayerRef),
    SpellOnStack(Filter),
}

/// Momento em que uma habilidade ativada pode ser usada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingRestriction {
    /// Qualquer momento em que se tem prioridade.
    Instant,
    /// Só no seu turno, fase principal, pilha vazia.
    Sorcery,
}

/// Contexto mínimo de quem controla a fonte de um efeito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceContext {
    pub controller: PlayerId,
}

// ---------------------------------------------------------------------------
// Modificação contínua já resolvida
// ---------------------------------------------------------------------------

/// Versão de `StaticMod` com valores já calculados. Efeito contínuo criado por
/// resolução trava seus números no momento da resolução (CR 611.2c); efeito de
/// habilidade estática recalcula sempre. Por isso os dois tipos são distintos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaticModRuntime {
    ModifyPT(i32, i32),
    SetPT(i32, i32),
    GrantKeywords(Vec<Keyword>),
    LoseKeywords(Vec<Keyword>),
    SetColors(Vec<Color>),
    AddTypes(TypeLine),
    GainControl(PlayerId),
    CantAttack,
    CantBlock,
    CantBeBlocked,
    CantBeBlockedExceptBy(Filter),
    CantAttackOrBlock,
    PreventAllDamage,
    /// Não desvira no próximo passo de desvirar.
    DoesNotUntap,
}

impl StaticModRuntime {
    /// Camada do sistema de camadas (CR 613). A ordem de aplicação é o que
    /// separa um motor que acerta "Humility + Opalescence" de um que não.
    pub fn layer(&self) -> u8 {
        match self {
            StaticModRuntime::GainControl(_) => 2,
            StaticModRuntime::AddTypes(_) => 4,
            StaticModRuntime::SetColors(_) => 5,
            StaticModRuntime::GrantKeywords(_)
            | StaticModRuntime::LoseKeywords(_)
            | StaticModRuntime::CantAttack
            | StaticModRuntime::CantBlock
            | StaticModRuntime::CantBeBlocked
            | StaticModRuntime::CantBeBlockedExceptBy(_)
            | StaticModRuntime::CantAttackOrBlock
            | StaticModRuntime::PreventAllDamage
            | StaticModRuntime::DoesNotUntap => 6,
            // 7b: define P/T; 7c: modifica P/T.
            StaticModRuntime::SetPT(_, _) => 7,
            StaticModRuntime::ModifyPT(_, _) => 8,
        }
    }
}
