-- Preto: remoção incondicional, descarte, recursão de cemitério e criaturas
-- que trocam vida por vantagem.

card {
  name = "Typhoid Rats", cost = "{B}", type = "Creature — Rat",
  pt = { 1, 1 }, rarity = "Common", set = "M13",
  text = "Deathtouch",
  keywords = { "Deathtouch" },
}

card {
  name = "Walking Corpse", cost = "{1}{B}", type = "Creature — Zombie",
  pt = { 2, 2 }, rarity = "Common", set = "M10", text = "",
}

card {
  name = "Child of Night", cost = "{1}{B}", type = "Creature — Vampire",
  pt = { 2, 1 }, rarity = "Common", set = "M10",
  text = "Lifelink",
  keywords = { "Lifelink" },
}

card {
  name = "Festering Goblin", cost = "{B}", type = "Creature — Zombie Goblin",
  pt = { 1, 1 }, rarity = "Common", set = "ONS",
  text = "When Festering Goblin dies, target creature gets -1/-1 until end of turn.",
  abilities = {
    dies(pump(-1, -1), {
      targets = { t_creature() },
      text = "When Festering Goblin dies, target creature gets -1/-1 until end of turn.",
    }),
  },
}

card {
  name = "Dusk Legion Zealot", cost = "{1}{B}", type = "Creature — Vampire Soldier",
  pt = { 1, 1 }, rarity = "Common", set = "RIX",
  text = "When Dusk Legion Zealot enters the battlefield, you draw a card and you lose 1 life.",
  abilities = {
    etb(seq { draw(1), lose_life(1, YOU) },
      { text = "When Dusk Legion Zealot enters the battlefield, you draw a card and you lose 1 life." }),
  },
}

card {
  name = "Ravenous Rats", cost = "{1}{B}", type = "Creature — Rat",
  pt = { 1, 1 }, rarity = "Common", set = "M11",
  text = "When Ravenous Rats enters the battlefield, target opponent discards a card.",
  abilities = {
    etb(discard(1, target_player(1)), {
      targets = { t_player("target opponent", OPPONENTS) },
      text = "When Ravenous Rats enters the battlefield, target opponent discards a card.",
    }),
  },
}

card {
  name = "Phyrexian Rager", cost = "{2}{B}", type = "Creature — Horror",
  pt = { 2, 2 }, rarity = "Common", set = "APC",
  text = "When Phyrexian Rager enters the battlefield, you draw a card and you lose 1 life.",
  abilities = {
    etb(seq { draw(1), lose_life(1, YOU) },
      { text = "When Phyrexian Rager enters the battlefield, you draw a card and you lose 1 life." }),
  },
}

card {
  name = "Vampire Nighthawk", cost = "{1}{B}{B}", type = "Creature — Vampire Shaman",
  pt = { 2, 3 }, rarity = "Uncommon", set = "ZEN",
  text = "Flying, deathtouch, lifelink",
  keywords = { "Flying", "Deathtouch", "Lifelink" },
}

card {
  name = "Bloodhunter Bat", cost = "{3}{B}", type = "Creature — Bat",
  pt = { 2, 2 }, rarity = "Uncommon", set = "M12",
  text = "Flying\nWhen Bloodhunter Bat enters the battlefield, target opponent loses 2 life and you gain 2 life.",
  keywords = { "Flying" },
  abilities = {
    etb(seq { lose_life(2, target_player(1)), gain_life(2, YOU) }, {
      targets = { t_player("target opponent", OPPONENTS) },
      text = "When Bloodhunter Bat enters the battlefield, target opponent loses 2 life and you gain 2 life.",
    }),
  },
}

card {
  name = "Gravedigger", cost = "{3}{B}", type = "Creature — Zombie",
  pt = { 2, 2 }, rarity = "Uncommon", set = "TMP",
  text = "When Gravedigger enters the battlefield, you may return target creature card from your graveyard to your hand.",
  abilities = {
    etb(may(bounce(target(1))), {
      targets = { t_in_graveyard("target creature card in your graveyard", CREATURE, YOU) },
      text = "When Gravedigger enters the battlefield, you may return target creature card from your graveyard to your hand.",
    }),
  },
}

card {
  name = "Nekrataal", cost = "{2}{B}{B}", type = "Creature — Human Assassin",
  pt = { 2, 1 }, rarity = "Uncommon", set = "VIS",
  text = "First strike\nWhen Nekrataal enters the battlefield, destroy target nonartifact, nonblack creature. That creature can't be regenerated.",
  keywords = { "FirstStrike" },
  abilities = {
    etb(destroy(target(1), true), {
      targets = { t_creature("target nonartifact, nonblack creature",
        { filter = f_and(CREATURE, f_not(ARTIFACT), f_not(has_color("Black"))) }) },
      text = "When Nekrataal enters the battlefield, destroy target nonartifact, nonblack creature. That creature can't be regenerated.",
    }),
  },
}

card {
  name = "Royal Assassin", cost = "{1}{B}{B}", type = "Creature — Human Assassin",
  pt = { 1, 1 }, rarity = "Rare", set = "LEA",
  text = "{T}: Destroy target tapped creature.",
  abilities = {
    activated {
      tap = true,
      targets = { t_creature("target tapped creature", { filter = f_and(CREATURE, TAPPED) }) },
      effect = destroy(),
      text = "{T}: Destroy target tapped creature.",
    },
  },
}

card {
  name = "Nightmare", cost = "{5}{B}", type = "Creature — Nightmare Horse",
  pt = { 0, 0 }, rarity = "Rare", set = "LEA",
  text = "Flying\nNightmare's power and toughness are each equal to the number of Swamps you control.",
  keywords = { "Flying" },
  abilities = {
    -- Característica definidora: camada 7b (SetPT), recalculada sempre — por
    -- isso é StaticMod e não um efeito de resolução (CR 613.4c).
    static_set_pt(
      count_of(sel { filter = has_subtype("Swamp"), owner = YOU }),
      count_of(sel { filter = has_subtype("Swamp"), owner = YOU }),
      sel { filter = IS_SELF },
      "Nightmare's power and toughness are each equal to the number of Swamps you control."
    ),
  },
}

card {
  name = "Doom Blade", cost = "{1}{B}", type = "Instant",
  rarity = "Common", set = "M10",
  text = "Destroy target nonblack creature.",
  targets = { t_creature("target nonblack creature",
    { filter = f_and(CREATURE, f_not(has_color("Black"))) }) },
  effect = destroy(),
}

card {
  name = "Murder", cost = "{1}{B}{B}", type = "Instant",
  rarity = "Common", set = "M13",
  text = "Destroy target creature.",
  targets = { t_creature() },
  effect = destroy(),
}

card {
  name = "Assassinate", cost = "{2}{B}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Destroy target tapped creature.",
  targets = { t_creature("target tapped creature", { filter = f_and(CREATURE, TAPPED) }) },
  effect = destroy(),
}

card {
  name = "Diabolic Edict", cost = "{1}{B}", type = "Instant",
  rarity = "Common", set = "TMP",
  text = "Target player sacrifices a creature.",
  targets = { t_player() },
  effect = sacrifice(1, target_player(1), CREATURE),
}

card {
  name = "Corrupt", cost = "{5}{B}", type = "Sorcery",
  rarity = "Uncommon", set = "M10",
  text = "Corrupt deals damage equal to the number of Swamps you control to any target. You gain that much life.",
  targets = { t_any() },
  effect = seq {
    deal_damage(count_of(sel { filter = has_subtype("Swamp"), owner = YOU })),
    gain_life(count_of(sel { filter = has_subtype("Swamp"), owner = YOU }), YOU),
  },
}

card {
  name = "Duress", cost = "{B}", type = "Sorcery",
  rarity = "Common", set = "USG",
  text = "Target opponent reveals their hand. You choose a noncreature, nonland card from it. That player discards that card.",
  targets = { t_player("target opponent", OPPONENTS) },
  effect = discard(1, target_player(1), { filter = f_and(f_not(CREATURE), f_not(LAND)) }),
}

card {
  name = "Mind Rot", cost = "{2}{B}", type = "Sorcery",
  rarity = "Common", set = "LEA",
  text = "Target player discards two cards.",
  targets = { t_player() },
  effect = discard(2, target_player(1)),
}

card {
  name = "Sign in Blood", cost = "{B}{B}", type = "Sorcery",
  rarity = "Common", set = "M10",
  text = "Target player draws two cards and loses 2 life.",
  targets = { t_player() },
  effect = seq { draw(2, target_player(1)), lose_life(2, target_player(1)) },
}

card {
  name = "Read the Bones", cost = "{2}{B}", type = "Sorcery",
  rarity = "Common", set = "THS",
  text = "Scry 2, then draw two cards. You lose 2 life.",
  effect = seq { scry(2, YOU), draw(2), lose_life(2, YOU) },
}

card {
  name = "Dark Ritual", cost = "{B}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Add {B}{B}{B}.",
  effect = add_mana("{B}{B}{B}", YOU),
}

card {
  name = "Raise Dead", cost = "{B}", type = "Sorcery",
  rarity = "Common", set = "LEA",
  text = "Return target creature card from your graveyard to your hand.",
  targets = { t_in_graveyard("target creature card in your graveyard", CREATURE, YOU) },
  effect = bounce(),
}

card {
  name = "Zombify", cost = "{3}{B}", type = "Sorcery",
  rarity = "Uncommon", set = "ODY",
  text = "Return target creature card from your graveyard to the battlefield.",
  targets = { t_in_graveyard("target creature card in your graveyard", CREATURE, YOU) },
  effect = reanimate(),
}

card {
  name = "Rise from the Grave", cost = "{4}{B}", type = "Sorcery",
  rarity = "Uncommon", set = "M10",
  text = "Put target creature card from a graveyard onto the battlefield under your control. That creature is a black Zombie in addition to its other colors and types.",
  targets = { t_object("target creature card in a graveyard",
    sel { zone = "Graveyard", filter = CREATURE }) },
  effect = reanimate(),
}

card {
  name = "Nausea", cost = "{1}{B}", type = "Sorcery",
  rarity = "Common", set = "M11",
  text = "All creatures get -1/-1 until end of turn.",
  effect = pump(-1, -1, { target = all(creatures()) }),
}

card {
  name = "Bad Moon", cost = "{1}{B}", type = "Enchantment",
  rarity = "Rare", set = "LEA",
  text = "Black creatures get +1/+1.",
  abilities = {
    static_pt(1, 1,
      creatures { filter = f_and(CREATURE, has_color("Black")) },
      "Black creatures get +1/+1."),
  },
}
