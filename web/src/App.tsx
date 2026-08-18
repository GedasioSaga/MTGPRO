import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { BoardLayout } from './components/board/BoardLayout'
import { MaterialDefs } from './design/materials'
import { GameLog } from './components/hud/GameLog'
import { Hud } from './components/hud/Hud'
import { MatchSetup } from './components/hud/MatchSetup'
import { StatsPanel } from './components/hud/StatsPanel'
import { FxLayer } from './fx/FxLayer'
import { useMatchStore } from './state/matchStore'
import './App.css'

/** Tecla que abre/fecha cada painel. Espaço, seta e 1-4 já são da PlaybackBar. */
const LOG_KEY = 'r'
const STATS_KEY = 'l'

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
  )
}

export default function App() {
  const activePlayer = useMatchStore((s) => s.view?.activePlayer ?? 0)
  const teardown = useMatchStore((s) => s.teardown)
  // Fechados por padrão: a mesa é o espetáculo, telemetria é consulta.
  const [logOpen, setLogOpen] = useState(false)
  const [statsOpen, setStatsOpen] = useState(false)

  // Isto é um simulador: ninguém joga, mas alguém escolhe o confronto. Quem
  // dispara `start` é a tela de abertura; daí em diante o HUD assume.
  useEffect(() => teardown, [teardown])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (event.ctrlKey || event.metaKey || event.altKey) return
      if (isTyping(event.target)) return

      if (event.key === 'Escape') {
        setLogOpen(false)
        setStatsOpen(false)
        return
      }
      const key = event.key.toLowerCase()
      if (key === LOG_KEY) setLogOpen((value) => !value)
      else if (key === STATS_KEY) setStatsOpen((value) => !value)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <div className="app-shell" data-active-player={activePlayer}>
      {/* Biblioteca de filtros: ids globais, montada UMA vez e antes de todo
          consumidor — `filter="url(#…)"` so resolve o que ja existe no DOM. */}
      <MaterialDefs />

      <div className="app-shell__table table-felt" data-fx-shake>
        <BoardLayout />
      </div>

      <Rail
        side="left"
        title="Registro"
        hotkey={LOG_KEY}
        open={logOpen}
        onToggle={() => setLogOpen((value) => !value)}
      >
        <GameLog />
      </Rail>

      <Rail
        side="right"
        title="Leitura"
        hotkey={STATS_KEY}
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
  hotkey: string
  open: boolean
  onToggle: () => void
  children: ReactNode
}

/**
 * Painel de telemetria em overlay.
 *
 * Deixou de ser coluna do grid: fechado, devolve a largura INTEIRA à mesa e
 * some numa aba fina na borda; aberto, desliza por cima do tabuleiro com fundo
 * translúcido. A mesa nunca reflui — o que muda é só o que está por cima dela.
 */
function Rail({ side, title, hotkey, open, onToggle, children }: RailProps) {
  const id = `rail-${side}`
  const label = open ? `Fechar ${title.toLowerCase()}` : `Abrir ${title.toLowerCase()}`

  return (
    <>
      <button
        type="button"
        className="app-rail-tab"
        data-side={side}
        data-open={open}
        aria-expanded={open}
        aria-controls={id}
        title={`${label} (${hotkey.toUpperCase()})`}
        onClick={onToggle}
      >
        <span className="app-rail-tab__text">{title}</span>
        <span className="app-rail-tab__key" aria-hidden="true">
          {hotkey.toUpperCase()}
        </span>
      </button>

      <aside
        className="app-rail"
        id={id}
        data-side={side}
        data-open={open}
        aria-label={title}
        inert={!open}
      >
        <div className="app-rail__head">
          <span className="app-rail__title">{title}</span>
          <button type="button" className="app-rail__close" onClick={onToggle} title={label}>
            {side === 'left' ? '‹' : '›'}
          </button>
        </div>
        <div className="app-rail__body">{children}</div>
      </aside>
    </>
  )
}
