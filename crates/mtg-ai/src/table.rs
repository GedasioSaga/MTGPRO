//! Retrato da mesa quando há mais de um oponente.
//!
//! O `Snapshot` de `eval` continua tendo um lado "eu" e um lado "oponente"
//! porque combate em Magic é sempre entre dois jogadores: só o defensor pode
//! bloquear (CR 509.1a). O que muda com três ou quatro jogadores não é o
//! combate — é *contra quem* ele acontece, e quanto o resto da mesa cobra por
//! eu ter me exposto. Este módulo guarda o que falta para responder isso:
//!
//!   - `OpponentInfo` — retrato público de um oponente que não é o foco.
//!   - `OppRef` — visão emprestada e uniforme, para que uma heurística possa
//!     percorrer foco e não-foco no mesmo laço sem clonar campo de criatura.
//!   - `commander_damage_between` — leitura do segundo relógio de vida do
//!     Commander (CR 903.10).
use mtg_core::ids::PlayerId;
use mtg_core::state::GameState;

use crate::eval::CreatureInfo;

/// CR 903.10 — vinte e um pontos de dano de combate de um mesmo comandante
/// fazem o jogador perder a partida, independente da vida dele. Reexportado do
/// motor de propósito: duas constantes com o mesmo nome e valores diferentes
/// seria a próxima falha silenciosa.
pub const COMMANDER_LETHAL: i32 = mtg_core::engine::commander::LETHAL_COMMANDER_DAMAGE;

/// Maior relógio de comandante aberto de `from` contra `to` (CR 903.10).
///
/// É o **máximo**, não a soma: 21 têm de vir de um mesmo comandante, então dois
/// comandantes com 15 cada não matam ninguém. O número é do motor —
/// `PlayerState::commander_damage` — e este módulo só o indexa por dono, que é
/// a pergunta que a heurística faz ("quanto o jogador B já tirou de mim?").
///
/// Fora de Commander o mapa nasce vazio e a função devolve zero para tudo, o
/// que faz todo termo de comandante sumir da avaliação em vez de inventar
/// número — o comportamento correto em Standard, Modern e Pauper.
pub fn commander_damage_between(state: &GameState, from: PlayerId, to: PlayerId) -> i32 {
    let Some(victim) = state.players.get(to.index()) else {
        return 0;
    };
    victim
        .commander_damage
        .iter()
        .filter(|(commander, _)| {
            state
                .object(**commander)
                .is_some_and(|o| o.is_commander && o.owner == from)
        })
        .map(|(_, amount)| *amount)
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Oponente
// ---------------------------------------------------------------------------

/// Retrato público de um oponente. Só informação que qualquer jogador na mesa
/// enxerga: nada de conteúdo de mão ou de biblioteca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpponentInfo {
    pub id: PlayerId,
    pub life: i32,
    pub poison: i32,
    pub hand: usize,
    pub library: usize,
    pub lands: usize,
    pub nonland_permanents: usize,
    pub mana_sources: usize,
    pub mana_colors: u32,
    pub creatures: Vec<CreatureInfo>,
    /// CR 903.10a — dano que o comandante *dele* já causou a mim.
    pub commander_damage_to_me: i32,
    /// CR 903.10a — dano que o *meu* comandante já causou a ele.
    pub my_commander_damage: i32,
}

impl OpponentInfo {
    pub fn new(id: PlayerId, life: i32, poison: i32) -> OpponentInfo {
        OpponentInfo {
            id,
            life,
            poison,
            hand: 0,
            library: 0,
            lands: 0,
            nonland_permanents: 0,
            mana_sources: 0,
            mana_colors: 0,
            creatures: Vec::new(),
            commander_damage_to_me: 0,
            my_commander_damage: 0,
        }
    }

    pub fn as_ref(&self) -> OppRef<'_> {
        OppRef {
            id: self.id,
            life: self.life,
            poison: self.poison,
            hand: self.hand,
            library: self.library,
            lands: self.lands,
            nonland_permanents: self.nonland_permanents,
            mana_sources: self.mana_sources,
            mana_colors: self.mana_colors,
            creatures: &self.creatures,
            commander_damage_to_me: self.commander_damage_to_me,
            my_commander_damage: self.my_commander_damage,
        }
    }
}

/// Visão emprestada de um oponente. Existe para que o foco (que mora em campos
/// planos do `Snapshot`, porque a simulação de combate escreve neles) e os
/// demais oponentes possam ser percorridos pelo mesmo laço sem alocar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OppRef<'a> {
    pub id: PlayerId,
    pub life: i32,
    pub poison: i32,
    pub hand: usize,
    pub library: usize,
    pub lands: usize,
    pub nonland_permanents: usize,
    pub mana_sources: usize,
    pub mana_colors: u32,
    pub creatures: &'a [CreatureInfo],
    pub commander_damage_to_me: i32,
    pub my_commander_damage: i32,
}

impl OppRef<'_> {
    /// Ainda está na partida. Cobre os três relógios: vida (CR 704.5a),
    /// veneno (CR 704.5c) e dano de comandante (CR 903.10a).
    pub fn is_alive(&self) -> bool {
        self.life > 0 && self.poison < 10 && self.my_commander_damage < COMMANDER_LETHAL
    }

    /// Poder somado das criaturas dele que conseguem atacar no próximo turno.
    pub fn attacking_power(&self) -> i32 {
        self.creatures
            .iter()
            .filter(|c| c.can_attack_next_turn())
            .map(|c| c.power.max(0))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ids::ObjectId;

    const B: PlayerId = PlayerId(1);

    fn opponent(life: i32, poison: i32) -> OpponentInfo {
        OpponentInfo::new(B, life, poison)
    }

    #[test]
    fn vivo_exige_os_tres_relogios_abertos() {
        let sadio = opponent(1, 0);
        assert!(sadio.as_ref().is_alive());

        // CR 704.5a — vida zero ou menos.
        assert!(!opponent(0, 0).as_ref().is_alive());
        assert!(!opponent(-3, 0).as_ref().is_alive());
        // CR 704.5c — dez marcadores de veneno.
        assert!(!opponent(40, 10).as_ref().is_alive());
        assert!(opponent(40, 9).as_ref().is_alive());
    }

    #[test]
    fn vinte_e_um_de_comandante_mata_com_a_vida_cheia() {
        // CR 903.10 — o relógio de comandante é independente da vida, e é
        // justamente isso que a heurística tem de enxergar.
        let mut alvo = opponent(40, 0);
        alvo.my_commander_damage = COMMANDER_LETHAL - 1;
        assert!(alvo.as_ref().is_alive(), "20 de dano de comandante já matou");
        alvo.my_commander_damage = COMMANDER_LETHAL;
        assert!(
            !alvo.as_ref().is_alive(),
            "21 de dano de comandante não matou com 40 de vida"
        );
    }

    #[test]
    fn constante_de_letalidade_segue_a_do_motor() {
        // Duas constantes com o mesmo nome e valores diferentes seria uma falha
        // silenciosa: a IA calcularia um relógio e o motor aplicaria outro.
        assert_eq!(
            COMMANDER_LETHAL,
            mtg_core::engine::commander::LETHAL_COMMANDER_DAMAGE
        );
    }

    #[test]
    fn poder_de_ataque_ignora_quem_nao_pode_atacar() {
        let mut o = opponent(20, 0);
        o.creatures
            .push(CreatureInfo::vanilla(ObjectId(1), B, 3, 3));
        let mut parado = CreatureInfo::vanilla(ObjectId(2), B, 5, 5);
        parado.cant_attack = true;
        o.creatures.push(parado);
        assert_eq!(o.as_ref().attacking_power(), 3);
    }
}
