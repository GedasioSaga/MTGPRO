# Suíte de interações — a metade mensurável da barra

Cada item vira **um teste** com o nome do item. O teste monta um estado mínimo,
executa, e afirma o resultado exato. Pass rate desta suíte é o número que
acompanha o julgamento visual: gosto sozinho infla, gosto com número não.

Convenção dos testes: `Game` com `FixedAgent`/`ScriptedAgent` (agentes de teste
determinísticos), semente fixa, ferramental único em
`crates/mtg-core/tests/common/mod.rs`.

**Onde cada teste vive** — itens 1–41 em `tests/interactions_core.rs`, 42–60 em
`tests/interactions_combat.rs`, 61–65 em `tests/fuzz.rs`.

## Legenda

`[x]` coberto por teste que afirma a regra · `[~]` parcial · `[ ]` ausente ·
`[!]` motor errado.

## Estado real em 18/08/2026

**65/65 `[x]`.** Nenhum `[~]`, nenhum `[ ]`, nenhum `[!]` em aberto. Verificado
com `cargo test --workspace` (175 testes, 0 falhas, 4 `#[ignore]` só por lentidão)
e `cargo test --workspace -- --ignored` (4 testes, 0 falhas).

Dois itens estavam `[!]` e o motor foi corrigido nesta rodada:

- **18** — `resolve::damage_object` só consultava `prevent_all_damage`, então
  dano fora de combate atravessava proteção. Corrigido com
  `resolve::protected_from` (CR 702.16c).
- **24** — `layers::collect_runtime_mods` aplicava `Duration::WhileSourcePresent`
  mesmo com a fonte fora do campo; só a limpeza do turno removia o efeito.
  Corrigido com `layers::effect_source_present` (CR 611.2b).

Itens 62, 63 e 65 são lentos e ficam sob `#[ignore]`: rodam com
`cargo test --workspace -- --ignored`. O `#[ignore]` deles é de custo, não de
falha.

## 1. Estrutura de turno e prioridade

1. [x] `primeiro_jogador_nao_compra_no_primeiro_turno` — CR 103.7a.
2. [x] `passo_de_desvirar_desvira_so_do_jogador_ativo` — CR 502.1.
3. [x] `pool_de_mana_esvazia_no_fim_do_passo` — CR 500.4.
4. [x] `ambos_passam_com_pilha_vazia_encerra_o_passo` — CR 117.4.
5. [x] `ambos_passam_com_pilha_cheia_resolve_o_topo` — CR 117.4.
6. [x] `limpeza_descarta_ate_sete_cartas` — CR 514.1.
7. [x] `limpeza_remove_dano_marcado` — CR 514.2.
8. [x] `gatilho_na_limpeza_da_prioridade_e_repete_a_limpeza` — CR 514.3a.
9. [x] `terreno_so_pode_ser_jogado_uma_vez_por_turno` — CR 305.1.
10. [x] `feitico_nao_pode_ser_lancado_com_pilha_nao_vazia` — CR 307.1.

## 2. Pilha, alvos e anulação

11. [x] `pilha_resolve_em_ordem_inversa` — LIFO.
12. [x] `magica_com_unico_alvo_ilegal_nao_resolve` — CR 608.2b (fizzle).
13. [x] `magica_com_dois_alvos_e_um_ilegal_ainda_resolve_no_outro` — CR 608.2b.
14. [x] `contra_magica_manda_o_alvo_para_o_cemiterio` — CR 701.5a.
15. [x] `hexproof_impede_alvo_de_oponente_mas_nao_do_controlador` — CR 702.11b.
16. [x] `shroud_impede_alvo_inclusive_do_controlador` — CR 702.18a.
17. [x] `protecao_contra_vermelho_impede_alvo_de_magica_vermelha` — CR 702.16b.
18. [x] `protecao_previne_dano_da_cor_protegida` — CR 702.16c. Era `[!]`: motor
    corrigido em `resolve::protected_from`.

## 3. Camadas (CR 613)

19. [x] `anthem_soma_com_marcador_de_mais_um` — +1/+1 estático + marcador = +2/+2.
20. [x] `define_pt_aplicado_depois_de_modifica_pt_ganha` — 7b antes de 7c.
21. [x] `dois_efeitos_de_define_pt_o_mais_novo_ganha` — timestamp, CR 613.7.
22. [x] `perda_de_palavra_chave_depois_de_ganho_remove` — camada 6 por timestamp.
23. [x] `efeito_de_fim_de_turno_expira_na_limpeza` — Duration::EndOfTurn.
24. [x] `efeito_expira_quando_a_fonte_sai_do_campo` — Duration::WhileSourcePresent.
    Era `[!]`: motor corrigido em `layers::effect_source_present`.
25. [x] `marcadores_menos_um_reduzem_resistencia_e_matam` — 7d + SBA 704.5f.

## 4. Ações baseadas em estado (CR 704)

26. [x] `vida_zero_perde_o_jogo` — 704.5a.
27. [x] `comprar_de_biblioteca_vazia_perde_na_proxima_sba` — 704.5b, não na hora.
28. [x] `criatura_com_resistencia_zero_vai_para_o_cemiterio` — 704.5f.
29. [x] `dano_letal_destroi` — 704.5g.
30. [x] `indestrutivel_sobrevive_a_dano_letal` — 704.5g + 702.12b.
31. [x] `indestrutivel_com_resistencia_zero_ainda_morre` — 704.5f ignora indestrutível.
32. [x] `toque_mortal_com_um_de_dano_destroi` — 704.5h.
33. [x] `regra_da_lenda_mantem_uma` — 704.5j.
34. [x] `marcadores_opostos_se_anulam` — 704.5r.
35. [x] `aura_sem_alvo_legal_vai_para_o_cemiterio` — 704.5m.

## 5. Gatilhos (CR 603)

36. [x] `gatilho_de_entrada_vai_para_a_pilha_antes_da_proxima_prioridade` — 603.3.
37. [x] `gatilho_de_morte_ve_o_estado_de_antes_da_morte` — 603.6d / "last known information".
38. [x] `gatilhos_simultaneos_apnap_jogador_ativo_primeiro` — 603.3b.
39. [x] `condicao_de_intervencao_falsa_impede_o_disparo` — 603.4.
40. [x] `gatilho_opcional_recusado_nao_faz_nada` — "você pode".
41. [x] `uma_vez_por_turno_nao_dispara_duas_vezes`.

## 6. Combate (CR 506–511)

42. [x] `criatura_com_enjoo_nao_pode_atacar` — CR 302.6.
43. [x] `pressa_permite_atacar_no_turno_que_entrou` — CR 702.10b.
44. [x] `vigilancia_ataca_sem_virar` — CR 702.20b.
45. [x] `voar_so_e_bloqueado_por_voar_ou_alcance` — CR 702.9b.
46. [x] `ameacar_exige_dois_bloqueadores` — CR 702.110b.
47. [x] `atacante_bloqueado_cujo_bloqueador_sumiu_nao_causa_dano_ao_jogador` — CR 509.1h.
48. [x] `atropelar_passa_o_excedente_ao_defensor` — CR 702.19b.
49. [x] `atropelar_com_toque_mortal_so_precisa_atribuir_um` — CR 702.2c.
50. [x] `primeiro_golpe_mata_antes_do_dano_normal` — CR 510.4.
51. [x] `golpe_duplo_causa_dano_nos_dois_passos` — CR 702.4b.
52. [x] `dano_de_combate_e_simultaneo_troca_mutua` — CR 510.2.
53. [x] `vinculo_com_a_vida_da_vida_ao_controlador` — CR 702.15b.
54. [x] `bloqueio_multiplo_respeita_a_ordem_de_dano` — CR 510.1c.

## 7. Mana e custos

55. [x] `custo_hibrido_pode_ser_pago_com_qualquer_metade` — CR 202.2f.
56. [x] `custo_phyrexiano_pode_ser_pago_com_dois_de_vida` — CR 107.4f.
57. [x] `x_igual_a_zero_e_um_valor_legal`.
58. [x] `habilidade_de_mana_nao_usa_a_pilha` — CR 605.3a.
59. [x] `custo_adicional_de_sacrificio_e_pago_antes_de_resolver` — CR 601.2h.
60. [x] `mana_nao_pago_impede_a_magica_de_aparecer_nas_acoes_legais`.

## 8. Determinismo e robustez do simulador

61. [x] `mesma_semente_produz_partida_identica` — dois `Game` com mesmo seed e mesmos
    bots geram o mesmo log completo.
62. [x] `partida_completa_termina_dentro_do_limite_de_turnos` (`--ignored`) — 200 partidas aleatórias,
    nenhuma estoura `max_turns` sem resultado.
63. [x] `nenhuma_partida_entra_em_panico` (`--ignored`) — 200 partidas com bots aleatórios, sementes
    0..199, nenhuma `panic!`, nenhum `unwrap` estourado.
64. [x] `acao_ilegal_de_agente_e_descartada_sem_corromper_estado`.
65. [x] `todas_as_cartas_do_catalogo_sao_lancaveis` (`--ignored`) — para cada `CardDef`, montar um
    estado com mana infinita e confirmar que a carta aparece nas ações legais e
    resolve sem erro.

## Metas

| Métrica | Alvo | Aferido em 18/08/2026 | Como medir |
|---|---|---|---|
| Pass rate da suíte | 100% dos itens 1–65 | 65/65 | `cargo test --workspace` |
| Partidas sem pânico | 200/200 | 200/200 | teste 63 (`--ignored`) |
| Determinismo | 100% | 100% | teste 61 |
| Cartas jogáveis | 100% do catálogo | 100% | teste 65 (`--ignored`) |
| Tempo de partida completa | < 150 ms com bot heurístico | não medido | benchmark em `mtg-server` |
