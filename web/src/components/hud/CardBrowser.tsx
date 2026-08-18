import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactElement } from 'react'
import { clsx } from 'clsx'
import { fetchCardPage, fetchCatalogStats } from '../../net/api'
import type { CardSummary, CatalogStats } from '../../net/api'
import type { CardView, ColorSet } from '../../types/protocol'
import { Card } from '../card/Card'

/** Cartas por página. Três fileiras cheias a 1920px. */
const PAGE_SIZE = 24
/** Espera entre a última tecla e a busca — evita uma requisição por letra. */
const DEBOUNCE_MS = 260

/** Filtro de jogabilidade. `all` não manda o campo e o servidor não filtra. */
type PlayableFilter = 'all' | 'yes' | 'no'

const PLAYABLE_TABS: readonly { value: PlayableFilter; label: string }[] = [
  { value: 'all', label: 'Todas' },
  { value: 'yes', label: 'Jogáveis' },
  { value: 'no', label: 'Não jogáveis' },
]

/**
 * Letra WUBRG na ordem em que o bit aparece em `ColorSet` — o índice aqui É o
 * bit. Escrito à mão porque a inicial de `Blue` colide com a de `Black`.
 */
const COLOR_BITS: readonly string[] = ['W', 'U', 'B', 'R', 'G']

const COLOR_CHIPS: readonly { letter: string; label: string; ink: string }[] = [
  { letter: 'W', label: 'Branco', ink: '#f4efd8' },
  { letter: 'U', label: 'Azul', ink: '#8fc3e8' },
  { letter: 'B', label: 'Preto', ink: '#9b93a8' },
  { letter: 'R', label: 'Vermelho', ink: '#e4907b' },
  { letter: 'G', label: 'Verde', ink: '#8ec89a' },
]

function colorMask(letters: readonly string[]): ColorSet {
  let mask = 0
  for (const letter of letters) {
    const bit = COLOR_BITS.indexOf(letter.toUpperCase())
    if (bit >= 0) mask |= 1 << bit
  }
  return mask
}

/**
 * `CardSummary` (metadado do catálogo) vira `CardView` (o que o componente
 * `Card` desenha). Tudo que é estado de partida — dano, virada, enjoo — entra
 * zerado: aqui a carta não está em jogo, está numa vitrine.
 */
export function toCardView(card: CardSummary, id: number): CardView {
  return {
    id,
    name: card.name,
    manaCost: card.manaCost.length > 0 ? card.manaCost : null,
    manaValue: card.manaValue,
    typeLine: card.typeLine.length > 0 ? card.typeLine : null,
    oracleText: card.oracleText.length > 0 ? card.oracleText : null,
    flavorText: null,
    colors: colorMask(card.colors),
    power: card.power,
    toughness: card.toughness,
    basePower: card.power,
    baseToughness: card.toughness,
    loyalty: null,
    damage: 0,
    tapped: false,
    faceDown: false,
    summoningSick: false,
    attacking: null,
    blocking: [],
    blockedBy: [],
    counters: [],
    keywords: [],
    attachedTo: null,
    attachments: [],
    isToken: false,
    controller: 0,
    owner: 0,
    zone: 'Library',
    // A URL do CDN vem pronta do servidor; `scryfallArtUrl` a repassa inteira.
    artKey: card.imageArtCrop,
    rarity: card.rarity,
    setCode: card.setCode,
    isLegalTarget: false,
    isActionable: false,
  }
}

export interface CardBrowserProps {
  open: boolean
  onClose: () => void
}

/**
 * Navegador do catálogo amplo — as ~32 mil cartas importadas do Scryfall,
 * servidas por `GET /api/cards`.
 *
 * Não confundir com `/api/catalog`, que é o punhado de cartas curadas em Lua e
 * alimenta o seletor de tapete. Aqui a busca é do servidor: o cliente nunca
 * carrega o catálogo inteiro, só a página que está mostrando.
 */
export function CardBrowser({ open, onClose }: CardBrowserProps): ReactElement | null {
  const [text, setText] = useState('')
  const [debounced, setDebounced] = useState('')
  const [colors, setColors] = useState<readonly string[]>([])
  const [playable, setPlayable] = useState<PlayableFilter>('all')
  const [offset, setOffset] = useState(0)

  const [items, setItems] = useState<readonly CardSummary[]>([])
  const [total, setTotal] = useState(0)
  const [stats, setStats] = useState<CatalogStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const resultsRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(text), DEBOUNCE_MS)
    return () => window.clearTimeout(timer)
  }, [text])

  // Filtro novo recomeça da primeira página: manter o offset alto mostraria
  // "nenhum resultado" numa busca que tem resultados de sobra.
  useEffect(() => {
    setOffset(0)
  }, [debounced, colors, playable])

  useEffect(() => {
    if (!open) return
    const controller = new AbortController()
    setLoading(true)
    fetchCardPage(
      {
        text: debounced,
        colors,
        playable: playable === 'all' ? undefined : playable === 'yes',
        limit: PAGE_SIZE,
        offset,
      },
      controller.signal,
    )
      .then((page) => {
        setItems(page.items)
        setTotal(page.total)
        setError(null)
      })
      .catch((cause: unknown) => {
        if (controller.signal.aborted) return
        setItems([])
        setTotal(0)
        setError(cause instanceof Error ? cause.message : 'falha ao consultar o catálogo')
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false)
      })
    return () => controller.abort()
  }, [open, debounced, colors, playable, offset])

  useEffect(() => {
    if (!open) return
    const controller = new AbortController()
    fetchCatalogStats(controller.signal)
      .then(setStats)
      .catch(() => {
        // O cabeçalho fica sem o total do catálogo; a busca continua de pé.
      })
    return () => controller.abort()
  }, [open])

  useEffect(() => {
    if (!open) return
    function onKeyDown(event: KeyboardEvent): void {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [open, onClose])

  const toggleColor = useCallback((letter: string) => {
    setColors((current) =>
      current.includes(letter) ? current.filter((c) => c !== letter) : [...current, letter],
    )
  }, [])

  // Resultado novo comeca do topo. Sem isso, avancar de pagina no fim da
  // rolagem mostra a pagina seguinte ja rolada, com a primeira fileira cortada
  // acima da area visivel — parece resultado faltando.
  useEffect(() => {
    resultsRef.current?.scrollTo({ top: 0 })
  }, [items])

  const views = useMemo(() => items.map((card, i) => toCardView(card, offset + i)), [items, offset])

  if (!open) return null

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const page = Math.floor(offset / PAGE_SIZE) + 1

  return (
    <div
      className="fixed inset-0 z-[300] flex flex-col overflow-hidden bg-[#0b0b0e]"
      role="dialog"
      aria-modal="true"
      aria-label="Catálogo de cartas"
    >
      <header className="flex shrink-0 flex-wrap items-end gap-x-6 gap-y-3 border-b border-edge-hair px-8 py-5">
        <div className="min-w-0">
          <p className="caps text-hud text-ink-muted">Catálogo</p>
          <h1 className="mt-1 font-display text-[26px] leading-none tracking-caps text-ink-strong uppercase">
            Cartas
          </h1>
        </div>

        <p className="text-[12.5px] text-ink-faint" data-testid="catalog-stats">
          {stats === null
            ? 'consultando o catálogo…'
            : `${stats.total.toLocaleString('pt-BR')} cartas no banco · ${stats.playable.toLocaleString('pt-BR')} jogáveis`}
        </p>

        <button
          type="button"
          onClick={onClose}
          className="metal-plate caps ml-auto cursor-pointer rounded-lg px-5 py-2.5 text-[12px] tracking-wide-caps text-ink hover:brightness-125"
        >
          Voltar
        </button>
      </header>

      <div className="flex shrink-0 flex-wrap items-end gap-x-7 gap-y-4 border-b border-edge-hair px-8 py-4">
        <div className="min-w-[280px] flex-1">
          <label htmlFor="catalog-search" className="caps text-hud block text-ink-muted">
            Buscar por nome ou texto
          </label>
          <input
            id="catalog-search"
            type="search"
            value={text}
            onChange={(event) => setText(event.target.value)}
            placeholder="dragon, counter target spell, lightning…"
            autoComplete="off"
            className="metal-well mt-2 w-full rounded-md px-3 py-2 text-[13.5px] text-ink-strong outline-none placeholder:text-ink-faint focus:ring-1 focus:ring-accent/60"
          />
        </div>

        <fieldset className="m-0 border-0 p-0">
          <legend className="caps text-hud text-ink-muted">Cor</legend>
          <div className="mt-2 flex gap-1.5">
            {COLOR_CHIPS.map((chip) => {
              const on = colors.includes(chip.letter)
              return (
                <button
                  key={chip.letter}
                  type="button"
                  aria-pressed={on}
                  title={chip.label}
                  onClick={() => toggleColor(chip.letter)}
                  className={clsx(
                    'grid size-9 cursor-pointer place-items-center rounded-full text-[13px] font-semibold',
                    'ring-1 ring-inset transition-[background-color,color] duration-[var(--duration-micro)]',
                    on ? 'ring-accent/70' : 'ring-edge-hair hover:bg-white/8',
                  )}
                  style={on ? { background: chip.ink, color: '#16161b' } : { color: chip.ink }}
                >
                  {chip.letter}
                </button>
              )
            })}
          </div>
        </fieldset>

        <fieldset className="m-0 border-0 p-0">
          <legend className="caps text-hud text-ink-muted">Jogabilidade</legend>
          <div className="metal-well mt-2 flex gap-1 rounded-md p-1" role="radiogroup" aria-label="Jogabilidade">
            {PLAYABLE_TABS.map((tab) => (
              <button
                key={tab.value}
                type="button"
                role="radio"
                aria-checked={playable === tab.value}
                onClick={() => setPlayable(tab.value)}
                className={clsx(
                  'cursor-pointer rounded-sm px-3 py-1.5 text-[12px] font-semibold',
                  'transition-[background-color,color] duration-[var(--duration-micro)]',
                  playable === tab.value
                    ? 'bg-accent text-ink-inverse'
                    : 'text-ink-muted hover:bg-white/8 hover:text-ink-strong',
                )}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </fieldset>

        <p className="ml-auto pb-1 text-[12.5px] text-ink-muted" data-testid="catalog-result-count">
          {loading ? 'buscando…' : `${total.toLocaleString('pt-BR')} resultado(s)`}
        </p>
      </div>

      <div ref={resultsRef} className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
        {error !== null ? (
          <p className="text-[13px] text-ink-muted">
            Não deu para consultar o catálogo: {error}. O <code>mtg-server</code> está no ar?
          </p>
        ) : views.length === 0 && !loading ? (
          <p className="text-[13px] text-ink-muted">Nenhuma carta para esses filtros.</p>
        ) : (
          <div
            className="grid grid-cols-[repeat(auto-fill,minmax(170px,170px))] justify-start gap-x-6 gap-y-7"
            data-testid="catalog-grid"
          >
            {views.map((view, index) => {
              const source = items[index]
              return (
                <figure key={source.oracleId ?? source.name} className="m-0 flex flex-col gap-2">
                  <Card card={view} size="medium" />
                  <figcaption className="flex min-w-0 flex-col gap-0.5">
                    <span
                      className="truncate text-[12px] font-semibold text-ink-strong"
                      title={source.name}
                    >
                      {source.name}
                    </span>
                    <span className="caps text-[10px] tracking-[0.14em] text-ink-faint">
                      {source.setCode.toUpperCase()} · {source.rarity}
                    </span>
                    {source.playable ? (
                      <span className="text-[10.5px] text-[#8ec89a]">jogável</span>
                    ) : (
                      <span
                        className="truncate text-[10.5px] text-ink-faint"
                        title={source.unsupportedReason ?? 'texto não compilado'}
                      >
                        sem regras: {source.unsupportedReason ?? 'não compilada'}
                      </span>
                    )}
                  </figcaption>
                </figure>
              )
            })}
          </div>
        )}
      </div>

      <footer className="flex shrink-0 items-center justify-center gap-4 border-t border-edge-hair px-8 py-4">
        <button
          type="button"
          onClick={() => setOffset((value) => Math.max(0, value - PAGE_SIZE))}
          disabled={offset === 0}
          className="metal-plate caps cursor-pointer rounded-md px-4 py-2 text-[12px] text-ink disabled:cursor-default disabled:opacity-35"
        >
          Anterior
        </button>
        <span className="text-[12.5px] text-ink-muted" data-testid="catalog-page">
          Página {page} de {pageCount.toLocaleString('pt-BR')}
        </span>
        <button
          type="button"
          onClick={() => setOffset((value) => (value + PAGE_SIZE < total ? value + PAGE_SIZE : value))}
          disabled={offset + PAGE_SIZE >= total}
          className="metal-plate caps cursor-pointer rounded-md px-4 py-2 text-[12px] text-ink disabled:cursor-default disabled:opacity-35"
        >
          Próxima
        </button>
      </footer>
    </div>
  )
}
