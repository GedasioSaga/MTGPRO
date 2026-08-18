import clsx from 'clsx'
import { motion, useReducedMotion } from 'motion/react'
import type { CardView, ObjectId } from '../../types/protocol'
import { CardSlot } from './CardSlot'

const OWN_WIDTH = 'clamp(88px, 6.6vw, 124px)'
const OPPONENT_WIDTH = 'clamp(46px, 3.3vw, 62px)'

export interface HandRowProps {
  ids: ObjectId[]
  cards: Record<ObjectId, CardView>
  side: 'top' | 'bottom'
  /** Usado quando a mão está redigida e só conhecemos o tamanho. */
  count: number
}

/**
 * Mão em leque. O leque não é enfeite: com dez cartas empilhadas retas não dá
 * para contar quantas são, e a curva devolve a contagem num relance.
 */
export function HandRow({ ids, cards, side, count }: HandRowProps) {
  const reduceMotion = useReducedMotion()
  const isOwn = side === 'bottom'
  const slots: (ObjectId | null)[] =
    ids.length > 0 ? ids : Array.from({ length: count }, () => null)

  if (slots.length === 0) {
    return <div className="h-full" aria-label="Mão vazia" />
  }

  const total = slots.length
  const center = (total - 1) / 2
  const spread = Math.min(4.2, 26 / Math.max(1, total))
  const overlapRatio = total <= 6 ? 0.12 : Math.min(0.62, 0.12 + (total - 6) * 0.07)
  const width = isOwn ? OWN_WIDTH : OPPONENT_WIDTH
  const lift = isOwn ? 5 : 3

  return (
    <div
      className={clsx(
        'flex h-full items-end justify-center',
        isOwn ? 'items-end pb-1' : 'items-start pt-1',
      )}
      aria-label={isOwn ? `Sua mão: ${total} cartas` : `Mão do oponente: ${total} cartas`}
    >
      {slots.map((id, index) => {
        const offset = index - center
        const rotate = (isOwn ? 1 : -1) * offset * spread
        const drop = Math.abs(offset) ** 1.8 * lift
        const translateY = isOwn ? drop : -drop
        const card = id === null ? null : (cards[id] ?? null)

        return (
          <motion.div
            key={id ?? `back-${index}`}
            className="relative"
            style={{
              zIndex: index,
              marginLeft: index === 0 ? 0 : `calc(${width} * -${overlapRatio})`,
              transformOrigin: isOwn ? 'bottom center' : 'top center',
            }}
            initial={false}
            animate={{ rotate, y: translateY }}
            whileHover={
              reduceMotion
                ? undefined
                : { y: translateY + (isOwn ? -26 : 26), rotate: 0, scale: 1.07, zIndex: 60 }
            }
            transition={
              reduceMotion
                ? { duration: 0 }
                : { type: 'spring', stiffness: 340, damping: 30 }
            }
          >
            <CardSlot
              id={id ?? -1 - index}
              card={isOwn ? card : null}
              width={width}
              size={isOwn ? 'medium' : 'micro'}
              revealed={isOwn}
              className="board-hand-card"
            />
          </motion.div>
        )
      })}
    </div>
  )
}
