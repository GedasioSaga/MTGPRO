-- Fichas (tokens).
--
-- Ficha não é carta: não entra no catálogo, não vai para deck, não tem custo.
-- O que existe aqui são as *plantas* reutilizadas pelas cartas que criam ficha.
-- Manter num arquivo só evita que "Soldier 1/1 branco" seja redigitado em vários
-- lugares com um detalhe diferente em cada um.
--
-- Este arquivo é carregado ANTES dos arquivos de carta: a ordem é fixada no
-- loader de `mtg-cards` (`CARD_FILES`), não depende da ordem alfabética do
-- sistema de arquivos. Todo helper daqui já existe quando as cartas são montadas.

function soldier_token(n)
  return token { name = "Soldier", type = "Creature — Soldier", pt = { 1, 1 }, colors = { "White" }, count = n }
end

function drake_token(n)
  return token { name = "Drake", type = "Creature — Drake", pt = { 2, 2 }, colors = { "Blue" }, keywords = { "Flying" }, count = n }
end

function goblin_token(n)
  return token { name = "Goblin", type = "Creature — Goblin", pt = { 1, 1 }, colors = { "Red" }, count = n }
end

function beast_token(n)
  return token { name = "Beast", type = "Creature — Beast", pt = { 3, 3 }, colors = { "Green" }, count = n }
end
