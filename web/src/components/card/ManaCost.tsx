import type { CSSProperties, ReactElement } from 'react'
import clsx from 'clsx'
import type { ManaColor, ManaToken } from './cardVisuals'
import { parseManaCost } from './cardVisuals'
import './card.css'

/**
 * Disco pleno na cor oficial, sem contorno, com o glifo vazado em branco — a
 * forma de `docs/reference-mana-symbols.png`. Nada de imagem externa: o símbolo
 * precisa aparecer mesmo com a rede caída.
 */
const DISC: Record<ManaColor, string> = {
  W: '#e39f13',
  U: '#2f52a0',
  B: '#4a0d6b',
  R: '#8e1424',
  G: '#12663a',
}

const RAY_ANGLES = [0, 45, 90, 135, 180, 225, 270, 315]

/** Um raio curvo do sol, apontando para cima a partir do centro (50,50). */
const SUN_RAY = 'M50 26 C44 23 39 15 40 4 C47 11 54 13 61 10 C57 17 56 21 56 27 Z'

/**
 * Glifo vetorial de cada cor, em viewBox 100x100. Tudo desenhado com
 * `currentColor` para que o mesmo caminho sirva no disco colorido (tinta
 * branca) e na metade cinza do híbrido (tinta escura).
 */
function ColorGlyph({ color, className }: { color: ManaColor; className: string }): ReactElement {
  const common = { viewBox: '0 0 100 100', className, 'aria-hidden': true } as const

  if (color === 'W') {
    return (
      <svg {...common}>
        {RAY_ANGLES.map((angle) => (
          <path key={angle} d={SUN_RAY} transform={`rotate(${angle} 50 50)`} />
        ))}
        <circle cx="50" cy="50" r="15" fill="none" stroke="currentColor" strokeWidth="5" />
      </svg>
    )
  }

  if (color === 'U') {
    return (
      <svg {...common} fillRule="evenodd" clipRule="evenodd">
        <path d="M50 5C50 5 19 40 19 58a31 31 0 0 0 62 0C81 40 50 5 50 5Zm16 52c5 4 6 11 3 16-3 5-8 6-11 4 5-1 9-5 10-10 1-4 0-7-2-10Z" />
      </svg>
    )
  }

  if (color === 'B') {
    return (
      <svg {...common} fillRule="evenodd" clipRule="evenodd">
        <path d="M50 15C44 6 33 3 25 8 13 15 7 28 7 42c0 11 5 21 13 27v11c0 5 4 9 9 9h4v-9h6v9h6v-9h6v9h4c5 0 9-4 9-9V69c8-6 13-16 13-27 0-14-6-27-18-34-8-5-19-2-25 7Zm-16 25a11 11 0 1 1 0 22 11 11 0 0 1 0-22Zm32 0a11 11 0 1 1 0 22 11 11 0 0 1 0-22ZM50 66l7 14H43l7-14Z" />
      </svg>
    )
  }

  if (color === 'R') {
    return (
      <svg {...common} fillRule="evenodd" clipRule="evenodd">
        <path d="M60 4c12 17 8 30-1 41-8 10-17 18-17 30 0 13 10 23 23 23 14 0 25-12 25-27 0-10-3-18-9-25 1 8-2 13-7 15 5-17-5-38-14-57Zm-2 51c10 3 16 11 16 20 0 9-7 16-15 16-7 0-12-5-12-11 0-5 4-9 9-9 4 0 6 3 6 6 0 2-1 3-2 4 3 2 7 0 9-4 3-7-3-16-11-22Z" />
        <path d="M36 46c-7 7-12 15-12 24 0 11 7 19 17 20-8-9-11-26-5-44Z" />
      </svg>
    )
  }

  return (
    <svg {...common} fillRule="evenodd" clipRule="evenodd">
      <path d="M50 8C40 3 28 5 23 13 13 13 6 21 7 31 1 36 0 46 6 52c2 8 10 13 19 11h50c9 2 17-3 19-11 6-6 5-16-1-21 1-10-6-18-16-18C72 5 60 3 50 8Z" />
      <path d="M40 54h20v22c12 3 20 9 24 16H16c4-7 12-13 24-16V54Zm-6 38 9-14 1 14h-10Zm32 0-9-14-1 14h10Z" />
    </svg>
  )
}

function ColorlessGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 100 100" className={className} aria-hidden fillRule="evenodd" clipRule="evenodd">
      <path d="M50 6 88 50 50 94 12 50 50 6Zm0 18L30 50l20 26 20-26-20-26Z" />
    </svg>
  )
}

function SnowGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg
      viewBox="0 0 100 100"
      className={className}
      aria-hidden
      fill="none"
      stroke="currentColor"
      strokeWidth="8"
      strokeLinecap="round"
    >
      {[0, 60, 120].map((angle) => (
        <g key={angle} transform={`rotate(${angle} 50 50)`}>
          <line x1="50" y1="10" x2="50" y2="90" />
          <line x1="50" y1="24" x2="38" y2="12" />
          <line x1="50" y1="24" x2="62" y2="12" />
        </g>
      ))}
    </svg>
  )
}

function TapGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 100 100" className={className} aria-hidden>
      <path d="M50 16a34 34 0 1 0 32 22" fill="none" stroke="currentColor" strokeWidth="15" />
      <path d="M44 1 79 17 44 33Z" />
    </svg>
  )
}

function PhyrexianGlyph({ className }: { className: string }): ReactElement {
  return (
    <svg viewBox="0 0 100 100" className={className} aria-hidden fillRule="evenodd" clipRule="evenodd">
      <path d="M50 8c-19 0-33 13-33 31 0 12 7 21 15 26l-7 30 20-17h10l20 17-7-30c8-5 15-14 15-26C83 21 69 8 50 8ZM38 31a9 9 0 1 1 0 18 9 9 0 0 1 0-18Zm24 0a9 9 0 1 1 0 18 9 9 0 0 1 0-18Z" />
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
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--generic', className)}
          style={style}
          aria-label={`${token.text} de mana genérico`}
        >
          <span className="mtgc-sym__text">{token.text}</span>
        </span>
      )
    case 'variable':
      return (
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--variable', className)}
          style={style}
          aria-label={`${token.letter} de mana`}
        >
          <span className="mtgc-sym__text">{token.letter}</span>
        </span>
      )
    case 'color':
      return (
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--color', className)}
          style={{ ...style, background: DISC[token.color] }}
          aria-label={`mana ${token.color}`}
        >
          <ColorGlyph color={token.color} className="mtgc-sym__glyph" />
        </span>
      )
    case 'hybrid':
      return (
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--split', className)}
          style={style}
          aria-label={`mana híbrido ${token.a} ou ${token.b}`}
        >
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
          <span className="mtgc-sym__half mtgc-sym__half--a mtgc-sym__half--grey" />
          <span className="mtgc-sym__half mtgc-sym__half--b" style={{ background: DISC[token.color] }} />
          <span className="mtgc-sym__miniText">{token.generic}</span>
          <ColorGlyph color={token.color} className="mtgc-sym__mini mtgc-sym__mini--b" />
        </span>
      )
    case 'phyrexian':
      return (
        <span
          className={clsx('mtgc-sym', 'mtgc-sym--color', className)}
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
