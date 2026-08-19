# Cobertura do texto de oráculo

Gerado por `mtg-import sync --coverage`. Não editar à mão.

- Bulk: `oracle_cards`, `2026-08-18T09:01:59.163+00:00`
- Linhas lidas: 38626
- Descartadas na entrada: 6170
- No catálogo: 32456
- Jogáveis: 3831 (11.8%)
- Não jogáveis: 28625 (88.2%)
- Travadas com trecho de texto: 18843 · sem trecho, só com motivo: 9782
- Tempo de importação: 6.4s
- SQLite gerado: 83.1 MB

## Cobertura por pool

A porcentagem crua do catálogo mede mal: ninguém monta deck com o catálogo inteiro. O que decide se dá para jogar é quanto de um **pool real** está coberto — e um pool pequeno e coerente como Pauper pode chegar perto de 100% enquanto o catálogo inteiro anda a 12%. Banida não conta como legal: ela inflaria o denominador sem mexer no numerador.

| Pool | No catálogo | Jogáveis | % jogável |
|---|---|---|---|
| Catálogo inteiro | 32456 | 3831 | 11.8% |
| Pauper | 10389 | 2565 | 24.7% |
| Standard | 4885 | 429 | 8.8% |
| Modern | 22365 | 2968 | 13.3% |
| Comuns e incomuns | 20208 | 3406 | 16.9% |

## O que implementar agora

Agrupado por **capacidade faltante**, não por frase literal: a frase literal espalha o trabalho por milhares de linhas de contagem pequena, e a capacidade junta o que se implementa de uma vez só. As colunas de formato existem porque destravar 200 cartas de Un-set vale menos que destravar 80 de Pauper.

A carta é contada na capacidade do seu **primeiro** bloqueio. É o piso do que voltaria a compilar, não o teto: uma carta pode ter mais de um parágrafo travado, e só some da lista quando o último cair.

### Falta padrão no parser — 19 capacidades, sem tocar em `mtg-core`

O vocabulário do IR **já tem** a construção; falta o compilador reconhecer o texto. A coluna "Construção do IR" nomeia o que resolve, para a afirmação poder ser conferida em `ir.rs` antes de alguém começar.

| # | Capacidade | Cartas | Pauper | Standard | Modern | Construção do IR que resolve | Exemplo |
|---|---|---|---|---|---|---|---|
| 1 | Substantivo, objeto ou sintagma de alvo não reconhecido | 3504 | 1217 | 520 | 2493 | 'Selector' e 'Filter' já descrevem estes alvos; falta o parser mapear a palavra | "Brims" Barone, Midway Mobster |
| 2 | Condição de gatilho não reconhecida | 3452 | 592 | 584 | 2192 | 'TriggerCondition' já tem as variantes de gatilho; falta o parser casar a frase | A Girl and Her Dogs |
| 3 | P/T que depende do estado do jogo | 888 | 252 | 98 | 581 | 'StaticAbility { condition, modification: StaticMod::ModifyPT(Value, Value) }' com 'Value::Count' | Aang, A Lot to Learn |
| 4 | Quantidade não reconhecida | 635 | 234 | 113 | 456 | 'Value::Fixed' e 'Value::Count' já expressam a quantidade; falta o parser lê-la | Abolish |
| 5 | Redução e aumento de custo de lançamento | 539 | 137 | 122 | 393 | 'StaticAbility' com 'StaticMod::CostReduction' / 'StaticMod::CostIncrease' (aplicadas em 'engine/cast.rs') | Academy Journeymage |
| 6 | Devolver do cemitério para a mão ou para o campo | 536 | 143 | 92 | 407 | 'Effect::ReturnToHand' e 'Effect::ReturnFromGraveyardToBattlefield' | Abstergo Entertainment |
| 7 | Pôr e tirar marcadores | 502 | 118 | 126 | 358 | 'Effect::AddCounters' e 'Effect::RemoveCounters' | Abomination, Terrifying Titan |
| 8 | Causar dano | 388 | 157 | 60 | 303 | 'Effect::DealDamage', 'Effect::DealDamageToPlayer', 'Effect::DivideDamage' | Abraded Bluffs |
| 9 | Pump em alvo somado a palavra-chave, até o fim do turno | 311 | 208 | 68 | 261 | 'Effect::Sequence([Effect::ModifyPT, Effect::GrantKeywords])' com 'Duration::EndOfTurn' | Abnormal Endurance |
| 10 | Sacrificar e descartar | 308 | 71 | 39 | 190 | 'Effect::Sacrifice' e 'Effect::Discard' | Abomination of Gudul |
| 11 | Modo: escolha uma ou mais opções | 282 | 53 | 70 | 200 | 'Effect::Modal { choose, options }' | Abiding Grace |
| 12 | Pump ou debuff de massa | 248 | 107 | 39 | 195 | 'Effect::ModifyPT { target: ObjRef::All(Selector), … }' — 'ObjRef::All' já é resolvido em 'engine/query.rs' | Aardwolf's Advantage |
| 13 | Buscar na biblioteca e pôr na mão | 240 | 70 | 41 | 195 | 'Effect::SearchLibrary { to_hand: true }' seguido de 'Effect::ShuffleLibrary' | Abzan Monument |
| 14 | Não pode bloquear nem atacar até o fim do turno | 211 | 116 | 30 | 169 | 'Effect::CantBeBlocked' e 'Effect::CantAttackOrBlock', com 'Duration::EndOfTurn' | Abandon the Post |
| 15 | Mana de qualquer cor | 167 | 40 | 42 | 116 | 'ManaProduction::AnyColor' e 'Effect::AddManaAnyColor' | A Realm Reborn |
| 16 | Exilar | 118 | 22 | 17 | 92 | 'Effect::Exile { until_source_leaves }' | Agate Assault |
| 17 | Scry, surveil, moer | 113 | 31 | 33 | 89 | 'Effect::Scry', 'Effect::Surveil', 'Effect::Mill' | Ashiok, Sculptor of Fears |
| 18 | Ganhar controle de permanente | 87 | 18 | 11 | 69 | 'Effect::GainControl { duration }' | Act of Aggression |
| 19 | Virar e desvirar | 62 | 16 | 4 | 43 | 'Effect::Tap', 'Effect::Untap', 'Effect::Freeze' | Aboshan, Cephalid Emperor |

### Falta capacidade no IR — 27 capacidades, exige `mtg-core`

Não sai sem vocabulário novo no motor. São os **pedidos de IR**: enquanto não existirem, estas cartas continuam `Unsupported` de propósito — marcar como jogável algo que o motor não sabe executar quebra a partida em silêncio, que é bem pior que carta ausente.

| # | Capacidade | Cartas | Pauper | Standard | Modern | O que falta em `mtg-core` | Exemplo |
|---|---|---|---|---|---|---|---|
| 1 | Carta de mais de uma face (split, adventure, transform, MDFC) | 882 | 128 | 242 | 764 | 'CardDef' representa uma face só: falta modelo de face e de troca de face | Aang, Swift Savior // Aang and La, Ocean's Fury |
| 2 | Aura e equipamento: efeito estático sobre o que está anexado | 753 | 311 | 68 | 528 | falta 'Filter::Attached' — 'StaticAbility.affects' é um 'Selector', e nenhum filtro descreve "o que isto encanta" | Abundant Growth |
| 3 | Palavra-chave sem variante em 'ir::Keyword' | 589 | 249 | 33 | 461 | variante nova em 'ir::Keyword' (devoid, changeling, infect, shadow, bushido, toxic, crew, …) | Abstruse Appropriation |
| 4 | Entra virado, entra com marcadores, substituição de entrada | 563 | 114 | 115 | 411 | 'ReplacementAbility' existe no IR mas o motor não lê nenhuma: zero ocorrências de 'Ability::Replacement' em 'engine/' | Abandoned Air Temple |
| 5 | Olhar o topo da biblioteca e escolher | 376 | 108 | 79 | 295 | falta efeito de olhar-e-escolher: 'Effect::Scry' decide topo ou fundo, não põe carta na mão | Acclaimed Contender |
| 6 | Buscar na biblioteca e pôr direto no campo | 369 | 103 | 59 | 258 | 'Effect::SearchLibrary' só tem 'to_hand: bool' — falta destino (campo, topo, cemitério) e estado virado | Aang's Journey |
| 7 | Ficha com corpo que 'TokenSpec' não descreve | 356 | 108 | 115 | 260 | 'TokenSpec' é ficha literal com 'keywords': sem 'abilities', Treasure e Food não cabem | A Killer Among Us |
| 8 | Custo adicional de lançamento | 304 | 121 | 75 | 230 | 'CardDef' não tem onde pendurar custo de lançamento: 'Cost' existe, o campo não | Abandon Hope |
| 9 | Prevenir dano (parcial, de combate, do próximo) | 271 | 135 | 3 | 148 | só existe 'StaticMod::PreventAllDamage', contínua e total — falta escudo com quantidade e prevenção de uma vez só | Abuna Acolyte |
| 10 | Revelar cartas (mão, topo da biblioteca) | 269 | 61 | 31 | 199 | falta 'Effect::Reveal' — revelar é informação pública, e nenhum efeito a produz | Abundance |
| 11 | P/T ou lealdade que depende do estado do jogo ('*') | 245 | 17 | 28 | 148 | 'CardDef.power' é um número: falta P/T característico, que se recalcula sozinho | Abominable Treefolk |
| 12 | Escolher tipo ou cor ao entrar no campo | 236 | 22 | 29 | 120 | escolha registrada no objeto: 'Value::ChosenNumber' cobre número, não tipo nem cor | Adaptive Automaton |
| 13 | Regenerar | 205 | 97 | 0 | 118 | falta escudo de regeneração — 'Effect::Destroy { no_regeneration }' só sabe ignorá-lo | Accursed Duneyard |
| 14 | Contramágica condicional ("a menos que pague") | 189 | 96 | 21 | 128 | 'Effect::CounterSpell' é incondicional: falta o ramo "a menos que" com custo | Annul |
| 15 | Ficha com habilidade ativada (Clue, Treasure, Food, …) | 168 | 38 | 42 | 90 | 'TokenSpec' tem 'keywords', não 'abilities' — ficha com "{2}, sacrifique: compre" não é representável | Academy Manufactor |
| 16 | Não pode ser bloqueado (estático, permanente) | 147 | 61 | 19 | 101 | falta 'StaticMod::CantBeBlocked' — a variante existe em 'StaticModRuntime', não na de autoria | Amrou Seekers |
| 17 | Tipo ou linha de tipo fora do modelo de carta | 122 | 0 | 0 | 0 | 'CardType' não tem o tipo (Stickers, Attraction, …), então a linha inteira não resolve | 1996 World Champion |
| 18 | Não desvira no passo de desvirar | 105 | 27 | 2 | 55 | falta 'StaticMod::DoesNotUntap' — existe em 'StaticModRuntime', não na de autoria | Ajani Vengeant |
| 19 | Modo com alvo próprio por opção | 104 | 58 | 28 | 87 | 'Effect::Modal' guarda efeitos, não 'TargetSpec' por modo — o alvo é da mágica inteira | Abrade |
| 20 | Designações: monarca, iniciativa, bênção da cidade | 102 | 18 | 0 | 24 | estado de jogador designado — 'GameState' não tem monarca nem iniciativa | Aarakocra Sneak |
| 21 | Contadores de energia {E} | 97 | 33 | 0 | 75 | reserva de energia por jogador — 'CounterKind' é de permanente, não de jogador | Aether Chaser |
| 22 | Não pode ser contraespelado | 96 | 4 | 18 | 83 | falta propriedade de objeto na pilha — 'Effect::CounterSpell' não consulta nada | Abrupt Decay |
| 23 | Ficha que é cópia de outro permanente | 89 | 0 | 23 | 58 | 'TokenSpec' é ficha literal — não há como copiar um objeto do jogo | Applied Geometry |
| 24 | Cara ou coroa, dados | 88 | 15 | 1 | 36 | efeito de sorteio com resultado ramificado — nenhum 'Effect' produz aleatório observável | Aberrant Mind Sorcerer |
| 25 | Ataca ou bloqueia se puder, precisa ser bloqueado | 78 | 31 | 8 | 61 | falta 'StaticMod::MustAttack' / 'MustBlock' — só existem 'CantAttack' e 'CantBlock' | Akoum Firebird |
| 26 | Velocidade / max speed | 42 | 12 | 38 | 38 | contador de velocidade por jogador e o gatilho que o incrementa | Aether Syphon |
| 27 | Custo de mana ou de ativação não representável | 10 | 0 | 0 | 4 | híbrido, Phyrexiano e {Q} não têm símbolo em 'ManaSymbol', nem custo de vida em 'Cost' | Ajani, Sleeper Agent |

### Ainda sem nome: o buraco da própria taxonomia

**9179 cartas** (2255 em Pauper, 1270 em Standard, 5815 em Modern) travaram num texto que          nenhuma regra de `mtg_oracle::coverage` reconheceu. Elas NÃO são pedido de IR nem          padrão de parser: são cartas de que ainda não se sabe de quem é o trabalho. Exemplo:          "Ach! Hans, Run!".

Enquanto esta for a maior linha do relatório, a hora de trabalho mais valiosa é          **classificar**, não implementar — a tabela de prioridade só vale o que a taxonomia          cobre. As tabelas de padrão literal logo abaixo são a matéria-prima para isso.

## Evidência: os padrões literais por trás dos números

`~` é o nome da própria carta, `N` é qualquer número, `<tipo>` é subtipo de criatura, `<cor>` é cor e `<terreno>` é tipo de terreno básico. 10890 padrões distintos, 284 subtipos de criatura no vocabulário. Estas tabelas servem para **conferir** o agrupamento acima, não para priorizar.

### 50 padrões não suportados mais frequentes

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
| 16 | 46 | `choose one or both —` | Against All Odds |
| 17 | 46 | `return target creature card from your graveyard to your hand` | Along the Crooked Way |
| 18 | 46 | `~ attacks each combat if able` | Akoum Firebird |
| 19 | 46 | `~ enters with x +N/+N counters on it` | Benevolent Hydra |
| 20 | 43 | `enchant player` | Cruel Reality |
| 21 | 43 | `investigate` | Alquist Proft, Master Sleuth |
| 22 | 42 | `start your engines!` | Aether Syphon |
| 23 | 42 | `you may look at the top card of your library any time` | All-You-Can-Eat Buffet |
| 24 | 41 | `attach it to target creature you control` | Barbed Bloodletter |
| 25 | 41 | `you get {e}{e}` | Aether Chaser |
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
| 47 | 27 | `gain control of target creature until end of turn` | Act of Treason |
| 48 | 27 | `if you do, draw a card` | Academy Raider |
| 49 | 26 | `flip a coin` | Boompile |
| 50 | 26 | `shadow` | Augur il-Vec |

### Por começo de frase (6 primeiras palavras, 7143 distintos)

| # | Cartas | Padrão normalizado | Exemplo |
|---|---|---|---|
| 1 | 301 | `as an additional cost to cast …` | Abandon Hope |
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
| 33 | 46 | `choose one or both —` | Against All Odds |
| 34 | 46 | `~ attacks each combat if able` | Akoum Firebird |
| 35 | 44 | `search your library for a <tipo> …` | Amrou Scout |
| 36 | 44 | `~ enters with a +N/+N counter …` | Ascendant Acolyte |
| 37 | 43 | `enchant player` | Cruel Reality |
| 38 | 43 | `investigate` | Alquist Proft, Master Sleuth |
| 39 | 43 | `you may have ~ enter as …` | Activated Sleeper |
| 40 | 43 | `you may look at the top …` | All-You-Can-Eat Buffet |
| 41 | 42 | `start your engines!` | Aether Syphon |
| 42 | 41 | `attach it to target creature you …` | Barbed Bloodletter |
| 43 | 41 | `enchanted creature gets +N/+N and has …` | Agility |
| 44 | 41 | `you get {e}{e}` | Aether Chaser |
| 45 | 40 | `~ can't be blocked by creatures …` | Ant-Man, Elusive Avenger |
| 46 | 39 | `look at the top card of …` | Amareth, the Lustrous |
| 47 | 38 | `infect` | Blackcleave Goblin |
| 48 | 38 | `you become the monarch` | Aragorn, King of Gondor |
| 49 | 37 | `as ~ enters, choose a color` | Alloy Golem |
| 50 | 37 | `return this card from your graveyard …` | Bloodsoaked Champion |
