import type { CardView, GameView, MatchEvent, ObjectId, PlayerId } from '../types/protocol'
import {
  cardPoint,
  cardRect,
  defenderPoint,
  handPoint,
  midpoint,
  playerPoint,
  stackPoint,
  stepsRect,
} from './fxAnchors'
import { colorSetHex, playerAccent } from './fxColors'
import { clamp } from './fxMotion'
import type { FxSpec } from './fxTypes'

export interface FxTranslateContext {
  view: GameView | null
  card: (id: ObjectId) => CardView | undefined
}

/**
 * O contrato deixa `MatchEvent` como `{ type: string } & Record<string, unknown>`
 * ate a uniao discriminada existir. Ler campo por campo com guarda mantem este
 * arquivo correto nas duas formas, sem `any` e sem quebrar quando o tipo apertar.
 */
type RawEvent = Record<string, unknown>

function num(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function str(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function bool(value: unknown): boolean {
  return value === true
}

function list(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

/** Tuplas do serde chegam como array: `[ObjectId, X]`. */
function tuple(value: unknown): [unknown, unknown] | null {
  return Array.isArray(value) && value.length >= 2 ? [value[0], value[1]] : null
}

/** Teto por evento — 12 bloqueadores nao viram 12 faiscas simultaneas. */
const FAN_OUT_CAP = 6

function spellColor(ctx: FxTranslateContext, id: ObjectId): string {
  const card = ctx.card(id)
  return card ? colorSetHex(card.colors) : colorSetHex(0)
}

function spellName(ctx: FxTranslateContext, id: ObjectId): string {
  return ctx.card(id)?.name ?? 'Spell'
}

function playerLabel(ctx: FxTranslateContext, id: PlayerId): string {
  return ctx.view?.players[id]?.name ?? `Player ${id + 1}`
}

function outcomeWinner(outcome: unknown): PlayerId | 'draw' | null {
  if (outcome === 'Draw') return 'draw'
  if (typeof outcome === 'object' && outcome !== null) {
    const winner = (outcome as Record<string, unknown>).Winner
    if (typeof winner === 'number') return winner
  }
  return null
}

function gameOverSpec(ctx: FxTranslateContext, outcome: unknown, duration: number): FxSpec[] {
  const winner = outcomeWinner(outcome)
  const decided = typeof winner === 'number'
  const players = ctx.view?.players ?? []
  const scoreboard = players.map((p) => ({
    name: p.name,
    life: p.life,
    won: decided && p.id === winner,
  }))
  const title = decided ? `${playerLabel(ctx, winner)} wins` : 'Draw'
  const subtitle = decided ? 'Match complete.' : 'Neither player could close it out.'
  return [
    {
      kind: 'gameOver',
      durationMs: duration,
      persistent: true,
      title,
      subtitle,
      scoreboard,
      turns: ctx.view?.turn ?? 0,
    },
    {
      kind: 'vignette',
      durationMs: duration,
      tone: decided ? 'victory' : 'defeat',
      intensity: 0.55,
    },
  ]
}

/** Duracao sugerida pelo motor, espelhando `MatchEvent::suggested_duration_ms`. */
export function suggestedDurationMs(type: string): number {
  switch (type) {
    case 'turnStart':
      return 900
    case 'stepChange':
      return 220
    case 'spellCast':
      return 700
    case 'spellResolved':
      return 450
    case 'spellCountered':
      return 650
    case 'abilityTriggered':
      return 600
    case 'cardMoved':
    case 'cardDrawn':
      return 380
    case 'attackersDeclared':
      return 800
    case 'blockersDeclared':
      return 700
    case 'damageDealt':
    case 'damageToPlayer':
      return 520
    case 'lifeChanged':
      return 400
    case 'died':
    case 'destroyed':
      return 600
    case 'gameOver':
      return 1600
    default:
      return 260
  }
}

/**
 * Traduz um evento do motor no estado visual efemero que a camada desenha.
 * Retorna vazio para eventos que a mesa ja anima sozinha (carta se movendo,
 * permanente virando) — duplicar ali so polui a tela.
 */
export function translateEvent(event: MatchEvent, ctx: FxTranslateContext): FxSpec[] {
  const raw = event as unknown as RawEvent
  const type = str(raw.type)
  const d = suggestedDurationMs(type)

  switch (type) {
    case 'turnStart': {
      const player = num(raw.player)
      return [
        {
          kind: 'turnBanner',
          durationMs: d,
          turn: num(raw.turn, 1),
          player,
          playerName: playerLabel(ctx, player),
          accent: playerAccent(player),
        },
      ]
    }

    case 'stepChange':
      return [
        { kind: 'stepPulse', durationMs: d, rect: stepsRect(), label: str(raw.label, 'Step') },
      ]

    case 'spellCast': {
      const card = num(raw.card)
      return [
        {
          kind: 'castBeam',
          durationMs: d,
          from: handPoint(num(raw.player)),
          to: stackPoint(),
          color: spellColor(ctx, card),
          name: spellName(ctx, card),
        },
      ]
    }

    case 'spellResolved':
      return [
        {
          kind: 'resolveBurst',
          durationMs: d,
          at: stackPoint(),
          color: spellColor(ctx, num(raw.card)),
        },
      ]

    case 'spellCountered':
      return [{ kind: 'counterShatter', durationMs: d, at: stackPoint() }]

    case 'abilityTriggered':
      return [
        {
          kind: 'triggerFlash',
          durationMs: d,
          rect: cardRect(num(raw.source)),
          text: str(raw.text, 'Triggered ability'),
        },
      ]

    case 'attackersDeclared':
      return list(raw.attackers)
        .slice(0, FAN_OUT_CAP)
        .flatMap((entry): FxSpec[] => {
          const pair = tuple(entry)
          if (!pair) return []
          return [
            {
              kind: 'attackLunge',
              durationMs: d,
              card: num(pair[0]),
              rect: cardRect(num(pair[0])),
              toward: defenderPoint(pair[1]),
            },
          ]
        })

    case 'blockersDeclared':
      return list(raw.blocks)
        .flatMap((entry): FxSpec[] => {
          const pair = tuple(entry)
          if (!pair) return []
          const attacker = cardPoint(num(pair[0]))
          return list(pair[1]).map(
            (blockerId): FxSpec => ({
              kind: 'blockClash',
              durationMs: d,
              at: midpoint(attacker, cardPoint(num(blockerId))),
            }),
          )
        })
        .slice(0, FAN_OUT_CAP)

    case 'damageDealt': {
      const amount = num(raw.amount)
      if (amount <= 0) return []
      const target = cardPoint(num(raw.target))
      return [
        {
          kind: 'damageNumber',
          durationMs: d,
          at: { x: target.x, y: target.y - 12 },
          amount,
          tone: 'damage',
          lethal: bool(raw.lethal),
        },
      ]
    }

    case 'damageToPlayer': {
      const amount = num(raw.amount)
      if (amount <= 0) return []
      const at = playerPoint(num(raw.player))
      const specs: FxSpec[] = [
        { kind: 'damageNumber', durationMs: d, at, amount, tone: 'damage', lethal: false },
        {
          kind: 'vignette',
          durationMs: d,
          tone: 'damage',
          intensity: clamp(amount / 10, 0.15, 0.7),
        },
      ]
      // Tremor so quando o golpe importa; senao a tela treme a partida inteira.
      if (amount >= 3) {
        specs.push({ kind: 'screenShake', durationMs: 260, intensity: clamp(amount / 12, 0.2, 1) })
      }
      return specs
    }

    case 'lifeChanged': {
      const delta = num(raw.delta)
      if (delta === 0) return []
      const at = playerPoint(num(raw.player))
      const specs: FxSpec[] = [
        { kind: 'lifePulse', durationMs: d, at, tone: delta > 0 ? 'life' : 'damage' },
      ]
      // Perda de vida ja ganhou numero em `damageToPlayer`; o ganho nao tem dono.
      if (delta > 0) {
        specs.push({
          kind: 'damageNumber',
          durationMs: d,
          at,
          amount: delta,
          tone: 'life',
          lethal: false,
        })
      }
      return specs
    }

    case 'countersChanged': {
      const delta = num(raw.delta)
      if (delta === 0) return []
      return [
        {
          kind: 'counterPop',
          durationMs: d,
          at: cardPoint(num(raw.card)),
          label: str(raw.kind, 'counter'),
          delta,
        },
      ]
    }

    case 'tokenCreated':
      return [{ kind: 'tokenSpawn', durationMs: 420, rect: cardRect(num(raw.card)) }]

    case 'died':
    case 'destroyed':
      return [{ kind: 'deathDissolve', durationMs: d, rect: cardRect(num(raw.card)), tone: 'death' }]

    case 'exiled':
      return [{ kind: 'deathDissolve', durationMs: 480, rect: cardRect(num(raw.card)), tone: 'exile' }]

    case 'playerLost':
      return [{ kind: 'vignette', durationMs: 700, tone: 'defeat', intensity: 0.5 }]

    case 'gameOver':
      return gameOverSpec(ctx, raw.outcome, d)

    default:
      return []
  }
}
