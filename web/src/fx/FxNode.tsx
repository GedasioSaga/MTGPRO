import type { CSSProperties, ReactNode } from 'react'
import type { FxPoint } from './fxTypes'

interface FxNodeProps {
  at: FxPoint
  children: ReactNode
  /** `true` centra o conteudo no ponto; `false` ancora o canto superior esquerdo. */
  pin?: boolean
  className?: string
  style?: CSSProperties
}

/**
 * Ancora de overlay. A posicao vive num `translate3d` do wrapper, entao o
 * conteudo pode animar o proprio transform sem recalcular layout nem perder o
 * ponto de origem.
 */
export function FxNode({ at, children, pin = true, className, style }: FxNodeProps) {
  return (
    <div
      className={className === undefined ? 'fx-node' : `fx-node ${className}`}
      style={{ transform: `translate3d(${at.x}px, ${at.y}px, 0)`, ...style }}
    >
      {pin ? <div className="fx-pin">{children}</div> : children}
    </div>
  )
}
