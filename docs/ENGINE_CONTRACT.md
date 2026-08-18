# Contrato do motor — assinaturas obrigatórias

Todo builder implementa **exatamente** estas assinaturas. `crates/mtg-core/src/engine/mod.rs`
já chama por elas; qualquer divergência quebra a compilação do workspace.

Regras gerais:
- `#![forbid(unsafe_code)]` está ativo no crate. Sem `unsafe`.
- Nada de I/O, `println!`, `std::fs`, rede ou tempo dentro de `mtg-core`.
- Aleatoriedade só via `game.rng` (`ChaCha8Rng`) — determinismo por semente é requisito.
- Nunca ler `CardDef` diretamente para decidir regra: use `layers::characteristics`.
- Todo efeito colateral visível emite `MatchEvent` via `game.push_event(...)` e
  `GameEvent` via `game.state.emit(...)`.
- Comentário explica **porquê**, não o quê. Referencie a regra: `// CR 704.5g`.

## `engine/query.rs` — leitura pura

```rust
use super::Game;
use crate::action::TargetChoice;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{Condition, Filter, ObjRef, PlayerRef, Selector, TargetSpec, Value};
use crate::state::TriggerContext;

#[derive(Debug, Clone, Default)]
pub struct EvalCtx {
    pub source: Option<ObjectId>,
    pub controller: PlayerId,
    pub targets: Vec<TargetChoice>,
    pub x: u32,
    pub selected: Option<ObjectId>,
    pub trigger: TriggerContext,
    pub remembered: Vec<ObjectId>,
    pub chosen_number: i32,
}
impl EvalCtx {
    pub fn for_source(source: ObjectId, controller: PlayerId) -> Self;
}

pub fn matches_filter(game: &Game, obj: ObjectId, filter: &Filter, ctx: &EvalCtx) -> bool;
pub fn select(game: &Game, sel: &Selector, ctx: &EvalCtx) -> Vec<ObjectId>;
pub fn eval_value(game: &Game, v: &Value, ctx: &EvalCtx) -> i32;
pub fn eval_condition(game: &Game, c: &Condition, ctx: &EvalCtx) -> bool;
pub fn resolve_players(game: &Game, r: &PlayerRef, ctx: &EvalCtx) -> Vec<PlayerId>;
pub fn resolve_objects(game: &Game, r: &ObjRef, ctx: &EvalCtx) -> Vec<ObjectId>;
pub fn legal_targets(game: &Game, spec: &TargetSpec, ctx: &EvalCtx) -> Vec<TargetChoice>;
pub fn can_be_targeted(game: &Game, obj: ObjectId, source: Option<ObjectId>, by: PlayerId) -> bool;
pub fn target_still_legal(game: &Game, t: TargetChoice, spec: &TargetSpec, ctx: &EvalCtx) -> bool;
```

`PlayerId` precisa de `Default` para `EvalCtx: Default` — adicione `impl Default for PlayerId`
em `ids.rs` retornando `PlayerId(0)` se ainda não existir.

## `engine/layers.rs` — CR 613

```rust
use super::{Characteristics, Game};
use crate::ids::ObjectId;

pub fn characteristics(game: &Game, id: ObjectId) -> Option<Characteristics>;
pub fn base_characteristics(game: &Game, id: ObjectId) -> Option<Characteristics>;
pub fn expire_continuous_effects(game: &mut Game);
```

Ordem das camadas: 1 cópia · 2 controle · 3 texto · 4 tipo · 5 cor · 6 habilidade ·
7a define P/T por característica · 7b define P/T · 7c modifica P/T · 7d marcadores ·
7e troca P/T. Dentro da camada, ordenar por `timestamp` (CR 613.7).
Habilidades estáticas de permanentes no campo entram junto com `state.continuous`.

## `engine/sba.rs` — CR 704

```rust
use super::Game;
pub fn check(game: &mut Game) -> bool;          // true se alguma SBA se aplicou
pub fn check_until_stable(game: &mut Game);     // laço com guarda de 1000
```

Cobrir: 704.5a vida ≤ 0 · 704.5b comprou de biblioteca vazia · 704.5c 10 venenos ·
704.5f resistência ≤ 0 · 704.5g dano letal · 704.5h toque mortal · 704.5i lealdade 0 ·
704.5j regra da lenda · 704.5m aura ilegal · 704.5q equipamento ilegal ·
704.5r marcadores +1/+1 e −1/−1 se anulam.

## `engine/triggers.rs` — CR 603

```rust
use super::Game;
use crate::card::TriggerCondition;
use crate::event::GameEvent;
use crate::ids::ObjectId;
use crate::state::TriggerContext;

pub fn collect(game: &mut Game);
pub fn matches(game: &Game, cond: &TriggerCondition, ev: &GameEvent, source: ObjectId)
    -> Option<TriggerContext>;
pub fn fire_step_triggers(game: &mut Game);
```

## `engine/stack.rs` — CR 601, 603.3, 608

```rust
use super::Game;
use crate::action::{Action, TargetChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::state::StackItem;

pub fn put_triggers_on_stack(game: &mut Game);
pub fn resolve_top(game: &mut Game);
pub fn counter_item(game: &mut Game, stack_id: ObjectId);
pub fn push_spell(game: &mut Game, object: ObjectId, controller: PlayerId,
                  targets: Vec<TargetChoice>, x: u32, modes: Vec<u8>);
pub fn push_activated(game: &mut Game, source: ObjectId, index: u16, controller: PlayerId,
                      targets: Vec<TargetChoice>, x: u32);
pub fn trigger_order_options(triggers: &[ObjectId]) -> Vec<Action>;
pub fn peek(game: &Game) -> Option<&StackItem>;
```

## `engine/cast.rs` — CR 601, 117

```rust
use super::Game;
use crate::action::{Action, ActionError, ManaSourceChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::ir::Cost;

pub fn priority_actions(game: &Game, player: PlayerId) -> Vec<Action>;
pub fn execute(game: &mut Game, player: PlayerId, action: Action) -> Result<(), ActionError>;
pub fn can_pay(game: &Game, player: PlayerId, cost: &Cost) -> bool;
pub fn pay_cost(game: &mut Game, player: PlayerId, cost: &Cost,
                plan: &[ManaSourceChoice]) -> Result<(), ActionError>;
pub fn available_mana(game: &Game, player: PlayerId) -> ManaAvailability;

#[derive(Debug, Clone, Default)]
pub struct ManaAvailability {
    pub by_color: [u16; 5],
    pub any_color: u16,
    pub colorless: u16,
    pub total: u16,
}
```

`priority_actions` **sempre** inclui `Action::PassPriority` como primeiro item.
Enumera: jogar terreno, lançar mágicas pagáveis (com cada combinação de alvo, teto de 40
por mágica), ativar habilidades pagáveis. Habilidade de mana **não** entra na lista —
é usada implicitamente por `pay_cost`.

## `engine/combat.rs` — CR 506–511

```rust
use super::Game;
use crate::action::Action;
use crate::event::Defender;
use crate::ids::{ObjectId, PlayerId};

pub fn attack_options(game: &Game, player: PlayerId, eligible: &[ObjectId]) -> Vec<Action>;
pub fn block_options(game: &Game, player: PlayerId, eligible: &[ObjectId],
                     attackers: &[ObjectId]) -> Vec<Action>;
pub fn order_options(attacker: ObjectId, blockers: &[ObjectId]) -> Vec<Action>;
pub fn damage_assignment_options(game: &Game, attacker: ObjectId, blockers: &[ObjectId],
                                 total: i32) -> Vec<Action>;
pub fn eligible_attackers(game: &Game, player: PlayerId) -> Vec<ObjectId>;
pub fn eligible_blockers(game: &Game, player: PlayerId) -> Vec<ObjectId>;
pub fn can_block(game: &Game, blocker: ObjectId, attacker: ObjectId) -> bool;
pub fn declare_attackers(game: &mut Game, assignments: &[(ObjectId, Defender)]);
pub fn declare_blockers(game: &mut Game, assignments: &[(ObjectId, ObjectId)]);
pub fn combat_damage_step(game: &mut Game, first_strike: bool);
pub fn end_combat(game: &mut Game);
pub fn has_first_strike_creatures(game: &Game) -> bool;
```

`attack_options` limita a combinações razoáveis (teto ~60): tudo que pode atacar,
nenhum ataque, e subconjuntos gerados por heurística — nunca 2^n completo.

## `engine/resolve.rs` — interpretador do IR

```rust
use super::Game;
use super::query::EvalCtx;
use crate::action::Action;
use crate::ids::ObjectId;
use crate::ir::Effect;

pub fn resolve_effect(game: &mut Game, effect: &Effect, ctx: &mut EvalCtx);
pub fn mode_options(count: usize, choose: u8) -> Vec<Action>;
pub fn selection_options(candidates: &[ObjectId], min: u8, max: u8) -> Vec<Action>;
pub fn arrange_options(cards: &[ObjectId]) -> Vec<Action>;
```

`resolve_effect` cobre **todas** as variantes de `Effect`. Variante não implementada
registra no log (`game.state.push_log`) em vez de entrar em pânico.

## `engine/turn.rs` — CR 117, 500, 400.7

```rust
use super::{Game, GameConfig, PlayerConfig};
use crate::action::{Action, ActionError};
use crate::card::CardDatabase;
use crate::ids::{ObjectId, PlayerId};
use crate::state::GameState;
use crate::zone::ZoneId;

pub fn initial_state(db: &CardDatabase, players: &[PlayerConfig], config: &GameConfig)
    -> Result<GameState, ActionError>;
pub fn shuffle_all_libraries(game: &mut Game);
pub fn opening_hands(game: &mut Game);
pub fn run_game(game: &mut Game);
pub fn bottom_card_options(game: &Game, player: PlayerId, count: u8) -> Vec<Action>;
pub fn force_draw(game: &mut Game, reason: &str);

// utilidades usadas por todos os outros módulos
pub fn move_object(game: &mut Game, obj: ObjectId, to: ZoneId);
pub fn draw_card(game: &mut Game, player: PlayerId) -> Option<ObjectId>;
pub fn lose_game(game: &mut Game, player: PlayerId, reason: crate::event::LossReason);
pub fn give_priority(game: &mut Game);   // laço de prioridade até pilha vazia e todos passarem
```

`run_game` implementa o laço: turno → passos → prioridade → resolver pilha → SBA →
gatilhos → repetir. Respeita `config.max_turns` (empate) e `config.max_decisions`.
Mulligan de Londres: compra 7, se embaralhar, coloca N no fundo.

## `engine/viewgen.rs`

```rust
use super::Game;
use crate::view::{GameView, Observer};
pub fn build_view(game: &Game, observer: Observer) -> GameView;
```

Redação: mão só do observador (`Observer::can_see_hand`), biblioteca nunca
(exceto `Omniscient`). `CardView.power/toughness` vêm de `layers::characteristics`;
`base_power/base_toughness` de `base_characteristics`.

## Protocolo de rede (`mtg-server`)

WebSocket em `/ws/match`. Servidor envia frames JSON:

```jsonc
{ "type": "init",   "view": GameView, "players": ["Bot A", "Bot B"], "seed": 123 }
{ "type": "events", "events": [MatchEvent, ...], "view": GameView }
{ "type": "done",   "outcome": GameOutcome, "turns": 14, "durationMs": 812 }
```

Cliente envia:

```jsonc
{ "type": "start", "deckA": "burn", "deckB": "elves", "seed": 123, "speed": 1.0 }
{ "type": "pause" } | { "type": "resume" } | { "type": "step" }
```

REST: `GET /api/decks` lista decks disponíveis; `GET /api/cards` devolve o catálogo
(`CardDef[]`) para a UI renderizar texto e arte.
