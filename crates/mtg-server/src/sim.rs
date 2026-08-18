//! Executa uma partida inteira numa thread bloqueante e transmite o
//! progresso pelo canal para a tarefa do WebSocket.
//!
//! Ponto crítico de design (ver prompt da tarefa): o motor (`Game::run`) é
//! síncrono e roda a partida inteira numa chamada só. Para a UI animar aos
//! poucos sem travar o runtime async, a simulação roda em
//! `spawn_blocking` e transmite eventos por um `mpsc` assim que ocorrem; a
//! tarefa do WebSocket (em `routes.rs`) é quem espaça a entrega no tempo —
//! ver `MatchEvent::suggested_duration_ms`. Isso mantém a simulação rápida e
//! a reprodução no ritmo certo, e permite pausar/retomar a reprodução sem
//! jamais pausar o motor.
//!
//! # Mesa de dois a quatro
//!
//! `TableRequest` é a forma geral: uma lista de `Seat` e um formato. `run_table_
//! blocking` é o único caminho de execução. `MatchRequest` e `run_match_blocking`
//! continuam existindo com a assinatura antiga porque `routes.rs` os chama, e
//! hoje são só um atalho de dois assentos em Constructed — `MatchRequest::into_
//! table` faz a conversão e não há um segundo motor de partida escondido aqui.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use mtg_cards::Format;
use mtg_core::card::CardDatabase;
use mtg_core::engine::{Agent, Game, GameConfig, GameFormat, PlayerConfig};
use mtg_core::ids::CardDefId;
use mtg_core::state::GameOutcome;
use mtg_core::view::Observer;
use mtg_core::{Action, Request};
use tokio::sync::mpsc::Sender;
use tracing::warn;

use crate::bot::SeededBot;
use crate::protocol::ServerFrame;

/// CR 100.4a — duelo é o mínimo; quatro é o teto que a mesa de Commander
/// deste projeto suporta (a UI só sabe desenhar até quatro assentos).
pub const MIN_SEATS: usize = 2;
pub const MAX_SEATS: usize = 4;

/// Teto de turnos por número de assentos.
///
/// Ganhar uma mesa de quatro exige eliminar **três** jogadores, não um: com o
/// teto de duelo a partida bate no relógio com gente viva e o resultado vira
/// empate por tempo, não por jogo. Medido em `mtg-ai/tests/multiplayer.rs`:
/// com 80 turnos, 37% das mesas de quatro empatavam por teto.
const TURN_CAP_DUEL: u32 = 60;
const TURN_CAP_TABLE: u32 = 160;

/// Um assento da mesa: quem senta, com que biblioteca e com que comandante.
#[derive(Debug, Clone)]
pub struct Seat {
    pub name: String,
    /// Cartas da biblioteca. Num deck de Commander são 99: o comandante
    /// começa na zona de comando (CR 903.6) e não entra aqui.
    pub deck: Vec<CardDefId>,
    /// CR 903.3 — obrigatório em Commander, ignorado nos outros formatos.
    pub commander: Option<CardDefId>,
    /// Nome do bot (`random`, `heuristic`, `greedy`). Desconhecido cai no
    /// padrão do servidor: o cliente é entrada hostil, não fonte de verdade.
    pub bot: String,
}

impl Seat {
    pub fn new(name: impl Into<String>, deck: Vec<CardDefId>) -> Seat {
        Seat {
            name: name.into(),
            deck,
            commander: None,
            bot: mtg_ai::DEFAULT_BOT.to_string(),
        }
    }

    /// `allow(dead_code)`: hoje só os testes chamam. São a API que `routes.rs`
    /// usa quando o frame `start` passar a trazer assentos e formato — e
    /// `routes.rs` está em outra frente agora. Ver o relatório desta tarefa.
    #[allow(dead_code)]
    pub fn with_commander(mut self, commander: CardDefId) -> Seat {
        self.commander = Some(commander);
        self
    }

    #[allow(dead_code)]
    pub fn with_bot(mut self, bot: impl Into<String>) -> Seat {
        self.bot = bot.into();
        self
    }
}

/// O que o cliente pediu, já resolvido para cartas concretas (a resolução de
/// nome de deck → cartas mora em `routes.rs`, que tem acesso ao catálogo).
#[derive(Debug, Clone)]
pub struct TableRequest {
    pub seats: Vec<Seat>,
    /// Formato pedido. `Format::from_slug` aceita o texto do frame `start`.
    pub format: Format,
    pub seed: u64,
    /// De quem é a visão transmitida. Ver `protocol::Perspective`.
    pub observer: Observer,
}

/// Por que uma mesa não pode ser montada. Vira `ServerFrame::Error`, então o
/// texto é o que o usuário lê.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("mesa pede de {MIN_SEATS} a {MAX_SEATS} jogadores, e veio com {found}")]
    SeatCount { found: usize },

    #[error("{seat} está em Commander sem comandante declarado (CR 903.3)")]
    MissingCommander { seat: String },

    #[error("{seat} não tem cartas no deck")]
    EmptyDeck { seat: String },
}

impl TableRequest {
    /// Checagem estrutural, feita **antes** de montar o `Game`. Devolver erro
    /// aqui vira uma mensagem no cliente; deixar passar viraria pânico ou
    /// partida sem sentido lá dentro.
    pub fn validate(&self) -> Result<(), TableError> {
        let found = self.seats.len();
        if !(MIN_SEATS..=MAX_SEATS).contains(&found) {
            return Err(TableError::SeatCount { found });
        }
        for seat in &self.seats {
            if seat.deck.is_empty() {
                return Err(TableError::EmptyDeck { seat: seat.name.clone() });
            }
            // CR 903.3 — sem comandante não existe deck de Commander.
            if self.format.requires_commander() && seat.commander.is_none() {
                return Err(TableError::MissingCommander { seat: seat.name.clone() });
            }
        }
        Ok(())
    }

    /// Regras de motor do formato pedido. Standard, Modern e Pauper diferem só
    /// na lista de cartas legal, que é assunto de `mtg-format`, não do motor —
    /// por isso os três caem em `Constructed`.
    pub fn game_format(&self) -> GameFormat {
        if self.format.requires_commander() {
            GameFormat::Commander
        } else {
            GameFormat::Constructed
        }
    }

    /// Vida inicial e teto de turnos já ajustados ao formato e ao tamanho da
    /// mesa. CR 903.7 — Commander começa com 40.
    pub fn config(&self) -> GameConfig {
        let cap = if self.seats.len() > MIN_SEATS { TURN_CAP_TABLE } else { TURN_CAP_DUEL };
        GameConfig { max_turns: cap, ..GameConfig::for_format(self.game_format()) }
    }

    pub fn player_names(&self) -> Vec<String> {
        self.seats.iter().map(|s| s.name.clone()).collect()
    }
}

/// O pedido de duelo antigo, mantido porque `routes.rs` o constrói campo a
/// campo. É um `TableRequest` de dois assentos em Constructed.
pub struct MatchRequest {
    pub name_a: String,
    pub name_b: String,
    pub deck_a: Vec<CardDefId>,
    pub deck_b: Vec<CardDefId>,
    pub seed: u64,
    /// De quem e a visao transmitida. Ver `protocol::Perspective`.
    pub observer: Observer,
}

impl MatchRequest {
    pub fn into_table(self) -> TableRequest {
        TableRequest {
            seats: vec![
                Seat::new(self.name_a, self.deck_a),
                Seat::new(self.name_b, self.deck_b),
            ],
            format: Format::Casual,
            seed: self.seed,
            observer: self.observer,
        }
    }
}

/// Decorador de `Agent`: nunca muta `Game` porque a assinatura de
/// `Agent::decide` só empresta `&Game` (CR de motor: decisão nunca muda
/// estado por si só). Por isso ele não pode chamar `Game::drain_events`
/// (exige `&mut`) — em vez disso lê o campo público `match_events` e usa um
/// cursor compartilhado entre **todos** os agentes da mesa, para nunca
/// reenviar um evento que outro agente já drenou.
struct StreamingAgent {
    inner: Box<dyn Agent>,
    tx: Sender<ServerFrame>,
    cursor: Arc<AtomicUsize>,
    observer: Observer,
}

impl StreamingAgent {
    fn flush(&self, game: &Game) {
        let already_sent = self.cursor.load(Ordering::Relaxed);
        let total = game.match_events.len();
        if total <= already_sent {
            return;
        }
        let batch = game.match_events[already_sent..total].to_vec();
        self.cursor.store(total, Ordering::Relaxed);
        let view = game.view(self.observer);
        // Melhor esforço: se o cliente WS já desconectou, o receptor foi
        // dropado e o envio falha — a simulação segue até o fim mesmo assim
        // (ela tem teto de decisões em `GameConfig::max_decisions`).
        if self.tx.blocking_send(ServerFrame::Events { events: batch, view }).is_err() {
            warn!("descartando eventos: cliente WS desconectado");
        }
    }
}

impl Agent for StreamingAgent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        self.flush(game);
        self.inner.decide(game, request, legal)
    }

    fn on_game_end(&mut self, game: &Game, outcome: GameOutcome) {
        self.flush(game);
        self.inner.on_game_end(game, outcome);
    }
}

/// Semente do bot do assento `index`.
///
/// Precisa ser função pura da semente da partida (senão o replay quebra) e
/// diferente por assento — quatro bots com a mesma semente sorteiam o mesmo
/// índice nas mesmas posições, e a mesa vira espelho em vez de partida.
fn seat_seed(seed: u64, index: usize) -> u64 {
    seed ^ ((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// Roda a mesa do início ao fim, mandando `Init`, `Events*` e `Done` pelo
/// canal. Chamar de dentro de `tokio::task::spawn_blocking` — a função em si
/// bloqueia a thread até a partida acabar.
pub fn run_table_blocking(db: Arc<CardDatabase>, req: TableRequest, tx: Sender<ServerFrame>) {
    if let Err(err) = req.validate() {
        let _ = tx.blocking_send(ServerFrame::Error { message: err.to_string() });
        return;
    }

    let config = req.config();
    let names = req.player_names();
    let cursor = Arc::new(AtomicUsize::new(0));

    let mut players = Vec::with_capacity(req.seats.len());
    let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(req.seats.len());
    for (index, seat) in req.seats.iter().enumerate() {
        players.push(PlayerConfig {
            name: seat.name.clone(),
            deck: seat.deck.clone(),
            commander: seat.commander,
        });
        let bot = SeededBot::with_kind(
            seat.name.clone(),
            &seat.bot,
            seat_seed(req.seed, index),
        );
        agents.push(Box::new(StreamingAgent {
            observer: req.observer,
            inner: Box::new(bot),
            tx: tx.clone(),
            cursor: cursor.clone(),
        }));
    }

    let mut game = match Game::new(db, players, agents, config, req.seed) {
        Ok(g) => g,
        Err(err) => {
            let _ = tx.blocking_send(ServerFrame::Error {
                message: format!("falha ao montar a partida: {err}"),
            });
            return;
        }
    };

    let init_view = game.view(req.observer);
    if tx
        .blocking_send(ServerFrame::Init { view: init_view, players: names, seed: req.seed })
        .is_err()
    {
        return; // cliente já foi embora antes da partida começar
    }

    let start = Instant::now();
    let outcome = game.run();

    // Eventos emitidos depois da última decisão (ex.: a ação baseada em
    // estado que fecha o jogo) nunca passam por `StreamingAgent::decide` —
    // drena o que sobrou direto do estado, que agora está livre (`run`
    // devolveu, então não há mais empréstimo ativo do motor).
    let already_sent = cursor.load(Ordering::Relaxed);
    if game.match_events.len() > already_sent {
        let batch = game.match_events[already_sent..].to_vec();
        let view = game.view(req.observer);
        let _ = tx.blocking_send(ServerFrame::Events { events: batch, view });
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = tx.blocking_send(ServerFrame::Done { outcome, turns: game.state.turn, duration_ms });
}

/// Atalho de duelo, que é o que `routes.rs` chama hoje.
pub fn run_match_blocking(db: Arc<CardDatabase>, req: MatchRequest, tx: Sender<ServerFrame>) {
    run_table_blocking(db, req.into_table(), tx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ids::CardDefId;

    /// Um assento pronto: biblioteca não-vazia, e comandante só onde o
    /// formato exige. Bots alternados para o pedido não sair uniforme.
    fn seat(index: usize, format: Format) -> Seat {
        let bot = if index % 2 == 0 { "heuristic" } else { "random" };
        let s = Seat::new(format!("P{index}"), vec![CardDefId(0); 40]).with_bot(bot);
        if format.requires_commander() {
            s.with_commander(CardDefId(1))
        } else {
            s
        }
    }

    fn table(count: usize, format: Format) -> TableRequest {
        let seats: Vec<Seat> = (0..count).map(|i| seat(i, format)).collect();
        TableRequest { seats, format, seed: 7, observer: Observer::Spectator }
    }

    #[test]
    fn aceita_de_dois_a_quatro_assentos_e_recusa_o_resto() {
        for count in MIN_SEATS..=MAX_SEATS {
            let req = table(count, Format::Commander);
            let Ok(()) = req.validate() else {
                panic!("mesa de {count} devia ser aceita");
            };
        }
        for count in [0usize, 1, 5, 8] {
            let req = table(count, Format::Commander);
            assert_eq!(
                req.validate(),
                Err(TableError::SeatCount { found: count }),
                "mesa de {count} passou e não devia"
            );
        }
    }

    #[test]
    fn commander_sem_comandante_e_recusado_e_constructed_nao_exige() {
        // CR 903.3 — deck de Commander é definido por um comandante.
        let mut req = table(4, Format::Commander);
        req.seats[2].commander = None;
        assert_eq!(
            req.validate(),
            Err(TableError::MissingCommander { seat: "P2".to_string() })
        );

        let duel = table(2, Format::Modern);
        assert!(duel.seats.iter().all(|s| s.commander.is_none()));
        let Ok(()) = duel.validate() else {
            panic!("Modern não exige comandante");
        };
    }

    #[test]
    fn deck_vazio_e_recusado_antes_de_montar_a_partida() {
        let mut req = table(3, Format::Commander);
        req.seats[1].deck.clear();
        assert_eq!(
            req.validate(),
            Err(TableError::EmptyDeck { seat: "P1".to_string() })
        );
    }

    #[test]
    fn formato_escolhe_regra_de_motor_vida_e_teto_de_turnos() {
        // CR 903.7 — Commander começa com 40; os construídos, com 20.
        let cmd = table(4, Format::Commander);
        assert_eq!(cmd.game_format(), GameFormat::Commander);
        assert_eq!(cmd.config().starting_life, 40);
        assert_eq!(cmd.config().max_turns, TURN_CAP_TABLE);

        for format in [Format::Standard, Format::Modern, Format::Pauper] {
            let duel = table(2, format);
            assert_eq!(
                duel.game_format(),
                GameFormat::Constructed,
                "{format} não devia ganhar regra de motor própria"
            );
            assert_eq!(duel.config().starting_life, 20);
            assert_eq!(duel.config().max_turns, TURN_CAP_DUEL);
        }
    }

    #[test]
    fn cada_assento_ganha_semente_propria_e_reproduzivel() {
        // Determinismo por semente é requisito: mesma semente, mesma mesa.
        let seeds: Vec<u64> = (0..MAX_SEATS).map(|i| seat_seed(4242, i)).collect();
        let again: Vec<u64> = (0..MAX_SEATS).map(|i| seat_seed(4242, i)).collect();
        assert_eq!(seeds, again, "semente de assento não é função pura");
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                assert_ne!(seeds[i], seeds[j], "assentos {i} e {j} têm a mesma semente");
            }
        }
        assert_ne!(seat_seed(4242, 0), seat_seed(4243, 0));
    }

    #[test]
    fn assento_leva_o_bot_pedido_e_o_comandante_declarado() {
        let s = Seat::new("Zé", vec![CardDefId(3); 99])
            .with_bot("greedy")
            .with_commander(CardDefId(9));
        assert_eq!(s.bot, "greedy");
        assert_eq!(s.commander, Some(CardDefId(9)));
        // Sem builder, o assento cai no bot padrão do servidor.
        assert_eq!(Seat::new("Ana", vec![CardDefId(3); 60]).bot, mtg_ai::DEFAULT_BOT);
    }

    #[test]
    fn duelo_antigo_vira_mesa_de_dois() {
        let req = MatchRequest {
            name_a: "A".to_string(),
            name_b: "B".to_string(),
            deck_a: vec![CardDefId(0); 60],
            deck_b: vec![CardDefId(0); 60],
            seed: 1,
            observer: Observer::Player(mtg_core::ids::PlayerId(0)),
        };
        let t = req.into_table();
        assert_eq!(t.seats.len(), 2);
        assert_eq!(t.player_names(), vec!["A".to_string(), "B".to_string()]);
        assert_eq!(t.game_format(), GameFormat::Constructed);
        let Ok(()) = t.validate() else {
            panic!("duelo padrão devia ser válido");
        };
    }
}
