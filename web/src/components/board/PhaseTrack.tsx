import type { CSSProperties } from 'react'
import { STEP_LABEL } from '../../types/protocol'
import type { PlayerId, PlayerView, Step } from '../../types/protocol'
import { seatAccent } from '../../design/tokens'
import { withAlpha } from '../../design/color'
import { cssVars } from './boardVisuals'
import './phaseTrack.css'

/**
 * Os sete marcos que um jogador de Magic realmente narra em voz alta. O motor
 * continua com os treze passos de CR 500 — o que colapsa aqui e a APRESENTACAO,
 * porque "1ST D…" e "END G…" sao vocabulario de ferramenta de replay, nao de
 * partida.
 */
type Milestone = 'Untap' | 'Upkeep' | 'Draw' | 'Main1' | 'Combat' | 'Main2' | 'End'

interface MilestoneSpec {
  readonly id: Milestone
  /** Rotulo padrao. */
  readonly long: string
  /** Usado quando a regua estreita; encolher e sempre melhor que truncar. */
  readonly short: string
}

const MILESTONES: readonly MilestoneSpec[] = [
  { id: 'Untap', long: 'UNTAP', short: 'UNT' },
  { id: 'Upkeep', long: 'UPKEEP', short: 'UPK' },
  { id: 'Draw', long: 'DRAW', short: 'DRW' },
  { id: 'Main1', long: 'MAIN 1', short: 'M1' },
  { id: 'Combat', long: 'COMBAT', short: 'CMB' },
  { id: 'Main2', long: 'MAIN 2', short: 'M2' },
  { id: 'End', long: 'END', short: 'END' },
]

/** Os treze passos do motor caem nos sete marcos. Combate inteiro vira um so. */
const MILESTONE_OF_STEP: Readonly<Record<Step, Milestone>> = {
  Untap: 'Untap',
  Upkeep: 'Upkeep',
  Draw: 'Draw',
  PrecombatMain: 'Main1',
  BeginCombat: 'Combat',
  DeclareAttackers: 'Combat',
  DeclareBlockers: 'Combat',
  FirstStrikeDamage: 'Combat',
  CombatDamage: 'Combat',
  EndCombat: 'Combat',
  PostcombatMain: 'Main2',
  End: 'End',
  Cleanup: 'End',
}

interface CombatSubSpec {
  readonly id: string
  readonly label: string
  readonly title: string
}

/** Tres batidas do combate. Sempre tres letras: largura constante, zero salto. */
const COMBAT_SUB: readonly CombatSubSpec[] = [
  { id: 'atk', label: 'ATK', title: 'Declarar atacantes' },
  { id: 'blk', label: 'BLK', title: 'Declarar bloqueadores' },
  { id: 'dmg', label: 'DMG', title: 'Dano de combate' },
]

/**
 * Posicao do passo atual dentro do combate: -1 antes da primeira batida,
 * 3 depois da ultima. Fora do combate o valor nao e lido.
 */
function combatSubIndex(step: Step): number {
  switch (step) {
    case 'DeclareAttackers':
      return 0
    case 'DeclareBlockers':
      return 1
    case 'FirstStrikeDamage':
    case 'CombatDamage':
      return 2
    case 'EndCombat':
      return 3
    default:
      return -1
  }
}

type MarkState = 'done' | 'now' | 'next'

function markState(index: number, current: number): MarkState {
  if (index < current) return 'done'
  if (index === current) return 'now'
  return 'next'
}

export interface PhaseTrackProps {
  turn: number
  step: Step
  activePlayer: PlayerId
  /** So alimenta o texto de acessibilidade; a regua funciona sem ele. */
  players?: readonly PlayerView[]
  /** Quem monta decide a posicao — a regua nao se posiciona sozinha. */
  className?: string
}

/**
 * Regua de fases da costura central: uma faixa de metal escovado embutida na
 * mesa, com os marcos gravados nela. Autocontida — sem `fixed`, sem margem
 * magica, sem saber onde foi montada.
 */
export function PhaseTrack({
  turn,
  step,
  activePlayer,
  players,
  className,
}: PhaseTrackProps) {
  const swatch = seatAccent[activePlayer % seatAccent.length]
  const currentMilestone = MILESTONE_OF_STEP[step]
  const currentIndex = MILESTONES.findIndex((mark) => mark.id === currentMilestone)
  const inCombat = currentMilestone === 'Combat'
  const subIndex = combatSubIndex(step)
  const activeName = players?.[activePlayer]?.name ?? `jogador ${activePlayer + 1}`

  const style: CSSProperties = cssVars({
    '--rail-accent': swatch.core,
    '--rail-accent-bright': swatch.bright,
    '--rail-accent-deep': swatch.deep,
    '--rail-accent-wash': swatch.bg,
    '--rail-accent-glow': withAlpha(swatch.core, 0.55),
    '--rail-accent-veil': withAlpha(swatch.core, 0.14),
  })

  return (
    <div
      className={className ? `phase-rail ${className}` : 'phase-rail'}
      data-fx-steps
      data-seat={activePlayer}
      style={style}
    >
      <div className="phase-rail__turn" title={`Turno ${turn} — vez de ${activeName}`}>
        <span className="phase-rail__turn-word">turno</span>
        <span className="phase-rail__turn-number">{turn}</span>
        {/* A seta aponta para o lado de quem joga: P0 embaixo, P1 em cima. */}
        <span className="phase-rail__turn-arrow" aria-hidden="true" />
      </div>

      <ol
        className="phase-rail__marks"
        aria-label={`Turno ${turn}, ${STEP_LABEL[step]}, vez de ${activeName}`}
      >
        {MILESTONES.map((mark, index) => {
          const state = markState(index, currentIndex)
          const isCombat = mark.id === 'Combat'

          return (
            <li
              key={mark.id}
              className="phase-rail__mark"
              data-mark={mark.id}
              data-state={state}
              aria-current={state === 'now' ? 'step' : undefined}
              title={isCombat && inCombat ? STEP_LABEL[step] : undefined}
            >
              <span className="phase-rail__lit" aria-hidden="true" />
              <span className="phase-rail__name">
                <span className="phase-rail__name-long">{mark.long}</span>
                <span className="phase-rail__name-short" aria-hidden="true">
                  {mark.short}
                </span>
              </span>

              {isCombat ? (
                <span className="phase-rail__combat" data-open={inCombat}>
                  {COMBAT_SUB.map((sub, subPosition) => (
                    <span
                      key={sub.id}
                      className="phase-rail__sub"
                      data-state={
                        inCombat ? markState(subPosition, subIndex) : 'idle'
                      }
                      title={sub.title}
                    >
                      {sub.label}
                    </span>
                  ))}
                </span>
              ) : null}
            </li>
          )
        })}
      </ol>
    </div>
  )
}

// Montar na costura central do tabuleiro, entre os dois playmats, ocupando a largura do feltro.
