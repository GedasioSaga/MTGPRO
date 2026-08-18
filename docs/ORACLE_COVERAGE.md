# Cobertura do texto de oráculo

Gerado por `mtg-import sync --coverage`. Não editar à mão.

- Bulk: `oracle_cards`, `2026-08-18T09:01:59.163+00:00`
- Linhas lidas: 38626
- Descartadas na entrada: 5828
- No catálogo: 32798
- Jogáveis: 3831 (11.7%)
- Não jogáveis: 28967 (88.3%)
- Bloqueio textual: 18604 · bloqueio estrutural (layout, duas faces): 10363
- Padrões distintos: 10707 · subtipos de criatura no vocabulário: 285
- Tempo de importação: 2.7s
- SQLite gerado: 49.2 MB

## Como ler

`~` é o nome da própria carta, `N` é qualquer número, `<tipo>` é subtipo de criatura, `<cor>` é cor e `<terreno>` é tipo de terreno básico. A coluna **Cartas** é quantas cartas têm esse padrão como **primeiro** bloqueio — é o piso do que voltaria a compilar, não o teto, porque uma carta pode ter mais de um parágrafo travado.

## 50 padrões não suportados mais frequentes

| # | Cartas | Padrão normalizado | Exemplo |
|---|---|---|---|
| 1 | 164 | `look at the top N cards of your library` | Accumulate Wisdom |
| 2 | 130 | `devoid` | Abstruse Appropriation |
| 3 | 115 | `choose one —` | Abiding Grace |
| 4 | 102 | `creatures you control get +N/+N until end of turn` | Akroan Phalanx |
| 5 | 86 | `regenerate ~` | Ancient Silverback |
| 6 | 83 | `this spell can't be countered` | Abrupt Decay |
| 7 | 65 | `changeling` | Amoeboid Changeling |
| 8 | 64 | `add one mana of any color` | Abundant Countryside |
| 9 | 61 | `target opponent reveals their hand` | Aggressive Negotiations |
| 10 | 52 | `as an additional cost to cast this spell, sacrifice a creature` | Altar of Bone |
| 11 | 52 | `it deals N damage to any target` | Akoum Boulderfoot |
| 12 | 50 | `~ can't be blocked` | Azorius Herald |
| 13 | 49 | `pay {N}` | Ant-Man, Colony Commander |
| 14 | 49 | `reveal the top N cards of your library` | Adéwalé, Breaker of Chains |
| 15 | 48 | `as ~ enters, choose a creature type` | Adaptive Automaton |
| 16 | 47 | `~ attacks each combat if able` | Akoum Firebird |
| 17 | 46 | `choose one or both —` | Against All Odds |
| 18 | 46 | `return target creature card from your graveyard to your hand` | Along the Crooked Way |
| 19 | 46 | `~ enters with x +N/+N counters on it` | Benevolent Hydra |
| 20 | 43 | `enchant player` | Cruel Reality |
| 21 | 43 | `investigate` | Alquist Proft, Master Sleuth |
| 22 | 42 | `start your engines!` | Aether Syphon |
| 23 | 41 | `attach it to target creature you control` | Barbed Bloodletter |
| 24 | 41 | `you get {e}{e}` | Aether Chaser |
| 25 | 41 | `you may look at the top card of your library any time` | All-You-Can-Eat Buffet |
| 26 | 38 | `infect` | Blackcleave Goblin |
| 27 | 38 | `you become the monarch` | Aragorn, King of Gondor |
| 28 | 37 | `as ~ enters, choose a color` | Alloy Golem |
| 29 | 37 | `prevent the next N damage that would be dealt to any target this turn` | Abuna Acolyte |
| 30 | 37 | `search your library for a basic land card, put it onto the battlefield tapped, then shuffle` | Beneath the Sands |
| 31 | 36 | `affinity for artifacts` | Assert Authority |
| 32 | 36 | `return target creature card from your graveyard to the battlefield` | Apprentice Necromancer |
| 33 | 34 | `bushido N` | Araba Mothrider |
| 34 | 34 | `search your library for a basic land card, reveal it, put it into your hand, then shuffle` | Attune with Aether |
| 35 | 33 | `crew N` | Air Response Unit |
| 36 | 33 | `target creature can't block this turn` | Bola Warrior |
| 37 | 33 | `you may choose not to untap ~ during your untap step` | Amber Prison |
| 38 | 32 | `activate only once each turn` | Akki Avalanchers |
| 39 | 31 | `target creature gets +N/+N and gains first strike until end of turn` | Ancestors' Aid |
| 40 | 31 | `target creature gets +N/+N and gains trample until end of turn` | Awaken the Bear |
| 41 | 31 | `toxic N` | Annex Sentry |
| 42 | 29 | `choose one` | Aetheric Amplifier |
| 43 | 29 | `it can't be regenerated` | Afterlife |
| 44 | 28 | `all creatures get -N/-N until end of turn` | Biting Rain |
| 45 | 28 | `ascend` | Andúril, Narsil Reforged |
| 46 | 28 | `~ doesn't untap during your untap step` | Basalt Monolith |
| 47 | 27 | `flip a coin` | Boompile |
| 48 | 27 | `gain control of target creature until end of turn` | Act of Treason |
| 49 | 27 | `if you do, draw a card` | Academy Raider |
| 50 | 26 | `shadow` | Augur il-Vec |

## Por começo de frase (6 primeiras palavras, 6997 distintos)

O texto inteiro é específico demais para priorizar: quase toda carta tem a sua variação. Agrupando pelo começo da frase aparece o parser que se escreve **uma vez** e cobre a família inteira. É por esta tabela que se escolhe o próximo trabalho.

| # | Cartas | Padrão normalizado | Exemplo |
|---|---|---|---|
| 1 | 300 | `as an additional cost to cast …` | Abandon Hope |
| 2 | 205 | `look at the top N cards …` | Accumulate Wisdom |
| 3 | 192 | `this spell costs {N} less to …` | Academy Journeymage |
| 4 | 144 | `search your library for a basic …` | Aang's Journey |
| 5 | 130 | `devoid` | Abstruse Appropriation |
| 6 | 129 | `target creature gets +N/+N and gains …` | Acrobatic Leap |
| 7 | 115 | `choose one —` | Abiding Grace |
| 8 | 103 | `creatures you control get +N/+N until …` | Akroan Phalanx |
| 9 | 95 | `return target creature card from your …` | Along the Crooked Way |
| 10 | 94 | `~ gets +N/+N as long as …` | Angrath's Ambusher |
| 11 | 86 | `regenerate ~` | Ancient Silverback |
| 12 | 83 | `this spell can't be countered` | Abrupt Decay |
| 13 | 79 | `~ enters tapped unless you control …` | Abandoned Air Temple |
| 14 | 78 | `search your library for up to …` | Archaeomancer's Map |
| 15 | 73 | `it deals N damage to target …` | Abraded Bluffs |
| 16 | 65 | `changeling` | Amoeboid Changeling |
| 17 | 64 | `add one mana of any color` | Abundant Countryside |
| 18 | 64 | `prevent all combat damage that would …` | Angelsong |
| 19 | 63 | `until end of turn, target creature …` | Abnormal Endurance |
| 20 | 61 | `target opponent reveals their hand` | Aggressive Negotiations |
| 21 | 60 | `prevent the next N damage that …` | Abuna Acolyte |
| 22 | 60 | `you gain N life for each …` | Aerial Assault |
| 23 | 60 | `~ enters with x +N/+N counters …` | Academy Elite |
| 24 | 59 | `it deals N damage to any …` | Akoum Boulderfoot |
| 25 | 55 | `reveal the top N cards of …` | Adéwalé, Breaker of Chains |
| 26 | 54 | `equipped creature gets +N/+N and has …` | Aegis of the Legion |
| 27 | 51 | `prevent all damage that would be …` | Avacyn, Guardian Angel |
| 28 | 50 | `as ~ enters, choose a creature …` | Adaptive Automaton |
| 29 | 50 | `~ can't be blocked` | Azorius Herald |
| 30 | 49 | `pay {N}` | Ant-Man, Colony Commander |
| 31 | 48 | `create a token that's a copy …` | Applied Geometry |
| 32 | 48 | `if you control N or more …` | Blade-Tribe Berserkers |
| 33 | 47 | `~ attacks each combat if able` | Akoum Firebird |
| 34 | 46 | `choose one or both —` | Against All Odds |
| 35 | 44 | `search your library for a <tipo> …` | Amrou Scout |
| 36 | 44 | `~ enters with a +N/+N counter …` | Ascendant Acolyte |
| 37 | 43 | `enchant player` | Cruel Reality |
| 38 | 43 | `investigate` | Alquist Proft, Master Sleuth |
| 39 | 43 | `you may have ~ enter as …` | Activated Sleeper |
| 40 | 42 | `start your engines!` | Aether Syphon |
| 41 | 42 | `you may look at the top …` | All-You-Can-Eat Buffet |
| 42 | 41 | `attach it to target creature you …` | Barbed Bloodletter |
| 43 | 41 | `enchanted creature gets +N/+N and has …` | Agility |
| 44 | 41 | `you get {e}{e}` | Aether Chaser |
| 45 | 40 | `~ can't be blocked by creatures …` | Ant-Man, Elusive Avenger |
| 46 | 38 | `infect` | Blackcleave Goblin |
| 47 | 38 | `look at the top card of …` | Amareth, the Lustrous |
| 48 | 38 | `you become the monarch` | Aragorn, King of Gondor |
| 49 | 37 | `as ~ enters, choose a color` | Alloy Golem |
| 50 | 37 | `return this card from your graveyard …` | Bloodsoaked Champion |
