import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { MatchEvent } from '../types/protocol'
import { clamp } from './fxMotion'
import type { FxEffect, FxSpec } from './fxTypes'
import { FX_PRIORITY } from './fxTypes'
import { translateEvent } from './translateEvent'
import type { FxTranslateContext } from './translateEvent'

/** Acima disso a tela vira sopa; a rajada encurta em vez de enfileirar. */
const SOFT_CAP = 8
/** Teto duro: o excedente e descartado por prioridade, nunca adiado. */
const HARD_CAP = 16
/** Piso de compressao — abaixo disso a animacao deixa de ser legivel. */
const MIN_SCALE = 0.4

/** Efeitos de tela cheia nao empilham: o mais forte vence. */
const SINGLETON_KINDS = new Set(['vignette', 'screenShake', 'turnBanner', 'gameOver'])

function intensityOf(effect: FxEffect): number {
  return effect.kind === 'vignette' || effect.kind === 'screenShake' ? effect.intensity : 1
}

function byPriority(a: FxEffect, b: FxEffect): number {
  return FX_PRIORITY[b.kind] - FX_PRIORITY[a.kind]
}

export interface FxEngine {
  effects: FxEffect[]
  /** Traduz e agenda eventos do motor. */
  ingest: (events: readonly MatchEvent[]) => void
  /** Agenda efeitos ja traduzidos — usado pela demo e por gatilhos locais. */
  push: (specs: readonly FxSpec[]) => void
  clear: () => void
}

export function useFxEngine(ctx: FxTranslateContext): FxEngine {
  const [effects, setEffects] = useState<FxEffect[]>([])
  const nextId = useRef(0)
  const ctxRef = useRef(ctx)
  ctxRef.current = ctx

  const push = useCallback((specs: readonly FxSpec[]) => {
    if (specs.length === 0) return
    setEffects((prev) => {
      const now = performance.now()
      const alive = prev.filter((e) => e.persistent || now - e.startedAt < e.durationMs)

      // Rajada: quanto mais coisa na tela, mais curto cada efeito novo fica.
      const load = alive.length + specs.length
      const scale = clamp(SOFT_CAP / Math.max(load, 1), MIN_SCALE, 1)

      let merged = alive
      const incoming: FxEffect[] = []

      for (const spec of specs) {
        nextId.current += 1
        const effect: FxEffect = {
          ...spec,
          id: `fx-${nextId.current}`,
          startedAt: now,
          durationMs: spec.persistent ? spec.durationMs : Math.round(spec.durationMs * scale),
        }
        if (SINGLETON_KINDS.has(effect.kind)) {
          const rival = merged.find((e) => e.kind === effect.kind)
          if (rival && intensityOf(rival) > intensityOf(effect)) continue
          merged = merged.filter((e) => e.kind !== effect.kind)
        }
        incoming.push(effect)
      }

      const all = [...merged, ...incoming]
      if (all.length <= HARD_CAP) return all
      // Excedente sai por prioridade, mantendo a ordem de chegada dos que ficam.
      const keep = new Set([...all].sort(byPriority).slice(0, HARD_CAP))
      return all.filter((e) => keep.has(e))
    })
  }, [])

  const ingest = useCallback(
    (events: readonly MatchEvent[]) => {
      if (events.length === 0) return
      const specs = events.flatMap((event) => translateEvent(event, ctxRef.current))
      push(specs)
    },
    [push],
  )

  const clear = useCallback(() => {
    setEffects([])
  }, [])

  const ephemeralCount = useMemo(() => effects.filter((e) => !e.persistent).length, [effects])

  useEffect(() => {
    if (ephemeralCount === 0) return
    let frame = 0
    const tick = () => {
      const now = performance.now()
      setEffects((prev) => {
        const next = prev.filter((e) => e.persistent || now - e.startedAt < e.durationMs)
        return next.length === prev.length ? prev : next
      })
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [ephemeralCount])

  return { effects, ingest, push, clear }
}
