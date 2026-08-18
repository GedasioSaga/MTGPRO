import type { CSSProperties, ReactElement } from 'react'
import type { CardView } from '../../types/protocol'
import { Card } from './Card'
import { ManaSymbol } from './ManaCost'
import { parseManaSymbol } from './cardVisuals'
import { CardBack } from './CardBack'
import type { CardSize } from './cardVisuals'
import { font, surface, text } from '../../design/tokens'
import './card.css'

/** Bits WUBRG do motor, repetidos aqui só para as cartas sintéticas ficarem legíveis. */
const W = 1
const U = 2
const B = 4
const R = 8
const G = 16

function makeCard(overrides: Partial<CardView> & { id: number; name: string }): CardView {
  return {
    manaCost: null,
    manaValue: 0,
    typeLine: null,
    oracleText: null,
    flavorText: null,
    colors: 0,
    power: null,
    toughness: null,
    basePower: null,
    baseToughness: null,
    loyalty: null,
    damage: 0,
    tapped: false,
    faceDown: false,
    summoningSick: false,
    attacking: null,
    blocking: [],
    blockedBy: [],
    counters: [],
    keywords: [],
    attachedTo: null,
    attachments: [],
    isToken: false,
    controller: 0,
    owner: 0,
    zone: 'Battlefield',
    artKey: null,
    rarity: 'Common',
    setCode: 'GAL',
    isLegalTarget: false,
    isActionable: false,
    ...overrides,
  }
}

const SERRA_ANGEL = makeCard({
  id: 1,
  name: 'Serra Angel',
  manaCost: '{3}{W}{W}',
  manaValue: 5,
  typeLine: 'Creature — Angel',
  oracleText: 'Flying, vigilance',
  flavorText: 'Born of a wish for peace and a desire for justice.',
  colors: W,
  power: 4,
  toughness: 4,
  basePower: 4,
  baseToughness: 4,
  keywords: ['Flying', 'Vigilance'],
  rarity: 'Uncommon',
  setCode: 'DOM',
})

interface Tile {
  label: string
  note: string
  card: CardView
}

const TILES: Tile[] = [
  {
    label: 'Mono vermelha',
    note: 'moldura de uma cor',
    card: makeCard({
      id: 2,
      name: 'Lightning Bolt',
      manaCost: '{R}',
      manaValue: 1,
      typeLine: 'Instant',
      oracleText: 'Lightning Bolt deals 3 damage to any target.',
      flavorText: 'The sparkmage shrieked, calling on the rage of the storm.',
      colors: R,
      zone: 'Stack',
      rarity: 'Uncommon',
      setCode: 'M11',
    }),
  },
  {
    label: 'Virada',
    note: 'rotação de 90° com a sombra reposicionada',
    card: { ...SERRA_ANGEL, id: 3, tapped: true },
  },
  {
    label: 'Dano marcado',
    note: 'resistência efetiva em vermelho',
    card: makeCard({
      id: 4,
      name: 'Grizzly Bears',
      manaCost: '{1}{G}',
      manaValue: 2,
      typeLine: 'Creature — Bear',
      oracleText: '',
      flavorText: 'Don’t try to outrun one of Dominaria’s grizzlies.',
      colors: G,
      power: 2,
      toughness: 2,
      basePower: 2,
      baseToughness: 2,
      damage: 1,
    }),
  },
  {
    label: 'Enjoo de invocação',
    note: 'dessaturação leve e ampulheta',
    card: makeCard({
      id: 5,
      name: 'Llanowar Elves',
      manaCost: '{G}',
      manaValue: 1,
      typeLine: 'Creature — Elf Druid',
      oracleText: '{T}: Add {G}.',
      colors: G,
      power: 1,
      toughness: 1,
      basePower: 1,
      baseToughness: 1,
      summoningSick: true,
    }),
  },
  {
    label: 'Buffada',
    note: 'valor atual em destaque, impresso riscado ao lado',
    card: makeCard({
      id: 6,
      name: 'Vampire Nighthawk',
      manaCost: '{1}{B}{B}',
      manaValue: 3,
      typeLine: 'Creature — Vampire Shaman',
      oracleText: 'Flying, deathtouch, lifelink',
      colors: B,
      power: 4,
      toughness: 5,
      basePower: 2,
      baseToughness: 3,
      counters: [['PlusOnePlusOne', 2]],
      keywords: ['Flying', 'Deathtouch', 'Lifelink'],
      rarity: 'Uncommon',
    }),
  },
  {
    label: 'Enfraquecida',
    note: 'marcadores -1/-1 empilhados com contagem',
    card: makeCard({
      id: 7,
      name: 'Kalonian Tusker',
      manaCost: '{G}{G}',
      manaValue: 2,
      typeLine: 'Creature — Beast',
      oracleText: '',
      colors: G,
      power: 1,
      toughness: 1,
      basePower: 3,
      baseToughness: 3,
      flavorText: 'It tramples the underbrush flat and calls the clearing home.',
      counters: [['MinusOneMinusOne', 2]],
      rarity: 'Uncommon',
    }),
  },
  {
    label: 'Atacando',
    note: 'moldura bicolor em gradiente e contorno luminoso',
    card: makeCard({
      id: 8,
      name: 'Dragonlord Ojutai',
      manaCost: '{3}{W}{U}',
      manaValue: 5,
      typeLine: 'Legendary Creature — Dragon',
      oracleText:
        'Flying\nOjutai has hexproof as long as it’s untapped.\nWhenever Ojutai deals combat damage to a player, look at the top three cards of your library.',
      colors: W | U,
      power: 5,
      toughness: 4,
      basePower: 5,
      baseToughness: 4,
      attacking: { Player: 1 },
      keywords: ['Flying', 'Hexproof'],
      rarity: 'Mythic',
      setCode: 'DTK',
    }),
  },
  {
    label: 'Bloqueando',
    note: 'contorno na cor do controlador',
    card: makeCard({
      id: 9,
      name: 'Wall of Omens',
      manaCost: '{1}{W}',
      manaValue: 2,
      typeLine: 'Creature — Wall',
      oracleText: 'Defender\nWhen Wall of Omens enters the battlefield, draw a card.',
      colors: W,
      power: 0,
      toughness: 4,
      basePower: 0,
      baseToughness: 4,
      blocking: [8],
      keywords: ['Defender'],
      controller: 1,
      owner: 1,
      rarity: 'Uncommon',
    }),
  },
  {
    label: 'Alvo legal',
    note: 'halo pulsante',
    card: makeCard({
      id: 10,
      name: 'Snapcaster Mage',
      manaCost: '{1}{U}',
      manaValue: 2,
      typeLine: 'Creature — Human Wizard',
      oracleText:
        'Flash\nWhen Snapcaster Mage enters the battlefield, target instant or sorcery card in your graveyard gains flashback until end of turn.',
      colors: U,
      power: 2,
      toughness: 1,
      basePower: 2,
      baseToughness: 1,
      keywords: ['Flash'],
      isLegalTarget: true,
      rarity: 'Rare',
      setCode: 'ISD',
    }),
  },
  {
    label: 'Ficha',
    note: 'hachura, canto dobrado e selo de ficha',
    card: makeCard({
      id: 11,
      name: 'Soldier',
      typeLine: 'Token Creature — Soldier',
      oracleText: '',
      colors: W,
      power: 1,
      toughness: 1,
      basePower: 1,
      baseToughness: 1,
      isToken: true,
      rarity: 'Special',
      setCode: 'TKN',
    }),
  },
  {
    label: 'Artefato',
    note: 'moldura incolor metálica',
    card: makeCard({
      id: 12,
      name: 'Sol Ring',
      manaCost: '{1}',
      manaValue: 1,
      typeLine: 'Artifact',
      oracleText: '{T}: Add {C}{C}.',
      flavorText: 'A ring of pure aether, worn by no hand.',
      rarity: 'Uncommon',
      setCode: 'CMR',
    }),
  },
  {
    label: 'Terreno',
    note: 'moldura de terreno, sem custo',
    card: makeCard({
      id: 13,
      name: 'Sacred Foundry',
      typeLine: 'Land — Mountain Plains',
      oracleText:
        '({T}: Add {R} or {W}.)\nAs Sacred Foundry enters the battlefield, you may pay 2 life. If you don’t, it enters tapped.',
      rarity: 'Rare',
      setCode: 'RNA',
    }),
  },
  {
    label: 'Ouro',
    note: 'três ou mais cores',
    card: makeCard({
      id: 14,
      name: 'Cruel Ultimatum',
      manaCost: '{U}{B}{B}{B}{R}',
      manaValue: 5,
      typeLine: 'Sorcery',
      oracleText:
        'Target opponent sacrifices a creature, discards three cards, then loses 5 life. You return a creature card from your graveyard to your hand, draw three cards, then gain 5 life.',
      colors: U | B | R,
      zone: 'Stack',
      rarity: 'Rare',
      setCode: 'ARB',
    }),
  },
  {
    label: 'Mono azul',
    note: 'moldura azul, disco de mana pleno',
    card: makeCard({
      id: 16,
      name: 'Counterspell',
      manaCost: '{U}{U}',
      manaValue: 2,
      typeLine: 'Instant',
      oracleText: 'Counter target spell.',
      colors: U,
      zone: 'Stack',
      rarity: 'Common',
      setCode: 'MH2',
    }),
  },
  {
    label: 'Mono preta',
    note: 'moldura preta e caveira no disco',
    card: makeCard({
      id: 17,
      name: 'Dark Ritual',
      manaCost: '{B}',
      manaValue: 1,
      typeLine: 'Instant',
      oracleText: 'Add {B}{B}{B}.',
      colors: B,
      zone: 'Stack',
      rarity: 'Common',
      setCode: 'ICE',
    }),
  },
  {
    label: 'Mono branca',
    note: 'moldura branca e sol de raios curvos',
    card: makeCard({
      id: 18,
      name: 'Wrath of God',
      manaCost: '{2}{W}{W}',
      manaValue: 4,
      typeLine: 'Sorcery',
      oracleText: 'Destroy all creatures. They cannot be regenerated.',
      colors: W,
      zone: 'Stack',
      rarity: 'Rare',
      setCode: 'DOM',
    }),
  },
  {
    label: 'Quatro palavras-chave',
    note: 'coluna de badges com transbordo +N',
    card: makeCard({
      id: 19,
      name: 'Akroma, Angel of Wrath',
      manaCost: '{5}{W}{W}{W}',
      manaValue: 8,
      typeLine: 'Legendary Creature — Angel',
      oracleText: 'Flying, first strike, vigilance, trample, haste, protection from black and from red',
      colors: W,
      power: 6,
      toughness: 6,
      basePower: 6,
      baseToughness: 6,
      keywords: ['Flying', 'FirstStrike', 'Vigilance', 'Trample', 'Haste'],
      rarity: 'Mythic',
      setCode: 'LGN',
    }),
  },
  {
    label: 'Ameaçar e defensor',
    note: 'glifos de ameaçar, defensor e vínculo com a vida',
    card: makeCard({
      id: 20,
      name: 'Guardião do Portão',
      manaCost: '{2}{B}{G}',
      manaValue: 4,
      typeLine: 'Creature — Golem',
      oracleText: 'Menace, defender, lifelink, deathtouch',
      colors: B | G,
      power: 2,
      toughness: 6,
      basePower: 2,
      baseToughness: 6,
      keywords: ['Menace', 'Defender', 'Lifelink', 'Deathtouch'],
      rarity: 'Rare',
      setCode: 'GRN',
    }),
  },
  {
    label: 'Virada com badges',
    note: 'glifos e P/T seguem horizontais na rotação de 90°',
    card: makeCard({
      id: 21,
      name: 'Colossal Dreadmaw',
      manaCost: '{4}{G}{G}',
      manaValue: 6,
      typeLine: 'Creature — Dinosaur',
      oracleText: 'Trample',
      colors: G,
      power: 6,
      toughness: 6,
      basePower: 6,
      baseToughness: 6,
      keywords: ['Trample'],
      tapped: true,
      rarity: 'Common',
      setCode: 'XLN',
    }),
  },
  {
    label: 'Só o poder alterado',
    note: 'o 5 fica azul, o 4 continua neutro',
    card: makeCard({
      id: 22,
      name: 'Fathom Fleet Captain',
      manaCost: '{1}{U}{B}',
      manaValue: 3,
      typeLine: 'Creature — Orc Pirate',
      oracleText: 'Menace',
      colors: U | B,
      power: 5,
      toughness: 4,
      basePower: 3,
      baseToughness: 4,
      keywords: ['Menace'],
      counters: [['PlusOnePlusOne', 2]],
      rarity: 'Rare',
      setCode: 'XLN',
    }),
  },
  {
    label: 'Híbrido e phyrexiano',
    note: 'disco dividido na diagonal com as duas cores',
    card: makeCard({
      id: 23,
      name: 'Ruína do Bosque',
      manaCost: '{2}{G/W}{R/P}{X}',
      manaValue: 4,
      typeLine: 'Sorcery',
      oracleText: 'Choose one — {T}: Add {C}. • Deal {X} damage. ({S} counts as snow.)',
      colors: G | W | R,
      zone: 'Stack',
      rarity: 'Rare',
      setCode: 'MH1',
    }),
  },
  {
    label: 'Planeswalker',
    note: 'escudo de lealdade no lugar do P/T',
    card: makeCard({
      id: 15,
      name: "Elspeth, Sun's Champion",
      manaCost: '{4}{W}{W}',
      manaValue: 6,
      typeLine: 'Legendary Planeswalker — Elspeth',
      oracleText:
        '+1: Create three 1/1 white Soldier creature tokens.\n-3: Destroy all creatures with power 4 or greater.\n-7: You get an emblem.',
      colors: W,
      loyalty: 4,
      rarity: 'Mythic',
      setCode: 'THS',
    }),
  },
]

const LADDER: CardSize[] = ['micro', 'small', 'medium', 'large']

const SYMBOLS = ['W', 'U', 'B', 'R', 'G', '3', 'X', 'C', 'S', 'T', 'W/U', 'B/R', '2/G', 'G/P']

/**
 * Bancada visual da carta, sem rede e sem store: um lugar onde todo estado que
 * a mesa consegue produzir aparece lado a lado, para comparação e captura.
 */
export function CardGallery(): ReactElement {
  return (
    <div
      className="min-h-screen w-full px-8 py-10"
      style={{
        background: `radial-gradient(120% 90% at 50% -10%, ${surface.raised} 0%, ${surface.void} 62%)`,
        fontFamily: font.sans,
        color: text.body,
      }}
    >
      <header className="mx-auto mb-10 max-w-[1440px]">
        <h1
          className="m-0 text-[26px] leading-none font-semibold tracking-[-0.01em]"
          style={{ fontFamily: font.display, color: text.strong }}
        >
          Bancada da carta
        </h1>
        <p className="mt-2 mb-0 text-[13px]" style={{ color: text.muted }}>
          Passe o mouse em qualquer carta para o painel de zoom. Nenhuma carta aqui vem do
          servidor.
        </p>
      </header>

      <section className="mx-auto mb-12 max-w-[1440px]">
        <SectionTitle>Escala</SectionTitle>
        <div className="flex flex-wrap items-end gap-8">
          {LADDER.map((size) => (
            <figure key={size} className="m-0 flex flex-col items-start gap-2">
              <Card card={SERRA_ANGEL} size={size} />
              <figcaption className="text-[11px] tracking-[0.14em] uppercase" style={{ color: text.faint }}>
                {size}
              </figcaption>
            </figure>
          ))}
          <figure className="m-0 flex flex-col items-start gap-2">
            <CardBack size="medium" seed="gallery" />
            <figcaption className="text-[11px] tracking-[0.14em] uppercase" style={{ color: text.faint }}>
              verso
            </figcaption>
          </figure>
        </div>
      </section>

      <section className="mx-auto mb-12 max-w-[1440px]">
        <SectionTitle>Símbolos de mana</SectionTitle>
        <div className="flex flex-wrap items-center gap-5" style={{ '--sym': '38px' } as CSSProperties}>
          {SYMBOLS.map((raw) => (
            <figure key={raw} className="m-0 flex flex-col items-center gap-2">
              <ManaSymbol token={parseManaSymbol(raw)} />
              <figcaption className="text-[11px]" style={{ color: text.faint }}>
                {`{${raw}}`}
              </figcaption>
            </figure>
          ))}
        </div>
      </section>

      <section className="mx-auto max-w-[1440px]">
        <SectionTitle>Estados</SectionTitle>
        {/* Coluna larga o bastante para a carta virada girar sem invadir a vizinha. */}
        <div className="grid grid-cols-[repeat(auto-fill,minmax(170px,170px))] justify-start gap-x-16 gap-y-9">
          {TILES.map((tile) => (
            <figure key={tile.card.id} className="m-0 flex flex-col gap-2">
              <Card card={tile.card} size="medium" />
              <figcaption className="flex flex-col gap-0.5">
                <span className="text-[12px] font-semibold" style={{ color: text.strong }}>
                  {tile.label}
                </span>
                <span className="text-[11px] leading-snug" style={{ color: text.muted }}>
                  {tile.note}
                </span>
              </figcaption>
            </figure>
          ))}
        </div>
      </section>
    </div>
  )
}

function SectionTitle({ children }: { children: string }): ReactElement {
  return (
    <h2
      className="m-0 mb-4 text-[11px] font-semibold tracking-[0.22em] uppercase"
      style={{ color: text.muted }}
    >
      {children}
    </h2>
  )
}
