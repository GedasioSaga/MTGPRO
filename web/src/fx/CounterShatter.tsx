import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

const SHARDS = 7

/** Cacos deterministicos: mesma magia contra-atacada quebra sempre igual. */
const SHARD_ANGLES = Array.from({ length: SHARDS }, (_, i) => (i * 360) / SHARDS - 90)

/** Anel que implode e cacos que voam: leitura de "isso nao vai resolver". */
export function CounterShatter({ at, durationMs }: FxOf<'counterShatter'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)

  return (
    <FxNode at={at}>
      <motion.div
        className="fx-shatter-ring"
        aria-hidden
        initial={{ opacity: 0 }}
        animate={reduced ? { opacity: [0, 0.9, 0] } : { opacity: [0, 0.9, 0], scale: [1.5, 0.85, 0.4] }}
        transition={{ duration: dur, ease: EASE.in, times: [0, 0.32, 1] }}
      />
      {!reduced &&
        SHARD_ANGLES.map((angle, index) => {
          const rad = (angle * Math.PI) / 180
          const reach = 48 + (index % 3) * 14
          const dx = Math.cos(rad) * reach
          const dy = Math.sin(rad) * reach
          return (
            <motion.span
              key={angle}
              className="fx-shard"
              aria-hidden
              style={{ rotate: angle + 90 }}
              initial={{ opacity: 0 }}
              animate={{
                opacity: [0, 1, 0],
                x: [0, dx * 0.55, dx],
                y: [0, dy * 0.55, dy],
                scale: [1, 1, 0.6],
              }}
              transition={{ duration: dur, ease: EASE.out, times: [0, 0.28, 1] }}
            />
          )
        })}
    </FxNode>
  )
}
