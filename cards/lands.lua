-- Terrenos.
--
-- Os cinco básicos vêm do helper `basic_land` do prelúdio. Os duplos são o
-- ciclo comum de Khans of Tarkir ("gain lands"): entram virados, dão 1 de vida
-- e produzem duas cores — tudo expressável, ao contrário de terreno com
-- "revele uma carta" ou busca condicional.

basic_land("Plains", "Plains", "{W}")
basic_land("Island", "Island", "{U}")
basic_land("Swamp", "Swamp", "{B}")
basic_land("Mountain", "Mountain", "{R}")
basic_land("Forest", "Forest", "{G}")

-- Um só molde para as dez variações: repetir dez vezes o mesmo bloco esconderia
-- o erro de digitação em um dos símbolos.
local function gain_land(name, a, b)
  return card {
    name = name,
    type = "Land",
    rarity = "Common",
    set = "KTK",
    text = name .. " enters the battlefield tapped.\nWhen " .. name ..
      " enters the battlefield, you gain 1 life.\n{T}: Add " .. a .. " or " .. b .. ".",
    abilities = {
      enters_tapped(),
      etb(gain_life(1, YOU),
        { text = "When " .. name .. " enters the battlefield, you gain 1 life." }),
      mana_ability {
        produces = { sym(a), sym(b) },
        text = "{T}: Add " .. a .. " or " .. b .. ".",
      },
    },
  }
end

gain_land("Tranquil Cove", "{W}", "{U}")
gain_land("Scoured Barrens", "{W}", "{B}")
gain_land("Wind-Scarred Crag", "{R}", "{W}")
gain_land("Blossoming Sands", "{G}", "{W}")
gain_land("Dismal Backwater", "{U}", "{B}")
gain_land("Swiftwater Cliffs", "{U}", "{R}")
gain_land("Thornwood Falls", "{G}", "{U}")
gain_land("Bloodfell Caves", "{B}", "{R}")
gain_land("Jungle Hollow", "{B}", "{G}")
gain_land("Rugged Highlands", "{R}", "{G}")

card {
  name = "Radiant Fountain", type = "Land",
  rarity = "Common", set = "ZEN",
  text = "When Radiant Fountain enters the battlefield, you gain 2 life.\n{T}: Add {C}.",
  abilities = {
    etb(gain_life(2, YOU),
      { text = "When Radiant Fountain enters the battlefield, you gain 2 life." }),
    mana_ability { produces = "{C}", text = "{T}: Add {C}." },
  },
}
