import { clsx } from 'clsx'
import { useEffect } from 'react'
import type { ReactNode } from 'react'
import { IconButton } from '../ui/IconButton'
import { Panel } from '../ui/Panel'
import { useMatchStore } from '../../state/matchStore'
import { PHASE_LABEL, STEP_SEQUENCE } from '../../types/protocol'
import type { Phase } from '../../types/protocol'

const SPEED_STEPS = [
  { value: 0.5, label: '0.5×', key: '1' },
  { value: 1, label: '1×', key: '2' },
  { value: 2, label: '2×', key: '3' },
  { value: 4, label: '4×', key: '4' },
] as const

/** Teto de velocidade do store: e o mais perto de "instantaneo" que ha. */
const INSTANT_SPEED = 8

/**
 * Barra de transporte da partida: pausar, avancar um evento, escolher
 * velocidade, e ver onde o turno esta na sequencia de passos. Le e escreve
 * direto em `matchStore` — nao tem estado proprio alem dos atalhos.
 */
export function PlaybackBar() {
  const playing = useMatchStore((s) => s.playing)
  const speed = useMatchStore((s) => s.speed)
  const connection = useMatchStore((s) => s.connection)
  const view = useMatchStore((s) => s.view)
  const pause = useMatchStore((s) => s.pause)
  const resume = useMatchStore((s) => s.resume)
  const setSpeed = useMatchStore((s) => s.setSpeed)
  const step = useMatchStore((s) => s.step)

  const finished = connection === 'done'

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      const target = event.target
      const typing =
        target instanceof HTMLElement &&
        (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
      if (typing) return

      if (event.code === 'Space') {
        event.preventDefault()
        if (finished) return
        if (playing) pause()
        else resume()
        return
      }
      if (event.code === 'ArrowRight') {
        event.preventDefault()
        if (!finished) step()
        return
      }
      const found = SPEED_STEPS.find((s) => s.key === event.key)
      if (found) setSpeed(found.value)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [playing, finished, pause, resume, step, setSpeed])

  const stepIndex = view ? Math.max(0, STEP_SEQUENCE.indexOf(view.step)) : 0
  const progress = STEP_SEQUENCE.length <= 1 ? 0 : stepIndex / (STEP_SEQUENCE.length - 1)

  return (
    <Panel
      elevation="floating"
      material="metal"
      className="flex flex-col gap-2.5 rounded-xl px-4 py-3"
      aria-label="Controles de reprodução"
    >
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5">
          <IconButton
            label={playing ? 'Pausar' : 'Continuar'}
            icon={playing ? <PauseIcon /> : <PlayIcon />}
            tone="accent"
            disabled={finished}
            onClick={() => (playing ? pause() : resume())}
          />
          <IconButton
            label="Avançar um evento"
            icon={<StepIcon />}
            tone="ghost"
            disabled={finished}
            onClick={() => step()}
          />
        </div>

        <PhaseTrack turn={view?.turn ?? 0} stepLabel={view?.stepLabel ?? '—'} phase={view?.phase} progress={progress} />

        <div className="flex items-center gap-1 rounded-md bg-black/25 p-1" role="radiogroup" aria-label="Velocidade">
          {SPEED_STEPS.map((option) => (
            <SpeedButton
              key={option.value}
              label={option.label}
              active={speed === option.value}
              onClick={() => setSpeed(option.value)}
            />
          ))}
          <SpeedButton
            label={<BoltIcon className="size-3.5" />}
            title="Instantâneo"
            active={speed === INSTANT_SPEED}
            onClick={() => setSpeed(INSTANT_SPEED)}
          />
        </div>
      </div>

      <p className="caps text-hud flex items-center justify-center gap-3 text-ink-faint select-none">
        <span><Kbd>Espaço</Kbd> pausa</span>
        <span><Kbd>→</Kbd> avança</span>
        <span><Kbd>1–4</Kbd> velocidade</span>
      </p>
    </Panel>
  )
}

function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-[3px] border border-edge bg-white/5 px-1 py-0.5 font-sans text-[10px] tracking-normal text-ink normal-case">
      {children}
    </kbd>
  )
}

function PhaseTrack({
  turn,
  stepLabel,
  phase,
  progress,
}: {
  turn: number
  stepLabel: string
  phase: Phase | undefined
  progress: number
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2 text-[11.5px]">
        <span className="caps text-hud text-ink-muted">Turno {turn}</span>
        <span className="truncate text-ink-strong">
          {phase ? PHASE_LABEL[phase] : '—'} <span className="text-ink-faint">·</span> {stepLabel}
        </span>
      </div>
      <div className="metal-well relative h-1.5 w-full overflow-hidden rounded-full">
        <div
          className="h-full rounded-full bg-gradient-to-r from-accent-deep via-accent to-accent-bright transition-[width] duration-[var(--duration-slow)] ease-out"
          style={{ width: `${Math.round(progress * 100)}%` }}
        />
      </div>
    </div>
  )
}

function SpeedButton({
  label,
  active,
  title,
  onClick,
}: {
  label: ReactNode
  active: boolean
  title?: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      title={title}
      onClick={onClick}
      className={clsx(
        'min-w-8 cursor-pointer rounded-sm px-2 py-1 text-[12px] font-semibold tabular-nums',
        'transition-[color,background-color] duration-[var(--duration-micro)] ease-standard',
        active ? 'bg-accent text-ink-inverse' : 'text-ink-muted hover:bg-white/8 hover:text-ink-strong',
      )}
    >
      <span className="flex items-center justify-center">{label}</span>
    </button>
  )
}

function PlayIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M7 5.5v13l12-6.5-12-6.5Z" fill="currentColor" />
    </svg>
  )
}

function PauseIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="6.5" y="5" width="4" height="14" rx="1" fill="currentColor" />
      <rect x="13.5" y="5" width="4" height="14" rx="1" fill="currentColor" />
    </svg>
  )
}

function StepIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M6 5.5v13l9-6.5-9-6.5Z" fill="currentColor" />
      <rect x="16.5" y="5" width="2.2" height="14" rx="1" fill="currentColor" />
    </svg>
  )
}

function BoltIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <path d="M13 3 5 13.5h5.2L10.5 21 19 9.5h-5.4L13 3Z" fill="currentColor" />
    </svg>
  )
}
