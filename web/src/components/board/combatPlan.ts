import { defenderPlayer } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'

/**
 * Modelo de combate para LEITURA, derivado só da view.
 *
 * O tabuleiro parou de contar quem bate em quem por seta: agora ele PAREIA. Um
 * duelo é uma raia — atacante e bloqueadores na mesma coluna vertical, nas duas
 * metades do campo, com o placar de dano na costura entre elas. Este arquivo é
 * quem decide as raias e quem faz a conta de dano; o desenho só obedece.
 *
 * Nenhuma regra nova é inventada aqui: a atribuição segue a ordem em que os
 * bloqueadores aparecem, atropelar joga a sobra no jogador e toque mortal
 * mata com 1. É o mesmo que o motor faz; se divergir, o motor ganha.
 */

/** Um atacante e quem o segurou. */
export interface CombatDuel {
  attacker: ObjectId
  blockers: ObjectId[]
  /** Dano que o atacante distribui, somado. */
  dealtByAttacker: number
  /** Dano que os bloqueadores devolvem, somado. */
  dealtByBlockers: number
  /** Sobra que ainda alcança o jogador (atropelar). */
  trample: number
}

export type CombatLaneKind = 'duel' | 'breach'

/**
 * Uma coluna do combate. As duas fileiras do campo consomem a MESMA lista, na
 * mesma ordem e com a mesma largura — é isso, e só isso, que faz o atacante
 * cair exatamente embaixo do bloqueador dele.
 */
export interface CombatLane {
  key: string
  kind: CombatLaneKind
  /** Quantas cartas de largura a coluna reserva. */
  slots: number
  duel: CombatDuel | null
}

export interface CombatPlan {
  active: boolean
  lanes: CombatLane[]
  /** Raia de cada criatura em combate. */
  laneOf: Map<ObjectId, number>
  /** Atacantes sem bloqueio. */
  breach: ObjectId[]
  /** Dano que chega no jogador defensor. */
  breachDamage: number
  defender: PlayerId | null
  /** Dano que cada criatura vai receber neste combate. */
  incoming: Map<ObjectId, number>
  /** Criaturas que não sobrevivem a este combate. */
  doomed: Set<ObjectId>
}

export const EMPTY_COMBAT_PLAN: CombatPlan = {
  active: false,
  lanes: [],
  laneOf: new Map(),
  breach: [],
  breachDamage: 0,
  defender: null,
  incoming: new Map(),
  doomed: new Set(),
}

function hasKeyword(card: CardView, keyword: string): boolean {
  return card.keywords.some((k) => k.toLowerCase().replace(/[\s_-]/g, '') === keyword)
}

function power(card: CardView): number {
  return Math.max(0, card.power ?? 0)
}

export function buildCombatPlan(cards: Record<ObjectId, CardView>): CombatPlan {
  const onBoard = Object.values(cards).filter((c) => c.zone === 'Battlefield')
  const attackers = onBoard.filter((c) => c.attacking !== null).sort((a, b) => a.id - b.id)
  if (attackers.length === 0) return EMPTY_COMBAT_PLAN

  // `blocking` (bloqueador → atacante) e `blockedBy` (atacante → bloqueador)
  // dizem a mesma coisa por dois caminhos; ler os dois evita raia vazia quando
  // só um dos lados chegou na view.
  const blockersOf = new Map<ObjectId, ObjectId[]>()
  for (const attacker of attackers) blockersOf.set(attacker.id, [])
  for (const card of onBoard) {
    for (const target of card.blocking) {
      const list = blockersOf.get(target)
      if (list !== undefined && !list.includes(card.id)) list.push(card.id)
    }
  }
  for (const attacker of attackers) {
    const list = blockersOf.get(attacker.id)
    if (list === undefined) continue
    for (const id of attacker.blockedBy) {
      if (cards[id] !== undefined && !list.includes(id)) list.push(id)
    }
  }

  const incoming = new Map<ObjectId, number>()
  const deathtouched = new Set<ObjectId>()
  const addDamage = (id: ObjectId, amount: number): void => {
    if (amount <= 0) return
    incoming.set(id, (incoming.get(id) ?? 0) + amount)
  }

  const duels: CombatDuel[] = []
  const breach: ObjectId[] = []
  let breachDamage = 0
  let defender: PlayerId | null = null

  for (const attacker of attackers) {
    if (defender === null && attacker.attacking !== null) {
      defender = defenderPlayer(attacker.attacking)
    }

    const blockers = blockersOf.get(attacker.id) ?? []
    if (blockers.length === 0) {
      breach.push(attacker.id)
      breachDamage += power(attacker)
      continue
    }

    const deadly = hasKeyword(attacker, 'deathtouch')
    let left = power(attacker)
    let dealtByBlockers = 0

    for (const id of blockers) {
      const blocker = cards[id]
      if (blocker === undefined) continue
      dealtByBlockers += power(blocker)
      if (hasKeyword(blocker, 'deathtouch') && power(blocker) > 0) {
        deathtouched.add(attacker.id)
      }
      const needed = deadly
        ? 1
        : Math.max(0, (blocker.toughness ?? 0) - Math.max(0, blocker.damage))
      const given = Math.min(left, needed)
      left -= given
      addDamage(id, given)
      if (deadly && given > 0) deathtouched.add(id)
    }

    const overflow = left
    const tramples = hasKeyword(attacker, 'trample')
    if (overflow > 0) {
      if (tramples) breachDamage += overflow
      else addDamage(blockers[blockers.length - 1], overflow)
    }

    addDamage(attacker.id, dealtByBlockers)
    duels.push({
      attacker: attacker.id,
      blockers,
      dealtByAttacker: power(attacker),
      dealtByBlockers,
      trample: tramples ? overflow : 0,
    })
  }

  const doomed = new Set<ObjectId>()
  for (const [id, amount] of incoming) {
    const card = cards[id]
    if (card === undefined || card.toughness === null) continue
    if (hasKeyword(card, 'indestructible')) continue
    if (deathtouched.has(id) || amount + Math.max(0, card.damage) >= card.toughness) {
      doomed.add(id)
    }
  }

  const lanes: CombatLane[] = []
  const laneOf = new Map<ObjectId, number>()
  for (const duel of duels) {
    laneOf.set(duel.attacker, lanes.length)
    for (const id of duel.blockers) laneOf.set(id, lanes.length)
    lanes.push({
      key: `duel-${duel.attacker}`,
      kind: 'duel',
      slots: Math.max(1, duel.blockers.length),
      duel,
    })
  }
  if (breach.length > 0) {
    for (const id of breach) laneOf.set(id, lanes.length)
    lanes.push({ key: 'breach', kind: 'breach', slots: breach.length, duel: null })
  }

  return { active: true, lanes, laneOf, breach, breachDamage, defender, incoming, doomed }
}

/** Total de colunas-carta reservadas: o que dita a largura de cada raia. */
export function totalSlots(plan: CombatPlan): number {
  return plan.lanes.reduce((sum, lane) => sum + lane.slots, 0)
}
