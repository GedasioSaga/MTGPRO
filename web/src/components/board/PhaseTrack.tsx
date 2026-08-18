import { motion, useReducedMotion } from 'motion/react'
import {
  PHASE_LABEL,
  PHASE_OF_STEP,
  STEP_LABEL,
  STEP_SEQUENCE,
  STEP_SHORT_LABEL,
} from '../../types/protocol'
import type { PlayerId, PlayerView, Step } from '../../types/protocol'
import { withAlpha } from '../../design/color'
import { seatAccent } from './boardVisuals'

export interface PhaseTrackProps {
  turn: number
  step: Step
  activePlayer: PlayerId
  players: PlayerView[]
}

/**
 * Trilha dos treze passos do turno (CR 500). Passo aceso é o de agora, o que
 * ficou para trás fica apagado: quem entra no meio da partida precisa saber
 * onde estamos sem ler texto nenhum.
 */
export function PhaseTrack({ turn, step, activePlayer, players }: PhaseTrackProps) {
  const reduceMotion = useReducedMotion()
  const accent = seatAccent(activePlayer)
  const current = STEP_SEQUENCE.indexOf(step)
  const activeName = players[activePlayer]?.name ?? `jogador ${activePlayer}`

  return (
    <div className="phase-track" data-fx-steps data-seat={activePlayer}>
      <div className="phase-track__turn">
        <span className="phase-track__turn-label">turno</span>
        <span className="phase-track__turn-number">{turn}</span>
        <span className="phase-track__turn-player" style={{ color: accent }}>
          {activeName}
        </span>
      </div>

      <ol className="phase-track__steps" aria-label="Passos do turno">
        {STEP_SEQUENCE.map((candidate, index) => {
          const phase = PHASE_OF_STEP[candidate]
          const opensPhase = index > 0 && PHASE_OF_STEP[STEP_SEQUENCE[index - 1]] !== phase
          const state = index < current ? 'done' : index === current ? 'now' : 'next'

          return (
            <li
              key={candidate}
              className="phase-step"
              data-state={state}
              data-phase-open={opensPhase ? 'true' : 'false'}
              title={`${PHASE_LABEL[phase]} — ${STEP_LABEL[candidate]}`}
              aria-current={state === 'now' ? 'step' : undefined}
              style={
                state === 'now'
                  ? { color: accent, borderColor: withAlpha(accent, 0.55) }
                  : undefined
              }
            >
              {state === 'now' ? (
                <motion.span
                  layoutId="phase-track-glow"
                  className="phase-step__glow"
                  style={{
                    background: `linear-gradient(180deg, ${withAlpha(accent, 0.32)}, ${withAlpha(accent, 0.06)})`,
                    boxShadow: `0 0 18px -4px ${withAlpha(accent, 0.8)}`,
                  }}
                  transition={
                    reduceMotion
                      ? { duration: 0 }
                      : { type: 'spring', stiffness: 420, damping: 36 }
                  }
                />
              ) : null}
              <span className="phase-step__label">{STEP_SHORT_LABEL[candidate]}</span>
            </li>
          )
        })}
      </ol>

      <div className="phase-track__now" style={{ color: accent }}>
        {STEP_LABEL[step]}
      </div>
    </div>
  )
}
