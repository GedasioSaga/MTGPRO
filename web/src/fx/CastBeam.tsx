import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { angleDeg, distance, midpoint } from './fxAnchors'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Feixe da mao ate a pilha, com o nome da magia acompanhando. */
export function CastBeam({ from, to, color, name, durationMs }: FxOf<'castBeam'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const length = distance(from, to)
  const angle = angleDeg(from, to)
  const mid = midpoint(from, to)

  return (
    <>
      <FxNode at={from} pin={false}>
        {/* Rotacao estatica no pai para que o `scaleX` do filho corra no eixo do feixe. */}
        <div
          className="fx-beam"
          aria-hidden
          style={
            {
              width: length,
              transform: `rotate(${angle}deg)`,
              '--fx-beam-color': color,
            } as CSSProperties
          }
        >
          <motion.div
            className="fx-beam__core"
            initial={{ opacity: 0 }}
            animate={
              reduced
                ? { opacity: [0, 1, 0] }
                : { opacity: [0, 1, 1, 0], scaleX: [0, 1, 1, 1] }
            }
            transition={{
              duration: dur,
              ease: EASE.out,
              times: reduced ? [0, 0.35, 1] : [0, 0.16, 0.62, 1],
            }}
          />
          {!reduced && (
            <motion.div
              className="fx-beam__orb"
              initial={{ opacity: 0 }}
              animate={{ opacity: [0, 1, 1, 0], x: [0, length * 0.3, length, length] }}
              transition={{ duration: dur, ease: EASE.inOut, times: [0, 0.14, 0.72, 1] }}
            />
          )}
        </div>
      </FxNode>

      <FxNode at={mid}>
        <motion.div
          className="fx-tag"
          style={{ '--fx-tag-bg': color } as CSSProperties}
          initial={{ opacity: 0 }}
          animate={
            reduced ? { opacity: [0, 1, 0] } : { opacity: [0, 1, 1, 0], y: [12, -2, -10, -18] }
          }
          transition={{
            duration: dur,
            ease: EASE.out,
            times: reduced ? [0, 0.35, 1] : [0, 0.18, 0.7, 1],
          }}
        >
          <span className="fx-tag__text">{name}</span>
        </motion.div>
      </FxNode>
    </>
  )
}
