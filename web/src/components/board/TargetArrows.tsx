import { useCallback, useEffect, useState } from 'react'
import type { GameView, ObjectId } from '../../types/protocol'
import { EMPTY_COMBAT_PLAN } from './combatPlan'
import type { CombatPlan } from './combatPlan'

type ArrowTone = 'breach' | 'spell'

interface Point {
  x: number
  y: number
}

interface Arrow {
  key: string
  tone: ArrowTone
  from: Point
  to: Point
  /** Só o vetor de dano ao jogador carrega número. */
  label: string | null
}

const TONE_COLOR: Record<ArrowTone, string> = {
  breach: '#ff5a48',
  spell: '#e8a94c',
}

/** Quanto a ponta recua para não entrar por baixo da carta. */
const EDGE_INSET = 0.44
/** Quanto a curva arqueia em relação ao comprimento do vão. */
const BOW = 0.16
/** Janela de reassentamento: cobre transições de tap, leque e entrada de carta. */
const SETTLE_MS = 900

export interface TargetArrowsProps {
  view: GameView
  plan?: CombatPlan
}

/**
 * Vetores de alvo, ancorados no DOM real.
 *
 * Combate NÃO desenha seta: quem bloqueia quem se lê pela coluna (ver
 * `combatPlan`), e o placar mora na costura. Sobrou aqui exatamente um traço
 * longo — o dano que PASSA, um vetor grosso do arrombamento até o retrato do
 * defensor — e as setas finas de alvo de mágica, que ligam pilha e alvo.
 */
export function TargetArrows({ view, plan = EMPTY_COMBAT_PLAN }: TargetArrowsProps) {
  const [arrows, setArrows] = useState<Arrow[]>([])

  const measure = useCallback(() => {
    const next = collectArrows(view, plan)
    setArrows((prev) => (signature(prev) === signature(next) ? prev : next))
  }, [view, plan])

  useEffect(() => {
    measure()
    // O tabuleiro ainda está animando quando a view chega; remedir por alguns
    // frames é o que impede a seta de apontar para onde a carta estava.
    let frame = 0
    const started = performance.now()
    const loop = (): void => {
      measure()
      if (performance.now() - started < SETTLE_MS) frame = requestAnimationFrame(loop)
    }
    frame = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(frame)
  }, [measure])

  useEffect(() => {
    const onResize = (): void => measure()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [measure])

  if (arrows.length === 0) return null

  return (
    <svg className="target-arrows" aria-hidden="true" focusable="false">
      <defs>
        {(Object.keys(TONE_COLOR) as ArrowTone[]).map((tone) => (
          <marker
            key={tone}
            id={`arrow-head-${tone}`}
            viewBox="0 0 10 10"
            refX="7"
            refY="5"
            markerWidth={tone === 'breach' ? 4 : 7}
            markerHeight={tone === 'breach' ? 4 : 7}
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill={TONE_COLOR[tone]} />
          </marker>
        ))}
      </defs>

      {arrows.map((arrow) => {
        const d =
          arrow.tone === 'breach'
            ? `M ${round(arrow.from.x)} ${round(arrow.from.y)} L ${round(arrow.to.x)} ${round(arrow.to.y)}`
            : curve(arrow.from, arrow.to)
        const color = TONE_COLOR[arrow.tone]
        const mid = { x: (arrow.from.x + arrow.to.x) / 2, y: (arrow.from.y + arrow.to.y) / 2 }
        return (
          <g key={arrow.key} className={`target-arrows__group target-arrows__group--${arrow.tone}`}>
            <path className="target-arrows__halo" d={d} stroke={color} />
            <path
              className="target-arrows__line"
              d={d}
              stroke={color}
              markerEnd={`url(#arrow-head-${arrow.tone})`}
            />
            {arrow.label === null ? (
              <circle
                className="target-arrows__origin"
                cx={arrow.from.x}
                cy={arrow.from.y}
                r={3.5}
                fill={color}
              />
            ) : (
              <>
                <circle className="target-arrows__chip" cx={mid.x} cy={mid.y} r={19} />
                <text className="target-arrows__label" x={mid.x} y={mid.y} fill={color}>
                  {arrow.label}
                </text>
              </>
            )}
          </g>
        )
      })}
    </svg>
  )
}

// ---------------------------------------------------------------------------
// Coleta
// ---------------------------------------------------------------------------

function collectArrows(view: GameView, plan: CombatPlan): Arrow[] {
  const arrows: Arrow[] = []

  // Um único vetor para todo o dano que passa, do arrombamento ao retrato.
  if (plan.active && plan.breachDamage > 0 && plan.defender !== null) {
    const source = anchorRect('[data-breach-anchor="true"]')
    const target = anchorRect(`[data-player-portrait="${plan.defender}"]`)
    if (source !== null && target !== null) {
      arrows.push({
        key: 'breach',
        tone: 'breach',
        label: `-${plan.breachDamage}`,
        ...trim(source, target),
      })
    }
  }

  for (const item of view.stack) {
    const source = anchorRect(`[data-stack-id="${item.id}"]`) ?? cardAnchor(item.sourceCard)
    if (source === null) continue
    for (const target of item.targets) {
      const to = cardAnchor(target)
      if (to !== null) {
        arrows.push({ key: `spl-${item.id}-c${target}`, tone: 'spell', label: null, ...trim(source, to) })
      }
    }
    for (const target of item.targetPlayers) {
      const to = playerAnchor(target)
      if (to !== null) {
        arrows.push({ key: `spl-${item.id}-p${target}`, tone: 'spell', label: null, ...trim(source, to) })
      }
    }
  }

  return arrows
}

interface Anchor {
  center: Point
  radius: number
}

function anchorRect(selector: string): Anchor | null {
  const element = document.querySelector(selector)
  if (element === null) return null
  const rect = element.getBoundingClientRect()
  if (rect.width === 0 && rect.height === 0) return null
  return {
    center: { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 },
    radius: Math.min(rect.width, rect.height) * EDGE_INSET,
  }
}

function cardAnchor(id: ObjectId): Anchor | null {
  return anchorRect(`[data-card-id="${id}"]`)
}

function playerAnchor(id: ObjectId): Anchor | null {
  return anchorRect(`[data-player-id="${id}"]`)
}

/** Encosta as pontas na borda de cada âncora em vez de no centro. */
function trim(from: Anchor, to: Anchor): { from: Point; to: Point } {
  const dx = to.center.x - from.center.x
  const dy = to.center.y - from.center.y
  const length = Math.hypot(dx, dy)
  if (length < 1) return { from: from.center, to: to.center }
  const ux = dx / length
  const uy = dy / length
  return {
    from: { x: from.center.x + ux * from.radius, y: from.center.y + uy * from.radius },
    to: { x: to.center.x - ux * to.radius, y: to.center.y - uy * to.radius },
  }
}

/** Bezier quadrática arqueada para a lateral: duas setas paralelas não colam. */
function curve(from: Point, to: Point): string {
  const dx = to.x - from.x
  const dy = to.y - from.y
  const length = Math.hypot(dx, dy) || 1
  const mx = (from.x + to.x) / 2
  const my = (from.y + to.y) / 2
  const bow = length * BOW
  const cx = mx + (-dy / length) * bow
  const cy = my + (dx / length) * bow
  return `M ${round(from.x)} ${round(from.y)} Q ${round(cx)} ${round(cy)} ${round(to.x)} ${round(to.y)}`
}

function round(value: number): number {
  return Math.round(value * 10) / 10
}

function signature(arrows: readonly Arrow[]): string {
  return arrows
    .map((a) => `${a.key}:${round(a.from.x)},${round(a.from.y)}>${round(a.to.x)},${round(a.to.y)}`)
    .join('|')
}
