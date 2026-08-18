import { AnimatePresence } from 'motion/react'
import './fx.css'
import { AttackLunge } from './AttackLunge'
import { BlockClash } from './BlockClash'
import { CastBeam } from './CastBeam'
import { CounterPop } from './CounterPop'
import { CounterShatter } from './CounterShatter'
import { DamageNumber } from './DamageNumber'
import { DeathDissolve } from './DeathDissolve'
import { GameOverOverlay } from './GameOverOverlay'
import { LifePulse } from './LifePulse'
import { ResolveBurst } from './ResolveBurst'
import { ScreenShake } from './ScreenShake'
import { StepPulse } from './StepPulse'
import { TokenSpawn } from './TokenSpawn'
import { TriggerFlash } from './TriggerFlash'
import { TurnBanner } from './TurnBanner'
import { Vignette } from './Vignette'
import type { FxEffect } from './fxTypes'
import { BUS_SHAKE_MS, useBusShakes } from './useBusShakes'
import { useEventChoreographer } from './useEventChoreographer'

/** Desenha um efeito. O `switch` e exaustivo — kind novo quebra a compilacao. */
export function FxEffectView({ effect }: { effect: FxEffect }) {
  switch (effect.kind) {
    case 'turnBanner':
      return <TurnBanner {...effect} />
    case 'stepPulse':
      return <StepPulse {...effect} />
    case 'castBeam':
      return <CastBeam {...effect} />
    case 'resolveBurst':
      return <ResolveBurst {...effect} />
    case 'counterShatter':
      return <CounterShatter {...effect} />
    case 'attackLunge':
      return <AttackLunge {...effect} />
    case 'blockClash':
      return <BlockClash {...effect} />
    case 'damageNumber':
      return <DamageNumber {...effect} />
    case 'lifePulse':
      return <LifePulse {...effect} />
    case 'deathDissolve':
      return <DeathDissolve {...effect} />
    case 'tokenSpawn':
      return <TokenSpawn {...effect} />
    case 'counterPop':
      return <CounterPop {...effect} />
    case 'triggerFlash':
      return <TriggerFlash {...effect} />
    case 'screenShake':
      return <ScreenShake intensity={effect.intensity} durationMs={effect.durationMs} />
    case 'vignette':
      return <Vignette {...effect} />
    case 'gameOver':
      return <GameOverOverlay {...effect} />
    default: {
      const unreachable: never = effect
      return unreachable
    }
  }
}

/**
 * Camada de efeitos da partida. Montar UMA vez, no topo do App: ela le a fila
 * de eventos do `matchStore` sozinha e nao recebe prop nenhuma.
 */
export function FxLayer() {
  const effects = useEventChoreographer()
  const busShakes = useBusShakes()

  return (
    <div className="fx-layer">
      <AnimatePresence>
        {effects.map((effect) => (
          <FxEffectView key={effect.id} effect={effect} />
        ))}
      </AnimatePresence>
      {busShakes.map((shake) => (
        <ScreenShake key={shake.id} intensity={shake.intensity} durationMs={BUS_SHAKE_MS} />
      ))}
    </div>
  )
}
