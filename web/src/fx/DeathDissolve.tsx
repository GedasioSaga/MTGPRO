import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { FX_TONES, withAlpha } from './fxColors'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

const EMBERS = 5

/** Carta se desfazendo: brasa laranja para cemiterio, violeta para exilio. */
export function DeathDissolve({ rect, tone, durationMs }: FxOf<'deathDissolve'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const color = tone === 'exile' ? FX_TONES.exile : FX_TONES.death
  const vars = {
    '--fx-dissolve-color': color,
    '--fx-dissolve-soft': withAlpha(color, 0.36),
  } as CSSProperties

  return (
    <FxNode at={{ x: rect.x, y: rect.y }}>
      <motion.div
        className="fx-dissolve"
        aria-hidden
        style={{ ...vars, width: rect.w, height: rect.h }}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 0.8, 0] }
            : { opacity: [0, 1, 0.8, 0], scale: [1, 1, 0.97, 0.93] }
        }
        transition={{ duration: dur, ease: EASE.in, times: [0, 0.16, 0.56, 1] }}
      />
      {!reduced &&
        Array.from({ length: EMBERS }, (_, index) => {
          const spread = (index / (EMBERS - 1) - 0.5) * rect.w * 0.72
          const rise = rect.h * (0.5 + (index % 2) * 0.22)
          return (
            <motion.span
              key={index}
              className="fx-ember"
              aria-hidden
              style={vars}
              initial={{ opacity: 0 }}
              animate={{
                opacity: [0, 1, 0],
                x: [spread, spread * 1.25, spread * 1.5],
                y: [rect.h * 0.2, -rise * 0.5, -rise],
              }}
              transition={{
                duration: dur,
                ease: EASE.out,
                times: [0, 0.34, 1],
                delay: index * 0.02,
              }}
            />
          )
        })}
    </FxNode>
  )
}
