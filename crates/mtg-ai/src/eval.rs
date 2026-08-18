//! Modelo de posição e avaliação estática.
//!
//! Por que um `Snapshot` em vez de avaliar `Game` direto: `Game` carrega
//! `Vec<Box<dyn Agent>>` e não é clonável, então busca precisa de uma
//! representação própria. O snapshot também isola a IA das camadas — ele é
//! construído uma vez por decisão chamando `layers::characteristics`, e depois
//! a simulação mexe só em números.
use mtg_core::engine::{cast, Game};
use mtg_core::event::Step;
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::Keyword;
use mtg_core::state::GameState;
use mtg_core::zone::ZoneId;

// ---------------------------------------------------------------------------
// Pesos
// ---------------------------------------------------------------------------
//
// Unidade: centipontos. Os pesos foram calibrados para que as trocas clássicas
// de Magic caiam do lado certo:
//   - criatura 2/2 = 201; carta na mão = 85; 3 vidas = 90. Trocar 3 de dano
//     por um corpo 2/2 é bom negócio (201 > 175), trocar uma carta não é.
//   - 1/1 = 128, abaixo do limiar de remoção de 2 manas (190): não se gasta
//     Doom Blade em Elfo.

/// Um ponto de vida. Vida é recurso, não objetivo: só o último ponto importa,
/// e disso cuidam os termos de letalidade.
pub const LIFE: i64 = 30;
/// Corpo em campo, independente de tamanho: qualquer criatura bloqueia.
pub const CREATURE_BASE: i64 = 55;
/// Poder ganha o jogo (relógio); vale mais que resistência.
pub const POWER: i64 = 45;
pub const TOUGHNESS: i64 = 28;
/// Evasão multiplica o poder porque converte poder em dano de verdade.
pub const EVASION: i64 = 12;
/// Carta na mão: opção futura. Menos que um corpo já em campo (tempo importa).
pub const CARD_IN_HAND: i64 = 85;
pub const NONLAND_PERMANENT: i64 = 65;
/// Fonte de mana até a sétima; depois o excedente quase não paga.
pub const MANA_SOURCE: i64 = 22;
pub const MANA_SURPLUS: i64 = 4;
pub const MANA_COLOR: i64 = 12;
/// Morrer no próximo ataque adversário domina qualquer vantagem material.
pub const LETHAL_THREAT: i64 = 900;
pub const LETHAL_CHANCE: i64 = 1100;
/// Biblioteca vazia = derrota na próxima compra (CR 704.5b).
pub const DECKING: i64 = 4000;
pub const TERMINAL: i64 = 1_000_000;

/// Limiar de relevância de alvo de remoção, em função do custo da remoção.
/// Gastar 2 manas num 1/1 é perda de carta; gastar num 2/2 já se paga.
pub fn removal_threshold(spell_mana_value: u32) -> i64 {
    100 + spell_mana_value as i64 * 45
}

// ---------------------------------------------------------------------------
// Lado
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Me,
    Opponent,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Me => Side::Opponent,
            Side::Opponent => Side::Me,
        }
    }
}

// ---------------------------------------------------------------------------
// Palavras-chave que a IA usa
// ---------------------------------------------------------------------------

/// Só as palavras-chave que mudam decisão de combate ou de alvo. Guardar o
/// `Vec<Keyword>` inteiro obrigaria a varrer vetor dentro do laço de simulação.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Traits {
    pub flying: bool,
    pub reach: bool,
    pub trample: bool,
    pub first_strike: bool,
    pub double_strike: bool,
    pub deathtouch: bool,
    pub lifelink: bool,
    pub vigilance: bool,
    pub haste: bool,
    pub menace: bool,
    pub defender: bool,
    pub indestructible: bool,
    pub hexproof: bool,
    pub shroud: bool,
    pub protection: bool,
}

impl Traits {
    pub fn from_keywords(keywords: &[Keyword]) -> Traits {
        let mut t = Traits::default();
        for k in keywords {
            match k {
                Keyword::Flying => t.flying = true,
                Keyword::Reach => t.reach = true,
                Keyword::Trample => t.trample = true,
                Keyword::FirstStrike => t.first_strike = true,
                Keyword::DoubleStrike => t.double_strike = true,
                Keyword::Deathtouch => t.deathtouch = true,
                Keyword::Lifelink => t.lifelink = true,
                Keyword::Vigilance => t.vigilance = true,
                Keyword::Haste => t.haste = true,
                Keyword::Menace => t.menace = true,
                Keyword::Defender => t.defender = true,
                Keyword::Indestructible => t.indestructible = true,
                Keyword::Hexproof => t.hexproof = true,
                Keyword::Shroud => t.shroud = true,
                Keyword::Protection(_) => t.protection = true,
                _ => {}
            }
        }
        t
    }

    /// Não pode ser alvo de mágica adversária (CR 702.11, 702.18).
    pub fn untargetable_by_opponent(self) -> bool {
        self.hexproof || self.shroud
    }

    /// Causa dano no passo de primeiro golpe (CR 510.4).
    pub fn strikes_first(self) -> bool {
        self.first_strike || self.double_strike
    }

    /// Causa dano no passo normal. Golpe duplo bate nos dois.
    pub fn strikes_normal(self) -> bool {
        !self.first_strike || self.double_strike
    }
}

// ---------------------------------------------------------------------------
// Criatura
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureInfo {
    pub id: ObjectId,
    pub controller: PlayerId,
    pub power: i32,
    pub toughness: i32,
    pub damage: i32,
    pub mana_value: u32,
    pub tapped: bool,
    pub summoning_sick: bool,
    pub attacking: bool,
    pub blocking: bool,
    pub cant_attack: bool,
    pub cant_block: bool,
    pub cant_be_blocked: bool,
    pub traits: Traits,
}

impl CreatureInfo {
    /// Criatura sem texto — usada em teste e na previsão de "o que entra em
    /// campo se eu lançar esta carta".
    pub fn vanilla(id: ObjectId, controller: PlayerId, power: i32, toughness: i32) -> CreatureInfo {
        CreatureInfo {
            id,
            controller,
            power,
            toughness,
            damage: 0,
            mana_value: 0,
            tapped: false,
            summoning_sick: false,
            attacking: false,
            blocking: false,
            cant_attack: false,
            cant_block: false,
            cant_be_blocked: false,
            traits: Traits::default(),
        }
    }

    pub fn effective_toughness(&self) -> i32 {
        self.toughness - self.damage
    }

    /// Poder que este corpo converte em dano recorrente. Criatura com defensor
    /// não ataca, então o poder dela só rende bloqueando.
    fn offensive_power(&self) -> i32 {
        if self.traits.defender || self.cant_attack {
            self.power.max(0) / 3
        } else {
            self.power.max(0)
        }
    }

    /// Pode declarar ataque agora (CR 508.1a).
    pub fn can_attack_now(&self) -> bool {
        !self.tapped
            && !self.cant_attack
            && !self.traits.defender
            && (!self.summoning_sick || self.traits.haste)
            && self.power > 0
    }

    /// Pode ser declarada bloqueadora agora (CR 509.1a).
    pub fn can_block_now(&self) -> bool {
        !self.tapped && !self.cant_block && !self.attacking
    }

    /// No próximo turno tudo desvira e o enjoo passa; só a trava persiste.
    pub fn can_attack_next_turn(&self) -> bool {
        !self.cant_attack && !self.traits.defender && self.power > 0
    }

    /// Este bloqueador consegue bloquear aquele atacante (CR 509.1b).
    /// Ameaçar exige dois bloqueadores e é tratado por quem chama.
    pub fn can_block_attacker(&self, attacker: &CreatureInfo) -> bool {
        if attacker.cant_be_blocked {
            return false;
        }
        if attacker.traits.flying && !(self.traits.flying || self.traits.reach) {
            return false;
        }
        true
    }

    /// O dano deste combatente é letal para `other` (CR 704.5g, 702.2b).
    pub fn deals_lethal_to(&self, other: &CreatureInfo) -> bool {
        if other.traits.indestructible || self.power <= 0 {
            return false;
        }
        self.traits.deathtouch || self.power >= other.effective_toughness()
    }

    /// Quanto vale esta criatura, em centipontos.
    pub fn value(&self) -> i64 {
        let eff_t = self.effective_toughness();
        // Resistência ≤ 0 morre por ação baseada em estado (CR 704.5f).
        if eff_t <= 0 {
            return 0;
        }
        let power = self.offensive_power() as i64;
        let mut v = CREATURE_BASE + power * POWER + eff_t as i64 * TOUGHNESS;
        if self.traits.flying || self.traits.menace || self.cant_be_blocked {
            v += power * EVASION;
        }
        if self.traits.trample {
            v += power * 6;
        }
        if self.traits.lifelink {
            v += power * 8;
        }
        if self.traits.double_strike {
            v += power * 20;
        } else if self.traits.first_strike {
            v += 25;
        }
        if self.traits.deathtouch {
            v += 40;
        }
        if self.traits.vigilance {
            v += 18;
        }
        if self.traits.indestructible {
            v += 60;
        }
        if self.traits.untargetable_by_opponent() {
            v += 30;
        }
        // Enjoo é desvantagem temporária: desconto pequeno, não estrutural.
        if self.summoning_sick && !self.traits.haste {
            v -= 12;
        }
        if self.tapped {
            v -= 10;
        }
        v
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub me: PlayerId,
    pub opponent: PlayerId,
    pub my_life: i32,
    pub opp_life: i32,
    pub my_poison: i32,
    pub opp_poison: i32,
    pub my_creatures: Vec<CreatureInfo>,
    pub opp_creatures: Vec<CreatureInfo>,
    pub my_hand: usize,
    pub opp_hand: usize,
    pub my_lands: usize,
    pub opp_lands: usize,
    pub my_nonland_permanents: usize,
    pub opp_nonland_permanents: usize,
    pub my_mana_sources: usize,
    pub opp_mana_sources: usize,
    pub my_mana_colors: u32,
    pub opp_mana_colors: u32,
    /// Mana desvirada agora — usada para decidir se vale segurar resposta.
    /// Fora da avaliação de propósito: senão a IA odiaria gastar mana.
    pub my_available_mana: u16,
    pub my_library: usize,
    pub opp_library: usize,
    pub is_my_turn: bool,
    pub step: Step,
    pub turn: u32,
    pub lands_played_this_turn: u8,
    pub max_lands_per_turn: u8,
}

impl Snapshot {
    pub fn empty(me: PlayerId, opponent: PlayerId) -> Snapshot {
        Snapshot {
            me,
            opponent,
            my_life: 20,
            opp_life: 20,
            my_poison: 0,
            opp_poison: 0,
            my_creatures: Vec::new(),
            opp_creatures: Vec::new(),
            my_hand: 0,
            opp_hand: 0,
            my_lands: 0,
            opp_lands: 0,
            my_nonland_permanents: 0,
            opp_nonland_permanents: 0,
            my_mana_sources: 0,
            opp_mana_sources: 0,
            my_mana_colors: 0,
            opp_mana_colors: 0,
            my_available_mana: 0,
            my_library: 40,
            opp_library: 40,
            is_my_turn: true,
            step: Step::PrecombatMain,
            turn: 1,
            lands_played_this_turn: 0,
            max_lands_per_turn: 1,
        }
    }

    pub fn creatures(&self, side: Side) -> &Vec<CreatureInfo> {
        match side {
            Side::Me => &self.my_creatures,
            Side::Opponent => &self.opp_creatures,
        }
    }

    pub fn creatures_mut(&mut self, side: Side) -> &mut Vec<CreatureInfo> {
        match side {
            Side::Me => &mut self.my_creatures,
            Side::Opponent => &mut self.opp_creatures,
        }
    }

    pub fn life(&self, side: Side) -> i32 {
        match side {
            Side::Me => self.my_life,
            Side::Opponent => self.opp_life,
        }
    }

    pub fn add_life(&mut self, side: Side, delta: i32) {
        match side {
            Side::Me => self.my_life += delta,
            Side::Opponent => self.opp_life += delta,
        }
    }

    pub fn hand_size(&self, side: Side) -> usize {
        match side {
            Side::Me => self.my_hand,
            Side::Opponent => self.opp_hand,
        }
    }

    /// Lado que controla este objeto, se ele for uma criatura em campo.
    pub fn side_of(&self, id: ObjectId) -> Option<Side> {
        if self.my_creatures.iter().any(|c| c.id == id) {
            return Some(Side::Me);
        }
        if self.opp_creatures.iter().any(|c| c.id == id) {
            return Some(Side::Opponent);
        }
        None
    }

    pub fn find(&self, id: ObjectId) -> Option<&CreatureInfo> {
        self.my_creatures
            .iter()
            .chain(self.opp_creatures.iter())
            .find(|c| c.id == id)
    }

    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut CreatureInfo> {
        self.my_creatures
            .iter_mut()
            .chain(self.opp_creatures.iter_mut())
            .find(|c| c.id == id)
    }

    pub fn remove_creature(&mut self, id: ObjectId) {
        self.my_creatures.retain(|c| c.id != id);
        self.opp_creatures.retain(|c| c.id != id);
    }

    /// Monta o retrato a partir do motor. Só aqui a IA fala com `Game`.
    pub fn from_game(game: &Game, me: PlayerId) -> Snapshot {
        let st = &game.state;
        let opponent = st
            .opponents(me)
            .first()
            .copied()
            .unwrap_or_else(|| st.next_player(me));
        let mut s = Snapshot::empty(me, opponent);

        s.my_life = life_of(st, me);
        s.opp_life = life_of(st, opponent);
        s.my_poison = poison_of(st, me);
        s.opp_poison = poison_of(st, opponent);
        s.my_hand = zone_len(st, ZoneId::hand(me));
        s.opp_hand = zone_len(st, ZoneId::hand(opponent));
        s.my_library = zone_len(st, ZoneId::library(me));
        s.opp_library = zone_len(st, ZoneId::library(opponent));
        s.is_my_turn = st.active_player == me;
        s.step = st.step;
        s.turn = st.turn;
        if let Some(p) = st.players.get(me.index()) {
            s.lands_played_this_turn = p.lands_played_this_turn;
            s.max_lands_per_turn = p.max_lands_per_turn;
        }

        for id in zone_objects(st, ZoneId::BATTLEFIELD) {
            let Some(obj) = st.object(*id) else { continue };
            // CR 613: características vêm das camadas, nunca do CardDef cru.
            let Some(ch) = game.characteristics(*id) else {
                continue;
            };
            let side = if ch.controller == me {
                Side::Me
            } else if ch.controller == opponent {
                Side::Opponent
            } else {
                continue;
            };

            if produces_mana(game, *id) {
                match side {
                    Side::Me => s.my_mana_sources += 1,
                    Side::Opponent => s.opp_mana_sources += 1,
                }
            }

            if ch.type_line.is_creature() {
                let info = CreatureInfo {
                    id: *id,
                    controller: ch.controller,
                    power: ch.power,
                    toughness: ch.toughness,
                    damage: obj.damage,
                    mana_value: ch.mana_value,
                    tapped: obj.tapped,
                    summoning_sick: obj.summoning_sick,
                    attacking: obj.combat.is_attacking(),
                    blocking: obj.combat.is_blocking(),
                    cant_attack: ch.cant_attack,
                    cant_block: ch.cant_block,
                    cant_be_blocked: ch.cant_be_blocked,
                    traits: Traits::from_keywords(&ch.keywords),
                };
                s.creatures_mut(side).push(info);
            } else if ch.type_line.is_land() {
                match side {
                    Side::Me => s.my_lands += 1,
                    Side::Opponent => s.opp_lands += 1,
                }
            } else {
                match side {
                    Side::Me => s.my_nonland_permanents += 1,
                    Side::Opponent => s.opp_nonland_permanents += 1,
                }
            }
        }

        let mine = cast::available_mana(game, me);
        let theirs = cast::available_mana(game, opponent);
        s.my_available_mana = mine.total;
        s.my_mana_colors = color_count(&mine);
        s.opp_mana_colors = color_count(&theirs);
        s
    }
}

fn color_count(m: &cast::ManaAvailability) -> u32 {
    if m.any_color > 0 {
        // Fonte "qualquer cor" cobre o que faltar: leque completo.
        return 5;
    }
    m.by_color.iter().filter(|n| **n > 0).count() as u32
}

/// Permanente com habilidade de mana. Ler `CardDef` aqui é heurística de IA,
/// não decisão de regra — o motor continua dono das regras.
fn produces_mana(game: &Game, id: ObjectId) -> bool {
    let Some(obj) = game.state.object(id) else {
        return false;
    };
    match game.db.get(obj.card) {
        Some(def) => def.mana_abilities().next().is_some(),
        None => false,
    }
}

pub fn life_of(state: &GameState, p: PlayerId) -> i32 {
    state.players.get(p.index()).map_or(0, |x| x.life)
}

pub fn poison_of(state: &GameState, p: PlayerId) -> i32 {
    state.players.get(p.index()).map_or(0, |x| x.poison)
}

/// Acesso a zona sem indexação que possa entrar em pânico.
pub fn zone_objects(state: &GameState, z: ZoneId) -> &[ObjectId] {
    let key = (z.kind, z.owner.map_or(u8::MAX, |p| p.0));
    state
        .zones
        .get(&key)
        .map_or(&[][..], |zone| zone.objects.as_slice())
}

pub fn zone_len(state: &GameState, z: ZoneId) -> usize {
    zone_objects(state, z).len()
}

// ---------------------------------------------------------------------------
// Ameaça de combate
// ---------------------------------------------------------------------------

/// Dano que passa se `attackers` atacarem e `blockers` bloquearem o melhor
/// possível. Modelo guloso: cada bloqueador neutraliza o maior atacante que
/// consegue bloquear. Responde "eu morro?", não "vale a troca?".
pub fn unblocked_damage(attackers: &[&CreatureInfo], blockers: &[&CreatureInfo]) -> i32 {
    let mut order: Vec<&CreatureInfo> = attackers.to_vec();
    // Ordenação total por (poder, id) mantém o resultado igual em toda execução.
    order.sort_by(|a, b| b.power.cmp(&a.power).then(a.id.cmp(&b.id)));
    let mut free: Vec<&CreatureInfo> = blockers.to_vec();
    free.sort_by_key(|b| b.id);

    let mut total = 0;
    for atk in order {
        // Ameaçar exige dois bloqueadores (CR 702.110b).
        let needed = if atk.traits.menace { 2 } else { 1 };
        let mut chosen: Vec<usize> = Vec::new();
        for (i, b) in free.iter().enumerate() {
            if b.can_block_attacker(atk) {
                chosen.push(i);
                if chosen.len() == needed {
                    break;
                }
            }
        }
        if chosen.len() == needed {
            // Remover de trás para frente preserva os índices restantes.
            for i in chosen.into_iter().rev() {
                free.remove(i);
            }
            if atk.traits.trample {
                // Atropelar passa o excedente mesmo bloqueado (CR 702.19b).
                let absorbed = 1;
                total += (atk.power.max(0) - absorbed).max(0);
            }
        } else {
            total += atk.power.max(0);
        }
    }
    total
}

/// Dano que o oponente causaria no próximo ataque dele. As criaturas viradas e
/// enjoadas dele contam (desviram no turno dele); as minhas viradas não
/// bloqueiam — é daí que sai a cautela ao atacar com tudo.
pub fn incoming_damage(s: &Snapshot) -> i32 {
    let attackers: Vec<&CreatureInfo> = s
        .opp_creatures
        .iter()
        .filter(|c| c.can_attack_next_turn())
        .collect();
    let blockers: Vec<&CreatureInfo> = s
        .my_creatures
        .iter()
        .filter(|c| !c.tapped && !c.cant_block)
        .collect();
    unblocked_damage(&attackers, &blockers)
}

/// Dano que eu causaria no meu próximo ataque.
pub fn outgoing_damage(s: &Snapshot) -> i32 {
    let attackers: Vec<&CreatureInfo> = s
        .my_creatures
        .iter()
        .filter(|c| c.can_attack_next_turn())
        .collect();
    let blockers: Vec<&CreatureInfo> = s
        .opp_creatures
        .iter()
        .filter(|c| !c.cant_block)
        .collect();
    unblocked_damage(&attackers, &blockers)
}

fn mana_score(sources: usize, colors: u32) -> i64 {
    let capped = sources.min(7) as i64 * MANA_SOURCE;
    let surplus = sources.saturating_sub(7) as i64 * MANA_SURPLUS;
    capped + surplus + colors as i64 * MANA_COLOR
}

fn board_value(creatures: &[CreatureInfo]) -> i64 {
    creatures.iter().map(CreatureInfo::value).sum()
}

// ---------------------------------------------------------------------------
// Avaliação
// ---------------------------------------------------------------------------

/// Avaliação da posição do ponto de vista de `s.me`. Positivo = estou melhor.
pub fn evaluate(s: &Snapshot) -> i64 {
    // Terminal domina tudo: nenhum material compensa ter perdido.
    if s.opp_life <= 0 || s.opp_poison >= 10 {
        return TERMINAL;
    }
    if s.my_life <= 0 || s.my_poison >= 10 {
        return -TERMINAL;
    }

    let mut v = 0i64;
    v += (s.my_life - s.opp_life) as i64 * LIFE;
    v += board_value(&s.my_creatures) - board_value(&s.opp_creatures);
    v += (s.my_hand as i64 - s.opp_hand as i64) * CARD_IN_HAND;
    v += (s.my_nonland_permanents as i64 - s.opp_nonland_permanents as i64) * NONLAND_PERMANENT;
    v += mana_score(s.my_mana_sources, s.my_mana_colors)
        - mana_score(s.opp_mana_sources, s.opp_mana_colors);
    // Veneno é um relógio paralelo (CR 704.5c).
    v += (s.opp_poison - s.my_poison) as i64 * 90;

    let incoming = incoming_damage(s);
    if incoming >= s.my_life {
        v -= LETHAL_THREAT;
    } else if incoming * 2 >= s.my_life {
        v -= LETHAL_THREAT / 4;
    }
    let outgoing = outgoing_damage(s);
    if outgoing >= s.opp_life {
        v += LETHAL_CHANCE;
    } else if outgoing * 2 >= s.opp_life {
        v += LETHAL_CHANCE / 4;
    }

    if s.my_library == 0 {
        v -= DECKING;
    }
    if s.opp_library == 0 {
        v += DECKING;
    }
    v
}

/// Desempate estável por semente: splitmix64 sobre o índice da ação.
/// Não é aleatoriedade de jogo (essa só sai de `game.rng`) — é ruído de ±3
/// centipontos, para que sementes diferentes gerem partidas diferentes sem
/// jamais inverter uma decisão que importa.
pub fn jitter(seed: u64, index: usize) -> i64 {
    let mut x = seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % 7) as i64 - 3
}
