import clsx from 'clsx'
import type { ReactNode } from 'react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import type { PlayerView } from '../../types/protocol'
import { COLORLESS_HEX, MANA_HEX, hashString, initialsOf, seatAccent, withAlpha } from './boardVisuals'
import { ExileIcon, GraveyardIcon, HandIcon, LibraryIcon, PoisonIcon } from './BoardIcons'

const STARTING_LIFE = 20
const LETHAL_POISON = 10

export interface PlayerBarProps {
  player: PlayerView
  side: 'top' | 'bottom'
}

export function PlayerBar({ player, side }: PlayerBarProps) {
  const accent = seatAccent(player.id)
  const reduceMotion = useReducedMotion()
  const critical = player.life <= 5 && !player.hasLost

  return (
    <aside
      className={clsx(
        'relative flex h-full flex-col gap-3 px-4 py-4',
        side === 'top' ? 'flex-col-reverse justify-end' : 'justify-end',
      )}
      aria-label={`${player.name}: ${player.life} de vida`}
    >
      <div
        className="pointer-events-none absolute inset-0 -z-10"
        style={{
          background: `radial-gradient(120% 60% at ${side === 'top' ? '50% 0%' : '50% 100%'}, ${withAlpha(accent, player.isActive ? 0.16 : 0.06)} 0%, transparent 70%)`,
        }}
      />

      <Portrait player={player} accent={accent} />

      <LifeOrb
        player={player}
        accent={accent}
        critical={critical}
        pulse={critical && !reduceMotion}
      />

      <div className="grid grid-cols-4 gap-1.5">
        <CountTile label="Mão" value={player.handCount} icon={<HandIcon className="size-4" />} />
        <CountTile label="Biblioteca" value={player.libraryCount} icon={<LibraryIcon className="size-4" />} warn={player.libraryCount <= 3} />
        <CountTile label="Cemitério" value={player.graveyardCount} icon={<GraveyardIcon className="size-4" />} />
        <CountTile label="Exílio" value={player.exileCount} icon={<ExileIcon className="size-4" />} />
      </div>

      <div className="flex min-h-6 items-center justify-between gap-2">
        <ManaPool player={player} />
        <AnimatePresence>
          {player.poison > 0 ? (
            <motion.span
              key="poison"
              initial={reduceMotion ? false : { opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              className={clsx(
                'flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-semibold tabular-nums',
                player.poison >= LETHAL_POISON
                  ? 'border-lime-300/60 bg-lime-400/25 text-lime-100'
                  : 'border-lime-400/35 bg-lime-500/12 text-lime-200/90',
              )}
              title={`${player.poison} marcadores de veneno`}
            >
              <PoisonIcon className="size-3.5" />
              {player.poison}
            </motion.span>
          ) : null}
        </AnimatePresence>
      </div>

      <LandGauge player={player} accent={accent} />
    </aside>
  )
}

function Portrait({ player, accent }: { player: PlayerView; accent: string }) {
  const hash = hashString(player.name)

  return (
    <div className="flex items-center gap-3">
      <div className="relative shrink-0">
        <div
          className={clsx(
            'grid size-13 place-items-center rounded-full text-[15px] font-semibold tracking-wide text-white/90',
            'ring-1 ring-inset ring-white/15',
          )}
          style={{
            background: `conic-gradient(from ${hash % 360}deg, ${withAlpha(accent, 0.85)}, #1a1f2e 45%, ${withAlpha(accent, 0.5)} 78%, #1a1f2e)`,
            boxShadow: player.isActive ? `0 0 0 2px ${withAlpha(accent, 0.75)}, 0 0 22px ${withAlpha(accent, 0.35)}` : 'none',
          }}
        >
          <span className="rounded-full bg-black/45 px-2 py-1 backdrop-blur-[2px]">
            {initialsOf(player.name)}
          </span>
        </div>
        {player.hasPriority ? (
          <span
            className="absolute -right-0.5 -bottom-0.5 size-3.5 rounded-full ring-2 ring-[#0a0c14] board-priority-dot"
            style={{ background: accent }}
            title="Tem prioridade"
          />
        ) : null}
      </div>

      <div className="min-w-0">
        <p className="truncate text-[15px] leading-tight font-semibold text-white/92">{player.name}</p>
        <div className="mt-1 flex flex-wrap items-center gap-1">
          {player.isActive ? (
            <Badge accent={accent}>turno</Badge>
          ) : null}
          {player.hasPriority ? <Badge accent={accent}>prioridade</Badge> : null}
          {player.hasLost ? (
            <span className="rounded-sm border border-red-400/45 bg-red-500/18 px-1.5 py-px text-[10px] font-semibold tracking-[0.09em] text-red-200 uppercase">
              derrotado
            </span>
          ) : null}
        </div>
      </div>
    </div>
  )
}

function Badge({ accent, children }: { accent: string; children: string }) {
  return (
    <span
      className="rounded-sm px-1.5 py-px text-[10px] font-semibold tracking-[0.09em] uppercase"
      style={{
        color: accent,
        background: withAlpha(accent, 0.14),
        border: `1px solid ${withAlpha(accent, 0.34)}`,
      }}
    >
      {children}
    </span>
  )
}

function LifeOrb({
  player,
  accent,
  critical,
  pulse,
}: {
  player: PlayerView
  accent: string
  critical: boolean
  pulse: boolean
}) {
  const ratio = Math.max(0, Math.min(1, player.life / STARTING_LIFE))
  const color = critical ? '#f2685e' : accent

  return (
    <div
      data-player-id={player.id}
      className={clsx(
        'relative flex items-center gap-3 overflow-hidden rounded-xl border px-3.5 py-2.5',
        pulse && 'board-life-pulse',
      )}
      style={{
        borderColor: withAlpha(color, 0.32),
        background: `linear-gradient(135deg, ${withAlpha(color, 0.16)} 0%, rgba(12,14,22,0.72) 62%)`,
        boxShadow: `inset 0 1px 0 rgba(255,255,255,0.06), 0 8px 24px -14px ${withAlpha(color, 0.9)}`,
      }}
    >
      <div
        className="pointer-events-none absolute inset-y-0 left-0"
        style={{ width: `${ratio * 100}%`, background: withAlpha(color, 0.1) }}
      />
      <span
        className="relative text-[13px] font-semibold tracking-[0.14em] uppercase"
        style={{ color: withAlpha(color, 0.72) }}
      >
        vida
      </span>
      <span
        className="relative ml-auto text-[44px] leading-none font-semibold tabular-nums"
        style={{ color, textShadow: `0 0 26px ${withAlpha(color, 0.5)}` }}
      >
        {player.life}
      </span>
    </div>
  )
}

function CountTile({
  label,
  value,
  icon,
  warn,
}: {
  label: string
  value: number
  icon: ReactNode
  warn?: boolean
}) {
  return (
    <div
      className={clsx(
        'flex flex-col items-center gap-0.5 rounded-lg border border-white/8 bg-white/4 py-1.5',
        warn && 'border-amber-400/40 bg-amber-400/8',
      )}
      title={`${label}: ${value}`}
    >
      <span className={clsx('text-white/45', warn && 'text-amber-300/80')} aria-hidden="true">
        {icon}
      </span>
      <span className={clsx('text-[13px] leading-none font-semibold tabular-nums text-white/85', warn && 'text-amber-200')}>
        {value}
      </span>
      <span className="sr-only">{label}</span>
    </div>
  )
}

function ManaPool({ player }: { player: PlayerView }) {
  const pips: { key: string; color: string; count: number }[] = []
  player.manaPool.colored.forEach((count, index) => {
    if (count > 0) pips.push({ key: `c${index}`, color: MANA_HEX[index], count })
  })
  if (player.manaPool.colorless > 0) {
    pips.push({ key: 'colorless', color: COLORLESS_HEX, count: player.manaPool.colorless })
  }
  if (pips.length === 0) {
    return <span className="text-[11px] tracking-wide text-white/25">sem mana flutuante</span>
  }

  return (
    <div className="flex items-center gap-1" aria-label="Mana flutuante">
      {pips.map((pip) => (
        <span
          key={pip.key}
          className="grid size-5 place-items-center rounded-full text-[11px] font-bold text-black/75 tabular-nums"
          style={{ background: pip.color, boxShadow: `0 0 12px ${withAlpha(pip.color, 0.45)}` }}
        >
          {pip.count}
        </span>
      ))}
    </div>
  )
}

function LandGauge({ player, accent }: { player: PlayerView; accent: string }) {
  const max = Math.max(1, player.maxLandsPerTurn)
  return (
    <div className="flex items-center gap-2">
      <span className="text-[10px] tracking-[0.12em] text-white/32 uppercase">terreno</span>
      <div className="flex gap-1">
        {Array.from({ length: max }, (_, index) => (
          <span
            key={index}
            className="h-1 w-6 rounded-full"
            style={{
              background:
                index < player.landsPlayedThisTurn ? accent : 'rgba(255,255,255,0.12)',
            }}
          />
        ))}
      </div>
      <span className="ml-auto text-[11px] tabular-nums text-white/35">
        {player.landsPlayedThisTurn}/{max}
      </span>
    </div>
  )
}
