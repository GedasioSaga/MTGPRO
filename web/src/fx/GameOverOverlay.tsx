import { motion } from 'motion/react'
import { useState } from 'react'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Painel final. Unico efeito que aceita ponteiro — o resto e decoracao. */
export function GameOverOverlay({ title, subtitle, scoreboard, turns }: FxOf<'gameOver'>) {
  const { reduced, d } = useFxMotion()
  const [dismissed, setDismissed] = useState(false)

  if (dismissed) return null

  return (
    <motion.div
      className="fx-gameover"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: d(320), ease: EASE.out }}
    >
      <motion.div
        className="fx-gameover__panel"
        role="status"
        initial={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.965, y: 12 }}
        animate={reduced ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: d(420), ease: EASE.out, delay: reduced ? 0 : 0.05 }}
      >
        <p className="fx-gameover__eyebrow">Match complete</p>
        <h2 className="fx-gameover__title">{title}</h2>
        <p className="fx-gameover__subtitle">
          {subtitle} · {turns} {turns === 1 ? 'turn' : 'turns'}
        </p>
        <div className="fx-gameover__score">
          {scoreboard.map((row, index) => (
            <div
              key={`${row.name}-${index}`}
              className="fx-gameover__row"
              data-won={row.won}
            >
              <span>{row.name}</span>
              <span className="fx-gameover__life">{row.life}</span>
            </div>
          ))}
        </div>
        <button
          type="button"
          className="fx-gameover__button"
          onClick={() => setDismissed(true)}
        >
          Back to the board
        </button>
      </motion.div>
    </motion.div>
  )
}
