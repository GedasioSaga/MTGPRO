import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { FX_TONES, withAlpha } from './fxColors'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/**
 * Carta se desfazendo: brasa laranja para cemiterio, violeta para exilio.
 *
 * A brasa em si vive na camada de particulas — aqui fica so a placa que apaga
 * sob ela. Em movimento reduzido a camada nao existe e esta placa e o efeito
 * inteiro, ainda legivel porque o vulto da carta some no lugar certo.
 */
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
    </FxNode>
  )
}
