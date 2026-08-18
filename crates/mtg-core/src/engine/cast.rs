//! Legalidade de lançamento, ativação de habilidades e pagamento de custos.
//!
//! CR 117 (prioridade), CR 601 (lançar mágica), CR 602 (ativar habilidade),
//! CR 202 (custo de mana), CR 605 (habilidade de mana).
//!
//! O casamento de mana é resolvido como emparelhamento bipartido máximo:
//! cada "pip" do custo é um requisito, cada mana disponível (pool ou fonte
//! não usada) é uma unidade. Emparelhamento máximo é exato — heurística
//! gulosa erra em custos como {W}{U}{B} com duas terras duais e uma Ilha.
use std::sync::Arc;

use super::query::{self, EvalCtx};
use super::{layers, stack, turn, Game};
use crate::action::{Action, ActionError, ManaSourceChoice, Request, TargetChoice};
use crate::card::{Ability, ManaProduction, StaticMod};
use crate::event::GameEvent;
use crate::ids::{AbilityRef, ObjectId, PlayerId};
use crate::ir::{Cost, Effect, Filter, Keyword, TargetSpec, TimingRestriction};
use crate::mana::{Color, ColorSet, ManaSymbol};
use crate::types::{CardType, CounterKind, Supertype};
use crate::view::MatchEvent;
use crate::zone::{ZoneId, ZoneKind};

/// Teto de combinações de alvo geradas por mágica/habilidade. Acima disso a
/// árvore de decisão explode sem ganho tático real para o bot.
const MAX_TARGET_COMBOS: usize = 40;
/// Teto de X enumerado. X maior que isso nunca é decisivo em jogo automático.
const MAX_X: u32 = 10;
/// Teto de pips num custo. Custo maior que isso é considerado impagável.
const MAX_REQS: usize = 64;
/// Teto de combinações de pagamento alternativo ({2/W}, {W/P}).
const MAX_SPECIAL_COMBOS: usize = 64;
/// Vida paga por símbolo Phyrexian (CR 107.4d).
const PHYREXIAN_LIFE: i32 = 2;
/// Genérico pago no lugar da cor num símbolo monohíbrido (CR 107.4e).
const MONOHYBRID_GENERIC: u8 = 2;

// ---------------------------------------------------------------------------
// Disponibilidade de mana
// ---------------------------------------------------------------------------

/// Resumo do mana que o jogador consegue produzir agora.
#[derive(Debug, Clone, Default)]
pub struct ManaAvailability {
    /// Mana que só sai de uma cor específica, na ordem de `Color::ALL`.
    pub by_color: [u16; 5],
    /// Fontes capazes de produzir mais de uma cor — coringas.
    pub any_color: u16,
    pub colorless: u16,
    pub total: u16,
}

// ---------------------------------------------------------------------------
// Modelo interno de mana
// ---------------------------------------------------------------------------

/// O que uma unidade de mana pode virar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Produce {
    colors: ColorSet,
    colorless: bool,
}

impl Produce {
    fn fixed(c: Color) -> Self {
        Produce { colors: ColorSet::single(c), colorless: false }
    }
    fn generic_colorless() -> Self {
        Produce { colors: ColorSet::COLORLESS, colorless: true }
    }
    fn any() -> Self {
        Produce {
            colors: Color::ALL.iter().fold(ColorSet::COLORLESS, |a, c| a.union(ColorSet::single(*c))),
            colorless: false,
        }
    }
    fn flexibility(self) -> u32 {
        self.colors.count() + u32::from(self.colorless)
    }
}

/// Uma unidade individual de mana disponível: 1 mana concreto.
#[derive(Debug, Clone, Copy)]
struct Unit {
    source: ObjectId,
    ability: u16,
    produce: Produce,
    snow: bool,
    from_pool: bool,
}

/// Habilidade de mana concreta encontrada no campo.
#[derive(Debug, Clone)]
struct SourceInfo {
    object: ObjectId,
    ability: u16,
    snow: bool,
    /// Uma entrada por mana produzido.
    units: Vec<Produce>,
    /// `Some(n)` quando produz n mana de **uma mesma** cor escolhida na hora.
    same_color_group: Option<u8>,
}

impl SourceInfo {
    fn flexibility(&self) -> u32 {
        self.units.iter().map(|u| u.flexibility()).sum()
    }
}

/// O que um pip do custo aceita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Accept {
    /// Genérico: qualquer mana serve (CR 107.4).
    Generic,
    /// {C}: só mana incolor.
    Colorless,
    /// Uma das cores do conjunto.
    Colors(ColorSet),
}

#[derive(Debug, Clone, Copy)]
struct Req {
    accept: Accept,
    /// {S} exige mana produzido por permanente com o supertipo Neve.
    snow: bool,
}

/// Decisões tomadas sobre símbolos com pagamento alternativo, mais as
/// ativações necessárias. Precisa sobreviver do cálculo até o débito.
#[derive(Debug, Clone, Default)]
struct ManaSolution {
    activations: Vec<ManaSourceChoice>,
    /// Índices (na lista de símbolos) de Phyrexian pagos com vida.
    life_symbols: Vec<usize>,
    /// Índices de monohíbridos pagos como genérico.
    generic_symbols: Vec<usize>,
}

impl ManaSolution {
    fn life_cost(&self) -> i32 {
        self.life_symbols.len() as i32 * PHYREXIAN_LIFE
    }
}

// ---------------------------------------------------------------------------
// Leitura auxiliar de estado (sem panic)
// ---------------------------------------------------------------------------

/// `GameState::zone` entra em pânico se a zona não existir; aqui não.
fn zone_objects(game: &Game, z: ZoneId) -> Vec<ObjectId> {
    game.state
        .zones
        .get(&(z.kind, z.owner.map_or(u8::MAX, |p| p.0)))
        .map(|zone| zone.objects.clone())
        .unwrap_or_default()
}

fn card_of(game: &Game, obj: ObjectId) -> Option<crate::ids::CardDefId> {
    game.state.object(obj).map(|o| o.card)
}

fn life_of(game: &Game, player: PlayerId) -> i32 {
    game.state
        .players
        .get(player.index())
        .map(|p| p.life)
        .unwrap_or(0)
}

fn ctx_for(source: Option<ObjectId>, controller: PlayerId, x: u32) -> EvalCtx {
    EvalCtx { source, controller, x, ..EvalCtx::default() }
}

/// Chave total sobre `TargetChoice` — a lista de alvos precisa ser estável
/// entre execuções para que a semente reproduza a partida.
fn target_key(t: &TargetChoice) -> (u8, u32) {
    match t {
        TargetChoice::Object(o) => (0, o.0),
        TargetChoice::Player(p) => (1, u32::from(p.0)),
    }
}

// ---------------------------------------------------------------------------
// Custos: decomposição
// ---------------------------------------------------------------------------

/// Separa a parte de mana (agregada) das demais partes de um custo composto.
fn flatten_cost<'a>(cost: &'a Cost, symbols: &mut Vec<ManaSymbol>, parts: &mut Vec<&'a Cost>) {
    match cost {
        Cost::Mana(s) => symbols.extend(s.iter().copied()),
        Cost::Composite(inner) => {
            for c in inner {
                flatten_cost(c, symbols, parts);
            }
        }
        Cost::Free => {}
        other => parts.push(other),
    }
}

fn split_cost(cost: &Cost) -> (Vec<ManaSymbol>, Vec<&Cost>) {
    let mut symbols = Vec::new();
    let mut parts = Vec::new();
    flatten_cost(cost, &mut symbols, &mut parts);
    (symbols, parts)
}

/// CR 302.6 — criatura com enjoo de invocação não pode usar custo de {T}.
fn cost_taps_source(cost: &Cost) -> bool {
    match cost {
        Cost::Tap => true,
        Cost::Composite(parts) => parts.iter().any(cost_taps_source),
        _ => false,
    }
}

/// Vida comprometida pelas partes não-mana, para o Phyrexian não gastar duas vezes.
fn committed_life(game: &Game, player: PlayerId, source: Option<ObjectId>, parts: &[&Cost]) -> i32 {
    let ctx = ctx_for(source, player, 0);
    parts
        .iter()
        .map(|c| match c {
            Cost::PayLife(v) => query::eval_value(game, v, &ctx).max(0),
            _ => 0,
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Fontes de mana
// ---------------------------------------------------------------------------

/// Traduz uma produção de mana em unidades individuais.
fn production_units(game: &Game, obj: ObjectId, controller: PlayerId, prod: &ManaProduction)
    -> (Vec<Produce>, Option<u8>)
{
    match prod {
        ManaProduction::Fixed(symbols) => {
            let mut out = Vec::new();
            for s in symbols {
                match s {
                    ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                        out.push(Produce::fixed(*c))
                    }
                    ManaSymbol::Hybrid(a, b) => out.push(Produce {
                        colors: ColorSet::single(*a).union(ColorSet::single(*b)),
                        colorless: false,
                    }),
                    ManaSymbol::Colorless | ManaSymbol::Snow => out.push(Produce::generic_colorless()),
                    ManaSymbol::Generic(n) => {
                        out.extend(std::iter::repeat(Produce::generic_colorless()).take(*n as usize))
                    }
                    // X em produção não tem valor definido fora de resolução.
                    ManaSymbol::X => {}
                }
            }
            (out, None)
        }
        ManaProduction::AnyColor(n) => {
            let n = (*n).max(1);
            let units = vec![Produce::any(); n as usize];
            (units, if n > 1 { Some(n) } else { None })
        }
        ManaProduction::OneOf(symbols) => {
            // Uma única unidade que pode virar qualquer uma das opções.
            let mut colors = ColorSet::COLORLESS;
            let mut colorless = false;
            for s in symbols {
                match s {
                    ManaSymbol::Colorless | ManaSymbol::Snow | ManaSymbol::Generic(_) => colorless = true,
                    other => colors = colors.union(other.colors()),
                }
            }
            (vec![Produce { colors, colorless }], None)
        }
        ManaProduction::Dynamic { symbol, amount } => {
            let ctx = ctx_for(Some(obj), controller, 0);
            let n = query::eval_value(game, amount, &ctx).max(0) as usize;
            let p = match symbol {
                ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                    Produce::fixed(*c)
                }
                ManaSymbol::Hybrid(a, b) => Produce {
                    colors: ColorSet::single(*a).union(ColorSet::single(*b)),
                    colorless: false,
                },
                _ => Produce::generic_colorless(),
            };
            (vec![p; n], None)
        }
    }
}

/// Habilidades de mana usáveis agora. Exclui as que exigem mana no custo:
/// resolvê-las exigiria recursão no solucionador, e o ganho tático é nulo
/// para os decks do simulador.
fn collect_sources(game: &Game, player: PlayerId, exclude: Option<ObjectId>) -> Vec<SourceInfo> {
    let mut out: Vec<SourceInfo> = Vec::new();
    let mut battlefield = zone_objects(game, ZoneId::BATTLEFIELD);
    battlefield.sort_unstable();

    for obj in battlefield {
        if Some(obj) == exclude {
            continue;
        }
        let Some(state) = game.state.object(obj) else { continue };
        if state.controller != player {
            continue;
        }
        let Some(card) = game.db.get(state.card) else { continue };
        let snow = layers::characteristics(game, obj)
            .map(|c| c.type_line.has_supertype(Supertype::Snow))
            .unwrap_or(false);

        for (index, ability) in card.mana_abilities() {
            let (mana_part, _) = split_cost(&ability.cost);
            if !mana_part.is_empty() {
                continue;
            }
            let ctx = ctx_for(Some(obj), player, 0);
            if !query::eval_condition(game, &ability.restriction, &ctx) {
                continue;
            }
            if !can_pay_from(game, player, Some(obj), &ability.cost, 0) {
                continue;
            }
            let (units, group) = production_units(game, obj, player, &ability.production);
            if units.is_empty() {
                continue;
            }
            out.push(SourceInfo {
                object: obj,
                ability: index as u16,
                snow,
                units,
                same_color_group: group,
            });
        }
    }

    // Menos flexível primeiro: gastar a Floresta antes da terra dual.
    out.sort_by_key(|s| (s.flexibility(), s.object.0, s.ability));
    out
}

fn pool_units(game: &Game, player: PlayerId) -> Vec<Unit> {
    let mut out = Vec::new();
    let Some(p) = game.state.players.get(player.index()) else { return out };
    for c in Color::ALL {
        for _ in 0..p.mana_pool.colored[c.index()] {
            out.push(Unit {
                source: ObjectId::NONE,
                ability: 0,
                produce: Produce::fixed(c),
                snow: false,
                from_pool: true,
            });
        }
    }
    for _ in 0..p.mana_pool.colorless {
        out.push(Unit {
            source: ObjectId::NONE,
            ability: 0,
            produce: Produce::generic_colorless(),
            snow: false,
            from_pool: true,
        });
    }
    out
}

/// Unidades das fontes, com as cores dos grupos "n de uma cor só" já fixadas.
fn source_units(infos: &[SourceInfo], group_colors: &[Color]) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut group_idx = 0usize;
    for info in infos {
        let fixed = if info.same_color_group.is_some() {
            let c = group_colors.get(group_idx).copied();
            group_idx += 1;
            c
        } else {
            None
        };
        for produce in &info.units {
            out.push(Unit {
                source: info.object,
                ability: info.ability,
                produce: fixed.map(Produce::fixed).unwrap_or(*produce),
                snow: info.snow,
                from_pool: false,
            });
        }
    }
    out
}

pub fn available_mana(game: &Game, player: PlayerId) -> ManaAvailability {
    let infos = collect_sources(game, player, None);
    let mut units = pool_units(game, player);
    units.extend(source_units(&infos, &[]));

    let mut avail = ManaAvailability::default();
    for u in &units {
        avail.total = avail.total.saturating_add(1);
        if u.produce.flexibility() >= 2 {
            avail.any_color = avail.any_color.saturating_add(1);
        } else if u.produce.colorless {
            avail.colorless = avail.colorless.saturating_add(1);
        } else if let Some(c) = u.produce.colors.iter().next() {
            avail.by_color[c.index()] = avail.by_color[c.index()].saturating_add(1);
        }
    }
    avail
}

// ---------------------------------------------------------------------------
// Emparelhamento pip x mana
// ---------------------------------------------------------------------------

fn unit_satisfies(u: &Unit, r: &Req) -> bool {
    if r.snow && !u.snow {
        return false;
    }
    match r.accept {
        Accept::Generic => true,
        Accept::Colorless => u.produce.colorless,
        Accept::Colors(set) => u.produce.colors.intersects(set),
    }
}

fn augment(
    req: usize,
    reqs: &[Req],
    units: &[Unit],
    seen: &mut [bool],
    unit_of_req: &mut [usize],
    req_of_unit: &mut [usize],
) -> bool {
    for (i, u) in units.iter().enumerate() {
        if seen[i] || !unit_satisfies(u, &reqs[req]) {
            continue;
        }
        seen[i] = true;
        let holder = req_of_unit[i];
        if holder == usize::MAX || augment(holder, reqs, units, seen, unit_of_req, req_of_unit) {
            req_of_unit[i] = req;
            unit_of_req[req] = i;
            return true;
        }
    }
    false
}

/// Emparelhamento máximo (Kuhn). `Some` só quando **todo** pip foi coberto.
fn match_all(reqs: &[Req], units: &[Unit]) -> Option<Vec<usize>> {
    if reqs.len() > units.len() {
        return None;
    }
    let mut unit_of_req = vec![usize::MAX; reqs.len()];
    let mut req_of_unit = vec![usize::MAX; units.len()];
    for r in 0..reqs.len() {
        let mut seen = vec![false; units.len()];
        if !augment(r, reqs, units, &mut seen, &mut unit_of_req, &mut req_of_unit) {
            return None;
        }
    }
    Some(unit_of_req)
}

/// Traduz símbolos em pips, dadas as decisões sobre Phyrexian/monohíbrido.
fn build_reqs(
    symbols: &[ManaSymbol],
    x: u32,
    life_symbols: &[usize],
    generic_symbols: &[usize],
) -> Option<Vec<Req>> {
    let mut reqs: Vec<Req> = Vec::new();
    macro_rules! push {
        ($accept:expr, $snow:expr) => {
            reqs.push(Req { accept: $accept, snow: $snow })
        };
    }

    for (i, s) in symbols.iter().enumerate() {
        match s {
            ManaSymbol::Generic(n) => {
                for _ in 0..*n {
                    push!(Accept::Generic, false);
                }
            }
            ManaSymbol::X => {
                for _ in 0..x {
                    push!(Accept::Generic, false);
                }
            }
            ManaSymbol::Colored(c) => push!(Accept::Colors(ColorSet::single(*c)), false),
            ManaSymbol::Colorless => push!(Accept::Colorless, false),
            ManaSymbol::Snow => push!(Accept::Generic, true),
            ManaSymbol::Hybrid(a, b) => {
                push!(Accept::Colors(ColorSet::single(*a).union(ColorSet::single(*b))), false)
            }
            ManaSymbol::MonoHybrid(c) => {
                if generic_symbols.contains(&i) {
                    for _ in 0..MONOHYBRID_GENERIC {
                        push!(Accept::Generic, false);
                    }
                } else {
                    push!(Accept::Colors(ColorSet::single(*c)), false);
                }
            }
            ManaSymbol::Phyrexian(c) => {
                if !life_symbols.contains(&i) {
                    push!(Accept::Colors(ColorSet::single(*c)), false);
                }
            }
        }
        if reqs.len() > MAX_REQS {
            return None;
        }
    }
    Some(reqs)
}

/// Índices dos símbolos que admitem pagamento alternativo.
fn special_symbols(symbols: &[ManaSymbol]) -> (Vec<usize>, Vec<usize>) {
    let mut mono = Vec::new();
    let mut phyrexian = Vec::new();
    for (i, s) in symbols.iter().enumerate() {
        match s {
            ManaSymbol::MonoHybrid(_) => mono.push(i),
            ManaSymbol::Phyrexian(_) => phyrexian.push(i),
            _ => {}
        }
    }
    (mono, phyrexian)
}

/// Máscaras de pagamento alternativo ordenadas por quantidade de alternativas
/// usadas: pagar com mana colorido é sempre a primeira tentativa.
fn alternative_masks(count: usize) -> Vec<u32> {
    if count == 0 || count > 6 {
        return vec![0];
    }
    let mut masks: Vec<u32> = (0..(1u32 << count)).collect();
    masks.sort_by_key(|m| (m.count_ones(), *m));
    masks.truncate(MAX_SPECIAL_COMBOS);
    masks
}

/// Cor de fallback para fontes "n mana de uma mesma cor" quando há grupos
/// demais para enumerar: a primeira cor que o custo pede.
fn dominant_color(symbols: &[ManaSymbol]) -> Color {
    for s in symbols {
        if let Some(c) = s.colors().iter().next() {
            return c;
        }
    }
    Color::White
}

/// Assinaturas de cor para os grupos "n de uma mesma cor".
fn group_color_options(groups: usize, symbols: &[ManaSymbol]) -> Vec<Vec<Color>> {
    match groups {
        0 => vec![Vec::new()],
        1 => Color::ALL.iter().map(|c| vec![*c]).collect(),
        2 => {
            let mut out = Vec::with_capacity(25);
            for a in Color::ALL {
                for b in Color::ALL {
                    out.push(vec![a, b]);
                }
            }
            out
        }
        n => vec![vec![dominant_color(symbols); n]],
    }
}

/// Resolve o pagamento: quais fontes ativar, quais símbolos vão por vida.
fn solve_mana(
    game: &Game,
    player: PlayerId,
    symbols: &[ManaSymbol],
    x: u32,
    exclude: Option<ObjectId>,
    extra_life: i32,
) -> Option<ManaSolution> {
    if symbols.is_empty() {
        return Some(ManaSolution::default());
    }
    let (mono, phyrexian) = special_symbols(symbols);
    let specials: Vec<usize> = phyrexian.iter().chain(mono.iter()).copied().collect();
    let infos = collect_sources(game, player, exclude);
    let pool = pool_units(game, player);
    let groups = infos.iter().filter(|i| i.same_color_group.is_some()).count();
    let color_options = group_color_options(groups, symbols);
    let life = life_of(game, player) - extra_life;

    for mask in alternative_masks(specials.len()) {
        let mut life_symbols = Vec::new();
        let mut generic_symbols = Vec::new();
        for (bit, idx) in specials.iter().enumerate() {
            if mask & (1 << bit) == 0 {
                continue;
            }
            if phyrexian.contains(idx) {
                life_symbols.push(*idx);
            } else {
                generic_symbols.push(*idx);
            }
        }
        // CR 118.4 — só se pode pagar vida que se tem.
        if life < life_symbols.len() as i32 * PHYREXIAN_LIFE {
            continue;
        }
        let Some(reqs) = build_reqs(symbols, x, &life_symbols, &generic_symbols) else { continue };
        if reqs.is_empty() {
            return Some(ManaSolution { activations: Vec::new(), life_symbols, generic_symbols });
        }

        for colors in &color_options {
            let mut units = pool.clone();
            units.extend(source_units(&infos, colors));
            let Some(assignment) = match_all(&reqs, &units) else { continue };

            let mut activations: Vec<ManaSourceChoice> = Vec::new();
            for (r, unit_idx) in assignment.iter().enumerate() {
                let Some(unit) = units.get(*unit_idx) else { continue };
                if unit.from_pool {
                    continue;
                }
                if activations
                    .iter()
                    .any(|a| a.source == unit.source && a.ability_index == unit.ability)
                {
                    continue;
                }
                activations.push(ManaSourceChoice {
                    source: unit.source,
                    ability_index: unit.ability,
                    color: pick_color(unit, &reqs[r]),
                });
            }
            activations.sort_by_key(|a| (a.source.0, a.ability_index));
            return Some(ManaSolution { activations, life_symbols, generic_symbols });
        }
    }
    None
}

/// Cor concreta que a fonte deve produzir para satisfazer o pip.
fn pick_color(unit: &Unit, req: &Req) -> Option<Color> {
    let wanted = match req.accept {
        Accept::Colorless => return None,
        Accept::Colors(set) => set,
        Accept::Generic => unit.produce.colors,
    };
    unit.produce.colors.iter().find(|c| wanted.contains(*c))
}

// ---------------------------------------------------------------------------
// can_pay
// ---------------------------------------------------------------------------

pub fn can_pay(game: &Game, player: PlayerId, cost: &Cost) -> bool {
    can_pay_from(game, player, None, cost, 0)
}

/// Versão com fonte e X explícitos — custos como {T} e {E} só fazem sentido
/// com a fonte conhecida.
pub fn can_pay_from(
    game: &Game,
    player: PlayerId,
    source: Option<ObjectId>,
    cost: &Cost,
    x: u32,
) -> bool {
    let (symbols, parts) = split_cost(cost);
    let ctx = ctx_for(source, player, x);

    for part in &parts {
        if !can_pay_part(game, player, source, part, &ctx) {
            return false;
        }
    }
    if symbols.is_empty() {
        return true;
    }
    // A própria fonte, se o custo a vira, não pode também produzir mana.
    let exclude = if cost_taps_source(cost) { source } else { None };
    let extra_life = committed_life(game, player, source, &parts);
    solve_mana(game, player, &symbols, x, exclude, extra_life).is_some()
}

fn can_pay_part(
    game: &Game,
    player: PlayerId,
    source: Option<ObjectId>,
    cost: &Cost,
    ctx: &EvalCtx,
) -> bool {
    match cost {
        Cost::Free => true,
        Cost::Tap => source
            .and_then(|s| game.state.object(s))
            .map(|o| o.on_battlefield() && !o.tapped && o.controller == player)
            .unwrap_or(false),
        Cost::Untap => source
            .and_then(|s| game.state.object(s))
            .map(|o| o.on_battlefield() && o.tapped && o.controller == player)
            .unwrap_or(false),
        Cost::PayLife(v) => {
            let amount = query::eval_value(game, v, ctx);
            amount <= 0 || life_of(game, player) >= amount
        }
        Cost::Discard(n, filter) => {
            matching_in_zone(game, ZoneId::hand(player), filter, ctx).len() >= *n as usize
        }
        Cost::ExileFromGraveyard(n, filter) => {
            matching_in_zone(game, ZoneId::graveyard(player), filter, ctx).len() >= *n as usize
        }
        Cost::Sacrifice(n, filter) => sacrificeable(game, player, filter, ctx).len() >= *n as usize,
        Cost::TapOther(n, filter) => tappable(game, player, source, filter, ctx).len() >= *n as usize,
        Cost::RemoveCounters(n, kind) => source
            .and_then(|s| game.state.object(s))
            .map(|o| o.counter(kind) >= *n as i32)
            .unwrap_or(false),
        Cost::AddCountersToSelf(_, _) => source.is_some(),
        // Mana e Composite já foram achatados por `split_cost`.
        Cost::Mana(_) | Cost::Composite(_) => true,
    }
}

fn matching_in_zone(game: &Game, zone: ZoneId, filter: &Filter, ctx: &EvalCtx) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = zone_objects(game, zone)
        .into_iter()
        .filter(|o| query::matches_filter(game, *o, filter, ctx))
        .collect();
    out.sort_unstable();
    out
}

fn sacrificeable(game: &Game, player: PlayerId, filter: &Filter, ctx: &EvalCtx) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = zone_objects(game, ZoneId::BATTLEFIELD)
        .into_iter()
        .filter(|o| {
            game.state.object(*o).map(|s| s.controller == player).unwrap_or(false)
                && query::matches_filter(game, *o, filter, ctx)
        })
        .collect();
    out.sort_unstable();
    out
}

fn tappable(
    game: &Game,
    player: PlayerId,
    source: Option<ObjectId>,
    filter: &Filter,
    ctx: &EvalCtx,
) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = zone_objects(game, ZoneId::BATTLEFIELD)
        .into_iter()
        .filter(|o| Some(*o) != source)
        .filter(|o| {
            game.state
                .object(*o)
                .map(|s| s.controller == player && !s.tapped)
                .unwrap_or(false)
                && query::matches_filter(game, *o, filter, ctx)
        })
        .collect();
    out.sort_unstable();
    out
}

// ---------------------------------------------------------------------------
// pay_cost
// ---------------------------------------------------------------------------

pub fn pay_cost(
    game: &mut Game,
    player: PlayerId,
    cost: &Cost,
    plan: &[ManaSourceChoice],
) -> Result<(), ActionError> {
    pay_cost_from(game, player, None, cost, 0, plan)
}

/// Paga de fato. Ordem: partes não-mana primeiro (virar a fonte impede que ela
/// mesma seja usada como fonte de mana — CR 602.2b), depois o mana.
pub fn pay_cost_from(
    game: &mut Game,
    player: PlayerId,
    source: Option<ObjectId>,
    cost: &Cost,
    x: u32,
    plan: &[ManaSourceChoice],
) -> Result<(), ActionError> {
    if !can_pay_from(game, player, source, cost, x) {
        return Err(ActionError::CannotPay(describe_cost(cost)));
    }
    let (symbols, parts) = split_cost(cost);
    let owned: Vec<Cost> = parts.into_iter().cloned().collect();
    for part in &owned {
        pay_part(game, player, source, part, x)?;
    }
    if symbols.is_empty() {
        return Ok(());
    }
    pay_mana(game, player, &symbols, x, plan)
}

fn pay_part(
    game: &mut Game,
    player: PlayerId,
    source: Option<ObjectId>,
    cost: &Cost,
    x: u32,
) -> Result<(), ActionError> {
    let ctx = ctx_for(source, player, x);
    match cost {
        Cost::Free | Cost::Mana(_) | Cost::Composite(_) => Ok(()),
        Cost::Tap => {
            let Some(obj) = source else {
                return Err(ActionError::CannotPay("{T} sem fonte".into()));
            };
            tap_object(game, obj);
            Ok(())
        }
        Cost::Untap => {
            let Some(obj) = source else {
                return Err(ActionError::CannotPay("{Q} sem fonte".into()));
            };
            if let Some(state) = game.state.object_mut(obj) {
                state.tapped = false;
            }
            game.state.emit(GameEvent::Untapped { object: obj });
            game.push_event(MatchEvent::Untapped { card: obj });
            Ok(())
        }
        Cost::PayLife(v) => {
            let amount = query::eval_value(game, v, &ctx);
            if amount > 0 {
                lose_life(game, player, amount);
            }
            Ok(())
        }
        Cost::Discard(n, filter) => {
            let candidates = matching_in_zone(game, ZoneId::hand(player), filter, &ctx);
            let chosen = choose_objects(game, player, "descarte como custo", candidates, *n);
            for obj in chosen {
                turn::move_object(game, obj, ZoneId::graveyard(player));
                game.state.emit(GameEvent::CardDiscarded { player, object: obj });
            }
            Ok(())
        }
        Cost::ExileFromGraveyard(n, filter) => {
            let candidates = matching_in_zone(game, ZoneId::graveyard(player), filter, &ctx);
            let chosen = choose_objects(game, player, "exile do cemitério como custo", candidates, *n);
            for obj in chosen {
                turn::move_object(game, obj, ZoneId::EXILE);
                game.state.emit(GameEvent::Exiled { object: obj });
                game.push_event(MatchEvent::Exiled { card: obj });
            }
            Ok(())
        }
        Cost::Sacrifice(n, filter) => {
            let candidates = sacrificeable(game, player, filter, &ctx);
            let chosen = choose_objects(game, player, "sacrifique como custo", candidates, *n);
            for obj in chosen {
                game.state.emit(GameEvent::Sacrificed { object: obj, controller: player });
                turn::move_object(game, obj, ZoneId::graveyard(player));
            }
            Ok(())
        }
        Cost::TapOther(n, filter) => {
            let candidates = tappable(game, player, source, filter, &ctx);
            let chosen = choose_objects(game, player, "vire permanentes como custo", candidates, *n);
            for obj in chosen {
                tap_object(game, obj);
            }
            Ok(())
        }
        Cost::RemoveCounters(n, kind) => {
            let Some(obj) = source else {
                return Err(ActionError::CannotPay("remoção de marcador sem fonte".into()));
            };
            if let Some(state) = game.state.object_mut(obj) {
                state.add_counter(kind.clone(), -(*n as i32));
            }
            game.state.emit(GameEvent::CountersRemoved {
                object: obj,
                kind: kind.clone(),
                amount: *n as i32,
            });
            Ok(())
        }
        Cost::AddCountersToSelf(n, kind) => {
            let Some(obj) = source else {
                return Err(ActionError::CannotPay("marcador sem fonte".into()));
            };
            if let Some(state) = game.state.object_mut(obj) {
                state.add_counter(kind.clone(), *n as i32);
            }
            game.state.emit(GameEvent::CountersAdded {
                object: obj,
                kind: kind.clone(),
                amount: *n as i32,
            });
            Ok(())
        }
    }
}

/// CR 601.2g/h — ativa as habilidades de mana necessárias e debita o pool.
fn pay_mana(
    game: &mut Game,
    player: PlayerId,
    symbols: &[ManaSymbol],
    x: u32,
    plan: &[ManaSourceChoice],
) -> Result<(), ActionError> {
    // Plano vindo do agente tem precedência; o solucionador completa o resto.
    for choice in plan {
        if !activate_mana_ability(game, player, *choice) {
            game.state.push_log(
                format!("fonte de mana {} do plano ignorada", choice.source),
                Some(player),
            );
        }
    }

    let Some(solution) = solve_mana(game, player, symbols, x, None, 0) else {
        return Err(ActionError::CannotPay(render_symbols(symbols)));
    };
    for choice in &solution.activations {
        if !activate_mana_ability(game, player, *choice) {
            game.state
                .push_log(format!("falha ao ativar mana de {}", choice.source), Some(player));
        }
    }

    let life = solution.life_cost();
    if life > 0 {
        lose_life(game, player, life);
    }

    let Some(reqs) = build_reqs(symbols, x, &solution.life_symbols, &solution.generic_symbols)
    else {
        return Err(ActionError::CannotPay(render_symbols(symbols)));
    };
    if reqs.is_empty() {
        return Ok(());
    }
    let units = pool_units(game, player);
    let Some(assignment) = match_all(&reqs, &units) else {
        game.state.push_log(
            format!("pool insuficiente após ativar fontes para {}", render_symbols(symbols)),
            Some(player),
        );
        return Err(ActionError::CannotPay(render_symbols(symbols)));
    };

    for unit_idx in assignment {
        let Some(unit) = units.get(unit_idx) else { continue };
        let color = unit.produce.colors.iter().next();
        if let Some(p) = game.state.players.get_mut(player.index()) {
            if !p.mana_pool.take(color, 1) {
                game.state
                    .push_log("débito de mana inconsistente no pool", Some(player));
            }
        }
    }
    Ok(())
}

/// CR 605.3a — habilidade de mana resolve na hora, sem usar a pilha.
fn activate_mana_ability(game: &mut Game, player: PlayerId, choice: ManaSourceChoice) -> bool {
    let Some(card_id) = card_of(game, choice.source) else { return false };
    let db = Arc::clone(&game.db);
    let Some(card) = db.get(card_id) else { return false };
    let Some(Ability::Mana(ability)) = card.abilities.get(choice.ability_index as usize) else {
        return false;
    };
    let Some(state) = game.state.object(choice.source) else { return false };
    if state.controller != player || !state.on_battlefield() {
        return false;
    }
    let ctx = ctx_for(Some(choice.source), player, 0);
    if !query::eval_condition(game, &ability.restriction, &ctx) {
        return false;
    }
    if !can_pay_from(game, player, Some(choice.source), &ability.cost, 0) {
        return false;
    }

    // Custo de habilidade de mana nunca inclui mana aqui (filtrado em
    // `collect_sources`), então não há recursão de pagamento.
    let (_, parts) = split_cost(&ability.cost);
    let owned: Vec<Cost> = parts.into_iter().cloned().collect();
    for part in &owned {
        if pay_part(game, player, Some(choice.source), part, 0).is_err() {
            return false;
        }
    }

    let produced = produced_mana(game, choice.source, player, &ability.production, choice.color);
    if let Some(p) = game.state.players.get_mut(player.index()) {
        for (color, amount) in &produced {
            p.mana_pool.add(*color, *amount);
        }
    }
    let name = game.card_name(choice.source);
    game.state.push_log(format!("{name} produz mana"), Some(player));
    true
}

/// Mana concreto que uma produção coloca no pool.
fn produced_mana(
    game: &Game,
    obj: ObjectId,
    player: PlayerId,
    prod: &ManaProduction,
    color: Option<Color>,
) -> Vec<(Option<Color>, u16)> {
    match prod {
        ManaProduction::Fixed(symbols) => symbols
            .iter()
            .filter_map(|s| match s {
                ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                    Some((Some(*c), 1))
                }
                ManaSymbol::Hybrid(a, b) => {
                    let chosen = color.filter(|c| c == a || c == b).unwrap_or(*a);
                    Some((Some(chosen), 1))
                }
                ManaSymbol::Colorless | ManaSymbol::Snow => Some((None, 1)),
                ManaSymbol::Generic(n) if *n > 0 => Some((None, u16::from(*n))),
                _ => None,
            })
            .collect(),
        ManaProduction::AnyColor(n) => {
            vec![(Some(color.unwrap_or(Color::White)), u16::from((*n).max(1)))]
        }
        ManaProduction::OneOf(symbols) => {
            let chosen = color.filter(|c| symbols.iter().any(|s| s.colors().contains(*c)));
            match chosen {
                Some(c) => vec![(Some(c), 1)],
                None => match symbols.first() {
                    Some(ManaSymbol::Colored(c)) => vec![(Some(*c), 1)],
                    Some(ManaSymbol::Hybrid(a, _)) => vec![(Some(*a), 1)],
                    _ => vec![(None, 1)],
                },
            }
        }
        ManaProduction::Dynamic { symbol, amount } => {
            let ctx = ctx_for(Some(obj), player, 0);
            let n = query::eval_value(game, amount, &ctx).max(0).min(u16::MAX as i32) as u16;
            let c = symbol.colors().iter().next();
            vec![(c, n)]
        }
    }
}

fn tap_object(game: &mut Game, obj: ObjectId) {
    let already = game.state.object(obj).map(|o| o.tapped).unwrap_or(true);
    if already {
        return;
    }
    if let Some(state) = game.state.object_mut(obj) {
        state.tapped = true;
    }
    game.state.emit(GameEvent::Tapped { object: obj });
    game.push_event(MatchEvent::Tapped { card: obj });
}

fn lose_life(game: &mut Game, player: PlayerId, amount: i32) {
    let Some(p) = game.state.players.get_mut(player.index()) else { return };
    p.life -= amount;
    p.life_lost_this_turn += amount;
    let total = p.life;
    game.state.emit(GameEvent::LifeLost { player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: -amount, total });
}

/// Escolha de objetos para custos que exigem seleção.
fn choose_objects(
    game: &mut Game,
    player: PlayerId,
    prompt: &str,
    candidates: Vec<ObjectId>,
    count: u8,
) -> Vec<ObjectId> {
    if count == 0 || candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() <= count as usize {
        return candidates;
    }
    let action = game.ask(Request::SelectObjects {
        player,
        prompt: prompt.to_string(),
        candidates: candidates.clone(),
        min: count,
        max: count,
    });
    match action {
        Action::SelectObjects { objects } if objects.len() == count as usize => objects,
        // Agente devolveu coisa inesperada: escolha determinística pelo menor id.
        _ => candidates.into_iter().take(count as usize).collect(),
    }
}

fn render_symbols(symbols: &[ManaSymbol]) -> String {
    symbols
        .iter()
        .map(|s| match s {
            ManaSymbol::Generic(n) => format!("{{{n}}}"),
            ManaSymbol::Colored(c) => format!("{{{}}}", c.letter()),
            ManaSymbol::Colorless => "{C}".to_string(),
            ManaSymbol::Hybrid(a, b) => format!("{{{}/{}}}", a.letter(), b.letter()),
            ManaSymbol::MonoHybrid(c) => format!("{{2/{}}}", c.letter()),
            ManaSymbol::Phyrexian(c) => format!("{{{}/P}}", c.letter()),
            ManaSymbol::Snow => "{S}".to_string(),
            ManaSymbol::X => "{X}".to_string(),
        })
        .collect()
}

fn describe_cost(cost: &Cost) -> String {
    let (symbols, parts) = split_cost(cost);
    let mut s = render_symbols(&symbols);
    for p in parts {
        s.push_str(match p {
            Cost::Tap => " {T}",
            Cost::Untap => " {Q}",
            Cost::PayLife(_) => " vida",
            Cost::Discard(_, _) => " descarte",
            Cost::Sacrifice(_, _) => " sacrifício",
            Cost::ExileFromGraveyard(_, _) => " exílio",
            Cost::RemoveCounters(_, _) => " marcadores",
            Cost::AddCountersToSelf(_, _) => " marcadores",
            Cost::TapOther(_, _) => " virar outros",
            _ => "",
        });
    }
    if s.is_empty() {
        "custo livre".to_string()
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Custo total de uma mágica (CR 601.2f)
// ---------------------------------------------------------------------------

/// Custo de mana da mágica depois de redutores e aumentadores estáticos.
pub fn spell_total_cost(game: &Game, object: ObjectId, controller: PlayerId) -> Vec<ManaSymbol> {
    let Some(card_id) = card_of(game, object) else { return Vec::new() };
    let Some(card) = game.db.get(card_id) else { return Vec::new() };
    let mut symbols = card.mana_cost.symbols.clone();
    let delta = cost_delta(game, object, controller);
    apply_generic_delta(&mut symbols, delta);
    symbols
}

/// Soma dos modificadores estáticos aplicáveis a esta mágica.
fn cost_delta(game: &Game, object: ObjectId, _controller: PlayerId) -> i32 {
    let mut delta = 0;
    let mut battlefield = zone_objects(game, ZoneId::BATTLEFIELD);
    battlefield.sort_unstable();
    for permanent in battlefield {
        let Some(state) = game.state.object(permanent) else { continue };
        let owner = state.controller;
        let Some(card_id) = card_of(game, permanent) else { continue };
        let Some(card) = game.db.get(card_id) else { continue };
        for (_, ability) in card.statics() {
            let ctx = ctx_for(Some(permanent), owner, 0);
            if !query::eval_condition(game, &ability.condition, &ctx) {
                continue;
            }
            match &ability.modification {
                StaticMod::CostReduction(value, filter) => {
                    if query::matches_filter(game, object, filter, &ctx) {
                        delta -= query::eval_value(game, value, &ctx);
                    }
                }
                StaticMod::CostIncrease(value, filter) => {
                    if query::matches_filter(game, object, filter, &ctx) {
                        delta += query::eval_value(game, value, &ctx);
                    }
                }
                _ => {}
            }
        }
    }
    delta
}

/// CR 601.2f — redução só corta a parte genérica; aumento vira genérico novo.
fn apply_generic_delta(symbols: &mut Vec<ManaSymbol>, delta: i32) {
    if delta == 0 {
        return;
    }
    if delta > 0 {
        symbols.push(ManaSymbol::Generic(delta.min(u8::MAX as i32) as u8));
        return;
    }
    let mut remaining = (-delta) as u32;
    for s in symbols.iter_mut() {
        if remaining == 0 {
            break;
        }
        if let ManaSymbol::Generic(n) = s {
            let cut = remaining.min(u32::from(*n));
            *n -= cut as u8;
            remaining -= cut;
        }
    }
    symbols.retain(|s| !matches!(s, ManaSymbol::Generic(0)));
}

// ---------------------------------------------------------------------------
// Timing
// ---------------------------------------------------------------------------

/// CR 307.1 / 305.1 — janela de feitiço: seu turno, fase principal, pilha vazia.
fn sorcery_timing(game: &Game, player: PlayerId) -> bool {
    game.state.active_player == player
        && game.state.step.is_main()
        && game.state.stack.is_empty()
}

/// CR 117.1a — instantâneo e mágica com Flash a qualquer momento com prioridade.
fn can_cast_now(game: &Game, player: PlayerId, object: ObjectId) -> bool {
    let Some(chars) = layers::characteristics(game, object) else { return false };
    if chars.type_line.has_type(CardType::Instant) || chars.has_keyword(&Keyword::Flash) {
        return true;
    }
    sorcery_timing(game, player)
}

// ---------------------------------------------------------------------------
// Enumeração de ações
// ---------------------------------------------------------------------------

pub fn priority_actions(game: &Game, player: PlayerId) -> Vec<Action> {
    enumerate_priority(game, player).0
}

/// Igual a `priority_actions`, mas registra no log os cortes de combinação.
/// Existe porque a enumeração precisa ser pura (`&Game`) para `legal_actions_for`.
pub fn priority_actions_logged(game: &mut Game, player: PlayerId) -> Vec<Action> {
    let (actions, notes) = enumerate_priority(game, player);
    for note in notes {
        game.state.push_log(note, Some(player));
    }
    actions
}

fn enumerate_priority(game: &Game, player: PlayerId) -> (Vec<Action>, Vec<String>) {
    let mut actions = vec![Action::PassPriority];
    let mut notes = Vec::new();
    if game.state.is_over() {
        return (actions, notes);
    }

    let mut hand = zone_objects(game, ZoneId::hand(player));
    hand.sort_unstable();

    // CR 305.1 — jogar terreno é ação especial, não usa a pilha.
    if sorcery_timing(game, player) {
        let played = game
            .state
            .players
            .get(player.index())
            .map(|p| (p.lands_played_this_turn, p.max_lands_per_turn))
            .unwrap_or((0, 0));
        if played.0 < played.1 {
            for obj in &hand {
                let is_land = layers::characteristics(game, *obj)
                    .map(|c| c.type_line.is_land())
                    .unwrap_or(false);
                if is_land {
                    actions.push(Action::PlayLand { object: *obj });
                }
            }
        }
    }

    for obj in &hand {
        collect_cast_actions(game, player, *obj, &mut actions, &mut notes);
    }

    let mut battlefield = zone_objects(game, ZoneId::BATTLEFIELD);
    battlefield.sort_unstable();
    for obj in battlefield {
        let controlled = game
            .state
            .object(obj)
            .map(|o| o.controller == player)
            .unwrap_or(false);
        if controlled {
            collect_activation_actions(game, player, obj, &mut actions, &mut notes);
        }
    }

    (actions, notes)
}

fn collect_cast_actions(
    game: &Game,
    player: PlayerId,
    object: ObjectId,
    actions: &mut Vec<Action>,
    notes: &mut Vec<String>,
) {
    let Some(chars) = layers::characteristics(game, object) else { return };
    // Terreno não é lançado (CR 305.1); permanentes e mágicas sim.
    if chars.type_line.is_land() {
        return;
    }
    if !can_cast_now(game, player, object) {
        return;
    }
    let Some(card_id) = card_of(game, object) else { return };
    let Some(card) = game.db.get(card_id) else { return };

    let symbols = spell_total_cost(game, object, player);
    let has_x = symbols.iter().any(|s| matches!(s, ManaSymbol::X));
    let specs: Vec<TargetSpec> = card.spell_targets.clone();
    let modal_size = modal_option_count(card.spell_effect.as_ref());

    let x_values: Vec<u32> = if has_x { (0..=MAX_X).collect() } else { vec![0] };
    for x in x_values {
        if solve_mana(game, player, &symbols, x, None, 0).is_none() {
            // Custo cresce com X: falhou aqui, falha para todo X maior.
            break;
        }
        let (combos, truncated) = target_combinations(game, object, player, &specs, x);
        if truncated {
            notes.push(format!(
                "combinações de alvo de {} cortadas em {MAX_TARGET_COMBOS}",
                game.card_name(object)
            ));
        }
        if combos.is_empty() {
            // CR 601.2c — sem alvo legal a mágica não pode ser lançada.
            continue;
        }
        for targets in combos {
            for modes in mode_choices(modal_size) {
                actions.push(Action::CastSpell {
                    object,
                    targets: targets.clone(),
                    x,
                    modes,
                    mana_plan: Vec::new(),
                });
            }
        }
    }
}

fn collect_activation_actions(
    game: &Game,
    player: PlayerId,
    source: ObjectId,
    actions: &mut Vec<Action>,
    notes: &mut Vec<String>,
) {
    let Some(card_id) = card_of(game, source) else { return };
    let db = Arc::clone(&game.db);
    let Some(card) = db.get(card_id) else { return };
    let loyalty_used = loyalty_ability_used(game, source, card);

    for (index, ability) in card.activated() {
        let index = index as u16;
        if !activation_legal(game, player, source, index, ability, loyalty_used) {
            continue;
        }
        let (symbols, _) = split_cost(&ability.cost);
        let has_x = symbols.iter().any(|s| matches!(s, ManaSymbol::X));
        let x_values: Vec<u32> = if has_x { (0..=MAX_X).collect() } else { vec![0] };

        for x in x_values {
            if !can_pay_from(game, player, Some(source), &ability.cost, x) {
                break;
            }
            let (combos, truncated) =
                target_combinations(game, source, player, &ability.targets, x);
            if truncated {
                notes.push(format!(
                    "combinações de alvo da habilidade {index} de {} cortadas em {MAX_TARGET_COMBOS}",
                    game.card_name(source)
                ));
            }
            if combos.is_empty() {
                continue;
            }
            for targets in combos {
                actions.push(Action::ActivateAbility {
                    source,
                    index,
                    targets,
                    x,
                    mana_plan: Vec::new(),
                });
            }
        }
    }
}

/// Alguma habilidade de lealdade desta fonte já foi ativada neste turno?
/// CR 606.3 — só uma por turno, por permanente.
fn loyalty_ability_used(game: &Game, source: ObjectId, card: &crate::card::CardDef) -> bool {
    let Some(state) = game.state.object(source) else { return true };
    card.activated().any(|(i, a)| {
        a.loyalty_change.is_some() && state.ability_uses.get(&(i as u16)).copied().unwrap_or(0) > 0
    })
}

fn activation_legal(
    game: &Game,
    player: PlayerId,
    source: ObjectId,
    index: u16,
    ability: &crate::card::ActivatedAbility,
    loyalty_used: bool,
) -> bool {
    let Some(state) = game.state.object(source) else { return false };
    if state.controller != player || state.zone.kind != ZoneKind::Battlefield {
        return false;
    }
    match ability.timing {
        TimingRestriction::Instant => {}
        TimingRestriction::Sorcery => {
            if !sorcery_timing(game, player) {
                return false;
            }
        }
    }
    if let Some(limit) = ability.uses_per_turn {
        if state.ability_uses.get(&index).copied().unwrap_or(0) >= limit {
            return false;
        }
    }
    if ability.loyalty_change.is_some() {
        // CR 606.3 — lealdade tem janela de feitiço e uma ativação por turno.
        if loyalty_used || !sorcery_timing(game, player) {
            return false;
        }
        if let Some(change) = ability.loyalty_change {
            if change < 0 && state.counter(&CounterKind::Loyalty) < -change {
                return false;
            }
        }
    }
    // CR 302.6 — enjoo de invocação bloqueia {T} em criatura sem Ímpeto.
    if cost_taps_source(&ability.cost) {
        if let Some(chars) = layers::characteristics(game, source) {
            if chars.is_creature() && state.summoning_sick && !chars.has_keyword(&Keyword::Haste) {
                return false;
            }
        }
    }
    let ctx = ctx_for(Some(source), player, 0);
    if !query::eval_condition(game, &ability.restriction, &ctx) {
        return false;
    }
    can_pay_from(game, player, Some(source), &ability.cost, 0)
}

fn modal_option_count(effect: Option<&Effect>) -> Option<(usize, u8)> {
    match effect {
        Some(Effect::Modal { choose, options }) => Some((options.len(), *choose)),
        _ => None,
    }
}

/// Combinações de modo. Sem modal, uma única entrada vazia.
fn mode_choices(modal: Option<(usize, u8)>) -> Vec<Vec<u8>> {
    let Some((count, choose)) = modal else { return vec![Vec::new()] };
    let choose = choose.max(1) as usize;
    if count == 0 || choose > count {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    let mut current = Vec::with_capacity(choose);
    combine_modes(count, choose, 0, &mut current, &mut out);
    if out.is_empty() {
        vec![Vec::new()]
    } else {
        out
    }
}

fn combine_modes(count: usize, choose: usize, start: usize, cur: &mut Vec<u8>, out: &mut Vec<Vec<u8>>) {
    if cur.len() == choose {
        out.push(cur.clone());
        return;
    }
    for i in start..count {
        cur.push(i as u8);
        combine_modes(count, choose, i + 1, cur, out);
        cur.pop();
    }
}

/// Produto cartesiano das listas de alvos legais, com teto determinístico.
fn target_combinations(
    game: &Game,
    source: ObjectId,
    controller: PlayerId,
    specs: &[TargetSpec],
    x: u32,
) -> (Vec<Vec<TargetChoice>>, bool) {
    if specs.is_empty() {
        return (vec![Vec::new()], false);
    }
    let ctx = ctx_for(Some(source), controller, x);
    let mut combos: Vec<Vec<TargetChoice>> = vec![Vec::new()];
    let mut truncated = false;

    for spec in specs {
        let mut options = query::legal_targets(game, spec, &ctx);
        options.sort_by_key(target_key);
        options.dedup();
        if options.is_empty() {
            return (Vec::new(), truncated);
        }
        let mut next: Vec<Vec<TargetChoice>> = Vec::new();
        'build: for base in &combos {
            for option in &options {
                // CR 601.2c — o mesmo objeto não pode ocupar dois alvos.
                if base.contains(option) {
                    continue;
                }
                let mut extended = base.clone();
                extended.push(*option);
                next.push(extended);
                if next.len() >= MAX_TARGET_COMBOS {
                    truncated = true;
                    break 'build;
                }
            }
        }
        if next.is_empty() {
            return (Vec::new(), truncated);
        }
        combos = next;
    }
    (combos, truncated)
}

// ---------------------------------------------------------------------------
// Execução
// ---------------------------------------------------------------------------

pub fn execute(game: &mut Game, player: PlayerId, action: Action) -> Result<(), ActionError> {
    if game.state.is_over() {
        return Err(ActionError::GameOver);
    }
    match action {
        Action::PlayLand { object } => play_land(game, player, object),
        Action::CastSpell { object, targets, x, modes, mana_plan } => {
            cast_spell(game, player, object, targets, x, modes, &mana_plan)
        }
        Action::ActivateAbility { source, index, targets, x, mana_plan } => {
            activate_ability(game, player, source, index, targets, x, &mana_plan)
        }
        Action::PassPriority => Ok(()),
        other => Err(ActionError::Illegal(format!(
            "cast::execute não trata a ação {}",
            other.label()
        ))),
    }
}

/// CR 305.1 — jogar terreno não usa a pilha e não pode ser respondido.
fn play_land(game: &mut Game, player: PlayerId, object: ObjectId) -> Result<(), ActionError> {
    if !sorcery_timing(game, player) {
        return Err(ActionError::Illegal("terreno fora da janela de feitiço".into()));
    }
    let in_hand = game
        .state
        .object(object)
        .map(|o| o.zone == ZoneId::hand(player) && o.controller == player)
        .unwrap_or(false);
    if !in_hand {
        return Err(ActionError::BadObject(object));
    }
    let is_land = layers::characteristics(game, object)
        .map(|c| c.type_line.is_land())
        .unwrap_or(false);
    if !is_land {
        return Err(ActionError::Illegal("objeto não é terreno".into()));
    }
    let Some(p) = game.state.players.get(player.index()) else {
        return Err(ActionError::WrongPlayer(player));
    };
    if p.lands_played_this_turn >= p.max_lands_per_turn {
        return Err(ActionError::Illegal("limite de terrenos do turno atingido".into()));
    }

    let name = game.card_name(object);
    turn::move_object(game, object, ZoneId::BATTLEFIELD);
    if let Some(p) = game.state.players.get_mut(player.index()) {
        p.lands_played_this_turn += 1;
    }
    game.state.emit(GameEvent::LandPlayed { player, object });
    game.state.push_log(format!("joga {name}"), Some(player));
    Ok(())
}

/// CR 601.2 — a sequência completa de lançar uma mágica.
fn cast_spell(
    game: &mut Game,
    player: PlayerId,
    object: ObjectId,
    targets: Vec<TargetChoice>,
    x: u32,
    modes: Vec<u8>,
    plan: &[ManaSourceChoice],
) -> Result<(), ActionError> {
    let from = game
        .state
        .object(object)
        .map(|o| o.zone)
        .ok_or(ActionError::BadObject(object))?;
    if from != ZoneId::hand(player) {
        return Err(ActionError::BadObject(object));
    }
    if !can_cast_now(game, player, object) {
        return Err(ActionError::Illegal("timing inválido para lançar".into()));
    }
    let Some(card_id) = card_of(game, object) else {
        return Err(ActionError::BadObject(object));
    };
    let db = Arc::clone(&game.db);
    let Some(card) = db.get(card_id) else {
        return Err(ActionError::BadObject(object));
    };
    let specs = card.spell_targets.clone();
    if targets.len() != specs.len() {
        return Err(ActionError::BadTargets(format!(
            "esperado {} alvo(s), recebido {}",
            specs.len(),
            targets.len()
        )));
    }

    // CR 601.2a — a carta vai para a pilha antes de escolher modos e alvos.
    turn::move_object(game, object, ZoneId::STACK);

    // CR 601.2b — modos.
    let modes = resolve_modes(game, player, card.spell_effect.as_ref(), modes);

    // CR 601.2c — alvos precisam ser legais no momento da escolha.
    let ctx = ctx_for(Some(object), player, x);
    for (spec, target) in specs.iter().zip(targets.iter()) {
        if !query::target_still_legal(game, *target, spec, &ctx) {
            turn::move_object(game, object, ZoneId::hand(player));
            return Err(ActionError::BadTargets(spec.description.clone()));
        }
    }

    // CR 601.2f/g/h — custo total, habilidades de mana, pagamento.
    let symbols = spell_total_cost(game, object, player);
    let cost = Cost::Mana(symbols);
    if let Err(err) = pay_cost_from(game, player, Some(object), &cost, x, plan) {
        // CR 601.2h — pagamento impossível desfaz o lançamento.
        turn::move_object(game, object, ZoneId::hand(player));
        game.state
            .push_log("lançamento revertido: custo não pago", Some(player));
        return Err(err);
    }

    if let Some(p) = game.state.players.get_mut(player.index()) {
        p.spells_cast_this_turn += 1;
    }
    let name = game.card_name(object);
    game.state.push_log(format!("lança {name}"), Some(player));
    game.state.emit(GameEvent::SpellCast { object, controller: player });
    game.push_event(MatchEvent::SpellCast {
        card: object,
        player,
        targets: object_targets(&targets),
    });

    stack::push_spell(game, object, player, targets, x, modes);
    Ok(())
}

/// CR 602.2 — ativar habilidade: custo pago, item vai para a pilha.
fn activate_ability(
    game: &mut Game,
    player: PlayerId,
    source: ObjectId,
    index: u16,
    targets: Vec<TargetChoice>,
    x: u32,
    plan: &[ManaSourceChoice],
) -> Result<(), ActionError> {
    let Some(card_id) = card_of(game, source) else {
        return Err(ActionError::BadObject(source));
    };
    let db = Arc::clone(&game.db);
    let Some(card) = db.get(card_id) else {
        return Err(ActionError::BadObject(source));
    };
    let Some(Ability::Activated(ability)) = card.abilities.get(index as usize) else {
        return Err(ActionError::Illegal(format!(
            "habilidade {index} de {source} não é ativada"
        )));
    };
    let loyalty_used = loyalty_ability_used(game, source, card);
    if !activation_legal(game, player, source, index, ability, loyalty_used) {
        return Err(ActionError::Illegal("habilidade não pode ser ativada agora".into()));
    }
    if targets.len() != ability.targets.len() {
        return Err(ActionError::BadTargets(format!(
            "esperado {} alvo(s), recebido {}",
            ability.targets.len(),
            targets.len()
        )));
    }
    let ctx = ctx_for(Some(source), player, x);
    for (spec, target) in ability.targets.iter().zip(targets.iter()) {
        if !query::target_still_legal(game, *target, spec, &ctx) {
            return Err(ActionError::BadTargets(spec.description.clone()));
        }
    }

    // CR 601.2h — custos são pagos juntos: confere antes de mexer em lealdade,
    // senão um X impagável deixaria o planeswalker com um marcador a menos.
    if !can_pay_from(game, player, Some(source), &ability.cost, x) {
        return Err(ActionError::CannotPay(describe_cost(&ability.cost)));
    }
    // CR 606.2 — o custo de lealdade é marcador, pago junto com o resto.
    if let Some(change) = ability.loyalty_change {
        apply_loyalty(game, source, change);
    }
    let cost = ability.cost.clone();
    pay_cost_from(game, player, Some(source), &cost, x, plan)?;

    if let Some(state) = game.state.object_mut(source) {
        *state.ability_uses.entry(index).or_insert(0) += 1;
    }
    let name = game.card_name(source);
    let text = ability.text.clone();
    game.state
        .push_log(format!("{name} ativa: {text}"), Some(player));
    game.state.emit(GameEvent::AbilityActivated {
        ability: AbilityRef { object: source, index },
        controller: player,
    });
    game.push_event(MatchEvent::Log { text: format!("{name}: {text}") });

    stack::push_activated(game, source, index, player, targets, x);
    Ok(())
}

fn apply_loyalty(game: &mut Game, source: ObjectId, change: i32) {
    if change == 0 {
        return;
    }
    if let Some(state) = game.state.object_mut(source) {
        state.add_counter(CounterKind::Loyalty, change);
    }
    let event = if change > 0 {
        GameEvent::CountersAdded {
            object: source,
            kind: CounterKind::Loyalty,
            amount: change,
        }
    } else {
        GameEvent::CountersRemoved {
            object: source,
            kind: CounterKind::Loyalty,
            amount: -change,
        }
    };
    game.state.emit(event);
    game.push_event(MatchEvent::CountersChanged {
        card: source,
        kind: "Loyalty".to_string(),
        delta: change,
    });
}

/// Se a mágica é modal e o agente não trouxe modos, pergunta agora (CR 601.2b).
fn resolve_modes(
    game: &mut Game,
    player: PlayerId,
    effect: Option<&Effect>,
    modes: Vec<u8>,
) -> Vec<u8> {
    let Some((count, choose)) = modal_option_count(effect) else { return modes };
    if !modes.is_empty() || count == 0 {
        return modes;
    }
    let labels: Vec<String> = match effect {
        Some(Effect::Modal { options, .. }) => options.iter().map(|(l, _)| l.clone()).collect(),
        _ => Vec::new(),
    };
    let action = game.ask(Request::ChooseModes {
        player,
        prompt: "escolha os modos".to_string(),
        options: labels,
        choose,
    });
    match action {
        Action::ChooseModes { modes } => modes,
        // Fallback determinístico: os primeiros modos da lista.
        _ => (0..choose.min(count as u8)).collect(),
    }
}

fn object_targets(targets: &[TargetChoice]) -> Vec<ObjectId> {
    targets
        .iter()
        .filter_map(|t| match t {
            TargetChoice::Object(o) => Some(*o),
            TargetChoice::Player(_) => None,
        })
        .collect()
}
