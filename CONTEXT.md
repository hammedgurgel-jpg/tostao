# Orçamento da família (nome provisório)

App de orçamento por envelopes para uso pessoal e da família: todo dinheiro que entra recebe uma função (é atribuído a uma categoria) antes de ser gasto. Vários aparelhos editam o mesmo orçamento e se sincronizam com um servidor próprio.

## Orçamento e estrutura

**Orçamento**:
O conjunto de contas, categorias, meses e transações de uma família, sincronizado como uma unidade.
_Evite_: plano, carteira

**Conta**:
Um lugar onde o dinheiro está de fato: conta corrente, poupança, dinheiro vivo, cartão de crédito.
_Evite_: carteira, banco

**Grupo de categorias**:
Agrupamento de categorias só para organizar a tela (ex.: "Contas fixas", "Lazer"). Não guarda dinheiro.
_Evite_: categoria-mãe, categoria mestre

**Categoria**:
Um envelope com finalidade definida (ex.: "Mercado", "Aluguel"), dentro de um grupo de categorias, onde o dinheiro é atribuído e de onde as transações saem.
_Evite_: subcategoria (é o nome no protocolo herdado, não na conversa), envelope, rubrica

**Mês do orçamento**:
Um mês do calendário em que se atribui dinheiro e se apuram saldos. Cada categoria tem um saldo próprio em cada mês.
_Evite_: período, competência

## Dinheiro e movimento

**Transação**:
Uma entrada ou saída de dinheiro numa conta, numa data, com valor, beneficiário e categoria. Saídas são valores negativos.
_Evite_: lançamento, movimento

**Subtransação**:
Uma parte de uma transação dividida entre várias categorias; as partes somam o valor da transação.
_Evite_: rateio, split

**Atribuição**:
O valor que a família decide reservar para uma categoria num mês do orçamento. Pode ser negativo (para devolver dinheiro).
_Evite_: orçado, alocação, budgeted

**Movimentação de dinheiro**:
O registro de um valor tirado de uma categoria e posto em outra (ou em "A atribuir") dentro de um mês, que altera as atribuições das duas pontas.
_Evite_: transferência (transferência é entre contas)

**Receita do mês**:
O dinheiro de transações de entrada classificadas como "A atribuir" no mês; aumenta o disponível do mês.
_Evite_: renda, entrada

## Apuração

**Saldo da categoria**:
Quanto resta numa categoria no fim de um mês: o saldo positivo do mês anterior, mais a atribuição do mês, mais as saídas do mês (negativas). Saldo negativo do mês anterior não passa para o mês seguinte da categoria.
_Evite_: disponível da categoria, sobra

**Estouro**:
Saldo negativo de uma categoria num mês: gastou-se mais do que havia. A parte paga com dinheiro (não crédito) é descontada do disponível do mês seguinte, e não da categoria. Como o estouro no cartão de crédito vira dívida ainda não foi definido.
_Evite_: déficit, negativo

**Disponível do mês**:
Dinheiro ainda sem atribuição em um mês: o disponível do mês anterior, mais o estouro do mês anterior (negativo), mais a receita do mês, menos as atribuições do mês. Pode ficar negativo quando se atribui mais do que se tem.
_Evite_: saldo do mês, sobra

**A atribuir**:
O que a tela mostra como dinheiro livre num mês: o disponível do mês menos o que já foi atribuído em meses futuros, só quando ambos são positivos e limitado ao disponível. É o "Ready to Assign" de outros apps.
_Evite_: Ready to Assign, To be Budgeted, saldo livre

**Atribuído no futuro**:
A soma das atribuições de todos os meses depois de um mês; é a parte do disponível já comprometida adiante.
_Evite_: reservado

## Sincronização

**Aparelho**:
Um navegador ou dispositivo que guarda uma cópia local do orçamento e faz mudanças nela, identificado por um id próprio.
_Evite_: cliente, sessão

**Servidor**:
A cópia de referência do orçamento, mantida pela família numa máquina sua. Recebe as mudanças de cada aparelho e as repassa aos outros.
_Evite_: nuvem, backend

**Conhecimento**:
Um contador que só cresce e marca até onde um lado viu as mudanças de outro. Há quatro: o conhecimento do aparelho (sobe a cada mudança local), o conhecimento do servidor (sobe a cada lote de mudanças que ele aceita), o que o servidor sabe do aparelho e o que o aparelho sabe do servidor.
_Evite_: versão, revisão, timestamp

**Sincronização incremental**:
Troca em que o aparelho envia suas mudanças ainda não recebidas pelo servidor e recebe só o que mudou no servidor depois do conhecimento que ele já tem. É a sincronização normal, a cada mudança e a cada minuto.
_Evite_: delta (é o nome no protocolo)

**Carga inicial**:
Primeira sincronização de um aparelho, que traz o orçamento sem registrar o conhecimento do servidor.
_Evite_: bootstrap (é o nome no protocolo)

**Recarga completa**:
Sincronização que traz todas as entidades do orçamento, ignorando o conhecimento que o aparelho já tem do servidor, sem enviar mudanças.
_Evite_: backfill (é o nome no protocolo)

**Versão de esquema**:
O número da forma dos dados trocados. Aparelho e servidor precisam ter a mesma; se divergem, o aparelho para de sincronizar até ser atualizado.
_Evite_: versão do app

**Remoção**:
Uma entidade apagada continua existindo marcada como removida, para que a remoção chegue aos outros aparelhos.
_Evite_: exclusão definitiva, tombstone (é o nome no protocolo)
