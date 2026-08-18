import { clsx } from 'clsx'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { PointerEvent as ReactPointerEvent, ReactNode } from 'react'

const CURSOR_OFFSET_X = 16
const CURSOR_OFFSET_Y = 18
const VIEWPORT_MARGIN = 10
/** Fora da tela ate a primeira medicao, para nao piscar no canto superior. */
const PARKED = 'translate3d(-9999px, -9999px, 0)'

export interface TooltipProps {
  content: ReactNode
  children: ReactNode
  /** Atraso ate aparecer. Curto o bastante para nao parecer travado. */
  delayMs?: number
  disabled?: boolean
  /** Largura maxima do balao. */
  maxWidth?: number
  className?: string
}

/**
 * Balao que segue o cursor e nunca vaza da janela: ele se mede depois de
 * montar e vira para o lado oposto quando nao cabe. Com teclado, ancora na
 * base do elemento em foco.
 */
export function Tooltip({
  content,
  children,
  delayMs = 120,
  disabled = false,
  maxWidth = 260,
  className,
}: TooltipProps) {
  const [open, setOpen] = useState(false)
  const anchorRef = useRef<HTMLSpanElement>(null)
  const bubbleRef = useRef<HTMLDivElement>(null)
  const pointer = useRef({ x: 0, y: 0 })
  const frame = useRef(0)
  const timer = useRef(0)

  const place = useCallback(() => {
    const el = bubbleRef.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    let x = pointer.current.x + CURSOR_OFFSET_X
    let y = pointer.current.y + CURSOR_OFFSET_Y
    if (x + rect.width + VIEWPORT_MARGIN > window.innerWidth) {
      x = pointer.current.x - rect.width - CURSOR_OFFSET_X
    }
    if (y + rect.height + VIEWPORT_MARGIN > window.innerHeight) {
      y = pointer.current.y - rect.height - CURSOR_OFFSET_Y
    }
    x = Math.max(VIEWPORT_MARGIN, x)
    y = Math.max(VIEWPORT_MARGIN, y)
    el.style.transform = `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`
  }, [])

  const schedule = useCallback(() => {
    if (frame.current !== 0) return
    frame.current = window.requestAnimationFrame(() => {
      frame.current = 0
      place()
    })
  }, [place])

  const show = useCallback(() => {
    if (disabled) return
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setOpen(true), delayMs)
  }, [delayMs, disabled])

  const hide = useCallback(() => {
    window.clearTimeout(timer.current)
    setOpen(false)
  }, [])

  const handlePointerMove = useCallback(
    (event: ReactPointerEvent<HTMLSpanElement>) => {
      pointer.current = { x: event.clientX, y: event.clientY }
      if (open) schedule()
    },
    [open, schedule],
  )

  const handleFocus = useCallback(() => {
    const anchor = anchorRef.current
    if (!anchor) return
    const rect = anchor.getBoundingClientRect()
    pointer.current = { x: rect.left + rect.width / 2, y: rect.bottom }
    show()
  }, [show])

  useLayoutEffect(() => {
    if (open) place()
  }, [open, place, content])

  useEffect(() => {
    if (!open) return
    const reposition = () => schedule()
    window.addEventListener('scroll', reposition, true)
    window.addEventListener('resize', reposition)
    return () => {
      window.removeEventListener('scroll', reposition, true)
      window.removeEventListener('resize', reposition)
    }
  }, [open, schedule])

  useEffect(
    () => () => {
      window.clearTimeout(timer.current)
      if (frame.current !== 0) window.cancelAnimationFrame(frame.current)
    },
    [],
  )

  useEffect(() => {
    if (disabled) setOpen(false)
  }, [disabled])

  return (
    <>
      <span
        ref={anchorRef}
        className={clsx('inline-flex', className)}
        onPointerEnter={show}
        onPointerMove={handlePointerMove}
        onPointerLeave={hide}
        onPointerDown={hide}
        onFocus={handleFocus}
        onBlur={hide}
      >
        {children}
      </span>

      {open
        ? createPortal(
            <div
              ref={bubbleRef}
              role="tooltip"
              className="pointer-events-none fixed top-0 left-0 z-[300]"
              style={{ transform: PARKED }}
            >
              <div
                className="glass animate-rise rounded-md px-2.5 py-1.5 text-[12.5px] leading-snug text-ink-strong"
                style={{ maxWidth }}
              >
                {content}
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  )
}
