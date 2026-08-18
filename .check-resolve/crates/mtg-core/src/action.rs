//! Interface de decisão. O motor nunca pergunta nada de forma implícita:
//! ele expõe uma `Request` pendente e enumera as `Action` legais para ela.
//!
//! Consequência: o bot é uma função `(GameStateView, &[Action]) -> Action`, o
//! que torna busca em árvore (MCTS/minimax) possível sem tocar no motor.
use crate::event::Defender;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetChoice {
    Object(ObjectId),
    Player(PlayerId),
}

/// Fonte concreta de mana escolhida para pagar um custo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaSourceChoice {
    pub source: ObjectId,
    pub ability_index: u16,
    /// Cor a produzir quando a habilidade é "qualquer cor".
    pub color: Option<Color>,
}

/// O que o motor está esperando neste instante.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Priority {
        player: PlayerId,
    },
    Mulligan {
        player: PlayerId,
        mulligans_taken: u8,
    },
    /// Após manter com N mulligans, devolver N cartas ao fundo.
    BottomCards {
        player: PlayerId,
        count: u8,
    },
    DeclareAttackers {
        player: PlayerId,
        eligible: Vec<ObjectId>,
    },
    DeclareBlockers {
        player: PlayerId,
        eligible: Vec<ObjectId>,
        attackers: Vec<ObjectId>,
    },
    /// Atacante bloqueado por 2+: o atacante ordena os bloqueadores.
    OrderBlockers {
        player: PlayerId,
        attacker: ObjectId,
        blockers: Vec<ObjectId>,
    },
    /// Distribuir dano de combate entre múltiplos bloqueadores.
    AssignCombatDamage {
        player: PlayerId,
        attacker: ObjectId,
        blockers: Vec<ObjectId>,
        total: i32,
    },
    /// Ordenar gatilhos simultâneos do mesmo controlador (CR 603.3b).
    OrderTriggers {
        player: PlayerId,
        triggers: Vec<ObjectId>,
    },
    /// "Você pode..." de gatilho opcional.
    ConfirmOptional {
        player: PlayerId,
        prompt: String,
    },
    ChooseModes {
        player: PlayerId,
        prompt: String,
        options: Vec<String>,
        choose: u8,
    },
    /// Seleção genérica de objetos (sacrificar, descartar, procurar, escolher).
    SelectObjects {
        player: PlayerId,
        prompt: String,
        candidates: Vec<ObjectId>,
        min: u8,
        max: u8,
    },
    ChooseColor {
        player: PlayerId,
        prompt: String,
    },
    /// Scry/Surveil: separar em topo (mantendo ordem) e fundo/cemitério.
    ArrangeCards {
        player: PlayerId,
        prompt: String,
        cards: Vec<ObjectId>,
        /// Rótulo do destino alternativo ("fundo" ou "cemitério").
        alt_label: String,
    },
    GameOver,
}

impl Request {
    pub fn player(&self) -> Option<PlayerId> {
        match self {
            Request::Priority { player }
            | Request::Mulligan { player, .. }
            | Request::BottomCards { player, .. }
            | Request::DeclareAttackers { player, .. }
            | Request::DeclareBlockers { player, .. }
            | Request::OrderBlockers { player, .. }
            | Request::AssignCombatDamage { player, .. }
            | Request::OrderTriggers { player, .. }
            | Request::ConfirmOptional { player, .. }
            | Request::ChooseModes { player, .. }
            | Request::SelectObjects { player, .. }
            | Request::ChooseColor { player, .. }
            | Request::ArrangeCards { player, .. } => Some(*player),
            Request::GameOver => None,
        }
    }
}

/// Resposta a uma `Request`. Só ações enumeradas por `legal_actions` são aceitas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    PassPriority,
    Concede,

    PlayLand {
        object: ObjectId,
    },
    CastSpell {
        object: ObjectId,
        #[serde(default)]
        targets: Vec<TargetChoice>,
        #[serde(default)]
        x: u32,
        #[serde(default)]
        modes: Vec<u8>,
        #[serde(default)]
        mana_plan: Vec<ManaSourceChoice>,
    },
    ActivateAbility {
        source: ObjectId,
        index: u16,
        #[serde(default)]
        targets: Vec<TargetChoice>,
        #[serde(default)]
        x: u32,
        #[serde(default)]
        mana_plan: Vec<ManaSourceChoice>,
    },

    Mulligan,
    KeepHand,
    PutOnBottom {
        objects: Vec<ObjectId>,
    },

    Attack {
        assignments: Vec<(ObjectId, Defender)>,
    },
    Block {
        /// (bloqueador, atacante)
        assignments: Vec<(ObjectId, ObjectId)>,
    },
    OrderBlockers {
        attacker: ObjectId,
        order: Vec<ObjectId>,
    },
    AssignDamage {
        attacker: ObjectId,
        /// (bloqueador, dano). O resto vai ao defensor se houver atropelar.
        assignments: Vec<(ObjectId, i32)>,
        trample_to_defender: i32,
    },

    OrderTriggers {
        order: Vec<ObjectId>,
    },
    Confirm {
        yes: bool,
    },
    ChooseModes {
        modes: Vec<u8>,
    },
    SelectObjects {
        objects: Vec<ObjectId>,
    },
    ChooseColor {
        color: Color,
    },
    ArrangeCards {
        top: Vec<ObjectId>,
        alt: Vec<ObjectId>,
    },
}

impl Action {
    /// Rótulo curto para log e UI.
    pub fn label(&self) -> &'static str {
        match self {
            Action::PassPriority => "pass",
            Action::Concede => "concede",
            Action::PlayLand { .. } => "play land",
            Action::CastSpell { .. } => "cast",
            Action::ActivateAbility { .. } => "activate",
            Action::Mulligan => "mulligan",
            Action::KeepHand => "keep",
            Action::PutOnBottom { .. } => "bottom",
            Action::Attack { .. } => "attack",
            Action::Block { .. } => "block",
            Action::OrderBlockers { .. } => "order blockers",
            Action::AssignDamage { .. } => "assign damage",
            Action::OrderTriggers { .. } => "order triggers",
            Action::Confirm { .. } => "confirm",
            Action::ChooseModes { .. } => "choose modes",
            Action::SelectObjects { .. } => "select",
            Action::ChooseColor { .. } => "choose color",
            Action::ArrangeCards { .. } => "arrange",
        }
    }
    pub fn is_pass(&self) -> bool {
        matches!(self, Action::PassPriority)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("ação ilegal no estado atual: {0}")]
    Illegal(String),
    #[error("não é a vez de {0}")]
    WrongPlayer(PlayerId),
    #[error("objeto {0} não existe ou está na zona errada")]
    BadObject(ObjectId),
    #[error("alvos inválidos: {0}")]
    BadTargets(String),
    #[error("mana insuficiente para pagar {0}")]
    CannotPay(String),
    #[error("partida já terminou")]
    GameOver,
}
