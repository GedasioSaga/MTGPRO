import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import type {
  CardView,
  ObjectId,
  PlayerView,
  StackItemView,
} from '../../types/protocol'
import { seatAccent, withAlpha } from './boardVisuals'

export interface StackPanelProps {
  stack: StackItemView[]
  cards: Record<ObjectId, CardView>
  players: PlayerView[]
}

/**
 * A pilha (CR 405). O item de cima resolve primeiro, então ele aparece
 * primeiro na faixa e os de baixo seguem à direita — a ordem de resolução tem
 * que ser lida na geometria, não no texto.
 */
export function StackPanel({ stack, cards, players }: StackPanelProps) {
  const reduceMotion = useReducedMotion()
  const top = [...stack].reverse()

  return (
    <section className="stack-panel" data-fx-stack aria-label="Pilha">
      <header className="stack-panel__head">
        <span className="stack-panel__title">pilha</span>
        <span className="stack-panel__count">{stack.length}</span>
      </header>

      <div className="stack-panel__body">
        <AnimatePresence initial={false}>
          {top.length === 0 ? (
            <motion.p
              key="empty"
              className="stack-panel__empty"
              initial={reduceMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              nada resolvendo
            </motion.p>
          ) : (
            top.map((item, index) => (
              <motion.article
                key={item.id}
                className="stack-item"
                data-stack-id={item.id}
                data-top={index === 0 ? 'true' : 'false'}
                style={{
                  zIndex: top.length - index,
                  borderColor: withAlpha(seatAccent(item.controller), index === 0 ? 0.55 : 0.22),
                  background: `linear-gradient(122deg, ${withAlpha(
                    seatAccent(item.controller),
                    index === 0 ? 0.2 : 0.1,
                  )} 0%, rgba(10,13,21,0.92) 62%)`,
                }}
                initial={reduceMotion ? false : { opacity: 0, x: 26, scale: 0.96 }}
                animate={{ opacity: 1, x: 0, scale: 1 }}
                exit={reduceMotion ? { opacity: 0 } : { opacity: 0, x: -18, scale: 0.94 }}
                transition={
                  reduceMotion
                    ? { duration: 0 }
                    : { type: 'spring', stiffness: 380, damping: 32 }
                }
              >
                <span
                  className="stack-item__seat"
                  style={{ background: seatAccent(item.controller) }}
                  aria-hidden="true"
                />
                <div className="stack-item__body">
                  <div className="stack-item__head">
                    <h3 className="stack-item__name">{item.name}</h3>
                    <span className="stack-item__kind">
                      {item.isAbility ? 'habilidade' : 'mágica'}
                    </span>
                  </div>
                  {item.text.length > 0 ? (
                    <p className="stack-item__text">{item.text}</p>
                  ) : null}
                  <TargetLine item={item} cards={cards} players={players} />
                </div>
              </motion.article>
            ))
          )}
        </AnimatePresence>
      </div>
    </section>
  )
}

function TargetLine({
  item,
  cards,
  players,
}: {
  item: StackItemView
  cards: Record<ObjectId, CardView>
  players: PlayerView[]
}) {
  const names = [
    ...item.targets.map((id) => cards[id]?.name ?? `objeto #${id}`),
    ...item.targetPlayers.map((id) => players[id]?.name ?? `jogador ${id}`),
  ]
  if (names.length === 0) return null

  return (
    <p className="stack-item__targets">
      <span aria-hidden="true">→</span>
      <span>{names.join(', ')}</span>
    </p>
  )
}
