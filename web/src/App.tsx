import { useEffect } from 'react'
import { BoardLayout } from './components/board/BoardLayout'
import { Hud } from './components/hud/Hud'
import { FxLayer } from './fx/FxLayer'
import { fetchDecks } from './net/api'
import { useMatchStore } from './state/matchStore'
import './App.css'

/** Ids do `mtg-server` (slug do nome do deck); só valem se a API não responder. */
const FALLBACK_DECKS: readonly [string, string] = ['goblin-onslaught', 'azorius-control']
const DEFAULT_SPEED = 1

export default function App() {
  const activePlayer = useMatchStore((s) => s.view?.activePlayer ?? 0)
  const start = useMatchStore((s) => s.start)
  const teardown = useMatchStore((s) => s.teardown)

  useEffect(() => {
    const controller = new AbortController()
    let cancelled = false

    // Isto é um simulador: ninguém joga, então a mesa não espera clique nenhum
    // para começar. O HUD assume o controle da partida a partir daqui.
    const boot = async (): Promise<void> => {
      let deckA = FALLBACK_DECKS[0]
      let deckB = FALLBACK_DECKS[1]
      try {
        const decks = await fetchDecks(controller.signal)
        if (decks.length >= 2) {
          deckA = decks[0].id
          deckB = decks[1].id
        }
      } catch {
        // Servidor fora do ar: o socket cai sozinho no replay de demonstração.
      }
      if (cancelled) return
      start(deckA, deckB, Math.floor(Math.random() * 0x7fffffff), DEFAULT_SPEED)
    }

    void boot()

    return () => {
      cancelled = true
      controller.abort()
      teardown()
    }
  }, [start, teardown])

  return (
    <div className="app-shell" data-active-player={activePlayer}>
      <div className="app-shell__table table-felt" data-fx-shake>
        <BoardLayout />
      </div>
      <Hud />
      <FxLayer />
    </div>
  )
}
