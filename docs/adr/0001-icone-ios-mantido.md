# 0001 — Ícone de iOS: manter o `apple-touch-icon`

Status: aceita (issue #8)

## Contexto

O iOS (Safari) não lê os ícones do manifesto para o ícone da tela inicial.
Sem um `<link rel="apple-touch-icon">`, ele usa uma captura da página, que
fica feia e muda com o conteúdo do orçamento.

## Decisão

Manter `<link rel="apple-touch-icon" href="icons/icon-192.png">` em `web/index.html`.
O iOS aceita o PNG de 192 px e o reduz para a tela; não há um arquivo de 180 px
dedicado.

## Consequências

- `tests/casca.rs` (`cabecalho_da_pagina_bate_com_o_manifesto`) confere o link
  `apple-touch-icon` pelo nome do arquivo, então a remoção do link quebra o teste.
- Se o ícone mudar, troque o PNG de `web/icons/` e os dois `href` continuam valendo.
