import { useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { CSSProperties, ReactElement } from 'react'
import type { CardView } from '../../types/protocol'
import { Card } from './Card'
import {
  combatRole,
  counterBadges,
  defenderLabel,
  explainKeywords,
  playerAccent,
  ptDisplay,
} from './cardVisuals'
import './card.css'

const GAP = 16
const MARGIN = 12

export interface CardDetailProps {
  card: CardView
  /** Retângulo da carta de origem, em coordenadas de viewport. */
  anchor: DOMRect
}

/**
 * Zoom de hover: a carta em tamanho de leitura mais o que a arte não conta —
 * o que cada palavra-chave faz e em que estado a permanente está agora.
 *
 * Vai para o `body` por portal porque o slot da carta cria contexto de
 * contenção, e um `position: fixed` lá dentro ficaria preso ao slot.
 */
export function CardDetail({ card, anchor }: CardDetailProps): ReactElement | null {
  const panelRef = useRef<HTMLDivElement | null>(null)
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)

  useLayoutEffect(() => {
    const panel = panelRef.current
    if (panel === null) return
    const { width, height } = panel.getBoundingClientRect()
    const viewportWidth = window.innerWidth
    const viewportHeight = window.innerHeight

    let left = anchor.right + GAP
    if (left + width > viewportWidth - MARGIN) left = anchor.left - GAP - width
    if (left < MARGIN) {
      left = Math.max(MARGIN, Math.min(viewportWidth - width - MARGIN, anchor.left))
    }

    const centered = anchor.top + anchor.height / 2 - height / 2
    const top = Math.max(MARGIN, Math.min(viewportHeight - height - MARGIN, centered))

    setPos({ left, top })
  }, [anchor])

  if (typeof document === 'undefined') return null

  const keywords = explainKeywords(card.keywords)
  const notes = stateNotes(card)
  const style: CSSProperties & { '--accent'?: string } = {
    '--accent': playerAccent(card.controller),
    left: pos?.left ?? 0,
    top: pos?.top ?? 0,
    visibility: pos === null ? 'hidden' : 'visible',
  }

  return createPortal(
    <div ref={panelRef} className="mtgc-detail" style={style} role="presentation">
      <Card card={card} size="large" detailOnHover={false} />

      <div className="mtgc-detail__side">
        <h3 className="mtgc-detail__heading">{card.name ?? 'Carta oculta'}</h3>

        <div className="mtgc-detail__meta">
          <span className="mtgc-detail__chip mtgc-detail__chip--accent">
            Jogador {card.controller + 1}
          </span>
          <span className="mtgc-detail__chip">{card.zone}</span>
          <span className="mtgc-detail__chip">VM {card.manaValue}</span>
          {card.rarity ? <span className="mtgc-detail__chip">{card.rarity}</span> : null}
          {card.setCode ? <span className="mtgc-detail__chip">{card.setCode}</span> : null}
          {card.isToken ? <span className="mtgc-detail__chip">Ficha</span> : null}
        </div>

        {keywords.length > 0 ? (
          <ul className="mtgc-detail__keywords">
            {keywords.map((entry) => (
              <li className="mtgc-detail__keyword" key={entry.keyword}>
                <span className="mtgc-detail__keywordName">{entry.keyword}</span>
                {entry.explanation ?? 'Habilidade de palavra-chave desta carta.'}
              </li>
            ))}
          </ul>
        ) : null}

        {notes.length > 0 ? <p className="mtgc-detail__note">{notes.join(' · ')}</p> : null}
      </div>
    </div>,
    document.body,
  )
}

/** Só o que muda de turno a turno; o que está impresso a carta já mostra. */
function stateNotes(card: CardView): string[] {
  const notes: string[] = []
  const pt = ptDisplay(card)

  if (card.tapped) notes.push('Virada')
  if (card.summoningSick) notes.push('Enjoo de invocação')

  if (pt !== null && pt.printed !== null) {
    notes.push(`${pt.power}/${pt.toughness} (impresso ${pt.printed})`)
  }
  if (pt !== null && pt.damage > 0) {
    notes.push(`${pt.damage} de dano marcado, resiste a mais ${Math.max(0, pt.remaining)}`)
  }

  if (card.attacking !== null) notes.push(`Atacando ${defenderLabel(card.attacking)}`)
  if (card.blocking.length > 0) notes.push(`Bloqueando ${card.blocking.length} criatura(s)`)
  if (combatRole(card) === 'blocked') notes.push(`Bloqueada por ${card.blockedBy.length}`)

  for (const badge of counterBadges(card)) {
    notes.push(`${badge.count}x marcador ${badge.label}`)
  }
  if (card.attachments.length > 0) notes.push(`${card.attachments.length} anexo(s)`)
  if (card.isLegalTarget) notes.push('Alvo legal')

  return notes
}
