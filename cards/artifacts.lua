-- Artefatos: aceleração de mana incolor e criaturas/utilitários que qualquer
-- deck pode jogar. Equipamentos ficaram de fora de propósito — o bônus de
-- equipamento fala da "criatura equipada", e `StaticAbility.affects` só sabe
-- descrever conjuntos por seletor, não "o objeto ao qual estou anexado".

card {
  name = "Sol Ring", cost = "{1}", type = "Artifact",
  rarity = "Rare", set = "LEA",
  text = "{T}: Add {C}{C}.",
  abilities = { mana_ability { produces = "{C}{C}", text = "{T}: Add {C}{C}." } },
}

card {
  name = "Mind Stone", cost = "{2}", type = "Artifact",
  rarity = "Uncommon", set = "WTH",
  text = "{T}: Add {C}.\n{1}, {T}, Sacrifice Mind Stone: Draw a card.",
  abilities = {
    mana_ability { produces = "{C}", text = "{T}: Add {C}." },
    activated {
      cost = "{1}", tap = true, sacrifice = IS_SELF,
      effect = draw(1),
      text = "{1}, {T}, Sacrifice Mind Stone: Draw a card.",
    },
  },
}

card {
  name = "Manalith", cost = "{3}", type = "Artifact",
  rarity = "Common", set = "M15",
  text = "{T}: Add one mana of any color.",
  abilities = { mana_ability { produces = "any", count = 1, text = "{T}: Add one mana of any color." } },
}

card {
  name = "Darksteel Ingot", cost = "{3}", type = "Artifact",
  rarity = "Uncommon", set = "DST",
  text = "Indestructible\n{T}: Add one mana of any color.",
  keywords = { "Indestructible" },
  abilities = { mana_ability { produces = "any", count = 1, text = "{T}: Add one mana of any color." } },
}

card {
  name = "Prophetic Prism", cost = "{2}", type = "Artifact",
  rarity = "Common", set = "ROE",
  text = "When Prophetic Prism enters the battlefield, draw a card.\n{1}, {T}: Add one mana of any color.",
  abilities = {
    etb(draw(1), { text = "When Prophetic Prism enters the battlefield, draw a card." }),
    mana_ability { cost = "{1}", produces = "any", count = 1, text = "{1}, {T}: Add one mana of any color." },
  },
}

card {
  name = "Palladium Myr", cost = "{4}", type = "Artifact Creature — Myr",
  pt = { 2, 2 }, rarity = "Uncommon", set = "SOM",
  text = "{T}: Add {C}{C}.",
  abilities = { mana_ability { produces = "{C}{C}", text = "{T}: Add {C}{C}." } },
}

card {
  name = "Ornithopter", cost = "{0}", type = "Artifact Creature — Thopter",
  pt = { 0, 2 }, rarity = "Uncommon", set = "ATQ",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Steel Wall", cost = "{1}", type = "Artifact Creature — Wall",
  pt = { 0, 4 }, rarity = "Common", set = "MRD",
  text = "Defender",
  keywords = { "Defender" },
}

card {
  name = "Bronze Sable", cost = "{2}", type = "Artifact Creature — Sable",
  pt = { 2, 1 }, rarity = "Common", set = "M13", text = "",
}

card {
  name = "Yotian Soldier", cost = "{3}", type = "Artifact Creature — Soldier",
  pt = { 1, 4 }, rarity = "Common", set = "ATQ",
  text = "Vigilance",
  keywords = { "Vigilance" },
}

card {
  name = "Skyscanner", cost = "{3}", type = "Artifact Creature — Thopter",
  pt = { 1, 1 }, rarity = "Common", set = "MRD",
  text = "Flying\nWhen Skyscanner enters the battlefield, draw a card.",
  keywords = { "Flying" },
  abilities = {
    etb(draw(1), { text = "When Skyscanner enters the battlefield, draw a card." }),
  },
}

card {
  name = "Runed Servitor", cost = "{3}", type = "Artifact Creature — Construct",
  pt = { 2, 2 }, rarity = "Common", set = "ROE",
  text = "When Runed Servitor dies, each player draws a card.",
  abilities = {
    dies(draw(1, EACH_PLAYER), { text = "When Runed Servitor dies, each player draws a card." }),
  },
}

card {
  name = "Bottle Gnomes", cost = "{3}", type = "Artifact Creature — Gnome",
  pt = { 1, 3 }, rarity = "Uncommon", set = "TMP",
  text = "Sacrifice Bottle Gnomes: You gain 3 life.",
  abilities = {
    activated {
      sacrifice = IS_SELF,
      effect = gain_life(3, YOU),
      text = "Sacrifice Bottle Gnomes: You gain 3 life.",
    },
  },
}

card {
  name = "Rod of Ruin", cost = "{4}", type = "Artifact",
  rarity = "Uncommon", set = "LEA",
  text = "{3}, {T}: Rod of Ruin deals 1 damage to any target.",
  abilities = {
    activated {
      cost = "{3}", tap = true,
      targets = { t_any() },
      effect = deal_damage(1),
      text = "{3}, {T}: Rod of Ruin deals 1 damage to any target.",
    },
  },
}

card {
  name = "Icy Manipulator", cost = "{4}", type = "Artifact",
  rarity = "Uncommon", set = "LEA",
  text = "{1}, {T}: Tap target artifact, creature, or land.",
  abilities = {
    activated {
      cost = "{1}", tap = true,
      targets = { t_permanent("target artifact, creature, or land", f_or(ARTIFACT, CREATURE, LAND)) },
      effect = tap(),
      text = "{1}, {T}: Tap target artifact, creature, or land.",
    },
  },
}

card {
  name = "Millstone", cost = "{2}", type = "Artifact",
  rarity = "Uncommon", set = "ATQ",
  text = "{2}, {T}: Target player mills two cards.",
  abilities = {
    activated {
      cost = "{2}", tap = true,
      targets = { t_player() },
      effect = mill(2, target_player(1)),
      text = "{2}, {T}: Target player mills two cards.",
    },
  },
}
