# Suíte de interações — a metade mensurável da barra

Cada item vira **um teste** em `crates/mtg-core/tests/interactions.rs`, nomeado
como o item. O teste monta um estado mínimo, executa, e afirma o resultado exato.
Pass rate desta suíte é o número que acompanha o julgamento visual: gosto sozinho
infla, gosto com número não.

Convenção dos testes: `Game` com `FixedAgent` (agente de teste que responde uma
fila pré-programada de `Action`), semente fixa, `GameConfig::default()`.

## 1. Estrutura de turno e prioridade

1. `primeiro_jogador_nao_compra_no_primeiro_turno` — CR 103.7a.
2. `passo_de_desvirar_desvira_so_do_jogador_ativo` — CR 502.1.
3. `pool_de_mana_esvazia_no_fim_do_passo` — CR 500.4.
4. `ambos_passam_com_pilha_vazia_encerra_o_passo` — CR 117.4.
5. `ambos_passam_com_pilha_cheia_resolve_o_topo` — CR 117.4.
6. `limpeza_descarta_ate_sete_cartas` — CR 514.1.
7. `limpeza_remove_dano_marcado` — CR 514.2.
8. `gatilho_na_limpeza_da_prioridade_e_repete_a_limpeza` — CR 514.3a.
9. `terreno_so_pode_ser_jogado_uma_vez_por_turno` — CR 305.1.
10. `feitico_nao_pode_ser_lancado_com_pilha_nao_vazia` — CR 307.1.

## 2. Pilha, alvos e anulação

11. `pilha_resolve_em_ordem_inversa` — LIFO.
12. `magica_com_unico_alvo_ilegal_nao_resolve` — CR 608.2b (fizzle).
13. `magica_com_dois_alvos_e_um_ilegal_ainda_resolve_no_outro` — CR 608.2b.
14. `contra_magica_manda_o_alvo_para_o_cemiterio` — CR 701.5a.
15. `hexproof_impede_alvo_de_oponente_mas_nao_do_controlador` — CR 702.11b.
16. `shroud_impede_alvo_inclusive_do_controlador` — CR 702.18a.
17. `protecao_contra_vermelho_impede_alvo_de_magica_vermelha` — CR 702.16b.
18. `protecao_previne_dano_da_cor_protegida` — CR 702.16c.

## 3. Camadas (CR 613)

19. `anthem_soma_com_marcador_de_mais_um` — +1/+1 estático + marcador = +2/+2.
20. `define_pt_aplicado_depois_de_modifica_pt_ganha` — 7b antes de 7c.
21. `dois_efeitos_de_define_pt_o_mais_novo_ganha` — timestamp, CR 613.7.
22. `perda_de_palavra_chave_depois_de_ganho_remove` — camada 6 por timestamp.
23. `efeito_de_fim_de_turno_expira_na_limpeza` — Duration::EndOfTurn.
24. `efeito_expira_quando_a_fonte_sai_do_campo` — Duration::WhileSourcePresent.
25. `marcadores_menos_um_reduzem_resistencia_e_matam` — 7d + SBA 704.5f.

## 4. Ações baseadas em estado (CR 704)

26. `vida_zero_perde_o_jogo` — 704.5a.
27. `comprar_de_biblioteca_vazia_perde_na_proxima_sba` — 704.5b, não na hora.
28. `criatura_com_resistencia_zero_vai_para_o_cemiterio` — 704.5f.
29. `dano_letal_destroi` — 704.5g.
30. `indestrutivel_sobrevive_a_dano_letal` — 704.5g + 702.12b.
31. `indestrutivel_com_resistencia_zero_ainda_morre` — 704.5f ignora indestrutível.
32. `toque_mortal_com_um_de_dano_destroi` — 704.5h.
33. `regra_da_lenda_mantem_uma` — 704.5j.
34. `marcadores_opostos_se_anulam` — 704.5r.
35. `aura_sem_alvo_legal_vai_para_o_cemiterio` — 704.5m.

## 5. Gatilhos (CR 603)

36. `gatilho_de_entrada_vai_para_a_pilha_antes_da_proxima_prioridade` — 603.3.
37. `gatilho_de_morte_ve_o_estado_de_antes_da_morte` — 603.6d / "last known information".
38. `gatilhos_simultaneos_apnap_jogador_ativo_primeiro` — 603.3b.
39. `condicao_de_intervencao_falsa_impede_o_disparo` — 603.4.
40. `gatilho_opcional_recusado_nao_faz_nada` — "você pode".
41. `uma_vez_por_turno_nao_dispara_duas_vezes`.

## 6. Combate (CR 506–511)

42. `criatura_com_enjoo_nao_pode_atacar` — CR 302.6.
43. `pressa_permite_atacar_no_turno_que_entrou` — CR 702.10b.
44. `vigilancia_ataca_sem_virar` — CR 702.20b.
45. `voar_so_e_bloqueado_por_voar_ou_alcance` — CR 702.9b.
46. `ameacar_exige_dois_bloqueadores` — CR 702.110b.
47. `atacante_bloqueado_cujo_bloqueador_sumiu_nao_causa_dano_ao_jogador` — CR 509.1h.
48. `atropelar_passa_o_excedente_ao_defensor` — CR 702.19b.
49. `atropelar_com_toque_mortal_so_precisa_atribuir_um` — CR 702.2c.
50. `primeiro_golpe_mata_antes_do_dano_normal` — CR 510.4.
51. `golpe_duplo_causa_dano_nos_dois_passos` — CR 702.4b.
52. `dano_de_combate_e_simultaneo_troca_mutua` — CR 510.2.
53. `vinculo_com_a_vida_da_vida_ao_controlador` — CR 702.15b.
54. `bloqueio_multiplo_respeita_a_ordem_de_dano` — CR 510.1c.

## 7. Mana e custos

55. `custo_hibrido_pode_ser_pago_com_qualquer_metade` — CR 202.2f.
56. `custo_phyrexiano_pode_ser_pago_com_dois_de_vida` — CR 107.4f.
57. `x_igual_a_zero_e_um_valor_legal`.
58. `habilidade_de_mana_nao_usa_a_pilha` — CR 605.3a.
59. `custo_adicional_de_sacrificio_e_pago_antes_de_resolver` — CR 601.2h.
60. `mana_nao_pago_impede_a_magica_de_aparecer_nas_acoes_legais`.

## 8. Determinismo e robustez do simulador

61. `mesma_semente_produz_partida_identica` — dois `Game` com mesmo seed e mesmos
    bots geram o mesmo log completo.
62. `partida_completa_termina_dentro_do_limite_de_turnos` — 200 partidas aleatórias,
    nenhuma estoura `max_turns` sem resultado.
63. `nenhuma_partida_entra_em_panico` — 200 partidas com bots aleatórios, sementes
    0..199, nenhuma `panic!`, nenhum `unwrap` estourado.
64. `acao_ilegal_de_agente_e_descartada_sem_corromper_estado`.
65. `todas_as_cartas_do_catalogo_sao_lancaveis` — para cada `CardDef`, montar um
    estado com mana infinita e confirmar que a carta aparece nas ações legais e
    resolve sem erro.

## Metas

| Métrica | Alvo | Como medir |
|---|---|---|
| Pass rate da suíte | 100% dos itens 1–60 | `cargo test --workspace` |
| Partidas sem pânico | 200/200 | teste 63 |
| Determinismo | 100% | teste 61 |
| Cartas jogáveis | 100% do catálogo | teste 65 |
| Tempo de partida completa | < 150 ms com bot heurístico | benchmark em `mtg-server` |
