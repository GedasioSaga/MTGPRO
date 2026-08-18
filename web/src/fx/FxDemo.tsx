import { AnimatePresence } from 'motion/react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import './fx.css'
import { FxEffectView } from './FxLayer'
import { LifeTicker } from './LifeTicker'
import { ScreenShake } from './ScreenShake'
import { cardRect, handPoint, midpoint, playerPoint, stackPoint, stepsRect } from './fxAnchors'
import { colorSetHex, playerAccent } from './fxColors'
import type { FxSpec } from './fxTypes'
import { ParticleCanvas } from './particles/ParticleCanvas'
import { BUS_SHAKE_MS, useBusShakes } from './useBusShakes'
import { useFxEngine } from './useFxEngine'
import type { FxTranslateContext } from './translateEvent'

/** Ids sinteticos: a demo marca dois slots com eles para as ancoras resolverem. */
const CARD_A = 9001
const CARD_B = 9002

const BLUE = colorSetHex(1 << 1)
const RED = colorSetHex(1 << 3)
const GREEN = colorSetHex(1 << 4)

interface DemoScene {
  id: string
  label: string
  /** Espaco reservado na sequencia, ja com folga para o efeito respirar. */
  holdMs: number
  build: () => FxSpec[]
}

const SCENES: DemoScene[] = [
  {
    id: 'turnBanner',
    label: 'Turn banner',
    holdMs: 1100,
    build: () => [
      {
        kind: 'turnBanner',
        durationMs: 900,
        turn: 7,
        player: 0,
        playerName: 'Boros Aggro',
        accent: playerAccent(0),
      },
    ],
  },
  {
    id: 'stepPulse',
    label: 'Step pulse',
    holdMs: 520,
    build: () => [
      { kind: 'stepPulse', durationMs: 220, rect: stepsRect(), label: 'Declare Attackers' },
    ],
  },
  {
    id: 'castBeam',
    label: 'Cast beam',
    holdMs: 900,
    build: () => [
      {
        kind: 'castBeam',
        durationMs: 700,
        from: handPoint(0),
        to: stackPoint(),
        color: RED,
        name: 'Lightning Bolt',
      },
    ],
  },
  {
    id: 'resolveBurst',
    label: 'Resolve',
    holdMs: 700,
    build: () => [{ kind: 'resolveBurst', durationMs: 450, at: stackPoint(), color: RED }],
  },
  {
    id: 'counterShatter',
    label: 'Countered',
    holdMs: 900,
    build: () => [{ kind: 'counterShatter', durationMs: 650, at: stackPoint() }],
  },
  {
    id: 'triggerFlash',
    label: 'Trigger',
    holdMs: 850,
    build: () => [
      {
        kind: 'triggerFlash',
        durationMs: 600,
        rect: cardRect(CARD_A),
        text: 'When this creature attacks, draw a card.',
      },
    ],
  },
  {
    id: 'tokenSpawn',
    label: 'Token',
    holdMs: 650,
    build: () => [{ kind: 'tokenSpawn', durationMs: 420, rect: cardRect(CARD_B) }],
  },
  {
    id: 'counterPop',
    label: 'Counters',
    holdMs: 620,
    build: () => [
      { kind: 'counterPop', durationMs: 300, at: cardRect(CARD_A), label: '+1/+1', delta: 2 },
    ],
  },
  {
    id: 'attackLunge',
    label: 'Attack',
    holdMs: 900,
    build: () => [
      {
        kind: 'attackLunge',
        durationMs: 700,
        card: CARD_A,
        rect: cardRect(CARD_A),
        toward: playerPoint(1),
      },
      {
        kind: 'attackLunge',
        durationMs: 700,
        card: CARD_B,
        rect: cardRect(CARD_B),
        toward: playerPoint(1),
      },
    ],
  },
  {
    id: 'blockClash',
    label: 'Block',
    holdMs: 800,
    build: () => [
      { kind: 'blockClash', durationMs: 620, at: midpoint(cardRect(CARD_A), cardRect(CARD_B)) },
    ],
  },
  {
    id: 'damageNumber',
    label: 'Damage',
    holdMs: 800,
    build: () => [
      {
        kind: 'damageNumber',
        durationMs: 520,
        at: cardRect(CARD_A),
        amount: 3,
        tone: 'damage',
        lethal: false,
        subject: 'card',
      },
      {
        kind: 'damageNumber',
        durationMs: 520,
        at: cardRect(CARD_B),
        amount: 7,
        tone: 'damage',
        lethal: true,
        subject: 'card',
      },
    ],
  },
  {
    id: 'deathDissolve',
    label: 'Death',
    holdMs: 850,
    build: () => [
      { kind: 'deathDissolve', durationMs: 600, rect: cardRect(CARD_B), tone: 'death' },
    ],
  },
  {
    id: 'exileDissolve',
    label: 'Exile',
    holdMs: 750,
    build: () => [
      { kind: 'deathDissolve', durationMs: 480, rect: cardRect(CARD_A), tone: 'exile' },
    ],
  },
  {
    id: 'lifeGain',
    label: 'Life gain',
    holdMs: 700,
    build: () => [
      { kind: 'lifePulse', durationMs: 400, at: playerPoint(0), tone: 'life' },
      {
        kind: 'damageNumber',
        durationMs: 400,
        at: playerPoint(0),
        amount: 4,
        tone: 'life',
        lethal: false,
        subject: 'player',
      },
    ],
  },
  {
    id: 'gameOver',
    label: 'Game over',
    holdMs: 1600,
    build: () => [
      {
        kind: 'gameOver',
        durationMs: 1600,
        persistent: true,
        title: 'Boros Aggro wins',
        subtitle: 'Match complete.',
        turns: 11,
        scoreboard: [
          { name: 'Boros Aggro', life: 9, won: true },
          { name: 'Dimir Control', life: 0, won: false },
        ],
      },
      { kind: 'vignette', durationMs: 1600, persistent: true, tone: 'victory', intensity: 0.55 },
    ],
  },
]

/** Rajada: dispara muito mais que o teto para provar que a fila encurta. */
function burstSpecs(): FxSpec[] {
  const specs: FxSpec[] = []
  for (let i = 0; i < 8; i += 1) {
    const rect = i % 2 === 0 ? cardRect(CARD_A) : cardRect(CARD_B)
    specs.push({
      kind: 'damageNumber',
      durationMs: 520,
      at: { x: rect.x + (i - 4) * 26, y: rect.y - i * 8 },
      amount: 1 + i,
      tone: 'damage',
      lethal: i === 7,
      subject: 'card',
    })
    specs.push({ kind: 'blockClash', durationMs: 620, at: { x: rect.x, y: rect.y + i * 6 } })
    specs.push({
      kind: 'castBeam',
      durationMs: 700,
      from: handPoint(i % 2),
      to: stackPoint(),
      color: i % 2 === 0 ? GREEN : BLUE,
      name: 'Burst ' + String(i + 1),
    })
  }
  return specs
}

/**
 * Bancada visual da camada de efeitos: dispara tudo com dados sinteticos sobre
 * uma mesa de mentira que carrega as ancoras reais (`data-fx-*`), entao cada
 * efeito cai onde cairia numa partida. Monte esta tela OU o `FxLayer`, nunca os
 * dois — sao duas camadas de overlay concorrentes.
 */
export function FxDemo() {
  const ctx = useMemo<FxTranslateContext>(() => ({ view: null, card: () => undefined }), [])
  const { effects, push, clear } = useFxEngine(ctx)
  const busShakes = useBusShakes()
  const [life, setLife] = useState(20)
  const timers = useRef<number[]>([])

  const cancelPending = useCallback(() => {
    for (const timer of timers.current) window.clearTimeout(timer)
    timers.current = []
  }, [])

  useEffect(() => cancelPending, [cancelPending])

  const playAll = useCallback(() => {
    cancelPending()
    clear()
    let delay = 0
    for (const scene of SCENES) {
      timers.current.push(window.setTimeout(() => push(scene.build()), delay))
      delay += scene.holdMs
    }
  }, [cancelPending, clear, push])

  const takeHit = useCallback(
    (amount: number) => {
      setLife((current) => Math.max(0, current - amount))
      push([
        {
          kind: 'damageNumber',
          durationMs: 520,
          at: playerPoint(0),
          amount,
          tone: 'damage',
          lethal: false,
          subject: 'player',
        },
        { kind: 'vignette', durationMs: 520, tone: 'damage', intensity: Math.min(amount / 10, 0.7) },
      ])
    },
    [push],
  )

  return (
    <div className="fx-demo">
      <header className="fx-demo__head">
        <h1 className="fx-demo__title">Effects bench</h1>
        <p className="fx-demo__hint">
          Synthetic events on a mock table. The anchors are the real ones, so every effect lands
          where it would land in a match.
        </p>
      </header>

      <div className="fx-demo__stage" data-fx-shake>
        <div className="fx-demo__rail" data-fx-steps>
          <span>UNTAP</span>
          <span>UPKEEP</span>
          <span>DRAW</span>
          <span>MAIN 1</span>
          <span>ATTACK</span>
          <span>BLOCK</span>
          <span>DAMAGE</span>
          <span>MAIN 2</span>
          <span>END</span>
        </div>

        <div className="fx-demo__plate fx-demo__plate--top" data-fx-player="1">
          <span className="fx-demo__seat">Dimir Control</span>
          <span className="fx-demo__life">14</span>
        </div>
        <div className="fx-demo__hand fx-demo__hand--top" data-fx-hand="1" />

        <div className="fx-demo__stack" data-fx-stack>
          <span className="fx-demo__seat">Stack</span>
        </div>

        <div className="fx-demo__row">
          <div className="fx-demo__slot" data-fx-card={CARD_A}>
            <span>Boros Reckoner</span>
            <span className="fx-demo__pt">3/3</span>
          </div>
          <div className="fx-demo__slot" data-fx-card={CARD_B}>
            <span>Serra Angel</span>
            <span className="fx-demo__pt">4/4</span>
          </div>
        </div>

        <div className="fx-demo__hand" data-fx-hand="0" />
        <div className="fx-demo__plate fx-demo__plate--bottom" data-fx-player="0">
          <span className="fx-demo__seat">Boros Aggro</span>
          <LifeTicker value={life} className="fx-demo__life" />
        </div>
      </div>

      <div className="fx-demo__bar">
        <button type="button" className="fx-demo__btn fx-demo__btn--primary" onClick={playAll}>
          Play sequence
        </button>
        <button
          type="button"
          className="fx-demo__btn"
          onClick={() => {
            cancelPending()
            push(burstSpecs())
          }}
        >
          Burst (24)
        </button>
        <button
          type="button"
          className="fx-demo__btn"
          onClick={() => {
            cancelPending()
            clear()
            setLife(20)
          }}
        >
          Reset
        </button>
        <span className="fx-demo__sep" />
        <button type="button" className="fx-demo__btn" onClick={() => takeHit(1)}>
          Take 1
        </button>
        <button type="button" className="fx-demo__btn" onClick={() => takeHit(7)}>
          Take 7
        </button>
        <button type="button" className="fx-demo__btn" onClick={() => setLife((v) => v + 5)}>
          Gain 5
        </button>
      </div>

      <div className="fx-demo__bar fx-demo__bar--wrap">
        {SCENES.map((scene) => (
          <button
            key={scene.id}
            type="button"
            className="fx-demo__btn fx-demo__btn--ghost"
            onClick={() => push(scene.build())}
          >
            {scene.label}
          </button>
        ))}
      </div>

      <div className="fx-layer">
        <AnimatePresence>
          {effects.map((effect) => (
            <FxEffectView key={effect.id} effect={effect} />
          ))}
        </AnimatePresence>
        {busShakes.map((shake) => (
          <ScreenShake key={shake.id} intensity={shake.intensity} durationMs={BUS_SHAKE_MS} />
        ))}
        <ParticleCanvas />
      </div>
    </div>
  )
}
