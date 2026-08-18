import { clsx } from 'clsx'
import { useEffect, useRef, useState } from 'react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useMatchStore } from '../../state/matchStore'

interface ToastEntry {
  id: number
  tone: 'warning' | 'danger'
  text: string
  /** Fica na tela ate a condicao que a gerou desaparecer. */
  sticky: boolean
}

let nextToastId = 1

/**
 * Avisos discretos no canto da mesa: conexao caindo, reconexao, e o selo fixo
 * de "modo demonstracao" quando o replay local assumiu a partida. Nao sabe de
 * nada alem do que `matchStore` expoe — sem props, sem estado de fora.
 */
export function Toast() {
  const connection = useMatchStore((s) => s.connection)
  const offline = useMatchStore((s) => s.offline)
  const error = useMatchStore((s) => s.error)
  const reduceMotion = useReducedMotion()

  const [entries, setEntries] = useState<ToastEntry[]>([])
  const prevConnection = useRef(connection)
  const everLive = useRef(false)

  useEffect(() => {
    const prev = prevConnection.current
    prevConnection.current = connection
    if (connection === 'live') everLive.current = true

    if (connection === 'connecting' && prev !== 'connecting' && everLive.current) {
      setEntries((list) => [
        ...list.filter((e) => e.text !== 'Conexão perdida — reconectando…'),
        { id: nextToastId++, tone: 'warning', text: 'Conexão perdida — reconectando…', sticky: true },
      ])
      return
    }
    if (connection === 'error' && error) {
      setEntries((list) => [
        ...list.filter((e) => e.tone !== 'danger'),
        { id: nextToastId++, tone: 'danger', text: error, sticky: true },
      ])
      return
    }
    if (connection === 'live' || connection === 'done') {
      // A causa sumiu: os avisos "sticky" ligados a ela somem junto.
      setEntries((list) => list.filter((e) => e.tone === 'danger' && connection !== 'live'))
    }
  }, [connection, error])

  // Avisos nao-sticky se auto-removem depois de um tempo de leitura.
  useEffect(() => {
    const timers = entries
      .filter((e) => !e.sticky)
      .map((e) => window.setTimeout(() => {
        setEntries((list) => list.filter((x) => x.id !== e.id))
      }, 4200))
    return () => timers.forEach(window.clearTimeout)
  }, [entries])

  const showDemoPill = offline

  if (entries.length === 0 && !showDemoPill) return null

  return (
    <div
      className="pointer-events-none fixed right-4 bottom-4 z-[200] flex flex-col items-end gap-2"
      aria-live="polite"
    >
      <AnimatePresence>
        {entries.map((entry) => (
          <motion.div
            key={entry.id}
            initial={reduceMotion ? false : { opacity: 0, x: 18, scale: 0.96 }}
            animate={{ opacity: 1, x: 0, scale: 1 }}
            exit={reduceMotion ? { opacity: 0 } : { opacity: 0, x: 18, scale: 0.96 }}
            transition={{ duration: reduceMotion ? 0.001 : 0.22, ease: [0.16, 1, 0.3, 1] }}
            className={clsx(
              'glass pointer-events-auto flex max-w-80 items-center gap-2 rounded-md py-2 pr-3 pl-2.5 text-[12.5px] leading-snug',
              entry.tone === 'danger' ? 'text-danger' : 'text-warning',
            )}
          >
            <StatusDot tone={entry.tone} />
            <span className="text-ink-strong">{entry.text}</span>
          </motion.div>
        ))}
      </AnimatePresence>

      {showDemoPill ? (
        <motion.div
          initial={reduceMotion ? false : { opacity: 0, x: 18, scale: 0.96 }}
          animate={{ opacity: 1, x: 0, scale: 1 }}
          transition={{ duration: reduceMotion ? 0.001 : 0.22, ease: [0.16, 1, 0.3, 1] }}
          className="glass pointer-events-auto flex items-center gap-2 rounded-md py-2 pr-3 pl-2.5"
        >
          <span className="relative flex size-2">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-60 motion-reduce:animate-none" />
            <span className="relative inline-flex size-2 rounded-full bg-accent" />
          </span>
          <span className="caps text-hud text-ink-muted">Modo demonstração</span>
        </motion.div>
      ) : null}
    </div>
  )
}

function StatusDot({ tone }: { tone: 'warning' | 'danger' }) {
  return (
    <span
      className={clsx(
        'inline-flex size-2 shrink-0 rounded-full',
        tone === 'danger' ? 'bg-danger shadow-[0_0_8px_-1px_var(--color-danger)]' : 'bg-warning shadow-[0_0_8px_-1px_var(--color-warning)]',
      )}
      aria-hidden="true"
    />
  )
}
