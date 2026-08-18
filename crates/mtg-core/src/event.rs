//! Eventos do jogo. Tudo que acontece emite um evento; gatilhos observam a
//! fila de eventos e efeitos de substituição a interceptam antes.
use crate::ids::{AbilityRef, ObjectId, PlayerId};
use crate::types::CounterKind;
use crate::zone::ZoneId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageKind {
    Combat,
    Noncombat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    // --- movimento de zona ---
    ObjectMoved { object: ObjectId, from: ZoneId, to: ZoneId },
    EnteredBattlefield { object: ObjectId, controller: PlayerId },
    LeftBattlefield { object: ObjectId, to: ZoneId },
    Died { object: ObjectId, controller: PlayerId },
    Sacrificed { object: ObjectId, controller: PlayerId },
    Exiled { object: ObjectId },

    // --- pilha ---
    SpellCast { object: ObjectId, controller: PlayerId },
    SpellResolved { object: ObjectId },
    SpellCountered { object: ObjectId },
    AbilityActivated { ability: AbilityRef, controller: PlayerId },
    AbilityTriggered { ability: AbilityRef, controller: PlayerId },

    // --- dano e vida ---
    DamageDealt {
        source: ObjectId,
        target: ObjectId,
        amount: i32,
        kind: DamageKind,
        deathtouch: bool,
    },
    DamageDealtToPlayer {
        source: ObjectId,
        player: PlayerId,
        amount: i32,
        kind: DamageKind,
    },
    LifeGained { player: PlayerId, amount: i32 },
    LifeLost { player: PlayerId, amount: i32 },

    // --- cartas ---
    CardDrawn { player: PlayerId, object: ObjectId },
    CardDiscarded { player: PlayerId, object: ObjectId },
    CardMilled { player: PlayerId, object: ObjectId },
    LandPlayed { player: PlayerId, object: ObjectId },

    // --- permanentes ---
    Tapped { object: ObjectId },
    Untapped { object: ObjectId },
    CountersAdded { object: ObjectId, kind: CounterKind, amount: i32 },
    CountersRemoved { object: ObjectId, kind: CounterKind, amount: i32 },
    Attached { equipment: ObjectId, to: ObjectId },
    Unattached { equipment: ObjectId, from: ObjectId },
    ControlChanged { object: ObjectId, from: PlayerId, to: PlayerId },
    Transformed { object: ObjectId },

    // --- combate ---
    AttackersDeclared { attackers: Vec<ObjectId> },
    BlockersDeclared { blocks: Vec<(ObjectId, Vec<ObjectId>)> },
    Attacked { object: ObjectId, defender: Defender },
    Blocked { attacker: ObjectId, blockers: Vec<ObjectId> },
    BecameUnblocked { attacker: ObjectId },
    CombatDamageDealt,

    // --- estrutura de turno ---
    TurnBegan { player: PlayerId, turn: u32 },
    PhaseBegan { phase: Phase },
    StepBegan { step: Step },
    StepEnded { step: Step },
    TurnEnded { player: PlayerId },

    // --- fim de jogo ---
    PlayerLost { player: PlayerId, reason: LossReason },
    PlayerWon { player: PlayerId },
    GameDrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossReason {
    ZeroLife,
    DrewFromEmptyLibrary,
    PoisonCounters,
    Effect,
    Concede,
}

/// Quem está sendo atacado (CR 506.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Defender {
    Player(PlayerId),
    Planeswalker(ObjectId),
    Battle(ObjectId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Phase {
    Beginning,
    PrecombatMain,
    Combat,
    PostcombatMain,
    Ending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    FirstStrikeDamage,
    CombatDamage,
    EndCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
    pub fn phase(self) -> Phase {
        match self {
            Step::Untap | Step::Upkeep | Step::Draw => Phase::Beginning,
            Step::PrecombatMain => Phase::PrecombatMain,
            Step::BeginCombat
            | Step::DeclareAttackers
            | Step::DeclareBlockers
            | Step::FirstStrikeDamage
            | Step::CombatDamage
            | Step::EndCombat => Phase::Combat,
            Step::PostcombatMain => Phase::PostcombatMain,
            Step::End | Step::Cleanup => Phase::Ending,
        }
    }
    /// Passos em que ninguém recebe prioridade (CR 502.1, 514.3).
    pub fn skips_priority(self) -> bool {
        matches!(self, Step::Untap | Step::Cleanup)
    }
    pub fn is_main(self) -> bool {
        matches!(self, Step::PrecombatMain | Step::PostcombatMain)
    }
    pub fn is_combat(self) -> bool {
        self.phase() == Phase::Combat
    }
    /// Sequência canônica de um turno, sem contar fases extras.
    pub const SEQUENCE: [Step; 13] = [
        Step::Untap,
        Step::Upkeep,
        Step::Draw,
        Step::PrecombatMain,
        Step::BeginCombat,
        Step::DeclareAttackers,
        Step::DeclareBlockers,
        Step::FirstStrikeDamage,
        Step::CombatDamage,
        Step::EndCombat,
        Step::PostcombatMain,
        Step::End,
        Step::Cleanup,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Step::Untap => "Untap",
            Step::Upkeep => "Upkeep",
            Step::Draw => "Draw",
            Step::PrecombatMain => "Main 1",
            Step::BeginCombat => "Begin Combat",
            Step::DeclareAttackers => "Declare Attackers",
            Step::DeclareBlockers => "Declare Blockers",
            Step::FirstStrikeDamage => "First Strike Damage",
            Step::CombatDamage => "Combat Damage",
            Step::EndCombat => "End Combat",
            Step::PostcombatMain => "Main 2",
            Step::End => "End Step",
            Step::Cleanup => "Cleanup",
        }
    }
}
