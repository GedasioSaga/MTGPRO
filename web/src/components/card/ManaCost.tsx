import type { CSSProperties, ReactElement } from 'react'
import clsx from 'clsx'
import type { ManaColor, ManaToken } from './cardVisuals'
import { parseManaCost } from './cardVisuals'
import './card.css'

/**
 * Discos pastel com glifo escuro, como no símbolo impresso. Nada de imagem
 * externa: o símbolo precisa aparecer mesmo com a rede caída.
 */
const DISC: Record<ManaColor, string> = {
  W: 'linear-gradient(180deg, #fffef0, #ece2bd)',
  U: 'linear-gradient(180deg, #cdeaff, #93c8ea)',
  B: 'linear-gradient(180deg, #ded6d2, #b0a5a1)',
  R: 'linear-gradient(180deg, #ffc6ac, #ef9377)',
  G: 'linear-gradient(180deg, #c2ecc9, #86c39a)',
}

const RAY_ANGLES = [0, 45, 90, 135, 180, 225, 270, 315]

/** Glifo vetorial de cada cor, em viewBox 24x24. */
function ColorGlyph({ color, className }: { color: ManaColor; className: string }): ReactElement {
  const common = { viewBox: '0 0 24 24', className, 'aria-hidden': true } as const

  if (color === 'W') {
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="4.4" />
        {RAY_ANGLES.map((angle) => (
          <path key={angle} d="M12 1.4 L14 6.6 L10 6.6 Z" transform={`rotate(${angle} 12 12)`} />
        ))}
      </svg>
    )
  }

  if (color === 'U') {
    return (
      <svg {...common}>
        <path d="M12 1.8C12 1.8 4.6 10.2 4.6 14.5a7.4 7.4 0 0 0 14.8 0C19.4 10.2 12 1.8 12 1.8Z" />
      </svg>
    )
  }

  if (color === 'B') {
    return (
      <svg {...common} fillRule="evenodd" clipRule="evenodd">
        <path
          d="M12 2.1c-4.7 0-8.1 3.3-8.1 7.8 0 2.5 1.1 4.5 2.7 5.7v2.6c0 1.2.9 2.1 2.1 2.1h6.6c1.2 0 2.1-.9 2.1-2.1v-2.6c1.6-1.2 2.7-3.2 2.7-5.7 0-4.5-3.4-7.8-8.1-7.8Zm-2.5 5.4a2.1 2.1 0 1 1 0 4.2 2.1 2.1 0 0 1 0-4.2Zm5 0a2.1 2.1 0 1 1 0 4.2 2.1 2.1 0 0 1 0-4.2Zm-2.5 6.2 1.5 2.9h-3l1.5-2.9Z"
        />
      </svg>
    )
  }

  if (color === 'R') {
    return (
      <svg {...common}>
        <path d="M13.1 1.6c2.3 3.6.9 5.7-.6 7.5-1.4 1.7-2.9 3.3-2.9 5.8 0 3.5 2.4 6.1 5.4 6.1 3.1 0 5.6-2.6 5.6-6.2 0-2.2-.8-4-2-5.5.1 1.7-.5 2.7-1.4 3.1.9-3.6-1.1-8.3-4.1-10.8Z" />
        <path d="M9.6 11.8c-1.4 1.4-2.4 3-2.4 4.8 0 2.4 1.5 4.1 3.6 4.4-1.6-2-2-5.6-1.2-9.2Z" />
      </svg>
    )
  }

  return (
    <svg {...common}>
      <path d="M12 1.9c-3.7 0-6.7 2.9-6.7 6.5 0 2.8 1.9 5.2 4.5 6.1l-.8 7.1h6l-.8-7.1c2.6-.9 4.5-3.3 4.5-6.1 0-3.6-3-6.5-6.7-6.5Z" />
    </svg>
  )
}

function ColorlessGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden>
      <path d="M12 1.8 21 12l-9 10.2L3 12 12 1.8Zm0 4.4L7 12l5 5.8 5-5.8-5-5.8Z" />
    </svg>
  )
}

function SnowGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden fill="none" stroke="#150e07" strokeWidth="1.9" strokeLinecap="round">
      {[0, 60, 120].map((angle) => (
        <g key={angle} transform={`rotate(${angle} 12 12)`}>
          <line x1="12" y1="2.6" x2="12" y2="21.4" />
          <line x1="12" y1="5.4" x2="9.2" y2="3" />
          <line x1="12" y1="5.4" x2="14.8" y2="3" />
        </g>
      ))}
    </svg>
  )
}

function TapGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden>
      <path d="M12 2.6a9.4 9.4 0 1 0 8.1 14.1l-2.6-1.5A6.4 6.4 0 1 1 12 5.6v3.6l5.6-3.3L12 2.6Z" />
      <path d="M13.6 11.6h2.6l-4.4 8.6-.4-5.4h-2.6l4-8.4.8 5.2Z" />
    </svg>
  )
}

function PhyrexianGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden fillRule="evenodd" clipRule="evenodd">
      <path d="M12 2.2c-4.2 0-7.4 3-7.4 7 0 2.7 1.5 4.7 3.3 5.9L6.4 21.8l4.3-3.9h2.6l4.3 3.9-1.5-6.7c1.8-1.2 3.3-3.2 3.3-5.9 0-4-3.2-7-7.4-7Zm-2.6 5a1.9 1.9 0 1 1 0 3.8 1.9 1.9 0 0 1 0-3.8Zm5.2 0a1.9 1.9 0 1 1 0 3.8 1.9 1.9 0 0 1 0-3.8Z" />
    </svg>
  )
}

export interface ManaSymbolProps {
  token: ManaToken
  /** Comprimento CSS do disco; herda `1em` quando não informado. */
  size?: string
  className?: string
}

/** Um único símbolo. Exportado porque o oracle text também precisa dele. */
export function ManaSymbol({ token, size, className }: ManaSymbolProps): ReactElement {
  const style: CSSProperties & { '--sym'?: string } = size ? { '--sym': size } : {}

  switch (token.kind) {
    case 'generic':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--generic', className)} style={style} aria-label={`${token.text} de mana genérico`}>
          <span className="mtgc-sym__text">{token.text}</span>
        </span>
      )
    case 'variable':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--variable', className)} style={style} aria-label={`${token.letter} de mana`}>
          <span className="mtgc-sym__text">{token.letter}</span>
        </span>
      )
    case 'color':
      return (
        <span
          className={clsx('mtgc-sym', className)}
          style={{ ...style, background: DISC[token.color] }}
          aria-label={`mana ${token.color}`}
        >
          <ColorGlyph color={token.color} className="mtgc-sym__glyph" />
        </span>
      )
    case 'hybrid':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--split', className)} style={style} aria-label={`mana híbrido ${token.a} ou ${token.b}`}>
          <span className="mtgc-sym__half mtgc-sym__half--a" style={{ background: DISC[token.a] }} />
          <span className="mtgc-sym__half mtgc-sym__half--b" style={{ background: DISC[token.b] }} />
          <ColorGlyph color={token.a} className="mtgc-sym__mini mtgc-sym__mini--a" />
          <ColorGlyph color={token.b} className="mtgc-sym__mini mtgc-sym__mini--b" />
        </span>
      )
    case 'monoHybrid':
      return (
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--split', className)}
          style={style}
          aria-label={`mana ${token.generic} genérico ou ${token.color}`}
        >
          <span className="mtgc-sym__half mtgc-sym__half--a" style={{ background: 'linear-gradient(180deg, #d7cfcc, #b3a8a4)' }} />
          <span className="mtgc-sym__half mtgc-sym__half--b" style={{ background: DISC[token.color] }} />
          <span className="mtgc-sym__miniText">{token.generic}</span>
          <ColorGlyph color={token.color} className="mtgc-sym__mini mtgc-sym__mini--b" />
        </span>
      )
    case 'phyrexian':
      return (
        <span
          className={clsx('mtgc-sym', className)}
          style={{ ...style, background: DISC[token.color] }}
          aria-label={`mana phyrexiano ${token.color}`}
        >
          <PhyrexianGlyph className="mtgc-sym__glyph" />
        </span>
      )
    case 'colorless':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--colorless', className)} style={style} aria-label="mana incolor">
          <ColorlessGlyph className="mtgc-sym__glyph" />
        </span>
      )
    case 'snow':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--snow', className)} style={style} aria-label="mana da neve">
          <SnowGlyph className="mtgc-sym__glyph" />
        </span>
      )
    case 'tap':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--tap', className)} style={style} aria-label="vire">
          <TapGlyph className="mtgc-sym__glyph" />
        </span>
      )
    case 'untap':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--untap', className)} style={style} aria-label="desvire">
          <TapGlyph className="mtgc-sym__glyph" />
        </span>
      )
    case 'text':
      return (
        <span className={clsx('mtgc-sym', 'mtgc-sym--text', className)} style={style}>
          <span className="mtgc-sym__text">{token.text}</span>
        </span>
      )
  }
}

export interface ManaCostProps {
  /** Custo cru vindo do motor, como `"{2}{W}{W}"`. */
  cost: string | null
  /** Comprimento CSS de cada disco. */
  size?: string
  className?: string
}

export function ManaCost({ cost, size, className }: ManaCostProps): ReactElement | null {
  const tokens = parseManaCost(cost)
  if (tokens.length === 0) return null
  const style: CSSProperties & { '--sym'?: string } = size ? { '--sym': size } : {}
  return (
    <span className={clsx('mtgc-cost', className)} style={style} aria-label={`custo ${cost ?? ''}`}>
      {tokens.map((token, index) => (
        <ManaSymbol key={`${token.kind}-${index}`} token={token} />
      ))}
    </span>
  )
}
