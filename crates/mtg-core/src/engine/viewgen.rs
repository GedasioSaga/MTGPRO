//! Projeção do estado para a interface, já redigida por observador.
//!
//! Regra do módulo: a UI não decide nada. Tudo que ela precisaria calcular —
//! P/T depois das camadas, palavras-chave concedidas, se a carta é alvo legal,
//! se dá para agir com ela agora — sai pronto daqui. E nada que o observador
//! não pode ver atravessa a fronteira: zona oculta vira `None`, não vira dado
//! escondido no JSON que um cliente curioso leria.
use super::{cast, layers, turn, Game};
use crate::action::{Action, Request, TargetChoice};
use crate::card::CardDef;
use crate::event::Phase;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{ColorSet, ManaCost, ManaSymbol};
use crate::state::{ObjectState, StackItem, StackItemKind};
use crate::types::{CardType, CounterKind};
use crate::view::{CardView, GameView, Observer, PlayerView, StackItemView};
use crate::zone::ZoneId;

/// Quantas linhas de log a UI recebe por atualização.
const LOG_TAIL: usize = 30;

/// Monta a visão completa da partida para um observador.
pub fn build_view(game: &Game, observer: Observer) -> GameView {
    let state = &game.state;
    let count = state.players.len();

    let highlights = pending_highlights(&state.pending, observer);
    let actionable = actionable_objects(game, observer);

    let mut cards: Vec<CardView> = Vec::new();
    let mut battlefield: Vec<Vec<ObjectId>> = vec![Vec::new(); count];
    let mut hands: Vec<Vec<ObjectId>> = vec![Vec::new(); count];
    let mut graveyards: Vec<Vec<ObjectId>> = vec![Vec::new(); count];
    let mut exiles: Vec<Vec<ObjectId>> = vec![Vec::new(); count];

    for id in turn::zone_objects(game, ZoneId::BATTLEFIELD) {
        let Some(obj) = state.object(id) else { continue };
        if let Some(slot) = battlefield.get_mut(obj.controller.index()) {
            slot.push(id);
        }
        // Permanente virado para baixo é público como objeto, oculto como carta.
        let visible = !obj.face_down || can_peek(observer, obj.owner);
        cards.push(card_view(game, obj, visible, &highlights, &actionable));
    }

    for player in state.players.iter() {
        let visible = observer.can_see_hand(player.id);
        for id in turn::zone_objects(game, ZoneId::hand(player.id)) {
            let Some(obj) = state.object(id) else { continue };
            if let Some(slot) = hands.get_mut(player.id.index()) {
                slot.push(id);
            }
            cards.push(card_view(game, obj, visible, &highlights, &actionable));
        }
        for id in turn::zone_objects(game, ZoneId::graveyard(player.id)) {
            let Some(obj) = state.object(id) else { continue };
            if let Some(slot) = graveyards.get_mut(player.id.index()) {
                slot.push(id);
            }
            cards.push(card_view(game, obj, true, &highlights, &actionable));
        }
    }

    // Exílio é zona compartilhada: agrupa por dono para a UI ter onde desenhar.
    for id in turn::zone_objects(game, ZoneId::EXILE) {
        let Some(obj) = state.object(id) else { continue };
        if let Some(slot) = exiles.get_mut(obj.owner.index()) {
            slot.push(id);
        }
        let visible = !obj.face_down || can_peek(observer, obj.owner);
        cards.push(card_view(game, obj, visible, &highlights, &actionable));
    }

    // Mágicas na pilha são públicas (CR 601.2a: lançar é ação visível).
    for id in turn::zone_objects(game, ZoneId::STACK) {
        let Some(obj) = state.object(id) else { continue };
        cards.push(card_view(game, obj, true, &highlights, &actionable));
    }

    // A biblioteca só aparece para o observador onisciente, e mesmo assim não
    // tem lista de ids em `GameView` — a UI vê apenas a contagem.
    cards.sort_by_key(|c| c.id.0);
    cards.dedup_by_key(|c| c.id.0);

    let players: Vec<PlayerView> = state
        .players
        .iter()
        .map(|p| PlayerView {
            id: p.id,
            name: p.name.clone(),
            life: p.life,
            poison: p.poison,
            mana_pool: p.mana_pool,
            hand_count: turn::zone_objects(game, ZoneId::hand(p.id)).len(),
            library_count: turn::zone_objects(game, ZoneId::library(p.id)).len(),
            graveyard_count: turn::zone_objects(game, ZoneId::graveyard(p.id)).len(),
            exile_count: exiles.get(p.id.index()).map_or(0, |z| z.len()),
            lands_played_this_turn: p.lands_played_this_turn,
            max_lands_per_turn: p.max_lands_per_turn,
            has_lost: p.has_lost,
            is_active: p.id == state.active_player,
            has_priority: state.pending.player() == Some(p.id),
        })
        .collect();

    // A pilha em `GameView` sai do topo para a base — é a ordem que a UI empilha.
    let stack: Vec<StackItemView> = state.stack.iter().rev().map(|i| stack_view(game, i)).collect();

    let log_tail: Vec<String> = state
        .log
        .iter()
        .rev()
        .take(LOG_TAIL)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|entry| format!("T{} {}: {}", entry.turn, entry.step.label(), entry.text))
        .collect();

    GameView {
        turn: state.turn,
        step: state.step,
        step_label: state.step.label().to_string(),
        phase: state.step.phase(),
        active_player: state.active_player,
        priority_player: priority_player(&state.pending),
        outcome: state.outcome,
        players,
        cards,
        battlefield,
        hands,
        graveyards,
        exiles,
        stack,
        prompt: prompt_for(game, &state.pending),
        log_tail,
    }
}

// ---------------------------------------------------------------------------
// Cartas
// ---------------------------------------------------------------------------

fn can_peek(observer: Observer, owner: PlayerId) -> bool {
    match observer {
        Observer::Omniscient => true,
        Observer::Player(me) => me == owner,
        Observer::Spectator => false,
    }
}

fn card_view(
    game: &Game,
    obj: &ObjectState,
    visible: bool,
    highlights: &[ObjectId],
    actionable: &[ObjectId],
) -> CardView {
    let current = layers::characteristics(game, obj.id);
    let base = layers::base_characteristics(game, obj.id);
    let def: Option<&CardDef> = game.db.get(obj.card);

    let is_creature = current
        .as_ref()
        .map(|c| c.type_line.is_creature())
        .or_else(|| def.map(|d| d.type_line.is_creature()))
        .unwrap_or(false);
    let is_planeswalker = current
        .as_ref()
        .map(|c| c.type_line.has_type(CardType::Planeswalker))
        .or_else(|| def.map(|d| d.type_line.has_type(CardType::Planeswalker)))
        .unwrap_or(false);

    // P/T de permanente é informação pública mesmo com a face oculta: uma carta
    // virada para baixo no campo é 2/2 e todo mundo enxerga isso.
    let show_stats = visible || obj.on_battlefield();

    let name = if visible {
        current
            .as_ref()
            .map(|c| c.name.clone())
            .or_else(|| obj.token_spec.as_ref().map(|t| t.name.clone()))
            .or_else(|| def.map(|d| d.name.clone()))
    } else {
        None
    };

    let counters: Vec<(String, i32)> = obj
        .counters
        .iter()
        .filter(|(_, n)| **n != 0)
        .map(|(kind, n)| (counter_label(kind), *n))
        .collect();

    let keywords = if show_stats {
        current
            .as_ref()
            .map(|c| c.keywords.iter().map(|k| k.label()).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let loyalty = if is_planeswalker && show_stats {
        if obj.on_battlefield() {
            Some(obj.counter(&CounterKind::Loyalty))
        } else {
            current
                .as_ref()
                .map(|c| c.loyalty)
                .or_else(|| def.and_then(|d| d.loyalty))
        }
    } else {
        None
    };

    CardView {
        id: obj.id,
        name,
        mana_cost: if visible {
            def.map(|d| render_mana_cost(&d.mana_cost))
        } else {
            None
        },
        mana_value: if visible {
            current
                .as_ref()
                .map(|c| c.mana_value)
                .or_else(|| def.map(|d| d.mana_value()))
                .unwrap_or(0)
        } else {
            0
        },
        type_line: if visible {
            current
                .as_ref()
                .map(|c| c.type_line.render())
                .or_else(|| def.map(|d| d.type_line.render()))
        } else {
            None
        },
        oracle_text: if visible {
            def.map(|d| d.oracle_text.clone())
        } else {
            None
        },
        flavor_text: if visible {
            def.and_then(|d| d.flavor_text.clone())
        } else {
            None
        },
        colors: if visible {
            current
                .as_ref()
                .map(|c| c.colors)
                .or_else(|| def.map(|d| d.colors()))
                .unwrap_or(ColorSet::COLORLESS)
        } else {
            ColorSet::COLORLESS
        },
        power: if is_creature && show_stats {
            current.as_ref().map(|c| c.power)
        } else {
            None
        },
        toughness: if is_creature && show_stats {
            current.as_ref().map(|c| c.toughness)
        } else {
            None
        },
        base_power: if is_creature && visible {
            base.as_ref()
                .map(|c| c.power)
                .or_else(|| def.and_then(|d| d.power))
        } else {
            None
        },
        base_toughness: if is_creature && visible {
            base.as_ref()
                .map(|c| c.toughness)
                .or_else(|| def.and_then(|d| d.toughness))
        } else {
            None
        },
        loyalty,
        damage: obj.damage,
        tapped: obj.tapped,
        face_down: obj.face_down,
        summoning_sick: obj.summoning_sick,
        attacking: obj.combat.attacking,
        blocking: obj.combat.blocking.clone(),
        blocked_by: obj.combat.blocked_by.clone(),
        counters,
        keywords,
        attached_to: obj.attached_to,
        attachments: obj.attachments.clone(),
        is_token: obj.is_token,
        controller: obj.controller,
        owner: obj.owner,
        zone: obj.zone.kind,
        art_key: if visible {
            def.and_then(|d| d.art_key.clone())
                .or_else(|| obj.token_spec.as_ref().and_then(|t| t.art_key.clone()))
        } else {
            None
        },
        rarity: if visible {
            def.map(|d| format!("{:?}", d.rarity))
        } else {
            None
        },
        set_code: if visible {
            def.map(|d| d.set_code.clone())
        } else {
            None
        },
        is_legal_target: highlights.contains(&obj.id),
        is_actionable: actionable.contains(&obj.id),
    }
}

fn counter_label(kind: &CounterKind) -> String {
    match kind {
        CounterKind::PlusOnePlusOne => "+1/+1".to_string(),
        CounterKind::MinusOneMinusOne => "-1/-1".to_string(),
        CounterKind::Named(name) => name.clone(),
        other => format!("{other:?}"),
    }
}

fn render_mana_cost(cost: &ManaCost) -> String {
    cost.symbols.iter().map(render_symbol).collect()
}

fn render_symbol(symbol: &ManaSymbol) -> String {
    match symbol {
        ManaSymbol::Generic(n) => format!("{{{n}}}"),
        ManaSymbol::Colored(c) => format!("{{{}}}", c.letter()),
        ManaSymbol::Colorless => "{C}".to_string(),
        ManaSymbol::Hybrid(a, b) => format!("{{{}/{}}}", a.letter(), b.letter()),
        ManaSymbol::MonoHybrid(c) => format!("{{2/{}}}", c.letter()),
        ManaSymbol::Phyrexian(c) => format!("{{{}/P}}", c.letter()),
        ManaSymbol::Snow => "{S}".to_string(),
        ManaSymbol::X => "{X}".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pilha
// ---------------------------------------------------------------------------

fn stack_view(game: &Game, item: &StackItem) -> StackItemView {
    let def = game.db.get(item.card);
    let name = def.map(|d| d.name.clone()).unwrap_or_else(|| "?".to_string());

    let (is_ability, text) = match &item.kind {
        StackItemKind::Spell { .. } | StackItemKind::CopiedSpell { .. } => (
            false,
            def.map(|d| d.oracle_text.clone()).unwrap_or_default(),
        ),
        StackItemKind::ActivatedAbility { source, index }
        | StackItemKind::TriggeredAbility { source, index } => {
            (true, ability_text(game, *source, *index))
        }
    };

    let mut targets = Vec::new();
    let mut target_players = Vec::new();
    for choice in &item.targets {
        match choice {
            TargetChoice::Object(id) => targets.push(*id),
            TargetChoice::Player(p) => target_players.push(*p),
        }
    }

    StackItemView {
        id: item.id,
        name: if is_ability {
            format!("{name} (habilidade)")
        } else {
            name
        },
        text,
        controller: item.controller,
        targets,
        target_players,
        is_ability,
        source_card: item.kind.source(),
        art_key: def.and_then(|d| d.art_key.clone()),
    }
}

fn ability_text(game: &Game, source: ObjectId, index: u16) -> String {
    let Some(obj) = game.state.object(source) else {
        return String::new();
    };
    let Some(def) = game.db.get(obj.card) else {
        return String::new();
    };
    def.abilities
        .get(index as usize)
        .map(|a| a.text())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Decisão pendente
// ---------------------------------------------------------------------------

fn priority_player(pending: &Request) -> Option<PlayerId> {
    match pending {
        Request::Priority { player } => Some(*player),
        _ => None,
    }
}

/// Objetos que a decisão pendente destaca. Só o jogador da vez vê o destaque —
/// para os demais, revelar os candidatos seria vazar intenção.
fn pending_highlights(pending: &Request, observer: Observer) -> Vec<ObjectId> {
    let asked = pending.player();
    let allowed = match observer {
        Observer::Omniscient | Observer::Spectator => true,
        Observer::Player(me) => asked == Some(me),
    };
    if !allowed {
        return Vec::new();
    }
    match pending {
        Request::DeclareAttackers { eligible, .. } => eligible.clone(),
        Request::DeclareBlockers { eligible, attackers, .. } => {
            eligible.iter().chain(attackers.iter()).copied().collect()
        }
        Request::OrderBlockers { attacker, blockers, .. }
        | Request::AssignCombatDamage { attacker, blockers, .. } => std::iter::once(*attacker)
            .chain(blockers.iter().copied())
            .collect(),
        Request::OrderTriggers { triggers, .. } => triggers.clone(),
        Request::SelectObjects { candidates, .. } => candidates.clone(),
        Request::ArrangeCards { cards, .. } => cards.clone(),
        _ => Vec::new(),
    }
}

/// Cartas com que o observador pode agir agora. Derivado das próprias ações
/// legais, então UI e motor nunca discordam sobre o que é jogável.
fn actionable_objects(game: &Game, observer: Observer) -> Vec<ObjectId> {
    let Observer::Player(me) = observer else {
        return Vec::new();
    };
    let Request::Priority { player } = &game.state.pending else {
        return Vec::new();
    };
    if *player != me {
        return Vec::new();
    }
    let mut ids: Vec<ObjectId> = cast::priority_actions(game, me)
        .iter()
        .filter_map(action_object)
        .collect();
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    ids
}

fn action_object(action: &Action) -> Option<ObjectId> {
    match action {
        Action::PlayLand { object } | Action::CastSpell { object, .. } => Some(*object),
        Action::ActivateAbility { source, .. } => Some(*source),
        _ => None,
    }
}

fn prompt_for(game: &Game, pending: &Request) -> Option<String> {
    let name = |p: PlayerId| {
        game.state
            .players
            .get(p.index())
            .map(|x| x.name.clone())
            .unwrap_or_else(|| format!("Jogador {}", p.0))
    };
    let card = |id: ObjectId| game.card_name(id);

    let text = match pending {
        Request::GameOver => return None,
        Request::Priority { player } => {
            format!("{}: prioridade em {}", name(*player), phase_label(game))
        }
        Request::Mulligan { player, mulligans_taken } => format!(
            "{}: manter a mão ou tomar mulligan (#{}) ?",
            name(*player),
            mulligans_taken + 1
        ),
        Request::BottomCards { player, count } => format!(
            "{}: devolver {count} carta(s) ao fundo da biblioteca",
            name(*player)
        ),
        Request::DeclareAttackers { player, eligible } => format!(
            "{}: declarar atacantes ({} disponível/eis)",
            name(*player),
            eligible.len()
        ),
        Request::DeclareBlockers { player, attackers, .. } => format!(
            "{}: declarar bloqueadores contra {} atacante(s)",
            name(*player),
            attackers.len()
        ),
        Request::OrderBlockers { player, attacker, blockers } => format!(
            "{}: ordenar os {} bloqueadores de {}",
            name(*player),
            blockers.len(),
            card(*attacker)
        ),
        Request::AssignCombatDamage { player, attacker, total, .. } => format!(
            "{}: distribuir {total} de dano de {}",
            name(*player),
            card(*attacker)
        ),
        Request::OrderTriggers { player, triggers } => format!(
            "{}: ordenar {} gatilhos simultâneos",
            name(*player),
            triggers.len()
        ),
        Request::ConfirmOptional { player, prompt } => format!("{}: {prompt}", name(*player)),
        Request::ChooseModes { player, prompt, choose, .. } => {
            format!("{}: escolher {choose} modo(s) — {prompt}", name(*player))
        }
        Request::SelectObjects { player, prompt, min, max, .. } => {
            if min == max {
                format!("{}: {prompt} (escolha {min})", name(*player))
            } else {
                format!("{}: {prompt} (escolha de {min} a {max})", name(*player))
            }
        }
        Request::ChooseColor { player, prompt } => format!("{}: {prompt}", name(*player)),
        Request::ArrangeCards { player, prompt, cards, alt_label } => format!(
            "{}: {prompt} — {} carta(s), topo ou {alt_label}",
            name(*player),
            cards.len()
        ),
    };
    Some(text)
}

fn phase_label(game: &Game) -> &'static str {
    match game.state.step.phase() {
        Phase::Beginning => "fase inicial",
        Phase::PrecombatMain => "fase principal pré-combate",
        Phase::Combat => "fase de combate",
        Phase::PostcombatMain => "fase principal pós-combate",
        Phase::Ending => "fase final",
    }
}
