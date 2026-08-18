//! Avaliação pura do IR: filtros, seletores, valores, condições e alvos.
//!
//! Nada aqui muta estado — é a camada de leitura sobre a qual `cast`, `combat`,
//! `resolve` e `sba` tomam decisão. Duas invariantes valem em todo o arquivo:
//!
//! 1. Característica sai de `layers::characteristics`, nunca de `CardDef`. Ler
//!    a carta crua ignora anthem, marcador e mudança de tipo, e o erro passa
//!    despercebido porque o motor continua rodando.
//! 2. Toda lista devolvida é determinística: ordenada por `ObjectId` e sem
//!    repetição. Semente igual precisa dar partida igual, e a ordem em que os
//!    alvos são enumerados entra na decisão do bot.
use super::{layers, Characteristics, Game};
use crate::action::TargetChoice;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ColorSet;
use crate::ir::{
    Cmp, Condition, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec, Value,
    ZoneScope,
};
use crate::state::{CombatState, LastKnown, ObjectState, StackItemKind, TriggerContext};
use crate::types::{CounterKind, TypeLine};
use crate::zone::{ZoneId, ZoneKind};

/// Contexto de avaliação: quem é a fonte, quem controla, o que foi escolhido.
/// Sem ele, `ObjRef::SelfObject` e `Value::X` não têm significado.
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
    pub fn for_source(source: ObjectId, controller: PlayerId) -> Self {
        EvalCtx { source: Some(source), controller, ..Default::default() }
    }
}

// ---------------------------------------------------------------------------
// Filtros
// ---------------------------------------------------------------------------

/// Como um filtro lê o objeto: vivo (estado corrente + camadas) ou pelo retrato
/// de última existência, quando ele já saiu da zona (CR 603.6d).
enum ObjView<'a> {
    Live { state: &'a ObjectState, ch: Characteristics },
    Remembered(&'a LastKnown),
}

impl ObjView<'_> {
    fn name(&self) -> &str {
        match self {
            ObjView::Live { ch, .. } => &ch.name,
            ObjView::Remembered(l) => &l.name,
        }
    }
    fn colors(&self) -> ColorSet {
        match self {
            ObjView::Live { ch, .. } => ch.colors,
            ObjView::Remembered(l) => l.colors,
        }
    }
    fn type_line(&self) -> &TypeLine {
        match self {
            ObjView::Live { ch, .. } => &ch.type_line,
            ObjView::Remembered(l) => &l.type_line,
        }
    }
    fn power(&self) -> i32 {
        match self {
            ObjView::Live { ch, .. } => ch.power,
            ObjView::Remembered(l) => l.power,
        }
    }
    fn toughness(&self) -> i32 {
        match self {
            ObjView::Live { ch, .. } => ch.toughness,
            ObjView::Remembered(l) => l.toughness,
        }
    }
    fn mana_value(&self) -> u32 {
        match self {
            ObjView::Live { ch, .. } => ch.mana_value,
            ObjView::Remembered(l) => l.mana_value,
        }
    }
    fn has_keyword(&self, k: &Keyword) -> bool {
        match self {
            ObjView::Live { ch, .. } => ch.has_keyword(k),
            ObjView::Remembered(l) => l.has_keyword(k),
        }
    }
    fn counter(&self, kind: &CounterKind) -> i32 {
        match self {
            ObjView::Live { state, .. } => state.counter(kind),
            ObjView::Remembered(l) => l.counter(kind),
        }
    }
    fn tapped(&self) -> bool {
        match self {
            ObjView::Live { state, .. } => state.tapped,
            ObjView::Remembered(l) => l.tapped,
        }
    }
    fn combat(&self) -> &CombatState {
        match self {
            ObjView::Live { state, .. } => &state.combat,
            ObjView::Remembered(l) => &l.combat,
        }
    }
    fn is_token(&self) -> bool {
        match self {
            ObjView::Live { state, .. } => state.is_token,
            ObjView::Remembered(l) => l.is_token,
        }
    }
    fn summoning_sick(&self) -> bool {
        match self {
            ObjView::Live { state, .. } => state.summoning_sick,
            ObjView::Remembered(l) => l.summoning_sick,
        }
    }
    fn controller(&self) -> PlayerId {
        match self {
            ObjView::Live { ch, .. } => ch.controller,
            ObjView::Remembered(l) => l.controller,
        }
    }
    fn owner(&self) -> PlayerId {
        match self {
            ObjView::Live { state, .. } => state.owner,
            ObjView::Remembered(l) => l.owner,
        }
    }
    fn is_live(&self) -> bool {
        matches!(self, ObjView::Live { .. })
    }
}

pub fn matches_filter(game: &Game, obj: ObjectId, filter: &Filter, ctx: &EvalCtx) -> bool {
    matches_filter_at(game, obj, filter, ctx, None)
}

/// Igual a `matches_filter`, mas julga pelo retrato de última existência quando
/// ele é fornecido (CR 603.6d): "se a criatura que morreu era vermelha" precisa
/// enxergar a criatura como ela era, não o que sobrou dela no cemitério.
pub fn matches_filter_at(
    game: &Game,
    obj: ObjectId,
    filter: &Filter,
    ctx: &EvalCtx,
    last: Option<&LastKnown>,
) -> bool {
    // Objeto que não existe não casa com nada, nem com `Any`: quem some do jogo
    // some das listas (CR 400.7). Havendo retrato, quem responde é o retrato.
    if last.is_none() && game.state.object(obj).is_none() {
        return false;
    }

    // Variantes estruturais antes de calcular as camadas: elas não precisam das
    // características, e recursar depois pagaria o custo duas vezes.
    match filter {
        Filter::Any => return true,
        Filter::And(fs) => return fs.iter().all(|f| matches_filter_at(game, obj, f, ctx, last)),
        Filter::Or(fs) => return fs.iter().any(|f| matches_filter_at(game, obj, f, ctx, last)),
        Filter::Not(f) => return !matches_filter_at(game, obj, f, ctx, last),
        _ => {}
    }

    let view = match last {
        Some(l) => ObjView::Remembered(l),
        None => {
            let Some(state) = game.state.object(obj) else { return false };
            let Some(ch) = layers::characteristics(game, obj) else { return false };
            ObjView::Live { state, ch }
        }
    };

    match filter {
        Filter::HasType(t) => view.type_line().has_type(*t),
        Filter::HasSubtype(s) => view.type_line().has_subtype(s),
        Filter::HasSupertype(s) => view.type_line().has_supertype(*s),
        Filter::HasName(n) => view.name().eq_ignore_ascii_case(n),
        Filter::HasColor(c) => view.colors().contains(*c),
        Filter::Colorless => view.colors().is_colorless(),
        Filter::Multicolored => view.colors().is_multicolored(),
        Filter::HasKeyword(k) => view.has_keyword(k),
        Filter::HasCounter(kind) => view.counter(kind) > 0,

        Filter::Tapped => view.tapped(),
        Filter::Untapped => !view.tapped(),
        Filter::Attacking => view.combat().is_attacking(),
        Filter::Blocking => view.combat().is_blocking(),
        // CR 509.1h — "bloqueada" continua valendo mesmo que o bloqueador saia.
        Filter::Blocked => view.combat().is_attacking() && view.combat().was_blocked,
        Filter::Unblocked => view.combat().is_attacking() && !view.combat().was_blocked,
        Filter::Token => view.is_token(),
        Filter::NonToken => !view.is_token(),
        Filter::SummoningSick => view.summoning_sick(),

        Filter::PowerAtLeast(n) => view.power() >= *n,
        Filter::PowerAtMost(n) => view.power() <= *n,
        Filter::ToughnessAtLeast(n) => view.toughness() >= *n,
        Filter::ToughnessAtMost(n) => view.toughness() <= *n,
        Filter::ManaValueAtLeast(n) => view.mana_value() >= *n,
        Filter::ManaValueAtMost(n) => view.mana_value() <= *n,
        Filter::ManaValueExactly(n) => view.mana_value() == *n,

        Filter::ControlledBy(p) => resolve_players(game, p, ctx).contains(&view.controller()),
        Filter::OwnedBy(p) => resolve_players(game, p, ctx).contains(&view.owner()),
        // "Isto" e "outra criatura" são relativos à fonte do efeito, não ao
        // objeto sendo testado — sem fonte, "outro" é trivialmente verdadeiro.
        Filter::IsSelf => ctx.source == Some(obj),
        Filter::IsOther => ctx.source != Some(obj),
        // CR 115.6 — o que já saiu da zona não é alvo legal de coisa alguma.
        Filter::Targetable => {
            view.is_live() && can_be_targeted(game, obj, ctx.source, ctx.controller)
        }

        // Estruturais já tratadas acima; repetir aqui só para o match ser total.
        Filter::Any | Filter::And(_) | Filter::Or(_) | Filter::Not(_) => true,
    }
}

// ---------------------------------------------------------------------------
// Seletores
// ---------------------------------------------------------------------------

/// Zonas varridas por um escopo. `Anywhere` varre tudo, inclusive as zonas
/// privadas de cada jogador (CR 400.1).
fn zones_in_scope(game: &Game, scope: ZoneScope) -> Vec<ZoneId> {
    let players: Vec<PlayerId> = game.state.players.iter().map(|p| p.id).collect();
    match scope {
        ZoneScope::Battlefield => vec![ZoneId::BATTLEFIELD],
        ZoneScope::Stack => vec![ZoneId::STACK],
        ZoneScope::Exile => vec![ZoneId::EXILE],
        ZoneScope::Graveyard => players.iter().map(|p| ZoneId::graveyard(*p)).collect(),
        ZoneScope::Hand => players.iter().map(|p| ZoneId::hand(*p)).collect(),
        ZoneScope::Library => players.iter().map(|p| ZoneId::library(*p)).collect(),
        ZoneScope::Anywhere => {
            let mut out = vec![ZoneId::BATTLEFIELD, ZoneId::STACK, ZoneId::EXILE];
            for p in players {
                out.push(ZoneId::library(p));
                out.push(ZoneId::hand(p));
                out.push(ZoneId::graveyard(p));
                out.push(ZoneId { kind: ZoneKind::Command, owner: Some(p) });
            }
            out
        }
    }
}

fn zone_in_scope(zone: ZoneId, scope: ZoneScope) -> bool {
    match scope {
        ZoneScope::Anywhere => true,
        ZoneScope::Battlefield => zone.kind == ZoneKind::Battlefield,
        ZoneScope::Graveyard => zone.kind == ZoneKind::Graveyard,
        ZoneScope::Hand => zone.kind == ZoneKind::Hand,
        ZoneScope::Library => zone.kind == ZoneKind::Library,
        ZoneScope::Exile => zone.kind == ZoneKind::Exile,
        ZoneScope::Stack => zone.kind == ZoneKind::Stack,
    }
}

/// Quem "possui" o objeto para efeito de `owner_scope`. Em zona compartilhada
/// quem manda é o controlador (CR 108.4); em zona privada, o dono.
fn scope_owner(game: &Game, obj: ObjectId) -> Option<PlayerId> {
    let state = game.state.object(obj)?;
    if state.zone.kind.is_shared() {
        Some(
            layers::characteristics(game, obj)
                .map(|c| c.controller)
                .unwrap_or(state.controller),
        )
    } else {
        Some(state.owner)
    }
}

/// O objeto casa com o seletor inteiro: zona, dono e filtro. Não aplica `max`
/// — corte de quantidade é decisão de quem chama, não de quem testa.
pub fn matches_selector(game: &Game, obj: ObjectId, sel: &Selector, ctx: &EvalCtx) -> bool {
    let Some(state) = game.state.object(obj) else { return false };
    if !zone_in_scope(state.zone, sel.zone) {
        return false;
    }
    if let Some(scope) = &sel.owner_scope {
        let Some(who) = scope_owner(game, obj) else { return false };
        if !resolve_players(game, scope, ctx).contains(&who) {
            return false;
        }
    }
    matches_filter(game, obj, &sel.filter, ctx)
}

/// Candidatos do seletor, ordenados e sem repetição, ignorando `max`.
fn gather(game: &Game, sel: &Selector, ctx: &EvalCtx) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = Vec::new();
    for zone in zones_in_scope(game, sel.zone) {
        for obj in zone_objects(game, zone) {
            if matches_selector(game, obj, sel, ctx) {
                out.push(obj);
            }
        }
    }
    // Ordem de zona não é estável entre execuções (cemitério reordena, campo
    // recebe em ordem de entrada); ordem de id é.
    out.sort_unstable();
    out.dedup();
    out
}

pub fn select(game: &Game, sel: &Selector, ctx: &EvalCtx) -> Vec<ObjectId> {
    let mut out = gather(game, sel, ctx);
    if let Some(max) = sel.max {
        out.truncate(max as usize);
    }
    out
}

// ---------------------------------------------------------------------------
// Referências
// ---------------------------------------------------------------------------

pub fn resolve_players(game: &Game, r: &PlayerRef, ctx: &EvalCtx) -> Vec<PlayerId> {
    let mut out: Vec<PlayerId> = match r {
        PlayerRef::You => vec![ctx.controller],
        PlayerRef::Opponents => game.state.opponents(ctx.controller),
        // CR 101.4 — ordem de turno a partir do jogador ativo.
        PlayerRef::Each => {
            let n = game.state.players.len();
            let start = game.state.active_player.index();
            (0..n)
                .filter_map(|i| game.state.players.get((start + i) % n.max(1)))
                .map(|p| p.id)
                .collect()
        }
        PlayerRef::Target(i) => match ctx.targets.get(*i as usize) {
            Some(TargetChoice::Player(p)) => vec![*p],
            // Alvo de objeto num slot de jogador: o controlador do objeto é a
            // leitura correta para "o jogador alvo" de um efeito mal montado.
            Some(TargetChoice::Object(o)) => controller_of(game, *o).into_iter().collect(),
            None => Vec::new(),
        },
        PlayerRef::ControllerOf(r) => resolve_objects(game, r, ctx)
            .into_iter()
            .filter_map(|o| controller_of(game, o))
            .collect(),
        PlayerRef::OwnerOf(r) => resolve_objects(game, r, ctx)
            .into_iter()
            .filter_map(|o| game.state.object(o).map(|s| s.owner))
            .collect(),
        PlayerRef::ActivePlayer => vec![game.state.active_player],
    };
    // Dedup preservando a ordem: em `Each` a ordem de turno é significativa.
    let mut seen: Vec<PlayerId> = Vec::with_capacity(out.len());
    out.retain(|p| {
        if seen.contains(p) {
            false
        } else {
            seen.push(*p);
            true
        }
    });
    out
}

fn controller_of(game: &Game, obj: ObjectId) -> Option<PlayerId> {
    let state = game.state.object(obj)?;
    Some(
        layers::characteristics(game, obj)
            .map(|c| c.controller)
            .unwrap_or(state.controller),
    )
}

pub fn resolve_objects(game: &Game, r: &ObjRef, ctx: &EvalCtx) -> Vec<ObjectId> {
    match r {
        ObjRef::SelfObject => ctx.source.into_iter().collect(),
        ObjRef::Target(i) => match ctx.targets.get(*i as usize) {
            Some(TargetChoice::Object(o)) => vec![*o],
            _ => Vec::new(),
        },
        ObjRef::Selected => ctx.selected.into_iter().collect(),
        // CR 301.5c — a aura/equipamento sabe a quem está anexada.
        ObjRef::Attached => ctx
            .source
            .and_then(|s| game.state.object(s))
            .and_then(|s| s.attached_to)
            .into_iter()
            .collect(),
        ObjRef::TriggerObject => ctx.trigger.trigger_object.into_iter().collect(),
        ObjRef::TriggerSource => ctx.trigger.trigger_source.into_iter().collect(),
        ObjRef::All(sel) => select(game, sel, ctx),
        ObjRef::Remembered(i) => ctx.remembered.get(*i as usize).copied().into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// Valores
// ---------------------------------------------------------------------------

/// Aritmética saturante em toda parte: um `Value` mal montado numa carta não
/// pode derrubar a partida com estouro de inteiro em debug.
pub fn eval_value(game: &Game, v: &Value, ctx: &EvalCtx) -> i32 {
    match v {
        Value::Const(n) => *n,
        Value::X => i32::try_from(ctx.x).unwrap_or(i32::MAX),
        Value::Count(sel) => clamp_len(gather(game, sel, ctx).len()),
        // Agregam quando a referência resolve para vários objetos ("poder total
        // das criaturas que você controla"); para referência única é o próprio.
        // CR 603.6d — com retrato de última existência, o valor sai dele: a 2/2
        // que morreu com dois marcadores +1/+1 morreu como 4/4.
        Value::PowerOf(r) => sum_over(game, r, ctx, |game, o, last| match last {
            Some(l) => l.power,
            None => layers::characteristics(game, o).map(|c| c.power).unwrap_or(0),
        }),
        Value::ToughnessOf(r) => sum_over(game, r, ctx, |game, o, last| match last {
            Some(l) => l.toughness,
            None => layers::characteristics(game, o).map(|c| c.toughness).unwrap_or(0),
        }),
        Value::ManaValueOf(r) => sum_over(game, r, ctx, |game, o, last| {
            let mv = match last {
                Some(l) => Some(l.mana_value),
                None => layers::characteristics(game, o).map(|c| c.mana_value),
            };
            mv.map(|v| i32::try_from(v).unwrap_or(i32::MAX)).unwrap_or(0)
        }),
        Value::CountersOn(r, kind) => sum_over(game, r, ctx, |game, o, last| match last {
            Some(l) => l.counter(kind),
            None => game.state.object(o).map(|s| s.counter(kind)).unwrap_or(0),
        }),
        Value::LifeOf(p) => resolve_players(game, p, ctx)
            .into_iter()
            .filter_map(|id| game.state.players.get(id.index()))
            .fold(0i32, |acc, p| acc.saturating_add(p.life)),
        Value::CardsInHandOf(p) => resolve_players(game, p, ctx)
            .into_iter()
            .fold(0i32, |acc, id| {
                acc.saturating_add(clamp_len(zone_objects(game, ZoneId::hand(id)).len()))
            }),
        Value::ChosenNumber => ctx.chosen_number,
        Value::Add(a, b) => eval_value(game, a, ctx).saturating_add(eval_value(game, b, ctx)),
        Value::Sub(a, b) => eval_value(game, a, ctx).saturating_sub(eval_value(game, b, ctx)),
        Value::Mul(a, b) => eval_value(game, a, ctx).saturating_mul(eval_value(game, b, ctx)),
        Value::Neg(a) => 0i32.saturating_sub(eval_value(game, a, ctx)),
        Value::Max(a, b) => eval_value(game, a, ctx).max(eval_value(game, b, ctx)),
        Value::Min(a, b) => eval_value(game, a, ctx).min(eval_value(game, b, ctx)),
    }
}

fn clamp_len(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn sum_over(
    game: &Game,
    r: &ObjRef,
    ctx: &EvalCtx,
    f: impl Fn(&Game, ObjectId, Option<&LastKnown>) -> i32,
) -> i32 {
    resolve_objects(game, r, ctx)
        .into_iter()
        .fold(0i32, |acc, o| {
            acc.saturating_add(f(game, o, last_known_for(game, r, o, ctx)))
        })
}

/// CR 603.6d — o retrato do objeto do gatilho, quando ele já não está na zona
/// em que estava no instante do evento. Enquanto ele continua lá, devolve
/// `None`: o estado corrente é a informação correta e mais barata.
///
/// Só `ObjRef::TriggerObject` tem retrato; as outras referências apontam para
/// objetos que ninguém prometeu lembrar.
fn last_known_for<'a>(
    game: &Game,
    r: &ObjRef,
    obj: ObjectId,
    ctx: &'a EvalCtx,
) -> Option<&'a LastKnown> {
    if !matches!(r, ObjRef::TriggerObject) {
        return None;
    }
    let last = ctx.trigger.last_known.as_deref()?;
    if last.object != obj {
        return None;
    }
    match game.state.object(obj) {
        Some(o) if o.zone == last.zone => None,
        _ => Some(last),
    }
}

// ---------------------------------------------------------------------------
// Condições
// ---------------------------------------------------------------------------

pub fn eval_condition(game: &Game, c: &Condition, ctx: &EvalCtx) -> bool {
    match c {
        Condition::Always => true,
        Condition::Never => false,
        Condition::Compare(a, cmp, b) => {
            let (a, b) = (eval_value(game, a, ctx), eval_value(game, b, ctx));
            match cmp {
                Cmp::Eq => a == b,
                Cmp::Ne => a != b,
                Cmp::Lt => a < b,
                Cmp::Le => a <= b,
                Cmp::Gt => a > b,
                Cmp::Ge => a >= b,
            }
        }
        Condition::Exists(sel) => !gather(game, sel, ctx).is_empty(),
        // Referência vazia não casa: "se a criatura alvo for vermelha" é falso
        // quando não existe criatura alvo.
        Condition::Matches(r, filter) => {
            let objs = resolve_objects(game, r, ctx);
            !objs.is_empty()
                && objs.iter().all(|o| {
                    // CR 603.6d — "se ela era uma criatura voadora" é julgado
                    // pelo retrato quando o objeto já saiu da zona.
                    matches_filter_at(game, *o, filter, ctx, last_known_for(game, r, *o, ctx))
                })
        }
        Condition::IsYourTurn => game.state.active_player == ctx.controller,
        Condition::IsMainPhase => game.state.step.is_main(),
        Condition::YouControlAtLeast(n, filter) => {
            let count = zone_objects(game, ZoneId::BATTLEFIELD)
                .into_iter()
                .filter(|o| controller_of(game, *o) == Some(ctx.controller))
                .filter(|o| matches_filter(game, *o, filter, ctx))
                .count();
            count >= *n as usize
        }
        Condition::And(cs) => cs.iter().all(|c| eval_condition(game, c, ctx)),
        Condition::Or(cs) => cs.iter().any(|c| eval_condition(game, c, ctx)),
        Condition::Not(c) => !eval_condition(game, c, ctx),
    }
}

// ---------------------------------------------------------------------------
// Alvos
// ---------------------------------------------------------------------------

/// CR 115.6 — pode ser escolhido como alvo por uma mágica/habilidade de `by`,
/// cuja fonte é `source`.
pub fn can_be_targeted(
    game: &Game,
    obj: ObjectId,
    source: Option<ObjectId>,
    by: PlayerId,
) -> bool {
    let Some(state) = game.state.object(obj) else { return false };
    let Some(ch) = layers::characteristics(game, obj) else { return false };

    // CR 113.6a — hexproof, shroud e proteção são habilidades que só funcionam
    // no campo de batalha. Uma carta no cemitério com hexproof impresso é alvo
    // legal de "devolva uma criatura do cemitério".
    if state.zone.kind != ZoneKind::Battlefield {
        return true;
    }

    // CR 702.18b — shroud impede qualquer alvo, inclusive do controlador.
    if ch.has_keyword(&Keyword::Shroud) {
        return false;
    }
    // CR 702.11b — hexproof só barra oponente do controlador.
    if by != ch.controller && ch.has_keyword(&Keyword::Hexproof) {
        return false;
    }
    // CR 702.16b — proteção de uma cor barra alvo de fonte daquela cor.
    if let Some(src) = source {
        if let Some(src_ch) = layers::characteristics(game, src) {
            for k in &ch.keywords {
                if let Keyword::Protection(color) = k {
                    if src_ch.colors.contains(*color) {
                        return false;
                    }
                }
            }
        }
    }

    // CR 702.21b — ward NÃO impede o alvo: ele dispara uma habilidade que
    // contra-atacar a mágica se o custo não for pago. Quem trata disso é
    // `triggers`, não este ponto; por isso fica de fora de propósito.
    true
}

fn player_is_in_game(game: &Game, p: PlayerId) -> bool {
    game.state
        .players
        .get(p.index())
        .map(|s| !s.has_lost)
        .unwrap_or(false)
}

pub fn legal_targets(game: &Game, spec: &TargetSpec, ctx: &EvalCtx) -> Vec<TargetChoice> {
    let mut out: Vec<TargetChoice> = Vec::new();
    match &spec.kind {
        TargetKind::Object(sel) => push_objects(game, sel, ctx, &mut out),
        TargetKind::Player(pr) => push_players(game, pr, ctx, &mut out),
        TargetKind::ObjectOrPlayer(sel, pr) => {
            push_objects(game, sel, ctx, &mut out);
            push_players(game, pr, ctx, &mut out);
        }
        TargetKind::SpellOnStack(filter) => {
            for obj in spells_on_stack(game) {
                if matches_filter(game, obj, filter, ctx)
                    && can_be_targeted(game, obj, ctx.source, ctx.controller)
                {
                    out.push(TargetChoice::Object(obj));
                }
            }
        }
    }
    // Ordem total e estável: objetos por id, depois jogadores por id. O bot
    // enumera na ordem em que esta lista chega, então ela é parte da semente.
    out.sort_by_key(target_key);
    out.dedup();
    out
}

fn target_key(t: &TargetChoice) -> (u8, u32) {
    match t {
        TargetChoice::Object(o) => (0, o.0),
        TargetChoice::Player(p) => (1, u32::from(p.0)),
    }
}

fn push_objects(game: &Game, sel: &Selector, ctx: &EvalCtx, out: &mut Vec<TargetChoice>) {
    // `max` do seletor descreve quantidade de efeito, não corte de candidatos:
    // truncar aqui esconderia alvo legal do enumerador.
    for obj in gather(game, sel, ctx) {
        if can_be_targeted(game, obj, ctx.source, ctx.controller) {
            out.push(TargetChoice::Object(obj));
        }
    }
}

fn push_players(game: &Game, pr: &PlayerRef, ctx: &EvalCtx, out: &mut Vec<TargetChoice>) {
    for p in resolve_players(game, pr, ctx) {
        if player_is_in_game(game, p) {
            out.push(TargetChoice::Player(p));
        }
    }
}

/// Só mágicas — habilidade na pilha não é objeto da zona e não é alvo de
/// "contra-ataque a mágica alvo".
fn spells_on_stack(game: &Game) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = game
        .state
        .stack
        .iter()
        .filter_map(|item| match item.kind {
            StackItemKind::Spell { object } => Some(object),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// CR 608.2b — na resolução, alvo que deixou de ser legal é ignorado; se todos
/// forem ilegais, a mágica não resolve. Rechecar aqui é o que evita "destrua a
/// criatura alvo" matar uma criatura que ganhou hexproof no meio.
pub fn target_still_legal(
    game: &Game,
    t: TargetChoice,
    spec: &TargetSpec,
    ctx: &EvalCtx,
) -> bool {
    match (&spec.kind, t) {
        (TargetKind::Object(sel), TargetChoice::Object(o)) => object_target_ok(game, o, sel, ctx),
        (TargetKind::Player(pr), TargetChoice::Player(p)) => player_target_ok(game, p, pr, ctx),
        (TargetKind::ObjectOrPlayer(sel, _), TargetChoice::Object(o)) => {
            object_target_ok(game, o, sel, ctx)
        }
        (TargetKind::ObjectOrPlayer(_, pr), TargetChoice::Player(p)) => {
            player_target_ok(game, p, pr, ctx)
        }
        (TargetKind::SpellOnStack(filter), TargetChoice::Object(o)) => {
            spells_on_stack(game).contains(&o)
                && matches_filter(game, o, filter, ctx)
                && can_be_targeted(game, o, ctx.source, ctx.controller)
        }
        // Tipo de alvo trocado (objeto onde se esperava jogador) nunca é legal.
        _ => false,
    }
}

fn object_target_ok(game: &Game, obj: ObjectId, sel: &Selector, ctx: &EvalCtx) -> bool {
    matches_selector(game, obj, sel, ctx)
        && can_be_targeted(game, obj, ctx.source, ctx.controller)
}

fn player_target_ok(game: &Game, p: PlayerId, pr: &PlayerRef, ctx: &EvalCtx) -> bool {
    player_is_in_game(game, p) && resolve_players(game, pr, ctx).contains(&p)
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

/// Leitura de zona sem pânico — `GameState::zone` indexa direto e explodiria
/// numa zona não registrada.
fn zone_objects(game: &Game, z: ZoneId) -> Vec<ObjectId> {
    let key = (z.kind, z.owner.map_or(u8::MAX, |p| p.0));
    game.state
        .zones
        .get(&key)
        .map(|zone| zone.objects.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, CardDatabase, CardDef, StaticAbility, StaticMod};
    use crate::engine::{GameConfig, PlayerConfig};
    use crate::ids::CardDefId;
    use crate::mana::{Color, ManaCost, ManaSymbol};
    use crate::types::{CardType, CounterKind, Rarity, TypeLine};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::sync::Arc;

    fn card(id: u32, name: &str, types: Vec<CardType>, pt: Option<(i32, i32)>) -> CardDef {
        CardDef {
            id: CardDefId(id),
            name: name.to_string(),
            mana_cost: ManaCost::default(),
            type_line: TypeLine { supertypes: Vec::new(), types, subtypes: Vec::new() },
            color_override: None,
            power: pt.map(|(p, _)| p),
            toughness: pt.map(|(_, t)| t),
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

    fn game_with(cards: Vec<CardDef>, deck_size: usize) -> Game {
        let db = Arc::new(CardDatabase { cards });
        let deck: Vec<CardDefId> = (0..deck_size)
            .map(|i| CardDefId((i % db.cards.len()) as u32))
            .collect();
        let players = vec![
            PlayerConfig { name: "A".into(), deck: deck.clone() },
            PlayerConfig { name: "B".into(), deck },
        ];
        let config = GameConfig::default();
        let state = match crate::engine::turn::initial_state(&db, &players, &config) {
            Ok(s) => s,
            Err(e) => panic!("estado inicial de teste inválido: {e}"),
        };
        Game {
            state,
            db,
            rng: ChaCha8Rng::seed_from_u64(1),
            config,
            agents: Vec::new(),
            match_events: Vec::new(),
            decisions_made: 0,
            seed: 1,
        }
    }

    /// N-ésima cópia (base zero) de `def` ainda na biblioteca de `player`. A
    /// biblioteca não foi embaralhada, então a ordem é a do deck — estável.
    fn nth(game: &Game, player: PlayerId, def: CardDefId, n: usize) -> ObjectId {
        let ids = zone_objects(game, ZoneId::library(player));
        match ids
            .into_iter()
            .filter(|id| game.state.object(*id).map(|o| o.card == def).unwrap_or(false))
            .nth(n)
        {
            Some(id) => id,
            None => panic!("{def} não tem {n} cópias na biblioteca de {player}"),
        }
    }

    fn take(game: &mut Game, player: PlayerId, def: CardDefId) -> ObjectId {
        nth(game, player, def, 0)
    }

    fn to_zone(game: &mut Game, id: ObjectId, to: ZoneId, ts: u64) {
        let from = match game.state.object(id) {
            Some(o) => o.zone,
            None => panic!("objeto {id} não existe"),
        };
        let from_key = (from.kind, from.owner.map_or(u8::MAX, |p| p.0));
        let Some(zone) = game.state.zones.get_mut(&from_key) else {
            panic!("zona de origem {from_key:?} não existe")
        };
        zone.remove(id);
        let Some(obj) = game.state.object_mut(id) else {
            panic!("objeto {id} sumiu no meio da montagem")
        };
        obj.zone = to;
        obj.timestamp = ts;
        let to_key = (to.kind, to.owner.map_or(u8::MAX, |p| p.0));
        let Some(zone) = game.state.zones.get_mut(&to_key) else {
            panic!("zona de destino {to_key:?} não existe")
        };
        zone.push_bottom(id);
    }

    fn battlefield(game: &mut Game, id: ObjectId, ts: u64) {
        to_zone(game, id, ZoneId::BATTLEFIELD, ts);
    }

    fn ctx_of(source: ObjectId, controller: PlayerId) -> EvalCtx {
        EvalCtx::for_source(source, controller)
    }

    // --- filtros ---------------------------------------------------------

    #[test]
    fn filtro_de_palavra_chave_le_a_camada_e_nao_o_carddef() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut lord = card(1, "Wind Lord", vec![CardType::Enchantment], None);
        lord.abilities.push(Ability::Static(StaticAbility {
            condition: Condition::Always,
            affects: Selector::creatures().yours(),
            modification: StaticMod::GrantKeywords(vec![Keyword::Flying]),
            text: "Creatures you control have flying.".to_string(),
        }));
        let mut game = game_with(vec![bear, lord], 8);
        let bear_id = take(&mut game, PlayerId::P0, CardDefId(0));
        let lord_id = take(&mut game, PlayerId::P0, CardDefId(1));
        battlefield(&mut game, bear_id, 5);
        battlefield(&mut game, lord_id, 6);

        let ctx = ctx_of(lord_id, PlayerId::P0);
        // O CardDef do urso não tem Flying; a camada 6 tem.
        assert!(matches_filter(
            &game,
            bear_id,
            &Filter::HasKeyword(Keyword::Flying),
            &ctx
        ));
    }

    #[test]
    fn filtro_de_poder_le_anthem_e_marcador() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let id = take(&mut game, PlayerId::P0, CardDefId(0));
        battlefield(&mut game, id, 5);
        let Some(o) = game.state.object_mut(id) else {
            panic!("criatura de teste não existe")
        };
        o.add_counter(CounterKind::PlusOnePlusOne, 2);
        let ctx = ctx_of(id, PlayerId::P0);
        assert!(matches_filter(&game, id, &Filter::PowerAtLeast(4), &ctx));
        assert!(!matches_filter(&game, id, &Filter::PowerAtLeast(5), &ctx));
        assert!(matches_filter(&game, id, &Filter::ToughnessAtMost(4), &ctx));
    }

    #[test]
    fn is_self_e_is_other_sao_relativos_a_fonte() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let a = take(&mut game, PlayerId::P0, CardDefId(0));
        let b = take(&mut game, PlayerId::P1, CardDefId(0));
        battlefield(&mut game, a, 5);
        battlefield(&mut game, b, 6);

        let ctx = ctx_of(a, PlayerId::P0);
        assert!(matches_filter(&game, a, &Filter::IsSelf, &ctx));
        assert!(!matches_filter(&game, a, &Filter::IsOther, &ctx));
        assert!(!matches_filter(&game, b, &Filter::IsSelf, &ctx));
        assert!(matches_filter(&game, b, &Filter::IsOther, &ctx));
    }

    #[test]
    fn and_vazio_casa_e_or_vazio_nao() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let id = take(&mut game, PlayerId::P0, CardDefId(0));
        battlefield(&mut game, id, 1);
        let ctx = ctx_of(id, PlayerId::P0);
        assert!(matches_filter(&game, id, &Filter::And(Vec::new()), &ctx));
        assert!(!matches_filter(&game, id, &Filter::Or(Vec::new()), &ctx));
    }

    // --- seletores -------------------------------------------------------

    #[test]
    fn anywhere_varre_todas_as_zonas_e_devolve_ordenado() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let a = nth(&game, PlayerId::P0, CardDefId(0), 0);
        let b = nth(&game, PlayerId::P0, CardDefId(0), 1);
        let c = take(&mut game, PlayerId::P1, CardDefId(0));
        // Fica um em cada zona diferente da biblioteca.
        battlefield(&mut game, b, 2);
        to_zone(&mut game, c, ZoneId::graveyard(PlayerId::P1), 3);

        let ctx = ctx_of(a, PlayerId::P0);
        let campo = select(&game, &Selector::creatures(), &ctx);
        assert_eq!(campo, vec![b]);

        let todos = select(
            &game,
            &Selector {
                zone: ZoneScope::Anywhere,
                filter: Filter::creature(),
                owner_scope: None,
                max: None,
            },
            &ctx,
        );
        assert!(todos.contains(&a) && todos.contains(&b) && todos.contains(&c));
        let mut ordenado = todos.clone();
        ordenado.sort_unstable();
        assert_eq!(todos, ordenado, "select precisa devolver ordenado");
    }

    #[test]
    fn owner_scope_separa_seu_do_oponente() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let meu = take(&mut game, PlayerId::P0, CardDefId(0));
        let dele = take(&mut game, PlayerId::P1, CardDefId(0));
        battlefield(&mut game, meu, 1);
        battlefield(&mut game, dele, 2);

        let ctx = ctx_of(meu, PlayerId::P0);
        assert_eq!(select(&game, &Selector::creatures().yours(), &ctx), vec![meu]);
        assert_eq!(
            select(&game, &Selector::creatures().opponents(), &ctx),
            vec![dele]
        );
    }

    #[test]
    fn max_do_seletor_corta_mas_gather_nao() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let a = nth(&game, PlayerId::P0, CardDefId(0), 0);
        let b = nth(&game, PlayerId::P0, CardDefId(0), 1);
        battlefield(&mut game, a, 1);
        battlefield(&mut game, b, 2);

        let ctx = ctx_of(a, PlayerId::P0);
        let mut sel = Selector::creatures();
        sel.max = Some(1);
        assert_eq!(select(&game, &sel, &ctx).len(), 1);
        assert_eq!(gather(&game, &sel, &ctx).len(), 2);
    }

    // --- valores e condições ---------------------------------------------

    #[test]
    fn valores_contam_e_somam_sem_estourar() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let a = nth(&game, PlayerId::P0, CardDefId(0), 0);
        let b = nth(&game, PlayerId::P0, CardDefId(0), 1);
        battlefield(&mut game, a, 1);
        battlefield(&mut game, b, 2);
        let ctx = ctx_of(a, PlayerId::P0);

        assert_eq!(eval_value(&game, &Value::Count(Selector::creatures()), &ctx), 2);
        assert_eq!(
            eval_value(&game, &Value::PowerOf(ObjRef::SelfObject), &ctx),
            2
        );
        // Poder somado de todas as criaturas.
        assert_eq!(
            eval_value(
                &game,
                &Value::PowerOf(ObjRef::All(Selector::creatures())),
                &ctx
            ),
            4
        );
        assert_eq!(eval_value(&game, &Value::LifeOf(PlayerRef::You), &ctx), 20);
        let estouro = Value::Mul(
            Box::new(Value::Const(i32::MAX)),
            Box::new(Value::Const(2)),
        );
        assert_eq!(eval_value(&game, &estouro, &ctx), i32::MAX);
    }

    #[test]
    fn condicoes_leem_estado_corrente() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let a = take(&mut game, PlayerId::P0, CardDefId(0));
        battlefield(&mut game, a, 1);
        let ctx = ctx_of(a, PlayerId::P0);

        assert!(eval_condition(&game, &Condition::Always, &ctx));
        assert!(!eval_condition(&game, &Condition::Never, &ctx));
        assert!(eval_condition(
            &game,
            &Condition::Exists(Selector::creatures()),
            &ctx
        ));
        assert!(eval_condition(
            &game,
            &Condition::YouControlAtLeast(1, Filter::creature()),
            &ctx
        ));
        assert!(!eval_condition(
            &game,
            &Condition::YouControlAtLeast(2, Filter::creature()),
            &ctx
        ));
        // Jogador 0 é o ativo no estado inicial.
        assert!(eval_condition(&game, &Condition::IsYourTurn, &ctx));
        let ctx_opp = ctx_of(a, PlayerId::P1);
        assert!(!eval_condition(&game, &Condition::IsYourTurn, &ctx_opp));
    }

    // --- alvos -----------------------------------------------------------

    #[test]
    fn hexproof_barra_oponente_e_shroud_barra_todos() {
        let mut hexy = card(0, "Hexy", vec![CardType::Creature], Some((1, 1)));
        hexy.abilities.push(Ability::Keyword(Keyword::Hexproof));
        let mut shy = card(1, "Shy", vec![CardType::Creature], Some((1, 1)));
        shy.abilities.push(Ability::Keyword(Keyword::Shroud));
        let mut game = game_with(vec![hexy, shy], 8);

        let h = take(&mut game, PlayerId::P0, CardDefId(0));
        let s = take(&mut game, PlayerId::P0, CardDefId(1));
        battlefield(&mut game, h, 1);
        battlefield(&mut game, s, 2);

        // Controlador pode mirar o próprio hexproof; oponente não.
        assert!(can_be_targeted(&game, h, None, PlayerId::P0));
        assert!(!can_be_targeted(&game, h, None, PlayerId::P1));
        // Shroud barra os dois, inclusive o controlador.
        assert!(!can_be_targeted(&game, s, None, PlayerId::P0));
        assert!(!can_be_targeted(&game, s, None, PlayerId::P1));
    }

    #[test]
    fn protecao_barra_fonte_da_cor() {
        let mut prot = card(0, "Protected", vec![CardType::Creature], Some((1, 1)));
        prot.abilities
            .push(Ability::Keyword(Keyword::Protection(Color::Red)));
        let mut bolt = card(1, "Bolt", vec![CardType::Instant], None);
        bolt.mana_cost = ManaCost { symbols: vec![ManaSymbol::Colored(Color::Red)] };
        let mut white = card(2, "Wrath", vec![CardType::Instant], None);
        white.mana_cost = ManaCost { symbols: vec![ManaSymbol::Colored(Color::White)] };

        let mut game = game_with(vec![prot, bolt, white], 9);
        let p = take(&mut game, PlayerId::P0, CardDefId(0));
        let red = take(&mut game, PlayerId::P1, CardDefId(1));
        let wht = take(&mut game, PlayerId::P1, CardDefId(2));
        battlefield(&mut game, p, 1);

        assert!(!can_be_targeted(&game, p, Some(red), PlayerId::P1));
        assert!(can_be_targeted(&game, p, Some(wht), PlayerId::P1));
        // Sem fonte conhecida não há como aplicar proteção de cor.
        assert!(can_be_targeted(&game, p, None, PlayerId::P1));
    }

    #[test]
    fn hexproof_so_funciona_no_campo() {
        let mut hexy = card(0, "Hexy", vec![CardType::Creature], Some((1, 1)));
        hexy.abilities.push(Ability::Keyword(Keyword::Hexproof));
        let mut game = game_with(vec![hexy], 8);
        let h = take(&mut game, PlayerId::P0, CardDefId(0));
        to_zone(&mut game, h, ZoneId::graveyard(PlayerId::P0), 1);
        // CR 113.6a — no cemitério a palavra-chave não funciona.
        assert!(can_be_targeted(&game, h, None, PlayerId::P1));
    }

    #[test]
    fn ward_nao_impede_o_alvo() {
        let mut warded = card(0, "Warded", vec![CardType::Creature], Some((1, 1)));
        warded.abilities.push(Ability::Keyword(Keyword::Ward(Box::new(
            crate::ir::Cost::Free,
        ))));
        let mut game = game_with(vec![warded], 8);
        let w = take(&mut game, PlayerId::P0, CardDefId(0));
        battlefield(&mut game, w, 1);
        assert!(can_be_targeted(&game, w, None, PlayerId::P1));
    }

    #[test]
    fn legal_targets_vem_ordenado_e_sem_repeticao() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let a = nth(&game, PlayerId::P0, CardDefId(0), 0);
        let b = nth(&game, PlayerId::P0, CardDefId(0), 1);
        let c = take(&mut game, PlayerId::P1, CardDefId(0));
        battlefield(&mut game, a, 1);
        battlefield(&mut game, b, 2);
        battlefield(&mut game, c, 3);

        let spec = TargetSpec {
            // Mesma criatura cabe nos dois ramos: o dedup é o que evita
            // enumerar o mesmo alvo duas vezes.
            kind: TargetKind::ObjectOrPlayer(Selector::creatures(), PlayerRef::Each),
            description: "alvo de criatura ou jogador".to_string(),
        };
        let ctx = ctx_of(a, PlayerId::P0);
        let alvos = legal_targets(&game, &spec, &ctx);

        let mut esperado = alvos.clone();
        esperado.sort_by_key(target_key);
        esperado.dedup();
        assert_eq!(alvos, esperado);
        assert!(alvos.contains(&TargetChoice::Object(a)));
        assert!(alvos.contains(&TargetChoice::Player(PlayerId::P0)));
        assert!(alvos.contains(&TargetChoice::Player(PlayerId::P1)));
        // Objetos antes de jogadores.
        assert!(matches!(alvos.first(), Some(TargetChoice::Object(_))));
    }

    #[test]
    fn target_still_legal_derruba_alvo_que_saiu_e_alvo_de_tipo_errado() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let alvo = take(&mut game, PlayerId::P0, CardDefId(0));
        let fonte = take(&mut game, PlayerId::P1, CardDefId(0));
        battlefield(&mut game, alvo, 1);

        let spec = TargetSpec {
            kind: TargetKind::Object(Selector::creatures()),
            description: "alvo de criatura".to_string(),
        };
        let ctx = ctx_of(fonte, PlayerId::P1);
        assert!(target_still_legal(
            &game,
            TargetChoice::Object(alvo),
            &spec,
            &ctx
        ));
        // Jogador onde se espera objeto nunca é legal.
        assert!(!target_still_legal(
            &game,
            TargetChoice::Player(PlayerId::P0),
            &spec,
            &ctx
        ));

        // CR 608.2b — saiu do campo, alvo deixa de ser legal.
        to_zone(&mut game, alvo, ZoneId::graveyard(PlayerId::P0), 9);
        assert!(!target_still_legal(
            &game,
            TargetChoice::Object(alvo),
            &spec,
            &ctx
        ));
    }

    #[test]
    fn resolve_players_each_segue_ordem_de_turno() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 6);
        let a = take(&mut game, PlayerId::P0, CardDefId(0));
        let ctx = ctx_of(a, PlayerId::P0);
        assert_eq!(
            resolve_players(&game, &PlayerRef::Each, &ctx),
            vec![PlayerId::P0, PlayerId::P1]
        );
        game.state.active_player = PlayerId::P1;
        assert_eq!(
            resolve_players(&game, &PlayerRef::Each, &ctx),
            vec![PlayerId::P1, PlayerId::P0]
        );
        assert_eq!(
            resolve_players(&game, &PlayerRef::Opponents, &ctx),
            vec![PlayerId::P1]
        );
    }

    #[test]
    fn resolve_objects_cobre_as_referencias_do_contexto() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let fonte = nth(&game, PlayerId::P0, CardDefId(0), 0);
        let anexo = nth(&game, PlayerId::P0, CardDefId(0), 1);
        battlefield(&mut game, fonte, 1);
        battlefield(&mut game, anexo, 2);
        let Some(o) = game.state.object_mut(fonte) else {
            panic!("fonte de teste não existe")
        };
        o.attached_to = Some(anexo);

        let mut ctx = ctx_of(fonte, PlayerId::P0);
        ctx.targets = vec![TargetChoice::Object(anexo)];
        ctx.selected = Some(anexo);
        ctx.remembered = vec![anexo];
        ctx.trigger.trigger_object = Some(anexo);
        ctx.trigger.trigger_source = Some(fonte);

        assert_eq!(resolve_objects(&game, &ObjRef::SelfObject, &ctx), vec![fonte]);
        assert_eq!(resolve_objects(&game, &ObjRef::Target(0), &ctx), vec![anexo]);
        assert_eq!(resolve_objects(&game, &ObjRef::Selected, &ctx), vec![anexo]);
        assert_eq!(resolve_objects(&game, &ObjRef::Attached, &ctx), vec![anexo]);
        assert_eq!(
            resolve_objects(&game, &ObjRef::TriggerObject, &ctx),
            vec![anexo]
        );
        assert_eq!(
            resolve_objects(&game, &ObjRef::TriggerSource, &ctx),
            vec![fonte]
        );
        assert_eq!(
            resolve_objects(&game, &ObjRef::Remembered(0), &ctx),
            vec![anexo]
        );
        // Índice fora da faixa devolve vazio em vez de entrar em pânico.
        assert!(resolve_objects(&game, &ObjRef::Target(9), &ctx).is_empty());
        assert!(resolve_objects(&game, &ObjRef::Remembered(9), &ctx).is_empty());
    }

    #[test]
    fn objeto_inexistente_nao_derruba_nada() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let game = game_with(vec![bear], 6);
        let fantasma = ObjectId(9_999);
        let ctx = EvalCtx::default();
        assert!(!matches_filter(&game, fantasma, &Filter::Any, &ctx));
        assert!(!matches_filter(&game, fantasma, &Filter::creature(), &ctx));
        assert!(!can_be_targeted(&game, fantasma, None, PlayerId::P0));
        assert!(layers::characteristics(&game, fantasma).is_none());
    }
}
