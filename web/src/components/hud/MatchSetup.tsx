import { clsx } from 'clsx'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useCallback, useEffect, useState } from 'react'
import type { FormEvent, ReactNode } from 'react'
import { fetchCardCatalog, fetchDecks } from '../../net/api'
import type { DeckInfo } from '../../net/api'
import { MOCK_CARDS, toCardDef } from '../../mock/mockCards'
import { useMatchStore } from '../../state/matchStore'
import type { CardDef } from '../../types/protocol'
import { Divider } from '../ui/Divider'
import { IconButton } from '../ui/IconButton'
import { PlaymatPicker } from './PlaymatPicker'

/**
 * Os quatro decks de `crates/mtg-cards/src/decks.rs`, com o id no formato que o
 * servidor gera (slug do nome). Só entram em cena se `GET /api/decks` falhar —
 * sem eles a tela de abertura ficaria vazia justamente quando o motor está fora
 * do ar e a partida vai cair no replay de demonstração.
 */
const FALLBACK_DECKS: readonly DeckInfo[] = [
  { id: 'goblin-onslaught', name: 'Goblin Onslaught', colorIdentity: ['R'] },
  { id: 'azorius-control', name: 'Azorius Control', colorIdentity: ['W', 'U'] },
  { id: 'selesnya-valor', name: 'Selesnya Valor', colorIdentity: ['W', 'G'] },
  { id: 'gruul-stampede', name: 'Gruul Stampede', colorIdentity: ['R', 'G'] },
]

/** Catálogo de reserva quando `GET /api/cards` não responde. */
const FALLBACK_CATALOG: readonly CardDef[] = Object.values(MOCK_CARDS).map(toCardDef)

type BotKind = 'random' | 'heuristic' | 'greedy'

/**
 * O frame `start` do protocolo ainda nao carrega o tipo de bot (ver
 * `ClientMessage::Start` em `mtg-server/src/protocol.rs`), entao a escolha vive
 * na tela: ela declara o confronto e vai junto no dia em que o campo existir.
 */
const BOTS: readonly { kind: BotKind; label: string; blurb: string }[] = [
  { kind: 'random', label: 'Aleatório', blurb: 'Sorteia entre as jogadas legais.' },
  { kind: 'heuristic', label: 'Heurístico', blurb: 'Pesa tabuleiro, curva e vida.' },
  { kind: 'greedy', label: 'Ganancioso', blurb: 'Busca o maior ganho imediato.' },
]

const SPEEDS: readonly number[] = [0.5, 1, 2, 4]

const MAX_SEED = 0x7fffffff
const SEAT_NUMERAL: readonly string[] = ['I', 'II']

function randomSeed(): number {
  return Math.floor(Math.random() * MAX_SEED)
}

/**
 * Tela de abertura. Fica no lugar da mesa enquanto nenhuma partida começou
 * (`connection === 'idle'`) e some assim que `start` é chamado — quem controla
 * a partida a partir daí é o HUD.
 */
export function MatchSetup() {
  const connection = useMatchStore((s) => s.connection)
  const start = useMatchStore((s) => s.start)
  const reduceMotion = useReducedMotion()

  const [decks, setDecks] = useState<readonly DeckInfo[]>(FALLBACK_DECKS)
  const [catalog, setCatalog] = useState<readonly CardDef[]>(FALLBACK_CATALOG)
  const [live, setLive] = useState(false)
  const [deckA, setDeckA] = useState(FALLBACK_DECKS[0].id)
  const [deckB, setDeckB] = useState(FALLBACK_DECKS[1].id)
  const [botA, setBotA] = useState<BotKind>('heuristic')
  const [botB, setBotB] = useState<BotKind>('heuristic')
  const [seedText, setSeedText] = useState(() => String(randomSeed()))
  const [speed, setSpeed] = useState(1)

  useEffect(() => {
    const controller = new AbortController()
    let cancelled = false

    const load = async (): Promise<void> => {
      try {
        const list = await fetchDecks(controller.signal)
        if (cancelled || list.length === 0) return
        setDecks(list)
        setLive(true)
        setDeckA(list[0].id)
        setDeckB(list[Math.min(1, list.length - 1)].id)
      } catch {
        // Servidor fora do ar: a lista embutida sustenta a escolha, e o socket
        // cai sozinho no replay de demonstração quando a partida começar.
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
    void loadCatalog()
    return () => {
      cancelled = true
      controller.abort()
    }
  }, [])

  const handleSubmit = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      const parsed = Number.parseInt(seedText, 10)
      const seed = Number.isFinite(parsed) && parsed >= 0 ? parsed % (MAX_SEED + 1) : randomSeed()
      start(deckA, deckB, seed, speed)
    },
    [deckA, deckB, seedText, speed, start],
  )

  const fade = reduceMotion ? undefined : { duration: 0.26, ease: [0.16, 1, 0.3, 1] as const }
  const rise = (delay: number) =>
    reduceMotion
      ? undefined
      : { duration: 0.42, delay, ease: [0.16, 1, 0.3, 1] as const }

  return (
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
            className="glass relative w-full max-w-[1040px] rounded-2xl px-8 py-7 sm:px-10 [@media(max-height:900px)]:py-5"
            initial={reduceMotion ? false : { opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            transition={rise(0.04)}
          >
            <Poster live={live} />

            <div className="mt-7 grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-start gap-5">
              <SeatColumn
                seat={0}
                decks={decks}
                deck={deckA}
                onDeck={setDeckA}
                bot={botA}
                onBot={setBotA}
                catalog={catalog}
                delay={0.1}
                reduceMotion={reduceMotion === true}
              />
              <Versus />
              <SeatColumn
                seat={1}
                decks={decks}
                deck={deckB}
                onDeck={setDeckB}
                bot={botB}
                onBot={setBotB}
                catalog={catalog}
                delay={0.16}
                reduceMotion={reduceMotion === true}
              />
            </div>

            <Divider className="mt-7" />

            <div className="mt-5 flex flex-wrap items-end gap-5">
              <SeedField value={seedText} onChange={setSeedText} onRoll={() => setSeedText(String(randomSeed()))} />
              <SpeedField value={speed} onChange={setSpeed} />
              <button
                type="submit"
                className={clsx(
                  'metal-plate sheen caps ml-auto min-w-[236px] cursor-pointer rounded-lg px-8 py-3.5',
                  'text-[15px] tracking-wide-caps text-accent-bright',
                  '[box-shadow:var(--shadow-e3),inset_0_1px_0_var(--color-edge-gloss),0_0_28px_-10px_var(--accent)]',
                  'transition-[filter,transform] duration-[var(--duration-micro)] ease-standard',
                  'hover:brightness-125 active:scale-[0.985]',
                )}
              >
                Iniciar duelo
              </button>
            </div>
          </motion.form>
        </motion.div>
      ) : null}
    </AnimatePresence>
  )
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

function Poster({ live }: { live: boolean }) {
  return (
    <header className="text-center">
      <p className="caps text-hud text-ink-muted">Magic: The Gathering</p>
      <h1 className="mt-2 font-display text-[42px] leading-none tracking-caps text-ink-strong uppercase">
        Simulador de Duelos
      </h1>
      <Ornament />
      <p className="text-balance mx-auto max-w-[52ch] text-[13.5px] text-ink-muted">
        Dois bots, um baralho cada, nenhuma mão humana. Escolha os lados e assista.
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

interface SeatColumnProps {
  seat: 0 | 1
  decks: readonly DeckInfo[]
  deck: string
  onDeck: (id: string) => void
  bot: BotKind
  onBot: (kind: BotKind) => void
  catalog: readonly CardDef[]
  delay: number
  reduceMotion: boolean
}

/**
 * `data-seat` reaponta `--accent` para o âmbar ou o índigo do assento (ver
 * `design/theme.css`), então tudo dentro da coluna herda a cor certa sem que
 * um único componente filho saiba de qual lado está.
 */
function SeatColumn({ seat, decks, deck, onDeck, bot, onBot, catalog, delay, reduceMotion }: SeatColumnProps) {
  const selectedDeck = decks.find((entry) => entry.id === deck) ?? decks[0]
  return (
    <motion.fieldset
      data-seat={seat}
      className="min-w-0"
      initial={reduceMotion ? false : { opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={reduceMotion ? undefined : { duration: 0.44, delay, ease: [0.16, 1, 0.3, 1] }}
    >
      <legend className="sr-only">Jogador {seat + 1}</legend>

      <div className="flex items-center gap-2.5">
        <span className="metal-plate accent-ring numeral grid size-8 place-items-center rounded-full text-[14px] text-accent-bright">
          {SEAT_NUMERAL[seat]}
        </span>
        <span className="caps text-hud text-ink-muted">Jogador {seat + 1}</span>
      </div>

      <div className="metal-well mt-3 max-h-[168px] [@media(max-height:900px)]:max-h-[128px] overflow-y-auto rounded-lg p-1.5" role="radiogroup" aria-label={`Baralho do jogador ${seat + 1}`}>
        {decks.map((entry) => (
          <DeckRow
            key={entry.id}
            deck={entry}
            selected={entry.id === deck}
            onSelect={() => onDeck(entry.id)}
          />
        ))}
      </div>

      <div className="mt-3" role="radiogroup" aria-label={`Bot do jogador ${seat + 1}`}>
        <div className="metal-well flex gap-1 rounded-md p-1">
          {BOTS.map((option) => (
            <Segment
              key={option.kind}
              selected={option.kind === bot}
              title={option.blurb}
              onSelect={() => onBot(option.kind)}
            >
              {option.label}
            </Segment>
          ))}
        </div>
        <p className="mt-2 text-[12px] leading-4 text-ink-faint">
          {BOTS.find((option) => option.kind === bot)?.blurb}
        </p>
      </div>

      <PlaymatPicker seat={seat} deck={selectedDeck} catalog={catalog} />
    </motion.fieldset>
  )
}

function DeckRow({
  deck,
  selected,
  onSelect,
}: {
  deck: DeckInfo
  selected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
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
          selected ? 'bg-accent/20 text-accent-bright ring-accent/50' : 'bg-black/30 text-ink-faint ring-edge-hair',
        )}
      >
        {deck.name.charAt(0)}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13.5px]">{deck.name}</span>
      {selected ? <span aria-hidden="true" className="size-1.5 rotate-45 bg-accent" /> : null}
    </button>
  )
}

function Segment({
  selected,
  title,
  onSelect,
  children,
}: {
  selected: boolean
  title?: string
  onSelect: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      title={title}
      onClick={onSelect}
      className={clsx(
        'flex-1 cursor-pointer rounded-sm px-2 py-1.5 text-[12px] font-semibold',
        'transition-[background-color,color] duration-[var(--duration-micro)] ease-standard',
        selected ? 'bg-accent text-ink-inverse' : 'text-ink-muted hover:bg-white/8 hover:text-ink-strong',
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
      <div className="metal-well mt-2 flex gap-1 rounded-md p-1" role="radiogroup" aria-label="Velocidade inicial">
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
