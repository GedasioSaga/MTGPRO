/**
 * Fonte unica de verdade do sistema visual.
 *
 * Todo valor aqui tem um espelho em `index.css` (bloco `@theme` do Tailwind v4).
 * Quem escreve JSX usa as utilities do Tailwind; quem precisa do valor cru em
 * TypeScript (canvas, motion, gradiente procedural) importa daqui. Se mudar um
 * lado, mude o outro — sao o mesmo sistema escrito em duas linguagens.
 */

// ---------------------------------------------------------------------------
// Superficies e texto
// ---------------------------------------------------------------------------

/**
 * A mesa e vista de cima sob luz de torneio noturno: feltro frio e escuro,
 * chrome de HUD em grafite metalico, luz quente vindo de cima. O contraste
 * frio/quente e o que da profundidade — sem ele isso vira dashboard cinza.
 */
export const surface = {
  /** Fundo absoluto, fora da mesa. */
  void: '#07090f',
  /** Painel encaixado (poco, zona vazia). */
  sunken: '#0a0d15',
  /** Painel base do HUD. */
  base: '#111621',
  /** Painel elevado (barra lateral, dock). */
  raised: '#19202e',
  /** Chrome metalico, botao em repouso. */
  metal: '#232c3e',
  /** Topo do metal escovado / hover. */
  metalHigh: '#303a50',
  /** Popover, tooltip, modal. */
  overlay: '#1b2231',
} as const

/** Superficie da mesa. Feltro verde-petroleo, tres profundidades de fibra. */
export const felt = {
  deep: '#0a1614',
  base: '#122220',
  light: '#1b332c',
  /** Costura / borda do tapete. */
  seam: '#2b4a3f',
} as const

export const text = {
  /** Titulo, numero grande, nome de carta. */
  strong: '#f3f5f9',
  /** Corpo, texto de regra. */
  body: '#bcc4d4',
  /** Legenda, rotulo, metadado. */
  muted: '#7e8aa2',
  /** Placeholder, texto desabilitado. */
  faint: '#4f5a73',
  /** Texto sobre superficie clara (badge de mana branco, por exemplo). */
  inverse: '#0a0d15',
} as const

export const line = {
  /** Hairline padrao entre secoes. */
  hairline: 'rgba(255, 255, 255, 0.07)',
  /** Borda de painel. */
  soft: 'rgba(255, 255, 255, 0.11)',
  /** Borda enfatizada. */
  strong: 'rgba(255, 255, 255, 0.18)',
  /** Highlight de 1px no topo do metal escovado. */
  gloss: 'rgba(255, 255, 255, 0.24)',
} as const

// ---------------------------------------------------------------------------
// Mana
// ---------------------------------------------------------------------------

export type ManaKey = 'W' | 'U' | 'B' | 'R' | 'G' | 'C' | 'M'
export type ColorKey = 'W' | 'U' | 'B' | 'R' | 'G'

export interface ManaSwatch {
  /** Cor solida do simbolo. */
  core: string
  /** Versao luminosa, para glow e estado ativo. */
  bright: string
  /** Versao funda, para gradiente e sombra colorida. */
  deep: string
  /** Fundo translucido, para tint de painel e moldura de carta. */
  bg: string
  /** Cor de texto legivel sobre `core`. */
  on: string
  label: string
}

/**
 * Ordem WUBRG. `C` e incolor e `M` e ouro (multicolorido) — nenhum dos dois
 * existe no bitmask do motor, os dois sao derivados por `dominantMana`.
 */
export const mana: Record<ManaKey, ManaSwatch> = {
  W: {
    core: '#f4ecd2',
    bright: '#fffaea',
    deep: '#b8ab88',
    bg: 'rgba(244, 236, 210, 0.13)',
    on: '#241f14',
    label: 'White',
  },
  U: {
    core: '#4a9eff',
    bright: '#94ccff',
    deep: '#1d4f8f',
    bg: 'rgba(74, 158, 255, 0.15)',
    on: '#04131f',
    label: 'Blue',
  },
  B: {
    core: '#a08fbe',
    bright: '#ccbde3',
    deep: '#4a3d64',
    bg: 'rgba(160, 143, 190, 0.15)',
    on: '#120d1c',
    label: 'Black',
  },
  R: {
    core: '#e8543c',
    bright: '#ff9075',
    deep: '#8c2416',
    bg: 'rgba(232, 84, 60, 0.15)',
    on: '#1e0805',
    label: 'Red',
  },
  G: {
    core: '#42ab6c',
    bright: '#84dda4',
    deep: '#1d5735',
    bg: 'rgba(66, 171, 108, 0.15)',
    on: '#05170d',
    label: 'Green',
  },
  C: {
    core: '#a8a49c',
    bright: '#d6d2c9',
    deep: '#4e4b46',
    bg: 'rgba(168, 164, 156, 0.13)',
    on: '#16150f',
    label: 'Colorless',
  },
  M: {
    core: '#d8ad46',
    bright: '#f6d986',
    deep: '#8a6716',
    bg: 'rgba(216, 173, 70, 0.15)',
    on: '#1c1305',
    label: 'Multicolor',
  },
}

/** Bits de `ColorSet` no motor (`mana.rs`): W=1, U=2, B=4, R=8, G=16. */
const COLOR_BITS: readonly (readonly [ColorKey, number])[] = [
  ['W', 1],
  ['U', 2],
  ['B', 4],
  ['R', 8],
  ['G', 16],
]

/** Decompoe o bitmask `CardView.colors` em letras WUBRG, na ordem canonica. */
export function colorSetToKeys(colors: number): ColorKey[] {
  return COLOR_BITS.filter((entry) => (colors & entry[1]) !== 0).map((entry) => entry[0])
}

/** Cor que manda no visual da carta: incolor, a unica cor, ou ouro se multi. */
export function dominantMana(colors: number): ManaKey {
  const keys = colorSetToKeys(colors)
  if (keys.length === 0) return 'C'
  if (keys.length === 1) return keys[0]
  return 'M'
}

// ---------------------------------------------------------------------------
// Acento por assento
// ---------------------------------------------------------------------------

export interface SeatSwatch {
  core: string
  bright: string
  deep: string
  bg: string
}

/**
 * O acento muda conforme o jogador ativo: ambar quente no assento de baixo,
 * indigo frio no de cima. De quem e o turno vira percepcao de cor, e nao
 * leitura de texto.
 */
export const seatAccent: readonly [SeatSwatch, SeatSwatch] = [
  { core: '#f0b23f', bright: '#ffd685', deep: '#8a5f10', bg: 'rgba(240, 178, 63, 0.14)' },
  { core: '#7c6ef2', bright: '#b3a9ff', deep: '#3c318f', bg: 'rgba(124, 110, 242, 0.16)' },
]

export const status = {
  success: '#3ecf8e',
  successBg: 'rgba(62, 207, 142, 0.14)',
  danger: '#f0524d',
  dangerBg: 'rgba(240, 82, 77, 0.15)',
  warning: '#f0b23f',
  warningBg: 'rgba(240, 178, 63, 0.14)',
  info: '#4a9eff',
  infoBg: 'rgba(74, 158, 255, 0.14)',
  /** Vida perdida / dano. */
  life: '#e0574f',
  poison: '#8bc34a',
} as const

// ---------------------------------------------------------------------------
// Espaco, forma, elevacao
// ---------------------------------------------------------------------------

/** Escala de 4px com dois passos finos embaixo, para chrome apertado. */
export const space = {
  px: '1px',
  hair: '2px',
  xs: '4px',
  sm: '6px',
  md: '8px',
  lg: '12px',
  xl: '16px',
  '2xl': '20px',
  '3xl': '24px',
  '4xl': '32px',
  '5xl': '40px',
  '6xl': '48px',
  '7xl': '64px',
  '8xl': '80px',
} as const

export const radius = {
  none: '0px',
  xs: '3px',
  sm: '5px',
  md: '8px',
  lg: '12px',
  xl: '18px',
  '2xl': '26px',
  /** Canto de carta de Magic: ~4.7% da largura. */
  card: '4.7%',
  full: '9999px',
} as const

/**
 * Quatro degraus de elevacao. Cada um combina sombra de contato (curta e opaca)
 * com sombra ambiente (longa e difusa) — so uma das duas faz o objeto flutuar
 * sem peso.
 */
export const shadow = {
  e1: '0 1px 2px rgba(0, 0, 0, 0.55), inset 0 0 0 1px rgba(255, 255, 255, 0.04)',
  e2: '0 2px 4px -1px rgba(0, 0, 0, 0.5), 0 8px 18px -8px rgba(0, 0, 0, 0.55)',
  e3: '0 4px 8px -2px rgba(0, 0, 0, 0.55), 0 18px 38px -12px rgba(0, 0, 0, 0.7)',
  e4: '0 8px 16px -4px rgba(0, 0, 0, 0.6), 0 40px 80px -20px rgba(0, 0, 0, 0.85)',
} as const

export const zIndex = {
  table: 0,
  card: 10,
  lifted: 40,
  stack: 60,
  hud: 80,
  banner: 100,
  overlay: 200,
  tooltip: 300,
} as const

// ---------------------------------------------------------------------------
// Tipografia
// ---------------------------------------------------------------------------

export const font = {
  /** Serifa capitular: nome de carta, numero grande, banner de fase. */
  display: "'Cinzel', 'Palatino Linotype', Georgia, serif",
  /** Sans neutra: texto de regra, log, chrome. */
  sans: "'Inter', system-ui, 'Segoe UI', Roboto, sans-serif",
  mono: "ui-monospace, 'Cascadia Mono', Consolas, monospace",
} as const

// ---------------------------------------------------------------------------
// Movimento
// ---------------------------------------------------------------------------

/**
 * Duracoes de micro-interacao, em ms. As duracoes de partida chegam do motor em
 * `MatchEvent.suggestedDurationMs` — nao duplique aquilo aqui; `fallbackEventMs`
 * so cobre evento que chegue sem duracao.
 */
export const duration = {
  /** Feedback imediato: cor de hover, foco. */
  micro: 110,
  /** Troca de estado pequena: badge, contador, tooltip. */
  fast: 170,
  /** Padrao: painel, realce, movimento curto. */
  base: 260,
  /** Entrada de elemento maior: carta na mao, item de pilha. */
  slow: 400,
  /** Transicao de zona, virada de carta. */
  slower: 560,
  /** Teto de micro-interacao. Acima disso e cinematica, e vem do motor. */
  dramatic: 700,
  fallbackEventMs: 260,
} as const

/**
 * Nenhuma curva linear: movimento linear parece maquina, e nao mesa. `standard`
 * e a curva de saida usada em quase tudo; `snap` tem overshoot leve e existe
 * para o que "assenta" (contador, tap, marcador).
 */
export const easing = {
  standard: 'cubic-bezier(0.32, 0.72, 0, 1)',
  out: 'cubic-bezier(0.16, 1, 0.3, 1)',
  in: 'cubic-bezier(0.7, 0, 0.84, 0)',
  inOut: 'cubic-bezier(0.65, 0, 0.35, 1)',
  snap: 'cubic-bezier(0.34, 1.4, 0.64, 1)',
  glide: 'cubic-bezier(0.22, 1, 0.36, 1)',
} as const

/** As mesmas curvas em array, para o prop `ease` do motion. */
export const easingArray = {
  standard: [0.32, 0.72, 0, 1],
  out: [0.16, 1, 0.3, 1],
  in: [0.7, 0, 0.84, 0],
  inOut: [0.65, 0, 0.35, 1],
  snap: [0.34, 1.4, 0.64, 1],
  glide: [0.22, 1, 0.36, 1],
} as const satisfies Record<string, readonly [number, number, number, number]>

/** Springs do motion, para o que responde a impacto. */
export const spring = {
  /** Chrome de UI: rapido, sem balanco. */
  snappy: { type: 'spring', stiffness: 520, damping: 38, mass: 0.8 },
  /** Carta se movendo entre zonas. */
  gentle: { type: 'spring', stiffness: 240, damping: 30, mass: 1 },
  /** Impacto: dano, criatura entrando no campo. */
  impact: { type: 'spring', stiffness: 700, damping: 22, mass: 0.9 },
} as const

/** Deslocamento de entrada, em px. Percurso curto le como intencional. */
export const travel = {
  hairline: 2,
  short: 6,
  medium: 14,
  long: 28,
} as const

export type SeatIndex = 0 | 1
export type Elevation = keyof typeof shadow
export type DurationKey = keyof typeof duration
export type EasingKey = keyof typeof easing
