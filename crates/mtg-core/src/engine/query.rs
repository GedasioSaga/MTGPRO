//! Leitura pura do estado: filtros, seletores, valores, condições e alvos.
//!
//! Nada aqui muta `Game`. Todo o resto do motor lê o estado por estas funções,
//! e nenhuma delas consulta `CardDef` para decidir regra — característica atual
//! vem sempre de `layers::characteristics`, senão um anthem ou um efeito de
//! "torna-se 0/1" seria invisível para filtro, alvo e condição.
use super::{layers, Game};
use crate::action::TargetChoice;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{
    Cmp, Condition, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec, Value,
    ZoneScope,
};
use crate::state::{StackItemKind, TriggerContext};
use crate::zone::ZoneKind;

// ---------------------------------------------------------------------------
// Contexto de avaliação
// ---------------------------------------------------------------------------

/// Tudo que um efeito precisa saber sobre "de onde ele veio" para se avaliar.
///
/// É deliberadamente um valor simples e clonável: o interpretador do IR entra e
/// sai de subcontextos (`ForEach`, `Repeat`, modo escolhido) o tempo todo, e
/// carregar referência com tempo de vida aqui tornaria isso inviável.
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
        EvalCtx {
            source: Some(source),
            controller,
            ..EvalCtx::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Utilidades internas de zona
// ---------------------------------------------------------------------------

/// Acesso a zona sem indexar o `BTreeMap` — `GameState::zone` entra em pânico
/// quando a zona não existe, e leitura nunca pode derrubar a simulação.
fn zone_objects(game: &Game, kind: ZoneKind, owner: Option<PlayerId>) -> &[ObjectId] {
    let key = (kind, owner.map_or(u8::MAX, |p| p.0));
    game.state
        .zones
        .get(&key)
        .map(|z| z.objects.as_slice())
        .unwrap_or(&[])
}

/// Zonas concretas cobertas por um escopo. Ordem fixa: o simulador precisa que
/// duas execuções com a mesma semente vejam a mesma lista na mesma ordem.
fn zones_in_scope(game: &Game, scope: ZoneScope) -> Vec<(ZoneKind, Option<PlayerId>)> {
    let players: Vec<PlayerId> = game.state.players.iter().map(|p| p.id).collect();
    match scope {
        ZoneScope::Battlefield => vec![(ZoneKind::Battlefield, None)],
        ZoneScope::Exile => vec![(ZoneKind::Exile, None)],
        ZoneScope::Stack => vec![(ZoneKind::Stack, None)],
        ZoneScope::Graveyard => players
            .into_iter()
            .map(|p| (ZoneKind::Graveyard, Some(p)))
            .collect(),
        ZoneScope::Hand => players
            .into_iter()
            .map(|p| (ZoneKind::Hand, Some(p)))
            .collect(),
        ZoneScope::Library => players
            .into_iter()
            .map(|p| (ZoneKind::Library, Some(p)))
            .collect(),
        // CR 400.1 — "em qualquer lugar" varre as sete zonas, não só o campo.
        ZoneScope::Anywhere => {
            let mut v = vec![(ZoneKind::Battlefield, None)];
            for p in &players {
                v.push((ZoneKind::Hand, Some(*p)));
                v.push((ZoneKind::Library, Some(*p)));
                v.push((ZoneKind::Graveyard, Some(*p)));
                v.push((ZoneKind::Command, Some(*p)));
            }
            v.push((ZoneKind::Exile, None));
            v.push((ZoneKind::Stack, None));
            v
        }
    }
}

fn zone_in_scope(kind: ZoneKind, scope: ZoneScope) -> bool {
    match scope {
        ZoneScope::Anywhere => true,
        ZoneScope::Battlefield => kind == ZoneKind::Battlefield,
        ZoneScope::Graveyard => kind == ZoneKind::Graveyard,
        ZoneScope::Hand => kind == ZoneKind::Hand,
        ZoneScope::Library => kind == ZoneKind::Library,
        ZoneScope::Exile => kind == ZoneKind::Exile,
        ZoneScope::Stack => kind == ZoneKind::Stack,
    }
}

fn push_unique<T: PartialEq>(v: &mut Vec<T>, item: T) {
    if !v.contains(&item) {
        v.push(item);
    }
}

// ---------------------------------------------------------------------------
// Filtros
// ---------------------------------------------------------------------------

/// Predicado do IR aplicado a um objeto concreto.
///
/// Objeto inexistente nunca casa — inclusive dentro de `Not`, porque a checagem
/// de existência acontece antes da recursão em cada nível.
pub fn matches_filter(game: &Game, obj: ObjectId, filter: &Filter, ctx: &EvalCtx) -> bool {
    let Some(o) = game.state.object(obj) else {
        return false;
    };
    // Característica derivada só é calculada no ramo que precisa dela: passar
    // pelas camadas é caro e a maioria dos filtros olha só estado bruto.
    let chars = || layers::characteristics(game, obj);

    match filter {
        Filter::Any => true,
        Filter::And(fs) => fs.iter().all(|f| matches_filter(game, obj, f, ctx)),
        Filter::Or(fs) => fs.iter().any(|f| matches_filter(game, obj, f, ctx)),
        Filter::Not(f) => !matches_filter(game, obj, f, ctx),

        Filter::HasType(t) => chars().is_some_and(|c| c.type_line.has_type(*t)),
        Filter::HasSubtype(s) => chars().is_some_and(|c| c.type_line.has_subtype(s)),
        Filter::HasSupertype(s) => chars().is_some_and(|c| c.type_line.has_supertype(*s)),
        Filter::HasName(n) => chars().is_some_and(|c| c.name.eq_ignore_ascii_case(n)),
        Filter::HasColor(col) => chars().is_some_and(|c| c.colors.contains(*col)),
        Filter::Colorless => chars().is_some_and(|c| c.colors.is_colorless()),
        Filter::Multicolored => chars().is_some_and(|c| c.colors.is_multicolored()),
        // Palavra-chave concedida por camada 6 conta igual à impressa (CR 613.1f).
        Filter::HasKeyword(k) => chars().is_some_and(|c| c.has_keyword(k)),
        Filter::HasCounter(k) => o.counter(k) > 0,

        Filter::Tapped => o.tapped,
        Filter::Untapped => !o.tapped,
        Filter::Attacking => o.combat.is_attacking(),
        Filter::Blocking => o.combat.is_blocking(),
        // CR 509.1h — segue "bloqueada" mesmo se os bloqueadores saírem.
        Filter::Blocked => o.combat.is_attacking() && o.combat.was_blocked,
        Filter::Unblocked => o.combat.is_attacking() && !o.combat.was_blocked,
        Filter::Token => o.is_token,
        Filter::NonToken => !o.is_token,
        // Flag bruta de CR 302.6: pressa é checada por quem vai agir, não aqui.
        Filter::SummoningSick => o.summoning_sick,

        Filter::PowerAtLeast(n) => chars().is_some_and(|c| c.power >= *n),
        Filter::PowerAtMost(n) => chars().is_some_and(|c| c.power <= *n),
        Filter::ToughnessAtLeast(n) => chars().is_some_and(|c| c.toughness >= *n),
        Filter::ToughnessAtMost(n) => chars().is_some_and(|c| c.toughness <= *n),
        Filter::ManaValueAtLeast(n) => chars().is_some_and(|c| c.mana_value >= *n),
        Filter::ManaValueAtMost(n) => chars().is_some_and(|c| c.mana_value <= *n),
        Filter::ManaValueExactly(n) => chars().is_some_and(|c| c.mana_value == *n),

        Filter::ControlledBy(r) => resolve_players(game, r, ctx).contains(&o.controller),
        Filter::OwnedBy(r) => resolve_players(game, r, ctx).contains(&o.owner),
        Filter::IsSelf => ctx.source == Some(obj),
        // "Outra criatura" é relativo à fonte do efeito, nunca ao alvo.
        Filter::IsOther => ctx.source != Some(obj),
        Filter::Targetable => can_be_targeted(game, obj, ctx.source, ctx.controller),
    }
}

/// Casa zona, escopo de dono e filtro de um `Selector` contra um objeto.
///
/// Adição ao contrato: `layers` precisa saber se um objeto está sob o alcance da
/// habilidade estática sem materializar a lista inteira a cada recálculo.
/// Ignora `Selector::max`, que é limite de cardinalidade da lista, não predicado.
pub fn matches_selector(game: &Game, obj: ObjectId, sel: &Selector, ctx: &EvalCtx) -> bool {
    let Some(o) = game.state.object(obj) else {
        return false;
    };
    if !zone_in_scope(o.zone.kind, sel.zone) {
        return false;
    }
    if let Some(scope) = &sel.owner_scope {
        // Em zona compartilhada quem manda é o controlador; nas privadas, o dono.
        let who = if o.zone.kind.is_shared() {
            o.controller
        } else {
            o.owner
        };
        if !resolve_players(game, scope, ctx).contains(&who) {
            return false;
        }
    }
    matches_filter(game, obj, &sel.filter, ctx)
}

/// Todos os objetos descritos por um seletor, em ordem estável de zona.
pub fn select(game: &Game, sel: &Selector, ctx: &EvalCtx) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = Vec::new();
    for (kind, owner) in zones_in_scope(game, sel.zone) {
        for id in zone_objects(game, kind, owner) {
            if out.contains(id) || !matches_selector(game, *id, sel, ctx) {
                continue;
            }
            out.push(*id);
            if sel.max.is_some_and(|m| out.len() >= m as usize) {
                return out;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Referências
// ---------------------------------------------------------------------------

pub fn resolve_players(game: &Game, r: &PlayerRef, ctx: &EvalCtx) -> Vec<PlayerId> {
    match r {
        PlayerRef::You => vec![ctx.controller],
        PlayerRef::Opponents => game.state.opponents(ctx.controller),
        // CR 101.4 — ordem de turno a partir do jogador ativo.
        PlayerRef::Each => {
            let n = game.state.players.len();
            if n == 0 {
                return Vec::new();
            }
            let start = game.state.active_player.index() % n;
            (0..n)
                .map(|i| game.state.players[(start + i) % n].id)
                .collect()
        }
        PlayerRef::Target(i) => match ctx.targets.get(*i as usize) {
            Some(TargetChoice::Player(p)) => vec![*p],
            _ => Vec::new(),
        },
        PlayerRef::ControllerOf(inner) => {
            let mut out = Vec::new();
            for id in resolve_objects(game, inner, ctx) {
                if let Some(o) = game.state.object(id) {
                    push_unique(&mut out, o.controller);
                }
            }
            out
        }
        PlayerRef::OwnerOf(inner) => {
            let mut out = Vec::new();
            for id in resolve_objects(game, inner, ctx) {
                if let Some(o) = game.state.object(id) {
                    push_unique(&mut out, o.owner);
                }
            }
            out
        }
        PlayerRef::ActivePlayer => vec![game.state.active_player],
    }
}

pub fn resolve_objects(game: &Game, r: &ObjRef, ctx: &EvalCtx) -> Vec<ObjectId> {
    match r {
        ObjRef::SelfObject => ctx.source.into_iter().collect(),
        ObjRef::Target(i) => match ctx.targets.get(*i as usize) {
            Some(TargetChoice::Object(o)) => vec![*o],
            _ => Vec::new(),
        },
        ObjRef::Selected => ctx.selected.into_iter().collect(),
        ObjRef::Attached => ctx
            .source
            .and_then(|s| game.state.object(s))
            .and_then(|o| o.attached_to)
            .into_iter()
            .collect(),
        ObjRef::TriggerObject => ctx.trigger.trigger_object.into_iter().collect(),
        ObjRef::TriggerSource => ctx.trigger.trigger_source.into_iter().collect(),
        ObjRef::All(sel) => select(game, sel, ctx),
        ObjRef::Remembered(i) => ctx.remembered.get(*i as usize).copied().into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// Valores e condições
// ---------------------------------------------------------------------------

/// Soma uma característica sobre todos os objetos de uma referência.
/// Referência simples devolve o valor dela; `ObjRef::All` devolve o total, que é
/// exatamente o idioma "igual ao poder total das criaturas que você controla".
fn sum_over(game: &Game, r: &ObjRef, ctx: &EvalCtx, f: impl Fn(&super::Characteristics) -> i32) -> i32 {
    resolve_objects(game, r, ctx)
        .into_iter()
        .filter_map(|id| layers::characteristics(game, id))
        .fold(0i32, |acc, c| acc.saturating_add(f(&c)))
}

pub fn eval_value(game: &Game, v: &Value, ctx: &EvalCtx) -> i32 {
    match v {
        Value::Const(n) => *n,
        Value::X => ctx.x as i32,
        Value::Count(sel) => select(game, sel, ctx).len() as i32,
        Value::PowerOf(r) => sum_over(game, r, ctx, |c| c.power),
        Value::ToughnessOf(r) => sum_over(game, r, ctx, |c| c.toughness),
        Value::ManaValueOf(r) => sum_over(game, r, ctx, |c| c.mana_value as i32),
        Value::LifeOf(p) => resolve_players(game, p, ctx)
            .into_iter()
            .filter_map(|id| game.state.players.get(id.index()))
            .fold(0i32, |acc, p| acc.saturating_add(p.life)),
        Value::CardsInHandOf(p) => resolve_players(game, p, ctx)
            .into_iter()
            .fold(0i32, |acc, id| {
                acc.saturating_add(zone_objects(game, ZoneKind::Hand, Some(id)).len() as i32)
            }),
        Value::CountersOn(r, kind) => resolve_objects(game, r, ctx)
            .into_iter()
            .filter_map(|id| game.state.object(id))
            .fold(0i32, |acc, o| acc.saturating_add(o.counter(kind))),
        Value::ChosenNumber => ctx.chosen_number,
        // Saturante em vez de aritmética comum: carta com X gigante não pode
        // derrubar a simulação por overflow em build de debug.
        Value::Add(a, b) => eval_value(game, a, ctx).saturating_add(eval_value(game, b, ctx)),
        Value::Sub(a, b) => eval_value(game, a, ctx).saturating_sub(eval_value(game, b, ctx)),
        Value::Mul(a, b) => eval_value(game, a, ctx).saturating_mul(eval_value(game, b, ctx)),
        Value::Neg(a) => eval_value(game, a, ctx).saturating_neg(),
        Value::Max(a, b) => eval_value(game, a, ctx).max(eval_value(game, b, ctx)),
        Value::Min(a, b) => eval_value(game, a, ctx).min(eval_value(game, b, ctx)),
    }
}

pub fn eval_condition(game: &Game, c: &Condition, ctx: &EvalCtx) -> bool {
    match c {
        Condition::Always => true,
        Condition::Never => false,
        Condition::Compare(a, cmp, b) => {
            let (x, y) = (eval_value(game, a, ctx), eval_value(game, b, ctx));
            match cmp {
                Cmp::Eq => x == y,
                Cmp::Ne => x != y,
                Cmp::Lt => x < y,
                Cmp::Le => x <= y,
                Cmp::Gt => x > y,
                Cmp::Ge => x >= y,
            }
        }
        Condition::Exists(sel) => !select(game, sel, ctx).is_empty(),
        // Referência vazia nunca satisfaz: "se o alvo for uma criatura" com alvo
        // já removido é falso, não vacuamente verdadeiro.
        Condition::Matches(r, f) => {
            let objs = resolve_objects(game, r, ctx);
            !objs.is_empty() && objs.iter().all(|id| matches_filter(game, *id, f, ctx))
        }
        Condition::IsYourTurn => game.state.active_player == ctx.controller,
        Condition::IsMainPhase => game.state.step.is_main(),
        Condition::YouControlAtLeast(n, f) => {
            let count = zone_objects(game, ZoneKind::Battlefield, None)
                .iter()
                .filter(|id| {
                    game.state
                        .object(**id)
                        .is_some_and(|o| o.controller == ctx.controller)
                        && matches_filter(game, **id, f, ctx)
                })
                .count();
            count >= *n as usize
        }
        Condition::And(cs) => cs.iter().all(|c| eval_condition(game, c, ctx)),
        Condition::Or(cs) => cs.iter().any(|c| eval_condition(game, c, ctx)),
        Condition::Not(inner) => !eval_condition(game, inner, ctx),
    }
}

// ---------------------------------------------------------------------------
// Alvos
// ---------------------------------------------------------------------------

/// CR 115.6 e 702.16/702.18 — quem pode ser escolhido como alvo.
///
/// - Hexproof (CR 702.11b) só barra fonte de **oponente**: o controlador ainda
///   pode mirar o próprio permanente.
/// - Shroud (CR 702.18a) barra todo mundo, inclusive o controlador.
/// - Proteção de cor (CR 702.16b) barra fonte daquela cor.
/// - **Ward não entra aqui.** Ward (CR 702.21) não torna o objeto alvo ilegal:
///   ele cria uma habilidade disparada que contra-atacar a mágica se o custo não
///   for pago. Tratar ward aqui removeria alvos legais e quebraria o jogo.
pub fn can_be_targeted(game: &Game, obj: ObjectId, source: Option<ObjectId>, by: PlayerId) -> bool {
    let Some(chars) = layers::characteristics(game, obj) else {
        return false;
    };
    if chars.has_keyword(&Keyword::Shroud) {
        return false;
    }
    if chars.has_keyword(&Keyword::Hexproof) && chars.controller != by {
        return false;
    }
    let source_colors = source
        .and_then(|s| layers::characteristics(game, s))
        .map(|c| c.colors);
    if let Some(colors) = source_colors {
        for k in &chars.keywords {
            if let Keyword::Protection(color) = k {
                if colors.contains(*color) {
                    return false;
                }
            }
        }
    }
    true
}

/// Objetos-mágica que podem ser alvo de "contra-atacar mágica alvo".
/// Habilidade na pilha fica de fora: a variante do IR se chama `SpellOnStack`,
/// e habilidade não tem `ObjectState` próprio para casar filtro.
fn spells_on_stack(game: &Game) -> Vec<ObjectId> {
    game.state
        .stack
        .iter()
        .filter_map(|item| match &item.kind {
            StackItemKind::Spell { object } => Some(*object),
            StackItemKind::CopiedSpell { .. } => Some(item.id),
            _ => None,
        })
        .collect()
}

/// Alvos legais para uma exigência. Lista deduplicada e ordenada por id — o
/// simulador enumera combinações a partir daqui, e ordem instável tornaria a
/// partida não reproduzível com a mesma semente.
pub fn legal_targets(game: &Game, spec: &TargetSpec, ctx: &EvalCtx) -> Vec<TargetChoice> {
    let mut out: Vec<TargetChoice> = Vec::new();

    let add_objects = |sel: &Selector, out: &mut Vec<TargetChoice>| {
        for id in select(game, sel, ctx) {
            if can_be_targeted(game, id, ctx.source, ctx.controller) {
                out.push(TargetChoice::Object(id));
            }
        }
    };
    let add_players = |r: &PlayerRef, out: &mut Vec<TargetChoice>| {
        for p in resolve_players(game, r, ctx) {
            // Jogador que já perdeu saiu do jogo (CR 104.3) — não é alvo.
            if game.state.players.get(p.index()).is_some_and(|s| !s.has_lost) {
                out.push(TargetChoice::Player(p));
            }
        }
    };

    match &spec.kind {
        TargetKind::Object(sel) => add_objects(sel, &mut out),
        TargetKind::Player(r) => add_players(r, &mut out),
        TargetKind::ObjectOrPlayer(sel, r) => {
            add_objects(sel, &mut out);
            add_players(r, &mut out);
        }
        TargetKind::SpellOnStack(f) => {
            for id in spells_on_stack(game) {
                // Uma mágica não é alvo legal de si mesma (CR 601.2c).
                if ctx.source == Some(id) {
                    continue;
                }
                if matches_filter(game, id, f, ctx)
                    && can_be_targeted(game, id, ctx.source, ctx.controller)
                {
                    out.push(TargetChoice::Object(id));
                }
            }
        }
    }

    out.sort_by_key(|t| match t {
        TargetChoice::Object(o) => (0u8, o.0, 0u8),
        TargetChoice::Player(p) => (1u8, 0u32, p.0),
    });
    out.dedup();
    out
}

/// CR 608.2b — na resolução, cada alvo é reconferido. Alvo que ficou ilegal é
/// ignorado; se **todos** ficaram ilegais quem resolve não faz nada (essa
/// segunda parte é decisão de `stack::resolve_top`, que chama esta função).
pub fn target_still_legal(
    game: &Game,
    t: TargetChoice,
    spec: &TargetSpec,
    ctx: &EvalCtx,
) -> bool {
    match (&spec.kind, t) {
        (TargetKind::Object(sel), TargetChoice::Object(id)) => {
            matches_selector(game, id, sel, ctx)
                && can_be_targeted(game, id, ctx.source, ctx.controller)
        }
        (TargetKind::Player(r), TargetChoice::Player(p)) => {
            resolve_players(game, r, ctx).contains(&p)
                && game.state.players.get(p.index()).is_some_and(|s| !s.has_lost)
        }
        (TargetKind::ObjectOrPlayer(sel, r), t) => match t {
            TargetChoice::Object(id) => {
                matches_selector(game, id, sel, ctx)
                    && can_be_targeted(game, id, ctx.source, ctx.controller)
            }
            TargetChoice::Player(p) => {
                resolve_players(game, r, ctx).contains(&p)
                    && game.state.players.get(p.index()).is_some_and(|s| !s.has_lost)
            }
        },
        (TargetKind::SpellOnStack(f), TargetChoice::Object(id)) => {
            // Mágica que já resolveu ou foi contra-atacada saiu da pilha.
            spells_on_stack(game).contains(&id)
                && matches_filter(game, id, f, ctx)
                && can_be_targeted(game, id, ctx.source, ctx.controller)
        }
        _ => false,
    }
}
