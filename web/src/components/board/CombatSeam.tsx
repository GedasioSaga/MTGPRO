import clsx from 'clsx'
import type { ReactElement } from 'react'
import type { CardView, ObjectId } from '../../types/protocol'
import { cssVars } from './boardVisuals'
import type { CombatLane, CombatPlan } from './combatPlan'

export interface CombatSeamProps {
  plan: CombatPlan
  cards: Record<ObjectId, CardView>
  /** Em que metade da mesa está o atacante. Decide para onde cada número aponta. */
  attackerSide: 'top' | 'bottom'
}

/**
 * Placar de combate, na costura entre os dois campos.
 *
 * Cada raia do plano vira uma célula com a MESMA largura usada pelas fileiras
 * de criatura, então o placar do duelo cai exatamente entre o atacante e o
 * bloqueador dele. O número de cima é o dano que sobe, o de baixo é o que
 * desce — quem morre está marcado no próprio número, não numa legenda.
 */
export function CombatSeam({ plan, cards, attackerSide }: CombatSeamProps): ReactElement | null {
  if (!plan.active) return null

  return (
    <div className="combat-seam" aria-hidden="true">
      <div className="combat-seam__rail">
        {plan.lanes.map((lane) => (
          <div
            key={lane.key}
            className={clsx('combat-seam__cell', `combat-seam__cell--${lane.kind}`)}
            style={cssVars({ '--slots': String(lane.slots) })}
          >
            {lane.duel === null ? (
              <BreachChip damage={plan.breachDamage} />
            ) : (
              <DuelChip lane={lane} plan={plan} cards={cards} attackerSide={attackerSide} />
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

function DuelChip({
  lane,
  plan,
  cards,
  attackerSide,
}: {
  lane: CombatLane
  plan: CombatPlan
  cards: Record<ObjectId, CardView>
  attackerSide: 'top' | 'bottom'
}): ReactElement | null {
  const duel = lane.duel
  if (duel === null) return null

  const attackerKills = duel.blockers.some((id) => plan.doomed.has(id))
  const attackerDies = plan.doomed.has(duel.attacker)
  const toBlockers = duel.dealtByAttacker - duel.trample

  const up =
    attackerSide === 'bottom'
      ? { value: toBlockers, kills: attackerKills }
      : { value: duel.dealtByBlockers, kills: attackerDies }
  const down =
    attackerSide === 'bottom'
      ? { value: duel.dealtByBlockers, kills: attackerDies }
      : { value: toBlockers, kills: attackerKills }

  return (
    <div className="combat-duel" title={duelTitle(duel, cards)}>
      <Beam dir="up" value={up.value} kills={up.kills} />
      <span className="combat-duel__cross">⚔</span>
      <Beam dir="down" value={down.value} kills={down.kills} />
    </div>
  )
}

function Beam({
  dir,
  value,
  kills,
}: {
  dir: 'up' | 'down'
  value: number
  kills: boolean
}): ReactElement {
  return (
    <span
      className={clsx('combat-beam', `combat-beam--${dir}`, kills && 'combat-beam--kills')}
      data-zero={value === 0 ? 'true' : 'false'}
    >
      <span className="combat-beam__glyph">{dir === 'up' ? '▲' : '▼'}</span>
      <span className="combat-beam__value">{value}</span>
      {kills ? <span className="combat-beam__skull">☠</span> : null}
    </span>
  )
}

/**
 * Origem do dano que passa. Só um ponto: o número vive ANCORADO NO ALVO (ver
 * `TargetArrows`), e repeti-lo aqui, solto no vão, era o elemento de maior
 * contraste da tela sem ser a informação mais importante dela.
 */
function BreachChip({ damage }: { damage: number }): ReactElement {
  return (
    <div
      className="combat-breach"
      data-breach-anchor="true"
      title={`passa ${damage}`}
    />
  )
}

function duelTitle(
  duel: { attacker: ObjectId; blockers: ObjectId[] },
  cards: Record<ObjectId, CardView>,
): string {
  const name = (id: ObjectId): string => cards[id]?.name ?? `#${id}`
  return `${name(duel.attacker)} × ${duel.blockers.map(name).join(', ')}`
}
