//! Estado da partida. Puramente serializável — sem RNG, sem I/O, sem banco.
//! Isso é o que permite snapshot, replay determinístico e busca em árvore.
use crate::action::{Request, TargetChoice};
use crate::ids::CardDefId;
use crate::event::{Defender, GameEvent, LossReason, Phase, Step};
use crate::ids::{AbilityRef, IdGen, ObjectId, PlayerId};
use crate::ir::{Duration, Keyword, StaticModRuntime, TokenSpec};
use crate::mana::{ColorSet, ManaPool};
use crate::types::{CounterKind, TypeLine};
use crate::zone::{Zone, ZoneId, ZoneKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Objeto
// ---------------------------------------------------------------------------

/// Estado de combate de um permanente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatState {
    pub attacking: Option<Defender>,
    /// Atacantes que este permanente está bloqueando.
    pub blocking: Vec<ObjectId>,
    /// Bloqueadores deste atacante, na ordem de dano escolhida pelo atacante.
    pub blocked_by: Vec<ObjectId>,
    /// Foi bloqueado neste combate, mesmo que os bloqueadores tenham saído.
    pub was_blocked: bool,
    pub removed_from_combat: bool,
}

impl CombatState {
    pub fn is_attacking(&self) -> bool {
        self.attacking.is_some() && !self.removed_from_combat
    }
    pub fn is_blocking(&self) -> bool {
        !self.blocking.is_empty() && !self.removed_from_combat
    }
    pub fn clear(&mut self) {
        *self = CombatState::default();
    }
}

/// Instância de um objeto no jogo. Carta, ficha, ou cópia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectState {
    pub id: ObjectId,
    pub card: CardDefId,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub zone: ZoneId,

    pub tapped: bool,
    pub face_down: bool,
    pub flipped: bool,
    /// Não desvira no próximo passo de desvirar.
    pub frozen: bool,
    /// Entrou no campo neste turno e ainda não pode atacar/usar {T}.
    pub summoning_sick: bool,
    pub damage: i32,
    /// Dano de fonte com toque mortal marcado neste permanente.
    pub deathtouch_damage: bool,
    pub counters: BTreeMap<CounterKind, i32>,

    pub attached_to: Option<ObjectId>,
    pub attachments: Vec<ObjectId>,

    pub is_token: bool,
    pub token_spec: Option<Box<TokenSpec>>,
    /// Turno em que entrou na zona atual.
    pub entered_turn: u32,
    /// Timestamp global para camadas (CR 613.7).
    pub timestamp: u64,

    pub combat: CombatState,
    /// CR 903.3 — o objeto é o comandante do seu dono. A designação é da carta,
    /// não da instância, então `reset_for_zone_change` não a apaga.
    #[serde(default)]
    pub is_commander: bool,
    /// Usos de habilidades ativadas neste turno: (índice, contagem).
    pub ability_uses: BTreeMap<u16, u8>,
    /// Gatilhos "uma vez por turno" já disparados.
    pub triggers_fired: BTreeMap<u16, u8>,
    /// Marcado por efeito de regeneração / escudo.
    pub regeneration_shields: u8,
}

impl ObjectState {
    pub fn new(id: ObjectId, card: CardDefId, owner: PlayerId, zone: ZoneId, ts: u64) -> Self {
        ObjectState {
            id,
            card,
            owner,
            controller: owner,
            zone,
            tapped: false,
            face_down: false,
            flipped: false,
            frozen: false,
            summoning_sick: false,
            damage: 0,
            deathtouch_damage: false,
            counters: BTreeMap::new(),
            attached_to: None,
            attachments: Vec::new(),
            is_token: false,
            token_spec: None,
            entered_turn: 0,
            timestamp: ts,
            combat: CombatState::default(),
            is_commander: false,
            ability_uses: BTreeMap::new(),
            triggers_fired: BTreeMap::new(),
            regeneration_shields: 0,
        }
    }
    pub fn counter(&self, kind: &CounterKind) -> i32 {
        self.counters.get(kind).copied().unwrap_or(0)
    }
    pub fn add_counter(&mut self, kind: CounterKind, n: i32) {
        *self.counters.entry(kind).or_insert(0) += n;
    }
    pub fn on_battlefield(&self) -> bool {
        self.zone.kind == ZoneKind::Battlefield
    }
    /// Reseta o que não sobrevive a mudança de zona (CR 400.7).
    pub fn reset_for_zone_change(&mut self, new_zone: ZoneId, turn: u32, ts: u64) {
        self.zone = new_zone;
        self.tapped = false;
        self.damage = 0;
        self.deathtouch_damage = false;
        self.counters.clear();
        self.attached_to = None;
        self.attachments.clear();
        self.combat.clear();
        self.ability_uses.clear();
        self.triggers_fired.clear();
        self.regeneration_shields = 0;
        self.frozen = false;
        self.face_down = false;
        self.entered_turn = turn;
        self.timestamp = ts;
        self.summoning_sick = new_zone.kind == ZoneKind::Battlefield;
    }
}

// ---------------------------------------------------------------------------
// Pilha
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackItemKind {
    /// Mágica: o próprio objeto-carta está na pilha.
    Spell { object: ObjectId },
    ActivatedAbility { source: ObjectId, index: u16 },
    TriggeredAbility { source: ObjectId, index: u16 },
    /// Cópia de mágica criada por efeito.
    CopiedSpell { original: ObjectId, source_card: CardDefId },
}

impl StackItemKind {
    pub fn source(&self) -> ObjectId {
        match self {
            StackItemKind::Spell { object } => *object,
            StackItemKind::ActivatedAbility { source, .. }
            | StackItemKind::TriggeredAbility { source, .. } => *source,
            StackItemKind::CopiedSpell { original, .. } => *original,
        }
    }
    pub fn as_ability_ref(&self) -> Option<AbilityRef> {
        match self {
            StackItemKind::ActivatedAbility { source, index }
            | StackItemKind::TriggeredAbility { source, index } => {
                Some(AbilityRef { object: *source, index: *index })
            }
            _ => None,
        }
    }
}

/// Retrato das características de um objeto no instante de um evento —
/// informação conhecida por último (CR 608.2h, CR 603.6d, CR 113.7a).
///
/// Existe porque `ObjectState::reset_for_zone_change` apaga marcador, dano e
/// combate assim que o objeto muda de zona, e as camadas (anthem, casca de
/// equipamento, mudança de tipo) deixam de valer para um objeto fora do campo.
/// Sem este retrato, "quando isto morrer, ganhe vida igual à sua resistência"
/// leria a resistência impressa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastKnown {
    pub object: ObjectId,
    /// Zona em que o objeto estava quando o retrato foi tirado. Enquanto ele
    /// continuar nela, o estado corrente é a verdade e o retrato é ignorado.
    pub zone: ZoneId,
    pub name: String,
    pub power: i32,
    pub toughness: i32,
    pub mana_value: u32,
    pub type_line: TypeLine,
    pub colors: ColorSet,
    pub keywords: Vec<Keyword>,
    pub counters: BTreeMap<CounterKind, i32>,
    pub controller: PlayerId,
    pub owner: PlayerId,
    pub tapped: bool,
    pub damage: i32,
    pub is_token: bool,
    pub summoning_sick: bool,
    pub combat: CombatState,
}

impl LastKnown {
    pub fn counter(&self, kind: &CounterKind) -> i32 {
        self.counters.get(kind).copied().unwrap_or(0)
    }
    pub fn has_keyword(&self, k: &Keyword) -> bool {
        self.keywords.iter().any(|x| x == k)
    }
}

/// Contexto capturado quando um gatilho dispara — as regras exigem que ele
/// lembre o que aconteceu mesmo que o objeto já tenha sumido.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerContext {
    pub trigger_object: Option<ObjectId>,
    pub trigger_source: Option<ObjectId>,
    pub trigger_player: Option<PlayerId>,
    pub amount: i32,
    /// CR 603.6d — retrato do `trigger_object` no instante do evento. Só é
    /// preenchido quando o objeto já deixou a zona em que estava: enquanto ele
    /// segue lá, ler o estado corrente é mais barato e igualmente correto, e o
    /// contexto não paga cópia nenhuma. Em `Box` porque a maioria dos gatilhos
    /// (começo de passo, dano, compra) nunca precisa dele e `StackItem` é
    /// clonado a cada resolução.
    pub last_known: Option<Box<LastKnown>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackItem {
    /// Id próprio do item na pilha (para ordenação e referência).
    pub id: ObjectId,
    pub kind: StackItemKind,
    pub controller: PlayerId,
    pub card: CardDefId,
    pub targets: Vec<TargetChoice>,
    pub x_value: u32,
    pub modes: Vec<u8>,
    pub trigger_ctx: TriggerContext,
    /// Objetos lembrados por efeitos anteriores da mesma resolução.
    pub remembered: Vec<ObjectId>,
    pub optional_confirmed: bool,
}

// ---------------------------------------------------------------------------
// Efeitos contínuos
// ---------------------------------------------------------------------------

/// Efeito contínuo gerado por resolução (não por habilidade estática).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuousEffect {
    pub id: u32,
    pub source: ObjectId,
    pub affected: Vec<ObjectId>,
    pub modification: StaticModRuntime,
    pub duration: Duration,
    pub timestamp: u64,
    /// Turno em que foi criado — usado para expirar `EndOfTurn`.
    pub created_turn: u32,
    pub controller: PlayerId,
}

// ---------------------------------------------------------------------------
// Jogador
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub name: String,
    pub life: i32,
    pub poison: i32,
    pub mana_pool: ManaPool,
    pub lands_played_this_turn: u8,
    pub max_lands_per_turn: u8,
    pub max_hand_size: usize,
    pub has_lost: bool,
    pub loss_reason: Option<LossReason>,
    pub mulligans_taken: u8,
    /// Contadores de estatística para a UI e para a IA.
    pub cards_drawn_this_turn: u32,
    pub spells_cast_this_turn: u32,
    pub life_gained_this_turn: i32,
    pub life_lost_this_turn: i32,
    pub damage_taken_this_turn: i32,

    /// CR 903.6 — objeto do comandante deste jogador. `None` fora de Commander.
    /// Commander de duas cabeças / Partner precisariam de uma lista; esta
    /// rodada suporta um comandante por jogador.
    #[serde(default)]
    pub commander: Option<ObjectId>,
    /// CR 903.8 — quantas vezes o comandante já foi lançado da zona de comando
    /// nesta partida. A taxa é {2} genérico por lançamento anterior.
    #[serde(default)]
    pub commander_casts: u32,
    /// CR 903.10 — dano de combate recebido de cada comandante, por objeto.
    /// `BTreeMap` e não `HashMap`: a ordem de iteração é parte do determinismo.
    #[serde(default)]
    pub commander_damage: BTreeMap<ObjectId, i32>,
}

impl PlayerState {
    pub fn new(id: PlayerId, name: impl Into<String>, life: i32) -> Self {
        PlayerState {
            id,
            name: name.into(),
            life,
            poison: 0,
            mana_pool: ManaPool::default(),
            lands_played_this_turn: 0,
            max_lands_per_turn: 1,
            max_hand_size: 7,
            has_lost: false,
            loss_reason: None,
            mulligans_taken: 0,
            cards_drawn_this_turn: 0,
            spells_cast_this_turn: 0,
            life_gained_this_turn: 0,
            life_lost_this_turn: 0,
            damage_taken_this_turn: 0,
            commander: None,
            commander_casts: 0,
            commander_damage: BTreeMap::new(),
        }
    }
    /// CR 903.10 — dano de combate já recebido do comandante indicado.
    pub fn commander_damage_from(&self, commander: ObjectId) -> i32 {
        self.commander_damage.get(&commander).copied().unwrap_or(0)
    }
    pub fn reset_turn_counters(&mut self) {
        self.lands_played_this_turn = 0;
        self.cards_drawn_this_turn = 0;
        self.spells_cast_this_turn = 0;
        self.life_gained_this_turn = 0;
        self.life_lost_this_turn = 0;
        self.damage_taken_this_turn = 0;
    }
}

// ---------------------------------------------------------------------------
// Estado global
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameOutcome {
    Ongoing,
    Winner(PlayerId),
    Draw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<PlayerState>,
    /// Indexado por `ObjectId.0`. Ids são monotônicos, então vetor basta.
    pub objects: Vec<ObjectState>,
    pub zones: BTreeMap<(ZoneKind, u8), Zone>,
    pub stack: Vec<StackItem>,
    pub continuous: Vec<ContinuousEffect>,

    pub turn: u32,
    pub active_player: PlayerId,
    pub priority_player: PlayerId,
    pub step: Step,
    /// Jogadores que passaram em sequência sem nada acontecer.
    pub consecutive_passes: u8,

    pub extra_turns: Vec<PlayerId>,
    pub extra_combats: u8,
    /// Combate atual foi resolvido com dano de primeiro golpe.
    pub first_strike_done: bool,

    pub outcome: GameOutcome,
    pub pending: Request,
    pub id_gen: IdGen,
    pub timestamp: u64,
    pub next_effect_id: u32,
    /// Fila de eventos ainda não processada por gatilhos.
    pub event_queue: Vec<GameEvent>,
    /// Gatilhos que dispararam e esperam ir para a pilha.
    pub pending_triggers: Vec<StackItem>,
    /// CR 603.6d — retrato de cada objeto no instante em que deixou o campo de
    /// batalha, gravado por `turn::move_object` antes de `reset_for_zone_change`
    /// destruir o estado. Uma entrada por objeto (a saída mais recente
    /// sobrescreve a anterior), então o mapa é limitado por `objects.len()`.
    #[serde(default)]
    pub last_known: BTreeMap<ObjectId, LastKnown>,
    /// Log completo para replay e para a UI.
    pub log: Vec<LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub turn: u32,
    pub step: Step,
    pub actor: Option<PlayerId>,
    pub text: String,
    pub event: Option<GameEvent>,
}

impl GameState {
    pub fn object(&self, id: ObjectId) -> Option<&ObjectState> {
        self.objects.get(id.0 as usize)
    }
    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut ObjectState> {
        self.objects.get_mut(id.0 as usize)
    }
    pub fn player(&self, id: PlayerId) -> &PlayerState {
        &self.players[id.index()]
    }
    pub fn player_mut(&mut self, id: PlayerId) -> &mut PlayerState {
        &mut self.players[id.index()]
    }
    pub fn zone(&self, z: ZoneId) -> &Zone {
        &self.zones[&(z.kind, z.owner.map_or(u8::MAX, |p| p.0))]
    }
    pub fn zone_mut(&mut self, z: ZoneId) -> &mut Zone {
        self.zones
            .get_mut(&(z.kind, z.owner.map_or(u8::MAX, |p| p.0)))
            .expect("zona inexistente")
    }
    pub fn battlefield(&self) -> &Zone {
        self.zone(ZoneId::BATTLEFIELD)
    }
    pub fn hand(&self, p: PlayerId) -> &Zone {
        self.zone(ZoneId::hand(p))
    }
    pub fn library(&self, p: PlayerId) -> &Zone {
        self.zone(ZoneId::library(p))
    }
    pub fn graveyard(&self, p: PlayerId) -> &Zone {
        self.zone(ZoneId::graveyard(p))
    }
    /// CR 903.6 — zona de comando do jogador.
    pub fn command(&self, p: PlayerId) -> &Zone {
        self.zone(ZoneId::command(p))
    }
    pub fn opponents(&self, p: PlayerId) -> Vec<PlayerId> {
        self.players.iter().map(|x| x.id).filter(|x| *x != p).collect()
    }
    /// Próximo jogador na ordem de turno.
    pub fn next_player(&self, p: PlayerId) -> PlayerId {
        let n = self.players.len() as u8;
        PlayerId((p.0 + 1) % n)
    }
    pub fn is_over(&self) -> bool {
        !matches!(self.outcome, GameOutcome::Ongoing)
    }
    pub fn next_timestamp(&mut self) -> u64 {
        self.timestamp += 1;
        self.timestamp
    }
    pub fn permanents(&self) -> impl Iterator<Item = &ObjectState> + '_ {
        self.battlefield().objects.iter().filter_map(|id| self.object(*id))
    }
    pub fn permanents_controlled_by(&self, p: PlayerId) -> impl Iterator<Item = &ObjectState> + '_ {
        self.permanents().filter(move |o| o.controller == p)
    }
    pub fn push_log(&mut self, text: impl Into<String>, actor: Option<PlayerId>) {
        let entry = LogEntry {
            turn: self.turn,
            step: self.step,
            actor,
            text: text.into(),
            event: None,
        };
        self.log.push(entry);
    }
    pub fn emit(&mut self, event: GameEvent) {
        self.event_queue.push(event);
    }
    pub fn phase(&self) -> Phase {
        self.step.phase()
    }
}
