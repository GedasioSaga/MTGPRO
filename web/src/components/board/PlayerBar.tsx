import clsx from 'clsx'
import type { ReactNode } from 'react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import type { PlayerView } from '../../types/protocol'
import {
  COLORLESS_HEX,
  MANA_HEX,
  cssVars,
  hashString,
  initialsOf,
  seatAccent,
  withAlpha,
} from './boardVisuals'
import { ExileIcon, GraveyardIcon, HandIcon, LibraryIcon, PoisonIcon } from './BoardIcons'

const STARTING_LIFE = 20
const LETHAL_POISON = 10

export interface PlayerBarProps {
  player: PlayerView
  side: 'top' | 'bottom'
  /** Dano de combate que vai chegar neste jogador agora. 0 fora de combate. */
  underFire?: number
}

/**
 * Borda da mesa de um jogador.
 *
 * Deixou de ser coluna lateral: quem senta na mesa senta numa BORDA, então
 * nome, retrato e vida vivem na faixa superior (oponente) e inferior (jogador
 * de baixo), à altura de quem olha. Os contadores de zona ficam na cauda da
 * faixa, pequenos e apagados — são consulta, não manchete.
 */
export function PlayerBar({ player, side, underFire = 0 }: PlayerBarProps) {
  const accent = seatAccent(player.id)
  const reduceMotion = useReducedMotion()
  const critical = player.life <= 5 && !player.hasLost

  return (
    <aside
      className={clsx('player-strip', `player-strip--${side}`)}
      data-active={player.isActive ? 'true' : 'false'}
      style={cssVars({ '--seat': accent, '--seat-soft': withAlpha(accent, 0.2) })}
      aria-label={`${player.name}: ${player.life} de vida`}
    >
      {/* Núcleo centralizado: retrato e vida ficam no eixo do tapete, que é
          para onde o vetor de dano aponta. Nome no canto obriga o vetor a
          atravessar a mesa na diagonal. */}
      <div className="player-strip__core">
        <Portrait player={player} accent={accent} underFire={underFire} />

        <LifeReadout
          player={player}
          accent={accent}
          critical={critical}
          pulse={critical && !reduceMotion}
        />
      </div>

      <div className="player-strip__meters">
        <ManaPool player={player} />

        <AnimatePresence>
          {player.poison > 0 ? (
            <motion.span
              key="poison"
              initial={reduceMotion ? false : { opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              className={clsx(
                'player-strip__poison',
                player.poison >= LETHAL_POISON && 'player-strip__poison--lethal',
              )}
              title={`${player.poison} marcadores de veneno`}
            >
              <PoisonIcon className="size-3.5" />
              {player.poison}
            </motion.span>
          ) : null}
        </AnimatePresence>

        <LandGauge player={player} />

        <div className="player-strip__counts">
          <CountTile label="Mão" value={player.handCount} icon={<HandIcon className="size-3.5" />} />
          <CountTile
            label="Biblioteca"
            value={player.libraryCount}
            icon={<LibraryIcon className="size-3.5" />}
            warn={player.libraryCount <= 3}
          />
          <CountTile
            label="Cemitério"
            value={player.graveyardCount}
            icon={<GraveyardIcon className="size-3.5" />}
          />
          <CountTile
            label="Exílio"
            value={player.exileCount}
            icon={<ExileIcon className="size-3.5" />}
          />
        </div>
      </div>
    </aside>
  )
}

function Portrait({
  player,
  accent,
  underFire,
}: {
  player: PlayerView
  accent: string
  underFire: number
}) {
  const hash = hashString(player.name)

  return (
    <>
      <div className="player-strip__name-block">
        <p className="player-strip__name">{player.name}</p>
        <div className="player-strip__tags">
          {player.isActive ? <Badge accent={accent}>turno</Badge> : null}
          {player.hasPriority ? <Badge accent={accent}>prioridade</Badge> : null}
          {player.hasLost ? <span className="player-strip__lost">derrotado</span> : null}
        </div>
      </div>

      <div
        className={clsx('player-strip__avatar-wrap', underFire > 0 && 'player-strip__avatar-wrap--hit')}
        data-player-portrait={player.id}
      >
        <div
          className="player-strip__avatar"
          style={{
            background: `conic-gradient(from ${hash % 360}deg, ${withAlpha(accent, 0.85)}, #1a1f2e 45%, ${withAlpha(accent, 0.5)} 78%, #1a1f2e)`,
            boxShadow: player.isActive
              ? `0 0 0 2px ${withAlpha(accent, 0.75)}, 0 0 22px ${withAlpha(accent, 0.35)}`
              : 'none',
          }}
        >
          <span className="player-strip__initials">{initialsOf(player.name)}</span>
        </div>
        {player.hasPriority ? (
          <span
            className="player-strip__dot board-priority-dot"
            style={{ background: accent }}
            title="Tem prioridade"
          />
        ) : null}
      </div>
    </>
  )
}

function Badge({ accent, children }: { accent: string; children: string }) {
  return (
    <span
      className="player-strip__badge"
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

function LifeReadout({
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
      data-fx-player={player.id}
      className={clsx('player-strip__life', pulse && 'board-life-pulse')}
      style={cssVars({
        '--life': color,
        '--life-fill': `${ratio * 100}%`,
      })}
    >
      <span className="player-strip__life-label">vida</span>
      <span className="player-strip__life-value">{player.life}</span>
      <span className="player-strip__life-bar" aria-hidden="true" />
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
      className={clsx('player-strip__count', warn && 'player-strip__count--warn')}
      title={`${label}: ${value}`}
    >
      <span aria-hidden="true">{icon}</span>
      <span className="player-strip__count-value">{value}</span>
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
  // Mana flutuante é evento, não estado permanente: sem mana, nenhum cartaz.
  if (pips.length === 0) return null

  return (
    <div className="player-strip__mana" aria-label="Mana flutuante">
      {pips.map((pip) => (
        <span
          key={pip.key}
          className="player-strip__pip"
          style={{ background: pip.color, boxShadow: `0 0 12px ${withAlpha(pip.color, 0.45)}` }}
        >
          {pip.count}
        </span>
      ))}
    </div>
  )
}

function LandGauge({ player }: { player: PlayerView }) {
  const max = Math.max(1, player.maxLandsPerTurn)
  return (
    <div className="player-strip__lands" title={`Terrenos jogados: ${player.landsPlayedThisTurn}/${max}`}>
      <span className="player-strip__lands-label">terreno</span>
      {Array.from({ length: max }, (_, index) => (
        <span
          key={index}
          className="player-strip__land-pip"
          data-on={index < player.landsPlayedThisTurn ? 'true' : 'false'}
        />
      ))}
    </div>
  )
}
