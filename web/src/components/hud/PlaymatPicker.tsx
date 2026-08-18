import { clsx } from 'clsx'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactElement } from 'react'
import type { DeckInfo } from '../../net/api'
import { isSafeImageUrl, tintFromIdentity, usePlaymatStore } from '../../state/playmatStore'
import type { Seat } from '../../state/playmatStore'
import { colorsOf } from '../../types/protocol'
import type { CardDef, Color, ManaCost, Rarity } from '../../types/protocol'
import { scryfallArtUrl } from '../card/cardVisuals'

/** Oito artes cobrem a variedade do baralho sem estourar o Scryfall de golpe. */
const MAX_THUMBS = 8

const SCRYFALL_PREFIX = 'https://api.scryfall.com/cards/named'

const EMPTY_IDENTITY: readonly string[] = []

type Source = 'deck' | 'url' | 'none'
type UrlStatus = 'idle' | 'checking' | 'rejected' | 'broken'

const SOURCE_LABEL: readonly { source: Source; label: string; hint: string }[] = [
  { source: 'deck', label: 'Baralho', hint: 'Arte de uma carta do baralho escolhido.' },
  { source: 'url', label: 'URL', hint: 'Endereço de imagem que você colar.' },
  { source: 'none', label: 'Nenhuma', hint: 'Tapete liso, tingido pela cor do baralho.' },
]

const COLOR_LETTER: Readonly<Record<Color, string>> = {
  White: 'W',
  Blue: 'U',
  Black: 'B',
  Red: 'R',
  Green: 'G',
}

/** Tinta de feltro por cor, escurecida — é fundo de tapete, não é a carta. */
const COLOR_INK: Readonly<Record<string, string>> = {
  W: '#8d8467',
  U: '#22527f',
  B: '#3a3246',
  R: '#8a3125',
  G: '#255c39',
}

const RARITY_RANK: Readonly<Record<Rarity, number>> = {
  Mythic: 4,
  Rare: 3,
  Special: 2,
  Uncommon: 1,
  Common: 0,
}

interface MatCandidate {
  readonly name: string
  readonly url: string
}

// ---------------------------------------------------------------------------
// Escolha das artes candidatas
// ---------------------------------------------------------------------------

function collectColors(symbol: ManaCost[number], push: (color: Color) => void): void {
  if (typeof symbol === 'string') return
  if ('Colored' in symbol) push(symbol.Colored)
  else if ('Hybrid' in symbol) {
    push(symbol.Hybrid[0])
    push(symbol.Hybrid[1])
  } else if ('MonoHybrid' in symbol) push(symbol.MonoHybrid)
  else if ('Phyrexian' in symbol) push(symbol.Phyrexian)
}

function cardColors(card: CardDef): string[] {
  const letters: string[] = []
  const push = (color: Color): void => {
    const letter = COLOR_LETTER[color]
    if (!letters.includes(letter)) letters.push(letter)
  }
  if (card.colorOverride !== null) {
    for (const color of colorsOf(card.colorOverride)) push(color)
    return letters
  }
  for (const symbol of card.manaCost) collectColors(symbol, push)
  return letters
}

function manaValue(cost: ManaCost): number {
  let total = 0
  for (const symbol of cost) {
    if (typeof symbol === 'string') {
      if (symbol !== 'X') total += 1
      continue
    }
    if ('Generic' in symbol) total += symbol.Generic
    else total += 1
  }
  return total
}

/**
 * O catálogo não diz a que baralho cada carta pertence — `GET /api/cards`
 * devolve tudo e `GET /api/decks` não expõe a lista de nomes. A aproximação
 * honesta é a identidade de cor: carta cuja cor cabe no baralho é carta que
 * poderia estar nele. Para escolher tapete isso basta, e ainda dá mais opção
 * que os 60 slots reais.
 */
function deckArtCandidates(
  catalog: readonly CardDef[],
  identity: readonly string[],
): MatCandidate[] {
  const wanted = new Set(identity)
  const spells = catalog.filter((card) => !card.typeLine.types.includes('Land'))
  const inColor = spells.filter((card) => {
    const colors = cardColors(card)
    if (colors.length === 0) return false
    if (wanted.size === 0) return true
    return colors.every((letter) => wanted.has(letter))
  })

  const pool = inColor.length > 0 ? inColor : spells
  const ranked = [...pool].sort((a, b) => {
    const rarity = RARITY_RANK[b.rarity] - RARITY_RANK[a.rarity]
    if (rarity !== 0) return rarity
    const value = manaValue(b.manaCost) - manaValue(a.manaCost)
    if (value !== 0) return value
    return a.name.localeCompare(b.name)
  })

  const out: MatCandidate[] = []
  for (const card of ranked) {
    const url = scryfallArtUrl(card)
    if (url === null) continue
    out.push({ name: card.name, url })
    if (out.length === MAX_THUMBS) break
  }
  return out
}

function deckTint(identity: readonly string[]): string {
  const inks = identity
    .map((letter) => COLOR_INK[letter])
    .filter((ink): ink is string => ink !== undefined)
  if (inks.length === 0) return 'linear-gradient(152deg,#1c2231 0%,#0a0d15 100%)'
  if (inks.length === 1) return `linear-gradient(152deg,${inks[0]} 0%,#0a0d15 82%)`
  return `linear-gradient(152deg,${inks[0]} 0%,${inks[inks.length - 1]} 55%,#0a0d15 100%)`
}

// ---------------------------------------------------------------------------
// Seletor
// ---------------------------------------------------------------------------

export interface PlaymatPickerProps {
  seat: Seat
  deck: DeckInfo
  catalog: readonly CardDef[]
}

export function PlaymatPicker({ seat, deck, catalog }: PlaymatPickerProps): ReactElement {
  const art = usePlaymatStore((s) => s.art[seat])
  const chosen = usePlaymatStore((s) => s.chosen[seat])
  const setArt = usePlaymatStore((s) => s.setArt)
  const suggestArt = usePlaymatStore((s) => s.suggestArt)
  const setTint = usePlaymatStore((s) => s.setTint)

  const identity = deck.colorIdentity ?? EMPTY_IDENTITY
  const candidates = useMemo(() => deckArtCandidates(catalog, identity), [catalog, identity])

  const [source, setSource] = useState<Source>(() => {
    if (art !== null) return art.startsWith(SCRYFALL_PREFIX) ? 'deck' : 'url'
    return chosen ? 'none' : 'deck'
  })
  const [urlText, setUrlText] = useState(() => (art !== null && !art.startsWith(SCRYFALL_PREFIX) ? art : ''))
  const [urlStatus, setUrlStatus] = useState<UrlStatus>('idle')

  // Uma sonda de imagem que termina depois de outra ter começado não pode
  // decidir o resultado; o token descarta a resposta atrasada.
  const probeToken = useRef(0)
  useEffect(() => () => {
    probeToken.current += 1
  }, [])

  /**
   * Enquanto o usuário não apontar uma arte, o padrão segue a lista de
   * candidatas. Isso importa porque a primeira lista vem do catálogo embutido:
   * sem seguir, a escolha do catálogo de reserva congelaria por cima da carta
   * marcante que o catálogo do servidor traz segundos depois.
   */
  useEffect(() => {
    if (source !== 'deck' || candidates.length === 0) return
    const preferred = candidates[0].url
    if (art === preferred) return
    if (chosen && art !== null && candidates.some((entry) => entry.url === art)) return
    suggestArt(seat, preferred)
  }, [source, candidates, art, chosen, seat, suggestArt])

  // O pigmento do feltro segue o baralho selecionado; é o que o tapete usa
  // quando o jogador escolhe "Nenhuma" arte.
  useEffect(() => {
    setTint(seat, tintFromIdentity(identity))
  }, [identity, seat, setTint])

  const pickSource = useCallback(
    (next: Source) => {
      setSource(next)
      setUrlStatus('idle')
      if (next === 'none') setArt(seat, null)
    },
    [seat, setArt],
  )

  const applyUrl = useCallback(() => {
    const raw = urlText.trim()
    if (!isSafeImageUrl(raw)) {
      setUrlStatus('rejected')
      return
    }
    const token = probeToken.current + 1
    probeToken.current = token
    setUrlStatus('checking')
    const probe = new Image()
    probe.decoding = 'async'
    probe.onload = () => {
      if (token !== probeToken.current) return
      setUrlStatus('idle')
      setArt(seat, raw)
    }
    probe.onerror = () => {
      if (token !== probeToken.current) return
      setUrlStatus('broken')
    }
    probe.src = raw
  }, [urlText, seat, setArt])

  const tint = useMemo(() => deckTint(identity), [identity])
  const groupName = `playmat-source-${seat}`

  return (
    <div className="mt-3">
      <span className="caps text-hud block text-ink-muted">Tapete</span>

      <div className="metal-well mt-2 flex gap-2.5 rounded-lg p-2">
        <MatPreview art={art} tint={tint} />

        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <div className="metal-well flex gap-1 rounded-md p-1" role="radiogroup" aria-label={`Fonte do tapete do jogador ${seat + 1}`}>
            {SOURCE_LABEL.map((option) => (
              <button
                key={option.source}
                type="button"
                role="radio"
                name={groupName}
                aria-checked={option.source === source}
                title={option.hint}
                onClick={() => pickSource(option.source)}
                className={clsx(
                  'flex-1 cursor-pointer rounded-sm px-1.5 py-1 text-[11.5px] font-semibold',
                  'transition-[background-color,color] duration-[var(--duration-micro)] ease-standard',
                  option.source === source
                    ? 'bg-accent text-ink-inverse'
                    : 'text-ink-muted hover:bg-white/8 hover:text-ink-strong',
                )}
              >
                {option.label}
              </button>
            ))}
          </div>

          {source === 'deck' ? (
            <ArtStrip candidates={candidates} selected={art} onPick={(url) => setArt(seat, url)} />
          ) : null}

          {source === 'url' ? (
            <UrlField
              seat={seat}
              value={urlText}
              status={urlStatus}
              onChange={(next) => {
                setUrlText(next)
                setUrlStatus('idle')
              }}
              onApply={applyUrl}
            />
          ) : null}

          {source === 'none' ? (
            <p className="text-[11.5px] leading-4 text-ink-faint">
              Feltro liso tingido pelas cores de {deck.name}. As linhas das zonas continuam impressas.
            </p>
          ) : null}
        </div>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Prévia do tapete
// ---------------------------------------------------------------------------

/**
 * O contorno é o mesmo do tapete físico: linha branca fina impressa por cima da
 * arte, não caixa de interface. Só as zonas que a mesa realmente usa.
 */
function MatPreview({ art, tint }: { art: string | null; tint: string }): ReactElement {
  const [shown, setShown] = useState<string | null>(null)

  return (
    <div
      className="relative aspect-[16/9] w-[176px] shrink-0 [@media(max-height:900px)]:w-[150px] overflow-hidden rounded-md ring-1 ring-edge-strong [box-shadow:var(--shadow-e2)]"
      style={{ background: tint }}
    >
      {art !== null ? (
        <img
          key={art}
          src={art}
          alt=""
          aria-hidden="true"
          draggable={false}
          decoding="async"
          onLoad={() => setShown(art)}
          className={clsx(
            'absolute inset-0 size-full object-cover',
            'transition-opacity duration-[var(--duration-base)] ease-standard',
            shown === art ? 'opacity-100' : 'opacity-0',
          )}
        />
      ) : null}

      <span
        aria-hidden="true"
        className="absolute inset-0 bg-[radial-gradient(120%_100%_at_50%_38%,transparent_36%,rgba(0,0,0,0.62)_100%)]"
      />

      <svg
        className="absolute inset-0 size-full"
        viewBox="0 0 160 90"
        preserveAspectRatio="none"
        aria-hidden="true"
        focusable="false"
      >
        <g
          fill="none"
          stroke="rgba(255,255,255,0.82)"
          strokeWidth="0.7"
          vectorEffect="non-scaling-stroke"
        >
          <rect x="5" y="5" width="107" height="51" rx="1.5" />
          <circle cx="58.5" cy="28" r="11" />
          <rect x="5" y="59" width="107" height="26" rx="1.5" />
          {[19, 39, 59, 79, 99].map((cx) => (
            <circle key={cx} cx={cx} cy="72" r="5" />
          ))}
          <rect x="116" y="5" width="18" height="36" rx="1.5" />
          <rect x="138" y="5" width="17" height="36" rx="1.5" />
          <rect x="116" y="44" width="18" height="41" rx="1.5" />
          <rect x="138" y="44" width="17" height="41" rx="1.5" />
        </g>
        <g fill="rgba(255,255,255,0.72)" fontSize="3.4" letterSpacing="0.5" textAnchor="middle">
          <text x="58.5" y="52">CAMPO DE BATALHA</text>
          <text x="58.5" y="81.5">TERRENOS</text>
          <text x="125" y="38">DECK</text>
          <text x="146.5" y="38">VIDA</text>
          <text x="125" y="81.5">EXÍLIO</text>
          <text x="146.5" y="81.5">CEMIT.</text>
        </g>
      </svg>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Fontes
// ---------------------------------------------------------------------------

function ArtStrip({
  candidates,
  selected,
  onPick,
}: {
  candidates: readonly MatCandidate[]
  selected: string | null
  onPick: (url: string) => void
}): ReactElement {
  if (candidates.length === 0) {
    return <p className="text-[11.5px] leading-4 text-ink-faint">Catálogo indisponível — cole uma URL ou fique no feltro liso.</p>
  }
  return (
    <div className="grid grid-cols-4 gap-1.5">
      {candidates.map((entry) => (
        <button
          key={entry.url}
          type="button"
          title={entry.name}
          aria-label={`Usar a arte de ${entry.name}`}
          aria-pressed={entry.url === selected}
          onClick={() => onPick(entry.url)}
          className={clsx(
            'relative h-7 w-full cursor-pointer overflow-hidden rounded-xs ring-1 ring-inset',
            'transition-[transform,box-shadow] duration-[var(--duration-micro)] ease-standard hover:-translate-y-px',
            entry.url === selected
              ? 'ring-accent [box-shadow:0_0_0_1px_var(--accent),0_0_12px_-4px_var(--accent)]'
              : 'ring-edge-hair',
          )}
        >
          <img
            src={entry.url}
            alt=""
            aria-hidden="true"
            draggable={false}
            loading="lazy"
            decoding="async"
            className="size-full object-cover"
          />
        </button>
      ))}
    </div>
  )
}

const URL_MESSAGE: Readonly<Record<UrlStatus, string>> = {
  idle: 'Aceita https:// ou data:image/.',
  checking: 'Carregando a imagem…',
  rejected: 'Endereço recusado: só https:// ou data:image/.',
  broken: 'A imagem não carregou. Confira o endereço.',
}

function UrlField({
  seat,
  value,
  status,
  onChange,
  onApply,
}: {
  seat: Seat
  value: string
  status: UrlStatus
  onChange: (next: string) => void
  onApply: () => void
}): ReactElement {
  const id = `playmat-url-${seat}`
  return (
    <div>
      <div className="flex gap-1.5">
        <input
          id={id}
          type="url"
          inputMode="url"
          spellCheck={false}
          placeholder="https://…"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter') return
            // O seletor vive dentro do formulário da partida: sem isto, Enter
            // iniciaria o duelo em vez de aplicar a arte.
            event.preventDefault()
            onApply()
          }}
          className="metal-well min-w-0 flex-1 rounded-md px-2 py-1.5 text-[12px] text-ink-strong outline-none"
        />
        <button
          type="button"
          onClick={onApply}
          disabled={status === 'checking'}
          className={clsx(
            'metal-plate cursor-pointer rounded-md px-3 py-1.5 text-[12px] font-semibold text-accent-bright',
            'transition-[filter] duration-[var(--duration-micro)] ease-standard hover:brightness-125',
            status === 'checking' && 'cursor-progress opacity-60',
          )}
        >
          Usar
        </button>
      </div>
      <p
        role={status === 'rejected' || status === 'broken' ? 'alert' : undefined}
        className={clsx(
          'mt-1.5 text-[11px] leading-4',
          status === 'rejected' || status === 'broken' ? 'text-danger' : 'text-ink-faint',
        )}
      >
        {URL_MESSAGE[status]}
      </p>
    </div>
  )
}
