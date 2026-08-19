import { clsx } from 'clsx'
import type { CardView, GameView, ObjectId, PlayerView } from '../../types/protocol'
import { defenderObject, defenderPlayer } from '../../types/protocol'
import { Card } from '../card/Card'
import { PhaseTrack } from './PhaseTrack'
import { StackPanel } from './StackPanel'

/**
 * A mesa de TRÊS e QUATRO jogadores.
 *
 * `BoardLayout` desenha dois tapetes de verdade, com a geometria serigrafada e
 * o pareamento de combate por coluna — e essa geometria é de duelo por
 * construção (dois mats, uma costura entre eles). Em vez de esticá-la para
 * quatro assentos, esta é uma segunda vista, deliberadamente simples: um painel
 * por jogador, em grade, com o que precisa ser LEGÍVEL numa partida
 * multiplayer — vida, contagens de zona, campo de batalha e quem está atacando
 * quem.
 *
 * **Isto ainda não é o acabamento visual.** Não há tapete, nem perspectiva, nem
 * setas de alvo; o polimento da mesa de três e quatro é trabalho separado. O
 * que esta vista garante é correção: nenhum jogador some, nenhuma vida fica
 * fora da tela e o combate é lido pelo nome do defensor, não pela posição.
 */
export function TableLayout({
  view,
  cards,
}: {
  view: GameView
  cards: Record<ObjectId, CardView>
}) {
  const attacks = collectAttacks(view, cards)

  return (
    <div className="flex h-full w-full flex-col gap-3 p-4">
      <div className="grid min-h-0 flex-1 grid-cols-2 gap-3">
        {view.players.map((player) => (
          <SeatPanel
            key={player.id}
            player={player}
            active={player.id === view.activePlayer}
            permanents={view.battlefield[player.id] ?? []}
            cards={cards}
            incoming={attacks.filter((attack) => attack.defenderPlayer === player.id)}
          />
        ))}
      </div>

      <CombatStrip attacks={attacks} players={view.players} />

      <div className="flex flex-wrap items-center gap-3">
        <PhaseTrack
          turn={view.turn}
          step={view.step}
          activePlayer={view.activePlayer}
          players={view.players}
        />
        <StackPanel stack={view.stack} cards={cards} players={view.players} />
      </div>
    </div>
  )
}

/** Um ataque declarado, já resolvido para nomes legíveis. */
interface Attack {
  attacker: CardView
  /** `null` quando o ataque é a um planeswalker ou batalha, não a um jogador. */
  defenderPlayer: number | null
  defenderObject: ObjectId | null
  blockers: readonly CardView[]
}

function collectAttacks(view: GameView, cards: Record<ObjectId, CardView>): Attack[] {
  const out: Attack[] = []
  for (const card of view.cards) {
    if (card.attacking === null) continue
    out.push({
      attacker: card,
      defenderPlayer: defenderPlayer(card.attacking),
      defenderObject: defenderObject(card.attacking),
      blockers: card.blockedBy
        .map((id) => cards[id])
        .filter((blocker): blocker is CardView => blocker !== undefined),
    })
  }
  return out
}

function SeatPanel({
  player,
  active,
  permanents,
  cards,
  incoming,
}: {
  player: PlayerView
  active: boolean
  permanents: readonly ObjectId[]
  cards: Record<ObjectId, CardView>
  incoming: readonly Attack[]
}) {
  const board = permanents
    .map((id) => cards[id])
    .filter((card): card is CardView => card !== undefined)

  return (
    <section
      data-seat={player.id % 2}
      className={clsx(
        'metal-well flex min-h-0 flex-col gap-2 rounded-lg p-3',
        active ? 'ring-1 ring-accent/60' : '',
        player.hasLost ? 'opacity-45' : '',
      )}
      aria-label={`Jogador ${player.id + 1}: ${player.name}`}
    >
      <header className="flex items-center gap-3">
        <span className="metal-plate accent-ring numeral grid size-8 shrink-0 place-items-center rounded-full text-[14px] text-accent-bright">
          {player.id + 1}
        </span>
        <span className="min-w-0 flex-1 truncate text-[14px] text-ink-strong">{player.name}</span>

        {player.hasLost ? (
          <span className="caps text-[10px] tracking-wide-caps text-danger">eliminado</span>
        ) : active ? (
          <span className="caps text-[10px] tracking-wide-caps text-accent-bright">turno</span>
        ) : null}

        {incoming.length > 0 ? (
          <span className="caps text-[10px] tracking-wide-caps text-danger">
            sob ataque ({incoming.length})
          </span>
        ) : null}

        <span
          className="font-display text-[30px] leading-none tabular-nums text-life"
          title="Pontos de vida"
        >
          {player.life}
        </span>
        {player.poison > 0 ? (
          <span className="text-[12px] tabular-nums text-poison" title="Contadores de veneno">
            ☠ {player.poison}
          </span>
        ) : null}
      </header>

      <p className="caps text-hud text-ink-faint">
        mão {player.handCount} · deck {player.libraryCount} · cemitério {player.graveyardCount} ·
        exílio {player.exileCount}
      </p>

      <div className="flex min-h-0 flex-1 flex-wrap content-start gap-1.5 overflow-y-auto">
        {board.length === 0 ? (
          <p className="text-[12px] text-ink-faint">campo vazio</p>
        ) : (
          board.map((card) => (
            // `shrink-0`: sem ele o flex comprime a largura da carta e as artes
            // se sobrepõem — o `Card` deriva tudo de `--cw`, então encolher a
            // caixa não encolhe o desenho.
            <Card
              key={card.id}
              card={card}
              size="small"
              detailOnHover={false}
              className="shrink-0"
            />
          ))
        )}
      </div>
    </section>
  )
}

/** Quem ataca quem, em texto — a leitura de combate que a grade não dá. */
function CombatStrip({
  attacks,
  players,
}: {
  attacks: readonly Attack[]
  players: readonly PlayerView[]
}) {
  if (attacks.length === 0) return null

  const nameOf = (id: number | null): string =>
    players.find((player) => player.id === id)?.name ?? 'planeswalker/batalha'

  return (
    <div className="metal-well rounded-lg px-3 py-2">
      <p className="caps text-hud text-ink-muted">Combate</p>
      <ul className="mt-1 flex flex-wrap gap-x-5 gap-y-1">
        {attacks.map((attack) => (
          <li key={attack.attacker.id} className="text-[12.5px] text-ink">
            <span className="text-ink-strong">{attack.attacker.name ?? 'criatura'}</span>
            <span className="text-ink-faint">
              {' '}
              {attack.attacker.power ?? '?'}/{attack.attacker.toughness ?? '?'}{' '}
            </span>
            → {attack.defenderObject !== null ? 'permanente' : nameOf(attack.defenderPlayer)}
            {attack.blockers.length > 0 ? (
              <span className="text-ink-muted">
                {' '}
                (bloqueado por {attack.blockers.map((b) => b.name ?? 'criatura').join(', ')})
              </span>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  )
}
