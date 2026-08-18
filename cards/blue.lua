-- Azul: contra-magia, compra, devolução para a mão e criaturas voadoras.
-- Nada de "olhe as três primeiras cartas e reordene" — o IR não tem manipulação
-- de topo de grimório, então Ponder/Brainstorm ficaram de fora em vez de virar
-- uma aproximação errada.

card {
  name = "Merfolk of the Pearl Trident", cost = "{U}", type = "Creature — Merfolk",
  pt = { 1, 1 }, rarity = "Common", set = "LEA", text = "",
}

card {
  name = "Merfolk Looter", cost = "{1}{U}", type = "Creature — Merfolk Rogue",
  pt = { 1, 1 }, rarity = "Uncommon", set = "USG",
  text = "{T}: Draw a card, then discard a card.",
  abilities = {
    activated {
      tap = true,
      effect = seq { draw(1, YOU), discard(1, YOU) },
      text = "{T}: Draw a card, then discard a card.",
    },
  },
}

card {
  name = "Wind Drake", cost = "{2}{U}", type = "Creature — Drake",
  pt = { 2, 2 }, rarity = "Common", set = "M12",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Cloudkin Seer", cost = "{2}{U}", type = "Creature — Elemental Wizard",
  pt = { 2, 1 }, rarity = "Common", set = "M20",
  text = "Flying\nWhen Cloudkin Seer enters the battlefield, draw a card.",
  keywords = { "Flying" },
  abilities = {
    etb(draw(1), { text = "When Cloudkin Seer enters the battlefield, draw a card." }),
  },
}

card {
  name = "Snapping Drake", cost = "{3}{U}", type = "Creature — Drake",
  pt = { 3, 2 }, rarity = "Common", set = "USG",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Air Elemental", cost = "{3}{U}{U}", type = "Creature — Elemental",
  pt = { 4, 4 }, rarity = "Uncommon", set = "LEA",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Man-o'-War", cost = "{2}{U}", type = "Creature — Jellyfish",
  pt = { 2, 2 }, rarity = "Common", set = "VIS",
  text = "When Man-o'-War enters the battlefield, return target creature to its owner's hand.",
  abilities = {
    etb(bounce(target(1)), {
      targets = { t_creature() },
      text = "When Man-o'-War enters the battlefield, return target creature to its owner's hand.",
    }),
  },
}

card {
  name = "Aether Adept", cost = "{1}{U}{U}", type = "Creature — Human Wizard",
  pt = { 2, 2 }, rarity = "Common", set = "M11",
  text = "When Aether Adept enters the battlefield, return target creature to its owner's hand.",
  abilities = {
    etb(bounce(target(1)), {
      targets = { t_creature() },
      text = "When Aether Adept enters the battlefield, return target creature to its owner's hand.",
    }),
  },
}

card {
  name = "Aven Fisher", cost = "{3}{U}", type = "Creature — Bird Soldier",
  pt = { 2, 2 }, rarity = "Common", set = "ODY",
  text = "Flying\nWhen Aven Fisher dies, draw a card.",
  keywords = { "Flying" },
  abilities = {
    dies(draw(1), { text = "When Aven Fisher dies, draw a card." }),
  },
}

card {
  name = "Frost Lynx", cost = "{2}{U}", type = "Creature — Elemental Cat",
  pt = { 2, 2 }, rarity = "Common", set = "M15",
  text = "When Frost Lynx enters the battlefield, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
  abilities = {
    etb(seq { tap(target(1)), freeze(target(1)) }, {
      targets = { t_creature("target creature an opponent controls", { owner = OPPONENTS }) },
      text = "When Frost Lynx enters the battlefield, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
    }),
  },
}

card {
  name = "Thieving Magpie", cost = "{3}{U}", type = "Creature — Bird",
  pt = { 1, 3 }, rarity = "Uncommon", set = "USG",
  text = "Flying\nWhenever Thieving Magpie deals combat damage to a player, draw a card.",
  keywords = { "Flying" },
  abilities = {
    when_deals_damage_to_player(draw(1),
      { text = "Whenever Thieving Magpie deals combat damage to a player, draw a card." }),
  },
}

card {
  name = "Sower of Temptation", cost = "{2}{U}{U}", type = "Creature — Faerie Rogue",
  pt = { 2, 2 }, rarity = "Rare", set = "LRW",
  text = "Flying\nWhen Sower of Temptation enters the battlefield, gain control of target creature for as long as Sower of Temptation remains on the battlefield.",
  keywords = { "Flying" },
  abilities = {
    -- Duration::WhileSourcePresent é exatamente "enquanto isto permanecer no
    -- campo de batalha" (CR 611.2b).
    etb(gain_control(target(1), YOU, "WhileSourcePresent"), {
      targets = { t_creature() },
      text = "When Sower of Temptation enters the battlefield, gain control of target creature for as long as Sower of Temptation remains on the battlefield.",
    }),
  },
}

card {
  name = "Counterspell", cost = "{U}{U}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Counter target spell.",
  targets = { t_spell() },
  effect = counter_spell(),
}

card {
  name = "Cancel", cost = "{1}{U}{U}", type = "Instant",
  rarity = "Common", set = "M10",
  text = "Counter target spell.",
  targets = { t_spell() },
  effect = counter_spell(),
}

card {
  name = "Essence Scatter", cost = "{1}{U}", type = "Instant",
  rarity = "Uncommon", set = "M10",
  text = "Counter target creature spell.",
  targets = { t_spell("target creature spell", CREATURE) },
  effect = counter_spell(),
}

card {
  name = "Negate", cost = "{1}{U}", type = "Instant",
  rarity = "Common", set = "M11",
  text = "Counter target noncreature spell.",
  targets = { t_spell("target noncreature spell", f_not(CREATURE)) },
  effect = counter_spell(),
}

card {
  name = "Mana Leak", cost = "{1}{U}", type = "Instant",
  rarity = "Common", set = "M12",
  text = "Counter target spell unless its controller pays {3}.",
  targets = { t_spell() },
  effect = counter_spell(target(1), { Mana = mana("{3}") }),
}

card {
  name = "Dismiss", cost = "{3}{U}{U}", type = "Instant",
  rarity = "Uncommon", set = "TMP",
  text = "Counter target spell. Draw a card.",
  targets = { t_spell() },
  effect = seq { counter_spell(), draw(1) },
}

card {
  name = "Unsummon", cost = "{U}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Return target creature to its owner's hand.",
  targets = { t_creature() },
  effect = bounce(),
}

card {
  name = "Boomerang", cost = "{U}{U}", type = "Instant",
  rarity = "Common", set = "MIR",
  text = "Return target permanent to its owner's hand.",
  targets = { t_permanent() },
  effect = bounce(),
}

card {
  name = "Griptide", cost = "{3}{U}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Put target creature on top of its owner's library.",
  targets = { t_creature() },
  effect = { PutOnTopOfLibrary = { target = target(1) } },
}

card {
  name = "Opt", cost = "{U}", type = "Instant",
  rarity = "Common", set = "INV",
  text = "Scry 1.\nDraw a card.",
  effect = seq { scry(1, YOU), draw(1) },
}

card {
  name = "Divination", cost = "{2}{U}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Draw two cards.",
  effect = draw(2),
}

card {
  name = "Jace's Ingenuity", cost = "{3}{U}{U}", type = "Instant",
  rarity = "Uncommon", set = "M11",
  text = "Draw three cards.",
  effect = draw(3),
}

card {
  name = "Tome Scour", cost = "{U}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Target player mills five cards.",
  targets = { t_player() },
  effect = mill(5, target_player(1)),
}

card {
  name = "Sleep", cost = "{2}{U}{U}", type = "Sorcery",
  rarity = "Uncommon", set = "M10",
  text = "Tap all creatures target player controls. Those creatures don't untap during that player's next untap step.",
  targets = { t_player() },
  effect = seq {
    tap(all(creatures { owner = target_player(1) })),
    freeze(all(creatures { owner = target_player(1) })),
  },
}

card {
  name = "Talrand's Invocation", cost = "{3}{U}", type = "Sorcery",
  rarity = "Uncommon", set = "M13",
  text = "Create two 2/2 blue Drake creature tokens with flying.",
  effect = drake_token(2),
}
