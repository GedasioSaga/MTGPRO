//! Política de mesa: contra quem jogar quando há mais de um oponente.
//!
//! No duelo essa pergunta não existe — só há um alvo, e tudo o que importa é
//! "esta jogada me deixa melhor?". Com três ou quatro jogadores aparecem três
//! perguntas novas, e este módulo responde as três:
//!
//!   1. **Quem é perigoso.** Vida alta não é ameaça: ameaça é poder em campo,
//!      carta na mão e relógio de comandante aberto contra mim.
//!   2. **Quem vale matar.** Perigo mais proximidade da eliminação — tirar um
//!      jogador da mesa apaga um relógio inteiro apontado para mim.
//!   3. **Quanto custa atacar.** Virar o time inteiro num oponente entrega o
//!      turno seguinte aos outros; o desconto é o preço dessa exposição.
//!
//! Tudo aqui é função pura de `Snapshot`: mesma posição, mesma nota, sempre.
//! Nenhuma iteração de mapa produz ordem, e todo laço percorre `Vec` já em
//! ordem de `PlayerId`.
use mtg_core::ids::{ObjectId, PlayerId};

use crate::eval::{self, Snapshot};
use crate::table::{OppRef, COMMANDER_LETHAL};

// ---------------------------------------------------------------------------
// Pesos de ameaça
// ---------------------------------------------------------------------------
//
// Mesma unidade de `eval`: centipontos. Calibrados para que um oponente com
// campo vazio e 40 de vida fique **abaixo** de um com 8 de vida e três 3/3 —
// que é exatamente o erro que "ataque quem tem mais vida" comete.

/// Poder em campo é o que mata: é o termo dominante da ameaça.
const THREAT_POWER: i64 = 55;
/// Corpo, independente de tamanho — bloqueia e vira alvo de anthem.
const THREAT_BODY: i64 = 20;
/// Carta na mão é ameaça desconhecida, e desconhecido assusta.
const THREAT_CARD: i64 = 40;
const THREAT_MANA: i64 = 12;
const THREAT_PERMANENT: i64 = 25;
/// Vida entra devagar e de propósito: mede quanto tempo ele aguenta, não
/// quanto dano ele faz. É este peso pequeno que separa "mais vida" de
/// "mais ameaçador".
const THREAT_LIFE: i64 = 6;
/// O ataque dele já me mata agora: nada na mesa é mais urgente.
const THREAT_LETHAL_NOW: i64 = 600;
/// Teto do termo de comandante, atingido quando o relógio dele fecha em 21.
const THREAT_COMMANDER: i64 = 1_200;

/// Eliminar este oponente está ao alcance agora.
const FINISH_NOW: i64 = 900;
/// Escala do "quão perto estou de eliminá-lo", proporcional à vida restante.
const FINISH_SCALE: i64 = 500;
/// Teto do termo "meu comandante já abriu relógio contra ele".
const FINISH_COMMANDER: i64 = 700;

/// Amplitude do bônus de escolher o alvo certo para o ataque. Pequena de
/// propósito: ela desempata entre alvos, não substitui a avaliação da posição.
const ATTACK_TARGET_SPREAD: i64 = 150;

/// Custo por ponto de poder adversário que fica sem resposta em casa.
const EXPOSURE_PER_POINT: i64 = 20;

/// Liderança tolerada sem imposto — abaixo disso ninguém percebe.
const LEADER_TAX_FLOOR: i64 = 400;
/// Fatia da liderança excedente que vira desconto.
const LEADER_TAX_PERCENT: i64 = 12;
/// Teto do imposto. Menor que `LETHAL_CHANCE / 4`, para que a política nunca
/// consiga vetar uma linha que de fato fecha a partida.
const LEADER_TAX_CAP: i64 = 500;

// ---------------------------------------------------------------------------
// Ameaça
// ---------------------------------------------------------------------------

/// Quão perigoso este oponente é para mim.
///
/// Deliberadamente **não** é "quem tem mais vida". Um jogador com 40 de vida e
/// campo vazio não me mata em turno nenhum; um com 8 de vida, mão cheia e três
/// criaturas grandes me mata no próximo. Bater no primeiro por ele "estar na
/// frente no placar" é o erro clássico de bot de mesa cheia.
pub fn threat(my_life: i32, o: &OppRef<'_>) -> i64 {
    if !o.is_alive() {
        return 0;
    }
    let power: i64 = o.creatures.iter().map(|c| c.power.max(0) as i64).sum();
    let mut v = power * THREAT_POWER
        + o.creatures.len() as i64 * THREAT_BODY
        + o.hand as i64 * THREAT_CARD
        + o.mana_sources.min(10) as i64 * THREAT_MANA
        + o.nonland_permanents as i64 * THREAT_PERMANENT
        + o.life.max(0) as i64 * THREAT_LIFE;

    if power > 0 && power >= my_life as i64 {
        v += THREAT_LETHAL_NOW;
    }
    // CR 903.10a — o comandante dele é um segundo relógio contra mim, e ele
    // acelera: os últimos pontos valem muito mais que os primeiros, porque são
    // eles que fecham os 21.
    v += commander_pressure(o.commander_damage_to_me, THREAT_COMMANDER);
    v
}

/// Termo quadrático do relógio de comandante: zero em zero, `cap` em 21.
fn commander_pressure(dealt: i32, cap: i64) -> i64 {
    let d = dealt.clamp(0, COMMANDER_LETHAL) as i64;
    let lethal = COMMANDER_LETHAL as i64;
    d * d * cap / (lethal * lethal)
}

/// Quanto vale mirar este oponente: perigo mais proximidade da eliminação.
///
/// Os dois termos são necessários. Só perigo faz o bot bater eternamente no
/// jogador mais forte sem nunca fechar nada; só proximidade faz ele catar o
/// mais fraco enquanto o mais forte monta a mesa e mata todo mundo.
pub fn kill_priority(s: &Snapshot, o: &OppRef<'_>) -> i64 {
    if !o.is_alive() {
        return 0;
    }
    let mut v = threat(s.my_life, o);
    let damage = eval::outgoing_damage_against(s, o) as i64;
    let life = o.life.max(1) as i64;
    if damage >= life {
        v += FINISH_NOW;
    } else {
        v += damage * FINISH_SCALE / life;
    }
    // CR 903.10a — relógio que eu já abri contra ele conta como vida que ele
    // não tem mais: fechar um relógio começado é mais barato que abrir outro.
    v += commander_pressure(o.my_commander_damage, FINISH_COMMANDER);
    v
}

/// Índice do oponente mais ameaçador na lista. Empate resolvido pelo menor
/// `PlayerId` — a lista chega em ordem de id, então a escolha é estável.
pub fn most_threatening(my_life: i32, list: &[OppRef<'_>]) -> Option<usize> {
    let mut best: Option<(i64, usize)> = None;
    for (i, o) in list.iter().enumerate() {
        if !o.is_alive() {
            continue;
        }
        let score = threat(my_life, o);
        if best.is_none_or(|(bs, _)| score > bs) {
            best = Some((score, i));
        }
    }
    best.map(|(_, i)| i)
}

// ---------------------------------------------------------------------------
// Escolha de alvo de ataque
// ---------------------------------------------------------------------------

/// Bônus por atacar o oponente certo, normalizado na faixa
/// `[0, ATTACK_TARGET_SPREAD]`.
///
/// Existe porque a avaliação de posição é quase simétrica entre oponentes:
/// tirar 3 de vida de um ou de outro mexe o mesmo tanto na nota. Sem este
/// termo o bot escolheria o defensor pelo desempate de `jitter`, ou seja, no
/// sorteio. Em duelo devolve zero — não há escolha a fazer.
pub fn target_player_bonus(s: &Snapshot, defender: PlayerId) -> i64 {
    let views = s.opponents_view();
    let living: Vec<&OppRef<'_>> = views.iter().filter(|o| o.is_alive()).collect();
    if living.len() < 2 {
        return 0;
    }
    let mut best = i64::MIN;
    let mut worst = i64::MAX;
    let mut chosen: Option<i64> = None;
    for o in &living {
        let p = kill_priority(s, o);
        best = best.max(p);
        worst = worst.min(p);
        if o.id == defender {
            chosen = Some(p);
        }
    }
    let Some(chosen) = chosen else { return 0 };
    if best <= worst {
        return 0;
    }
    (chosen - worst) * ATTACK_TARGET_SPREAD / (best - worst)
}

/// Desconto por ficar indefeso depois do ataque.
///
/// CR 508.1a não obriga ninguém a atacar com tudo, e numa mesa de três ou mais
/// atacar com o time inteiro custa caro: os atacantes viram (CR 508.1f), e o
/// oponente que eu **não** ataquei chega no turno dele com a minha casa aberta.
/// O desconto é a diferença entre o que eles podem mandar e o que sobrou em pé
/// para bloquear. Em duelo devolve zero: lá não existe "os outros".
pub fn exposure_penalty(s: &Snapshot, defender: PlayerId, attackers: &[ObjectId]) -> i64 {
    let views = s.opponents_view();
    let living: Vec<&OppRef<'_>> = views.iter().filter(|o| o.is_alive()).collect();
    if living.len() < 2 {
        return 0;
    }
    let pressure: i64 = living
        .iter()
        .filter(|o| o.id != defender)
        .map(|o| o.attacking_power() as i64)
        .sum();
    if pressure <= 0 {
        return 0;
    }
    // Vigilância não vira ao atacar (CR 702.20b), então continua em casa.
    let defense: i64 = s
        .my_creatures
        .iter()
        .filter(|c| !c.tapped && !c.cant_block)
        .filter(|c| !attackers.contains(&c.id) || c.traits.vigilance)
        .map(|c| (c.power.max(0) + c.effective_toughness().max(0)) as i64 / 2)
        .sum();
    (pressure - defense).max(0) * EXPOSURE_PER_POINT
}

// ---------------------------------------------------------------------------
// Não ser o líder visível
// ---------------------------------------------------------------------------

/// Desconto sobre a própria liderança quando há três ou mais jogadores vivos.
///
/// **Isto é heurística de sabor, não de regra.** Não existe carta nem CR que
/// puna quem está ganhando; o que existe é a mesa. Numa partida de quatro, o
/// jogador que claramente está na frente recebe o ataque dos outros três ao
/// mesmo tempo, e sai da posição de líder direto para a de morto. O bot então
/// prefere, entre duas linhas de valor parecido, a que não o deixa como alvo
/// óbvio.
///
/// Três travas para que a política nunca custe a partida: só vale a partir de
/// dois oponentes vivos (duelo fica intocado), só incide sobre a liderança
/// acima de `LEADER_TAX_FLOOR`, e é limitada a `LEADER_TAX_CAP`, que é menor
/// que qualquer termo de letalidade de `eval`. Um bot com medo de ganhar
/// perderia mais do que ganha.
pub fn leader_tax(mine: i64, best_opponent: i64, living_opponents: usize) -> i64 {
    if living_opponents < 2 {
        return 0;
    }
    let lead = mine - best_opponent;
    if lead <= LEADER_TAX_FLOOR {
        return 0;
    }
    ((lead - LEADER_TAX_FLOOR) * LEADER_TAX_PERCENT / 100).min(LEADER_TAX_CAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::CreatureInfo;
    use crate::table::OpponentInfo;

    const ME: PlayerId = PlayerId(0);
    const B: PlayerId = PlayerId(1);
    const C: PlayerId = PlayerId(2);

    fn opponent(id: PlayerId, life: i32, bodies: &[(u32, i32, i32)]) -> OpponentInfo {
        let mut o = OpponentInfo::new(id, life, 0);
        for (oid, p, t) in bodies {
            o.creatures
                .push(CreatureInfo::vanilla(ObjectId(*oid), id, *p, *t));
        }
        o
    }

    #[test]
    fn vida_alta_com_campo_vazio_ameaca_menos_que_vida_baixa_com_campo() {
        // O erro que este teste tranca: escolher alvo pelo placar de vida.
        let calmo = opponent(B, 40, &[]);
        let perigoso = opponent(C, 8, &[(1, 5, 5), (2, 4, 4), (3, 3, 3)]);
        let a = threat(20, &calmo.as_ref());
        let b = threat(20, &perigoso.as_ref());
        assert!(
            b > a,
            "campo de 12 de poder ameaçou menos que 40 de vida vazia: {b} <= {a}"
        );
    }

    #[test]
    fn mao_cheia_ameaca_mais_que_mao_vazia() {
        let mut cheio = opponent(B, 20, &[(1, 2, 2)]);
        cheio.hand = 7;
        let vazio = opponent(C, 20, &[(2, 2, 2)]);
        assert!(threat(20, &cheio.as_ref()) > threat(20, &vazio.as_ref()));
    }

    #[test]
    fn relogio_de_comandante_aberto_aumenta_a_ameaca() {
        // CR 903.10a: 18 de dano de comandante contra mim significa que faltam
        // três — esse oponente é mais perigoso que um idêntico sem relógio.
        let limpo = opponent(B, 20, &[(1, 3, 3)]);
        let mut apertando = opponent(C, 20, &[(2, 3, 3)]);
        apertando.commander_damage_to_me = 18;
        let sem = threat(20, &limpo.as_ref());
        let com = threat(20, &apertando.as_ref());
        assert!(com > sem, "relógio de comandante não pesou: {com} <= {sem}");
    }

    #[test]
    fn pressao_de_comandante_e_zero_no_zero_e_teto_no_letal() {
        assert_eq!(commander_pressure(0, 1_000), 0);
        assert_eq!(commander_pressure(COMMANDER_LETHAL, 1_000), 1_000);
        assert_eq!(commander_pressure(99, 1_000), 1_000, "não saturou acima de 21");
        assert_eq!(commander_pressure(-5, 1_000), 0, "valor negativo virou bônus");
    }

    #[test]
    fn mais_ameacador_escolhe_o_campo_maior_e_ignora_morto() {
        let fraco = opponent(B, 40, &[]);
        let forte = opponent(C, 12, &[(1, 6, 6)]);
        let list = vec![fraco.as_ref(), forte.as_ref()];
        let Some(i) = most_threatening(20, &list) else {
            panic!("nenhum oponente escolhido com dois vivos na mesa");
        };
        assert_eq!(i, 1);

        let morto = opponent(B, 0, &[(1, 9, 9)]);
        let vivo = opponent(C, 5, &[]);
        let list = vec![morto.as_ref(), vivo.as_ref()];
        let Some(i) = most_threatening(20, &list) else {
            panic!("oponente vivo não foi escolhido");
        };
        assert_eq!(i, 1, "escolheu um jogador já eliminado");
    }

    #[test]
    fn imposto_de_lideranca_nao_incide_em_duelo() {
        // Regressão dura: o duelo tem de continuar com a nota de antes.
        assert_eq!(leader_tax(50_000, 0, 1), 0);
        assert_eq!(leader_tax(50_000, 0, 0), 0);
    }

    #[test]
    fn imposto_de_lideranca_e_comedido_e_limitado() {
        assert_eq!(leader_tax(400, 0, 2), 0, "liderança pequena foi taxada");
        assert_eq!(leader_tax(1_400, 0, 2), 120);
        assert_eq!(
            leader_tax(1_000_000, 0, 2),
            LEADER_TAX_CAP,
            "imposto passou do teto"
        );
        assert_eq!(leader_tax(0, 5_000, 3), 0, "quem está atrás foi taxado");
    }

    #[test]
    fn exposicao_nao_pune_duelo_e_pune_mesa_cheia() {
        let mut s = Snapshot::empty(ME, B);
        s.my_creatures
            .push(CreatureInfo::vanilla(ObjectId(10), ME, 3, 3));
        s.opp_creatures
            .push(CreatureInfo::vanilla(ObjectId(20), B, 4, 4));
        let attackers = vec![ObjectId(10)];
        assert_eq!(
            exposure_penalty(&s, B, &attackers),
            0,
            "duelo não tem terceiro para me punir"
        );

        s.others.push(opponent(C, 20, &[(30, 6, 6)]));
        let penalty = exposure_penalty(&s, B, &attackers);
        assert!(
            penalty > 0,
            "atacar B com tudo deixou C livre e não custou nada: {penalty}"
        );
    }
}
