import { clsx } from 'clsx'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { fetchCardCatalog, fetchDecks, fetchFormats } from '../../net/api'
import type { DeckInfo, FormatInfo } from '../../net/api'
import { MOCK_CARDS, toCardDef } from '../../mock/mockCards'
import { useMatchStore } from '../../state/matchStore'
import type { CardDef, SeatFrame } from '../../types/protocol'
import { CardBrowser } from './CardBrowser'
import { Divider } from '../ui/Divider'
import { IconButton } from '../ui/IconButton'
import { PlaymatPicker } from './PlaymatPicker'

/**
 * Os decks de `crates/mtg-cards/src/decks.rs`, com o id no formato que o
 * servidor gera (slug do nome). Só entram em cena se `GET /api/decks` falhar —
 * sem eles a tela de abertura ficaria vazia justamente quando o motor está fora
 * do ar e a partida vai cair no replay de demonstração.
 */
const FALLBACK_DECKS: readonly DeckInfo[] = [
  { id: 'goblin-onslaught', name: 'Goblin Onslaught', colorIdentity: ['R'], format: 'modern' },
  { id: 'azorius-control', name: 'Azorius Control', colorIdentity: ['W', 'U'], format: 'casual' },
  { id: 'selesnya-valor', name: 'Selesnya Valor', colorIdentity: ['W', 'G'], format: 'modern' },
  { id: 'gruul-stampede', name: 'Gruul Stampede', colorIdentity: ['R', 'G'], format: 'modern' },
  {
    id: 'conclave-of-emmara',
    name: 'Conclave of Emmara',
    colorIdentity: ['W', 'G'],
    format: 'commander',
    commander: 'Emmara, Soul of the Accord',
  },
  {
    id: 'storm-of-adeliz',
    name: 'Storm of Adeliz',
    colorIdentity: ['U', 'R'],
    format: 'commander',
    commander: 'Adeliz, the Cinder Wind',
  },
]

/**
 * Formatos embutidos, na ordem em que a tela os lista. Espelham
 * `Format::ALL` de `mtg-format`; `GET /api/formats` sobrescreve assim que
 * responde, e é ele quem traz a contagem de decks por formato.
 *
 * `casual` aparece como "Duelo" porque é isso que ele é aqui: dois jogadores,
 * sem lista de banidos. Só Commander é multiplayer (CR 903).
 */
const FALLBACK_FORMATS: readonly FormatInfo[] = [
  fallbackFormat('casual', 'Duelo', 40, 2),
  fallbackFormat('commander', 'Commander', 100, 4),
  fallbackFormat('standard', 'Standard', 60, 2),
  fallbackFormat('modern', 'Modern', 60, 2),
  fallbackFormat('pauper', 'Pauper', 60, 2),
]

function fallbackFormat(
  slug: string,
  name: string,
  minDeckSize: number,
  maxPlayers: number,
): FormatInfo {
  return {
    slug,
    name,
    minDeckSize,
    exactDeckSize: slug === 'commander' ? 100 : null,
    maxCopies: null,
    requiresCommander: slug === 'commander',
    minPlayers: 2,
    maxPlayers,
    deckCount: 0,
  }
}

/** Rótulo de tela por slug. O servidor manda "Casual"; a mesa chama de duelo. */
const FORMAT_LABEL: Readonly<Record<string, string>> = { casual: 'Duelo' }

/** Ordem de exibição dos formatos, independente da ordem que o servidor mande. */
const FORMAT_ORDER: readonly string[] = ['casual', 'commander', 'standard', 'modern', 'pauper']

/** Catálogo de reserva quando `GET /api/cards` não responde. */
const FALLBACK_CATALOG: readonly CardDef[] = Object.values(MOCK_CARDS).map(toCardDef)

type BotKind = 'random' | 'heuristic' | 'greedy'

const BOTS: readonly { kind: BotKind; label: string; blurb: string }[] = [
  { kind: 'random', label: 'Aleatório', blurb: 'Sorteia entre as jogadas legais.' },
  { kind: 'heuristic', label: 'Heurístico', blurb: 'Pesa tabuleiro, curva e vida.' },
  { kind: 'greedy', label: 'Ganancioso', blurb: 'Busca o maior ganho imediato.' },
]

const SPEEDS: readonly number[] = [0.5, 1, 2, 4]

const MAX_SEED = 0x7fffffff
const MAX_SEATS = 4
const SEAT_NUMERAL: readonly string[] = ['I', 'II', 'III', 'IV']

/** Escolha de um assento. `commander` vazio = usa o da própria lista. */
interface SeatChoice {
  deck: string
  bot: BotKind
  commander: string
}

/** O que impede um assento de entrar na mesa, já em texto de tela. */
interface SeatProblem {
  seat: number
  deckName: string
  messages: readonly string[]
}

function randomSeed(): number {
  return Math.floor(Math.random() * MAX_SEED)
}

function formatLabel(format: FormatInfo): string {
  return FORMAT_LABEL[format.slug] ?? format.name
}

/** `true` quando a lista passa na validação do formato, ou quando não sabemos. */
function deckIsLegal(deck: DeckInfo, slug: string): boolean {
  return deck.legality?.[slug]?.legal ?? true
}

function deckViolations(deck: DeckInfo, slug: string): readonly string[] {
  return deck.legality?.[slug]?.violations ?? []
}

/**
 * Tela de abertura. Fica no lugar da mesa enquanto nenhuma partida começou
 * (`connection === 'idle'`) e some assim que `start` é chamado — quem controla
 * a partida a partir daí é o HUD.
 */
export function MatchSetup() {
  const connection = useMatchStore((s) => s.connection)
  const start = useMatchStore((s) => s.start)
  const serverError = useMatchStore((s) => s.error)
  const reduceMotion = useReducedMotion()

  const [decks, setDecks] = useState<readonly DeckInfo[]>(FALLBACK_DECKS)
  const [formats, setFormats] = useState<readonly FormatInfo[]>(FALLBACK_FORMATS)
  const [catalog, setCatalog] = useState<readonly CardDef[]>(FALLBACK_CATALOG)
  const [live, setLive] = useState(false)
  const [formatSlug, setFormatSlug] = useState('casual')
  const [seatCount, setSeatCount] = useState(2)
  const [seats, setSeats] = useState<readonly SeatChoice[]>(() =>
    FALLBACK_DECKS.slice(0, MAX_SEATS).map((deck) => ({
      deck: deck.id,
      bot: 'heuristic' as BotKind,
      commander: '',
    })),
  )
  const [seedText, setSeedText] = useState(() => String(randomSeed()))
  const [speed, setSpeed] = useState(1)
  // Navegacao do catalogo amplo (~32 mil cartas do Scryfall). Sobrepoe a tela
  // de abertura em vez de substitui-la: a escolha de baralho continua intacta
  // atras dela quando o usuario volta.
  const [browsing, setBrowsing] = useState(false)

  useEffect(() => {
    const controller = new AbortController()
    let cancelled = false

    const load = async (): Promise<void> => {
      try {
        const list = await fetchDecks(controller.signal)
        if (cancelled || list.length === 0) return
        setDecks(list)
        setLive(true)
        setSeats((current) =>
          current.map((seat, index) => ({
            ...seat,
            deck: (list[Math.min(index, list.length - 1)] ?? list[0]).id,
          })),
        )
      } catch {
        // Servidor fora do ar: a lista embutida sustenta a escolha, e o socket
        // cai sozinho no replay de demonstração quando a partida começar.
      }
    }

    const loadFormats = async (): Promise<void> => {
      try {
        const list = await fetchFormats(controller.signal)
        if (cancelled || list.length === 0) return
        setFormats(list)
      } catch {
        // Ficam os formatos embutidos.
      }
    }

    // O catálogo só alimenta as miniaturas do tapete: falhar aqui não pode
    // impedir a escolha dos baralhos, então vai numa promessa separada.
    const loadCatalog = async (): Promise<void> => {
      try {
        const cards = await fetchCardCatalog(controller.signal)
        if (cancelled || cards.length === 0) return
        setCatalog(cards)
      } catch {
        // Fica o catálogo embutido.
      }
    }

    void load()
    void loadFormats()
    void loadCatalog()
    return () => {
      cancelled = true
      controller.abort()
    }
  }, [])

  const ordered = useMemo(
    () =>
      [...formats].sort(
        (a, b) => orderOf(a.slug, FORMAT_ORDER) - orderOf(b.slug, FORMAT_ORDER),
      ),
    [formats],
  )

  const format = useMemo(
    () => ordered.find((entry) => entry.slug === formatSlug) ?? ordered[0],
    [ordered, formatSlug],
  )

  const activeSeats = useMemo(() => seats.slice(0, seatCount), [seats, seatCount])

  /**
   * Troca de formato reescreve a mesa: o número de jogadores volta para dentro
   * da faixa e cada assento que ficou com deck ilegal pega o primeiro legal.
   * Sem isso, mudar para Commander deixaria quatro decks de 60 na tela e o
   * botão de iniciar morto sem explicação óbvia.
   */
  const chooseFormat = useCallback(
    (next: FormatInfo) => {
      setFormatSlug(next.slug)
      setSeatCount((count) => Math.min(Math.max(count, next.minPlayers), next.maxPlayers))
      setSeats((current) =>
        current.map((seat) => {
          const chosen = decks.find((entry) => entry.id === seat.deck)
          if (chosen !== undefined && deckIsLegal(chosen, next.slug)) return seat
          const legal = decks.find((entry) => deckIsLegal(entry, next.slug))
          return legal === undefined ? seat : { ...seat, deck: legal.id, commander: '' }
        }),
      )
    },
    [decks],
  )

  const patchSeat = useCallback((index: number, patch: Partial<SeatChoice>) => {
    setSeats((current) =>
      current.map((seat, i) => (i === index ? { ...seat, ...patch } : seat)),
    )
  }, [])

  const problems = useMemo<readonly SeatProblem[]>(() => {
    const out: SeatProblem[] = []
    activeSeats.forEach((seat, index) => {
      const deck = decks.find((entry) => entry.id === seat.deck)
      if (deck === undefined) {
        out.push({ seat: index, deckName: seat.deck, messages: ['deck desconhecido'] })
        return
      }
      const messages = deckViolations(deck, formatSlug)
      if (messages.length > 0) out.push({ seat: index, deckName: deck.name, messages })
    })
    return out
  }, [activeSeats, decks, formatSlug])

  const blocked = problems.length > 0

  const handleSubmit = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      if (blocked) return
      const parsed = Number.parseInt(seedText, 10)
      const seed = Number.isFinite(parsed) && parsed >= 0 ? parsed % (MAX_SEED + 1) : randomSeed()
      const frames: SeatFrame[] = activeSeats.map((seat) => {
        const frame: SeatFrame = { deck: seat.deck, bot: seat.bot }
        if (seat.commander.length > 0) frame.commander = seat.commander
        return frame
      })
      start({ format: formatSlug, seats: frames, seed, speed })
    },
    [activeSeats, blocked, formatSlug, seedText, speed, start],
  )

  const fade = reduceMotion ? undefined : { duration: 0.26, ease: [0.16, 1, 0.3, 1] as const }
  const rise = (delay: number) =>
    reduceMotion ? undefined : { duration: 0.42, delay, ease: [0.16, 1, 0.3, 1] as const }

  const commanderNames = useMemo(
    () =>
      decks
        .map((deck) => deck.commander)
        .filter((name): name is string => typeof name === 'string' && name.length > 0),
    [decks],
  )

  return (
    <>
      <AnimatePresence>
        {connection === 'idle' ? (
          <motion.div
            key="match-setup"
            className="fixed inset-0 z-[200] grid place-items-center overflow-y-auto p-6"
            initial={reduceMotion ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 1.015 }}
            transition={fade}
          >
            <Backdrop />

            <motion.form
              onSubmit={handleSubmit}
              aria-label="Configuração da partida"
              className="glass relative w-full max-w-[1180px] rounded-2xl px-8 py-7 sm:px-10 [@media(max-height:900px)]:py-5"
              initial={reduceMotion ? false : { opacity: 0, y: 16 }}
              animate={{ opacity: 1, y: 0 }}
              transition={rise(0.04)}
            >
              <Poster live={live} format={format} seatCount={seatCount} />

              {serverError !== null ? <ServerRejection message={serverError} /> : null}

              <div className="mt-6 flex flex-wrap items-end gap-6">
                <FormatField
                  formats={ordered}
                  selected={formatSlug}
                  onSelect={chooseFormat}
                />
                <PlayerCountField
                  value={seatCount}
                  min={format.minPlayers}
                  max={format.maxPlayers}
                  onChange={setSeatCount}
                />
              </div>

              <div
                className={clsx(
                  'mt-6 grid items-start gap-5',
                  seatCount === 2
                    ? 'grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]'
                    : 'grid-cols-2',
                )}
              >
                {activeSeats.map((seat, index) => (
                  <SeatColumnWithDivider
                    key={index}
                    index={index}
                    seatCount={seatCount}
                    seat={seat}
                    decks={decks}
                    formatSlug={formatSlug}
                    requiresCommander={format.requiresCommander}
                    commanderNames={commanderNames}
                    catalog={catalog}
                    onPatch={patchSeat}
                    reduceMotion={reduceMotion === true}
                  />
                ))}
              </div>

              {blocked ? <ViolationPanel problems={problems} format={format} /> : null}

              <Divider className="mt-6" />

              <div className="mt-5 flex flex-wrap items-end gap-5">
                <SeedField
                  value={seedText}
                  onChange={setSeedText}
                  onRoll={() => setSeedText(String(randomSeed()))}
                />
                <SpeedField value={speed} onChange={setSpeed} />
                <button
                  type="button"
                  onClick={() => setBrowsing(true)}
                  className={clsx(
                    'metal-plate caps ml-auto cursor-pointer rounded-lg px-6 py-3.5',
                    'text-[13px] tracking-wide-caps text-ink',
                    'transition-[filter] duration-[var(--duration-micro)] ease-standard hover:brightness-125',
                  )}
                >
                  Explorar cartas
                </button>
                <button
                  type="submit"
                  disabled={blocked}
                  title={blocked ? 'Corrija as violações de legalidade para iniciar' : undefined}
                  className={clsx(
                    'metal-plate sheen caps min-w-[236px] rounded-lg px-8 py-3.5',
                    'text-[15px] tracking-wide-caps',
                    '[box-shadow:var(--shadow-e3),inset_0_1px_0_var(--color-edge-gloss),0_0_28px_-10px_var(--accent)]',
                    'transition-[filter,transform] duration-[var(--duration-micro)] ease-standard',
                    blocked
                      ? 'cursor-not-allowed text-ink-faint opacity-50'
                      : 'cursor-pointer text-accent-bright hover:brightness-125 active:scale-[0.985]',
                  )}
                >
                  {seatCount === 2 ? 'Iniciar duelo' : `Iniciar mesa de ${seatCount}`}
                </button>
              </div>
            </motion.form>
          </motion.div>
        ) : null}
      </AnimatePresence>
      <CardBrowser open={browsing} onClose={() => setBrowsing(false)} />
    </>
  )
}

function orderOf(slug: string, order: readonly string[]): number {
  const index = order.indexOf(slug)
  return index === -1 ? order.length : index
}

/** Feltro sob holofote atrás do pôster — a mesa aparece antes da partida. */
function Backdrop() {
  return (
    <div aria-hidden="true" className="table-felt absolute inset-0 -z-10">
      <div className="absolute inset-0 bg-[radial-gradient(120%_90%_at_50%_-10%,rgba(240,178,63,0.12),transparent_58%),radial-gradient(110%_80%_at_50%_110%,rgba(124,110,242,0.12),transparent_58%)]" />
      <div className="absolute inset-0 bg-[radial-gradient(130%_105%_at_50%_45%,transparent_38%,rgba(0,0,0,0.72)_100%)]" />
    </div>
  )
}

function Poster({
  live,
  format,
  seatCount,
}: {
  live: boolean
  format: FormatInfo
  seatCount: number
}) {
  const size =
    format.exactDeckSize !== null
      ? `${format.exactDeckSize} cartas`
      : `mínimo ${format.minDeckSize} cartas`
  return (
    <header className="text-center">
      <p className="caps text-hud text-ink-muted">Magic: The Gathering</p>
      <h1 className="mt-2 font-display text-[42px] leading-none tracking-caps text-ink-strong uppercase">
        Simulador de Duelos
      </h1>
      <Ornament />
      <p className="text-balance mx-auto max-w-[56ch] text-[13.5px] text-ink-muted">
        {seatCount} bots, um baralho cada, nenhuma mão humana. {formatLabel(format)}, {size}.
        Escolha a mesa e assista.
      </p>
      <p className="caps text-hud mt-3 text-ink-faint">
        {live ? 'Baralhos vindos do motor' : 'Motor offline — baralhos embutidos'}
      </p>
    </header>
  )
}

function Ornament() {
  return (
    <div aria-hidden="true" className="my-4 flex items-center justify-center gap-3">
      <span className="rule-soft h-px w-24" />
      <span className="size-1.5 rotate-45 bg-accent/75" />
      <span className="rule-soft h-px w-24" />
    </div>
  )
}

function Versus() {
  return (
    <div aria-hidden="true" className="flex h-full flex-col items-center gap-3 pt-14">
      <span className="w-px flex-1 bg-gradient-to-b from-transparent via-edge-strong to-transparent" />
      <span className="metal-plate caps grid size-11 place-items-center rounded-full text-[13px] text-ink">
        VS
      </span>
      <span className="w-px flex-1 bg-gradient-to-b from-transparent via-edge-strong to-transparent" />
    </div>
  )
}

/** O que o servidor respondeu ao recusar o último `start`. */
function ServerRejection({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="mt-5 rounded-lg border border-danger/45 bg-danger/10 px-4 py-3 text-[13px] text-ink"
    >
      <p className="caps text-hud text-danger">O motor recusou a mesa</p>
      <ul className="mt-2 space-y-1">
        {message.split('\n').map((line, index) => (
          <li key={index} className="leading-snug">
            {line}
          </li>
        ))}
      </ul>
    </div>
  )
}

function FormatField({
  formats,
  selected,
  onSelect,
}: {
  formats: readonly FormatInfo[]
  selected: string
  onSelect: (format: FormatInfo) => void
}) {
  return (
    <div>
      <span className="caps text-hud block text-ink-muted">Formato</span>
      <div
        className="metal-well mt-2 flex gap-1 rounded-md p-1"
        role="radiogroup"
        aria-label="Formato da partida"
      >
        {formats.map((format) => (
          <Segment
            key={format.slug}
            selected={format.slug === selected}
            title={`${format.deckCount} baralho(s) legal(is) neste formato`}
            onSelect={() => onSelect(format)}
          >
            {formatLabel(format)}
          </Segment>
        ))}
      </div>
    </div>
  )
}

function PlayerCountField({
  value,
  min,
  max,
  onChange,
}: {
  value: number
  min: number
  max: number
  onChange: (next: number) => void
}) {
  const options = [2, 3, 4]
  return (
    <div>
      <span className="caps text-hud block text-ink-muted">Jogadores</span>
      <div
        className="metal-well mt-2 flex gap-1 rounded-md p-1"
        role="radiogroup"
        aria-label="Número de jogadores"
      >
        {options.map((count) => {
          const allowed = count >= min && count <= max
          return (
            <Segment
              key={count}
              selected={count === value}
              disabled={!allowed}
              title={allowed ? undefined : 'O formato escolhido é de duelo'}
              onSelect={() => onChange(count)}
            >
              <span className="tabular-nums">{count}</span>
            </Segment>
          )
        })}
      </div>
    </div>
  )
}

/** Tudo que impede a mesa de começar, por assento. */
function ViolationPanel({
  problems,
  format,
}: {
  problems: readonly SeatProblem[]
  format: FormatInfo
}) {
  return (
    <div
      role="alert"
      className="mt-6 rounded-lg border border-danger/45 bg-danger/10 px-4 py-3 text-[13px] text-ink"
    >
      <p className="caps text-hud text-danger">
        Legalidade em {formatLabel(format)} — corrija para iniciar
      </p>
      <div className="mt-2 space-y-2">
        {problems.map((problem) => (
          <div key={problem.seat}>
            <p className="text-[12.5px] font-semibold text-ink-strong">
              Jogador {problem.seat + 1} — {problem.deckName}
            </p>
            <ul className="mt-1 ml-4 list-disc space-y-0.5">
              {problem.messages.map((message, index) => (
                <li key={index} className="leading-snug">
                  {message}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  )
}

interface SeatColumnProps {
  index: number
  seatCount: number
  seat: SeatChoice
  decks: readonly DeckInfo[]
  formatSlug: string
  requiresCommander: boolean
  commanderNames: readonly string[]
  catalog: readonly CardDef[]
  onPatch: (index: number, patch: Partial<SeatChoice>) => void
  reduceMotion: boolean
}

/** A coluna do assento, com o "VS" entre os dois primeiros quando é duelo. */
function SeatColumnWithDivider(props: SeatColumnProps) {
  return (
    <>
      <SeatColumn {...props} />
      {props.seatCount === 2 && props.index === 0 ? <Versus /> : null}
    </>
  )
}

/**
 * `data-seat` reaponta `--accent` para a cor do assento (ver
 * `design/theme.css`), então tudo dentro da coluna herda a cor certa sem que
 * um único componente filho saiba de qual lado está. O tema só define dois
 * assentos, então o terceiro e o quarto reaproveitam as duas cores.
 */
function SeatColumn({
  index,
  seat,
  decks,
  formatSlug,
  requiresCommander,
  commanderNames,
  catalog,
  onPatch,
  reduceMotion,
}: SeatColumnProps) {
  const selectedDeck = decks.find((entry) => entry.id === seat.deck) ?? decks[0]
  const declared = selectedDeck?.commander ?? null

  return (
    <motion.fieldset
      data-seat={index % 2}
      className="min-w-0"
      initial={reduceMotion ? false : { opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={
        reduceMotion ? undefined : { duration: 0.44, delay: 0.1 + index * 0.05, ease: [0.16, 1, 0.3, 1] }
      }
    >
      <legend className="sr-only">Jogador {index + 1}</legend>

      <div className="flex items-center gap-2.5">
        <span className="metal-plate accent-ring numeral grid size-8 place-items-center rounded-full text-[14px] text-accent-bright">
          {SEAT_NUMERAL[index]}
        </span>
        <span className="caps text-hud text-ink-muted">Jogador {index + 1}</span>
      </div>

      <div
        className="metal-well mt-3 max-h-[168px] [@media(max-height:900px)]:max-h-[128px] overflow-y-auto rounded-lg p-1.5"
        role="radiogroup"
        aria-label={`Baralho do jogador ${index + 1}`}
      >
        {decks.map((entry) => (
          <DeckRow
            key={entry.id}
            deck={entry}
            selected={entry.id === seat.deck}
            legal={deckIsLegal(entry, formatSlug)}
            onSelect={() => onPatch(index, { deck: entry.id, commander: '' })}
          />
        ))}
      </div>

      {requiresCommander ? (
        <CommanderField
          seat={index}
          value={seat.commander}
          declared={declared}
          options={commanderNames}
          onChange={(next) => onPatch(index, { commander: next })}
        />
      ) : null}

      <div className="mt-3" role="radiogroup" aria-label={`Bot do jogador ${index + 1}`}>
        <div className="metal-well flex gap-1 rounded-md p-1">
          {BOTS.map((option) => (
            <Segment
              key={option.kind}
              selected={option.kind === seat.bot}
              title={option.blurb}
              onSelect={() => onPatch(index, { bot: option.kind })}
            >
              {option.label}
            </Segment>
          ))}
        </div>
        <p className="mt-2 text-[12px] leading-4 text-ink-faint">
          {BOTS.find((option) => option.kind === seat.bot)?.blurb}
        </p>
      </div>

      {/* O tapete só existe para os dois assentos que o `playmatStore` conhece;
          a mesa de três e quatro ainda não desenha tapete próprio. */}
      {index < 2 && selectedDeck !== undefined ? (
        <PlaymatPicker seat={index === 0 ? 0 : 1} deck={selectedDeck} catalog={catalog} />
      ) : null}
    </motion.fieldset>
  )
}

/** CR 903.3 — quem comanda o deck. Vazio significa "o que a lista declara". */
function CommanderField({
  seat,
  value,
  declared,
  options,
  onChange,
}: {
  seat: number
  value: string
  declared: string | null
  options: readonly string[]
  onChange: (next: string) => void
}) {
  const id = `commander-${seat}`
  return (
    <div className="mt-3">
      <label htmlFor={id} className="caps text-hud block text-ink-muted">
        Comandante
      </label>
      <select
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="metal-well mt-2 w-full cursor-pointer rounded-md px-3 py-2 text-[13px] text-ink-strong outline-none"
      >
        <option value="">
          {declared === null ? 'Padrão da lista' : `${declared} (da lista)`}
        </option>
        {options.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
    </div>
  )
}

function DeckRow({
  deck,
  selected,
  legal,
  onSelect,
}: {
  deck: DeckInfo
  selected: boolean
  legal: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      title={legal ? undefined : 'Não é legal no formato escolhido'}
      className={clsx(
        'flex w-full cursor-pointer items-center gap-3 rounded-md px-2.5 py-2 text-left',
        'transition-[background-color,color] duration-[var(--duration-micro)] ease-standard',
        selected ? 'bg-accent-bg text-ink-strong' : 'text-ink hover:bg-white/6',
      )}
    >
      <span
        aria-hidden="true"
        className={clsx(
          'caps grid size-7 shrink-0 place-items-center rounded-full text-[12px] ring-1 ring-inset',
          selected
            ? 'bg-accent/20 text-accent-bright ring-accent/50'
            : 'bg-black/30 text-ink-faint ring-edge-hair',
        )}
      >
        {deck.name.charAt(0)}
      </span>
      <span className={clsx('min-w-0 flex-1 truncate text-[13.5px]', legal ? '' : 'opacity-55')}>
        {deck.name}
      </span>
      {legal ? null : (
        <span className="caps shrink-0 text-[10px] tracking-wide-caps text-danger">ilegal</span>
      )}
      {selected ? <span aria-hidden="true" className="size-1.5 rotate-45 bg-accent" /> : null}
    </button>
  )
}

function Segment({
  selected,
  title,
  disabled = false,
  onSelect,
  children,
}: {
  selected: boolean
  title?: string
  disabled?: boolean
  onSelect: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-disabled={disabled}
      disabled={disabled}
      title={title}
      onClick={onSelect}
      className={clsx(
        'flex-1 rounded-sm px-2 py-1.5 text-[12px] font-semibold',
        'transition-[background-color,color] duration-[var(--duration-micro)] ease-standard',
        disabled
          ? 'cursor-not-allowed text-ink-faint opacity-40'
          : 'cursor-pointer',
        selected && !disabled
          ? 'bg-accent text-ink-inverse'
          : disabled
            ? ''
            : 'text-ink-muted hover:bg-white/8 hover:text-ink-strong',
      )}
    >
      {children}
    </button>
  )
}

function SeedField({
  value,
  onChange,
  onRoll,
}: {
  value: string
  onChange: (next: string) => void
  onRoll: () => void
}) {
  return (
    <div>
      <label htmlFor="match-seed" className="caps text-hud block text-ink-muted">
        Semente
      </label>
      <div className="mt-2 flex items-center gap-2">
        <input
          id="match-seed"
          type="number"
          min={0}
          max={MAX_SEED}
          inputMode="numeric"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className="metal-well w-[168px] rounded-md px-3 py-2 font-mono text-[13px] tabular-nums text-ink-strong outline-none"
        />
        <IconButton label="Sortear semente" icon={<DiceIcon />} tone="metal" onClick={onRoll} />
      </div>
    </div>
  )
}

function SpeedField({ value, onChange }: { value: number; onChange: (next: number) => void }) {
  return (
    <div>
      <span className="caps text-hud block text-ink-muted">Velocidade</span>
      <div
        className="metal-well mt-2 flex gap-1 rounded-md p-1"
        role="radiogroup"
        aria-label="Velocidade inicial"
      >
        {SPEEDS.map((option) => (
          <Segment key={option} selected={option === value} onSelect={() => onChange(option)}>
            <span className="tabular-nums">{option}×</span>
          </Segment>
        ))}
      </div>
    </div>
  )
}

function DiceIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="3.5" y="3.5" width="17" height="17" rx="4" stroke="currentColor" strokeWidth="1.6" />
      <circle cx="8.5" cy="8.5" r="1.5" fill="currentColor" />
      <circle cx="15.5" cy="15.5" r="1.5" fill="currentColor" />
      <circle cx="12" cy="12" r="1.5" fill="currentColor" />
    </svg>
  )
}
