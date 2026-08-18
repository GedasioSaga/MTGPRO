import { useMemo, useRef, useState } from 'react'
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactElement } from 'react'
import clsx from 'clsx'
import type { CardView } from '../../types/protocol'
import { CardArt } from './CardArt'
import { CardBack } from './CardBack'
import { CardDetail } from './CardDetail'
import { ManaCost, ManaSymbol } from './ManaCost'
import type { CardSize, PtDisplay, TextRun } from './cardVisuals'
import {
  CARD_SIZES,
  cardCssVars,
  combatRole,
  counterBadges,
  framePaintFor,
  parseRulesText,
  ptDisplay,
  setStamp,
} from './cardVisuals'
import './card.css'

export interface CardProps {
  card: CardView
  size?: CardSize
  /** `false` mostra o verso mesmo quando o observador tem os dados. */
  revealed?: boolean
  /** Preenche o contêiner do pai em vez de usar a largura nominal do tamanho. */
  fill?: boolean
  /** Painel de zoom ao passar o mouse. Desligado no próprio zoom. */
  detailOnHover?: boolean
  interactive?: boolean
  className?: string
  style?: CSSProperties
  onSelect?: () => void
}

/**
 * Uma carta de Magic. A mesma marcação serve de 44px a 316px: tudo é derivado
 * de `--cw`, então trocar de tamanho é trocar uma variável, não um layout.
 */
export function Card({
  card,
  size = 'medium',
  revealed = true,
  fill = false,
  detailOnHover = true,
  interactive = false,
  className,
  style,
  onSelect,
}: CardProps): ReactElement {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const [anchor, setAnchor] = useState<DOMRect | null>(null)

  const hidden = !revealed || card.faceDown || card.name === null
  const paint = useMemo(() => framePaintFor(card), [card])

  if (hidden) {
    return (
      <CardBack size={size} fill={fill} seed={String(card.id)} className={className} style={style} />
    )
  }

  const spec = CARD_SIZES[size]
  const compact = spec.layout === 'compact'
  const pt = ptDisplay(card)
  const role = combatRole(card)
  const counters = counterBadges(card)
  const showDetail = detailOnHover && size !== 'large'

  const vars: CSSProperties = {
    ...cardCssVars(paint, size, card.controller, card.rarity),
    ...nameFit(card.name, card.manaCost),
    ...style,
  }

  const clickable = interactive || onSelect !== undefined

  function openDetail(event: PointerEvent<HTMLDivElement>): void {
    if (!showDetail || event.pointerType === 'touch') return
    setAnchor(rootRef.current?.getBoundingClientRect() ?? null)
  }

  function closeDetail(): void {
    setAnchor(null)
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (onSelect === undefined) return
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    onSelect()
  }

  return (
    <div
      ref={rootRef}
      className={clsx(
        'mtgc',
        `mtgc--${size}`,
        compact && 'mtgc--compact',
        fill && 'mtgc--fill',
        paint.secondKind !== null && 'mtgc--dual',
        card.isToken && 'mtgc--token',
        card.tapped && 'mtgc--tapped',
        card.summoningSick && 'mtgc--sick',
        card.isLegalTarget && 'mtgc--target',
        role !== null && `mtgc--${role}`,
        pt !== null && pt.damage > 0 && 'mtgc--damaged',
        pt !== null && pt.lethal && 'mtgc--lethal',
        pt !== null && pt.buffed && 'mtgc--buffed',
        pt !== null && pt.weakened && 'mtgc--weakened',
        clickable && 'mtgc--interactive',
        className,
      )}
      style={vars}
      role={clickable ? 'button' : 'img'}
      tabIndex={clickable ? 0 : undefined}
      aria-label={describe(card, pt)}
      onClick={onSelect}
      onKeyDown={handleKeyDown}
      onPointerEnter={openDetail}
      onPointerLeave={closeDetail}
    >
      <div className="mtgc__tilt">
        <div className="mtgc__bezel">
          <div className="mtgc__frame">
            <div className="mtgc__bar mtgc__title">
              <span className="mtgc__name">{card.name}</span>
              {size === 'micro' ? null : <ManaCost cost={card.manaCost} />}
            </div>

            <div className="mtgc__artwin">
              <CardArt card={card} paint={paint} eager={size === 'large'} />
            </div>

            {compact ? null : (
              <>
                <div className="mtgc__bar mtgc__type">
                  <span className="mtgc__typeText">{card.typeLine ?? ''}</span>
                  {card.isToken ? <span className="mtgc__tokenTag">FICHA</span> : null}
                  <span className="mtgc__stamp">{setStamp(card)}</span>
                </div>

                <div className="mtgc__text">
                  {card.oracleText ? <RulesText text={card.oracleText} /> : null}
                  {/* Criatura baunilha tem a caixa preenchida pelo flavor; caixa
                      vazia na mesa parece carta quebrada. */}
                  {card.flavorText && (spec.showFlavor || !card.oracleText) ? (
                    <>
                      {card.oracleText ? <hr className="mtgc__flavorRule" /> : null}
                      <p className="mtgc__flavor">{card.flavorText}</p>
                    </>
                  ) : null}
                </div>

                <div className="mtgc__foot" />
              </>
            )}
          </div>

          {card.isToken ? <span className="mtgc__fold" aria-hidden="true" /> : null}
          {card.summoningSick ? <span className="mtgc__sickWash" aria-hidden="true" /> : null}

          <StateBadges attacking={role === 'attacking'} blocking={role === 'blocking'} sick={card.summoningSick} />

          {counters.length > 0 ? (
            <div className="mtgc__counters">
              {counters.map((badge) => (
                <span
                  key={`${badge.label}-${badge.tone}`}
                  className={`mtgc__counter mtgc__counter--${badge.tone}`}
                >
                  {badge.label}
                  <span className="mtgc__counterCount">{badge.count}</span>
                </span>
              ))}
            </div>
          ) : null}

          {pt !== null ? <PtBox pt={pt} /> : null}
          {pt === null && card.loyalty !== null ? (
            <div className="mtgc__loyalty">{card.loyalty}</div>
          ) : null}
        </div>

        <span className="mtgc__glow" aria-hidden="true" />
        <span className="mtgc__halo" aria-hidden="true" />
      </div>

      {anchor !== null ? <CardDetail card={card} anchor={anchor} /> : null}
    </div>
  )
}

/**
 * Quadro de P/T. Três informações moram aqui e precisam ser lidas de relance:
 * o valor atual, quanto dano já entrou e qual era o valor impresso.
 */
function PtBox({ pt }: { pt: PtDisplay }): ReactElement {
  const damaged = pt.damage > 0
  const fill = pt.toughness > 0 ? Math.min(100, (pt.damage / pt.toughness) * 100) : 100

  return (
    <>
      {pt.printed !== null ? <span className="mtgc__printedPt">{pt.printed}</span> : null}
      {damaged ? <span className="mtgc__damageChip">-{pt.damage}</span> : null}
      <div className="mtgc__pt">
        {damaged ? <span className="mtgc__ptDamage" style={{ height: `${fill}%` }} /> : null}
        <span className="mtgc__ptValue">
          {pt.power}/
          {damaged ? (
            <span className="mtgc__ptRemain">{Math.max(0, pt.remaining)}</span>
          ) : (
            pt.toughness
          )}
        </span>
      </div>
    </>
  )
}

function RulesText({ text }: { text: string }): ReactElement {
  const lines = parseRulesText(text)
  return (
    <div className="mtgc__rules">
      {lines.map((runs, lineIndex) => (
        <p className="mtgc__rulesLine" key={lineIndex}>
          {runs.map((run, runIndex) => (
            <Run key={runIndex} run={run} />
          ))}
        </p>
      ))}
    </div>
  )
}

function Run({ run }: { run: TextRun }): ReactElement {
  if (run.kind === 'symbol') return <ManaSymbol token={run.token} />
  if (run.kind === 'reminder') return <em className="mtgc__reminder">{run.text}</em>
  return <>{run.text}</>
}

function StateBadges({
  attacking,
  blocking,
  sick,
}: {
  attacking: boolean
  blocking: boolean
  sick: boolean
}): ReactElement | null {
  if (!attacking && !blocking && !sick) return null
  return (
    <div className="mtgc__badges">
      {attacking ? (
        <span className="mtgc__badge mtgc__badge--attack" title="Atacando">
          <SwordIcon />
        </span>
      ) : null}
      {blocking ? (
        <span className="mtgc__badge mtgc__badge--block" title="Bloqueando">
          <ShieldIcon />
        </span>
      ) : null}
      {sick ? (
        <span className="mtgc__badge mtgc__badge--sick" title="Enjoo de invocação">
          <HourglassIcon />
        </span>
      ) : null}
    </div>
  )
}

function SwordIcon(): ReactElement {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M21.6 2.4h-3.3l-8.2 8.2 3.3 3.3 8.2-8.2V2.4ZM8.9 12.1l-2.6 2.6 3.3 3.3 2.6-2.6-3.3-3.3ZM5.2 15.8 2 19l3 3 3.2-3.2-3-3Z" />
    </svg>
  )
}

function ShieldIcon(): ReactElement {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M12 2.2 4.2 5v6.4c0 4.7 3.2 8.6 7.8 10.4 4.6-1.8 7.8-5.7 7.8-10.4V5L12 2.2Zm0 2.3 5.6 2v4.9c0 3.5-2.2 6.5-5.6 8.1V4.5Z" />
    </svg>
  )
}

function HourglassIcon(): ReactElement {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
      <path d="M6 2.4h12v2.1h-1.2c0 2.8-1.2 5-3.1 6.1v2.8c1.9 1.1 3.1 3.3 3.1 6.1H18v2.1H6v-2.1h1.2c0-2.8 1.2-5 3.1-6.1v-2.8C8.4 9.5 7.2 7.3 7.2 4.5H6V2.4Zm3.3 2.1c0 2.2 1.1 3.9 2.7 4.7 1.6-.8 2.7-2.5 2.7-4.7H9.3Zm2.7 10.3c-1.6.8-2.7 2.5-2.7 4.7h5.4c0-2.2-1.1-3.9-2.7-4.7Z" />
    </svg>
  )
}

/**
 * A carta impressa aperta a fonte do nome em vez de cortá-lo, e reticências no
 * lugar de metade do nome custam caro num tabuleiro que se lê de relance.
 */
function nameFit(name: string | null, manaCost: string | null): { '--name-fit': string } {
  const symbols = (manaCost?.match(/\{/g) ?? []).length
  const budget = Math.max(8, 15 - symbols)
  const length = name?.length ?? 0
  const fit = length <= budget ? 1 : Math.max(0.6, budget / length)
  return { '--name-fit': fit.toFixed(3) }
}

function describe(card: CardView, pt: PtDisplay | null): string {
  const parts: string[] = [card.name ?? 'carta']
  if (card.manaCost) parts.push(card.manaCost)
  if (card.typeLine) parts.push(card.typeLine)
  if (pt !== null) {
    parts.push(`${pt.power}/${pt.toughness}`)
    if (pt.damage > 0) parts.push(`${pt.damage} de dano marcado`)
  }
  if (card.tapped) parts.push('virada')
  if (card.summoningSick) parts.push('com enjoo de invocação')
  if (card.attacking !== null) parts.push('atacando')
  if (card.blocking.length > 0) parts.push('bloqueando')
  return parts.join(', ')
}
