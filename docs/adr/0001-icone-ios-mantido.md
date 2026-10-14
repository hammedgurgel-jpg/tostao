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

- `tests/casca.rs` confere que o ícone da página bate com o do manifesto; o
  `apple-touch-icon` usa o mesmo arquivo, então o ícone de iOS também é coberto.
- Se o ícone mudar, troque o PNG de `web/icons/` e os dois `href` continuam valendo.
