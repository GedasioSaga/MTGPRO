-- Criaturas lendárias.
--
-- Arquivo novo, e não uma adição aos arquivos de cor, por dois motivos: as
-- lendas daqui são bicolores (não caberiam em `white.lua` nem em `green.lua`
-- sem quebrar a regra de que arquivo de cor só tem símbolo daquela cor), e
-- porque a razão de existirem é uma só — CR 903.3 exige uma criatura lendária
-- para ser comandante, e o catálogo curado não tinha nenhuma.
--
-- Mesmo critério dos outros arquivos: só carta cujo texto impresso cabe
-- inteiro no DSL. Nada foi simplificado para caber.

card {
  name = "Emmara, Soul of the Accord", cost = "{1}{G}{W}",
  type = "Legendary Creature — Elf Cleric",
  pt = { 2, 2 }, rarity = "Rare", set = "GRN",
  text = "Whenever Emmara, Soul of the Accord becomes tapped, create a 1/1 white Soldier creature token with lifelink.",
  abilities = {
    trigger({ Taps = sel { filter = IS_SELF } },
      token {
        name = "Soldier", type = "Creature — Soldier", pt = { 1, 1 },
        colors = { "White" }, keywords = { "Lifelink" }, count = 1,
      },
      { text = "Whenever Emmara, Soul of the Accord becomes tapped, create a 1/1 white Soldier creature token with lifelink." }),
  },
}

card {
  name = "Adeliz, the Cinder Wind", cost = "{1}{U}{R}",
  type = "Legendary Creature — Human Wizard",
  pt = { 2, 2 }, rarity = "Uncommon", set = "DOM",
  text = "Flying, haste\nWhenever you cast an instant or sorcery spell, Wizards you control get +1/+1 until end of turn.",
  keywords = { "Flying", "Haste" },
  abilities = {
    -- `on_cast` não distingue quem lançou; aqui a mágica tem de ser sua, então
    -- o gatilho é escrito à mão com `owner = YOU`.
    trigger({ SpellCast = sel {
        zone = "Stack",
        filter = f_or(has_type("Instant"), has_type("Sorcery")),
        owner = YOU,
      } },
      pump(1, 1, { target = all(creatures {
        filter = f_and(CREATURE, has_subtype("Wizard")),
        owner = YOU,
      }) }),
      { text = "Whenever you cast an instant or sorcery spell, Wizards you control get +1/+1 until end of turn." }),
  },
}
