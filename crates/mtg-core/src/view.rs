//! Projeção do estado para a interface.
//!
//! A UI nunca recebe `GameState` cru: recebe `GameView`, já com características
//! calculadas (P/T depois das camadas, palavras-chave concedidas, se pode
//! atacar) e com zonas ocultas redigidas conforme o observador. Isso mantém
//! regra fora do frontend — o React desenha, não decide.
use crate::event::{Defender, Phase, Step};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{ColorSet, ManaPool};
use crate::state::GameOutcome;
use crate::zone::ZoneKind;
use serde::{Deserialize, Serialize};

/// Carta como a UI precisa dela. Tudo já resolvido, nada a calcular no cliente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    pub id: ObjectId,
    /// `None` quando a carta está oculta para o observador.
    pub name: Option<String>,
    pub mana_cost: Option<String>,
    pub mana_value: u32,
    pub type_line: Option<String>,
    pub oracle_text: Option<String>,
    pub flavor_text: Option<String>,
    pub colors: ColorSet,
    /// P/T atuais, já com camadas e marcadores aplicados.
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    /// P/T impressos, para a UI mostrar o buff em destaque.
    pub base_power: Option<i32>,
    pub base_toughness: Option<i32>,
    pub loyalty: Option<i32>,
    pub damage: i32,
    pub tapped: bool,
    pub face_down: bool,
    pub summoning_sick: bool,
    pub attacking: Option<Defender>,
    pub blocking: Vec<ObjectId>,
    pub blocked_by: Vec<ObjectId>,
    pub counters: Vec<(String, i32)>,
    pub keywords: Vec<String>,
    pub attached_to: Option<ObjectId>,
    pub attachments: Vec<ObjectId>,
    pub is_token: bool,
    pub controller: PlayerId,
    pub owner: PlayerId,
    pub zone: ZoneKind,
    pub art_key: Option<String>,
    pub rarity: Option<String>,
    pub set_code: Option<String>,
    /// Verdadeiro quando este objeto é alvo legal da decisão pendente.
    pub is_legal_target: bool,
    /// Verdadeiro quando o observador pode agir com esta carta agora.
    pub is_actionable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    pub id: PlayerId,
    pub name: String,
    pub life: i32,
    pub poison: i32,
    pub mana_pool: ManaPool,
    pub hand_count: usize,
    pub library_count: usize,
    pub graveyard_count: usize,
    pub exile_count: usize,
    pub lands_played_this_turn: u8,
    pub max_lands_per_turn: u8,
    pub has_lost: bool,
    pub is_active: bool,
    pub has_priority: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackItemView {
    pub id: ObjectId,
    pub name: String,
    pub text: String,
    pub controller: PlayerId,
    pub targets: Vec<ObjectId>,
    pub target_players: Vec<PlayerId>,
    pub is_ability: bool,
    pub source_card: ObjectId,
    pub art_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    pub turn: u32,
    pub step: Step,
    pub step_label: String,
    pub phase: Phase,
    pub active_player: PlayerId,
    pub priority_player: Option<PlayerId>,
    pub outcome: GameOutcome,
    pub players: Vec<PlayerView>,
    /// Todas as cartas visíveis ao observador, indexadas por id.
    pub cards: Vec<CardView>,
    pub battlefield: Vec<Vec<ObjectId>>,
    pub hands: Vec<Vec<ObjectId>>,
    pub graveyards: Vec<Vec<ObjectId>>,
    pub exiles: Vec<Vec<ObjectId>>,
    pub stack: Vec<StackItemView>,
    /// Descrição da decisão pendente, para a UI legendar.
    pub prompt: Option<String>,
    pub log_tail: Vec<String>,
}

// ---------------------------------------------------------------------------
// Eventos de animação
// ---------------------------------------------------------------------------

/// Delta semântico entre dois estados, o suficiente para a UI animar.
/// Sem isso o frontend só consegue trocar um estado por outro, e a partida
/// vira slideshow em vez de jogo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MatchEvent {
    TurnStart { turn: u32, player: PlayerId },
    StepChange { step: Step, label: String },
    CardMoved { card: ObjectId, from: ZoneKind, to: ZoneKind, owner: PlayerId, reveal: bool },
    CardDrawn { card: ObjectId, player: PlayerId },
    SpellCast { card: ObjectId, player: PlayerId, targets: Vec<ObjectId> },
    SpellResolved { card: ObjectId },
    SpellCountered { card: ObjectId },
    AbilityTriggered { source: ObjectId, text: String },
    Tapped { card: ObjectId },
    Untapped { card: ObjectId },
    AttackersDeclared { attackers: Vec<(ObjectId, Defender)> },
    BlockersDeclared { blocks: Vec<(ObjectId, Vec<ObjectId>)> },
    DamageDealt { source: ObjectId, target: ObjectId, amount: i32, lethal: bool },
    DamageToPlayer { source: ObjectId, player: PlayerId, amount: i32 },
    LifeChanged { player: PlayerId, delta: i32, total: i32 },
    CountersChanged { card: ObjectId, kind: String, delta: i32 },
    TokenCreated { card: ObjectId, controller: PlayerId },
    Died { card: ObjectId },
    Destroyed { card: ObjectId },
    Exiled { card: ObjectId },
    PlayerLost { player: PlayerId, reason: String },
    GameOver { outcome: GameOutcome },
    Log { text: String },
}

impl MatchEvent {
    /// Duração sugerida da animação, em milissegundos. A UI pode ignorar, mas
    /// ter o ritmo definido no motor mantém desktop e mobile sincronizados.
    pub fn suggested_duration_ms(&self) -> u32 {
        match self {
            MatchEvent::TurnStart { .. } => 900,
            MatchEvent::StepChange { .. } => 220,
            MatchEvent::SpellCast { .. } => 700,
            MatchEvent::SpellResolved { .. } => 450,
            MatchEvent::SpellCountered { .. } => 650,
            MatchEvent::AbilityTriggered { .. } => 600,
            MatchEvent::CardMoved { .. } | MatchEvent::CardDrawn { .. } => 380,
            MatchEvent::AttackersDeclared { .. } => 800,
            MatchEvent::BlockersDeclared { .. } => 700,
            MatchEvent::DamageDealt { .. } | MatchEvent::DamageToPlayer { .. } => 520,
            MatchEvent::LifeChanged { .. } => 400,
            MatchEvent::Died { .. } | MatchEvent::Destroyed { .. } => 600,
            MatchEvent::GameOver { .. } => 1600,
            _ => 260,
        }
    }
}

/// Quem está olhando. `Spectator` enxerga tudo que é público; `Player` enxerga
/// também a própria mão. `Omniscient` é só para debug e replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observer {
    Player(PlayerId),
    Spectator,
    Omniscient,
}

impl Observer {
    pub fn can_see_hand(&self, p: PlayerId) -> bool {
        match self {
            Observer::Player(me) => *me == p,
            Observer::Spectator => false,
            Observer::Omniscient => true,
        }
    }
    pub fn can_see_library(&self) -> bool {
        matches!(self, Observer::Omniscient)
    }
}
