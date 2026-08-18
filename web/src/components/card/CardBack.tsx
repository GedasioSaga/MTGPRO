import { useId } from 'react'
import clsx from 'clsx'
import type { CSSProperties, ReactElement } from 'react'
import type { CardSize } from './cardVisuals'
import { hashString } from '../../design/color'
import { CARD_SIZES } from './cardVisuals'
import './card.css'

export interface CardBackProps {
  size?: CardSize
  /** Preenche o contêiner do pai em vez de usar a largura nominal do tamanho. */
  fill?: boolean
  /** Muda de leve o miolo do padrão, para a pilha não parecer fotocópia. */
  seed?: string
  className?: string
  style?: CSSProperties
}

/**
 * Verso desenhado em SVG: couro escuro, cercadura dourada e roseta central.
 * Nada de imagem externa — o verso é o que aparece quando a rede falha, então
 * ele mesmo não pode depender da rede.
 */
export function CardBack({
  size = 'medium',
  fill = false,
  seed = 'back',
  className,
  style,
}: CardBackProps): ReactElement {
  const gradientId = useId()
  const hash = hashString(seed)
  const spin = hash % 60
  const petals = 8 + (hash >> 6) % 5

  const vars: CSSProperties & { '--cw-nominal'?: string } = {
    '--cw-nominal': `${CARD_SIZES[size].width}px`,
    ...style,
  }

  const leatherId = `${gradientId}-leather`
  const goldId = `${gradientId}-gold`

  return (
    <div
      className={clsx('mtgc-back', fill && 'mtgc-back--fill', className)}
      style={vars}
      aria-label="Carta virada para baixo"
      role="img"
    >
      <svg viewBox="0 0 100 140" preserveAspectRatio="none" aria-hidden="true" focusable="false">
        <defs>
          <radialGradient id={leatherId} cx="0.5" cy="0.32" r="0.85">
            <stop offset="0%" stopColor="#3b2a1e" />
            <stop offset="52%" stopColor="#1b120d" />
            <stop offset="100%" stopColor="#080505" />
          </radialGradient>
          <linearGradient id={goldId} x1="0" y1="0" x2="0.4" y2="1">
            <stop offset="0%" stopColor="#e8cf94" />
            <stop offset="48%" stopColor="#9a7b3d" />
            <stop offset="100%" stopColor="#5d4620" />
          </linearGradient>
        </defs>

        <rect x="0" y="0" width="100" height="140" fill={`url(#${leatherId})`} />

        <rect
          x="4.5"
          y="4.5"
          width="91"
          height="131"
          rx="4"
          fill="none"
          stroke={`url(#${goldId})`}
          strokeWidth="1.6"
        />
        <rect
          x="8"
          y="8"
          width="84"
          height="124"
          rx="3"
          fill="none"
          stroke="rgba(232, 207, 148, 0.22)"
          strokeWidth="0.6"
        />

        <g transform={`rotate(${spin} 50 70)`} opacity="0.9">
          {Array.from({ length: petals }, (_, index) => (
            <ellipse
              key={index}
              cx="50"
              cy="46"
              rx="6.5"
              ry="21"
              fill="none"
              stroke={`url(#${goldId})`}
              strokeWidth="0.9"
              transform={`rotate(${(index * 360) / petals} 50 70)`}
            />
          ))}
        </g>

        <circle cx="50" cy="70" r="12" fill="rgba(8, 5, 5, 0.7)" />
        <circle cx="50" cy="70" r="12" fill="none" stroke={`url(#${goldId})`} strokeWidth="1.3" />
        <circle cx="50" cy="70" r="5.2" fill={`url(#${goldId})`} opacity="0.75" />
        <circle cx="47.4" cy="67.4" r="1.7" fill="rgba(255, 246, 220, 0.75)" />
      </svg>
    </div>
  )
}
