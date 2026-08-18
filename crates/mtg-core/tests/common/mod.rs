//! Ferramental compartilhado dos testes de integração do motor.
//!
//! Três coisas vivem aqui e nada mais:
//!   1. Agentes determinísticos (`FixedAgent`, `ScriptedAgent`) — o motor só
//!      avança quando alguém decide, e teste com decisão aleatória não afirma
//!      nada.
//!   2. `Setup` — montagem de partida com catálogo e deck controlados. Parte do
//!      catálogo real (`mtg_cards::build_database`) e aceita `CardDef`
//!      sintéticos para o que o catálogo não tem (gatilho de limpeza, criatura
//!      com proteção, mágica de dois alvos).
//!   3. Manipuladores de estado (`put_on_battlefield`, `set_life`,
//!      `give_counters`, `goto_step`) — montar o estado exato de uma regra sem
//!      ter de jogar dez turnos até ele acontecer.
//!
//! Regra de casa: helper que não encontra o que foi pedido **entra em pânico**.
//! Em teste, silêncio é pior que falha — um `Option` ignorado vira teste verde
//! que não testou nada.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use mtg_core::action::{Action, Request, TargetChoice};
use mtg_core::card::{CardDatabase, CardDef};
use mtg_core::engine::query::EvalCtx;
use mtg_core::engine::{stack, turn};
use mtg_core::engine::{Agent, Game, GameConfig, PlayerConfig};
use mtg_core::event::Step;
use mtg_core::ids::{CardDefId, ObjectId, PlayerId};
use mtg_core::ir::{Effect, TargetSpec};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Rarity, Supertype, TypeLine};
use mtg_core::zone::{ZoneId, ZoneKind};

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

pub fn passing_agents() -> Vec<Box<dyn Agent>> {
    vec![
        FixedAgent::passing("A").boxed(),
        FixedAgent::passing("B").boxed(),
    ]
}

// ---------------------------------------------------------------------------
// Fábrica de `CardDef` sintético
// ---------------------------------------------------------------------------

/// Carta em branco: sem custo, sem habilidade, sem P/T. O `id` é sobrescrito
/// por `Setup::add_card`, que é quem sabe a posição real no catálogo.
pub fn card_def(name: &str, types: Vec<CardType>) -> CardDef {
    CardDef {
        id: CardDefId(0),
        name: name.to_string(),
        mana_cost: ManaCost::FREE,
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

pub fn creature_def(name: &str, power: i32, toughness: i32) -> CardDef {
    let mut c = card_def(name, vec![CardType::Creature]);
    c.power = Some(power);
    c.toughness = Some(toughness);
    c
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
            seed: 7,
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
// Manipuladores de estado
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

/// Tira uma cópia da carta da biblioteca do controlador e a põe no campo de
/// batalha. Os eventos de entrada **não** são limpos: gatilho de entrada
/// depende deles.
pub fn put_on_battlefield(game: &mut Game, card_name: &str, controller: PlayerId) -> ObjectId {
    let def = card_id(game, card_name);
    let id = take_from_library(game, controller, def);
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
    if let Some(obj) = game.state.object_mut(id) {
        obj.summoning_sick = false;
        obj.tapped = false;
    }
    id
}

pub fn put_in_hand(game: &mut Game, card_name: &str, player: PlayerId) -> ObjectId {
    let def = card_id(game, card_name);
    let id = take_from_library(game, player, def);
    turn::move_object(game, id, ZoneId::hand(player));
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
    let def = card_id(game, card_name);
    let id = take_from_library(game, controller, def);
    turn::move_object(game, id, ZoneId::STACK);
    stack::push_spell(game, id, controller, targets, 0, Vec::new());
    id
}

pub fn set_life(game: &mut Game, player: PlayerId, life: i32) {
    game.state.player_mut(player).life = life;
}

pub fn life(game: &Game, player: PlayerId) -> i32 {
    game.state.player(player).life
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

pub fn damage(game: &Game, obj: ObjectId) -> i32 {
    match game.state.object(obj) {
        Some(o) => o.damage,
        None => panic!("{obj} não existe: sem dano a ler"),
    }
}

pub fn tap(game: &mut Game, obj: ObjectId) {
    match game.state.object_mut(obj) {
        Some(o) => o.tapped = true,
        None => panic!("{obj} não existe: não dá para virar"),
    }
}

pub fn is_tapped(game: &Game, obj: ObjectId) -> bool {
    match game.state.object(obj) {
        Some(o) => o.tapped,
        None => panic!("{obj} não existe: sem estado de virado"),
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

pub fn clear_events(game: &mut Game) {
    game.state.event_queue.clear();
    game.match_events.clear();
}

pub fn on_battlefield(game: &Game, obj: ObjectId) -> bool {
    game.state
        .object(obj)
        .map(|o| o.zone.kind == ZoneKind::Battlefield)
        .unwrap_or(false)
}

pub fn in_graveyard(game: &Game, obj: ObjectId) -> bool {
    game.state
        .object(obj)
        .map(|o| o.zone.kind == ZoneKind::Graveyard)
        .unwrap_or(false)
}

pub fn in_zone(game: &Game, obj: ObjectId, kind: ZoneKind) -> bool {
    game.state
        .object(obj)
        .map(|o| o.zone.kind == kind)
        .unwrap_or(false)
}

/// P/T atuais, já com todas as camadas aplicadas (CR 613).
pub fn pt(game: &Game, obj: ObjectId) -> (i32, i32) {
    match game.characteristics(obj) {
        Some(ch) => (ch.power, ch.toughness),
        None => panic!("{obj} não tem características: objeto inexistente"),
    }
}

pub fn has_keyword(game: &Game, obj: ObjectId, k: &mtg_core::ir::Keyword) -> bool {
    match game.characteristics(obj) {
        Some(ch) => ch.has_keyword(k),
        None => panic!("{obj} não tem características: objeto inexistente"),
    }
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
