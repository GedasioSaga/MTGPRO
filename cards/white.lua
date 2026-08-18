-- Branco: criaturas pequenas e eficientes, ganho de vida, remoção condicional
-- e efeitos de "todas as criaturas". Só cartas de core set clássicas cujo texto
-- cabe inteiro no DSL — carta que precisaria de sintaxe inventada ficou fora.

card {
  name = "Elite Vanguard", cost = "{W}", type = "Creature — Human Soldier",
  pt = { 2, 1 }, rarity = "Common", set = "M10", text = "",
}

card {
  name = "Savannah Lions", cost = "{W}", type = "Creature — Cat",
  pt = { 2, 1 }, rarity = "Rare", set = "LEA", text = "",
}

card {
  name = "Suntail Hawk", cost = "{W}", type = "Creature — Bird",
  pt = { 1, 1 }, rarity = "Common", set = "ODY",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Healer's Hawk", cost = "{W}", type = "Creature — Bird",
  pt = { 1, 1 }, rarity = "Common", set = "GRN",
  text = "Flying, lifelink",
  keywords = { "Flying", "Lifelink" },
}

card {
  name = "Soul Warden", cost = "{W}", type = "Creature — Human Cleric",
  pt = { 1, 1 }, rarity = "Common", set = "EXO",
  text = "Whenever another creature enters the battlefield, you gain 1 life.",
  abilities = {
    when_other_enters(CREATURE, gain_life(1, YOU),
      { text = "Whenever another creature enters the battlefield, you gain 1 life." }),
  },
}

card {
  name = "Gideon's Lawkeeper", cost = "{W}", type = "Creature — Human Soldier",
  pt = { 1, 1 }, rarity = "Common", set = "ORI",
  text = "{W}, {T}: Tap target creature.",
  abilities = {
    activated {
      cost = "{W}", tap = true,
      targets = { t_creature() },
      effect = tap(),
      text = "{W}, {T}: Tap target creature.",
    },
  },
}

card {
  name = "Youthful Knight", cost = "{1}{W}", type = "Creature — Human Knight",
  pt = { 2, 1 }, rarity = "Common", set = "7ED",
  text = "First strike",
  keywords = { "FirstStrike" },
}

card {
  name = "Leonin Skyhunter", cost = "{W}{W}", type = "Creature — Cat Knight",
  pt = { 2, 2 }, rarity = "Uncommon", set = "MRD",
  text = "Flying",
  keywords = { "Flying" },
}

card {
  name = "Fencing Ace", cost = "{1}{W}", type = "Creature — Human Soldier",
  pt = { 1, 1 }, rarity = "Uncommon", set = "M13",
  text = "Double strike",
  keywords = { "DoubleStrike" },
}

card {
  name = "Veteran Armorer", cost = "{1}{W}", type = "Creature — Human Soldier",
  pt = { 2, 2 }, rarity = "Common", set = "M10",
  text = "Other creatures you control get +0/+1.",
  abilities = {
    static_pt(0, 1,
      creatures { filter = f_and(CREATURE, IS_OTHER), owner = YOU },
      "Other creatures you control get +0/+1."),
  },
}

card {
  name = "Angelic Wall", cost = "{1}{W}", type = "Creature — Wall",
  pt = { 0, 4 }, rarity = "Common", set = "8ED",
  text = "Defender, flying",
  keywords = { "Defender", "Flying" },
}

card {
  name = "Wall of Omens", cost = "{1}{W}", type = "Creature — Wall",
  pt = { 0, 4 }, rarity = "Uncommon", set = "ROE",
  text = "Defender\nWhen Wall of Omens enters the battlefield, draw a card.",
  keywords = { "Defender" },
  abilities = {
    etb(draw(1), { text = "When Wall of Omens enters the battlefield, draw a card." }),
  },
}

card {
  name = "Ajani's Pridemate", cost = "{1}{W}", type = "Creature — Cat Soldier",
  pt = { 2, 2 }, rarity = "Uncommon", set = "M11",
  text = "Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate.",
  abilities = {
    -- CR 603.2: dispara uma vez por evento de ganho de vida, não por ponto.
    when_you_gain_life(add_counters(1, "PlusOnePlusOne", SELF),
      { text = "Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate." }),
  },
}

card {
  name = "Squadron Hawk", cost = "{1}{W}", type = "Creature — Bird",
  pt = { 1, 1 }, rarity = "Common", set = "M11",
  text = "Flying\nWhen Squadron Hawk enters the battlefield, you may search your library for up to three cards named Squadron Hawk, reveal them, put them into your hand, then shuffle.",
  keywords = { "Flying" },
  abilities = {
    etb(
      may(seq {
        search { count = 3, filter = named("Squadron Hawk"), to_hand = true },
        shuffle(YOU),
      }),
      { text = "When Squadron Hawk enters the battlefield, you may search your library for up to three cards named Squadron Hawk, reveal them, put them into your hand, then shuffle." }
    ),
  },
}

card {
  name = "Attended Knight", cost = "{3}{W}", type = "Creature — Human Knight",
  pt = { 2, 2 }, rarity = "Common", set = "M13",
  text = "First strike\nWhen Attended Knight enters the battlefield, create a 1/1 white Soldier creature token.",
  keywords = { "FirstStrike" },
  abilities = {
    etb(soldier_token(1),
      { text = "When Attended Knight enters the battlefield, create a 1/1 white Soldier creature token." }),
  },
}

card {
  name = "Serra Angel", cost = "{3}{W}{W}", type = "Creature — Angel",
  pt = { 4, 4 }, rarity = "Uncommon", set = "LEA",
  text = "Flying, vigilance",
  keywords = { "Flying", "Vigilance" },
}

card {
  name = "Angel of Mercy", cost = "{4}{W}", type = "Creature — Angel",
  pt = { 3, 3 }, rarity = "Uncommon", set = "INV",
  text = "Flying\nWhen Angel of Mercy enters the battlefield, you gain 3 life.",
  keywords = { "Flying" },
  abilities = {
    etb(gain_life(3, YOU),
      { text = "When Angel of Mercy enters the battlefield, you gain 3 life." }),
  },
}

card {
  name = "Captain of the Watch", cost = "{4}{W}{W}", type = "Creature — Human Soldier",
  pt = { 3, 3 }, rarity = "Rare", set = "M10",
  text = "Vigilance\nOther Soldier creatures you control get +1/+1 and have vigilance.\nWhen Captain of the Watch enters the battlefield, create three 1/1 white Soldier creature tokens.",
  keywords = { "Vigilance" },
  abilities = {
    static_pt(1, 1,
      creatures { filter = f_and(CREATURE, has_subtype("Soldier"), IS_OTHER), owner = YOU },
      "Other Soldier creatures you control get +1/+1."),
    static_grant({ "Vigilance" },
      creatures { filter = f_and(CREATURE, has_subtype("Soldier"), IS_OTHER), owner = YOU },
      "Other Soldier creatures you control have vigilance."),
    etb(soldier_token(3),
      { text = "When Captain of the Watch enters the battlefield, create three 1/1 white Soldier creature tokens." }),
  },
}

card {
  name = "Swords to Plowshares", cost = "{W}", type = "Instant",
  rarity = "Uncommon", set = "LEA",
  text = "Exile target creature. Its controller gains life equal to its power.",
  targets = { t_creature() },
  -- O ganho de vida lê o poder ANTES do exílio: fora do campo o objeto não tem
  -- mais características para consultar (CR 608.2h).
  effect = seq {
    gain_life(power_of(target(1)), controller_of(target(1))),
    exile(target(1)),
  },
}

card {
  name = "Disenchant", cost = "{1}{W}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Destroy target artifact or enchantment.",
  targets = { t_permanent("target artifact or enchantment", f_or(ARTIFACT, ENCHANTMENT)) },
  effect = destroy(),
}

card {
  name = "Divine Verdict", cost = "{3}{W}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Destroy target attacking or blocking creature.",
  targets = { t_creature("target attacking or blocking creature",
    { filter = f_and(CREATURE, f_or(ATTACKING, BLOCKING)) }) },
  effect = destroy(),
}

card {
  name = "Sunlance", cost = "{W}", type = "Sorcery",
  rarity = "Common", set = "PLC",
  text = "Sunlance deals 3 damage to target creature without flying.",
  targets = { t_creature("target creature without flying",
    { filter = f_and(CREATURE, f_not(has_keyword("Flying"))) }) },
  effect = deal_damage(3),
}

card {
  name = "Raise the Alarm", cost = "{1}{W}", type = "Instant",
  rarity = "Common", set = "M11",
  text = "Create two 1/1 white Soldier creature tokens.",
  effect = soldier_token(2),
}

card {
  name = "Mighty Leap", cost = "{2}{W}", type = "Instant",
  rarity = "Common", set = "M12",
  text = "Target creature gets +2/+2 and gains flying until end of turn.",
  targets = { t_creature() },
  effect = pump(2, 2, { keywords = { "Flying" } }),
}

card {
  name = "Oblivion Ring", cost = "{2}{W}", type = "Enchantment",
  rarity = "Uncommon", set = "LRW",
  text = "When Oblivion Ring enters the battlefield, exile another target nonland permanent.\nWhen Oblivion Ring leaves the battlefield, return the exiled card to the battlefield under its owner's control.",
  abilities = {
    -- `until_source_leaves` embute o segundo gatilho: o exílio é desfeito
    -- quando o anel sai do campo (CR 610.3).
    etb(exile(target(1), true), {
      targets = { t_permanent("another target nonland permanent", f_and(f_not(LAND), IS_OTHER)) },
      text = "When Oblivion Ring enters the battlefield, exile another target nonland permanent.",
    }),
  },
}

card {
  name = "Glorious Anthem", cost = "{1}{W}{W}", type = "Enchantment",
  rarity = "Uncommon", set = "TMP",
  text = "Creatures you control get +1/+1.",
  abilities = { static_pt(1, 1, your_creatures(), "Creatures you control get +1/+1.") },
}

card {
  name = "Honor of the Pure", cost = "{1}{W}", type = "Enchantment",
  rarity = "Rare", set = "M10",
  text = "White creatures you control get +1/+1.",
  abilities = {
    static_pt(1, 1,
      creatures { filter = f_and(CREATURE, has_color("White")), owner = YOU },
      "White creatures you control get +1/+1."),
  },
}

card {
  name = "Day of Judgment", cost = "{2}{W}{W}", type = "Sorcery",
  rarity = "Rare", set = "ZEN",
  text = "Destroy all creatures.",
  effect = destroy(all(creatures())),
}
