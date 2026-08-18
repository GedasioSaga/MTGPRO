//! `HeuristicBot`: pontua cada `Action` legal e escolhe a melhor.
//!
//! Como a nota é montada, em duas camadas:
//!   1. **Previsão** — `sim::apply_action` dá um passo à frente sobre uma cópia
//!      do `Snapshot` e `eval::evaluate` mede o resultado. É o que resolve
//!      combate, remoção, queima e rampa sem uma regra escrita à mão para cada.
//!   2. **Correção de papel** — o que a avaliação estática não enxerga:
//!      instantâneo reativo vale mais guardado, remoção não se gasta em alvo
//!      irrelevante, e terreno vale muito mais do que os pontos que o
//!      avaliador dá a uma fonte de mana.
//!
//! Determinismo é requisito. Nenhuma nota vem de `HashMap`, de ponteiro ou de
//! relógio: a lista `legal` já chega ordenada do motor, a varredura escolhe o
//! **primeiro** máximo estrito, e o único ruído é `eval::jitter`, que é função
//! pura da semente do bot e do contador de decisões.
use mtg_core::action::{Action, Request, TargetChoice};
use mtg_core::engine::{Agent, Game};
use mtg_core::event::Step;
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::Effect;
use mtg_core::mana::Color;
use mtg_core::state::GameOutcome;
use mtg_core::zone::ZoneId;

use crate::cards::{self, SpellRole};
use crate::eval::{self, CreatureInfo, Side, Snapshot};
use crate::sim;

// ---------------------------------------------------------------------------
// Escalas
// ---------------------------------------------------------------------------
//
// Três faixas que nunca se cruzam, para que a ordem de prioridade seja
// propriedade dos números e não da ordem em que os `match` foram escritos:
//
//   |nota| >= WIN_BONUS   → a jogada ganha (ou perde) a partida agora
//   nota  == LAND_SCORE   → jogar terreno, acima de qualquer jogada normal
//   |nota| <  LAND_SCORE  → tudo o mais, na escala de centipontos de `eval`

/// Uma jogada que leva a posição a terminal. `evaluate` devolve `TERMINAL`
/// nesse caso, então metade dele já reconhece a situação com folga.
const WIN_THRESHOLD: i64 = eval::TERMINAL / 2;
const WIN_BONUS: i64 = 10_000_000;
/// Jogar terreno é a jogada mais importante que existe fora de ganhar: cada
/// terreno perdido é uma carta a menos lançável em todos os turnos seguintes.
/// A avaliação estática não vê isso (ela desconta a carta da mão), então a
/// decisão sai de uma faixa própria em vez de sair de pesos afinados.
const LAND_SCORE: i64 = 900_000;

/// Bônus por ainda ter a carta na mão quando o momento certo não chegou.
const HOLD_REACTIVE: i64 = 600;
/// Remoção queimada em alvo irrelevante custa a carta inteira.
const WASTED_REMOVAL: i64 = 900;
/// Mirar o próprio material com efeito hostil quase nunca é o que se quer.
const SELF_HARM: i64 = 5_000;

// ---------------------------------------------------------------------------
// Bot
// ---------------------------------------------------------------------------

pub struct HeuristicBot {
    seed: u64,
    /// Contador de decisões: entra no `jitter` para que duas decisões
    /// diferentes não recebam exatamente o mesmo ruído de desempate.
    decisions: u64,
}

impl HeuristicBot {
    pub fn new(seed: u64) -> HeuristicBot {
        HeuristicBot { seed, decisions: 0 }
    }
}

impl Agent for HeuristicBot {
    fn name(&self) -> &str {
        "heuristic"
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        // `Game::ask` já garante lista não-vazia, mas o agente não deve
        // confiar num invariante do chamador.
        let Some(first) = legal.first() else {
            return Action::PassPriority;
        };
        if legal.len() == 1 {
            return first.clone();
        }
        self.decisions = self.decisions.wrapping_add(1);
        let Some(me) = request.player() else {
            return first.clone();
        };
        let s = Snapshot::from_game(game, me);
        let ctx = Ctx { game, s: &s, me, request };
        pick_best(legal, self.seed, self.decisions, |action| score(&ctx, action))
    }

    fn on_game_end(&mut self, _game: &Game, _outcome: GameOutcome) {}
}

/// Varre a lista uma vez e devolve o **primeiro** máximo estrito. Empate
/// resolvido pela posição na lista (que o motor gera em ordem estável) mais o
/// ruído determinístico de `jitter`.
pub(crate) fn pick_best(
    legal: &[Action],
    seed: u64,
    decisions: u64,
    mut score_of: impl FnMut(&Action) -> i64,
) -> Action {
    let mut best_index = 0usize;
    let mut best_score = i64::MIN;
    for (i, action) in legal.iter().enumerate() {
        let value = score_of(action).saturating_add(eval::jitter(seed ^ decisions, i));
        if value > best_score {
            best_score = value;
            best_index = i;
        }
    }
    match legal.get(best_index) {
        Some(a) => a.clone(),
        None => Action::PassPriority,
    }
}

// ---------------------------------------------------------------------------
// Contexto de uma decisão
// ---------------------------------------------------------------------------

pub(crate) struct Ctx<'a> {
    pub game: &'a Game,
    pub s: &'a Snapshot,
    pub me: PlayerId,
    pub request: &'a Request,
}

impl Ctx<'_> {
    /// Nota da posição depois de aplicar a ação, do meu ponto de vista.
    fn after(&self, action: &Action) -> i64 {
        let mut next = self.s.clone();
        sim::apply_action(&mut next, self.game, action);
        eval::evaluate(&next)
    }

    /// Ganho previsto da ação em relação a não fazer nada.
    fn delta(&self, action: &Action) -> i64 {
        self.after(action) - eval::evaluate(self.s)
    }

    fn under_pressure(&self) -> bool {
        eval::incoming_damage(self.s) * 2 >= self.s.my_life
    }

    fn my_creature(&self, id: ObjectId) -> Option<&CreatureInfo> {
        self.s.my_creatures.iter().find(|c| c.id == id)
    }

    fn opp_creature(&self, id: ObjectId) -> Option<&CreatureInfo> {
        self.s.opp_creatures.iter().find(|c| c.id == id)
    }
}

pub(crate) fn make_ctx<'a>(
    game: &'a Game,
    s: &'a Snapshot,
    me: PlayerId,
    request: &'a Request,
) -> Ctx<'a> {
    Ctx { game, s, me, request }
}

// ---------------------------------------------------------------------------
// Roteador
// ---------------------------------------------------------------------------

pub(crate) fn score(ctx: &Ctx, action: &Action) -> i64 {
    match ctx.request {
        Request::Priority { .. } => score_priority(ctx, action),
        Request::Mulligan { .. } => score_mulligan(ctx, action),
        Request::BottomCards { .. } => score_bottom(ctx, action),
        Request::DeclareAttackers { .. } | Request::DeclareBlockers { .. } => {
            score_combat_declaration(ctx, action)
        }
        Request::OrderBlockers { .. } => score_order_blockers(ctx, action),
        Request::AssignCombatDamage { .. } => score_assign_damage(ctx, action),
        Request::ConfirmOptional { prompt, .. } => score_confirm(prompt, action),
        Request::ChooseModes { options, .. } => score_modes(options, action),
        Request::SelectObjects { prompt, .. } => score_selection(ctx, prompt, action),
        Request::ChooseColor { .. } => score_color(ctx, action),
        Request::ArrangeCards { .. } => score_arrange(ctx, action),
        Request::OrderTriggers { .. } | Request::GameOver => 0,
    }
}

// ---------------------------------------------------------------------------
// Prioridade
// ---------------------------------------------------------------------------

fn score_priority(ctx: &Ctx, action: &Action) -> i64 {
    match action {
        // Passar é a linha de base: qualquer jogada útil tem de valer mais que
        // zero para ser feita, e é isso que impede o bot de terminar o turno
        // com mana sobrando havendo jogada boa.
        Action::PassPriority => 0,
        // CR 305.1 — terreno é ação especial e não disputa com mágica.
        Action::PlayLand { .. } => LAND_SCORE,
        Action::CastSpell { object, targets, x, .. } => {
            score_spell(ctx, action, *object, targets, *x)
        }
        Action::ActivateAbility { source, index, targets, x, .. } => {
            score_ability(ctx, action, *source, *index, targets, *x)
        }
        // Conceder nunca é a jogada.
        Action::Concede => -WIN_BONUS,
        _ => 0,
    }
}

fn score_spell(
    ctx: &Ctx,
    action: &Action,
    object: ObjectId,
    targets: &[TargetChoice],
    x: u32,
) -> i64 {
    let delta = ctx.delta(action);
    if let Some(terminal) = terminal_score(delta) {
        return terminal;
    }
    let Some(def) = cards::card_def(ctx.game, object) else {
        return delta;
    };
    let role = cards::classify(def);
    let adjustment = role_adjustment(
        ctx,
        role,
        def.mana_value(),
        cards::is_instant_speed(def),
        def.spell_effect.as_ref(),
        targets,
        x,
    );
    delta + adjustment
}

fn score_ability(
    ctx: &Ctx,
    action: &Action,
    source: ObjectId,
    index: u16,
    targets: &[TargetChoice],
    x: u32,
) -> i64 {
    let delta = ctx.delta(action);
    if let Some(terminal) = terminal_score(delta) {
        return terminal;
    }
    let Some(def) = cards::card_def(ctx.game, source) else {
        return delta;
    };
    let Some((_, ability)) = def.activated().find(|(i, _)| *i == index as usize) else {
        return delta;
    };
    let role = cards::classify_effect(&ability.effect);
    // Habilidade ativada é repetível: não há carta a guardar, então o bônus de
    // segurar reativo não se aplica — daí `instant_speed = false`.
    let adjustment = role_adjustment(
        ctx,
        role,
        def.mana_value(),
        false,
        Some(&ability.effect),
        targets,
        x,
    );
    delta + adjustment
}

/// Jogada que decide a partida agora domina qualquer consideração de material.
fn terminal_score(delta: i64) -> Option<i64> {
    if delta >= WIN_THRESHOLD {
        Some(WIN_BONUS.saturating_add(delta))
    } else if delta <= -WIN_THRESHOLD {
        Some((-WIN_BONUS).saturating_add(delta))
    } else {
        None
    }
}

/// O que a avaliação estática não sabe: quando a carta vale mais na mão, e
/// quando o alvo não paga a carta gasta nele.
fn role_adjustment(
    ctx: &Ctx,
    role: SpellRole,
    mana_value: u32,
    instant_speed: bool,
    effect: Option<&Effect>,
    targets: &[TargetChoice],
    x: u32,
) -> i64 {
    let s = ctx.s;
    let mut adj = 0i64;

    // Efeito hostil apontado para o próprio material é quase sempre acidente
    // de enumeração de alvo, não jogada.
    if matches!(role, SpellRole::Removal | SpellRole::Burn) && hits_own_side(ctx, targets) {
        adj -= SELF_HARM;
    }

    match role {
        SpellRole::Removal => {
            let threshold = eval::removal_threshold(mana_value);
            if already_answered(ctx, targets) {
                return adj - WASTED_REMOVAL * 2;
            }
            match best_hostile_target(ctx, targets) {
                Some(value) if value >= threshold => adj += 150,
                // Alvo irrelevante: a carta vale mais guardada para a ameaça
                // de verdade que ainda vai aparecer.
                Some(_) => adj -= WASTED_REMOVAL,
                None => {}
            }
            // Remoção instantânea rende mais no turno do oponente: lá dá para
            // ver o que ele comprometeu. Sob pressão, esperar não é opção.
            if instant_speed && s.is_my_turn && !ctx.under_pressure() && !in_combat(s.step) {
                adj -= HOLD_REACTIVE / 2;
            }
        }
        SpellRole::Burn => {
            if already_answered(ctx, targets) {
                return adj - WASTED_REMOVAL * 2;
            }
            let damage = effect.map_or(0, |e| cards::fixed_damage(e, x));
            let threshold = eval::removal_threshold(mana_value);
            if targets_a_player(targets, s.opponent) {
                // Dano na cara sem fechar o jogo é o pior uso de uma carta de
                // queima enquanto houver criatura que ela mataria. O caso
                // letal já saiu por `terminal_score`, antes de chegar aqui.
                if cards::has_worthy_creature_target(s, threshold) {
                    adj -= 400;
                } else {
                    adj += 60;
                }
            } else {
                match hostile_creature_targets(ctx, targets).max() {
                    Some((value, toughness)) if damage >= toughness && value >= threshold => {
                        adj += 120
                    }
                    Some(_) => adj -= WASTED_REMOVAL,
                    None => {}
                }
            }
        }
        SpellRole::Counter => {
            // O que importa é o alvo escolhido, não "existe mágica adversária
            // na pilha": o motor enumera todo alvo legal, e anular a própria
            // mágica é legal. Sem esta checagem o bot anulava a criatura que
            // ele mesmo tinha acabado de lançar.
            adj += match counter_target_value(ctx, targets) {
                Some(value) => 400 + value / 2,
                None => -SELF_HARM,
            };
        }
        SpellRole::Pump => {
            let mine = targets
                .iter()
                .any(|t| matches!(t, TargetChoice::Object(id) if ctx.my_creature(*id).is_some()));
            if !mine {
                adj -= 2_000;
            } else if !instant_speed {
                // Efeito de massa em velocidade de feitiço não tem "momento
                // certo": a previsão decide sozinha.
            } else if in_combat(s.step) {
                adj += 300;
            } else {
                // Truque de combate vale o dobro depois de o oponente se
                // comprometer. Fora do combate, segurar.
                adj -= HOLD_REACTIVE;
            }
        }
        SpellRole::Draw => adj += 120,
        SpellRole::Ramp => adj += 80,
        // Desenvolver o campo é o que ganha partida longa; a avaliação
        // estática desconta a carta da mão e subestima o tempo.
        SpellRole::Creature => {
            adj += 100;
            if mana_value as usize + 1 >= s.my_lands {
                adj += 40; // usa o turno de mana inteiro
            }
        }
        SpellRole::Land | SpellRole::Other => {}
    }
    adj
}

fn in_combat(step: Step) -> bool {
    matches!(
        step,
        Step::DeclareAttackers
            | Step::DeclareBlockers
            | Step::FirstStrikeDamage
            | Step::CombatDamage
    )
}

fn targets_a_player(targets: &[TargetChoice], who: PlayerId) -> bool {
    targets
        .iter()
        .any(|t| matches!(t, TargetChoice::Player(p) if *p == who))
}

fn hits_own_side(ctx: &Ctx, targets: &[TargetChoice]) -> bool {
    targets.iter().any(|t| match t {
        TargetChoice::Player(p) => *p == ctx.me,
        TargetChoice::Object(id) => ctx.my_creature(*id).is_some(),
    })
}

/// Ameaça do melhor alvo adversário entre os escolhidos.
fn best_hostile_target(ctx: &Ctx, targets: &[TargetChoice]) -> Option<i64> {
    targets
        .iter()
        .filter_map(|t| match t {
            TargetChoice::Object(id) => ctx.opp_creature(*id),
            _ => None,
        })
        .map(|c| cards::threat_value(ctx.s, c))
        .max()
}

/// `(ameaça, resistência efetiva)` de cada criatura adversária mirada.
fn hostile_creature_targets<'a>(
    ctx: &'a Ctx<'a>,
    targets: &'a [TargetChoice],
) -> impl Iterator<Item = (i64, i32)> + 'a {
    targets.iter().filter_map(move |t| match t {
        TargetChoice::Object(id) => ctx
            .opp_creature(*id)
            .map(|c| (cards::threat_value(ctx.s, c), c.effective_toughness())),
        _ => None,
    })
}

/// Valor da mágica que este contra-feitiço vai anular — `None` quando o alvo
/// escolhido não é uma mágica adversária ainda sem resposta.
fn counter_target_value(ctx: &Ctx, targets: &[TargetChoice]) -> Option<i64> {
    let target = targets.iter().find_map(|t| match t {
        TargetChoice::Object(id) => Some(*id),
        TargetChoice::Player(_) => None,
    })?;
    let item = ctx.game.state.stack.iter().find(|it| it.id == target)?;
    if item.controller == ctx.me {
        return None;
    }
    if already_answered(ctx, targets) {
        return None;
    }
    Some(cards::object_value(ctx.game, ctx.s, target))
}

/// Já existe mágica minha na pilha mirando algum destes alvos?
///
/// Empilhar duas respostas no mesmo alvo é um dois-por-um a favor do
/// oponente: a segunda resolve com o alvo já ilegal e vai direto para o
/// cemitério. Era o erro mais caro que o bot cometia — dois `Swords to
/// Plowshares` na mesma criatura, dois contra-feitiços na mesma mágica.
fn already_answered(ctx: &Ctx, targets: &[TargetChoice]) -> bool {
    ctx.game.state.stack.iter().any(|item| {
        item.controller == ctx.me
            && item
                .targets
                .iter()
                .any(|mine| targets.contains(mine))
    })
}

// ---------------------------------------------------------------------------
// Combate
// ---------------------------------------------------------------------------

/// Atacar e bloquear saem da mesma conta: aplicar a declaração ao snapshot,
/// deixar `sim::plan_blocks`/`sim::simulate_combat` resolverem o combate
/// inteiro e medir o que sobrou. Como `simulate_combat` vira os atacantes, a
/// avaliação já enxerga o contra-ataque sem bloqueador em casa — é daí que sai
/// o "não ataco com a única criatura que segura o troco", sem regra explícita.
fn score_combat_declaration(ctx: &Ctx, action: &Action) -> i64 {
    let mut value = ctx.after(action);
    match action {
        // Pressão de leve como desempate: entre duas linhas de valor idêntico,
        // a que declara mais atacantes fecha a partida mais cedo.
        Action::Attack { assignments } => value += assignments.len() as i64 * 2,
        Action::Block { assignments } => {
            if matches_planned_blocks(ctx, assignments) {
                value += 50;
            }
        }
        _ => {}
    }
    value
}

/// O bloqueio proposto é o mesmo que `plan_blocks` escolheria? A enumeração do
/// motor tem teto, então o plano ideal pode não estar na lista; quando está,
/// ele desempata a favor.
fn matches_planned_blocks(ctx: &Ctx, assignments: &[(ObjectId, ObjectId)]) -> bool {
    let Request::DeclareBlockers { attackers, .. } = ctx.request else {
        return false;
    };
    let mut plan = sim::plan_blocks(ctx.s, Side::Me, attackers);
    if plan.len() != assignments.len() {
        return false;
    }
    let mut chosen: Vec<(ObjectId, ObjectId)> = assignments.to_vec();
    plan.sort_unstable();
    chosen.sort_unstable();
    plan == chosen
}

/// Ordem de dano entre bloqueadores: o mais frágil primeiro, para que o mesmo
/// poder mate o maior número de corpos (CR 510.1c).
fn score_order_blockers(ctx: &Ctx, action: &Action) -> i64 {
    let Action::OrderBlockers { order, .. } = action else {
        return 0;
    };
    let n = order.len() as i64;
    order
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let toughness = ctx.s.find(*id).map_or(0, |c| c.effective_toughness()) as i64;
            (n - i as i64) * -toughness
        })
        .sum()
}

/// Divisão de dano: matar bloqueador vale mais que o ponto de vida que passa,
/// mas o excedente de atropelar não se joga fora.
fn score_assign_damage(ctx: &Ctx, action: &Action) -> i64 {
    let Action::AssignDamage { attacker, assignments, trample_to_defender } = action else {
        return 0;
    };
    let deathtouch = ctx.s.find(*attacker).is_some_and(|c| c.traits.deathtouch);
    let mut score = *trample_to_defender as i64 * eval::LIFE;
    for (blocker, amount) in assignments {
        let Some(c) = ctx.s.find(*blocker) else { continue };
        if c.traits.indestructible {
            continue;
        }
        let lethal = (deathtouch && *amount > 0) || *amount >= c.effective_toughness();
        if lethal {
            score += c.value();
        }
    }
    score
}

// ---------------------------------------------------------------------------
// Mão inicial
// ---------------------------------------------------------------------------

/// Mulligan de Londres: manta-se sempre 7 e devolve-se depois. Mão sem terreno
/// não joga e mão só de terreno não faz nada; entre 2 e 5 o deck funciona. A
/// partir do terceiro mulligan a mão encolhe demais para o remédio valer.
fn score_mulligan(ctx: &Ctx, action: &Action) -> i64 {
    let Request::Mulligan { player, mulligans_taken } = ctx.request else {
        return 0;
    };
    let hand = eval::zone_objects(&ctx.game.state, ZoneId::hand(*player));
    let lands = hand.iter().filter(|id| is_land(ctx.game, **id)).count();
    let keep = *mulligans_taken >= 2 || hand.is_empty() || (2..=5).contains(&lands);
    let vote = if keep { 100 } else { -100 };
    match action {
        Action::KeepHand => vote,
        Action::Mulligan => -vote,
        _ => 0,
    }
}

/// Devolve ao fundo as cartas menos úteis para a mão que fica.
fn score_bottom(ctx: &Ctx, action: &Action) -> i64 {
    let Action::PutOnBottom { objects } = action else {
        return 0;
    };
    let Some(player) = ctx.request.player() else {
        return 0;
    };
    let hand = eval::zone_objects(&ctx.game.state, ZoneId::hand(player));
    let lands_in_hand = hand.iter().filter(|id| is_land(ctx.game, **id)).count();
    // Maximizar a nota equivale a mandar embora o de menor valor de mão.
    -objects
        .iter()
        .map(|id| opening_hand_value(ctx, *id, lands_in_hand))
        .sum::<i64>()
}

/// Quanto esta carta vale numa mão inicial, dado quantos terrenos já há nela.
/// Diferente de `object_value`: aqui o que importa é a curva, não o corpo.
fn opening_hand_value(ctx: &Ctx, id: ObjectId, lands_in_hand: usize) -> i64 {
    let Some(def) = cards::card_def(ctx.game, id) else {
        return 0;
    };
    if def.type_line.is_land() {
        return match lands_in_hand {
            0..=2 => 300,
            3..=4 => 120,
            _ => -60,
        };
    }
    let mv = def.mana_value() as i64;
    let base = cards::object_value(ctx.game, ctx.s, id) / 3;
    // Curva alta demais numa mão inicial é carta morta por vários turnos.
    base - (mv - 3).max(0) * 90
}

fn is_land(game: &Game, id: ObjectId) -> bool {
    cards::card_def(game, id).is_some_and(|d| d.type_line.is_land())
}

// ---------------------------------------------------------------------------
// Escolhas genéricas
// ---------------------------------------------------------------------------

fn score_confirm(prompt: &str, action: &Action) -> i64 {
    let Action::Confirm { yes } = action else {
        return 0;
    };
    let wants = !cards::prompt_has_downside(prompt);
    if *yes == wants {
        100
    } else {
        -100
    }
}

fn score_modes(options: &[String], action: &Action) -> i64 {
    let Action::ChooseModes { modes } = action else {
        return 0;
    };
    modes
        .iter()
        .filter_map(|i| options.get(*i as usize))
        .map(|text| cards::text_value(text))
        .sum()
}

/// Seleção de custo (sacrificar, descartar) escolhe o objeto menos valioso;
/// seleção de benefício (procurar, devolver) escolhe o mais valioso.
fn score_selection(ctx: &Ctx, prompt: &str, action: &Action) -> i64 {
    let Action::SelectObjects { objects } = action else {
        return 0;
    };
    let total: i64 = objects
        .iter()
        .map(|id| cards::object_value(ctx.game, ctx.s, *id))
        .sum();
    if cards::prompt_is_cost(prompt) {
        // Menos objetos e mais baratos: o mínimo que o custo aceita.
        -total - objects.len() as i64 * 10
    } else {
        total + objects.len() as i64 * 10
    }
}

/// Cor mais representada no campo adversário — é a que mais aparece em
/// proteção, prevenção de dano e escolha de cor em geral.
fn score_color(ctx: &Ctx, action: &Action) -> i64 {
    let Action::ChooseColor { color } = action else {
        return 0;
    };
    let mut counts = [0i64; 5];
    for id in eval::zone_objects(&ctx.game.state, ZoneId::BATTLEFIELD) {
        let Some(ch) = ctx.game.characteristics(*id) else {
            continue;
        };
        if ch.controller == ctx.me {
            continue;
        }
        for (i, c) in Color::ALL.into_iter().enumerate() {
            if ch.colors.contains(c) {
                counts[i] += 1;
            }
        }
    }
    Color::ALL
        .into_iter()
        .position(|c| c == *color)
        .and_then(|i| counts.get(i).copied())
        .unwrap_or(0)
}

/// Vidência: fica no topo o que se quer comprar agora, na ordem em que se quer
/// comprar. Posição pesa — a primeira carta chega já no próximo turno.
fn score_arrange(ctx: &Ctx, action: &Action) -> i64 {
    let Action::ArrangeCards { top, alt } = action else {
        return 0;
    };
    let mut score = 0i64;
    for (i, id) in top.iter().enumerate() {
        let weight = match i {
            0 => 3,
            1 => 2,
            _ => 1,
        };
        score += cards::draw_desirability(ctx.game, ctx.s, *id) * weight;
    }
    for id in alt {
        // Mandar embora carta boa custa; mandar embora carta ruim é o objetivo.
        score -= cards::draw_desirability(ctx.game, ctx.s, *id).max(0);
    }
    score
}

// ---------------------------------------------------------------------------
// Acesso para o `GreedyBot`
// ---------------------------------------------------------------------------

/// Nota puramente prospectiva: aplica a ação e mede. Sem correção de papel —
/// é exatamente isso que separa a busca rasa da heurística.
pub(crate) fn lookahead_score(ctx: &Ctx, action: &Action) -> i64 {
    match action {
        // A avaliação estática subestima terreno (ver `LAND_SCORE`); mesmo a
        // busca rasa precisa desta exceção, senão nunca desenvolve mana.
        Action::PlayLand { .. } => LAND_SCORE,
        Action::Concede => -WIN_BONUS,
        Action::Attack { .. } | Action::Block { .. } => score_combat_declaration(ctx, action),
        _ => {
            let delta = ctx.delta(action);
            terminal_score(delta).unwrap_or(delta)
        }
    }
}
