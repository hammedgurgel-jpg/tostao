# Checklist no Android (Chrome)

Roda a cada versão publicada do Tostão, no celular, para confirmar a casca PWA
(instalar, abrir sem rede, avisar versão nova). Os passos são feitos à mão: nenhum
teste automático cobre o comportamento no navegador.

## Antes de começar

- Celular Android com o Chrome atualizado.
- Endereço: https://hammedgurgel-jpg.github.io/tostao/
- Confira a versão publicada: `VERSAO` em `web/sw.js` no site
  (`https://hammedgurgel-jpg.github.io/tostao/sw.js`). Anote o valor no resultado.
- Se já houver um Tostão instalado, desinstale antes de começar (pressione o ícone e escolha desinstalar).

## Passos

1. [ ] Abrir o endereço no Chrome. O plano do mês aparece.
2. [ ] O botão **Instalar o Tostão** aparece. Tocar nele abre a instalação do Chrome.
3. [ ] Confirmar a instalação. O ícone **Tostão** aparece na tela inicial e o botão some do site.
4. [ ] Abrir pelo ícone. O app abre sem a barra de endereço do Chrome (modo standalone).
5. [ ] Criar uma transação e atribuir um valor a uma categoria.
6. [ ] Ativar o modo avião e fechar o app pelo gesto de apps recentes.
7. [ ] Abrir pelo ícone, ainda com o modo avião. O app abre e mostra o orçamento salvo, com a transação e a atribuição do passo 5.
8. [ ] Com o modo avião, fazer uma mudança nova (atribuir outro valor). Fechar e reabrir o app. A mudança continua lá.
9. [ ] Desativar o modo avião.
10. [ ] Versão nova (só quando houver uma versão publicada depois da instalada):
    - Ao abrir o app, aparece o aviso **Nova versão do Tostão.** com o botão **Atualizar**.
    - Não tocar e fechar o app: ao reabrir, o aviso aparece de novo.
    - Tocar em **Atualizar**: a página recarrega e o aviso some.
11. [ ] Armazenamento persistente. No celular não há console fácil: conferir no Chrome Desktop, com o app instalado aberto e o DevTools no console, `await navigator.storage.persisted()`. O resultado precisa ser `true`. Se for `false`, anotar: o navegador ainda não concedeu a persistência.
12. [ ] Exportar o backup e importar de volta: os dois funcionam e o orçamento não muda.
13. [ ] Nenhuma tela tem rolagem horizontal (largura de celular).

## Resultado

- Data:
- Versão publicada (`VERSAO` em `sw.js`):
- Aparelho e versão do Android:
- Versão do Chrome:
- Passos que falharam (número e o que aconteceu):
