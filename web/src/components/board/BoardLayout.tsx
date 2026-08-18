import { useMatchStore } from '../../state/matchStore'
import type { PlayerView } from '../../types/protocol'
import { BattlefieldRow } from './BattlefieldRow'
import { ExilePile } from './ExilePile'
import { GraveyardPile } from './GraveyardPile'
import { HandRow } from './HandRow'
import { PhaseTrack } from './PhaseTrack'
import { PlayerBar } from './PlayerBar'
import { StackPanel } from './StackPanel'
import { TargetArrows } from './TargetArrows'
import { cssVars, useBoardScale } from './boardVisuals'

/**
 * A mesa inteira.
 *
 * Do topo para a base: borda do oponente (nome + vida), mão dele, o campo
 * dele, a costura central, o campo de baixo, a mão de baixo, a borda de baixo
 * e o rodapé de telemetria (fases + pilha). Cada campo é um par de zonas
 * DESENHADAS na mesa — batalha e terrenos —, visíveis mesmo vazias, porque
 * mesa sem zona marcada é fundo, não tabuleiro.
 *
 * O espelhamento do lado de cima é na ORDEM das fileiras, nunca em rotação de
 * carta. O tamanho de tudo sai de `--board-scale` (ver `useBoardScale`), então
 * 1920x1080 e 1280x800 são a mesma composição em dois tamanhos.
 */
export function BoardLayout() {
  const view = useMatchStore((s) => s.view)
  const cards = useMatchStore((s) => s.cards)
  const connection = useMatchStore((s) => s.connection)
  const scale = useBoardScale()

  if (view === null || view.players.length < 2) {
    return <TableStandby connecting={connection === 'connecting'} />
  }

  const bottom: PlayerView = view.players[0]
  const top: PlayerView = view.players[1]

  return (
    <div
      className="board-grid"
      data-active-player={view.activePlayer}
      style={cssVars({ '--board-scale': String(scale) })}
    >
      <div className="board-area board-area--foe-strip" data-seat={top.id}>
        <PlayerBar player={top} side="top" />
      </div>

      <div className="board-area board-area--foe-hand">
        <HandRow
          player={top.id}
          ids={view.hands[top.id] ?? []}
          cards={cards}
          side="top"
          count={top.handCount}
        />
      </div>

      <div className="board-area board-area--foe-zones">
        <ExilePile
          ids={view.exiles[top.id] ?? []}
          cards={cards}
          count={top.exileCount}
          side="top"
        />
        <GraveyardPile
          ids={view.graveyards[top.id] ?? []}
          cards={cards}
          count={top.graveyardCount}
          side="top"
        />
      </div>

      <div className="board-area board-area--foe-field" data-seat={top.id}>
        <BattlefieldRow
          player={top.id}
          side="top"
          permanents={view.battlefield[top.id] ?? []}
          cards={cards}
        />
      </div>

      <div className="board-area board-area--seam" aria-hidden="true" />

      <div className="board-area board-area--own-field" data-seat={bottom.id}>
        <BattlefieldRow
          player={bottom.id}
          side="bottom"
          permanents={view.battlefield[bottom.id] ?? []}
          cards={cards}
        />
      </div>

      <div className="board-area board-area--own-hand">
        <HandRow
          player={bottom.id}
          ids={view.hands[bottom.id] ?? []}
          cards={cards}
          side="bottom"
          count={bottom.handCount}
        />
      </div>

      <div className="board-area board-area--own-zones">
        <GraveyardPile
          ids={view.graveyards[bottom.id] ?? []}
          cards={cards}
          count={bottom.graveyardCount}
          side="bottom"
        />
        <ExilePile
          ids={view.exiles[bottom.id] ?? []}
          cards={cards}
          count={bottom.exileCount}
          side="bottom"
        />
      </div>

      <div className="board-area board-area--own-strip" data-seat={bottom.id}>
        <PlayerBar player={bottom} side="bottom" />
      </div>

      {/* Rodapé: motor da partida, não manchete. Fica rente à borda de baixo,
          num tom abaixo da mesa, para o olho passar por ele só quando procura. */}
      <div className="board-area board-area--footer">
        <PhaseTrack
          turn={view.turn}
          step={view.step}
          activePlayer={view.activePlayer}
          players={view.players}
        />
        <StackPanel stack={view.stack} cards={cards} players={view.players} />
      </div>

      <TargetArrows view={view} cards={cards} />
    </div>
  )
}

/** Mesa vazia enquanto o servidor não mandou a primeira view. */
function TableStandby({ connecting }: { connecting: boolean }) {
  return (
    <div className="board-standby">
      <div className="board-standby__mark" aria-hidden="true" />
      <p className="board-standby__text">
        {connecting ? 'embaralhando decks…' : 'mesa livre'}
      </p>
    </div>
  )
}
