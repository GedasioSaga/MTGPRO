import { useEffect, useId, useState } from 'react'
import type { ReactNode } from 'react'
import clsx from 'clsx'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import type { CardView, ObjectId } from '../../types/protocol'
import { CardSlot } from './CardSlot'

export type ZoneTone = 'grave' | 'exile'

export interface ZonePileProps {
  label: string
  /** Ordem do motor: o último id é o topo da pilha. */
  ids: ObjectId[]
  cards: Record<ObjectId, CardView>
  /** Contagem autoritativa do `PlayerView`; pode passar dos ids conhecidos. */
  count: number
  tone: ZoneTone
  icon: ReactNode
  side: 'top' | 'bottom'
}

/** Quantas cartas fingem espessura embaixo da de cima. */
const DEPTH_LAYERS = 3

/**
 * Pilha de zona fechada (cemitério, exílio). Fechada ela é só espessura e
 * número — abrir é ação deliberada, porque em partida automática o conteúdo
 * quase nunca importa, mas quando importa importa inteiro.
 */
export function ZonePile({ label, ids, cards, count, tone, icon, side }: ZonePileProps) {
  const [open, setOpen] = useState(false)
  const reduceMotion = useReducedMotion()
  const sheetId = useId()
  const empty = count === 0

  useEffect(() => {
    if (!open) return
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open])

  useEffect(() => {
    if (empty) setOpen(false)
  }, [empty])

  const topId = ids.length > 0 ? ids[ids.length - 1] : null
  const topCard = topId === null ? null : (cards[topId] ?? null)
  const depth = Math.min(DEPTH_LAYERS, Math.max(0, count - 1))

  return (
    <div className={clsx('zone-pile', `zone-pile--${tone}`)} data-open={open}>
      <button
        type="button"
        className="zone-pile__trigger"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        aria-controls={sheetId}
        disabled={empty}
        title={`${label}: ${count} carta(s)`}
      >
        <span className="zone-pile__well">
          {Array.from({ length: depth }, (_, layer) => (
            <span
              key={layer}
              className="zone-pile__layer"
              style={{ transform: `translate(${(layer + 1) * 2}px, ${(layer + 1) * 2}px)` }}
              aria-hidden="true"
            />
          ))}
          {topCard === null ? (
            <span className="zone-pile__icon" aria-hidden="true">
              {icon}
            </span>
          ) : (
            <CardSlot
              id={topCard.id}
              card={topCard}
              size="small"
              scale={0.58}
              className="zone-pile__top"
            />
          )}
        </span>
        <span className="zone-pile__meta">
          <span className="zone-pile__label">{label}</span>
          <span className="zone-pile__count">{count}</span>
        </span>
      </button>

      <AnimatePresence>
        {open ? (
          <motion.div
            id={sheetId}
            role="dialog"
            aria-label={`${label} — ${count} cartas`}
            className="zone-pile__sheet"
            data-side={side}
            initial={reduceMotion ? false : { opacity: 0, y: side === 'top' ? -10 : 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={reduceMotion ? { opacity: 0 } : { opacity: 0, y: side === 'top' ? -8 : 8 }}
            transition={reduceMotion ? { duration: 0 } : { duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
          >
            <header className="zone-pile__sheet-head">
              <span>{label}</span>
              <button
                type="button"
                className="zone-pile__close"
                onClick={() => setOpen(false)}
                aria-label="Fechar"
              >
                ✕
              </button>
            </header>
            <div className="zone-pile__grid">
              {[...ids].reverse().map((id) => {
                const card = cards[id] ?? null
                return (
                  <CardSlot key={id} id={id} card={card} size="small" scale={0.92} />
                )
              })}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  )
}
