/**
 * Partida de demonstracao: roda inteira sem servidor, no formato exato do
 * protocolo (`InitFrame` + `EventsFrame[]`). E o que `matchSocket.ts` usa
 * quando o `mtg-server` nao responde, e o que da a este arquivo sua regra
 * mais dura — nenhum evento pode descrever um estado que a `view` do mesmo
 * frame nao sustente.
 *
 * Goblin Onslaught (vermelho agressivo) vs. Selesnya Valor (verde-branco de
 * valor), os dois primeiros decks reais de `crates/mtg-cards/src/decks.rs`.
 * Nove turnos: terrenos, curva de criaturas, um gatilho de compra, um bloqueio
 * que troca, uma remocao instantanea tentando (e falhando) segurar o jogo, e
 * um vencedor por dano.
 *
 * A partida inteira e construida por um pequeno interpretador de estado
 * abaixo — nao por `GameView` escritos a mao — porque so assim contagem de
 * mao/cemiterio e vida ficam garantidamente coerentes entre um frame e o
 * proximo.
 */
import type {
  CardView,
  Defender,
  EventsFrame,
  GameView,
  InitFrame,
  MatchEvent,
  ObjectId,
  Outcome,
  PlayerId,
  PlayerView,
  StackItemView,
  Step,
  ZoneKind,
} from '../types/protocol'
import { PHASE_OF_STEP, STEP_LABEL } from '../types/protocol'
import { MOCK_CARDS, toCardView } from './mockCards'

const P0_NAME = 'Goblin Onslaught'
const P1_NAME = 'Selesnya Valor'
const DEMO_SEED = 424242
const STARTING_LIFE = 20
const STARTING_LIBRARY = 60 - 7

function buildDemoMatch(): { init: InitFrame; frames: EventsFrame[] } {
  // -- Estado mutavel do interpretador -------------------------------------
  let idSeq = 1
  const nextId = (): ObjectId => idSeq++

  const cardsById = new Map<ObjectId, CardView>()
  const hands: [ObjectId[], ObjectId[]] = [[], []]
  const battlefield: [ObjectId[], ObjectId[]] = [[], []]
  const graveyards: [ObjectId[], ObjectId[]] = [[], []]
  const exiles: [ObjectId[], ObjectId[]] = [[], []]
  let stack: StackItemView[] = []
  let players: [PlayerView, PlayerView] = [mkPlayer(0, P0_NAME), mkPlayer(1, P1_NAME)]
  let turn = 1
  let step: Step = 'Untap'
  let activePlayer: PlayerId = 0
  let priorityPlayer: PlayerId | null = 0
  let outcome: Outcome = 'Ongoing'

  const frames: EventsFrame[] = []
  let pendingEvents: MatchEvent[] = []

  function mkPlayer(id: PlayerId, name: string): PlayerView {
    return {
      id,
      name,
      life: STARTING_LIFE,
      poison: 0,
      manaPool: { colored: [0, 0, 0, 0, 0], colorless: 0 },
      handCount: 0,
      libraryCount: STARTING_LIBRARY,
      graveyardCount: 0,
      exileCount: 0,
      landsPlayedThisTurn: 0,
      maxLandsPerTurn: 1,
      hasLost: false,
      isActive: id === 0,
      hasPriority: id === 0,
    }
  }

  function push(event: MatchEvent): void {
    pendingEvents.push(event)
  }

  function setPlayer(id: PlayerId, patch: Partial<PlayerView>): void {
    players[id === 0 ? 0 : 1] = { ...players[id], ...patch }
  }

  function card(id: ObjectId): CardView {
    const found = cardsById.get(id)
    if (!found) throw new Error(`demoMatch: carta desconhecida ${id}`)
    return found
  }

  function replaceCard(id: ObjectId, patch: Partial<CardView>): void {
    cardsById.set(id, { ...card(id), ...patch })
  }

  function zoneArray(zone: ZoneKind, player: PlayerId): ObjectId[] {
    if (zone === 'Hand') return hands[player]
    if (zone === 'Battlefield') return battlefield[player]
    if (zone === 'Graveyard') return graveyards[player]
    if (zone === 'Exile') return exiles[player]
    return [] // Library/Stack/Command nao tem array de zona na view.
  }

  function setZoneArray(zone: ZoneKind, player: PlayerId, ids: ObjectId[]): void {
    if (zone === 'Hand') hands[player] = ids
    else if (zone === 'Battlefield') battlefield[player] = ids
    else if (zone === 'Graveyard') graveyards[player] = ids
    else if (zone === 'Exile') exiles[player] = ids
  }

  function syncZoneCounts(owner: PlayerId): void {
    setPlayer(owner, {
      handCount: hands[owner].length,
      graveyardCount: graveyards[owner].length,
      exileCount: exiles[owner].length,
    })
  }

  function moveZone(id: ObjectId, from: ZoneKind, to: ZoneKind, owner: PlayerId): void {
    setZoneArray(from, owner, zoneArray(from, owner).filter((x) => x !== id))
    setZoneArray(to, owner, [...zoneArray(to, owner), id])
    replaceCard(id, { zone: to })
    push({ type: 'cardMoved', card: id, from, to, owner, reveal: true })
    syncZoneCounts(owner)
  }

  function instantiateInHand(templateKey: keyof typeof MOCK_CARDS, owner: PlayerId): ObjectId {
    const id = nextId()
    cardsById.set(id, toCardView(MOCK_CARDS[templateKey], id, { zone: 'Hand', controller: owner, owner }))
    hands[owner] = [...hands[owner], id]
    return id
  }

  function draw(player: PlayerId, templateKey: keyof typeof MOCK_CARDS): ObjectId {
    const id = instantiateInHand(templateKey, player)
    setPlayer(player, { libraryCount: players[player].libraryCount - 1 })
    push({ type: 'cardDrawn', card: id, player })
    syncZoneCounts(player)
    return id
  }

  function goToStep(next: Step): void {
    step = next
    push({ type: 'stepChange', step: next, label: STEP_LABEL[next] })
  }

  function untapAndCleanup(active: PlayerId): void {
    for (const id of battlefield[active]) {
      if (card(id).tapped) {
        replaceCard(id, { tapped: false })
        push({ type: 'untapped', card: id })
      }
    }
    // Limpeza: dano marcado esvazia entre turnos, para os dois lados.
    for (const side of [0, 1] as const) {
      for (const id of battlefield[side]) {
        if (card(id).damage !== 0) replaceCard(id, { damage: 0 })
      }
    }
  }

  function beginTurn(active: PlayerId, turnNumber: number): void {
    turn = turnNumber
    activePlayer = active
    priorityPlayer = active
    untapAndCleanup(active)
    push({ type: 'turnStart', turn: turnNumber, player: active })
    setPlayer(0, { isActive: active === 0, hasPriority: active === 0, landsPlayedThisTurn: active === 0 ? 0 : players[0].landsPlayedThisTurn })
    setPlayer(1, { isActive: active === 1, hasPriority: active === 1, landsPlayedThisTurn: active === 1 ? 0 : players[1].landsPlayedThisTurn })
    goToStep('Untap')
    goToStep('Draw')
  }

  function playLand(player: PlayerId, id: ObjectId): void {
    moveZone(id, 'Hand', 'Battlefield', player)
    setPlayer(player, { landsPlayedThisTurn: players[player].landsPlayedThisTurn + 1 })
  }

  function tapLandsForMana(player: PlayerId, amount: number): void {
    const untapped = battlefield[player]
      .map((id) => card(id))
      .filter((c) => c.typeLine === 'Land' && !c.tapped)
      .slice(0, amount)
    if (untapped.length < amount) {
      throw new Error(`demoMatch: mana insuficiente para o jogador ${player}`)
    }
    for (const land of untapped) {
      replaceCard(land.id, { tapped: true })
      push({ type: 'tapped', card: land.id })
    }
  }

  interface CastOptions {
    haste?: boolean
    /** Bonus de +X/+Y de um anthem ja em campo (ex.: Goblin Chieftain). */
    buff?: readonly [number, number]
    etbDraw?: keyof typeof MOCK_CARDS
  }

  function castCreature(player: PlayerId, id: ObjectId, opts: CastOptions = {}): void {
    const spell = card(id)
    tapLandsForMana(player, spell.manaValue)
    moveZone(id, 'Hand', 'Stack', player)
    stack = [
      ...stack,
      {
        id,
        name: spell.name ?? '',
        text: spell.oracleText ?? '',
        controller: player,
        targets: [],
        targetPlayers: [],
        isAbility: false,
        sourceCard: id,
        artKey: spell.artKey,
      },
    ]
    push({ type: 'spellCast', card: id, player, targets: [] })
    push({ type: 'spellResolved', card: id })
    stack = stack.filter((item) => item.id !== id)
    moveZone(id, 'Stack', 'Battlefield', player)
    const [buffP, buffT] = opts.buff ?? [0, 0]
    const keywords =
      opts.haste && !spell.keywords.includes('Haste') ? [...spell.keywords, 'Haste'] : spell.keywords
    replaceCard(id, {
      power: spell.power === null ? null : spell.power + buffP,
      toughness: spell.toughness === null ? null : spell.toughness + buffT,
      keywords,
      summoningSick: !opts.haste,
      tapped: false,
    })
    if (opts.etbDraw) {
      push({ type: 'abilityTriggered', source: id, text: `${spell.name ?? 'Carta'} compra uma carta.` })
      draw(player, opts.etbDraw)
    }
  }

  function castInstant(player: PlayerId, id: ObjectId, targetId: ObjectId): void {
    const spell = card(id)
    tapLandsForMana(player, spell.manaValue)
    moveZone(id, 'Hand', 'Stack', player)
    stack = [
      ...stack,
      {
        id,
        name: spell.name ?? '',
        text: spell.oracleText ?? '',
        controller: player,
        targets: [targetId],
        targetPlayers: [],
        isAbility: false,
        sourceCard: id,
        artKey: spell.artKey,
      },
    ]
    push({ type: 'spellCast', card: id, player, targets: [targetId] })
    push({ type: 'spellResolved', card: id })
    stack = stack.filter((item) => item.id !== id)
    destroyPermanent(targetId)
    moveZone(id, 'Stack', 'Graveyard', player)
  }

  function destroyPermanent(id: ObjectId): void {
    const target = card(id)
    push({ type: 'destroyed', card: id })
    moveZone(id, 'Battlefield', 'Graveyard', target.owner)
    replaceCard(id, { tapped: false, attacking: null, blocking: [], blockedBy: [], damage: 0 })
  }

  // -- Combate ---------------------------------------------------------------

  function declareAttackers(defender: PlayerId, attackerIds: readonly ObjectId[]): void {
    for (const id of attackerIds) {
      replaceCard(id, { tapped: true, attacking: { Player: defender } })
      push({ type: 'tapped', card: id })
    }
    push({
      type: 'attackersDeclared',
      attackers: attackerIds.map((id): [ObjectId, Defender] => [id, { Player: defender }]),
    })
  }

  function declareBlockers(blocks: readonly (readonly [ObjectId, readonly ObjectId[]])[]): void {
    for (const [attackerId, blockerIds] of blocks) {
      replaceCard(attackerId, { blockedBy: [...blockerIds] })
      for (const blockerId of blockerIds) replaceCard(blockerId, { blocking: [attackerId] })
    }
    push({ type: 'blockersDeclared', blocks: blocks.map(([a, bs]) => [a, [...bs]] as [ObjectId, ObjectId[]]) })
  }

  function fightDamage(sourceId: ObjectId, targetId: ObjectId): void {
    const source = card(sourceId)
    const amount = source.power ?? 0
    if (amount <= 0) return
    const target = card(targetId)
    const lethal = amount >= (target.toughness ?? 0)
    push({ type: 'damageDealt', source: sourceId, target: targetId, amount, lethal })
    replaceCard(targetId, { damage: target.damage + amount })
  }

  function damagePlayer(sourceId: ObjectId, defender: PlayerId): void {
    const amount = card(sourceId).power ?? 0
    if (amount <= 0) return
    push({ type: 'damageToPlayer', source: sourceId, player: defender, amount })
    const before = players[defender].life
    const total = Math.max(0, before - amount)
    setPlayer(defender, { life: total })
    if (total !== before) push({ type: 'lifeChanged', player: defender, delta: total - before, total })
  }

  function applyLethalDamage(ids: readonly ObjectId[]): void {
    for (const id of ids) {
      const c = card(id)
      if (c.zone !== 'Battlefield') continue
      const toughness = c.toughness ?? 0
      if (toughness > 0 && c.damage >= toughness) {
        push({ type: 'died', card: id })
        moveZone(id, 'Battlefield', 'Graveyard', c.owner)
        replaceCard(id, { tapped: false, attacking: null, blocking: [], blockedBy: [], damage: 0 })
      }
    }
  }

  function runCombat(
    active: PlayerId,
    defender: PlayerId,
    attackerIds: readonly ObjectId[],
    blocks: readonly (readonly [ObjectId, readonly ObjectId[]])[],
    instantResponse?: () => void,
  ): void {
    if (attackerIds.length === 0) return
    goToStep('BeginCombat')
    goToStep('DeclareAttackers')
    declareAttackers(defender, attackerIds)

    if (instantResponse) {
      priorityPlayer = defender
      setPlayer(defender, { hasPriority: true })
      setPlayer(active, { hasPriority: false })
      instantResponse()
      priorityPlayer = active
      setPlayer(defender, { hasPriority: false })
      setPlayer(active, { hasPriority: true })
    }

    const blocked = new Set(blocks.map(([attackerId]) => attackerId))
    if (blocks.length > 0) {
      goToStep('DeclareBlockers')
      declareBlockers(blocks)
    }

    goToStep('CombatDamage')
    const touched: ObjectId[] = []
    for (const [attackerId, blockerIds] of blocks) {
      if (card(attackerId).zone !== 'Battlefield') continue
      for (const blockerId of blockerIds) {
        fightDamage(attackerId, blockerId)
        fightDamage(blockerId, attackerId)
        touched.push(blockerId)
      }
      touched.push(attackerId)
    }
    for (const attackerId of attackerIds) {
      if (!blocked.has(attackerId) && card(attackerId).zone === 'Battlefield') {
        damagePlayer(attackerId, defender)
      }
    }
    applyLethalDamage(touched)

    goToStep('EndCombat')
    for (const id of attackerIds) {
      if (card(id).zone === 'Battlefield') replaceCard(id, { attacking: null, blockedBy: [] })
    }
    for (const [, blockerIds] of blocks) {
      for (const blockerId of blockerIds) {
        if (card(blockerId).zone === 'Battlefield') replaceCard(blockerId, { blocking: [] })
      }
    }
  }

  // -- Frames ------------------------------------------------------------

  function buildView(logTail: readonly string[]): GameView {
    return {
      turn,
      step,
      stepLabel: STEP_LABEL[step],
      phase: PHASE_OF_STEP[step],
      activePlayer,
      priorityPlayer,
      outcome,
      players: [players[0], players[1]],
      cards: Array.from(cardsById.values()),
      battlefield: [battlefield[0].slice(), battlefield[1].slice()],
      hands: [hands[0].slice(), hands[1].slice()],
      graveyards: [graveyards[0].slice(), graveyards[1].slice()],
      exiles: [exiles[0].slice(), exiles[1].slice()],
      stack: stack.slice(),
      prompt: null,
      logTail: [...logTail],
    }
  }

  function commit(logLines: readonly string[]): void {
    if (pendingEvents.length === 0) return
    frames.push({ type: 'events', events: pendingEvents, view: buildView(logLines) })
    pendingEvents = []
  }

  // ==========================================================================
  // Roteiro da partida
  // ==========================================================================

  // Maos iniciais — sete cartas cada, ja com os terrenos e o que sera jogado.
  const [mtnA1, mtnA2, mtnA3, mtnA4, ragingGoblinId, goblinPikerId, goblinChieftainId] = (
    ['mountain', 'mountain', 'mountain', 'mountain', 'ragingGoblin', 'goblinPiker', 'goblinChieftain'] as const
  ).map((key) => instantiateInHand(key, 0))

  const [, plainsB1, plainsB2, elvishVisionaryId, veteranArmorerId] = (
    ['forest', 'plains', 'plains', 'elvishVisionary', 'veteranArmorer', 'wallOfBlossoms', 'centaurCourser'] as const
  ).map((key) => instantiateInHand(key, 1))
  syncZoneCounts(0)
  syncZoneCounts(1)

  const initView = buildView([])
  const init: InitFrame = { type: 'init', view: initView, players: [P0_NAME, P1_NAME], seed: DEMO_SEED }

  // -- Turno 1 (P0): joga terreno, conjura Raging Goblin, ataca com pressa --
  beginTurn(0, 1)
  goToStep('PrecombatMain')
  playLand(0, mtnA1)
  castCreature(0, ragingGoblinId, { haste: true })
  commit(['Goblin Onslaught joga Mountain.', 'Goblin Onslaught conjura Raging Goblin.'])

  runCombat(0, 1, [ragingGoblinId], [])
  commit(['Raging Goblin ataca com pressa.', 'Selesnya Valor sofre 1 de dano de combate.'])

  // -- Turno 2 (P1): compra Divine Verdict, so joga terreno --
  beginTurn(1, 2)
  const divineVerdictId = draw(1, 'divineVerdict')
  goToStep('PrecombatMain')
  playLand(1, plainsB1)
  commit(['Selesnya Valor compra uma carta.', 'Selesnya Valor joga Plains.'])

  // -- Turno 3 (P0): segundo terreno, Goblin Piker, ataca de novo --
  beginTurn(0, 3)
  const moggFanaticId = draw(0, 'moggFanatic')
  goToStep('PrecombatMain')
  playLand(0, mtnA2)
  castCreature(0, goblinPikerId)
  commit(['Goblin Onslaught compra uma carta.', 'Goblin Onslaught joga Mountain.', 'Goblin Onslaught conjura Goblin Piker.'])

  runCombat(0, 1, [ragingGoblinId], [])
  commit(['Raging Goblin ataca.', 'Selesnya Valor sofre 1 de dano de combate.'])

  // -- Turno 4 (P1): Elvish Visionary entra e compra --
  beginTurn(1, 4)
  const extraPlainsId = draw(1, 'plains')
  goToStep('PrecombatMain')
  playLand(1, plainsB2)
  castCreature(1, elvishVisionaryId, { etbDraw: 'forest' })
  commit([
    'Selesnya Valor compra uma carta.',
    'Selesnya Valor joga Plains.',
    'Selesnya Valor conjura Elvish Visionary.',
    'Elvish Visionary entra no campo de batalha: Selesnya Valor compra uma carta.',
  ])

  // -- Turno 5 (P0): Goblin Chieftain, anthem retroativo, ataque com bloqueio --
  beginTurn(0, 5)
  const emberHaulerId = draw(0, 'emberHauler')
  goToStep('PrecombatMain')
  playLand(0, mtnA3)
  castCreature(0, goblinChieftainId, { haste: true })
  // Anthem do Chieftain: os outros Goblins ja em campo recebem +1/+1 e pressa.
  for (const goblinId of [ragingGoblinId, goblinPikerId]) {
    const goblin = card(goblinId)
    replaceCard(goblinId, {
      power: (goblin.power ?? 0) + 1,
      toughness: (goblin.toughness ?? 0) + 1,
      keywords: goblin.keywords.includes('Haste') ? goblin.keywords : [...goblin.keywords, 'Haste'],
    })
  }
  commit([
    'Goblin Onslaught compra uma carta.',
    'Goblin Onslaught joga Mountain.',
    'Goblin Onslaught conjura Goblin Chieftain.',
    'Outros Goblins recebem +1/+1 e pressa.',
  ])

  runCombat(0, 1, [ragingGoblinId, goblinPikerId, goblinChieftainId], [[goblinPikerId, [elvishVisionaryId]]])
  commit([
    'Goblin Onslaught ataca com Raging Goblin, Goblin Piker e Goblin Chieftain.',
    'Selesnya Valor bloqueia Goblin Piker com Elvish Visionary.',
    'Elvish Visionary morre no combate.',
    'Selesnya Valor sofre 4 de dano de combate.',
  ])

  // -- Turno 6 (P1): Veteran Armorer entra como bloqueador --
  beginTurn(1, 6)
  draw(1, 'plains')
  goToStep('PrecombatMain')
  playLand(1, extraPlainsId)
  castCreature(1, veteranArmorerId)
  commit(['Selesnya Valor compra uma carta.', 'Selesnya Valor joga Plains.', 'Selesnya Valor conjura Veteran Armorer.'])

  // -- Turno 7 (P0): Mogg Fanatic e Ember Hauler, ja com o anthem --
  beginTurn(0, 7)
  draw(0, 'lightningBolt')
  goToStep('PrecombatMain')
  playLand(0, mtnA4)
  castCreature(0, moggFanaticId, { haste: true, buff: [1, 1] })
  castCreature(0, emberHaulerId, { haste: true, buff: [1, 1] })
  commit([
    'Goblin Onslaught compra uma carta.',
    'Goblin Onslaught joga Mountain.',
    'Goblin Onslaught conjura Mogg Fanatic e Ember Hauler, ja com pressa pelo Chieftain.',
  ])

  runCombat(
    0,
    1,
    [ragingGoblinId, goblinPikerId, goblinChieftainId, moggFanaticId, emberHaulerId],
    [[emberHaulerId, [veteranArmorerId]]],
  )
  commit([
    'Goblin Onslaught ataca com todos os cinco Goblins.',
    'Selesnya Valor bloqueia Ember Hauler com Veteran Armorer.',
    'Veteran Armorer morre no combate.',
    'Selesnya Valor sofre 9 de dano de combate.',
  ])

  // -- Turno 8 (P1): so terreno, segurando mana para a resposta --
  beginTurn(1, 8)
  const landForT8 = draw(1, 'plains')
  goToStep('PrecombatMain')
  playLand(1, landForT8)
  commit(['Selesnya Valor compra uma carta.', 'Selesnya Valor joga Plains e segura mana.'])

  // -- Turno 9 (P0): ataque final; Divine Verdict tenta segurar e nao basta --
  beginTurn(0, 9)
  draw(0, 'mountain')
  goToStep('PrecombatMain')
  commit(['Goblin Onslaught compra uma carta.'])

  runCombat(
    0,
    1,
    [ragingGoblinId, goblinPikerId, goblinChieftainId, moggFanaticId, emberHaulerId],
    [],
    () => castInstant(1, divineVerdictId, emberHaulerId),
  )
  commit([
    'Goblin Onslaught ataca com todos os cinco Goblins.',
    'Selesnya Valor conjura Divine Verdict em Ember Hauler.',
    'Ember Hauler e destruido antes do dano.',
    'Selesnya Valor sofre 9 de dano de combate e cai a 0 de vida.',
  ])

  setPlayer(1, { hasLost: true })
  push({ type: 'playerLost', player: 1, reason: 'ZeroLife' })
  outcome = { Winner: 0 }
  push({ type: 'gameOver', outcome })
  commit(['Selesnya Valor perde a partida.', 'Goblin Onslaught vence a partida!'])

  return { init, frames }
}

export const demoMatch: { init: InitFrame; frames: EventsFrame[] } = buildDemoMatch()
