import type { ReactElement } from 'react'
import type { KeywordIcon } from './cardVisuals'
import { keywordBadges } from './cardVisuals'
import './card.css'

/**
 * Estado de combate lido sem abrir a carta: um círculo escuro com glifo branco
 * por palavra-chave, encostado na borda esquerda. Vive FORA da camada que gira
 * (`__tilt`), pelo mesmo motivo do quadro de P/T — carta virada gira 90 graus e
 * um glifo deitado não se lê.
 */

/** Acima disso a coluna passa da altura da carta virada; o resto vira `+N`. */
const MAX_BADGES = 4

const GLYPHS: Record<KeywordIcon, ReactElement> = {
  flying: (
    <g fill="none" stroke="currentColor" strokeWidth="12" strokeLinecap="round">
      <path d="M6 64C20 64 32 54 40 34" />
      <path d="M94 64C80 64 68 54 60 34" />
    </g>
  ),
  haste: <path d="M58 4 24 58h19l-7 38 35-54H52l6-38Z" />,
  vigilance: (
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M50 22c24 0 42 16 48 28-6 12-24 28-48 28S8 62 2 50c6-12 24-28 48-28Zm0 12a16 16 0 1 0 0 32 16 16 0 0 0 0-32Zm0 9a7 7 0 1 1 0 14 7 7 0 0 1 0-14Z"
    />
  ),
  trample: (
    <path d="M32 8c9 0 16 15 16 32S41 72 32 72 16 57 16 40 23 8 32 8Zm36 0c9 0 16 15 16 32S77 72 68 72 52 57 52 40 59 8 68 8ZM12 82h76v11H12V82Z" />
  ),
  deathtouch: (
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M50 6C28 6 12 22 12 44c0 12 6 23 15 30v10c0 5 4 9 9 9h3v-9h5v9h6v-9h5v9h3c5 0 9-4 9-9V74c9-7 15-18 15-30C82 22 66 6 50 6Zm-16 27a11 11 0 1 1 0 22 11 11 0 0 1 0-22Zm32 0a11 11 0 1 1 0 22 11 11 0 0 1 0-22ZM50 60l7 14H43l7-14Z"
    />
  ),
  lifelink: (
    <path d="M50 90C22 69 7 54 7 36 7 21 19 9 33 9c8 0 14 4 17 10 3-6 9-10 17-10 14 0 26 12 26 27 0 18-15 33-43 54Z" />
  ),
  firstStrike: (
    <path d="M62 4 76 30v42H48V30L62 4ZM40 78h44v9H40v-9Zm14 9h16v11H54V87ZM18 24h16v9H18v-9ZM6 46h28v9H6v-9Zm12 22h16v9H18v-9Z" />
  ),
  doubleStrike: (
    <path d="M28 4 40 28v44H16V28L28 4Zm44 0 12 24v44H60V28L72 4ZM12 78h32v9H12v-9Zm44 0h32v9H56v-9ZM22 87h12v11H22V87Zm44 0h12v11H66V87Z" />
  ),
  menace: (
    <path d="M4 50 34 24v17h10v18H34v17L4 50Zm92 0L66 24v17H56v18h10v17l30-26Z" />
  ),
  defender: (
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M50 4 10 19v31c0 23 17 40 40 47 23-7 40-24 40-47V19L50 4Zm-3 12h6v18h-6V16ZM12 38h76v6H12v-6Zm14 6h6v18h-6V44Zm42 0h6v18h-6V44ZM12 62h76v6H12v-6Zm35 6h6v20h-6V68Z"
    />
  ),
}

export interface KeywordBadgesProps {
  keywords: string[]
}

export function KeywordBadges({ keywords }: KeywordBadgesProps): ReactElement | null {
  const badges = keywordBadges(keywords)
  if (badges.length === 0) return null

  const shown = badges.slice(0, MAX_BADGES)
  const hidden = badges.length - shown.length

  return (
    <div className="mtgc__kws">
      {shown.map((badge) => (
        <span key={badge.icon} className="mtgc__kw" title={badge.label} aria-label={badge.label}>
          <svg viewBox="0 0 100 100" fill="currentColor" aria-hidden="true" focusable="false">
            {GLYPHS[badge.icon]}
          </svg>
        </span>
      ))}
      {hidden > 0 ? (
        <span className="mtgc__kw mtgc__kw--more" title={`mais ${hidden}`}>
          +{hidden}
        </span>
      ) : null}
    </div>
  )
}
