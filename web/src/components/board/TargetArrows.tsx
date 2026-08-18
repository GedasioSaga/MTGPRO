import { useCallback, useEffect, useState } from 'react'
import { defenderObject, defenderPlayer } from '../../types/protocol'
import type { CardView, GameView, ObjectId } from '../../types/protocol'

type ArrowTone = 'attack' | 'block' | 'spell'

interface Point {
  x: number
  y: number
}

interface Arrow {
  key: string
  tone: ArrowTone
  from: Point
  to: Point
}

const TONE_COLOR: Record<ArrowTone, string> = {
  attack: '#f2685e',
  block: '#6f9cf5',
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
  cards: Record<ObjectId, CardView>
}

/**
 * Setas de combate e de alvo, ancoradas no DOM real.
 *
 * Ler quem ataca quem por anel de cor não escala: com quatro criaturas trocando
 * golpes a relação some. A seta é a única forma que sobrevive à bagunça — e por
 * isso ela mede posição de verdade (`getBoundingClientRect`), em vez de deduzir
 * do layout.
 */
export function TargetArrows({ view, cards }: TargetArrowsProps) {
  const [arrows, setArrows] = useState<Arrow[]>([])

  const measure = useCallback(() => {
    const next = collectArrows(view, cards)
    setArrows((prev) => (signature(prev) === signature(next) ? prev : next))
  }, [view, cards])

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
            refX="8"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill={TONE_COLOR[tone]} />
          </marker>
        ))}
      </defs>

      {arrows.map((arrow) => {
        const d = curve(arrow.from, arrow.to)
        const color = TONE_COLOR[arrow.tone]
        return (
          <g key={arrow.key}>
            <path className="target-arrows__halo" d={d} stroke={color} />
            <path
              className="target-arrows__line"
              d={d}
              stroke={color}
              markerEnd={`url(#arrow-head-${arrow.tone})`}
            />
            <circle className="target-arrows__origin" cx={arrow.from.x} cy={arrow.from.y} r={3.5} fill={color} />
          </g>
        )
      })}
    </svg>
  )
}

// ---------------------------------------------------------------------------
// Coleta
// ---------------------------------------------------------------------------

function collectArrows(view: GameView, cards: Record<ObjectId, CardView>): Arrow[] {
  const arrows: Arrow[] = []

  for (const card of Object.values(cards)) {
    if (card.zone !== 'Battlefield') continue

    if (card.attacking !== null) {
      const player = defenderPlayer(card.attacking)
      const object = defenderObject(card.attacking)
      const target =
        player !== null ? playerAnchor(player) : object !== null ? cardAnchor(object) : null
      const source = cardAnchor(card.id)
      if (source !== null && target !== null) {
        arrows.push({ key: `atk-${card.id}`, tone: 'attack', ...trim(source, target) })
      }
    }

    for (const blocked of card.blocking) {
      const source = cardAnchor(card.id)
      const target = cardAnchor(blocked)
      if (source !== null && target !== null) {
        arrows.push({ key: `blk-${card.id}-${blocked}`, tone: 'block', ...trim(source, target) })
      }
    }
  }

  for (const item of view.stack) {
    const source = anchorRect(`[data-stack-id="${item.id}"]`) ?? cardAnchor(item.sourceCard)
    if (source === null) continue
    for (const target of item.targets) {
      const to = cardAnchor(target)
      if (to !== null) {
        arrows.push({ key: `spl-${item.id}-c${target}`, tone: 'spell', ...trim(source, to) })
      }
    }
    for (const target of item.targetPlayers) {
      const to = playerAnchor(target)
      if (to !== null) {
        arrows.push({ key: `spl-${item.id}-p${target}`, tone: 'spell', ...trim(source, to) })
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
