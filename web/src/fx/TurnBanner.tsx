import { motion } from 'motion/react'
import { withAlpha } from '../design/color'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Faixa que anuncia o turno e de quem ele e. */
export function TurnBanner({ turn, playerName, accent }: FxOf<'turnBanner'>) {
  const { reduced, d } = useFxMotion()

  return (
    <div className="fx-node" style={{ left: 0, right: 0, top: '34%' }}>
      <motion.div
        className="fx-banner"
        style={
          {
            position: 'relative',
            overflow: 'hidden',
            '--fx-accent': accent,
            '--fx-accent-soft': withAlpha(accent, 0.45),
          } as React.CSSProperties
        }
        initial={reduced ? { opacity: 0 } : { opacity: 0, x: -56 }}
        animate={reduced ? { opacity: 1 } : { opacity: 1, x: 0 }}
        exit={reduced ? { opacity: 0 } : { opacity: 0, x: 28 }}
        transition={{ duration: d(320), ease: EASE.out }}
      >
        <span className="fx-banner__turn">Turn {turn}</span>
        <span className="fx-banner__name">{playerName}</span>
        {!reduced && (
          <motion.span
            className="fx-banner__sheen"
            aria-hidden
            initial={{ x: '-70%' }}
            animate={{ x: '160%' }}
            transition={{ duration: d(700), ease: EASE.inOut, delay: 0.12 }}
          />
        )}
      </motion.div>
    </div>
  )
}
