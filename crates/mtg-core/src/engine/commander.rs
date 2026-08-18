//! Regras próprias de Commander (CR 903).
//!
//! Quatro regras vivem aqui e nada mais:
//!   - CR 903.4  — identidade de cor de uma carta (`color_identity`)
//!   - CR 903.6  — o comandante começa na zona de comando (`setup`)
//!   - CR 903.8  — taxa de {2} por lançamento anterior (`cast_tax`)
//!   - CR 903.9  — substituição de mudança de zona (`redirect_zone_change`)
//!   - CR 903.10 — 21 de dano do mesmo comandante (`lethal_commander_damage`)
//!
//! Fronteira deliberada: este módulo **não** aplica derrota. `sba` é o dono
//! único das ações baseadas em estado (CR 704.3 exige que todas sejam vistas
//! como um evento só), então aqui só existe a função de leitura
//! `lethal_commander_damage`, que `sba` consulta junto com vida e veneno.
//!
//! Fora de escopo nesta rodada, e documentado para não parecer esquecimento:
//! Partner / Background / Commander de duas cabeças (vários comandantes por
//! jogador), e a identidade de cor derivada de tipos básicos de terreno.
use super::{Game, GameFormat, PlayerConfig};
use crate::action::{Action, Request};
use crate::card::CardDef;
use crate::ids::{CardDefId, ObjectId, PlayerId};
use crate::mana::{Color, ColorSet};
use crate::state::{GameState, ObjectState};
use crate::zone::{ZoneId, ZoneKind};

/// CR 903.8 — cada lançamento anterior da zona de comando encarece {2}.
pub const TAX_PER_PREVIOUS_CAST: u32 = 2;
/// CR 903.10 — dano de combate acumulado de um mesmo comandante que derrota.
pub const LETHAL_COMMANDER_DAMAGE: i32 = 21;

// ---------------------------------------------------------------------------
// CR 903.6 — montagem
// ---------------------------------------------------------------------------

/// Tira o comandante do deck e o põe na zona de comando (CR 903.6).
///
/// Chamado por `Game::new` antes do embaralho: o comandante não pode estar na
/// biblioteca quando ela é embaralhada, ou a mesma semente daria mãos
/// diferentes conforme o comandante caísse ou não na mão inicial.
pub fn setup(game: &mut Game, players: &[PlayerConfig]) {
    if !game.config.format.is_commander() {
        return;
    }
    for (index, cfg) in players.iter().enumerate() {
        let Some(def) = cfg.commander else { continue };
        let player = PlayerId(index as u8);
        let Some(object) = claim_commander(game, player, def) else {
            game.state.push_log(
                format!("comandante {def} não pôde ser colocado na zona de comando"),
                Some(player),
            );
            continue;
        };
        if let Some(state) = game.state.object_mut(object) {
            // CR 903.3 — a designação acompanha a carta por todas as zonas.
            state.is_commander = true;
        }
        if let Some(p) = game.state.players.get_mut(player.index()) {
            p.commander = Some(object);
        }
        let name = game.card_name(object);
        game.state
            .push_log(format!("{name} começa na zona de comando"), Some(player));
    }
}

/// Acha a primeira cópia do comandante na biblioteca e a move para a zona de
/// comando. Se o deck não a contiver, cria o objeto direto na zona.
fn claim_commander(game: &mut Game, player: PlayerId, def: CardDefId) -> Option<ObjectId> {
    let from_library = game
        .state
        .library(player)
        .objects
        .iter()
        .copied()
        .find(|id| game.state.object(*id).map(|o| o.card) == Some(def));

    let object = match from_library {
        Some(id) => {
            if let Some(zone) = game.state.zones.get_mut(&(ZoneKind::Library, player.0)) {
                zone.remove(id);
            }
            id
        }
        None => new_object_for(game, player, def)?,
    };

    if let Some(state) = game.state.object_mut(object) {
        state.zone = ZoneId::command(player);
    }
    let zone = game.state.zones.get_mut(&(ZoneKind::Command, player.0))?;
    zone.push_bottom(object);
    Some(object)
}

/// Cria o objeto do comandante quando ele não veio na lista do deck.
///
/// `GameState::object` indexa `objects` por `ObjectId.0`, então o id novo tem
/// de coincidir com o comprimento do vetor. Se não coincidir, o estado já está
/// inconsistente e é melhor desistir do que corrompê-lo mais.
fn new_object_for(game: &mut Game, player: PlayerId, def: CardDefId) -> Option<ObjectId> {
    game.db.get(def)?;
    let id = game.state.id_gen.next_object();
    if id.0 as usize != game.state.objects.len() {
        return None;
    }
    let ts = game.state.next_timestamp();
    game.state
        .objects
        .push(ObjectState::new(id, def, player, ZoneId::command(player), ts));
    Some(id)
}

// ---------------------------------------------------------------------------
// Consultas
// ---------------------------------------------------------------------------

/// CR 903.3 — este objeto é um comandante?
pub fn is_commander(state: &GameState, object: ObjectId) -> bool {
    state.object(object).map(|o| o.is_commander).unwrap_or(false)
}

/// Comandante de um jogador, se ele tiver um.
pub fn commander_of(state: &GameState, player: PlayerId) -> Option<ObjectId> {
    state.players.get(player.index()).and_then(|p| p.commander)
}

// ---------------------------------------------------------------------------
// CR 903.8 — taxa de comandante
// ---------------------------------------------------------------------------

/// {2} genérico por vez que este comandante já foi lançado da zona de comando
/// nesta partida (CR 903.8). Zero para qualquer objeto que não seja comandante.
pub fn cast_tax(state: &GameState, object: ObjectId) -> u32 {
    if !is_commander(state, object) {
        return 0;
    }
    let Some(owner) = state.object(object).map(|o| o.owner) else {
        return 0;
    };
    let Some(player) = state.players.get(owner.index()) else {
        return 0;
    };
    TAX_PER_PREVIOUS_CAST * player.commander_casts
}

/// Registra um lançamento da zona de comando — é o que faz a taxa subir.
pub fn note_cast_from_command_zone(state: &mut GameState, object: ObjectId) {
    let Some(owner) = state.object(object).map(|o| o.owner) else {
        return;
    };
    if let Some(player) = state.players.get_mut(owner.index()) {
        player.commander_casts += 1;
    }
}

// ---------------------------------------------------------------------------
// CR 903.9 — substituição de mudança de zona
// ---------------------------------------------------------------------------

/// Zonas de onde o dono pode resgatar o comandante (CR 903.9).
fn is_rescuable_destination(kind: ZoneKind) -> bool {
    matches!(
        kind,
        ZoneKind::Graveyard | ZoneKind::Exile | ZoneKind::Hand | ZoneKind::Library
    )
}

/// CR 903.9 — comandante que iria para cemitério, exílio, mão ou biblioteca
/// pode ir para a zona de comando em vez disso; quem escolhe é o dono.
///
/// Devolve a zona de destino final. Chamado por `turn::move_object` antes de
/// qualquer mutação, porque é efeito de substituição (CR 614): o evento
/// original nunca chega a acontecer.
pub fn redirect_zone_change(game: &mut Game, object: ObjectId, to: ZoneId) -> ZoneId {
    if !is_rescuable_destination(to.kind) {
        return to;
    }
    let Some(state) = game.state.object(object) else { return to };
    if !state.is_commander || state.zone.kind == ZoneKind::Command {
        return to;
    }
    let owner = state.owner;
    let name = game.card_name(object);
    let answer = game.ask(Request::ConfirmOptional {
        player: owner,
        prompt: format!("mandar {name} para a zona de comando? (CR 903.9)"),
    });
    if !matches!(answer, Action::Confirm { yes: true }) {
        return to;
    }
    game.state.push_log(
        format!("{name} vai para a zona de comando em vez de {:?}", to.kind),
        Some(owner),
    );
    ZoneId::command(owner)
}

// ---------------------------------------------------------------------------
// CR 903.10 — dano de comandante
// ---------------------------------------------------------------------------

/// Ponto único de crédito de dano de comandante: `combat` chama logo depois de
/// aplicar dano de combate a um jogador (CR 903.10a).
///
/// Recebe `&mut GameState` e não `&mut Game` de propósito — a contagem não lê
/// catálogo nem faz pergunta a agente, e a assinatura menor deixa o teste
/// montar a matriz sem precisar de uma partida inteira. (Havia um segundo
/// ponto de entrada `credit_combat_damage(&mut Game, …)` escrito em paralelo;
/// era o mesmo corpo com um `Game` a mais e foi removido.)
///
/// Dano que não é de combate não conta.
pub fn note_combat_damage(
    state: &mut GameState,
    source: ObjectId,
    player: PlayerId,
    amount: i32,
) {
    if amount <= 0 || !is_commander(state, source) {
        return;
    }
    if let Some(p) = state.players.get_mut(player.index()) {
        *p.commander_damage.entry(source).or_insert(0) += amount;
    }
}

/// CR 903.10 — jogadores que receberam 21 ou mais de dano de combate de um
/// **mesmo** comandante. Dano de comandantes diferentes não soma.
///
/// Leitura pura: quem aplica a derrota é `sba`, que trata isto como mais uma
/// ação baseada em estado ao lado de vida zero e dez venenos (CR 704.3).
pub fn lethal_commander_damage(state: &GameState) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .filter(|p| !p.has_lost)
        .filter(|p| {
            p.commander_damage
                .values()
                .any(|d| *d >= LETHAL_COMMANDER_DAMAGE)
        })
        .map(|p| p.id)
        .collect()
}

// ---------------------------------------------------------------------------
// CR 903.4 — identidade de cor
// ---------------------------------------------------------------------------

/// CR 903.4 — identidade de cor de uma carta: as cores do custo de mana, do
/// indicador de cor, e de **todo símbolo de mana colorido no texto de regras**.
///
/// A validação do deck contra a identidade do comandante (CR 903.5d) é de outra
/// camada; aqui só se produz o conjunto.
pub fn color_identity(card: &CardDef) -> ColorSet {
    // CR 202.2 — `CardDef::colors` já resolve custo e indicador de cor; a
    // identidade parte daí e só acrescenta o que está no texto.
    let mut identity = card.colors();
    identity = identity.union(colors_in_text(&card.oracle_text));
    for ability in &card.abilities {
        identity = identity.union(colors_in_text(&ability.text()));
    }
    identity
}

/// Cores de todos os símbolos `{...}` de um texto de regras.
fn colors_in_text(text: &str) -> ColorSet {
    let mut found = ColorSet::COLORLESS;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '{' {
            continue;
        }
        let mut body = String::new();
        for c in chars.by_ref() {
            if c == '}' {
                break;
            }
            body.push(c);
        }
        found = found.union(symbol_colors(&body));
    }
    found
}

/// Cores de um único símbolo já sem chaves: `W`, `W/U`, `2/W`, `W/P`, `T`, `3`.
///
/// Só letra isolada de cor conta — `P` (fírexiano), `C`, `S`, `X`, `T` e os
/// números caem fora sozinhos, sem lista de exceções para manter.
fn symbol_colors(body: &str) -> ColorSet {
    let mut found = ColorSet::COLORLESS;
    for part in body.split('/') {
        let mut chars = part.trim().chars();
        let (Some(letter), None) = (chars.next(), chars.next()) else {
            continue;
        };
        if let Some(color) = Color::from_letter(letter) {
            found.insert(color);
        }
    }
    found
}

/// Identidade de cor de um comandante já dentro de uma partida.
pub fn commander_color_identity(game: &Game, player: PlayerId) -> Option<ColorSet> {
    let object = commander_of(&game.state, player)?;
    let card = game.state.object(object)?.card;
    game.db.get(card).map(color_identity)
}

/// Formato desta partida, para quem só tem o `Game` à mão.
pub fn format_of(game: &Game) -> GameFormat {
    game.config.format
}
