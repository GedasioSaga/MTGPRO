-- Vermelho: dano direto, criaturas baratas com pressa e destruição de
-- artefato/terreno. Cartas com {X} no custo ficaram de fora: `v()` não sabe
-- embrulhar Value::X, e forçar isso seria inventar sintaxe.

card {
  name = "Raging Goblin", cost = "{R}", type = "Creature — Goblin Berserker",
  pt = { 1, 1 }, rarity = "Common", set = "USG",
  text = "Haste",
  keywords = { "Haste" },
}

card {
  name = "Mogg Fanatic", cost = "{R}", type = "Creature — Goblin",
  pt = { 1, 1 }, rarity = "Uncommon", set = "TMP",
  text = "Sacrifice Mogg Fanatic: Mogg Fanatic deals 1 damage to any target.",
  abilities = {
    activated {
      sacrifice = IS_SELF,
      targets = { t_any() },
      effect = deal_damage(1),
      text = "Sacrifice Mogg Fanatic: Mogg Fanatic deals 1 damage to any target.",
    },
  },
}

card {
  name = "Goblin Piker", cost = "{1}{R}", type = "Creature — Goblin Warrior",
  pt = { 2, 1 }, rarity = "Common", set = "M10", text = "",
}

card {
  name = "Ember Hauler", cost = "{1}{R}", type = "Creature — Goblin",
  pt = { 2, 2 }, rarity = "Rare", set = "M11",
  text = "{1}, Sacrifice Ember Hauler: Ember Hauler deals 2 damage to any target.",
  abilities = {
    activated {
      cost = "{1}", sacrifice = IS_SELF,
      targets = { t_any() },
      effect = deal_damage(2),
      text = "{1}, Sacrifice Ember Hauler: Ember Hauler deals 2 damage to any target.",
    },
  },
}

card {
  name = "Goblin Chieftain", cost = "{1}{R}{R}", type = "Creature — Goblin",
  pt = { 2, 2 }, rarity = "Rare", set = "M10",
  text = "Haste\nOther Goblin creatures you control get +1/+1 and have haste.",
  keywords = { "Haste" },
  abilities = {
    static_pt(1, 1,
      creatures { filter = f_and(CREATURE, has_subtype("Goblin"), IS_OTHER), owner = YOU },
      "Other Goblin creatures you control get +1/+1."),
    static_grant({ "Haste" },
      creatures { filter = f_and(CREATURE, has_subtype("Goblin"), IS_OTHER), owner = YOU },
      "Other Goblin creatures you control have haste."),
  },
}

card {
  name = "Prodigal Pyromancer", cost = "{2}{R}", type = "Creature — Human Wizard",
  pt = { 1, 1 }, rarity = "Uncommon", set = "M10",
  text = "{T}: Prodigal Pyromancer deals 1 damage to any target.",
  abilities = {
    activated {
      tap = true,
      targets = { t_any() },
      effect = deal_damage(1),
      text = "{T}: Prodigal Pyromancer deals 1 damage to any target.",
    },
  },
}

card {
  name = "Manic Vandal", cost = "{2}{R}", type = "Creature — Human Warrior",
  pt = { 2, 2 }, rarity = "Common", set = "M12",
  text = "When Manic Vandal enters the battlefield, destroy target artifact.",
  abilities = {
    etb(destroy(), {
      targets = { t_permanent("target artifact", ARTIFACT) },
      text = "When Manic Vandal enters the battlefield, destroy target artifact.",
    }),
  },
}

card {
  name = "Furnace Whelp", cost = "{3}{R}", type = "Creature — Dragon",
  pt = { 2, 2 }, rarity = "Uncommon", set = "M10",
  text = "Flying\n{R}: Furnace Whelp gets +1/+0 until end of turn.",
  keywords = { "Flying" },
  abilities = {
    activated {
      cost = "{R}",
      effect = pump(1, 0, { target = SELF }),
      text = "{R}: Furnace Whelp gets +1/+0 until end of turn.",
    },
  },
}

card {
  name = "Fire Elemental", cost = "{3}{R}{R}", type = "Creature — Elemental",
  pt = { 5, 4 }, rarity = "Common", set = "LEA", text = "",
}

card {
  name = "Shivan Dragon", cost = "{4}{R}{R}", type = "Creature — Dragon",
  pt = { 5, 5 }, rarity = "Rare", set = "LEA",
  text = "Flying\n{R}: Shivan Dragon gets +1/+0 until end of turn.",
  keywords = { "Flying" },
  abilities = {
    activated {
      cost = "{R}",
      effect = pump(1, 0, { target = SELF }),
      text = "{R}: Shivan Dragon gets +1/+0 until end of turn.",
    },
  },
}

card {
  name = "Lightning Bolt", cost = "{R}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Lightning Bolt deals 3 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(3),
}

card {
  name = "Shock", cost = "{R}", type = "Instant",
  rarity = "Common", set = "M10",
  text = "Shock deals 2 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(2),
}

card {
  name = "Searing Spear", cost = "{1}{R}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Searing Spear deals 3 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(3),
}

card {
  name = "Incinerate", cost = "{1}{R}", type = "Instant",
  rarity = "Common", set = "M10",
  text = "Incinerate deals 3 damage to any target. If a creature dealt damage this way would die this turn, exile it instead.",
  targets = { t_any() },
  effect = deal_damage(3),
}

card {
  name = "Volcanic Hammer", cost = "{1}{R}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Volcanic Hammer deals 3 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(3),
}

card {
  name = "Flame Slash", cost = "{R}", type = "Sorcery",
  rarity = "Common", set = "ROE",
  text = "Flame Slash deals 4 damage to target creature.",
  targets = { t_creature() },
  effect = deal_damage(4),
}

card {
  name = "Seismic Strike", cost = "{1}{R}", type = "Instant",
  rarity = "Common", set = "ROE",
  text = "Seismic Strike deals damage equal to the number of Mountains you control to target creature.",
  targets = { t_creature() },
  effect = deal_damage(count_of(sel { filter = has_subtype("Mountain"), owner = YOU })),
}

card {
  name = "Chandra's Outrage", cost = "{3}{R}{R}", type = "Instant",
  rarity = "Common", set = "M12",
  text = "Chandra's Outrage deals 4 damage to target creature and 2 damage to that creature's controller.",
  targets = { t_creature() },
  effect = seq {
    deal_damage(4, target(1)),
    damage_player(2, controller_of(target(1))),
  },
}

card {
  name = "Lava Axe", cost = "{4}{R}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Lava Axe deals 5 damage to target player or planeswalker.",
  targets = { t_player() },
  effect = damage_player(5, target_player(1)),
}

card {
  name = "Pyroclasm", cost = "{1}{R}", type = "Sorcery",
  rarity = "Uncommon", set = "USG",
  text = "Pyroclasm deals 2 damage to each creature.",
  effect = deal_damage(2, all(creatures())),
}

card {
  name = "Shatter", cost = "{1}{R}", type = "Instant",
  rarity = "Common", set = "MRD",
  text = "Destroy target artifact.",
  targets = { t_permanent("target artifact", ARTIFACT) },
  effect = destroy(),
}

card {
  name = "Smelt", cost = "{R}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Destroy target artifact.",
  targets = { t_permanent("target artifact", ARTIFACT) },
  effect = destroy(),
}

card {
  name = "Stone Rain", cost = "{2}{R}", type = "Sorcery",
  rarity = "Common", set = "LEA",
  text = "Destroy target land.",
  targets = { t_permanent("target land", LAND) },
  effect = destroy(),
}

card {
  name = "Act of Treason", cost = "{2}{R}", type = "Sorcery",
  rarity = "Uncommon", set = "M10",
  text = "Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn.",
  targets = { t_creature() },
  effect = seq {
    gain_control(target(1), YOU, "EndOfTurn"),
    untap(target(1)),
    grant({ "Haste" }),
  },
}

card {
  name = "Titan's Strength", cost = "{R}", type = "Instant",
  rarity = "Common", set = "THS",
  text = "Target creature gets +3/+1 until end of turn. Scry 1.",
  targets = { t_creature() },
  effect = seq { pump(3, 1), scry(1, YOU) },
}

card {
  name = "Trumpet Blast", cost = "{2}{R}", type = "Instant",
  rarity = "Common", set = "M10",
  text = "Attacking creatures get +2/+0 until end of turn.",
  effect = pump(2, 0, { target = all(sel { filter = f_and(CREATURE, ATTACKING) }) }),
}

card {
  name = "Dragon Fodder", cost = "{1}{R}", type = "Sorcery",
  rarity = "Common", set = "ALA",
  text = "Create two 1/1 red Goblin creature tokens.",
  effect = goblin_token(2),
}

card {
  name = "Krenko's Command", cost = "{1}{R}", type = "Sorcery",
  rarity = "Common", set = "M13",
  text = "Create two 1/1 red Goblin creature tokens.",
  effect = goblin_token(2),
}
