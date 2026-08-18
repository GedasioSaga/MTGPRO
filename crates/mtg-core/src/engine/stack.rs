//! A pilha: entrada de itens, ordenação de gatilhos e resolução
//! (CR 601, 603.3, 608).
//!
//! `state.stack` é um vetor em que o **último** elemento é o topo — é a ordem
//! que `viewgen` inverte para desenhar. Só este módulo empilha e desempilha.
use std::sync::Arc;

use super::query::EvalCtx;
use super::{layers, query, resolve, turn, Game};
use crate::action::{Action, Request, TargetChoice};
use crate::card::{Ability, CardDatabase, CardDef, TriggeredAbility};
use crate::event::GameEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::TargetSpec;
use crate::state::{StackItem, StackItemKind, TriggerContext};
use crate::view::MatchEvent;
use crate::zone::ZoneId;

/// Teto de permutações oferecidas para ordenar gatilhos simultâneos. 24 = 4!,
/// que já cobre a esmagadora maioria das situações reais; acima disso o custo
/// de enumerar cresce como n! sem ganho de decisão.
const MAX_TRIGGER_ORDERS: usize = 24;

// ---------------------------------------------------------------------------
// Entrada na pilha
// ---------------------------------------------------------------------------

/// Ids de item de pilha que não são objetos (habilidades) saem do topo do
/// espaço de ids, e não do `IdGen`: `state.objects` é indexado por `ObjectId.0`,
/// então gastar um id do gerador sem empurrar um `ObjectState` abriria um
/// buraco no vetor e desalinharia todo objeto criado depois.
pub(crate) fn reserve_stack_id(game: &Game) -> ObjectId {
    let taken = |id: ObjectId| {
        game.state.stack.iter().any(|i| i.id == id)
            || game.state.pending_triggers.iter().any(|i| i.id == id)
    };
    // `ObjectId::NONE` é u32::MAX e está reservado para "nenhum objeto".
    let mut candidate = u32::MAX - 1;
    while candidate > 0 && taken(ObjectId(candidate)) {
        candidate -= 1;
    }
    ObjectId(candidate)
}

/// CR 601.2a — a carta já está na zona da pilha; aqui ela ganha o item que
/// carrega alvos, X e modos escolhidos no lançamento.
pub fn push_spell(
    game: &mut Game,
    object: ObjectId,
    controller: PlayerId,
    targets: Vec<TargetChoice>,
    x: u32,
    modes: Vec<u8>,
) {
    let Some(card) = game.state.object(object).map(|o| o.card) else {
        game.state.push_log(
            format!("{object} não existe: mágica não entra na pilha"),
            Some(controller),
        );
        return;
    };
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
        // Mágica não tem escolha de "você pode" na entrada da pilha.
        optional_confirmed: true,
    });
}

/// CR 602.2a — a habilidade ativada existe só na pilha, independente da fonte.
pub fn push_activated(
    game: &mut Game,
    source: ObjectId,
    index: u16,
    controller: PlayerId,
    targets: Vec<TargetChoice>,
    x: u32,
) {
    let Some(card) = game.state.object(source).map(|o| o.card) else {
        game.state.push_log(
            format!("{source} não existe: habilidade não entra na pilha"),
            Some(controller),
        );
        return;
    };
    let id = reserve_stack_id(game);
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
        optional_confirmed: true,
    });
}

/// Topo da pilha sem removê-lo.
pub fn peek(game: &Game) -> Option<&StackItem> {
    game.state.stack.last()
}

// ---------------------------------------------------------------------------
// Gatilhos esperando (CR 603.3)
// ---------------------------------------------------------------------------

/// CR 603.3b — gatilhos que dispararam vão para a pilha na próxima vez que um
/// jogador receberia prioridade, em ordem APNAP: o jogador ativo põe os dele
/// primeiro, depois cada não-ativo na ordem de turno.
///
/// Devolve `true` se algum item chegou de fato à pilha. Quem chama usa isso para
/// reiniciar a rodada de prioridade: com três ou quatro jogadores, alguém que já
/// tinha passado precisa poder responder ao gatilho novo (CR 117.4).
pub fn put_triggers_on_stack(game: &mut Game) -> bool {
    if game.state.pending_triggers.is_empty() {
        return false;
    }
    let pending = std::mem::take(&mut game.state.pending_triggers);
    let order = apnap_order(game);
    let mut matched = 0usize;
    let mut placed = 0usize;

    for player in order.iter().copied() {
        let group: Vec<StackItem> = pending
            .iter()
            .filter(|i| i.controller == player)
            .cloned()
            .collect();
        if group.is_empty() {
            continue;
        }
        matched += group.len();
        // CR 603.3b — com dois ou mais do mesmo controlador, ele escolhe a
        // ordem em que entram (o último a entrar resolve primeiro).
        let ordered = if group.len() > 1 {
            ask_trigger_order(game, player, group)
        } else {
            group
        };
        for item in ordered {
            if let Some(ready) = prepare_trigger(game, item) {
                push_trigger_item(game, ready);
                placed += 1;
            }
        }
    }

    // CR 800.4a — gatilho de quem já deixou a partida deixa de existir; ele não
    // pode sumir sem registro, ou o log mente sobre o que aconteceu.
    if matched < pending.len() {
        let orphans: Vec<PlayerId> = pending
            .iter()
            .filter(|i| !order.contains(&i.controller))
            .map(|i| i.controller)
            .collect();
        for controller in orphans {
            game.state.push_log(
                format!("gatilho descartado: controlador {controller} fora da partida"),
                Some(controller),
            );
        }
    }

    placed > 0
}

/// Jogadores ainda na partida a partir do ativo, na ordem de turno (CR 101.4).
///
/// É a ordem canônica de toda escolha simultânea — gatilho indo para a pilha,
/// mas também qualquer decisão que vários jogadores fariam ao mesmo tempo.
pub fn apnap_order(game: &Game) -> Vec<PlayerId> {
    let mut out = Vec::with_capacity(game.state.players.len());
    let mut current = game.state.active_player;
    for _ in 0..game.state.players.len() {
        if let Some(p) = game.state.players.get(current.index()) {
            if !p.has_lost {
                out.push(current);
            }
        }
        current = game.state.next_player(current);
    }
    out
}

fn ask_trigger_order(game: &mut Game, player: PlayerId, group: Vec<StackItem>) -> Vec<StackItem> {
    let ids: Vec<ObjectId> = group.iter().map(|i| i.id).collect();
    let action = game.ask(Request::OrderTriggers {
        player,
        triggers: ids.clone(),
    });
    let Action::OrderTriggers { order } = action else {
        return group;
    };
    if !is_permutation_of(&order, &ids) {
        game.state
            .push_log("ordem de gatilhos inválida: mantida a ordem de disparo", Some(player));
        return group;
    }
    let mut out = Vec::with_capacity(group.len());
    for id in &order {
        if let Some(item) = group.iter().find(|i| i.id == *id) {
            out.push(item.clone());
        }
    }
    if out.len() == group.len() {
        out
    } else {
        group
    }
}

fn is_permutation_of(candidate: &[ObjectId], reference: &[ObjectId]) -> bool {
    if candidate.len() != reference.len() {
        return false;
    }
    let mut sorted_a = candidate.to_vec();
    let mut sorted_b = reference.to_vec();
    sorted_a.sort_unstable();
    sorted_b.sort_unstable();
    sorted_a == sorted_b
}

/// CR 603.3d — alvos de um gatilho são escolhidos ao colocá-lo na pilha; sem
/// alvo legal, o gatilho é simplesmente removido.
fn prepare_trigger(game: &mut Game, mut item: StackItem) -> Option<StackItem> {
    let db = Arc::clone(&game.db);
    let specs = target_specs(&db, &item);
    if specs.is_empty() || !item.targets.is_empty() {
        return Some(item);
    }
    let mut chosen: Vec<TargetChoice> = Vec::with_capacity(specs.len());
    for spec in &specs {
        let ctx = EvalCtx {
            source: Some(item.kind.source()),
            controller: item.controller,
            targets: chosen.clone(),
            x: item.x_value,
            trigger: item.trigger_ctx.clone(),
            ..Default::default()
        };
        let candidates = query::legal_targets(game, spec, &ctx);
        match pick_target(game, item.controller, spec, &candidates) {
            Some(t) => chosen.push(t),
            None => {
                game.state.push_log(
                    format!("gatilho removido: sem alvo legal para {}", spec.description),
                    Some(item.controller),
                );
                return None;
            }
        }
    }
    item.targets = chosen;
    Some(item)
}

/// Escolhe um alvo entre os candidatos. `Request` não tem variante para
/// "escolha um jogador como alvo", então candidato-jogador é resolvido de forma
/// determinística; entre objetos o controlador decide.
fn pick_target(
    game: &mut Game,
    player: PlayerId,
    spec: &TargetSpec,
    candidates: &[TargetChoice],
) -> Option<TargetChoice> {
    match candidates.len() {
        0 => None,
        1 => candidates.first().copied(),
        _ => {
            let objects: Vec<ObjectId> = candidates
                .iter()
                .filter_map(|c| match c {
                    TargetChoice::Object(o) => Some(*o),
                    TargetChoice::Player(_) => None,
                })
                .collect();
            if objects.len() >= 2 {
                let action = game.ask(Request::SelectObjects {
                    player,
                    prompt: spec.description.clone(),
                    candidates: objects.clone(),
                    min: 1,
                    max: 1,
                });
                if let Action::SelectObjects { objects: picked } = action {
                    if let Some(first) = picked.first() {
                        if objects.contains(first) {
                            return Some(TargetChoice::Object(*first));
                        }
                    }
                }
            }
            candidates.first().copied()
        }
    }
}

fn push_trigger_item(game: &mut Game, item: StackItem) {
    let name = game
        .db
        .get(item.card)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "?".to_string());
    game.state
        .push_log(format!("gatilho de {name} vai para a pilha"), Some(item.controller));
    game.state.stack.push(item);
}

/// Permutações deterministicas dos gatilhos, com teto. A ordem base é a de
/// disparo, e as permutações saem em ordem lexicográfica de índice.
pub fn trigger_order_options(triggers: &[ObjectId]) -> Vec<Action> {
    if triggers.is_empty() {
        return Vec::new();
    }
    let mut indices: Vec<usize> = (0..triggers.len()).collect();
    let mut out = Vec::new();
    loop {
        let order: Vec<ObjectId> = indices.iter().filter_map(|i| triggers.get(*i).copied()).collect();
        out.push(Action::OrderTriggers { order });
        if out.len() >= MAX_TRIGGER_ORDERS || !next_permutation(&mut indices) {
            break;
        }
    }
    out
}

/// Próxima permutação em ordem lexicográfica; `false` quando já é a última.
fn next_permutation(idx: &mut [usize]) -> bool {
    if idx.len() < 2 {
        return false;
    }
    let mut i = idx.len() - 1;
    while i > 0 && idx[i - 1] >= idx[i] {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    let pivot = i - 1;
    let mut j = idx.len() - 1;
    while idx[j] <= idx[pivot] {
        j -= 1;
    }
    idx.swap(pivot, j);
    idx[i..].reverse();
    true
}

// ---------------------------------------------------------------------------
// Resolução (CR 608)
// ---------------------------------------------------------------------------

/// Resolve o item do topo. Coração do motor: revalida alvos, roda o efeito e
/// põe o objeto na zona certa.
pub fn resolve_top(game: &mut Game) {
    let Some(item) = game.state.stack.pop() else {
        return;
    };
    let db = Arc::clone(&game.db);
    let specs = target_specs(&db, &item);

    let mut ctx = EvalCtx {
        source: Some(item.kind.source()),
        controller: item.controller,
        targets: item.targets.clone(),
        x: item.x_value,
        trigger: item.trigger_ctx.clone(),
        remembered: item.remembered.clone(),
        ..Default::default()
    };

    // CR 608.2b — alvo que ficou ilegal é ignorado; se a mágica exigia alvo e
    // nenhum sobrou legal, ela não resolve ("fizzle").
    let mut any_legal = specs.is_empty();
    let mut effective = item.targets.clone();
    for (i, spec) in specs.iter().enumerate() {
        let Some(target) = effective.get(i).copied() else {
            continue;
        };
        if query::target_still_legal(game, target, spec, &ctx) {
            any_legal = true;
        } else {
            // Posição preservada: `ObjRef::Target(i)` indexa por posição, então
            // remover encolheria a lista e apontaria para o alvo errado.
            effective[i] = TargetChoice::Object(ObjectId::NONE);
        }
    }
    ctx.targets = effective;

    if !any_legal {
        fizzle(game, &item);
        return;
    }

    match item.kind.clone() {
        StackItemKind::Spell { object } => resolve_spell(game, &item, object, &mut ctx),
        StackItemKind::CopiedSpell { source_card, .. } => {
            resolve_copy(game, &item, source_card, &mut ctx)
        }
        StackItemKind::ActivatedAbility { source, index } => {
            resolve_activated(game, &item, source, index, &mut ctx)
        }
        StackItemKind::TriggeredAbility { source, index } => {
            resolve_triggered(game, &item, source, index, &mut ctx)
        }
    }
}

fn resolve_spell(game: &mut Game, item: &StackItem, object: ObjectId, ctx: &mut EvalCtx) {
    let db = Arc::clone(&game.db);
    let Some(def) = db.get(item.card) else {
        game.state.push_log(
            format!("{object}: mágica sem definição, removida da pilha"),
            Some(item.controller),
        );
        return;
    };
    let name = def.name.clone();
    let effect = def.spell_effect.clone();

    if is_permanent_spell(game, object, def) {
        // CR 608.3 — mágica de permanente resolve entrando no campo de batalha.
        turn::move_object(game, object, ZoneId::BATTLEFIELD);
        if let Some(e) = effect {
            resolve::resolve_effect(game, &e, ctx);
        }
    } else {
        if let Some(e) = effect {
            resolve::resolve_effect(game, &e, ctx);
        }
        // CR 608.2m — instantâneo e feitiço vão ao cemitério do dono.
        let owner = game
            .state
            .object(object)
            .map(|o| o.owner)
            .unwrap_or(item.controller);
        turn::move_object(game, object, ZoneId::graveyard(owner));
    }

    game.state.emit(GameEvent::SpellResolved { object });
    game.push_event(MatchEvent::SpellResolved { card: object });
    game.state
        .push_log(format!("{name} resolve"), Some(item.controller));
}

/// CR 707.10 — a cópia resolve como a original e depois deixa de existir; ela
/// nunca esteve em zona alguma, então não há movimento a fazer.
fn resolve_copy(
    game: &mut Game,
    item: &StackItem,
    source_card: crate::ids::CardDefId,
    ctx: &mut EvalCtx,
) {
    let db = Arc::clone(&game.db);
    let Some(def) = db.get(source_card) else {
        game.state
            .push_log("cópia sem definição: removida da pilha", Some(item.controller));
        return;
    };
    let name = def.name.clone();
    if let Some(effect) = def.spell_effect.clone() {
        resolve::resolve_effect(game, &effect, ctx);
    }
    game.state
        .push_log(format!("cópia de {name} resolve"), Some(item.controller));
}

fn resolve_activated(
    game: &mut Game,
    item: &StackItem,
    source: ObjectId,
    index: u16,
    ctx: &mut EvalCtx,
) {
    let db = Arc::clone(&game.db);
    let ability = match db.get(item.card).and_then(|d| d.abilities.get(index as usize)) {
        Some(Ability::Activated(a)) => a,
        _ => {
            game.state.push_log(
                format!("habilidade {index} de {source} não existe mais: nada resolve"),
                Some(item.controller),
            );
            return;
        }
    };
    let effect = ability.effect.clone();
    let text = ability.text.clone();
    resolve::resolve_effect(game, &effect, ctx);
    game.state
        .push_log(format!("resolve habilidade: {text}"), Some(item.controller));
}

fn resolve_triggered(
    game: &mut Game,
    item: &StackItem,
    source: ObjectId,
    index: u16,
    ctx: &mut EvalCtx,
) {
    let db = Arc::clone(&game.db);
    let Some(ability) = triggered_ability(&db, item, index) else {
        game.state.push_log(
            format!("gatilho {index} de {source} não existe mais: nada resolve"),
            Some(item.controller),
        );
        return;
    };

    // CR 603.4 — a condição de intervenção é checada de novo na resolução; se
    // falhar, a habilidade é removida da pilha sem fazer nada.
    if !query::eval_condition(game, &ability.intervening_if, ctx) {
        let text = ability.text.clone();
        game.state.push_log(
            format!("gatilho não resolve, condição falhou: {text}"),
            Some(item.controller),
        );
        return;
    }

    // CR 603.5 — a escolha de "você pode" acontece na resolução.
    if ability.optional && !item.optional_confirmed {
        let prompt = ability.text.clone();
        let action = game.ask(Request::ConfirmOptional {
            player: item.controller,
            prompt,
        });
        if !matches!(action, Action::Confirm { yes: true }) {
            game.state
                .push_log("gatilho opcional recusado", Some(item.controller));
            return;
        }
    }

    let effect = ability.effect.clone();
    let text = ability.text.clone();
    resolve::resolve_effect(game, &effect, ctx);
    game.state
        .push_log(format!("resolve gatilho: {text}"), Some(item.controller));
}

/// CR 608.2b — mágica sem alvo legal não resolve e vai direto ao cemitério.
fn fizzle(game: &mut Game, item: &StackItem) {
    let name = game
        .db
        .get(item.card)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "?".to_string());
    match item.kind {
        StackItemKind::Spell { object } => {
            game.state.push_log(
                format!("{name} não resolve: todos os alvos ficaram ilegais"),
                Some(item.controller),
            );
            game.state.emit(GameEvent::SpellCountered { object });
            game.push_event(MatchEvent::SpellCountered { card: object });
            let owner = game
                .state
                .object(object)
                .map(|o| o.owner)
                .unwrap_or(item.controller);
            turn::move_object(game, object, ZoneId::graveyard(owner));
        }
        _ => {
            game.state.push_log(
                format!("habilidade de {name} não resolve: alvos ilegais"),
                Some(item.controller),
            );
            game.push_event(MatchEvent::Log {
                text: format!("{name}: habilidade removida por falta de alvo legal"),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Contra-magia
// ---------------------------------------------------------------------------

/// CR 701.5 — remove um item da pilha. Mágica anulada vai ao cemitério do dono;
/// habilidade anulada simplesmente deixa de existir.
pub fn counter_item(game: &mut Game, stack_id: ObjectId) {
    let Some(pos) = game.state.stack.iter().position(|i| i.id == stack_id) else {
        game.state
            .push_log(format!("{stack_id} não está na pilha: nada a anular"), None);
        return;
    };
    let item = game.state.stack.remove(pos);
    let name = game
        .db
        .get(item.card)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "?".to_string());

    match item.kind {
        StackItemKind::Spell { object } => {
            game.state
                .push_log(format!("{name} foi anulada"), Some(item.controller));
            game.state.emit(GameEvent::SpellCountered { object });
            game.push_event(MatchEvent::SpellCountered { card: object });
            let owner = game
                .state
                .object(object)
                .map(|o| o.owner)
                .unwrap_or(item.controller);
            turn::move_object(game, object, ZoneId::graveyard(owner));
        }
        // CR 707.10 — cópia anulada some sem tocar em zona nenhuma.
        StackItemKind::CopiedSpell { .. } => {
            game.state
                .push_log(format!("cópia de {name} foi anulada"), Some(item.controller));
        }
        _ => {
            game.state.push_log(
                format!("habilidade de {name} foi anulada"),
                Some(item.controller),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------

/// Especificações de alvo do item, na mesma ordem em que os alvos foram
/// escolhidos.
fn target_specs(db: &CardDatabase, item: &StackItem) -> Vec<TargetSpec> {
    let card = match &item.kind {
        StackItemKind::CopiedSpell { source_card, .. } => *source_card,
        _ => item.card,
    };
    let Some(def) = db.get(card) else {
        return Vec::new();
    };
    match &item.kind {
        StackItemKind::Spell { .. } | StackItemKind::CopiedSpell { .. } => def.spell_targets.clone(),
        StackItemKind::ActivatedAbility { index, .. } => {
            match def.abilities.get(*index as usize) {
                Some(Ability::Activated(a)) => a.targets.clone(),
                _ => Vec::new(),
            }
        }
        StackItemKind::TriggeredAbility { index, .. } => {
            match def.abilities.get(*index as usize) {
                Some(Ability::Triggered(a)) => a.targets.clone(),
                _ => Vec::new(),
            }
        }
    }
}

fn triggered_ability<'a>(
    db: &'a CardDatabase,
    item: &StackItem,
    index: u16,
) -> Option<&'a TriggeredAbility> {
    match db.get(item.card)?.abilities.get(index as usize) {
        Some(Ability::Triggered(a)) => Some(a),
        _ => None,
    }
}

/// Tipo de permanente decide o destino da mágica ao resolver. A fonte de
/// verdade é `layers`; o tipo impresso só entra quando as camadas não têm o que
/// dizer sobre o objeto (por exemplo, objeto já removido).
fn is_permanent_spell(game: &Game, object: ObjectId, def: &CardDef) -> bool {
    match layers::characteristics(game, object) {
        Some(ch) => ch.type_line.is_permanent(),
        None => def.type_line.is_permanent(),
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod testkit {
    //! Fábrica mínima de partida para os testes dos módulos deste agente.
    use std::sync::Arc;

    use super::*;
    use crate::card::{CardDatabase, CardDef};
    use crate::engine::{FirstLegalAgent, GameConfig, PlayerConfig};
    use crate::ids::CardDefId;
    use crate::mana::ManaCost;
    use crate::types::{CardType, Rarity, TypeLine};

    pub fn card(id: u32, name: &str, types: Vec<CardType>) -> CardDef {
        CardDef {
            id: CardDefId(id),
            name: name.to_string(),
            mana_cost: ManaCost::FREE,
            type_line: TypeLine {
                supertypes: Vec::new(),
                types,
                subtypes: Vec::new(),
            },
            color_override: None,
            power: Some(1),
            toughness: Some(1),
            loyalty: None,
            abilities: Vec::new(),
            spell_effect: None,
            spell_targets: Vec::new(),
            oracle_text: String::new(),
            flavor_text: None,
            rarity: Rarity::Common,
            set_code: "TST".to_string(),
            collector_number: String::new(),
            artist: None,
            art_key: None,
        }
    }

    /// Partida de duas pessoas com deck de 12 cópias da primeira carta e
    /// mulligan desligado — nenhuma decisão acontece na montagem.
    pub fn game_with(cards: Vec<CardDef>) -> Game {
        let mut db = CardDatabase { cards };
        db.reindex();
        let deck: Vec<CardDefId> = (0..12).map(|_| CardDefId(0)).collect();
        let players = vec![
            PlayerConfig { name: "A".into(), deck: deck.clone(), commander: None },
            PlayerConfig { name: "B".into(), deck, commander: None },
        ];
        let agents: Vec<Box<dyn crate::engine::Agent>> =
            vec![Box::new(FirstLegalAgent), Box::new(FirstLegalAgent)];
        let config = GameConfig { allow_mulligan: false, ..Default::default() };
        match Game::new(Arc::new(db), players, agents, config, 42) {
            Ok(g) => g,
            Err(e) => panic!("montagem de partida de teste falhou: {e}"),
        }
    }

    /// Partida de `count` jogadores (nomes `P0`..`Pn`), todos com o mesmo deck e
    /// com o agente que sempre escolhe a primeira ação legal.
    pub fn game_with_players(cards: Vec<CardDef>, count: usize) -> Game {
        let agents: Vec<Box<dyn crate::engine::Agent>> = (0..count)
            .map(|_| Box::new(FirstLegalAgent) as Box<dyn crate::engine::Agent>)
            .collect();
        game_with_agents(cards, agents)
    }

    /// Igual a `game_with_players`, mas com agentes escolhidos pelo teste — a
    /// quantidade de agentes define a quantidade de jogadores.
    pub fn game_with_agents(
        cards: Vec<CardDef>,
        agents: Vec<Box<dyn crate::engine::Agent>>,
    ) -> Game {
        let mut db = CardDatabase { cards };
        db.reindex();
        let deck: Vec<CardDefId> = (0..12).map(|_| CardDefId(0)).collect();
        let players: Vec<PlayerConfig> = (0..agents.len())
            .map(|i| PlayerConfig { name: format!("P{i}"), deck: deck.clone(), commander: None })
            .collect();
        let config = GameConfig { allow_mulligan: false, ..Default::default() };
        match Game::new(Arc::new(db), players, agents, config, 42) {
            Ok(g) => g,
            Err(e) => panic!("montagem de partida de teste falhou: {e}"),
        }
    }

    /// Coloca um objeto qualquer no campo de batalha, devolvendo seu id.
    pub fn put_on_battlefield(game: &mut Game, owner: PlayerId) -> ObjectId {
        let id = turn::zone_objects(game, ZoneId::library(owner))
            .first()
            .copied()
            .expect("biblioteca de teste vazia");
        turn::move_object(game, id, ZoneId::BATTLEFIELD);
        game.state.event_queue.clear();
        id
    }

    pub fn trigger_item(id: ObjectId, source: ObjectId, controller: PlayerId) -> StackItem {
        StackItem {
            id,
            kind: StackItemKind::TriggeredAbility { source, index: 0 },
            controller,
            card: CardDefId(0),
            targets: Vec::new(),
            x_value: 0,
            modes: Vec::new(),
            trigger_ctx: TriggerContext::default(),
            remembered: Vec::new(),
            optional_confirmed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::ir::{Selector, TargetKind};
    use crate::types::CardType;

    fn spell_card_with_target() -> Vec<CardDef> {
        let mut land = card(0, "Terreno de Teste", vec![CardType::Land]);
        land.power = None;
        land.toughness = None;
        let mut bolt = card(1, "Raio de Teste", vec![CardType::Instant]);
        bolt.power = None;
        bolt.toughness = None;
        bolt.spell_targets = vec![TargetSpec {
            kind: TargetKind::Object(Selector::creatures()),
            description: "alvo de criatura".to_string(),
        }];
        vec![land, bolt]
    }

    #[test]
    fn magica_sem_alvo_legal_nao_resolve_e_vai_ao_cemiterio() {
        let mut game = game_with(spell_card_with_target());
        let owner = PlayerId::P0;
        let spell = put_on_battlefield(&mut game, owner);
        // Coloca a carta na pilha como se tivesse sido lançada com um alvo que
        // já não existe — nenhum objeto tem o id NONE.
        turn::move_object(&mut game, spell, ZoneId::STACK);
        if let Some(o) = game.state.object_mut(spell) {
            o.card = crate::ids::CardDefId(1);
        }
        push_spell(
            &mut game,
            spell,
            owner,
            vec![TargetChoice::Object(ObjectId::NONE)],
            0,
            Vec::new(),
        );
        assert_eq!(game.state.stack.len(), 1);

        resolve_top(&mut game);

        assert!(game.state.stack.is_empty(), "a pilha deveria esvaziar");
        let graveyard = turn::zone_objects(&game, ZoneId::graveyard(owner));
        assert!(graveyard.contains(&spell), "mágica sem alvo vai ao cemitério");
        assert!(
            game.state
                .event_queue
                .iter()
                .any(|e| matches!(e, GameEvent::SpellCountered { .. })),
            "fizzle precisa emitir SpellCountered"
        );
        assert!(
            !game
                .state
                .event_queue
                .iter()
                .any(|e| matches!(e, GameEvent::SpellResolved { .. })),
            "mágica que não resolve não pode emitir SpellResolved"
        );
    }

    #[test]
    fn gatilhos_entram_na_pilha_em_ordem_apnap() {
        let mut game = game_with(vec![card(0, "Peça", vec![CardType::Creature])]);
        game.state.active_player = PlayerId::P1;
        let source_a = put_on_battlefield(&mut game, PlayerId::P0);
        let source_b = put_on_battlefield(&mut game, PlayerId::P1);

        // Ordem de disparo propositalmente ao contrário da APNAP.
        game.state.pending_triggers = vec![
            trigger_item(ObjectId(9000), source_a, PlayerId::P0),
            trigger_item(ObjectId(9001), source_b, PlayerId::P1),
        ];

        put_triggers_on_stack(&mut game);

        let ids: Vec<ObjectId> = game.state.stack.iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            vec![ObjectId(9001), ObjectId(9000)],
            "o jogador ativo (P1) põe o gatilho dele primeiro"
        );
        // Último a entrar é o topo, logo resolve antes.
        assert_eq!(peek(&game).map(|i| i.id), Some(ObjectId(9000)));
    }

    #[test]
    fn apnap_com_quatro_jogadores_ordena_gatilhos() {
        let mut game = game_with_players(vec![card(0, "Peça", vec![CardType::Creature])], 4);
        // Ativo no meio da mesa: a ordem correta dá a volta e não é a ordem dos
        // ids nem a ordem de disparo.
        game.state.active_player = PlayerId(2);
        let sources: Vec<ObjectId> = (0..4)
            .map(|i| put_on_battlefield(&mut game, PlayerId(i)))
            .collect();

        // Ordem de disparo propositalmente embaralhada.
        game.state.pending_triggers = vec![
            trigger_item(ObjectId(9000), sources[1], PlayerId(1)),
            trigger_item(ObjectId(9001), sources[3], PlayerId(3)),
            trigger_item(ObjectId(9002), sources[0], PlayerId(0)),
            trigger_item(ObjectId(9003), sources[2], PlayerId(2)),
        ];

        assert!(
            put_triggers_on_stack(&mut game),
            "os quatro gatilhos tinham que entrar na pilha"
        );

        let controllers: Vec<PlayerId> = game.state.stack.iter().map(|i| i.controller).collect();
        assert_eq!(
            controllers,
            vec![PlayerId(2), PlayerId(3), PlayerId(0), PlayerId(1)],
            "CR 101.4: ativo primeiro, depois os não-ativos em ordem de turno"
        );
        let Some(top) = peek(&game) else {
            panic!("a pilha ficou vazia depois de colocar quatro gatilhos");
        };
        assert_eq!(
            top.controller,
            PlayerId(1),
            "o último a entrar é o topo, logo resolve primeiro"
        );
    }

    #[test]
    fn opcoes_de_ordem_de_gatilho_sao_permutacoes_limitadas() {
        let two = trigger_order_options(&[ObjectId(1), ObjectId(2)]);
        assert_eq!(two.len(), 2);
        assert_eq!(
            two[0],
            Action::OrderTriggers { order: vec![ObjectId(1), ObjectId(2)] }
        );
        assert_eq!(
            two[1],
            Action::OrderTriggers { order: vec![ObjectId(2), ObjectId(1)] }
        );

        let many: Vec<ObjectId> = (0..8).map(ObjectId).collect();
        let opts = trigger_order_options(&many);
        assert_eq!(opts.len(), MAX_TRIGGER_ORDERS, "teto de permutações respeitado");
        for (i, a) in opts.iter().enumerate() {
            assert!(
                !opts[i + 1..].contains(a),
                "permutações não podem repetir"
            );
        }
    }

    #[test]
    fn ids_de_item_de_pilha_nao_colidem() {
        let mut game = game_with(vec![card(0, "Peça", vec![CardType::Creature])]);
        let first = reserve_stack_id(&game);
        game.state
            .pending_triggers
            .push(trigger_item(first, ObjectId(0), PlayerId::P0));
        let second = reserve_stack_id(&game);
        assert_ne!(first, second);
        assert!(!first.is_none() && !second.is_none());
    }
}
