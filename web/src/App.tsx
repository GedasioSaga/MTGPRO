import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { BoardLayout } from './components/board/BoardLayout'
import { GameLog } from './components/hud/GameLog'
import { Hud } from './components/hud/Hud'
import { MatchSetup } from './components/hud/MatchSetup'
import { StatsPanel } from './components/hud/StatsPanel'
import { FxLayer } from './fx/FxLayer'
import { useMatchStore } from './state/matchStore'
import './App.css'

/** Abaixo disto as duas colunas laterais roubam largura demais da mesa. */
const RAIL_BREAKPOINT_PX = 1440

/** `true` enquanto a janela for estreita demais para mesa + duas colunas. */
function useNarrowViewport(): boolean {
  const [narrow, setNarrow] = useState(
    () =>
      typeof window !== 'undefined' &&
      window.matchMedia(`(max-width: ${RAIL_BREAKPOINT_PX - 1}px)`).matches,
  )

  useEffect(() => {
    const query = window.matchMedia(`(max-width: ${RAIL_BREAKPOINT_PX - 1}px)`)
    const update = (): void => setNarrow(query.matches)
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  }, [])

  return narrow
}

export default function App() {
  const activePlayer = useMatchStore((s) => s.view?.activePlayer ?? 0)
  const teardown = useMatchStore((s) => s.teardown)
  const narrow = useNarrowViewport()
  const [logOpen, setLogOpen] = useState(true)
  const [statsOpen, setStatsOpen] = useState(true)

  // Isto é um simulador: ninguém joga, mas alguém escolhe o confronto. Quem
  // dispara `start` é a tela de abertura; daí em diante o HUD assume.
  useEffect(() => teardown, [teardown])

  // Em tela estreita as colunas recolhem sozinhas; o usuário ainda pode abrir.
  useEffect(() => {
    setLogOpen(!narrow)
    setStatsOpen(!narrow)
  }, [narrow])

  return (
    <div className="app-shell" data-active-player={activePlayer}>
      <Rail
        side="left"
        title="Registro"
        open={logOpen}
        onToggle={() => setLogOpen((value) => !value)}
      >
        <GameLog />
      </Rail>

      <div className="app-shell__table table-felt" data-fx-shake>
        <BoardLayout />
      </div>

      <Rail
        side="right"
        title="Leitura"
        open={statsOpen}
        onToggle={() => setStatsOpen((value) => !value)}
      >
        <StatsPanel className="app-rail__card" />
      </Rail>

      <Hud />
      <FxLayer />
      <MatchSetup />
    </div>
  )
}

interface RailProps {
  side: 'left' | 'right'
  title: string
  open: boolean
  onToggle: () => void
  children: ReactNode
}

function pointsInward(side: 'left' | 'right', open: boolean): boolean {
  return side === 'left' ? open : !open
}

/**
 * Coluna lateral da moldura. É coluna do grid, não overlay: o que estiver aqui
 * nunca cobre a mesa, e recolher devolve a largura para o tabuleiro.
 */
function Rail({ side, title, open, onToggle, children }: RailProps) {
  const id = `rail-${side}`
  return (
    <aside className="app-rail" data-side={side} data-open={open}>
      <button
        type="button"
        className="app-rail__toggle"
        aria-expanded={open}
        aria-controls={id}
        title={open ? `Recolher ${title.toLowerCase()}` : `Abrir ${title.toLowerCase()}`}
        onClick={onToggle}
      >
        <span className="app-rail__toggle-text">{title}</span>
        <span aria-hidden="true">{pointsInward(side, open) ? '‹' : '›'}</span>
      </button>
      <div className="app-rail__body" id={id} hidden={!open}>
        {children}
      </div>
    </aside>
  )
}
