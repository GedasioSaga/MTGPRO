import { useEffect, useMemo, useRef } from 'react'
import { useMatchStore } from '../state/matchStore'
import type { MatchEvent, ObjectId } from '../types/protocol'
import { forgetRects } from './fxAnchors'
import type { FxEffect } from './fxTypes'
import { useFxEngine } from './useFxEngine'
import type { FxTranslateContext } from './translateEvent'

/**
 * Cola entre a fila de eventos do `matchStore` e a camada de efeitos.
 *
 * Cada `MatchEvent` e um objeto novo vindo do socket, entao um `WeakSet` de
 * referencias ja vistas garante consumo exatamente uma vez — funcione a store
 * empurrando na fila ou drenando um evento por vez em `currentEvent`.
 */
export function useEventChoreographer(): FxEffect[] {
  const view = useMatchStore((s) => s.view)
  const cards = useMatchStore((s) => s.cards)
  const eventQueue = useMatchStore((s) => s.eventQueue)
  const currentEvent = useMatchStore((s) => s.currentEvent)
  const connection = useMatchStore((s) => s.connection)

  const ctx = useMemo<FxTranslateContext>(
    () => ({
      view,
      card: (id: ObjectId) => cards[id] ?? view?.cards.find((c) => c.id === id),
    }),
    [view, cards],
  )

  const { effects, ingest, clear } = useFxEngine(ctx)

  const seen = useRef<WeakSet<object>>(new WeakSet())
  // A store pode expor a fila inteira ou soltar um evento por vez. Assim que
  // `currentEvent` aparece uma vez, ele vira a fonte — animar pela fila crua
  // dispararia a partida toda de uma vez so.
  const drivenByCurrent = useRef(false)

  useEffect(() => {
    if (currentEvent !== null) drivenByCurrent.current = true
    const fresh: MatchEvent[] = []
    const take = (event: MatchEvent | null): void => {
      if (event === null || seen.current.has(event)) return
      seen.current.add(event)
      fresh.push(event)
    }
    if (drivenByCurrent.current) {
      take(currentEvent)
    } else {
      for (const event of eventQueue) take(event)
    }
    ingest(fresh)
  }, [eventQueue, currentEvent, ingest])

  useEffect(() => {
    if (connection !== 'connecting') return
    // Partida nova: nenhum efeito nem retangulo memorizado da anterior sobrevive.
    seen.current = new WeakSet()
    drivenByCurrent.current = false
    forgetRects()
    clear()
  }, [connection, clear])

  return effects
}
