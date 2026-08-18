import type { CSSProperties } from 'react'
import type { ManaColor } from '../card/cardVisuals'

/*
 * GEOMETRIA IMPRESSA DO PLAYMAT — fonte única de verdade.
 *
 * O tapete não é mais um fundo: é um objeto físico com as zonas SERIGRAFADAS
 * nele, como o mat de torneio. Isso só funciona se a linha impressa e a carta
 * de verdade caírem no mesmo lugar. Duas medidas separadas — uma no desenho,
 * outra no posicionador — divergem no primeiro ajuste e produzem o pior defeito
 * possível aqui: contorno vazio ao lado da carta que ele deveria conter.
 *
 * Por isso a geometria mora AQUI, em coordenadas normalizadas 0..1 da caixa do
 * mat, e tanto `Playmat.tsx` (que imprime) quanto quem posiciona carta leem a
 * mesma tabela. `rectStyle()` já devolve a caixa em porcentagem, então o
 * posicionador não precisa nem saber que a unidade é 0..1.
 *
 * ESPELHAMENTO. Numa mesa real o mat do oponente está girado 180°. Aqui a
 * rotação é aplicada à GEOMETRIA (cada retângulo vira `1-x-w`, `1-y-h`) e nunca
 * ao elemento: nada neste projeto recebe `rotate(180deg)`. O motivo é que quem
 * assiste continua lendo a tela na orientação normal — mat girado por CSS
 * levaria o rótulo "CAMPO DE BATALHA" de cabeça para baixo, e texto ilegível é
 * ruído, não realismo. Então: caixa espelhada, tipografia em pé.
 *
 * A única consequência visível do espelhamento no rótulo é a ARESTA em que ele
 * é impresso: no assento 0 a serigrafia fica na borda de baixo de cada zona; ao
 * girar a caixa, essa mesma borda passa a ser a de cima. `labelAnchor` carrega
 * essa troca.
 */

export type Seat = 0 | 1

export type PlaymatZoneId =
  | 'battlefield'
  | 'lands'
  | 'library'
  | 'exile'
  | 'life'
  | 'graveyard'

/** Caixa em fração da largura/altura do mat. Origem no canto superior esquerdo. */
export interface NormRect {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

export interface PlaymatZone {
  readonly id: PlaymatZoneId
  /** Rótulo serigrafado, em versalete. */
  readonly label: string
  readonly rect: NormRect
  /** Aresta interna da caixa em que o rótulo é impresso. */
  readonly labelAnchor: 'top' | 'bottom'
}

export interface ManaPip {
  readonly color: ManaColor
  readonly cx: number
  readonly cy: number
  /** Diâmetro como fração da ALTURA do mat — o círculo não pode achatar. */
  readonly diameter: number
}

/** Lado do viewBox do SVG de impressão: 1 unidade normalizada = 1000 unidades. */
export const PLAYMAT_VIEWBOX = 1000

/* Margens da impressão. Frações diferentes em X e Y porque a metade da mesa é
   muito mais larga que alta: a mesma fração nos dois eixos daria uma margem
   superior grossa e uma lateral invisível. */
const PAD_X = 0.014
const PAD_Y = 0.034
const GUTTER_X = 0.012
const GUTTER_Y = 0.026

/** Onde a coluna lateral (deck, exílio, vida, cemitério) começa. */
const SIDEBAR_X = 0.735
/** Divisa entre a faixa de batalha e a de terrenos.
 *  A batalha ficou com a fatia maior porque é lá que o nome da carta precisa
 *  caber inteiro: a carta é 5/7, então largura de leitura só vem de altura de
 *  faixa. O terreno aguenta ser menor — ele é identificado pela arte. */
const BAND_SPLIT = 0.7

const MAIN_W = SIDEBAR_X - GUTTER_X - PAD_X
const SIDE_COL_W = (1 - PAD_X - SIDEBAR_X - GUTTER_X) / 2
const SIDE_OUTER_X = SIDEBAR_X + SIDE_COL_W + GUTTER_X

const BAND_TOP_Y = PAD_Y
const BAND_TOP_H = BAND_SPLIT - GUTTER_Y / 2 - PAD_Y
const BAND_BOTTOM_Y = BAND_SPLIT + GUTTER_Y / 2
const BAND_BOTTOM_H = 1 - PAD_Y - BAND_BOTTOM_Y

/**
 * Ordem de pintura, do fundo para a frente. Também é a ordem em que os rótulos
 * entram no DOM, então é a ordem de leitura para quem usa leitor de tela.
 */
export const PLAYMAT_ZONE_ORDER: readonly PlaymatZoneId[] = [
  'battlefield',
  'lands',
  'library',
  'exile',
  'life',
  'graveyard',
]

type ZoneTable = Record<PlaymatZoneId, PlaymatZone>

/*
 * Assento 0 (quem assiste está do lado dele): as duas bandas horizontais à
 * esquerda — batalha em cima, terrenos embaixo — e uma coluna 2x2 à direita
 * alinhada nas MESMAS bandas. O alinhamento é o que faz a impressão ler como
 * uma grade gravada e não como seis caixas soltas.
 */
const SEAT_0: ZoneTable = {
  battlefield: {
    id: 'battlefield',
    label: 'Campo de Batalha',
    rect: { x: PAD_X, y: BAND_TOP_Y, w: MAIN_W, h: BAND_TOP_H },
    labelAnchor: 'bottom',
  },
  lands: {
    id: 'lands',
    label: 'Terrenos',
    rect: { x: PAD_X, y: BAND_BOTTOM_Y, w: MAIN_W, h: BAND_BOTTOM_H },
    labelAnchor: 'bottom',
  },
  library: {
    id: 'library',
    label: 'Deck',
    rect: { x: SIDEBAR_X, y: BAND_TOP_Y, w: SIDE_COL_W, h: BAND_TOP_H },
    labelAnchor: 'bottom',
  },
  exile: {
    id: 'exile',
    label: 'Exílio',
    rect: { x: SIDEBAR_X, y: BAND_BOTTOM_Y, w: SIDE_COL_W, h: BAND_BOTTOM_H },
    labelAnchor: 'bottom',
  },
  life: {
    id: 'life',
    label: 'Vida',
    rect: { x: SIDE_OUTER_X, y: BAND_TOP_Y, w: SIDE_COL_W, h: BAND_TOP_H },
    labelAnchor: 'bottom',
  },
  graveyard: {
    id: 'graveyard',
    label: 'Cemitério',
    rect: { x: SIDE_OUTER_X, y: BAND_BOTTOM_Y, w: SIDE_COL_W, h: BAND_BOTTOM_H },
    labelAnchor: 'bottom',
  },
}

/**
 * Espelhamento do mat de cima: só no eixo Y.
 *
 * A rotação de 180° de uma mesa real também inverteria X, e foi o que se tentou
 * primeiro. Mas inverter X move o CAMPO DE BATALHA do oponente para o lado
 * oposto da tela, e aí atacante e bloqueador passam a viver em faixas
 * horizontais diferentes — medido: trilho de cima em x=994, o de baixo em
 * x=590, a costura em x=792, três colunas distintas. Combate que se lê por
 * POSIÇÃO deixa de existir, e a única forma de dizer quem bate em quem volta a
 * ser a seta atravessando a tela, que é exatamente o que foi eliminado.
 *
 * Invertendo só Y, as duas metades mantêm o mesmo intervalo em X — os dois
 * campos de batalha ficam na mesma coluna e encostam na costura —, e o mat
 * continuar lendo como espelhado, porque a banda de batalha de cada jogador
 * segue sendo a mais próxima do centro da mesa.
 */
export function mirrorRect(rect: NormRect): NormRect {
  return { x: rect.x, y: 1 - rect.y - rect.h, w: rect.w, h: rect.h }
}

function mirrorZone(zone: PlaymatZone): PlaymatZone {
  return {
    id: zone.id,
    label: zone.label,
    rect: mirrorRect(zone.rect),
    labelAnchor: zone.labelAnchor === 'bottom' ? 'top' : 'bottom',
  }
}

const SEAT_1: ZoneTable = {
  battlefield: mirrorZone(SEAT_0.battlefield),
  lands: mirrorZone(SEAT_0.lands),
  library: mirrorZone(SEAT_0.library),
  exile: mirrorZone(SEAT_0.exile),
  life: mirrorZone(SEAT_0.life),
  graveyard: mirrorZone(SEAT_0.graveyard),
}

const TABLES: readonly ZoneTable[] = [SEAT_0, SEAT_1]

const ZONE_LISTS: readonly (readonly PlaymatZone[])[] = TABLES.map((table) =>
  PLAYMAT_ZONE_ORDER.map((id) => table[id]),
)

/** Todas as zonas do assento, em ordem de pintura. */
export function zonesForSeat(seat: Seat): readonly PlaymatZone[] {
  return ZONE_LISTS[seat]
}

export function playmatZone(seat: Seat, id: PlaymatZoneId): PlaymatZone {
  return TABLES[seat][id]
}

export function playmatRect(seat: Seat, id: PlaymatZoneId): NormRect {
  return TABLES[seat][id].rect
}

export function rectCenter(rect: NormRect): { x: number; y: number } {
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 }
}

const PIP_ORDER: readonly ManaColor[] = ['W', 'U', 'B', 'R', 'G']
/** Diâmetro do círculo vazado, em fração da altura da faixa de terrenos. */
const PIP_SCALE = 0.46

/**
 * Os cinco discos vazados impressos na faixa de terrenos.
 *
 * A ORDEM não espelha junto com a caixa: WUBRG continua lendo da esquerda para
 * a direita nos dois assentos. A fileira de mana é legenda, não zona de jogo —
 * cai na mesma exceção da tipografia. Inverter para BURG… seria fiel ao tapete
 * girado e ilegível para quem assiste.
 */
export function manaPipsForSeat(seat: Seat): readonly ManaPip[] {
  const lands = playmatRect(seat, 'lands')
  const diameter = lands.h * PIP_SCALE
  const cy = lands.y + lands.h / 2
  return PIP_ORDER.map((color, index) => ({
    color,
    cx: lands.x + (lands.w * (index + 0.5)) / PIP_ORDER.length,
    cy,
    diameter,
  }))
}

function pct(value: number): string {
  return `${Math.round(value * 1e5) / 1e3}%`
}

/**
 * A caixa da zona pronta para `position: absolute` dentro do mat. É por aqui
 * que quem posiciona carta consome a geometria — sem reimplementar a conversão.
 */
export function rectStyle(rect: NormRect): CSSProperties {
  return {
    left: pct(rect.x),
    top: pct(rect.y),
    width: pct(rect.w),
    height: pct(rect.h),
  }
}

/* ---------------------------------------------------------------------------
 * INCLINAÇÃO DA MESA
 *
 * O tapete deixou de ser visto de cima e passou a ser visto em ÂNGULO: cada mat
 * gira em `rotateX` dentro de uma `perspective`, e a borda distante fica mais
 * estreita que a próxima. Chapado, o tabuleiro lia como diagrama; inclinado,
 * lê como um objeto apoiado numa mesa.
 *
 * A DOBRADIÇA é a borda da COSTURA — a aresta que os dois mats compartilham no
 * meio da tela. Ali z = 0 nos dois assentos, então os dois tapetes têm
 * exatamente a mesma largura onde se encontram e a silhueta é um trapézio
 * contínuo, sem degrau. É a única escolha de pivô que não produz uma cintura no
 * meio da mesa.
 *
 * CONSEQUÊNCIA QUE PRECISA SER NEUTRALIZADA. Com o pivô na costura, as duas
 * faixas CAMPO DE BATALHA ficam em profundidades OPOSTAS (uma à frente, outra
 * atrás do plano da tela), e a projeção perspectiva multiplica o x de cada uma
 * por um fator diferente. Isso desalinharia as colunas de combate — que é o
 * único jeito de ler quem bloqueia quem nesta mesa.
 *
 * A saída é aritmética, não empírica. A transformação do pai é uma rotação em
 * torno do pivô; se o filho aplicar a rotação INVERSA com o `transform-origin`
 * NO MESMO PIVÔ, as duas se cancelam exatamente — matriz por matriz — e o filho
 * volta a cair na posição, no tamanho e na profundidade que teria sem
 * inclinação nenhuma. `pivotOriginY` devolve esse ponto na caixa da própria
 * zona, e é o que `MatZone` usa nas zonas "presas" (as duas de batalha).
 *
 * As demais zonas ancoram no próprio centro: elas SEGUEM a perspectiva, e é daí
 * que vem a leitura de profundidade — o mat do oponente fica visivelmente menor
 * que o de quem assiste.
 * ------------------------------------------------------------------------ */

/** Aresta do mat que encosta na costura, em coordenada normalizada. */
export function seamEdgeY(seat: Seat): 0 | 1 {
  return seat === 0 ? 0 : 1
}

/**
 * A dobradiça vista de dentro da caixa da zona, em porcentagem da altura dela.
 * Sai fora do intervalo 0–100% de propósito: a costura quase nunca está dentro
 * da zona, e `transform-origin` aceita valor negativo ou acima de 100%.
 */
export function pivotOriginY(seat: Seat, id: PlaymatZoneId): string {
  const rect = playmatRect(seat, id)
  return pct((seamEdgeY(seat) - rect.y) / rect.h)
}
