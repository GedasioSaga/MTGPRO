-- Verde: aceleração de mana, corpos grandes, truques de combate e "fight".

card {
  name = "Llanowar Elves", cost = "{G}", type = "Creature — Elf Druid",
  pt = { 1, 1 }, rarity = "Common", set = "LEA",
  text = "{T}: Add {G}.",
  abilities = { mana_ability { produces = "{G}", text = "{T}: Add {G}." } },
}

card {
  name = "Elvish Mystic", cost = "{G}", type = "Creature — Elf Druid",
  pt = { 1, 1 }, rarity = "Common", set = "M14",
  text = "{T}: Add {G}.",
  abilities = { mana_ability { produces = "{G}", text = "{T}: Add {G}." } },
}

card {
  name = "Arbor Elf", cost = "{G}", type = "Creature — Elf Druid",
  pt = { 1, 1 }, rarity = "Common", set = "M13",
  text = "{T}: Untap target Forest.",
  abilities = {
    -- Não é habilidade de mana: desvirar um terreno não adiciona mana ao pool,
    -- então usa a pilha como qualquer ativada (CR 605.1a).
    activated {
      tap = true,
      targets = { t_permanent("target Forest", has_subtype("Forest")) },
      effect = untap(),
      text = "{T}: Untap target Forest.",
    },
  },
}

card {
  name = "Grizzly Bears", cost = "{1}{G}", type = "Creature — Bear",
  pt = { 2, 2 }, rarity = "Common", set = "LEA", text = "",
}

card {
  name = "Runeclaw Bear", cost = "{1}{G}", type = "Creature — Bear",
  pt = { 2, 2 }, rarity = "Common", set = "M10", text = "",
}

card {
  name = "Elvish Visionary", cost = "{1}{G}", type = "Creature — Elf Shaman",
  pt = { 1, 1 }, rarity = "Common", set = "M13",
  text = "When Elvish Visionary enters the battlefield, draw a card.",
  abilities = {
    etb(draw(1), { text = "When Elvish Visionary enters the battlefield, draw a card." }),
  },
}

card {
  name = "Sylvan Ranger", cost = "{1}{G}", type = "Creature — Elf Scout",
  pt = { 1, 1 }, rarity = "Common", set = "M11",
  text = "When Sylvan Ranger enters the battlefield, you may search your library for a basic land card, reveal it, put it into your hand, then shuffle.",
  abilities = {
    etb(
      may(seq {
        search { count = 1, filter = BASIC_LAND, to_hand = true },
        shuffle(YOU),
      }),
      { text = "When Sylvan Ranger enters the battlefield, you may search your library for a basic land card, reveal it, put it into your hand, then shuffle." }
    ),
  },
}

card {
  name = "Ambush Viper", cost = "{1}{G}", type = "Creature — Snake",
  pt = { 2, 1 }, rarity = "Common", set = "ZEN",
  text = "Flash\nDeathtouch",
  keywords = { "Flash", "Deathtouch" },
}

card {
  name = "Thornweald Archer", cost = "{1}{G}", type = "Creature — Elf Archer",
  pt = { 2, 1 }, rarity = "Uncommon", set = "EVE",
  text = "Reach, deathtouch",
  keywords = { "Reach", "Deathtouch" },
}

card {
  name = "Wall of Blossoms", cost = "{1}{G}", type = "Creature — Plant Wall",
  pt = { 0, 4 }, rarity = "Uncommon", set = "STH",
  text = "Defender\nWhen Wall of Blossoms enters the battlefield, draw a card.",
  keywords = { "Defender" },
  abilities = {
    etb(draw(1), { text = "When Wall of Blossoms enters the battlefield, draw a card." }),
  },
}

card {
  name = "Garruk's Companion", cost = "{G}{G}", type = "Creature — Beast",
  pt = { 3, 1 }, rarity = "Uncommon", set = "M12",
  text = "Trample",
  keywords = { "Trample" },
}

card {
  name = "Kalonian Tusker", cost = "{G}{G}", type = "Creature — Beast",
  pt = { 3, 3 }, rarity = "Uncommon", set = "M14", text = "",
}

card {
  name = "Centaur Courser", cost = "{2}{G}", type = "Creature — Centaur Warrior",
  pt = { 3, 3 }, rarity = "Common", set = "M10", text = "",
}

card {
  name = "Giant Spider", cost = "{3}{G}", type = "Creature — Spider",
  pt = { 2, 4 }, rarity = "Common", set = "LEA",
  text = "Reach",
  keywords = { "Reach" },
}

card {
  name = "Leatherback Baloth", cost = "{G}{G}{G}", type = "Creature — Beast",
  pt = { 4, 5 }, rarity = "Uncommon", set = "WWK", text = "",
}

card {
  name = "Acidic Slime", cost = "{3}{G}{G}", type = "Creature — Ooze",
  pt = { 2, 2 }, rarity = "Uncommon", set = "M10",
  text = "Deathtouch\nWhen Acidic Slime enters the battlefield, destroy target artifact, enchantment, or land.",
  keywords = { "Deathtouch" },
  abilities = {
    etb(destroy(), {
      targets = { t_permanent("target artifact, enchantment, or land",
        f_or(ARTIFACT, ENCHANTMENT, LAND)) },
      text = "When Acidic Slime enters the battlefield, destroy target artifact, enchantment, or land.",
    }),
  },
}

card {
  name = "Craw Wurm", cost = "{4}{G}{G}", type = "Creature — Wurm",
  pt = { 6, 4 }, rarity = "Common", set = "LEA", text = "",
}

card {
  name = "Vastwood Gorger", cost = "{5}{G}", type = "Creature — Wurm",
  pt = { 4, 6 }, rarity = "Common", set = "ROE", text = "",
}

card {
  name = "Thragtusk", cost = "{4}{G}", type = "Creature — Beast",
  pt = { 5, 3 }, rarity = "Rare", set = "M13",
  text = "When Thragtusk enters the battlefield, you gain 5 life.\nWhen Thragtusk leaves the battlefield, create a 3/3 green Beast creature token.",
  abilities = {
    etb(gain_life(5, YOU), { text = "When Thragtusk enters the battlefield, you gain 5 life." }),
    -- Dispara já fora do campo, então precisa valer com a fonte no cemitério
    -- (CR 603.6d, gatilho de saída de zona).
    when_leaves(beast_token(1), {
      from_graveyard = true,
      text = "When Thragtusk leaves the battlefield, create a 3/3 green Beast creature token.",
    }),
  },
}

card {
  name = "Giant Growth", cost = "{G}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Target creature gets +3/+3 until end of turn.",
  targets = { t_creature() },
  effect = pump(3, 3),
}

card {
  name = "Titanic Growth", cost = "{1}{G}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Target creature gets +4/+4 until end of turn.",
  targets = { t_creature() },
  effect = pump(4, 4),
}

card {
  name = "Aggressive Urge", cost = "{1}{G}", type = "Instant",
  rarity = "Common", set = "INV",
  text = "Target creature gets +1/+1 until end of turn.\nDraw a card.",
  targets = { t_creature() },
  effect = seq { pump(1, 1), draw(1) },
}

card {
  name = "Prey Upon", cost = "{G}", type = "Sorcery",
  rarity = "Common", set = "ISD",
  text = "Target creature you control fights target creature you don't control.",
  targets = {
    t_creature("target creature you control", { owner = YOU }),
    t_creature("target creature you don't control", { owner = OPPONENTS }),
  },
  effect = fight(target(1), target(2)),
}

card {
  name = "Rabid Bite", cost = "{1}{G}", type = "Sorcery",
  rarity = "Common", set = "M20",
  text = "Target creature you control deals damage equal to its power to target creature you don't control.",
  targets = {
    t_creature("target creature you control", { owner = YOU }),
    t_creature("target creature you don't control", { owner = OPPONENTS }),
  },
  effect = deal_damage(power_of(target(1)), target(2)),
}

card {
  name = "Hunt the Weak", cost = "{3}{G}", type = "Sorcery",
  rarity = "Common", set = "M13",
  text = "Put a +1/+1 counter on target creature you control. Then that creature fights target creature you don't control.",
  targets = {
    t_creature("target creature you control", { owner = YOU }),
    t_creature("target creature you don't control", { owner = OPPONENTS }),
  },
  effect = seq {
    add_counters(1, "PlusOnePlusOne", target(1)),
    fight(target(1), target(2)),
  },
}

card {
  name = "Plummet", cost = "{1}{G}", type = "Instant",
  rarity = "Common", set = "M12",
  text = "Destroy target creature with flying.",
  targets = { t_creature("target creature with flying",
    { filter = f_and(CREATURE, has_keyword("Flying")) }) },
  effect = destroy(),
}

card {
  name = "Naturalize", cost = "{1}{G}", type = "Instant",
  rarity = "Common", set = "ONS",
  text = "Destroy target artifact or enchantment.",
  targets = { t_permanent("target artifact or enchantment", f_or(ARTIFACT, ENCHANTMENT)) },
  effect = destroy(),
}

card {
  name = "Lay of the Land", cost = "{G}", type = "Sorcery",
  rarity = "Common", set = "ODY",
  text = "Search your library for a basic land card, reveal it, put it into your hand, then shuffle.",
  effect = seq {
    search { count = 1, filter = BASIC_LAND, to_hand = true },
    shuffle(YOU),
  },
}

card {
  name = "Overrun", cost = "{2}{G}{G}{G}", type = "Sorcery",
  rarity = "Uncommon", set = "TMP",
  text = "Creatures you control get +3/+3 and gain trample until end of turn.",
  effect = pump(3, 3, { target = all(your_creatures()), keywords = { "Trample" } }),
}
