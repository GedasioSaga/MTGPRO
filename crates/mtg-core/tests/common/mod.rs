//! Ferramental compartilhado de **todos** os testes de integração do motor.
//!
//! Módulo único: `interactions_core`, `interactions_combat` e `fuzz` importam
//! daqui. (Houve um segundo módulo `tests/support/mod.rs` enquanto os arquivos
//! eram escritos em paralelo; ele foi fundido aqui e removido.)
//!
//! Quatro coisas vivem aqui e nada mais:
//!   1. Agentes determinísticos (`FixedAgent`, `ScriptedAgent`, `RandomAgent`) —
//!      o motor só avança quando alguém decide, e teste com decisão aleatória
//!      não semeada não afirma nada.
//!   2. Montagem de partida — `Setup` (catálogo real + cartas sintéticas) e
//!      `game_with_defs` (catálogo puramente sintético).
//!   3. Fábrica de `CardDef` sintético.
//!   4. Manipuladores e leitores de estado (`put_on_battlefield`, `set_life`,
//!      `give_counters`, `goto_step`, …) — montar o estado exato de uma regra
//!      sem ter de jogar dez turnos até ele acontecer.
//!
//! Regra de casa: helper que não encontra o que foi pedido **entra em pânico**.
//! Em teste, silêncio é pior que falha — um `Option` ignorado vira teste verde
//! que não testou nada.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use mtg_core::action::{Action, Request, TargetChoice};
use mtg_core::card::{
    Ability, ActivatedAbility, CardDatabase, CardDef, ManaAbility, ManaProduction,
};
use mtg_core::engine::query::EvalCtx;
use mtg_core::engine::{sba, stack, triggers, turn};
use mtg_core::engine::{Agent, Game, GameConfig, PlayerConfig};
use mtg_core::event::{Defender, Step};
use mtg_core::ids::{CardDefId, ObjectId, PlayerId};
use mtg_core::ir::{Condition, Cost, Effect, Keyword, TargetSpec, TimingRestriction};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Rarity, Supertype, TypeLine};
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

/// Escolha segura quando não há instrução: passar prioridade se for legal, e a
/// primeira ação legal caso contrário (declarar bloqueadores, por exemplo, não
/// aceita `PassPriority`). `attack_options`/`block_options` põem "nada" em
/// primeiro lugar, então o padrão nunca ataca nem bloqueia sozinho.
pub fn default_action(legal: &[Action]) -> Action {
    if legal.contains(&Action::PassPriority) {
        return Action::PassPriority;
    }
    legal.first().cloned().unwrap_or(Action::PassPriority)
}

/// Contador compartilhado — o agente vira `Box<dyn Agent>` dentro do `Game` e
/// não dá para lê-lo de volta sem isto.
pub type Counter = Arc<Mutex<usize>>;

pub fn counter() -> Counter {
    Arc::new(Mutex::new(0))
}

pub fn count_of(c: &Counter) -> usize {
    match c.lock() {
        Ok(g) => *g,
        Err(e) => panic!("contador de teste envenenado: {e}"),
    }
}

/// Registro compartilhado de observações feitas de dentro de um agente.
pub type Log<T> = Arc<Mutex<Vec<T>>>;

pub fn log<T>() -> Log<T> {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn push_log<T>(l: &Log<T>, item: T) {
    match l.lock() {
        Ok(mut g) => g.push(item),
        Err(e) => panic!("log de teste envenenado: {e}"),
    }
}

pub fn read_log<T: Clone>(l: &Log<T>) -> Vec<T> {
    match l.lock() {
        Ok(g) => g.clone(),
        Err(e) => panic!("log de teste envenenado: {e}"),
    }
}

/// Agente de fila: responde as `Action` na ordem em que foram enfileiradas e,
/// quando a fila acaba, passa prioridade.
///
/// Atenção ao usar: `Game::ask` **não consulta o agente** quando só existe uma
/// ação legal (ele devolve a única direto). Uma fila só avança nas decisões em
/// que o motor de fato pergunta.
pub struct FixedAgent {
    name: String,
    queue: VecDeque<Action>,
    asked: Option<Counter>,
}

impl FixedAgent {
    pub fn new(name: &str, actions: Vec<Action>) -> FixedAgent {
        FixedAgent {
            name: name.to_string(),
            queue: actions.into_iter().collect(),
            asked: None,
        }
    }

    /// Sempre passa prioridade; usa a primeira ação legal quando passar não é
    /// legal.
    pub fn passing(name: &str) -> FixedAgent {
        FixedAgent::new(name, Vec::new())
    }

    /// Conta quantas vezes o motor de fato perguntou a este agente.
    pub fn counting(mut self, c: Counter) -> FixedAgent {
        self.asked = Some(c);
        self
    }

    /// Quantas ações enfileiradas ainda não foram consumidas.
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    pub fn boxed(self) -> Box<dyn Agent> {
        Box::new(self)
    }
}

impl Agent for FixedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        if let Some(c) = &self.asked {
            match c.lock() {
                Ok(mut g) => *g += 1,
                Err(e) => panic!("contador de teste envenenado: {e}"),
            }
        }
        match self.queue.pop_front() {
            Some(action) => action,
            None => default_action(legal),
        }
    }
}

/// Agente por closure: decide olhando a `Request` e a lista legal (e, na versão
/// `with_game`, o estado inteiro). É o que permite escrever "recuse este
/// gatilho opcional" ou "escolha a segunda lenda" sem inventar um agente novo
/// por teste.
pub struct ScriptedAgent {
    name: String,
    decide: Box<dyn FnMut(&Game, &Request, &[Action]) -> Action + Send>,
}

impl ScriptedAgent {
    pub fn new(
        name: &str,
        mut f: impl FnMut(&Request, &[Action]) -> Action + Send + 'static,
    ) -> ScriptedAgent {
        ScriptedAgent {
            name: name.to_string(),
            decide: Box::new(move |_game, request, legal| f(request, legal)),
        }
    }

    pub fn with_game(
        name: &str,
        f: impl FnMut(&Game, &Request, &[Action]) -> Action + Send + 'static,
    ) -> ScriptedAgent {
        ScriptedAgent {
            name: name.to_string(),
            decide: Box::new(f),
        }
    }

    /// Observa cada decisão e responde o padrão seguro.
    pub fn observing(
        name: &str,
        mut f: impl FnMut(&Game, &Request) + Send + 'static,
    ) -> ScriptedAgent {
        ScriptedAgent::with_game(name, move |game, request, legal| {
            f(game, request);
            default_action(legal)
        })
    }

    pub fn boxed(self) -> Box<dyn Agent> {
        Box::new(self)
    }
}

impl Agent for ScriptedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        (self.decide)(game, request, legal)
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
        FixedAgent::passing("A").boxed(),
        FixedAgent::passing("B").boxed(),
    ]
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

/// Carta em branco: sem custo, sem habilidade, sem P/T. O `id` é sobrescrito
/// por `Setup::add_card` ou por `game_with_defs`, que é quem sabe a posição real
/// no catálogo.
pub fn card_def(name: &str, types: Vec<CardType>) -> CardDef {
    blank_card(name, "", types)
}

/// Carta em branco com custo de mana escrito em notação de símbolo.
pub fn blank_card(name: &str, cost: &str, types: Vec<CardType>) -> CardDef {
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

/// Criatura baunilha e grátis — o corpo genérico dos testes de regra.
pub fn creature_def(name: &str, power: i32, toughness: i32) -> CardDef {
    let mut c = card_def(name, vec![CardType::Creature]);
    c.power = Some(power);
    c.toughness = Some(toughness);
    c
}

/// Criatura com custo e palavras-chave — o corpo dos testes de combate.
pub fn creature_def_costed(
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

pub fn legendary_creature_def(name: &str, power: i32, toughness: i32) -> CardDef {
    let mut c = creature_def(name, power, toughness);
    c.type_line.supertypes.push(Supertype::Legendary);
    c
}

pub fn enchantment_def(name: &str) -> CardDef {
    card_def(name, vec![CardType::Enchantment])
}

pub fn aura_def(name: &str) -> CardDef {
    let mut c = enchantment_def(name);
    c.type_line.subtypes.push("Aura".to_string());
    c
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

pub fn instant_def(name: &str, effect: Effect, targets: Vec<TargetSpec>) -> CardDef {
    let mut c = card_def(name, vec![CardType::Instant]);
    c.spell_effect = Some(effect);
    c.spell_targets = targets;
    c
}

pub fn sorcery_def(name: &str, effect: Effect, targets: Vec<TargetSpec>) -> CardDef {
    let mut c = card_def(name, vec![CardType::Sorcery]);
    c.spell_effect = Some(effect);
    c.spell_targets = targets;
    c
}

/// Instantânea com custo de mana real — usada onde o teste paga o custo de
/// verdade em vez de empurrar a mágica direto para a pilha.
pub fn instant_def_costed(name: &str, cost: &str, effect: Effect) -> CardDef {
    let mut def = blank_card(name, cost, vec![CardType::Instant]);
    def.spell_effect = Some(effect);
    def
}

pub fn sorcery_def_costed(name: &str, cost: &str, effect: Effect) -> CardDef {
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
    let mut def = instant_def_costed(name, cost, effect);
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

/// Pinta a carta de uma cor dando-lhe um símbolo colorido no custo — é assim
/// que `CardDef::colors` deriva a cor (CR 202.2).
pub fn colored(mut def: CardDef, color: Color) -> CardDef {
    def.mana_cost = ManaCost {
        symbols: vec![ManaSymbol::Colored(color)],
    };
    def
}

// ---------------------------------------------------------------------------
// Montagem de partida
// ---------------------------------------------------------------------------

/// Configuração padrão dos testes: mão inicial vazia e sem mulligan, para que
/// `Game::new` não tome decisão nenhuma e a biblioteca fique com o deck inteiro
/// — todo objeto de teste sai de lá por `take_from_library`.
pub fn test_config() -> GameConfig {
    GameConfig {
        starting_life: 20,
        starting_hand_size: 0,
        allow_mulligan: false,
        max_turns: 20,
        max_decisions: 100_000,
    }
}

/// Igual à padrão, com folga de turnos e decisões para os testes que deixam a
/// partida rodar sozinha até o fim.
pub fn sim_config() -> GameConfig {
    GameConfig {
        max_turns: 60,
        max_decisions: 200_000,
        ..test_config()
    }
}

/// Catálogo real carregado uma única vez por binário de teste: interpretar o
/// Lua a cada `Setup::with_catalog` custaria mais que os testes inteiros.
fn catalog_cards() -> &'static Vec<CardDef> {
    static CATALOG: OnceLock<Vec<CardDef>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut db = match mtg_cards::build_database() {
            Ok(db) => db,
            Err(e) => panic!("catálogo de cartas não carregou: {e}"),
        };
        db.reindex();
        db.cards
    })
}

/// Catálogo real, lido de `cards/*.lua`. Falha alto se não carregar: sem cartas
/// não há o que testar.
pub fn catalog() -> Arc<CardDatabase> {
    match mtg_cards::build_database() {
        Ok(db) => Arc::new(db),
        Err(err) => panic!("catálogo real não carregou: {err}"),
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
    let mut db = CardDatabase { cards: defs };
    db.reindex();
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
    match Game::new(Arc::new(db), players, agents, sim_config(), seed) {
        Ok(g) => g,
        Err(err) => panic!("montagem da partida de teste falhou: {err}"),
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

pub struct Setup {
    cards: Vec<CardDef>,
    decks: Vec<Vec<CardDefId>>,
    pub config: GameConfig,
    pub seed: u64,
    pub names: [String; 2],
}

impl Setup {
    /// Catálogo real de `mtg-cards`, ao qual se pode acrescentar cartas
    /// sintéticas. É o caminho preferido: o que existe no jogo de verdade é
    /// testado no objeto de verdade.
    pub fn with_catalog() -> Setup {
        Setup::from_cards(catalog_cards().clone())
    }

    /// Catálogo vazio: só as cartas sintéticas do próprio teste.
    pub fn empty() -> Setup {
        Setup::from_cards(Vec::new())
    }

    fn from_cards(cards: Vec<CardDef>) -> Setup {
        Setup {
            cards,
            decks: vec![Vec::new(), Vec::new()],
            config: test_config(),
            seed: TEST_SEED,
            names: ["Alice".to_string(), "Bob".to_string()],
        }
    }

    /// Acrescenta uma carta sintética e devolve o id definitivo dela.
    pub fn add_card(&mut self, mut def: CardDef) -> CardDefId {
        let id = CardDefId(self.cards.len() as u32);
        def.id = id;
        self.cards.push(def);
        id
    }

    /// Id de uma carta do catálogo pelo nome. Pânico é proposital: nome errado
    /// tem de aparecer na hora, não virar teste que não testa nada.
    pub fn id(&self, name: &str) -> CardDefId {
        match self.cards.iter().find(|c| c.name.eq_ignore_ascii_case(name)) {
            Some(c) => c.id,
            None => panic!("carta '{name}' não existe no catálogo montado"),
        }
    }

    /// Acrescenta cartas ao deck do jogador, na ordem dada.
    pub fn deck(&mut self, player: PlayerId, cards: &[CardDefId]) -> &mut Setup {
        self.decks[player.index()].extend_from_slice(cards);
        self
    }

    /// Acrescenta `n` cópias da mesma carta ao deck.
    pub fn fill(&mut self, player: PlayerId, card: CardDefId, n: usize) -> &mut Setup {
        for _ in 0..n {
            self.decks[player.index()].push(card);
        }
        self
    }

    /// Acrescenta as mesmas cartas aos dois decks.
    pub fn deck_both(&mut self, cards: &[CardDefId]) -> &mut Setup {
        self.deck(PlayerId::P0, cards);
        self.deck(PlayerId::P1, cards);
        self
    }

    pub fn build(&self, agents: Vec<Box<dyn Agent>>) -> Game {
        let mut db = CardDatabase {
            cards: self.cards.clone(),
        };
        db.reindex();
        let players: Vec<PlayerConfig> = (0..2)
            .map(|i| {
                let deck = self.decks[i].clone();
                if deck.is_empty() {
                    panic!("deck do jogador {i} está vazio: monte-o com Setup::deck/fill");
                }
                PlayerConfig {
                    name: self.names[i].clone(),
                    deck,
                }
            })
            .collect();
        match Game::new(Arc::new(db), players, agents, self.config.clone(), self.seed) {
            Ok(g) => g,
            Err(e) => panic!("montagem da partida de teste falhou: {e}"),
        }
    }

    /// Atalho para o caso comum: dois agentes que só passam prioridade.
    pub fn build_passing(&self) -> Game {
        self.build(passing_agents())
    }
}

// ---------------------------------------------------------------------------
// Leitura de estado
// ---------------------------------------------------------------------------

pub fn card_id(game: &Game, name: &str) -> CardDefId {
    match game.db.id_by_name(name) {
        Some(id) => id,
        None => panic!("carta '{name}' não está no catálogo da partida"),
    }
}

pub fn zone(game: &Game, z: ZoneId) -> Vec<ObjectId> {
    turn::zone_objects(game, z)
}

pub fn hand(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    zone(game, ZoneId::hand(player))
}

pub fn library(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    zone(game, ZoneId::library(player))
}

pub fn graveyard(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    zone(game, ZoneId::graveyard(player))
}

pub fn battlefield(game: &Game) -> Vec<ObjectId> {
    zone(game, ZoneId::BATTLEFIELD)
}

pub fn hand_size(game: &Game, player: PlayerId) -> usize {
    hand(game, player).len()
}

/// Primeiro objeto da zona cuja definição tem este nome.
pub fn find_in_zone(game: &Game, zone: ZoneId, name: &str) -> Option<ObjectId> {
    turn::zone_objects(game, zone).into_iter().find(|id| {
        game.state
            .object(*id)
            .and_then(|o| game.db.get(o.card))
            .is_some_and(|c| c.name.eq_ignore_ascii_case(name))
    })
}

/// Primeiro objeto da biblioteca do jogador cuja definição é `def`.
pub fn take_from_library(game: &Game, player: PlayerId, def: CardDefId) -> ObjectId {
    let found = library(game, player)
        .into_iter()
        .find(|id| game.state.object(*id).map(|o| o.card == def).unwrap_or(false));
    match found {
        Some(id) => id,
        None => panic!(
            "nenhuma cópia de {def} na biblioteca de {player}: acrescente-a ao deck no Setup"
        ),
    }
}

fn take_named_from_library(game: &Game, name: &str, owner: PlayerId) -> ObjectId {
    match find_in_zone(game, ZoneId::library(owner), name) {
        Some(id) => id,
        None => panic!("nenhuma cópia de '{name}' sobrou na biblioteca de {owner}"),
    }
}

pub fn zone_of(game: &Game, id: ObjectId) -> ZoneId {
    match game.state.object(id) {
        Some(obj) => obj.zone,
        None => panic!("{id} não existe: não dá para ler a zona"),
    }
}

pub fn on_battlefield(game: &Game, id: ObjectId) -> bool {
    zone_of(game, id).kind == ZoneKind::Battlefield
}

pub fn in_graveyard(game: &Game, id: ObjectId) -> bool {
    zone_of(game, id).kind == ZoneKind::Graveyard
}

pub fn in_zone(game: &Game, id: ObjectId, kind: ZoneKind) -> bool {
    zone_of(game, id).kind == kind
}

pub fn life(game: &Game, player: PlayerId) -> i32 {
    game.state.player(player).life
}

pub fn damage(game: &Game, obj: ObjectId) -> i32 {
    match game.state.object(obj) {
        Some(o) => o.damage,
        None => panic!("{obj} não existe: sem dano a ler"),
    }
}

/// Nome antigo de `damage`, mantido porque os testes de combate leem assim.
pub fn damage_on(game: &Game, obj: ObjectId) -> i32 {
    damage(game, obj)
}

pub fn is_tapped(game: &Game, obj: ObjectId) -> bool {
    match game.state.object(obj) {
        Some(o) => o.tapped,
        None => panic!("{obj} não existe: sem estado de virado"),
    }
}

/// P/T atuais, já com todas as camadas aplicadas (CR 613).
pub fn pt(game: &Game, obj: ObjectId) -> (i32, i32) {
    match game.characteristics(obj) {
        Some(ch) => (ch.power, ch.toughness),
        None => panic!("{obj} não tem características: objeto inexistente"),
    }
}

pub fn has_keyword(game: &Game, obj: ObjectId, k: &Keyword) -> bool {
    match game.characteristics(obj) {
        Some(ch) => ch.has_keyword(k),
        None => panic!("{obj} não tem características: objeto inexistente"),
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

/// Contexto de avaliação com fonte, controlador e alvos já escolhidos.
pub fn eval_ctx(source: ObjectId, controller: PlayerId, targets: Vec<TargetChoice>) -> EvalCtx {
    EvalCtx {
        source: Some(source),
        controller,
        targets,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Manipuladores de estado
// ---------------------------------------------------------------------------

/// Tira uma cópia da carta da biblioteca do controlador e a põe no campo de
/// batalha. Os eventos de entrada **não** são limpos: gatilho de entrada
/// depende deles. O enjoo de invocação fica como o motor deixa (CR 302.6).
pub fn put_on_battlefield(game: &mut Game, card_name: &str, controller: PlayerId) -> ObjectId {
    let id = take_named_from_library(game, card_name, controller);
    turn::move_object(game, id, ZoneId::BATTLEFIELD);
    id
}

/// Igual a `put_on_battlefield`, mas já pronta para agir: sem enjoo de
/// invocação e desvirada.
pub fn put_on_battlefield_ready(
    game: &mut Game,
    card_name: &str,
    controller: PlayerId,
) -> ObjectId {
    let id = put_on_battlefield(game, card_name, controller);
    match game.state.object_mut(id) {
        Some(obj) => {
            obj.summoning_sick = false;
            obj.tapped = false;
        }
        None => panic!("{id} não existe logo após entrar no campo"),
    }
    id
}

/// Nome curto de `put_on_battlefield_ready` — o caso comum em teste de combate,
/// onde a criatura "já estava lá".
pub fn put_ready(game: &mut Game, card_name: &str, controller: PlayerId) -> ObjectId {
    put_on_battlefield_ready(game, card_name, controller)
}

pub fn put_in_hand(game: &mut Game, card_name: &str, player: PlayerId) -> ObjectId {
    let id = take_named_from_library(game, card_name, player);
    turn::move_object(game, id, ZoneId::hand(player));
    id
}

pub fn put_in_hand_by_def(game: &mut Game, def: CardDefId, player: PlayerId) -> ObjectId {
    let id = take_from_library(game, player, def);
    turn::move_object(game, id, ZoneId::hand(player));
    id
}

pub fn put_in_graveyard(game: &mut Game, card_name: &str, player: PlayerId) -> ObjectId {
    let id = take_named_from_library(game, card_name, player);
    turn::move_object(game, id, ZoneId::graveyard(player));
    id
}

/// Põe a carta na pilha como mágica lançada, sem passar pelo pagamento de
/// custo. Serve para testar resolução, contra-magia e alvo ilegal sem ter de
/// montar mana.
pub fn cast_onto_stack(
    game: &mut Game,
    card_name: &str,
    controller: PlayerId,
    targets: Vec<TargetChoice>,
) -> ObjectId {
    let id = take_named_from_library(game, card_name, controller);
    turn::move_object(game, id, ZoneId::STACK);
    stack::push_spell(game, id, controller, targets, 0, Vec::new());
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

pub fn give_counters(game: &mut Game, obj: ObjectId, kind: CounterKind, n: i32) {
    match game.state.object_mut(obj) {
        Some(o) => o.add_counter(kind, n),
        None => panic!("{obj} não existe: marcadores não aplicados"),
    }
}

pub fn set_damage(game: &mut Game, obj: ObjectId, amount: i32) {
    match game.state.object_mut(obj) {
        Some(o) => o.damage = amount,
        None => panic!("{obj} não existe: dano não aplicado"),
    }
}

pub fn tap(game: &mut Game, obj: ObjectId) {
    match game.state.object_mut(obj) {
        Some(o) => o.tapped = true,
        None => panic!("{obj} não existe: não dá para virar"),
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

/// Posiciona o estado num passo, sem executar os passos anteriores. Usado pelos
/// testes que chamam `turn::give_priority` ou `cast::priority_actions` direto.
pub fn goto_step(game: &mut Game, step: Step) {
    game.state.step = step;
    game.state.consecutive_passes = 0;
    game.state.priority_player = game.state.active_player;
}

pub fn set_active(game: &mut Game, player: PlayerId) {
    game.state.active_player = player;
    game.state.priority_player = player;
}

/// Enche o pool de mana do jogador — "mana abundante" sem ter de montar um
/// campo inteiro de terrenos.
pub fn fill_mana_pool(game: &mut Game, player: PlayerId, amount: u16) {
    let pool = &mut game.state.player_mut(player).mana_pool;
    pool.colored = [amount; 5];
    pool.colorless = amount;
}

pub fn clear_events(game: &mut Game) {
    game.state.event_queue.clear();
    game.match_events.clear();
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
