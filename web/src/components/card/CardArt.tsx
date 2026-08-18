import { useEffect, useId, useMemo, useRef, useState } from 'react'
import clsx from 'clsx'
import type { ReactElement } from 'react'
import type { CardView } from '../../types/protocol'
import type { FramePaint } from './cardVisuals'
import { proceduralArt, scryfallArtUrl } from './cardVisuals'
import './card.css'

type ArtState = 'loading' | 'ready' | 'failed'

/**
 * Só o desfecho, não os bytes: o cache de imagem do browser guarda o arquivo.
 * O que precisa sobreviver à desmontagem é a decisão "já sei que essa URL
 * carrega / não carrega", senão cada volta da carta à mesa recomeça o shimmer.
 */
const RESOLVED = new Map<string, ArtState>()

function initialState(url: string | null): ArtState {
  if (url === null) return 'failed'
  return RESOLVED.get(url) ?? 'loading'
}

export interface CardArtProps {
  card: Pick<CardView, 'artKey' | 'name'>
  paint: FramePaint
  /** Arte já visível na primeira tela não deve esperar o lazy loading. */
  eager?: boolean
  className?: string
}

/**
 * Janela de arte. O fundo procedural é desenhado sempre e a foto do Scryfall
 * entra por cima quando (e se) chegar — assim nenhuma falha de rede consegue
 * produzir uma caixa vazia na mesa.
 */
export function CardArt({ card, paint, eager = false, className }: CardArtProps): ReactElement {
  const url = scryfallArtUrl(card)
  const [state, setState] = useState<ArtState>(() => initialState(url))
  const imgRef = useRef<HTMLImageElement | null>(null)
  const gradientId = useId()

  useEffect(() => {
    setState(initialState(url))
  }, [url])

  // Imagem servida do cache termina antes de o React ligar o onLoad; sem esta
  // conferência a carta ficaria em shimmer permanente.
  useEffect(() => {
    if (url === null) return
    const img = imgRef.current
    if (img !== null && img.complete && img.naturalWidth > 0) {
      RESOLVED.set(url, 'ready')
      setState('ready')
    }
  }, [url])

  const seed = card.artKey ?? card.name ?? 'unknown'
  const art = useMemo(() => proceduralArt(seed, paint), [seed, paint])

  const skyId = `${gradientId}-sky`
  const orbId = `${gradientId}-orb`

  return (
    <div className={clsx('mtgc-art', className)}>
      <svg
        className="mtgc-art__fallback"
        viewBox="0 0 100 72"
        preserveAspectRatio="xMidYMid slice"
        aria-hidden="true"
        focusable="false"
      >
        <defs>
          <linearGradient id={skyId} x1="0" y1="0" x2="0.18" y2="1">
            <stop offset="0%" stopColor={art.skyTop} />
            <stop offset="100%" stopColor={art.skyBottom} />
          </linearGradient>
          <radialGradient id={orbId}>
            <stop offset="0%" stopColor={art.orb} stopOpacity="0.92" />
            <stop offset="55%" stopColor={art.orb} stopOpacity="0.3" />
            <stop offset="100%" stopColor={art.orb} stopOpacity="0" />
          </radialGradient>
        </defs>
        <rect x="0" y="0" width="100" height="72" fill={`url(#${skyId})`} />
        <circle cx={art.orbX} cy={art.orbY} r={art.orbR * 2.6} fill={`url(#${orbId})`} />
        <circle cx={art.orbX} cy={art.orbY} r={art.orbR * 0.42} fill={art.orb} opacity="0.85" />
        {art.ridges.map((ridge, index) => (
          <path key={index} d={ridge.path} fill={ridge.fill} />
        ))}
      </svg>

      {url !== null && state !== 'failed' ? (
        <img
          ref={imgRef}
          className={clsx('mtgc-art__img', state === 'ready' && 'mtgc-art__img--ready')}
          src={url}
          alt=""
          aria-hidden="true"
          loading={eager ? 'eager' : 'lazy'}
          decoding="async"
          draggable={false}
          onLoad={() => {
            RESOLVED.set(url, 'ready')
            setState('ready')
          }}
          onError={() => {
            RESOLVED.set(url, 'failed')
            setState('failed')
          }}
        />
      ) : null}

      {state === 'loading' ? <span className="mtgc-art__shimmer" aria-hidden="true" /> : null}
      <span className="mtgc-art__vignette" aria-hidden="true" />
    </div>
  )
}
