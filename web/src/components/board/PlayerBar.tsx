import clsx from 'clsx'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import type { PlayerView } from '../../types/protocol'
import { hashString, withAlpha } from '../../design/color'
import { COLORLESS_HEX, MANA_HEX, cssVars, initialsOf, seatSwatch } from './boardVisuals'
import { HandIcon, PoisonIcon } from './BoardIcons'

const STARTING_LIFE = 20
const LETHAL_POISON = 10

export interface PlayerBarProps {
  player: PlayerView
  side: 'top' | 'bottom'
  /** Dano de combate que vai chegar neste jogador agora. 0 fora de combate. */
  underFire?: number
}

/**
 * Faixa fina do jogador, FORA do tapete.
 *
 * O que é zona de jogo mudou de endereço: vida, deck, cemitério e exílio agora
 * moram nos quadros serigrafados do mat. Aqui fica só o que não é zona — quem
 * está jogando, se é a vez dele, quanta mana flutua e quantos terrenos já
 * caíram. Por isso a faixa perdeu altura: ela é a beira da mesa, não um HUD.
 */
export function PlayerBar({ player, side, underFire = 0 }: PlayerBarProps) {
  const accent = seatSwatch(player.id).core
  const reduceMotion = useReducedMotion()

  return (
    <aside
      className={clsx('player-strip', `player-strip--${side}`)}
      data-active={player.isActive ? 'true' : 'false'}
      style={cssVars({ '--seat': accent, '--seat-soft': withAlpha(accent, 0.2) })}
      aria-label={`${player.name}: ${player.life} de vida`}
    >
      <Portrait player={player} accent={accent} underFire={underFire} />

      <div className="player-strip__name-block">
        <p className="player-strip__name">{player.name}</p>
        <div className="player-strip__tags">
          {player.isActive ? <Badge accent={accent}>turno</Badge> : null}
          {player.hasPriority ? <Badge accent={accent}>prioridade</Badge> : null}
          {player.hasLost ? <span className="player-strip__lost">derrotado</span> : null}
        </div>
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

        <div className="player-strip__count" title={`Mão: ${player.handCount}`}>
          <span aria-hidden="true">
            <HandIcon className="size-3.5" />
          </span>
          <span className="player-strip__count-value">{player.handCount}</span>
          <span className="sr-only">cartas na mão</span>
        </div>
      </div>
    </aside>
  )
}

/**
 * Total de vida pousado no quadro VIDA impresso no mat.
 *
 * É o mesmo nó que a camada de efeitos e as setas de alvo procuram
 * (`data-fx-player`, `data-player-id`): o dano passa a apontar para o lugar da
 * mesa onde a vida está escrita, e não para uma caixa flutuante na borda.
 */
export function PlayerLife({ player }: { player: PlayerView }) {
  const accent = seatSwatch(player.id).core
  const reduceMotion = useReducedMotion()
  const critical = player.life <= 5 && !player.hasLost
  const ratio = Math.max(0, Math.min(1, player.life / STARTING_LIFE))
  const color = critical ? '#f2685e' : accent

  return (
    <div
      data-player-id={player.id}
      data-fx-player={player.id}
      className={clsx('mat-life', critical && !reduceMotion && 'board-life-pulse')}
      style={cssVars({ '--life': color, '--life-fill': `${ratio * 100}%` })}
      aria-label={`${player.life} de vida`}
    >
      <span className="mat-life__value">{player.life}</span>
      <span className="mat-life__bar" aria-hidden="true" />
    </div>
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
    <div
      className={clsx(
        'player-strip__avatar-wrap',
        underFire > 0 && 'player-strip__avatar-wrap--hit',
      )}
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
    <div
      className="player-strip__lands"
      title={`Terrenos jogados: ${player.landsPlayedThisTurn}/${max}`}
    >
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
