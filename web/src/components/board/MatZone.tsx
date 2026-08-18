import clsx from 'clsx'
import type { ReactNode } from 'react'
import type { PlayerId } from '../../types/protocol'
import { pivotOriginY, playmatZone, rectStyle } from './playmatZones'
import type { PlaymatZoneId, Seat } from './playmatZones'

/** O motor numera jogadores; a impressão só conhece dois assentos. */
export function asSeat(player: PlayerId): Seat {
  return player === 1 ? 1 : 0
}

export interface MatZoneProps {
  seat: Seat
  zone: PlaymatZoneId
  /**
   * Ancora a zona na dobradiça da mesa em vez do próprio centro, o que cancela
   * a inclinação e devolve a zona à posição exata que ela teria com a mesa
   * chapada. Só as duas faixas de batalha usam: é o que preserva o pareamento
   * de combate por coluna depois da perspectiva (ver `playmatZones.ts`).
   */
  pinned?: boolean
  className?: string
  children?: ReactNode
}

/**
 * Ancoragem de uma zona IMPRESSA do tapete.
 *
 * A caixa vem de `playmatZones`, a mesma tabela de onde `Playmat` tira o
 * contorno serigrafado, e o mesmo `rectStyle` faz a conversão dos dois lados.
 * Como impressão e carta pintam dentro do MESMO bloco de contenção
 * (`.mat-plane`), a carta pousa dentro da linha por construção: não há medição
 * em JS, e portanto não há desalinhamento para corrigir depois.
 *
 * A zona é contra-rotacionada em `App.css`: o tapete está inclinado, mas o que
 * pousa nele fica DE PÉ, porque quem assiste precisa ler o nome da carta.
 */
export function MatZone({ seat, zone, pinned = false, className, children }: MatZoneProps) {
  const printed = playmatZone(seat, zone)

  return (
    <section
      className={clsx('mat-zone', className)}
      data-zone={zone}
      aria-label={printed.label}
      style={{
        ...rectStyle(printed.rect),
        transformOrigin: pinned ? `50% ${pivotOriginY(seat, zone)}` : '50% 50%',
      }}
    >
      <div className="mat-zone__inner">{children}</div>
    </section>
  )
}
