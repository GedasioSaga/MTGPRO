import type { ReactElement } from 'react'
import { material, stoneVein } from './tokens'
import type { SurfaceSpec } from './tokens'

/*
 * MATERIAL PROCEDURAL — a superficie da mesa como matéria, nao como gradiente.
 *
 * Gradiente pinta uma transicao de cor; material descreve um RELEVO e depois
 * acende uma luz sobre ele. Toda cadeia aqui e a mesma, e essa diferenca e o
 * assunto do arquivo:
 *
 *     feTurbulence  ->  relevo (altura no canal alfa)
 *     feDistantLight -> uma unica luz, sempre vinda do TOPO da tela
 *     feDiffuseLighting / feSpecularLighting -> o relevo virando pixel
 *     feComponentTransfer -> rampa que recentra o resultado em cinza medio
 *
 * A rampa e o passo que quase todo mundo esquece. Iluminacao difusa devolve
 * uma imagem clara (a media fica perto de `sin(elevation)`), e uma imagem clara
 * misturada em `soft-light` lava a mesa inteira. `slope`/`intercept` de
 * `tokens.ts` puxam a media de volta para 0.5, onde `soft-light` e neutro:
 * o material passa a somar so RELEVO, sem mexer na cor de baixo.
 *
 * DOIS CAMINHOS, DE PROPOSITO — e a decisao de desempenho deste arquivo:
 *
 *   1. Forma vetorial (moldura da arena, patamares, cartucho do HUD) usa estes
 *      filtros por `filter="url(#...)"`. Sao quatro caminhos ESTATICOS que
 *      nunca animam, entao o navegador rasteriza uma vez e guarda.
 *   2. Superficie de CSS (tapete, faixa de terrenos, costura) NAO usa
 *      `filter: url()`. Usa os ladrilhos `--mat-*` de `theme.css`, que sao o
 *      mesmo material assado num data URI e repetido como `background-image`.
 *
 * O caminho 2 existe porque `filter` em CSS reprocessa o elemento INTEIRO a
 * cada repintura — e a mesa repinta a cada carta que entra, cada badge que
 * muda. Um ladrilho e uma imagem: custa upload de textura uma vez e zero por
 * frame. Nenhum dos dois anima; animar filtro em laco derruba o frame rate.
 */

/** Ids referenciados por `filter="url(#...)"` em SVG. */
export const MATERIAL_FELT = 'mat-felt'
export const MATERIAL_LEATHER = 'mat-leather'
export const MATERIAL_METAL = 'mat-brushed'
export const MATERIAL_STONE = 'mat-stone'

/**
 * As camadas da arena sao desenhadas com `preserveAspectRatio="none"`: um
 * viewBox quadrado esticado numa faixa larga e baixa. A unidade de usuario
 * vale bem menos em Y do que em X, entao a frequencia vertical crua sairia
 * abaixo de um pixel e viraria moire. Dividir Y devolve a proporcao da trama.
 */
const ARENA_STRETCH = 2

/** `baseFrequency` de token ("x y") corrigida para o espaco esticado da arena. */
function stretched(baseFrequency: string): string {
  const parts = baseFrequency.trim().split(/\s+/).map(Number)
  const x = parts[0] ?? 0
  const y = parts[1] ?? x
  return `${x} ${round(y / ARENA_STRETCH)}`
}

function round(value: number): number {
  return Math.round(value * 10000) / 10000
}

/**
 * Recentra o mapa de luz em cinza medio. Sem isso o material clareia tudo que
 * estiver embaixo dele em vez de dar textura.
 */
function Ramp({ slope, intercept }: { slope: number; intercept: number }): ReactElement | null {
  if (slope === 1 && intercept === 0) return null
  return (
    <feComponentTransfer>
      <feFuncR type="linear" slope={slope} intercept={intercept} />
      <feFuncG type="linear" slope={slope} intercept={intercept} />
      <feFuncB type="linear" slope={slope} intercept={intercept} />
    </feComponentTransfer>
  )
}

interface FilterShellProps {
  id: string
  children: ReactElement | (ReactElement | null)[]
}

/**
 * Regiao do filtro presa a caixa da forma. O padrao do SVG estoura 10% para
 * cada lado, o que numa camada de tela cheia significa rasterizar uma imagem
 * 20% maior do que o necessario, todo repaint.
 */
function FilterShell({ id, children }: FilterShellProps): ReactElement {
  return (
    <filter
      id={id}
      x="0%"
      y="0%"
      width="100%"
      height="100%"
      colorInterpolationFilters="sRGB"
    >
      {children}
    </filter>
  )
}

/** Fibra e escovado: uma turbulencia, uma luz difusa, a rampa. */
function ReliefFilter({ id, spec }: { id: string; spec: SurfaceSpec }): ReactElement {
  return (
    <FilterShell id={id}>
      <feTurbulence
        type={spec.noise}
        baseFrequency={stretched(spec.baseFrequency)}
        numOctaves={spec.octaves}
        seed={spec.seed}
        stitchTiles="stitch"
        result="relief"
      />
      <feDiffuseLighting
        in="relief"
        surfaceScale={spec.surfaceScale}
        diffuseConstant={1}
        lightingColor={spec.lightColor}
      >
        <feDistantLight azimuth={spec.azimuth} elevation={spec.elevation} />
      </feDiffuseLighting>
      <Ramp slope={spec.slope} intercept={spec.intercept} />
    </FilterShell>
  )
}

/**
 * Couro. A turbulencia sozinha da nuvem; o que faz PORO e comprimir o proprio
 * ruido contra ele mesmo (`arithmetic` com k2 alto e k4 negativo): os vales
 * afundam de vez e o que sobra sao celulas com vinco entre elas.
 */
function LeatherFilter({ id, spec }: { id: string; spec: SurfaceSpec }): ReactElement {
  return (
    <FilterShell id={id}>
      <feTurbulence
        type={spec.noise}
        baseFrequency={stretched(spec.baseFrequency)}
        numOctaves={spec.octaves}
        seed={spec.seed}
        stitchTiles="stitch"
        result="cells"
      />
      <feComposite
        in="cells"
        in2="cells"
        operator="arithmetic"
        k1={1.35}
        k2={0.25}
        k3={0}
        k4={-0.12}
        result="grain"
      />
      <feDiffuseLighting
        in="grain"
        surfaceScale={spec.surfaceScale}
        diffuseConstant={1}
        lightingColor={spec.lightColor}
      >
        <feDistantLight azimuth={spec.azimuth} elevation={spec.elevation} />
      </feDiffuseLighting>
      <Ramp slope={spec.slope} intercept={spec.intercept} />
    </FilterShell>
  )
}

/**
 * Pedra. Unico material com duas turbulencias: a de baixa frequencia e o veio,
 * a fina DESLOCA o veio (`feDisplacementMap`). Ruido nao deslocado le como
 * fumaca; deslocado, o veio ganha o serrilhado irregular de rocha partida.
 *
 * E o unico com brilho especular somado por cima do difuso — e o especular que
 * diz "polida" em vez de "porosa".
 */
function StoneFilter({ id, spec }: { id: string; spec: SurfaceSpec }): ReactElement {
  return (
    <FilterShell id={id}>
      <feTurbulence
        type={spec.noise}
        baseFrequency={stretched(spec.baseFrequency)}
        numOctaves={spec.octaves}
        seed={spec.seed}
        stitchTiles="stitch"
        result="vein"
      />
      <feTurbulence
        type="fractalNoise"
        baseFrequency={stretched(stoneVein.warpFrequency)}
        numOctaves={stoneVein.warpOctaves}
        seed={stoneVein.warpSeed}
        stitchTiles="stitch"
        result="warp"
      />
      <feDisplacementMap
        in="vein"
        in2="warp"
        scale={stoneVein.displace}
        xChannelSelector="R"
        yChannelSelector="G"
        result="rock"
      />
      <feDiffuseLighting
        in="rock"
        surfaceScale={spec.surfaceScale}
        diffuseConstant={1}
        lightingColor={spec.lightColor}
        result="body"
      >
        <feDistantLight azimuth={spec.azimuth} elevation={spec.elevation} />
      </feDiffuseLighting>
      <feSpecularLighting
        in="rock"
        surfaceScale={spec.surfaceScale}
        specularConstant={stoneVein.specularConstant}
        specularExponent={stoneVein.specularExponent}
        lightingColor={stoneVein.specularColor}
        result="gloss"
      >
        <feDistantLight azimuth={spec.azimuth} elevation={spec.elevation} />
      </feSpecularLighting>
      <feComposite
        in="gloss"
        in2="body"
        operator="arithmetic"
        k1={0}
        k2={1}
        k3={1}
        k4={0}
      />
      <Ramp slope={spec.slope} intercept={spec.intercept} />
    </FilterShell>
  )
}

/**
 * Biblioteca de materiais do documento. Monte UMA vez — sao ids globais, e dois
 * montes dariam id duplicado.
 *
 * Sai como `<svg>` de tamanho zero para poder ser montado solto em qualquer
 * lugar da arvore; `<svg>` tambem e filho valido de `<defs>`, entao a mesma
 * peca serve quando quem monta ja tem o proprio bloco de definicoes.
 */
export function MaterialDefs(): ReactElement {
  return (
    <svg
      className="material-defs"
      width="0"
      height="0"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <ReliefFilter id={MATERIAL_FELT} spec={material.felt} />
        <ReliefFilter id={MATERIAL_METAL} spec={material.brushed} />
        <LeatherFilter id={MATERIAL_LEATHER} spec={material.leather} />
        <StoneFilter id={MATERIAL_STONE} spec={material.stone} />
      </defs>
    </svg>
  )
}
