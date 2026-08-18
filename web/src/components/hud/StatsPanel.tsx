import { clsx } from 'clsx'
import { motion, useReducedMotion } from 'motion/react'
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { RefObject } from 'react'
import { Divider } from '../ui/Divider'
import { Panel } from '../ui/Panel'
import { seatAccent } from '../../design/tokens'
import { useMatchStore } from '../../state/matchStore'
import type { GameView, MatchEvent, PlayerId } from '../../types/protocol'

/** Vida dos dois jogadores num instante da partida. */
interface LifeSample {
  turn: number
  life: [number, number]
}

/** Dano que cada jogador causou ao oponente dentro de um turno. */
interface TurnDamage {
  turn: number
  dealt: [number, number]
}

interface MatchStats {
  samples: LifeSample[]
  damage: TurnDamage[]
}

const EMPTY_STATS: MatchStats = { samples: [], damage: [] }

const SPARK_HEIGHT = 92
const SPARK_PAD = 8
const REFERENCE_LIFE = 20
const VISIBLE_TURNS = 6

/**
 * Painel de leitura da partida: mostra POR QUE um lado está ganhando, e não só
 * que está. O store guarda a `GameView` corrente mas não o passado, então o
 * histórico de vida e o dano por turno são acumulados aqui, a partir dos
 * `MatchEvent` que passam pela fila.
 */
export function StatsPanel({ className }: { className?: string }) {
  const view = useMatchStore((s) => s.view)
  const stats = useMatchStats(view)
  const reduceMotion = useReducedMotion()

  const names = playerNames(view)
  const lives = currentLives(view, stats.samples)
  const totals = damageTotals(stats.damage)

  return (
    <motion.div
      initial={reduceMotion ? false : { opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={reduceMotion ? undefined : { duration: 0.42, ease: [0.16, 1, 0.3, 1] }}
      className={className}
    >
      <Panel
        elevation="floating"
        material="metal"
        className="flex w-full flex-col gap-3 rounded-xl px-4 py-3.5"
        aria-label="Estatísticas da partida"
      >
        <header className="flex items-baseline justify-between gap-2">
          <h2 className="caps text-[13px] tracking-caps text-ink-strong">Leitura da partida</h2>
          <span className="caps text-hud text-ink-faint">Turno {view?.turn ?? 0}</span>
        </header>

        <LifeChart samples={stats.samples} names={names} lives={lives} />

        <Divider />

        <div className="flex flex-col gap-2.5">
          <VersusRow label="Vida" values={lives} names={names} />
          <VersusRow label="Mão" values={handCounts(view)} names={names} />
          <VersusRow label="Permanentes" values={permanentCounts(view)} names={names} />
          <VersusRow label="Dano causado" values={totals} names={names} />
        </div>

        <Divider label="Dano por turno" />

        <DamageByTurn damage={stats.damage} names={names} />
      </Panel>
    </motion.div>
  )
}

// ---------------------------------------------------------------------------
// Acúmulo de histórico
// ---------------------------------------------------------------------------

/**
 * Escuta a fila de eventos direto no store (não pela seleção de estado) porque
 * cada evento precisa ser contado exatamente uma vez: um `useEffect` sobre
 * `currentEvent` perderia eventos consumidos no mesmo tick de render.
 */
function useMatchStats(view: GameView | null): MatchStats {
  const [stats, setStats] = useState<MatchStats>(EMPTY_STATS)
  const seed = useMatchStore((s) => s.seed)

  useEffect(() => {
    setStats(EMPTY_STATS)
  }, [seed])

  useEffect(() => {
    return useMatchStore.subscribe((state, prev) => {
      const event = state.currentEvent
      if (event === null || event === prev.currentEvent) return
      const turn = state.view?.turn ?? 0
      setStats((current) => reduceStats(current, event, turn))
    })
  }, [])

  // Linha de base: sem ela a curva começaria no primeiro dano, e não nos 20.
  useEffect(() => {
    if (view === null || view.players.length < 2) return
    setStats((current) =>
      current.samples.length > 0
        ? current
        : {
            ...current,
            samples: [{ turn: view.turn, life: [view.players[0].life, view.players[1].life] }],
          },
    )
  }, [view])

  return stats
}

function reduceStats(current: MatchStats, event: MatchEvent, turn: number): MatchStats {
  switch (event.type) {
    case 'lifeChanged': {
      if (!isSeat(event.player)) return current
      const last = current.samples[current.samples.length - 1]
      const life: [number, number] = last ? [last.life[0], last.life[1]] : [event.total, event.total]
      life[event.player] = event.total
      return { ...current, samples: [...current.samples, { turn, life }] }
    }

    case 'damageToPlayer': {
      if (!isSeat(event.player)) return current
      // Duelo: quem levou dano define quem o causou.
      const dealer = event.player === 0 ? 1 : 0
      const damage = current.damage.slice()
      const index = damage.findIndex((entry) => entry.turn === turn)
      if (index === -1) {
        const dealt: [number, number] = [0, 0]
        dealt[dealer] = event.amount
        damage.push({ turn, dealt })
      } else {
        const dealt: [number, number] = [damage[index].dealt[0], damage[index].dealt[1]]
        dealt[dealer] += event.amount
        damage[index] = { turn, dealt }
      }
      return { ...current, damage }
    }

    default:
      return current
  }
}

function isSeat(player: PlayerId): player is 0 | 1 {
  return player === 0 || player === 1
}

// ---------------------------------------------------------------------------
// Leitura da view
// ---------------------------------------------------------------------------

function playerNames(view: GameView | null): [string, string] {
  return [view?.players[0]?.name ?? 'Jogador I', view?.players[1]?.name ?? 'Jogador II']
}

function currentLives(view: GameView | null, samples: LifeSample[]): [number, number] {
  if (view && view.players.length >= 2) return [view.players[0].life, view.players[1].life]
  const last = samples[samples.length - 1]
  return last ? last.life : [REFERENCE_LIFE, REFERENCE_LIFE]
}

function handCounts(view: GameView | null): [number, number] {
  return [view?.players[0]?.handCount ?? 0, view?.players[1]?.handCount ?? 0]
}

function permanentCounts(view: GameView | null): [number, number] {
  return [view?.battlefield[0]?.length ?? 0, view?.battlefield[1]?.length ?? 0]
}

function damageTotals(damage: TurnDamage[]): [number, number] {
  return damage.reduce<[number, number]>(
    (acc, entry) => [acc[0] + entry.dealt[0], acc[1] + entry.dealt[1]],
    [0, 0],
  )
}

// ---------------------------------------------------------------------------
// Sparkline
// ---------------------------------------------------------------------------

/** Largura real do elemento: o SVG é desenhado em pixels, sem escalar traço. */
function useElementWidth(): [RefObject<HTMLDivElement | null>, number] {
  const ref = useRef<HTMLDivElement | null>(null)
  const [width, setWidth] = useState(0)

  useLayoutEffect(() => {
    const node = ref.current
    if (node === null) return
    setWidth(node.clientWidth)
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setWidth(entry.contentRect.width)
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  return [ref, width]
}

function LifeChart({
  samples,
  names,
  lives,
}: {
  samples: LifeSample[]
  names: [string, string]
  lives: [number, number]
}) {
  const [ref, width] = useElementWidth()
  const ceiling = Math.max(
    REFERENCE_LIFE,
    ...samples.map((sample) => Math.max(sample.life[0], sample.life[1])),
  )

  const toY = (life: number): number =>
    SPARK_PAD + (1 - clamp01(life / ceiling)) * (SPARK_HEIGHT - SPARK_PAD * 2)

  const toX = (index: number): number => {
    if (samples.length <= 1) return width
    return (index / (samples.length - 1)) * width
  }

  return (
    <div ref={ref} className="metal-well relative overflow-hidden rounded-lg px-0 py-0">
      <svg
        width={width}
        height={SPARK_HEIGHT}
        viewBox={`0 0 ${Math.max(width, 1)} ${SPARK_HEIGHT}`}
        role="img"
        aria-label={`Vida ao longo da partida: ${names[0]} com ${lives[0]}, ${names[1]} com ${lives[1]}`}
        className="block"
      >
        <defs>
          {[0, 1].map((seat) => (
            <linearGradient key={seat} id={`life-fill-${seat}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={seatAccent[seat].core} stopOpacity="0.28" />
              <stop offset="100%" stopColor={seatAccent[seat].core} stopOpacity="0" />
            </linearGradient>
          ))}
        </defs>

        {/* Referência: a linha dos 20 e a linha da morte. */}
        <line
          x1={0}
          x2={width}
          y1={toY(REFERENCE_LIFE)}
          y2={toY(REFERENCE_LIFE)}
          stroke="rgba(255,255,255,0.09)"
          strokeDasharray="3 5"
        />
        <line
          x1={0}
          x2={width}
          y1={toY(0)}
          y2={toY(0)}
          stroke="rgba(240,82,77,0.35)"
          strokeDasharray="3 5"
        />

        {width > 0 && samples.length > 0
          ? [0, 1].map((seat) => {
              const points = samples.map((sample, index) => ({
                x: toX(index),
                y: toY(sample.life[seat]),
              }))
              const line = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(' ')
              const area = `M0 ${toY(0).toFixed(1)} ${points
                .map((p) => `L${p.x.toFixed(1)} ${p.y.toFixed(1)}`)
                .join(' ')} L${width.toFixed(1)} ${toY(0).toFixed(1)} Z`
              const last = points[points.length - 1]
              return (
                <g key={seat}>
                  <path d={area} fill={`url(#life-fill-${seat})`} />
                  <path
                    d={line}
                    fill="none"
                    stroke={seatAccent[seat].core}
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  <circle cx={last.x} cy={last.y} r={3} fill={seatAccent[seat].bright} />
                </g>
              )
            })
          : null}
      </svg>

      <div className="flex items-center justify-between gap-2 px-3 pb-2.5">
        {[0, 1].map((seat) => (
          <div key={seat} className={clsx('flex min-w-0 items-center gap-2', seat === 1 && 'flex-row-reverse')}>
            <span
              aria-hidden="true"
              className="size-2 shrink-0 rounded-full"
              style={{ backgroundColor: seatAccent[seat].core }}
            />
            <span className="truncate text-[12px] text-ink-muted">{names[seat]}</span>
            <span className="numeral text-[18px] leading-none">{lives[seat]}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Comparativos
// ---------------------------------------------------------------------------

/** Duas barras espelhadas a partir do centro: a vantagem se lê sem ler número. */
function VersusRow({
  label,
  values,
  names,
}: {
  label: string
  values: [number, number]
  names: [string, string]
}) {
  const peak = Math.max(1, values[0], values[1])
  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <span className="numeral text-[15px] leading-none" style={{ color: seatAccent[0].bright }}>
          {values[0]}
        </span>
        <span className="caps text-hud text-ink-muted">{label}</span>
        <span className="numeral text-[15px] leading-none" style={{ color: seatAccent[1].bright }}>
          {values[1]}
        </span>
      </div>
      <div className="mt-1.5 flex items-center gap-1">
        <Bar
          ratio={values[0] / peak}
          color={seatAccent[0].core}
          align="right"
          label={`${label}, ${names[0]}: ${values[0]}`}
        />
        <span aria-hidden="true" className="h-3 w-px bg-edge-strong" />
        <Bar
          ratio={values[1] / peak}
          color={seatAccent[1].core}
          align="left"
          label={`${label}, ${names[1]}: ${values[1]}`}
        />
      </div>
    </div>
  )
}

function Bar({
  ratio,
  color,
  align,
  label,
}: {
  ratio: number
  color: string
  align: 'left' | 'right'
  label: string
}) {
  return (
    <div
      className={clsx('metal-well h-2 flex-1 overflow-hidden rounded-full', align === 'right' && 'flex justify-end')}
      title={label}
    >
      <div
        className="h-full rounded-full transition-[width] duration-[var(--duration-slow)] ease-out"
        style={{ width: `${clamp01(ratio) * 100}%`, backgroundColor: color }}
      />
    </div>
  )
}

function DamageByTurn({ damage, names }: { damage: TurnDamage[]; names: [string, string] }) {
  const recent = damage.slice(-VISIBLE_TURNS)
  if (recent.length === 0) {
    return <p className="py-1 text-center text-[12px] text-ink-faint">Nenhum dano ainda.</p>
  }
  const peak = Math.max(1, ...recent.map((entry) => Math.max(entry.dealt[0], entry.dealt[1])))

  return (
    <ul className="flex flex-col gap-1.5">
      {recent.map((entry) => (
        <li key={entry.turn} className="flex items-center gap-2">
          <span className="caps text-hud w-8 shrink-0 text-ink-faint">T{entry.turn}</span>
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            {[0, 1].map((seat) => (
              <div
                key={seat}
                className="h-1.5 rounded-full transition-[width] duration-[var(--duration-base)] ease-out"
                style={{
                  width: `${clamp01(entry.dealt[seat] / peak) * 100}%`,
                  backgroundColor: seatAccent[seat].core,
                  opacity: entry.dealt[seat] === 0 ? 0.15 : 1,
                }}
                title={`${names[seat]} causou ${entry.dealt[seat]} no turno ${entry.turn}`}
              />
            ))}
          </div>
          <span className="w-12 shrink-0 text-right font-mono text-[11px] tabular-nums text-ink-muted">
            {entry.dealt[0]}–{entry.dealt[1]}
          </span>
        </li>
      ))}
    </ul>
  )
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(1, Math.max(0, value))
}
