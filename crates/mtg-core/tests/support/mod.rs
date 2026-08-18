//! Ferramental dos testes `interactions_combat` e `fuzz`.
//!
//! Vive separado de `tests/common/mod.rs` de propósito: os dois arquivos que
//! dependem deste módulo precisam compilar sozinhos, sem esperar por outro
//! arquivo que está sendo escrito em paralelo.
//!
//! Regra de casa: helper que não encontra o que foi pedido **entra em pânico**.
//! Em teste, `Option` engolido silenciosamente vira teste verde que não testou
//! nada — que foi exatamente o achado da auditoria.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Arc;

use mtg_core::action::{Action, Request};
use mtg_core::card::{
    Ability, ActivatedAbility, CardDatabase, CardDef, ManaAbility, ManaProduction,
};
use mtg_core::engine::{sba, stack, triggers, turn, Agent, Game, GameConfig, PlayerConfig};
use mtg_core::event::{Defender, Step};
use mtg_core::ids::{CardDefId, ObjectId, PlayerId};
use mtg_core::ir::{Condition, Cost, Effect, Keyword, TargetSpec, TimingRestriction};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Rarity, TypeLine};
use mtg_core::zone::{ZoneId, ZoneKind};

/// Cópias de cada `CardDef` sintético postas na biblioteca dos dois jogadores.
pub const DECK_COPIES: usize = 6;
/// Semente dos testes de estado montado à mão. Qualquer valor serve; o que
/// importa é ser fixo — determinismo por semente é requisito do projeto.
pub const TEST_SEED: u64 = 7;
/// Teto de resoluções em `resolve_stack`: acima disso a pilha não converge e o
/// teste tem de falhar em vez de girar para sempre.
const STACK_GUARD: u32 = 300;

// ---------------------------------------------------------------------------
// Agentes
// ---------------------------------------------------------------------------

/// Escolha segura quando não há instrução: passar prioridade quando isso for
/// legal, e a primeira ação legal caso contrário — `Request::DeclareBlockers`,
/// por exemplo, não aceita `PassPriority`. `attack_options`/`block_options`
/// põem "nada" em primeiro lugar, então o padrão nunca ataca nem bloqueia por
/// conta própria.
pub fn default_action(legal: &[Action]) -> Action {
    if legal.contains(&Action::PassPriority) {
        return Action::PassPriority;
    }
    legal.first().cloned().unwrap_or(Action::PassPriority)
}

/// Agente de fila: responde as `Action` na ordem em que foram enfileiradas e,
/// quando a fila acaba, cai no padrão seguro.
///
/// Atenção: `Game::ask` **não consulta o agente** quando existe uma única ação
/// legal. Uma fila só avança nas decisões em que o motor de fato pergunta.
pub struct FixedAgent {
    name: String,
    queue: VecDeque<Action>,
}

impl FixedAgent {
    pub fn new(name: &str, actions: Vec<Action>) -> FixedAgent {
        FixedAgent {
            name: name.to_string(),
            queue: actions.into_iter().collect(),
        }
    }

    /// Só passa prioridade (e usa a primeira ação legal quando passar não é
    /// legal).
    pub fn passing(name: &str) -> FixedAgent {
        FixedAgent::new(name, Vec::new())
    }

    pub fn remaining(&self) -> usize {
        self.queue.len()
    }
}

impl Agent for FixedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        match self.queue.pop_front() {
            Some(action) => action,
            None => default_action(legal),
        }
    }
}

/// Agente por closure: decide olhando a `Request` e a lista legal. É o que
/// permite escrever "inverta a ordem dos bloqueadores" sem inventar um agente
/// novo por teste.
pub struct ScriptedAgent {
    name: String,
    decide: Box<dyn FnMut(&Request, &[Action]) -> Action + Send>,
}

impl ScriptedAgent {
    pub fn new<F>(name: &str, f: F) -> ScriptedAgent
    where
        F: FnMut(&Request, &[Action]) -> Action + Send + 'static,
    {
        ScriptedAgent {
            name: name.to_string(),
            decide: Box::new(f),
        }
    }
}

impl Agent for ScriptedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, request: &Request, legal: &[Action]) -> Action {
        (self.decide)(request, legal)
    }
}

/// Bot aleatório determinístico. `Agent::decide` só recebe `&Game`, de
/// propósito — o motor nunca cede mutabilidade ao agente —, então o bot carrega
/// o próprio LCG, semeado a partir da semente da partida.
pub struct RandomAgent {
    name: String,
    state: u64,
}

impl RandomAgent {
    pub fn new(name: &str, seed: u64) -> RandomAgent {
        RandomAgent {
            name: name.to_string(),
            state: seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407),
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state >> 33
    }
}

impl Agent for RandomAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        if legal.is_empty() {
            return Action::PassPriority;
        }
        let idx = (self.next() as usize) % legal.len();
        match legal.get(idx) {
            Some(a) => a.clone(),
            None => Action::PassPriority,
        }
    }
}

pub fn passing_agents() -> Vec<Box<dyn Agent>> {
    vec![
        Box::new(FixedAgent::passing("A")),
        Box::new(FixedAgent::passing("B")),
    ]
}

// ---------------------------------------------------------------------------
// Construção de partida
// ---------------------------------------------------------------------------

/// Sem mão inicial e sem mulligan: a montagem do estado fica inteiramente
/// explícita no corpo do teste.
pub fn test_config() -> GameConfig {
    GameConfig {
        starting_life: 20,
        starting_hand_size: 0,
        allow_mulligan: false,
        max_turns: 60,
        max_decisions: 200_000,
    }
}

/// Partida com catálogo sintético: cada `CardDef` entra `DECK_COPIES` vezes na
/// biblioteca dos dois jogadores. Os ids são reindexados, então o chamador pode
/// deixar `CardDefId(0)` em todos.
pub fn game_with_defs(defs: Vec<CardDef>, agents: Vec<Box<dyn Agent>>) -> Game {
    game_with_defs_seeded(defs, agents, TEST_SEED)
}

pub fn game_with_defs_seeded(
    mut defs: Vec<CardDef>,
    agents: Vec<Box<dyn Agent>>,
    seed: u64,
) -> Game {
    assert!(!defs.is_empty(), "catálogo sintético vazio");
    for (i, def) in defs.iter_mut().enumerate() {
        def.id = CardDefId(i as u32);
    }
    let db = CardDatabase { cards: defs };
    let deck: Vec<CardDefId> = (0..db.cards.len() as u32)
        .flat_map(|i| std::iter::repeat(CardDefId(i)).take(DECK_COPIES))
        .collect();
    let players = vec![
        PlayerConfig {
            name: "A".to_string(),
            deck: deck.clone(),
        },
        PlayerConfig {
            name: "B".to_string(),
            deck,
        },
    ];
    match Game::new(Arc::new(db), players, agents, test_config(), seed) {
        Ok(g) => g,
        Err(err) => panic!("montagem da partida de teste falhou: {err}"),
    }
}

/// Catálogo real, lido de `cards/*.lua`. Falha alto se não carregar: sem cartas
/// não há o que testar.
pub fn catalog() -> Arc<CardDatabase> {
    match mtg_cards::build_database() {
        Ok(db) => Arc::new(db),
        Err(err) => panic!("catálogo real não carregou: {err}"),
    }
}

/// Partida com o catálogo real e decks explícitos.
pub fn game_with_catalog(
    db: Arc<CardDatabase>,
    deck_a: Vec<CardDefId>,
    deck_b: Vec<CardDefId>,
    agents: Vec<Box<dyn Agent>>,
    config: GameConfig,
    seed: u64,
) -> Game {
    let players = vec![
        PlayerConfig {
            name: "Alice".to_string(),
            deck: deck_a,
        },
        PlayerConfig {
            name: "Bob".to_string(),
            deck: deck_b,
        },
    ];
    match Game::new(db, players, agents, config, seed) {
        Ok(g) => g,
        Err(err) => panic!("Game::new falhou (semente {seed}): {err}"),
    }
}

// ---------------------------------------------------------------------------
// Fábrica de `CardDef` sintético
// ---------------------------------------------------------------------------

/// Traduz `"{2}{W/U}{W/P}"` em símbolos. Entrada inválida é pânico: custo
/// escrito errado num teste tem de aparecer na hora, não virar "impagável".
pub fn parse_symbols(spec: &str) -> Vec<ManaSymbol> {
    let mut out = Vec::new();
    let mut rest = spec;
    while let Some(start) = rest.find('{') {
        let Some(offset) = rest[start..].find('}') else {
            panic!("símbolo de mana sem fechamento em {spec:?}");
        };
        let end = start + offset;
        out.push(parse_symbol(&rest[start + 1..end], spec));
        rest = &rest[end + 1..];
    }
    out
}

pub fn parse_cost(spec: &str) -> ManaCost {
    ManaCost {
        symbols: parse_symbols(spec),
    }
}

fn single_color(text: &str) -> Option<Color> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Color::from_letter(first)
}

fn parse_symbol(body: &str, whole: &str) -> ManaSymbol {
    if let Ok(n) = body.parse::<u8>() {
        return ManaSymbol::Generic(n);
    }
    if body.eq_ignore_ascii_case("X") {
        return ManaSymbol::X;
    }
    if body.eq_ignore_ascii_case("C") {
        return ManaSymbol::Colorless;
    }
    if body.eq_ignore_ascii_case("S") {
        return ManaSymbol::Snow;
    }
    if let Some(c) = single_color(body) {
        return ManaSymbol::Colored(c);
    }
    if let Some((left, right)) = body.split_once('/') {
        // CR 107.4d — {C/P}: a cor nomeada ou 2 de vida.
        if right.eq_ignore_ascii_case("P") {
            if let Some(c) = single_color(left) {
                return ManaSymbol::Phyrexian(c);
            }
        }
        // CR 107.4e — {2/C}: 2 genérico ou a cor nomeada.
        if left == "2" {
            if let Some(c) = single_color(right) {
                return ManaSymbol::MonoHybrid(c);
            }
        }
        // CR 107.4a — {C/C}: qualquer uma das duas cores.
        if let (Some(a), Some(b)) = (single_color(left), single_color(right)) {
            return ManaSymbol::Hybrid(a, b);
        }
    }
    panic!("símbolo de mana desconhecido {{{body}}} em {whole:?}");
}

fn blank_card(name: &str, cost: &str, types: Vec<CardType>) -> CardDef {
    CardDef {
        id: CardDefId(0),
        name: name.to_string(),
        mana_cost: parse_cost(cost),
        type_line: TypeLine {
            supertypes: Vec::new(),
            types,
            subtypes: Vec::new(),
        },
        color_override: None,
        power: None,
        toughness: None,
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

pub fn creature_def(
    name: &str,
    cost: &str,
    power: i32,
    toughness: i32,
    keywords: &[Keyword],
) -> CardDef {
    let mut def = blank_card(name, cost, vec![CardType::Creature]);
    def.type_line.subtypes = vec!["Test".to_string()];
    def.power = Some(power);
    def.toughness = Some(toughness);
    def.abilities = keywords.iter().cloned().map(Ability::Keyword).collect();
    def
}

/// Terreno sintético: `{T}: adicione um mana fixo`.
pub fn land_def(name: &str, produces: ManaSymbol) -> CardDef {
    let mut def = blank_card(name, "", vec![CardType::Land]);
    def.type_line.subtypes = vec![name.to_string()];
    def.abilities = vec![Ability::Mana(ManaAbility {
        cost: Cost::Tap,
        production: ManaProduction::Fixed(vec![produces]),
        restriction: Condition::always(),
        text: "{T}: adicione mana.".to_string(),
    })];
    def
}

pub fn instant_def(name: &str, cost: &str, effect: Effect) -> CardDef {
    let mut def = blank_card(name, cost, vec![CardType::Instant]);
    def.spell_effect = Some(effect);
    def
}

pub fn sorcery_def(name: &str, cost: &str, effect: Effect) -> CardDef {
    let mut def = blank_card(name, cost, vec![CardType::Sorcery]);
    def.spell_effect = Some(effect);
    def
}

pub fn instant_def_targeted(
    name: &str,
    cost: &str,
    targets: Vec<TargetSpec>,
    effect: Effect,
) -> CardDef {
    let mut def = instant_def(name, cost, effect);
    def.spell_targets = targets;
    def
}

/// Artefato com uma única habilidade ativada — o veículo dos testes de custo.
pub fn artifact_with_activated(name: &str, cost: &str, ability: ActivatedAbility) -> CardDef {
    let mut def = blank_card(name, cost, vec![CardType::Artifact]);
    def.abilities = vec![Ability::Activated(ability)];
    def
}

pub fn activated_ability(cost: Cost, effect: Effect, text: &str) -> ActivatedAbility {
    ActivatedAbility {
        cost,
        targets: Vec::new(),
        effect,
        timing: TimingRestriction::Instant,
        restriction: Condition::always(),
        uses_per_turn: None,
        loyalty_change: None,
        text: text.to_string(),
    }
}

/// Índice da primeira habilidade ativada da carta. Pânico se não houver: quem
/// chama depende de ela existir.
pub fn first_activated_index(game: &Game, obj: ObjectId) -> u16 {
    let Some(state) = game.state.object(obj) else {
        panic!("{obj} não existe");
    };
    let Some(card) = game.db.get(state.card) else {
        panic!("definição de {obj} não está no catálogo");
    };
    let Some((index, _)) = card.activated().next() else {
        panic!("'{}' não tem habilidade ativada", card.name);
    };
    index as u16
}

pub fn has_mana_ability(game: &Game, obj: ObjectId) -> bool {
    game.state
        .object(obj)
        .and_then(|o| game.db.get(o.card))
        .is_some_and(|c| c.mana_abilities().next().is_some())
}

// ---------------------------------------------------------------------------
// Montagem de estado
// ---------------------------------------------------------------------------

/// Primeiro objeto da zona cuja definição tem este nome.
pub fn find_in_zone(game: &Game, zone: ZoneId, name: &str) -> Option<ObjectId> {
    turn::zone_objects(game, zone).into_iter().find(|id| {
        game.state
            .object(*id)
            .and_then(|o| game.db.get(o.card))
            .is_some_and(|c| c.name.eq_ignore_ascii_case(name))
    })
}

fn take_from_library(game: &mut Game, name: &str, owner: PlayerId) -> ObjectId {
    match find_in_zone(game, ZoneId::library(owner), name) {
        Some(id) => id,
        None => panic!("nenhuma cópia de '{name}' sobrou na biblioteca de {owner}"),
    }
}

/// Coloca uma cópia da carta no campo de batalha sob controle do jogador.
///
/// Deixa o enjoo de invocação como o motor deixa (CR 302.6): quem precisa de
/// criatura pronta para atacar chama `put_ready` ou
/// `clear_summoning_sickness` de propósito.
pub fn put_on_battlefield(game: &mut Game, name: &str, controller: PlayerId) -> ObjectId {
    let id = take_from_library(game, name, controller);
    turn::move_object(game, id, ZoneId::BATTLEFIELD);
    id
}

/// Igual a `put_on_battlefield`, mas já sem enjoo — o caso comum em teste de
/// combate, onde a criatura "já estava lá".
pub fn put_ready(game: &mut Game, name: &str, controller: PlayerId) -> ObjectId {
    let id = put_on_battlefield(game, name, controller);
    clear_summoning_sickness(game, id);
    id
}

pub fn put_in_hand(game: &mut Game, name: &str, player: PlayerId) -> ObjectId {
    let id = take_from_library(game, name, player);
    turn::move_object(game, id, ZoneId::hand(player));
    id
}

pub fn put_in_hand_by_def(game: &mut Game, def: CardDefId, player: PlayerId) -> ObjectId {
    let found = turn::zone_objects(game, ZoneId::library(player))
        .into_iter()
        .find(|id| game.state.object(*id).is_some_and(|o| o.card == def));
    let Some(id) = found else {
        panic!("nenhuma cópia de {def} sobrou na biblioteca de {player}");
    };
    turn::move_object(game, id, ZoneId::hand(player));
    id
}

pub fn put_in_graveyard(game: &mut Game, name: &str, player: PlayerId) -> ObjectId {
    let id = take_from_library(game, name, player);
    turn::move_object(game, id, ZoneId::graveyard(player));
    id
}

pub fn clear_summoning_sickness(game: &mut Game, id: ObjectId) {
    match game.state.object_mut(id) {
        Some(obj) => obj.summoning_sick = false,
        None => panic!("{id} não existe: não dá para tirar o enjoo"),
    }
}

pub fn set_life(game: &mut Game, player: PlayerId, life: i32) {
    game.state.player_mut(player).life = life;
}

pub fn life(game: &Game, player: PlayerId) -> i32 {
    game.state.player(player).life
}

pub fn give_counters(game: &mut Game, obj: ObjectId, kind: CounterKind, count: i32) {
    match game.state.object_mut(obj) {
        Some(state) => state.add_counter(kind, count),
        None => panic!("{obj} não existe: não dá para pôr marcador"),
    }
}

/// Posiciona a partida num passo. Não roda as ações baseadas em turno daquele
/// passo — quem quiser isso chama a função pública do módulo correspondente.
pub fn goto_step(game: &mut Game, step: Step) {
    game.state.step = step;
    game.state.consecutive_passes = 0;
}

pub fn tap(game: &mut Game, id: ObjectId) {
    match game.state.object_mut(id) {
        Some(obj) => obj.tapped = true,
        None => panic!("{id} não existe: não dá para virar"),
    }
}

/// Marca o permanente como atacante sem passar por `declare_attackers` — o
/// cenário do item 65 roda na fase principal do jogador ativo, onde a
/// declaração real não é possível, mas cartas como "destrua a criatura
/// atacante alvo" precisam de um alvo legal.
pub fn set_attacking(game: &mut Game, id: ObjectId, defender: PlayerId) {
    match game.state.object_mut(id) {
        Some(obj) => obj.combat.attacking = Some(Defender::Player(defender)),
        None => panic!("{id} não existe: não dá para marcar como atacante"),
    }
}

pub fn is_tapped(game: &Game, id: ObjectId) -> bool {
    match game.state.object(id) {
        Some(obj) => obj.tapped,
        None => panic!("{id} não existe: não dá para ler o estado de virado"),
    }
}

pub fn damage_on(game: &Game, id: ObjectId) -> i32 {
    match game.state.object(id) {
        Some(obj) => obj.damage,
        None => panic!("{id} não existe: não dá para ler o dano"),
    }
}

pub fn zone_of(game: &Game, id: ObjectId) -> ZoneId {
    match game.state.object(id) {
        Some(obj) => obj.zone,
        None => panic!("{id} não existe: não dá para ler a zona"),
    }
}

pub fn in_graveyard(game: &Game, id: ObjectId) -> bool {
    zone_of(game, id).kind == ZoneKind::Graveyard
}

pub fn on_battlefield(game: &Game, id: ObjectId) -> bool {
    zone_of(game, id).kind == ZoneKind::Battlefield
}

pub fn hand_size(game: &Game, player: PlayerId) -> usize {
    turn::zone_objects(game, ZoneId::hand(player)).len()
}

/// Enche o pool de mana do jogador — o "mana abundante" do item 65, sem ter de
/// montar um campo inteiro de terrenos.
pub fn fill_mana_pool(game: &mut Game, player: PlayerId, amount: u16) {
    let pool = &mut game.state.player_mut(player).mana_pool;
    pool.colored = [amount; 5];
    pool.colorless = amount;
}

/// Zera a fila de eventos e os gatilhos pendentes deixados pela montagem. Sem
/// isso, o gatilho de entrada de uma carta-cenário resolveria no meio do teste
/// e a asserção passaria a medir outra coisa.
pub fn clear_pending_events(game: &mut Game) {
    game.state.event_queue.clear();
    game.state.pending_triggers.clear();
}

/// Resolve a pilha inteira como o laço de prioridade faria: SBA, gatilhos, topo.
pub fn resolve_stack(game: &mut Game) {
    let mut guard = 0u32;
    loop {
        guard += 1;
        assert!(
            guard < STACK_GUARD,
            "pilha não esvaziou em {STACK_GUARD} resoluções"
        );
        sba::check_until_stable(game);
        if game.state.is_over() {
            return;
        }
        triggers::collect(game);
        stack::put_triggers_on_stack(game);
        if game.state.stack.is_empty() {
            return;
        }
        stack::resolve_top(game);
    }
}

// ---------------------------------------------------------------------------
// Verificação de integridade
// ---------------------------------------------------------------------------

/// Toda referência de zona bate dos dois lados? É o mínimo que "sem corromper
/// estado" precisa significar: objeto que se diz na mão sem estar na lista da
/// mão é campo de batalha fantasma esperando para acontecer.
///
/// Fichas ficam de fora: CR 111.7 as faz deixar de existir ao sair do campo, e
/// elas guardam uma zona de destino em que legitimamente não estão.
pub fn assert_zone_bookkeeping(game: &Game) {
    for object in &game.state.objects {
        if object.is_token {
            continue;
        }
        let key = (object.zone.kind, object.zone.owner.map_or(u8::MAX, |p| p.0));
        let Some(zone) = game.state.zones.get(&key) else {
            panic!(
                "{} aponta para zona inexistente {:?}",
                object.id, object.zone
            );
        };
        assert!(
            zone.objects.contains(&object.id),
            "{} diz estar em {:?} mas a zona não o lista",
            object.id,
            object.zone
        );
    }

    for ((kind, owner), zone) in &game.state.zones {
        for id in &zone.objects {
            let Some(object) = game.state.object(*id) else {
                panic!("zona {kind:?}/{owner} lista {id}, que não existe");
            };
            assert_eq!(
                object.zone.kind, *kind,
                "{id} está listado em {kind:?} mas se diz em {:?}",
                object.zone
            );
        }
    }
}
