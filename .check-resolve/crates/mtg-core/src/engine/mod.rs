//! Motor de regras. `Game` é a única coisa que muta estado.
//!
//! Modelo de execução: síncrono com callback. Quando o motor precisa de uma
//! decisão, ele monta uma `Request`, enumera as `Action` legais e chama o
//! `Agent` do jogador na hora. Não há estado suspenso nem continuação — o que
//! elimina a classe de bug mais cara em motor de card game.
//!
//! Divisão de responsabilidade (cada submódulo é dono do seu pedaço):
//!   - `query`    — leitura pura: filtros, seletores, valores, alvos legais
//!   - `layers`   — características atuais de um objeto (CR 613)
//!   - `turn`     — estrutura de turno, prioridade, mudança de zona (CR 117, 500)
//!   - `stack`    — colocar na pilha e resolver (CR 601, 608)
//!   - `cast`     — legalidade de lançamento, custos, mana (CR 601, 202)
//!   - `combat`   — ataque, bloqueio, dano (CR 506–511)
//!   - `sba`      — ações baseadas em estado (CR 704)
//!   - `triggers` — casamento de evento com gatilho (CR 603)
//!   - `resolve`  — interpretador do IR de efeitos
//!   - `viewgen`  — projeção redigida para a UI
use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::action::{Action, ActionError, Request};
use crate::card::CardDatabase;
use crate::ids::{CardDefId, ObjectId, PlayerId};
use crate::ir::Keyword;
use crate::mana::ColorSet;
use crate::state::{GameOutcome, GameState};
use crate::types::TypeLine;
use crate::view::{GameView, MatchEvent, Observer};

pub mod cast;
pub mod combat;
pub mod layers;
pub mod query;
pub mod resolve;
pub mod sba;
pub mod stack;
pub mod triggers;
pub mod turn;
pub mod viewgen;

// ---------------------------------------------------------------------------
// Agente
// ---------------------------------------------------------------------------

/// Quem toma decisões por um jogador. Bot, humano via canal, ou script de teste.
///
/// Recebe `&Game` para poder simular, mas deve consultar `game.view(observer)`
/// para respeitar informação oculta — ver `Observer`.
pub trait Agent: Send {
    fn name(&self) -> &str;
    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action;
    /// Chamado uma vez no fim da partida. Útil para bots que aprendem.
    fn on_game_end(&mut self, _game: &Game, _outcome: GameOutcome) {}
}

/// Agente que sempre escolhe a primeira ação legal. Usado como placeholder
/// durante o swap temporário em `Game::ask`, e como baseline de comparação.
#[derive(Debug, Default)]
pub struct FirstLegalAgent;

impl Agent for FirstLegalAgent {
    fn name(&self) -> &str {
        "first-legal"
    }
    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        legal.first().cloned().unwrap_or(Action::PassPriority)
    }
}

// ---------------------------------------------------------------------------
// Características
// ---------------------------------------------------------------------------

/// Características de um objeto depois de aplicar todas as camadas.
/// Nunca ler `CardDef` direto para decidir regra — sempre passar por aqui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Characteristics {
    pub name: String,
    pub colors: ColorSet,
    pub type_line: TypeLine,
    pub mana_value: u32,
    pub power: i32,
    pub toughness: i32,
    pub loyalty: i32,
    pub keywords: Vec<Keyword>,
    pub controller: PlayerId,
    pub cant_attack: bool,
    pub cant_block: bool,
    pub cant_be_blocked: bool,
    pub prevent_all_damage: bool,
    pub does_not_untap: bool,
}

impl Characteristics {
    pub fn has_keyword(&self, k: &Keyword) -> bool {
        self.keywords.iter().any(|x| x == k)
    }
    pub fn is_creature(&self) -> bool {
        self.type_line.is_creature()
    }
    pub fn remaining_toughness(&self, damage: i32) -> i32 {
        self.toughness - damage
    }
}

// ---------------------------------------------------------------------------
// Configuração
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameConfig {
    pub starting_life: i32,
    pub starting_hand_size: usize,
    pub allow_mulligan: bool,
    /// Trava de segurança para simulação: empata após N turnos.
    pub max_turns: u32,
    /// Trava contra loop infinito de prioridade/gatilho.
    pub max_decisions: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig {
            starting_life: 20,
            starting_hand_size: 7,
            allow_mulligan: true,
            max_turns: 60,
            max_decisions: 100_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerConfig {
    pub name: String,
    /// Cartas do deck, na ordem em que entram na biblioteca antes do embaralho.
    pub deck: Vec<CardDefId>,
}

// ---------------------------------------------------------------------------
// Motor
// ---------------------------------------------------------------------------

pub struct Game {
    pub state: GameState,
    pub db: Arc<CardDatabase>,
    pub rng: ChaCha8Rng,
    pub config: GameConfig,
    pub agents: Vec<Box<dyn Agent>>,
    /// Eventos para a UI, drenados pelo servidor.
    pub match_events: Vec<MatchEvent>,
    pub decisions_made: u32,
    pub seed: u64,
}

impl Game {
    /// Monta a partida: cria zonas, popula bibliotecas, embaralha e distribui
    /// as mãos iniciais (com mulligan de Londres, se habilitado).
    pub fn new(
        db: Arc<CardDatabase>,
        players: Vec<PlayerConfig>,
        agents: Vec<Box<dyn Agent>>,
        config: GameConfig,
        seed: u64,
    ) -> Result<Game, ActionError> {
        let state = turn::initial_state(&db, &players, &config)?;
        let mut game = Game {
            state,
            db,
            rng: ChaCha8Rng::seed_from_u64(seed),
            config,
            agents,
            match_events: Vec::new(),
            decisions_made: 0,
            seed,
        };
        turn::shuffle_all_libraries(&mut game);
        turn::opening_hands(&mut game);
        Ok(game)
    }

    /// Joga a partida inteira. Retorna o resultado.
    pub fn run(&mut self) -> GameOutcome {
        turn::run_game(self);
        self.state.outcome
    }

    /// Pergunta ao agente do jogador. Ponto único de decisão do motor.
    ///
    /// A ação devolvida é validada contra a lista legal; agente que responde
    /// coisa ilegal cai na primeira ação legal em vez de corromper o estado.
    pub fn ask(&mut self, request: Request) -> Action {
        let Some(player) = request.player() else {
            return Action::PassPriority;
        };
        let legal = self.legal_actions_for(&request);
        if legal.is_empty() {
            return Action::PassPriority;
        }
        if legal.len() == 1 {
            self.state.pending = request;
            return legal.into_iter().next().expect("checado acima");
        }

        self.decisions_made += 1;
        if self.decisions_made > self.config.max_decisions {
            turn::force_draw(self, "limite de decisões atingido");
            return legal.into_iter().next().expect("checado acima");
        }

        self.state.pending = request.clone();
        let idx = player.index();
        // Tira o agente da lista para poder emprestar `&Game` imutável.
        let placeholder: Box<dyn Agent> = Box::new(FirstLegalAgent);
        let mut agent = std::mem::replace(&mut self.agents[idx], placeholder);
        let chosen = agent.decide(self, &request, &legal);
        self.agents[idx] = agent;

        if legal.contains(&chosen) {
            chosen
        } else {
            self.state.push_log(
                format!("ação ilegal do agente descartada: {}", chosen.label()),
                Some(player),
            );
            legal.into_iter().next().expect("checado acima")
        }
    }

    /// Ações legais para uma request específica.
    pub fn legal_actions_for(&self, request: &Request) -> Vec<Action> {
        match request {
            Request::GameOver => Vec::new(),
            Request::Priority { player } => cast::priority_actions(self, *player),
            Request::Mulligan { .. } => vec![Action::KeepHand, Action::Mulligan],
            Request::BottomCards { player, count } => {
                turn::bottom_card_options(self, *player, *count)
            }
            Request::DeclareAttackers { player, eligible } => {
                combat::attack_options(self, *player, eligible)
            }
            Request::DeclareBlockers { player, eligible, attackers } => {
                combat::block_options(self, *player, eligible, attackers)
            }
            Request::OrderBlockers { attacker, blockers, .. } => {
                combat::order_options(*attacker, blockers)
            }
            Request::AssignCombatDamage { attacker, blockers, total, .. } => {
                combat::damage_assignment_options(self, *attacker, blockers, *total)
            }
            Request::OrderTriggers { triggers, .. } => stack::trigger_order_options(triggers),
            Request::ConfirmOptional { .. } => {
                vec![Action::Confirm { yes: true }, Action::Confirm { yes: false }]
            }
            Request::ChooseModes { options, choose, .. } => {
                resolve::mode_options(options.len(), *choose)
            }
            Request::SelectObjects { candidates, min, max, .. } => {
                resolve::selection_options(candidates, *min, *max)
            }
            Request::ChooseColor { .. } => crate::mana::Color::ALL
                .into_iter()
                .map(|color| Action::ChooseColor { color })
                .collect(),
            Request::ArrangeCards { cards, .. } => resolve::arrange_options(cards),
        }
    }

    pub fn view(&self, observer: Observer) -> GameView {
        viewgen::build_view(self, observer)
    }

    pub fn drain_events(&mut self) -> Vec<MatchEvent> {
        std::mem::take(&mut self.match_events)
    }

    pub fn push_event(&mut self, e: MatchEvent) {
        self.match_events.push(e);
    }

    /// Características atuais de um objeto, com todas as camadas aplicadas.
    pub fn characteristics(&self, id: ObjectId) -> Option<Characteristics> {
        layers::characteristics(self, id)
    }

    pub fn card_name(&self, id: ObjectId) -> String {
        self.characteristics(id)
            .map(|c| c.name)
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn is_over(&self) -> bool {
        self.state.is_over()
    }
}

impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("turn", &self.state.turn)
            .field("step", &self.state.step)
            .field("outcome", &self.state.outcome)
            .finish()
    }
}
