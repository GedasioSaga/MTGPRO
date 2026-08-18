//! Pilha: colocar itens, resolver e cancelar (CR 601, 603.3, 608).
//!
//! Convenção de orientação: `state.stack.last()` é o **topo**. O último item
//! colocado é o primeiro a resolver. A zona física da pilha (`ZoneId::STACK`)
//! continua usando `Zone::push_top`, onde índice 0 é o topo — as duas visões
//! concordam, só diferem no sentido do vetor.
use std::sync::Arc;

use crate::action::{Action, Request, TargetChoice};
use crate::card::{Ability, CardDatabase};
use crate::event::GameEvent;
use crate::ids::{AbilityRef, ObjectId, PlayerId};
use crate::ir::{Effect, Keyword, TargetSpec};
use crate::state::{GameState, StackItem, StackItemKind, TriggerContext};
use crate::view::MatchEvent;
use crate::zone::{ZoneId, ZoneKind};

use super::query::{self, EvalCtx};
use super::{resolve, turn, Game};

/// Piso do espaço de ids sintéticos. Ids reais saem de `IdGen`, que começa em 0
/// e sobe uma unidade por objeto; nenhuma partida chega perto deste valor.
const SYNTHETIC_FLOOR: u32 = u32::MAX / 2;

/// Teto de permutações oferecidas para ordenar gatilhos simultâneos.
const MAX_TRIGGER_ORDERS: usize = 24;

// ---------------------------------------------------------------------------
// Ids de item de pilha
// ---------------------------------------------------------------------------

/// Id para um item de pilha que **não** é um objeto (habilidade ativada ou
/// disparada). Desce a partir de `ObjectId::NONE` em vez de consumir `IdGen`:
/// `state.objects` é indexado por posição, então queimar um id sem criar o
/// `ObjectState` correspondente desalinharia o vetor inteiro.
pub fn next_stack_id(state: &GameState) -> ObjectId {
    let lowest = state
        .stack
        .iter()
        .chain(state.pending_triggers.iter())
        .map(|item| item.id.0)
        .filter(|v| *v >= SYNTHETIC_FLOOR)
        .min()
        .unwrap_or(ObjectId::NONE.0);
    ObjectId(lowest.saturating_sub(1))
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

pub fn peek(game: &Game) -> Option<&StackItem> {
    game.state.stack.last()
}

fn owner_of(game: &Game, id: ObjectId, fallback: PlayerId) -> PlayerId {
    game.state.object(id).map(|o| o.owner).unwrap_or(fallback)
}

/// Especificações de alvo do item, buscadas na definição da carta.
fn target_specs<'a>(db: &'a CardDatabase, item: &StackItem) -> &'a [TargetSpec] {
    const NONE: &[TargetSpec] = &[];
    let card_id = match &item.kind {
        StackItemKind::CopiedSpell { source_card, .. } => *source_card,
        _ => item.card,
    };
    let Some(def) = db.get(card_id) else {
        return NONE;
    };
    match &item.kind {
        StackItemKind::Spell { .. } | StackItemKind::CopiedSpell { .. } => &def.spell_targets,
        StackItemKind::ActivatedAbility { index, .. } => {
            match def.abilities.get(*index as usize) {
                Some(Ability::Activated(a)) => &a.targets,
                _ => NONE,
            }
        }
        StackItemKind::TriggeredAbility { index, .. } => {
            match def.abilities.get(*index as usize) {
                Some(Ability::Triggered(t)) => &t.targets,
                _ => NONE,
            }
        }
    }
}

fn ctx_for(item: &StackItem) -> EvalCtx {
    EvalCtx {
        source: Some(item.kind.source()),
        controller: item.controller,
        targets: item.targets.clone(),
        x: item.x_value,
        trigger: item.trigger_ctx.clone(),
        remembered: item.remembered.clone(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Colocar na pilha
// ---------------------------------------------------------------------------

/// CR 601.2a — a carta vai para a pilha e a mágica passa a existir.
pub fn push_spell(
    game: &mut Game,
    object: ObjectId,
    controller: PlayerId,
    targets: Vec<TargetChoice>,
    x: u32,
    modes: Vec<u8>,
) {
    let Some(obj) = game.state.object(object) else {
        game.state
            .push_log(format!("mágica sem objeto: {object}"), Some(controller));
        return;
    };
    let card = obj.card;
    let already_on_stack = obj.zone.kind == ZoneKind::Stack;
    if !already_on_stack {
        turn::move_object(game, object, ZoneId::STACK);
    }
    if let Some(o) = game.state.object_mut(object) {
        o.controller = controller;
    }

    let target_objects: Vec<ObjectId> = targets
        .iter()
        .filter_map(|t| match t {
            TargetChoice::Object(o) => Some(*o),
            TargetChoice::Player(_) => None,
        })
        .collect();

    game.state.stack.push(StackItem {
        id: object,
        kind: StackItemKind::Spell { object },
        controller,
        card,
        targets,
        x_value: x,
        modes,
        trigger_ctx: TriggerContext::default(),
        remembered: Vec::new(),
        optional_confirmed: false,
    });
    game.state.player_mut(controller).spells_cast_this_turn += 1;
    // Qualquer coisa entrando na pilha reabre a rodada de prioridade (CR 117.3c).
    game.state.consecutive_passes = 0;

    let name = game.card_name(object);
    game.state
        .push_log(format!("{name} é lançada"), Some(controller));
    game.state.emit(GameEvent::SpellCast { object, controller });
    game.push_event(MatchEvent::SpellCast {
        card: object,
        player: controller,
        targets: target_objects,
    });
}

/// CR 602.2 — a habilidade ativada existe na pilha, independente da fonte.
pub fn push_activated(
    game: &mut Game,
    source: ObjectId,
    index: u16,
    controller: PlayerId,
    targets: Vec<TargetChoice>,
    x: u32,
) {
    let Some(obj) = game.state.object(source) else {
        game.state
            .push_log(format!("habilidade sem fonte: {source}"), Some(controller));
        return;
    };
    let card = obj.card;
    let id = next_stack_id(&game.state);

    game.state.stack.push(StackItem {
        id,
        kind: StackItemKind::ActivatedAbility { source, index },
        controller,
        card,
        targets,
        x_value: x,
        modes: Vec::new(),
        trigger_ctx: TriggerContext::default(),
        remembered: Vec::new(),
        optional_confirmed: false,
    });
    // Contabiliza o uso aqui, no ponto único onde a ativação vira realidade —
    // é isso que faz `uses_per_turn` valer alguma coisa.
    if let Some(o) = game.state.object_mut(source) {
        let n = o.ability_uses.entry(index).or_insert(0);
        *n = n.saturating_add(1);
    }
    game.state.consecutive_passes = 0;

    let name = game.card_name(source);
    let text = game
        .db
        .get(card)
        .and_then(|d| d.abilities.get(index as usize))
        .map(|a| a.text())
        .unwrap_or_default();
    game.state.push_log(
        format!("{name} ativa habilidade: {text}"),
        Some(controller),
    );
    game.state.emit(GameEvent::AbilityActivated {
        ability: AbilityRef { object: source, index },
        controller,
    });
    game.push_event(MatchEvent::Log {
        text: format!("{name}: {text}"),
    });
}

// ---------------------------------------------------------------------------
// Gatilhos → pilha (CR 603.3)
// ---------------------------------------------------------------------------

/// Ordem de turno a partir do jogador ativo (APNAP, CR 101.4).
fn apnap_order(state: &GameState) -> Vec<PlayerId> {
    let n = state.players.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n);
    let mut p = state.active_player;
    for _ in 0..n {
        out.push(p);
        p = state.next_player(p);
    }
    out
}

/// CR 603.3b — os gatilhos vão para a pilha em ordem APNAP; dentro de um mesmo
/// controlador, ele escolhe a ordem.
pub fn put_triggers_on_stack(game: &mut Game) {
    if game.state.pending_triggers.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut game.state.pending_triggers);
    let player_count = game.state.players.len();
    if player_count == 0 {
        return;
    }

    let mut by_player: Vec<Vec<StackItem>> = vec![Vec::new(); player_count];
    for item in pending {
        let idx = item.controller.index().min(player_count - 1);
        by_player[idx].push(item);
    }

    for player in apnap_order(&game.state) {
        let mut group = std::mem::take(&mut by_player[player.index().min(player_count - 1)]);
        if group.is_empty() {
            continue;
        }
        if group.len() >= 2 {
            let ids: Vec<ObjectId> = group.iter().map(|t| t.id).collect();
            let answer = game.ask(Request::OrderTriggers { player, triggers: ids });
            if let Action::OrderTriggers { order } = answer {
                group = reorder(group, &order);
            }
        }
        for item in group {
            place_trigger(game, item);
        }
    }
}

/// Reordena o grupo conforme a escolha do controlador; o que sobrar mantém a
/// ordem em que disparou.
fn reorder(group: Vec<StackItem>, order: &[ObjectId]) -> Vec<StackItem> {
    let mut rest = group;
    let mut out = Vec::with_capacity(rest.len());
    for id in order {
        if let Some(pos) = rest.iter().position(|t| t.id == *id) {
            out.push(rest.remove(pos));
        }
    }
    out.extend(rest);
    out
}

/// CR 603.3d — os alvos do gatilho são escolhidos ao colocá-lo na pilha. Sem
/// alvo legal, o gatilho é simplesmente removido.
fn place_trigger(game: &mut Game, mut item: StackItem) {
    let db = Arc::clone(&game.db);
    let specs: Vec<TargetSpec> = target_specs(&db, &item).to_vec();

    if !specs.is_empty() && item.targets.is_empty() {
        let mut ctx = ctx_for(&item);
        let mut chosen: Vec<TargetChoice> = Vec::with_capacity(specs.len());
        for spec in &specs {
            let options = query::legal_targets(game, spec, &ctx);
            if options.is_empty() {
                let name = game.card_name(item.kind.source());
                game.state.push_log(
                    format!(
                        "gatilho de {name} removido da pilha: sem alvo legal para \"{}\" (CR 603.3d)",
                        spec.description
                    ),
                    Some(item.controller),
                );
                return;
            }
            let pick = pick_target(game, item.controller, &spec.description, &options);
            chosen.push(pick);
            ctx.targets = chosen.clone();
        }
        item.targets = chosen;
    }

    game.state.stack.push(item);
    game.state.consecutive_passes = 0;
}

/// Escolha de um alvo entre os legais.
///
/// `Request` só sabe pedir seleção de **objetos**; quando um jogador aparece
/// entre os candidatos a escolha cai no primeiro legal e fica registrada, em
/// vez de mentir para o agente com uma lista incompleta.
fn pick_target(
    game: &mut Game,
    player: PlayerId,
    prompt: &str,
    options: &[TargetChoice],
) -> TargetChoice {
    let fallback = options
        .first()
        .copied()
        .unwrap_or(TargetChoice::Player(player));
    if options.len() <= 1 {
        return fallback;
    }

    let objects: Vec<ObjectId> = options
        .iter()
        .filter_map(|t| match t {
            TargetChoice::Object(o) => Some(*o),
            TargetChoice::Player(_) => None,
        })
        .collect();
    if objects.len() != options.len() {
        game.state.push_log(
            format!("alvo de gatilho \"{prompt}\" escolhido automaticamente"),
            Some(player),
        );
        return fallback;
    }

    let answer = game.ask(Request::SelectObjects {
        player,
        prompt: prompt.to_string(),
        candidates: objects.clone(),
        min: 1,
        max: 1,
    });
    if let Action::SelectObjects { objects: picked } = answer {
        if let Some(first) = picked.first() {
            if objects.contains(first) {
                return TargetChoice::Object(*first);
            }
        }
    }
    fallback
}

/// Permutações da ordem dos gatilhos, com teto — 5 gatilhos já dariam 120
/// opções, e nenhum bot ganha nada com essa cauda.
pub fn trigger_order_options(triggers: &[ObjectId]) -> Vec<Action> {
    let mut out = Vec::new();
    let mut used = vec![false; triggers.len()];
    let mut current = Vec::with_capacity(triggers.len());
    permute(triggers, &mut used, &mut current, &mut out);
    if out.is_empty() {
        out.push(Action::OrderTriggers { order: triggers.to_vec() });
    }
    out
}

fn permute(
    items: &[ObjectId],
    used: &mut [bool],
    current: &mut Vec<ObjectId>,
    out: &mut Vec<Action>,
) {
    if out.len() >= MAX_TRIGGER_ORDERS {
        return;
    }
    if current.len() == items.len() {
        out.push(Action::OrderTriggers { order: current.clone() });
        return;
    }
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        current.push(items[i]);
        permute(items, used, current, out);
        current.pop();
        used[i] = false;
        if out.len() >= MAX_TRIGGER_ORDERS {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Cancelar (CR 701.5)
// ---------------------------------------------------------------------------

pub fn counter_item(game: &mut Game, stack_id: ObjectId) {
    let Some(pos) = game.state.stack.iter().position(|it| it.id == stack_id) else {
        game.state
            .push_log(format!("nada a anular: {stack_id} não está na pilha"), None);
        return;
    };
    let item = game.state.stack.remove(pos);
    match item.kind {
        StackItemKind::Spell { object } => {
            let name = game.card_name(object);
            game.state
                .push_log(format!("{name} é anulada"), Some(item.controller));
            game.state.emit(GameEvent::SpellCountered { object });
            game.push_event(MatchEvent::SpellCountered { card: object });
            let owner = owner_of(game, object, item.controller);
            turn::move_object(game, object, ZoneId::graveyard(owner));
        }
        StackItemKind::CopiedSpell { original, .. } => {
            game.state.push_log(
                "cópia de mágica é anulada e deixa de existir (CR 707.10)",
                Some(item.controller),
            );
            game.state.emit(GameEvent::SpellCountered { object: original });
            game.push_event(MatchEvent::SpellCountered { card: original });
        }
        StackItemKind::ActivatedAbility { source, .. }
        | StackItemKind::TriggeredAbility { source, .. } => {
            let name = game.card_name(source);
            game.state.push_log(
                format!("habilidade de {name} é anulada"),
                Some(item.controller),
            );
            game.push_event(MatchEvent::Log {
                text: format!("habilidade de {name} anulada"),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Resolver (CR 608)
// ---------------------------------------------------------------------------

pub fn resolve_top(game: &mut Game) {
    let Some(item) = game.state.stack.pop() else {
        return;
    };
    game.state.consecutive_passes = 0;

    // `db` é um `Arc` próprio: as referências à definição da carta sobrevivem
    // às chamadas que pegam `&mut Game`.
    let db = Arc::clone(&game.db);
    let specs = target_specs(&db, &item);
    let mut ctx = ctx_for(&item);

    // CR 608.2b — alvo que ficou ilegal some da lista; se a mágica tinha alvo e
    // nenhum sobrou, ela não resolve.
    let mut kept: Vec<TargetChoice> = Vec::with_capacity(item.targets.len());
    for (i, target) in item.targets.iter().enumerate() {
        match specs.get(i) {
            Some(spec) if !query::target_still_legal(game, *target, spec, &ctx) => {
                let label = match target {
                    TargetChoice::Object(o) => game.card_name(*o),
                    TargetChoice::Player(p) => game.state.player(*p).name.clone(),
                };
                game.state.push_log(
                    format!("alvo {label} ficou ilegal e é removido (CR 608.2b)"),
                    Some(item.controller),
                );
            }
            _ => kept.push(*target),
        }
    }
    if !item.targets.is_empty() && kept.is_empty() {
        fizzle(game, &item);
        return;
    }
    ctx.targets = kept;

    match item.kind {
        StackItemKind::Spell { object } => resolve_spell(game, &item, object, &mut ctx, &db),
        StackItemKind::CopiedSpell { original, source_card } => {
            let effect = db.get(source_card).and_then(|d| d.spell_effect.clone());
            let permanent = db
                .get(source_card)
                .map(|d| d.type_line.is_permanent())
                .unwrap_or(false);
            if let Some(effect) = effect {
                let effect = apply_chosen_modes(&effect, &item.modes);
                resolve::resolve_effect(game, &effect, &mut ctx);
            }
            if permanent {
                // CR 707.10 — a cópia de mágica de permanente viraria ficha; a
                // criação de ficha por cópia ainda não é suportada.
                game.state.push_log(
                    "cópia de mágica de permanente resolve sem criar ficha (não suportado)",
                    Some(item.controller),
                );
            }
            game.push_event(MatchEvent::SpellResolved { card: original });
        }
        StackItemKind::ActivatedAbility { source, index } => {
            resolve_activated(game, &item, source, index, &mut ctx, &db);
        }
        StackItemKind::TriggeredAbility { source, index } => {
            resolve_triggered(game, &item, source, index, &mut ctx, &db);
        }
    }
}

/// CR 608.2b — a mágica que perdeu todos os alvos não resolve e vai para o
/// cemitério do dono; a habilidade simplesmente deixa de existir.
fn fizzle(game: &mut Game, item: &StackItem) {
    match item.kind {
        StackItemKind::Spell { object } => {
            let name = game.card_name(object);
            game.state.push_log(
                format!("{name} não resolve: todos os alvos ficaram ilegais (CR 608.2b)"),
                Some(item.controller),
            );
            game.push_event(MatchEvent::SpellCountered { card: object });
            let owner = owner_of(game, object, item.controller);
            turn::move_object(game, object, ZoneId::graveyard(owner));
        }
        StackItemKind::CopiedSpell { original, .. } => {
            game.state.push_log(
                "cópia de mágica não resolve: alvos ilegais (CR 608.2b)",
                Some(item.controller),
            );
            game.push_event(MatchEvent::SpellCountered { card: original });
        }
        StackItemKind::ActivatedAbility { source, .. }
        | StackItemKind::TriggeredAbility { source, .. } => {
            let name = game.card_name(source);
            game.state.push_log(
                format!("habilidade de {name} não resolve: alvos ilegais (CR 608.2b)"),
                Some(item.controller),
            );
            game.push_event(MatchEvent::Log {
                text: format!("habilidade de {name} não resolve (alvos ilegais)"),
            });
        }
    }
}

fn resolve_spell(
    game: &mut Game,
    item: &StackItem,
    object: ObjectId,
    ctx: &mut EvalCtx,
    db: &CardDatabase,
) {
    let Some(def) = db.get(item.card) else {
        game.state.push_log(
            format!("mágica {object} sem definição de carta; removida da pilha"),
            Some(item.controller),
        );
        let owner = owner_of(game, object, item.controller);
        turn::move_object(game, object, ZoneId::graveyard(owner));
        return;
    };
    let name = def.name.clone();
    let is_permanent = def.type_line.is_permanent();
    let is_aura = def.type_line.has_subtype("Aura")
        || def
            .keywords()
            .any(|k| matches!(k, Keyword::Enchant(_)));

    game.state.emit(GameEvent::SpellResolved { object });
    game.push_event(MatchEvent::SpellResolved { card: object });

    if is_permanent {
        // CR 608.3 — a permanente resolve indo para o campo de batalha; o
        // controlador da mágica passa a controlar o permanente.
        turn::move_object(game, object, ZoneId::BATTLEFIELD);
        if let Some(o) = game.state.object_mut(object) {
            o.controller = item.controller;
        }
        if is_aura {
            attach_aura(game, object, ctx);
        }
        game.state.push_log(
            format!("{name} resolve e entra no campo de batalha"),
            Some(item.controller),
        );
        if let Some(effect) = &def.spell_effect {
            let effect = apply_chosen_modes(effect, &item.modes);
            resolve::resolve_effect(game, &effect, ctx);
        }
        return;
    }

    // CR 608.2m — instantâneo/feitiço faz o que diz e vai para o cemitério.
    if let Some(effect) = &def.spell_effect {
        let effect = apply_chosen_modes(effect, &item.modes);
        resolve::resolve_effect(game, &effect, ctx);
    } else {
        game.state
            .push_log(format!("{name} não tem efeito definido"), Some(item.controller));
    }
    game.state
        .push_log(format!("{name} resolve"), Some(item.controller));
    let owner = owner_of(game, object, item.controller);
    turn::move_object(game, object, ZoneId::graveyard(owner));
}

/// CR 303.4f — a aura entra no campo já anexada ao que ela mirou.
fn attach_aura(game: &mut Game, aura: ObjectId, ctx: &EvalCtx) {
    let Some(TargetChoice::Object(host)) = ctx.targets.first().copied() else {
        return;
    };
    if !game
        .state
        .object(host)
        .is_some_and(|o| o.on_battlefield())
    {
        return;
    }
    if let Some(o) = game.state.object_mut(aura) {
        o.attached_to = Some(host);
    }
    if let Some(h) = game.state.object_mut(host) {
        if !h.attachments.contains(&aura) {
            h.attachments.push(aura);
        }
    }
    game.state.emit(GameEvent::Attached { equipment: aura, to: host });
    let (aura_name, host_name) = (game.card_name(aura), game.card_name(host));
    game.state
        .push_log(format!("{aura_name} encanta {host_name}"), None);
}

fn resolve_activated(
    game: &mut Game,
    item: &StackItem,
    source: ObjectId,
    index: u16,
    ctx: &mut EvalCtx,
    db: &CardDatabase,
) {
    let Some(Ability::Activated(ability)) = db
        .get(item.card)
        .and_then(|d| d.abilities.get(index as usize))
    else {
        game.state.push_log(
            format!("habilidade ativada {index} de {source} não existe mais"),
            Some(item.controller),
        );
        return;
    };
    let effect = apply_chosen_modes(&ability.effect, &item.modes);
    resolve::resolve_effect(game, &effect, ctx);
    let name = game.card_name(source);
    game.state.push_log(
        format!("habilidade de {name} resolve: {}", ability.text),
        Some(item.controller),
    );
}

fn resolve_triggered(
    game: &mut Game,
    item: &StackItem,
    source: ObjectId,
    index: u16,
    ctx: &mut EvalCtx,
    db: &CardDatabase,
) {
    let Some(Ability::Triggered(ability)) = db
        .get(item.card)
        .and_then(|d| d.abilities.get(index as usize))
    else {
        game.state.push_log(
            format!("habilidade disparada {index} de {source} não existe mais"),
            Some(item.controller),
        );
        return;
    };
    let name = game.card_name(source);

    // CR 603.4 — a condição de intervenção é checada de novo na resolução.
    if !query::eval_condition(game, &ability.intervening_if, ctx) {
        game.state.push_log(
            format!("gatilho de {name} não resolve: condição de intervenção falsa (CR 603.4)"),
            Some(item.controller),
        );
        return;
    }

    // "Você pode..." (CR 603.3c) é decidido na resolução, não no disparo.
    if ability.optional && !item.optional_confirmed {
        let answer = game.ask(Request::ConfirmOptional {
            player: item.controller,
            prompt: ability.text.clone(),
        });
        if !matches!(answer, Action::Confirm { yes: true }) {
            game.state.push_log(
                format!("controlador declina o gatilho opcional de {name}"),
                Some(item.controller),
            );
            return;
        }
    }

    let effect = apply_chosen_modes(&ability.effect, &item.modes);
    resolve::resolve_effect(game, &effect, ctx);
    game.state.push_log(
        format!("gatilho de {name} resolve: {}", ability.text),
        Some(item.controller),
    );
}

/// CR 601.2b — os modos são escolhidos ao lançar, não ao resolver. Quando a
/// escolha já veio no item, o `Modal` vira a sequência das opções escolhidas;
/// se não veio, o efeito segue intacto e `resolve` pergunta.
fn apply_chosen_modes(effect: &Effect, modes: &[u8]) -> Effect {
    let Effect::Modal { choose, options } = effect else {
        return effect.clone();
    };
    if modes.is_empty() {
        return effect.clone();
    }
    let mut picked: Vec<u8> = modes.to_vec();
    picked.sort_unstable();
    picked.dedup();
    picked.truncate((*choose).max(1) as usize);
    let chosen: Vec<Effect> = picked
        .iter()
        .filter_map(|i| options.get(*i as usize))
        .map(|(_, e)| e.clone())
        .collect();
    if chosen.is_empty() {
        effect.clone()
    } else {
        Effect::Sequence(chosen)
    }
}
