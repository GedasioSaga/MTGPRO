import { useMemo } from 'react'
import { useMatchStore } from '../../state/matchStore'
import { usePlaymatStore } from '../../state/playmatStore'
import type { CardView, GameView, ObjectId, PlayerView } from '../../types/protocol'
import { ArenaFrame } from './ArenaFrame'
import { BattlefieldRow } from './BattlefieldRow'
import { LibraryIcon } from './BoardIcons'
import { ExilePile } from './ExilePile'
import { GraveyardPile } from './GraveyardPile'
import { HandRow } from './HandRow'
import { MatZone, asSeat } from './MatZone'
import { PhaseTrack } from './PhaseTrack'
import { PlayerBar, PlayerLife } from './PlayerBar'
import { Playmat } from './Playmat'
import { StackPanel } from './StackPanel'
import { TargetArrows } from './TargetArrows'
import { CombatSeam } from './CombatSeam'
import { ZonePile } from './ZonePile'
import { buildCombatPlan, totalSlots } from './combatPlan'
import type { CombatPlan } from './combatPlan'
import { cssVars, useBoardScale } from './boardVisuals'

/** Largura de uma coluna de combate a 1920x1080, antes de `--board-scale`. */
const LANE_WIDTH = 168
/** Largura útil da zona CAMPO DE BATALHA impressa, para as raias caberem nela. */
const RAIL_BUDGET = 1000

/**
 * A mesa: um tampo com DOIS PLAYMATS apoiados nele, um por jogador.
 *
 * O tabuleiro deixou de ser uma grade de caixas e virou o objeto físico que se
 * usa numa partida de verdade. As zonas não são mais sinalizadas pela interface
 * — elas estão SERIGRAFADAS no tapete, e cada jogador escolhe a arte do dele.
 * Isso resolve os dois becos das rodadas anteriores de uma vez: linha impressa
 * não lê como painel de controle, porque não é interface, é o mat; e a arte do
 * mat preenche a mesa sem precisar de asset pintado próprio.
 *
 * Consequência de arquitetura: quem manda na posição da carta passou a ser a
 * geometria de `playmatZones`, não o grid do CSS. O grid só empilha as bandas
 * (borda, mão, mat, costura, mat, mão, borda); dentro do mat, cada carta é
 * ancorada na mesma caixa normalizada de onde `Playmat` tira o contorno — então
 * carta e linha impressa coincidem por construção, sem medição.
 *
 * O espelhamento do lado de cima é na GEOMETRIA do mat, nunca em rotação de
 * elemento: o tapete do oponente está girado 180° como na mesa, mas o texto
 * continua em pé, porque quem assiste lê a tela sempre na mesma orientação.
 */
export function BoardLayout() {
  const view = useMatchStore((s) => s.view)
  const cards = useMatchStore((s) => s.cards)
  const connection = useMatchStore((s) => s.connection)
  const scale = useBoardScale()
  const plan = useMemo(() => buildCombatPlan(cards), [cards])

  if (view === null || view.players.length < 2) {
    return <TableStandby connecting={connection === 'connecting'} />
  }

  const bottom: PlayerView = view.players[0]
  const top: PlayerView = view.players[1]

  // O atacante é quem NÃO está defendendo. Sem defensor conhecido (ataque a
  // planeswalker) o atacante ainda é o jogador do turno.
  const attackerSide: 'top' | 'bottom' =
    plan.defender !== null
      ? plan.defender === top.id
        ? 'bottom'
        : 'top'
      : view.activePlayer === bottom.id
        ? 'bottom'
        : 'top'

  const damageTo = (id: PlayerView['id']): number =>
    plan.defender === id ? plan.breachDamage : 0

  const slots = Math.max(1, totalSlots(plan))
  const lane = Math.min(LANE_WIDTH, Math.floor(RAIL_BUDGET / slots))

  return (
    <div
      className="board-grid"
      data-active-player={view.activePlayer}
      data-combat={plan.active ? 'true' : 'false'}
      style={cssVars({
        '--board-scale': String(scale),
        '--combat-col': `calc(${lane}px * var(--board-scale, 1))`,
      })}
    >
      {/* O móvel embaixo dos dois tapetes. Discreto de propósito: o objeto que
          o olho deve encontrar primeiro agora é o mat. */}
      <ArenaFrame />

      <div className="board-area board-area--foe-strip" data-seat={top.id}>
        <PlayerBar player={top} side="top" underFire={damageTo(top.id)} />
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

      <SeatMat player={top} side="top" view={view} cards={cards} plan={plan} />

      <div className="board-area board-area--seam" aria-hidden="true">
        <CombatSeam plan={plan} cards={cards} attackerSide={attackerSide} />
      </div>

      <SeatMat player={bottom} side="bottom" view={view} cards={cards} plan={plan} />

      <div className="board-area board-area--own-hand">
        <HandRow
          player={bottom.id}
          ids={view.hands[bottom.id] ?? []}
          cards={cards}
          side="bottom"
          count={bottom.handCount}
        />
      </div>

      <div className="board-area board-area--own-strip" data-seat={bottom.id}>
        <PlayerBar player={bottom} side="bottom" underFire={damageTo(bottom.id)} />
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

      <TargetArrows view={view} plan={plan} />
    </div>
  )
}

interface SeatMatProps {
  player: PlayerView
  side: 'top' | 'bottom'
  view: GameView
  cards: Record<ObjectId, CardView>
  plan: CombatPlan
}

/**
 * Um tapete e tudo que pousa nele.
 *
 * `Playmat` imprime as linhas; os filhos seguintes são as cartas de verdade,
 * ancoradas nas mesmas caixas. O tapete é o bloco de contenção dos dois, e é
 * essa coincidência — e não um número copiado — que garante o alinhamento.
 */
function SeatMat({ player, side, view, cards, plan }: SeatMatProps) {
  const seat = asSeat(player.id)
  // A arte é escolha do jogador na tela de abertura; o tapete só a consome.
  const artUrl = usePlaymatStore((s) => s.art[seat])
  const deckColors = usePlaymatStore((s) => s.tint[seat])

  return (
    <div className={`board-area board-area--mat board-area--mat-${side}`}>
      <div className="playmat-seat" data-seat={player.id} data-side={side}>
        {/* DOIS planos com a MESMA inclinação, e não um só.
            Num contexto 3D único o tapete e as cartas se cruzam em
            profundidade, e a metade do feltro que vem para a frente do
            observador passa a ser pintada POR CIMA das criaturas — medido: a
            faixa de batalha de baixo sumia inteira atrás do mat. Separando em
            dois irmãos achatados, a ordem de pintura volta a ser o `z-index`,
            que é determinístico. */}
        <div className="mat-plane mat-plane--print">
          <Playmat seat={seat} artUrl={artUrl} deckColors={deckColors} />
        </div>

        <div className="mat-plane mat-plane--live">
          <BattlefieldRow
            player={player.id}
            seat={seat}
            permanents={view.battlefield[player.id] ?? []}
            cards={cards}
            plan={plan}
          />

          <MatZone seat={seat} zone="library" className="mat-zone--pile">
            <ZonePile
              label="deck"
              ids={[]}
              cards={cards}
              count={player.libraryCount}
              tone="library"
              side={side}
              icon={<LibraryIcon className="size-5" />}
            />
          </MatZone>

          <MatZone seat={seat} zone="graveyard" className="mat-zone--pile">
            <GraveyardPile
              ids={view.graveyards[player.id] ?? []}
              cards={cards}
              count={player.graveyardCount}
              side={side}
            />
          </MatZone>

          <MatZone seat={seat} zone="exile" className="mat-zone--pile">
            <ExilePile
              ids={view.exiles[player.id] ?? []}
              cards={cards}
              count={player.exileCount}
              side={side}
            />
          </MatZone>

          <MatZone seat={seat} zone="life" className="mat-zone--life">
            <PlayerLife player={player} />
          </MatZone>
        </div>
      </div>
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
