import { useEffect } from 'react'
import { BoardLayout } from './components/board/BoardLayout'
import { Hud } from './components/hud/Hud'
import { MatchSetup } from './components/hud/MatchSetup'
import { StatsPanel } from './components/hud/StatsPanel'
import { FxLayer } from './fx/FxLayer'
import { useMatchStore } from './state/matchStore'
import './App.css'

export default function App() {
  const activePlayer = useMatchStore((s) => s.view?.activePlayer ?? 0)
  const teardown = useMatchStore((s) => s.teardown)

  // Isto é um simulador: ninguém joga, mas alguém escolhe o confronto. Quem
  // dispara `start` é a tela de abertura; daí em diante o HUD assume.
  useEffect(() => teardown, [teardown])

  return (
    <div className="app-shell" data-active-player={activePlayer}>
      <div className="app-shell__table table-felt" data-fx-shake>
        <BoardLayout />
      </div>
      <Hud />
      <StatsPanel className="fixed top-24 right-4 z-[90] w-[300px]" />
      <FxLayer />
      <MatchSetup />
    </div>
  )
}
