import clsx from 'clsx'
import type { ReactNode } from 'react'
import type { PlayerId } from '../../types/protocol'
import { playmatZone, rectStyle } from './playmatZones'
import type { PlaymatZoneId, Seat } from './playmatZones'

/** O motor numera jogadores; a impressão só conhece dois assentos. */
export function asSeat(player: PlayerId): Seat {
  return player === 1 ? 1 : 0
}

export interface MatZoneProps {
  seat: Seat
  zone: PlaymatZoneId
  className?: string
  children?: ReactNode
}

/**
 * Ancoragem de uma zona IMPRESSA do tapete.
 *
 * A caixa vem de `playmatZones`, a mesma tabela de onde `Playmat` tira o
 * contorno serigrafado, e o mesmo `rectStyle` faz a conversão dos dois lados.
 * Como impressão e carta pintam dentro do MESMO bloco de contenção
 * (`.playmat-seat`), a carta pousa dentro da linha por construção: não há
 * medição em JS, e portanto não há desalinhamento para corrigir depois.
 */
export function MatZone({ seat, zone, className, children }: MatZoneProps) {
  const printed = playmatZone(seat, zone)

  return (
    <section
      className={clsx('mat-zone', className)}
      data-zone={zone}
      aria-label={printed.label}
      style={rectStyle(printed.rect)}
    >
      <div className="mat-zone__inner">{children}</div>
    </section>
  )
}
