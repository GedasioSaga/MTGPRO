import type { ObjectId, PlayerId } from '../types/protocol'
import type { FxPoint, FxRect } from './fxTypes'

/**
 * Contrato de ancoras entre a camada de efeitos e a mesa.
 *
 * A mesa marca os nos com estes atributos; a FX resolve a posicao por
 * `getBoundingClientRect`. Nada aqui importa componente de outro agente, entao
 * a mesa pode mudar de layout sem quebrar efeito nenhum. Toda ancora ausente
 * cai num palpite razoavel — a UI nunca fica com efeito no canto (0,0).
 */
export const FX_ATTR = {
  card: 'data-fx-card',
  hand: 'data-fx-hand',
  player: 'data-fx-player',
  stack: 'data-fx-stack',
  steps: 'data-fx-steps',
  shake: 'data-fx-shake',
} as const

/** Ultimo retangulo conhecido por carta: criatura que morre ja saiu do DOM. */
const rectMemory = new Map<ObjectId, FxRect>()

function toRect(el: Element): FxRect {
  const r = el.getBoundingClientRect()
  return { x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height }
}

function query(selector: string): FxRect | null {
  if (typeof document === 'undefined') return null
  const el = document.querySelector(selector)
  return el ? toRect(el) : null
}

function viewport(): { w: number; h: number } {
  if (typeof window === 'undefined') return { w: 1280, h: 800 }
  return { w: window.innerWidth, h: window.innerHeight }
}

/** Jogador 0 fica embaixo, 1 em cima — igual a qualquer cliente de card game. */
function isBottom(player: PlayerId): boolean {
  return player === 0
}

export function cardRect(id: ObjectId): FxRect {
  const found = query(`[${FX_ATTR.card}="${id}"]`)
  if (found) {
    rectMemory.set(id, found)
    return found
  }
  const remembered = rectMemory.get(id)
  if (remembered) return remembered
  const { w, h } = viewport()
  return { x: w * 0.5, y: h * 0.5, w: 120, h: 168 }
}

export function cardPoint(id: ObjectId): FxPoint {
  const r = cardRect(id)
  return { x: r.x, y: r.y }
}

export function handPoint(player: PlayerId): FxPoint {
  const found = query(`[${FX_ATTR.hand}="${player}"]`)
  if (found) return { x: found.x, y: found.y }
  const { w, h } = viewport()
  return { x: w * 0.5, y: isBottom(player) ? h * 0.9 : h * 0.1 }
}

export function playerPoint(player: PlayerId): FxPoint {
  const found = query(`[${FX_ATTR.player}="${player}"]`)
  if (found) return { x: found.x, y: found.y }
  const { w, h } = viewport()
  return { x: w * 0.08, y: isBottom(player) ? h * 0.78 : h * 0.22 }
}

export function stackPoint(): FxPoint {
  const found = query(`[${FX_ATTR.stack}]`)
  if (found) return { x: found.x, y: found.y }
  const { w, h } = viewport()
  return { x: w * 0.72, y: h * 0.5 }
}

export function stepsRect(): FxRect {
  const found = query(`[${FX_ATTR.steps}]`)
  if (found) return found
  const { w, h } = viewport()
  return { x: w * 0.5, y: h * 0.5, w: w * 0.06, h: h * 0.46 }
}

/** Alvo do tremor de tela. Cai no `#root` quando a mesa nao marca nada. */
export function shakeTarget(): HTMLElement | null {
  if (typeof document === 'undefined') return null
  const marked = document.querySelector(`[${FX_ATTR.shake}]`)
  if (marked instanceof HTMLElement) return marked
  const root = document.getElementById('root')
  return root ?? document.body
}

export function defenderPoint(defender: unknown): FxPoint {
  if (typeof defender === 'object' && defender !== null) {
    const record = defender as Record<string, unknown>
    if (typeof record.Player === 'number') return playerPoint(record.Player)
    if (typeof record.Planeswalker === 'number') return cardPoint(record.Planeswalker)
    if (typeof record.Battle === 'number') return cardPoint(record.Battle)
  }
  const { w, h } = viewport()
  return { x: w * 0.5, y: h * 0.18 }
}

/** Centro da tela — ancora dos efeitos que nao pertencem a nenhum no. */
export function viewportCenter(): FxPoint {
  const { w, h } = viewport()
  return { x: w / 2, y: h / 2 }
}

export function midpoint(a: FxPoint, b: FxPoint): FxPoint {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
}

export function distance(a: FxPoint, b: FxPoint): number {
  return Math.hypot(b.x - a.x, b.y - a.y)
}

/** Angulo em graus, pronto para `rotate()`. */
export function angleDeg(a: FxPoint, b: FxPoint): number {
  return (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI
}

export function forgetRects(): void {
  rectMemory.clear()
}
