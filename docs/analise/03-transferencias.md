# Análise 03 — transferências entre contas

Fonte: bundle web `chunk.7bd67ec4d1cff4ed86e3.js`, lido em 2026-10-13.
O arquivo usado foi a cópia do scratchpad de outra sessão (`…/C--Users-manut-dev-retencao-diaria/a5be8006-…/scratchpad/ynab-src/chunk.7bd67ec4d1cff4ed86e3.js`), com 3.093.874 bytes e **SHA-256 06032e4389cc6cfd39db0080887b01569b842db8cacc7188bea6805669522ef3**.
Esse hash **não bate** com o citado em `src/lib.rs` (c7e8b170…). Não achei outra cópia, então os offsets desta análise valem para o arquivo 06032e43…. Se a cópia c7e8b170… aparecer, os offsets podem estar deslocados.
Os números entre parênteses são offsets no arquivo (para reencontrar o trecho com `node`) ou ids de módulo webpack.
As regras estão descritas com nossas palavras; o código do YNAB não foi copiado.

Legenda de confiança (igual às análises 01 e 02):
- **visto**: lido no bundle;
- **reproduzido**: o trecho foi portado e rodado com casos de teste;
- **conferido na tela**: observado num orçamento de teste do YNAB;
- **inferido**: deduzido de nomes e uso, sem a lógica à vista;
- **não achado**: não está no bundle web.

Nada foi reproduzido. O roteiro 3 (entre contas orçamentárias) foi conferido na tela em 2026-10-13. O pagamento do cartão já vinha conferido da análise 01. Os outros roteiros da §9 ainda faltam.

Vocabulário: "transferência" é sempre entre **contas** (CONTEXT.md). Chamo de **ponta** cada uma das duas transações de uma transferência, e de **espelho** a ponta que o app cria sozinho.

## 1. Modelo

### Campos — visto (1935300, 1968560, 1993293; mapeamento do servidor em 1906750, 1910350, 1911797, 1898800)

| Entidade | Campo | Para que serve |
|---|---|---|
| Transação e subtransação | `transfer_account_id` | conta da outra ponta |
| Transação e subtransação | `transfer_transaction_id` | id da **transação** espelho |
| Transação | `transfer_subtransaction_id` | id da **subtransação** que é a outra ponta (quando a transferência sai de uma divisão) |
| Transação agendada e subtransação agendada | `transfer_account_id` | só a conta; **não** há id de espelho (o servidor devolve `transferTransactionId: null`, 1898800) |
| Beneficiário | `account_id` | preenchido só no beneficiário de transferência |

- "É transferência" (`isTransferTransaction`) = tem `transfer_account_id` (1936044). **visto**
- Uma transação comum tem as duas pontas como **duas entidades `be_transaction` separadas**, cada uma com o id da outra em `transfer_transaction_id`. **visto**
- Quando a transferência é uma parte de uma divisão, a subtransação aponta para a transação espelho (`transfer_transaction_id`) e a espelho aponta de volta para a subtransação (`transfer_subtransaction_id`). A espelho é sempre uma transação inteira, nunca uma subtransação. **visto** (função `Q`, 2344955)

### Beneficiário de transferência — visto (66689, 1975956, 2064751)

- Cada conta tem um beneficiário próprio, com `account_id` = a conta. É "beneficiário de transferência" quem tem `account_id` não vazio (`isTransferPayee`).
- O nome é `"Transfer : " + nome da conta` (prefixo `TransferPayeeNamePrefix`). Ao renomear a conta, o app renomeia esse beneficiário (`updateTransferPayeeName`, 2064751).
- Nomes internos que aparecem junto: `transfer_on_budget`, `transfer`, `transfer_off_budget` (66689). O uso deles não foi lido.
- Quem cria o beneficiário quando a conta nasce: **não achado** no bundle web. **Inferido:** o servidor cria na criação da conta.
- A ponta guarda como beneficiário **o beneficiário da outra conta**. Uma transferência da Corrente para a Poupança fica, na Corrente, com beneficiário "Transfer : Poupança"; na Poupança, com "Transfer : Corrente".
- Ao digitar um nome que começa com "Transfer : " e não existe, o app não cria beneficiário novo (2298541). **visto**

### O que define a transferência — visto (função `Q`, 2344955)

O campo que manda é o **beneficiário**. Ao salvar, se o beneficiário é de transferência, o app preenche `transfer_account_id` com a conta dele e cria ou atualiza a espelho. Se não é, apaga os campos de transferência e remove a espelho. `transfer_account_id` é derivado do beneficiário.

## 2. Criar, editar e remover

Tudo passa por uma função só, chamada a cada salvamento de transação e de cada subtransação (`Q`, 2344955; chamada em `createTransaction` 2332777, `updateTransaction` 2334280, `delete` 2341550, `approveTransaction` 2340315 e na conversão para agendada). Chamo de **A** a ponta salva e de **B** a outra.

### Quando A deixa de ser transferência — visto

Acontece se A foi removida, se o beneficiário deixou de ser de transferência, ou se A é uma importação crua (fonte fora do conjunto de "visíveis", módulo 60947, ver análise 02 §3).

1. Em A: apaga `transfer_account_id`, `transfer_transaction_id` e `transfer_subtransaction_id`. Se o beneficiário ainda era de transferência, ele também é apagado.
2. Se B existe **e aponta de volta para A**: apaga os campos de transferência de B e o beneficiário, remove as subtransações de B, remove as imagens de B e **remove B**.

Ou seja: **remover uma ponta remove a outra**, e trocar o beneficiário de transferência por um comum também remove a espelho.

### Quando A é transferência — visto

**Se B ainda não existe** (criação), o app cria B:
- conta = conta do beneficiário de A; data = data de A;
- valor = **−valor de A**;
- estado = **não compensada**; número do cheque vazio;
- aceite = o de A (conta de empréstimo: aceita sempre; função `nt`);
- beneficiário = o beneficiário de transferência da conta de A;
- B aponta de volta para A (`transfer_transaction_id`, ou `transfer_subtransaction_id` se A é subtransação);
- se A veio de uma agendada, o id de B é determinístico (ver §6);
- depois de criada, B tenta casar com uma importada da mesma conta (§6).

**Sempre que B aponta de volta para A** (criação ou edição), copia de A para B:

| Campo | Regra |
|---|---|
| valor | −valor de A |
| memo | copiado |
| beneficiário | o de transferência da conta de A |
| conta de transferência de B | conta de A |
| sinalizador (flag) | copiado, só de transação para transação (subtransação não tem) |
| conta de B | a conta do beneficiário de A (é assim que a troca de destino funciona, abaixo) |
| data | copiada |
| estado de compensação | **não copiado**. B só vira compensada se a conta dela é de dinheiro vivo (e não está reconciliada), ou se quem salvou pediu um estado para a espelho (`setTransferCounterpartClearedStatus`, sem chamada neste bundle) |
| aceite | **não copiado na edição**. Mas aprovar uma ponta aprova a outra (abaixo) |
| fonte "pendente" | acompanha: se só A é pendente, B vira pendente; se B era pendente e A não é mais, B volta ao normal e é aceita |
| categoria | ver §3 |

Por fim, A recebe `transfer_account_id` = conta do beneficiário e `transfer_transaction_id` = id de B.

**Se B existe mas não aponta de volta para A**, o app não mexe em B, e A passa a apontar para B mesmo assim. Não achei o caso de uso; pode ser um estado de recuperação.

**Se A aponta para uma B que não está no banco local**, o app cria **outra** B com id novo. **visto** (o `if (!o)` só olha o banco local). Ver §7, risco de duplicar.

### Trocar a conta de destino — visto

Trocar o beneficiário "Transfer : Poupança" por "Transfer : Cartão" **não cria espelho nova**: a mesma B (mesmo id) **muda de conta**, e valor, memo e data são copiados de novo. O estado de compensação de B não muda; uma B compensada na Poupança chega compensada no Cartão (exceto em conta de dinheiro vivo).

Quando a outra ponta é uma subtransação (A é a espelho de uma parte de divisão), editar A **muda a conta e a data da transação dividida inteira** (`o.transaction.setAccountId`, `setDate`). **visto**

### Aprovar e compensar — visto (2340315)

- Aprovar uma ponta aprova a outra (`autoApproveTransferCounterpart`). Se a outra estava casada com uma importada, o casamento é aceito.
- Marcar compensada (`setCleared`, com aceite) também aprova a outra ponta, mas **o estado de compensação não passa** para ela.
- Rejeitar a espelho de uma parte de divisão rejeita a transação dividida (`rejectTransaction`).

### Remover — visto (2341550, 2326697)

- `delete` remove a transação, as imagens, a casada e as subtransações. Para cada parte que é transferência, a espelho também é removida.
- **Não se pode remover a espelho de uma parte de divisão**: erro "The transaction transfers to a sub-transaction and cannot be deleted." (`validateDelete`). Tem que remover a divisão (ou a parte) do lado de origem.
- Remover transferências de/para conta fechada mostra aviso (`BulkEdit_EditClosedTransferAccountTransaction…`, 2423134).
- Converter uma transação em agendada (data futura) remove a transação e a espelho, e cria a agendada (`convertToScheduledTransaction`, 2337882).

### Outras regras da tela — visto

- Transferência para a própria conta: o app apaga o beneficiário e a conta de transferência em silêncio (`validateNotTransferredToOwnAccount`, 2327747).
- Duplicar uma transferência duplica uma vez por par (se as duas pontas estão selecionadas, só uma é duplicada) e cria a espelho nova (2893700).
- Reconciliada: a tela tem uma função que acha as pontas do outro lado já reconciliadas (`getOtherSideReconciledTransfers`, 2809274), mas não achei quem a chama neste bundle.

## 3. Categoria

### Regras — visto

- **Transferência entre orçamentárias** (`isOnBudgetTransfer`, 1969132) = a conta é orçamentária, o beneficiário é de transferência e a conta dele é orçamentária.
- Uma transação pode ter categoria se a conta é orçamentária e ela **não** é transferência entre orçamentárias (`canHaveSubCategory`, 1970162). A espelho de agendada desenhada na tela também não (§6).
- Ao salvar, o app apaga a categoria se ela não é permitida (`removeSubCategoryIfNotAllowed`, 2331662): com beneficiário de transferência, ela só fica se **exatamente uma** das duas contas é orçamentária; sem transferência, só fica se a conta é orçamentária.
- **Categoria muda de lado** (função `Q`): se A está numa conta de acompanhamento e tem categoria, a categoria vai para B (se B é orçamentária e A não é empréstimo) e é apagada de A. Assim, quem lança a transferência pela conta de investimento ainda acaba com a categoria na ponta orçamentária.
- "Precisa de categoria" (`needsCategory`, 2812753): conta orçamentária, sem categoria, não agendada, não pendente e não é transferência entre orçamentárias. Então **a ponta orçamentária de uma transferência para acompanhamento precisa de categoria**, como qualquer gasto ou receita.
- "Aparece como sem categoria" (`isDisplayableAsUncategorized`, 1995480): sem categoria e (não é transferência ou a outra conta é de acompanhamento).

Resumo:

| Origem → destino | Categoria | Efeito |
|---|---|---|
| orçamentária → orçamentária | nenhuma nas duas | saldos das contas mudam; nada muda nas categorias nem no "A atribuir" |
| orçamentária → acompanhamento | só na ponta orçamentária | é um gasto: sai da categoria escolhida; sem categoria, vira "sem categoria" (estouro em dinheiro, análise 01 §2) |
| acompanhamento → orçamentária | só na ponta orçamentária | é uma entrada: com "Pronto para atribuir" (entrada imediata) vira receita do mês; com outra categoria, soma nela |
| acompanhamento → acompanhamento | nenhuma | fora do orçamento |

### Efeito no "A atribuir" e nas categorias — inferido

- O cálculo é do servidor (análise 01 §1), então a regra de soma não está no bundle. Pelos campos do cliente (`otherInflowsThisMonth`, 2115236; `x`, 3022305), uma transferência entre orçamentárias **não conta** como receita nem como gasto de categoria.
- Na ponta orçamentária de uma transferência para acompanhamento, a transação conta como qualquer outra com aquela categoria. **Inferido**, a conferir nos roteiros 4.
- **Conferido na tela (roteiro 4B):** uma entrada sem categoria (de acompanhamento para orçamentária) **não** entra no "A atribuir". Ela fica na linha "Uncategorized Transactions" com disponível positivo.

### Empréstimo — visto, não aprofundado

- Transferência de conta orçamentária para conta de empréstimo recebe a categoria da dívida (`accountSubCategory`) no lado orçamentário, e há validação disso (`validatePaymentToLoanAccountCategory`, `DebtPaymentCategoryError`). Também `setDebtTransactionTypeIfNeeded`. Fica para uma análise de empréstimos.

## 4. Cartão de crédito

### Pagamento é transferência — visto

- "É pagamento de cartão" (`isCreditCardPaymentTransfer`, 1995260): transferência entre orçamentárias em que uma ponta está no cartão, e
  - vista da conta de origem: saída (≤ 0) para um cartão, de conta que não é dívida; ou
  - vista do cartão: entrada (≥ 0) vinda de conta que não é dívida.
- `isCreditPayment` (2343279): transferência entre orçamentárias entre contas de tipos diferentes em que uma é de crédito.
- **Atalho da tela** (2854364): numa conta que não é de crédito, escolher como categoria a **categoria de pagamento de um cartão** troca o beneficiário para "Transfer : (cartão)". Ao salvar, a categoria é apagada (é transferência entre orçamentárias). Ao escolher o beneficiário de transferência para um cartão, a grade mostra a categoria de pagamento só para exibir.
- Último pagamento: a conta guarda o último beneficiário que pagou (`updateLastPaymentPayee`, 1970700; `updateLastPayment`, 2935337).

### Ligação com a análise 01, §3 — visto + conferido na tela

- Painel da categoria de pagamento (módulo 21200):
  - **pagamentos no mês** = soma das **entradas no cartão que são transferência entre orçamentárias** (`paymentsMadeThisMonth`, 2113044);
  - outras entradas = receita (entrada imediata) ou sem categoria, positivas, que **não** são transferência (2115236);
  - outras saídas = saídas classificadas como entrada imediata (exceto saldo inicial) ou **transferências entre orçamentárias que saem do cartão** (2115543). Ou seja, mandar dinheiro do cartão para a corrente não é gasto nem pagamento no painel.
  - "sem categoria" e "estouro no crédito" ignoram as transferências entre orçamentárias (2112732, 2114000).
- A regra conferida na tela (análise 01, roteiro 2, passo 2B): o pagamento de 30 da corrente para o cartão **sai da categoria de pagamento** (Activity +30 − 30 = 0). Juntando com o painel: **cada entrada no cartão que é transferência de conta orçamentária é uma saída em dinheiro da categoria de pagamento daquele cartão**. É o termo "pagamentos (saídas em dinheiro, ≤ 0)" da fórmula em `calculate_credit_card_payment_balance`.
- **Não verificado:**
  - ~~transferência do cartão para a corrente~~: **conferido (roteiro 5D)**, não volta para a categoria de pagamento e não muda o "A atribuir".
  - pagamento vindo de conta de acompanhamento: não é transferência entre orçamentárias, então pede categoria no cartão e não conta como pagamento no painel. Roteiro não incluído.
  - pagamento maior do que o disponível: **conferido (roteiro 5B e novembro)**, a categoria de pagamento fica negativa em vermelho (estouro em dinheiro) e isso desconta do "A atribuir" do mês seguinte.

## 5. Subtransações (divisão com transferência)

### Regras — visto (2327190, 2327706, 2329200, 2316535)

- Uma **parte** da divisão pode ser transferência. A espelho é uma transação inteira na outra conta, com `transfer_subtransaction_id` = a parte. O memo da espelho é o da parte.
- A transação principal **não pode** ser transferência entre orçamentárias e dividida ao mesmo tempo:
  - "This transaction is already a split transaction, so it can't be turned into a transfer…" (`OnBudgetTransferSplitError`);
  - "This transaction is already a transfer, so it cannot be split…" (`InvalidCategoryError`).
- A principal **pode** ser transferência para conta de acompanhamento e dividida entre categorias. Nesse caso nenhuma parte pode ser transferência ("This split transaction is already a transfer. Please remove the transfers from the sub-transactions.", `OffBudgetTransferSplitDupeError`).
- Na espelho de uma parte, o **valor não pode ser editado** ("There is an error in the transaction values.", `AmountEditedError`, `validateTransferToSubTransactionAmountNotModified`). O mesmo vale para a espelho de uma principal dividida (`validateTransferToSplitAmountNotModified`).
- A espelho de uma parte **não pode ser removida** (§2).
- Editar data ou conta na espelho de uma parte muda a data ou a conta da **divisão inteira** (§2), pelo código. **Na tela, a data da espelho não pode ser editada** (roteiro 6D).
- Divisão que fica com uma parte só vira transação simples (`unSplitIfNotReallyASplit`, 2331994).
- Uma parte pode ter categoria se a conta é orçamentária e a parte não é transferência entre orçamentárias (`canSubTransactionHaveSubCategory`, 1970373).

## 6. Agendadas e importação

### Transferência agendada — visto

- A agendada guarda **só uma ponta**, com `transfer_account_id` (`setTransferAccountId` em `createScheduledTransaction` e `updateScheduledTransaction`, 2334938). Não existe agendada espelho no banco nem no protocolo.
- Na outra conta, a tela **desenha** uma espelho só para exibir (`fabricateScheduledTransactionTransferCounterpart`, 1981756):
  - id = id da agendada + `_fabricatedOtherSideOfScheduledTransfer` (ou `…OfScheduledSubTransfer` para parte), 2505465;
  - valor = −valor, mesmo memo, mesma data, flag (só na principal), sem categoria, beneficiário = o de transferência da conta de origem;
  - marcada como não gravável (`setIsMergeable(false)`).
- Editar essa espelho desenhada edita a agendada de verdade (2859100). Remover a espelho desenhada de uma parte de divisão agendada é bloqueado.
- **Geração** (`createTransactionInstance`, 2302347; análise 02 §2): a transação gerada recebe o beneficiário da agendada e passa pelo mesmo salvamento, então a espelho nasce pela função `Q`. Os ids são determinísticos:

| Ponta | Id |
|---|---|
| gerada (lado da agendada) | `{agendada}_{data ISO}` (análise 02) |
| espelho da principal | `{agendada}_t_{data ISO}` (`getScheduledTransactionTransferTransactionId`, 2507247) |
| espelho de uma parte | `{agendada}_st_{índice da parte}_{data ISO}` |

  - A data usada no id da espelho é `dateEnteredFromSchedule`, que é a data da ocorrência mesmo no "enviar agora". Então **as duas pontas da geração são idempotentes**: dois aparelhos que gerarem a mesma ocorrência produzem os mesmos dois ids.
  - Aceite da espelho = aceite da gerada. Estado: não compensada (compensada em dinheiro vivo).

### Importada casada com transferência — visto (2308302, 2309300, 2314041)

- Casamento: mesma conta, **mesmo valor**, até **10 dias** de distância (`MATCH_MAX_DAYS_APART = 10`), a mais próxima, e **nunca as duas transferências** (`!o.isTransferTransaction() || !n.isTransferTransaction()`).
- A espelho recém-criada tenta casar com uma importada da conta dela (`attemptMatchToImportedTransaction`). Então, se a Poupança já importou o crédito de 200, a transferência lançada na Corrente casa com ele.
- Ao casar uma transferência do usuário com uma importada: o valor passa a ser o da importada **e a espelho recebe −esse valor**; a categoria da importada não é aplicada numa transferência entre orçamentárias; o estado sobe para compensada ou reconciliada conforme a importada. O estado da espelho não muda.
- Importada ainda crua com beneficiário de transferência não gera espelho: a função `Q` trata a fonte crua como "não é transferência" e limpa o beneficiário.
- Regra de renomear beneficiário que aponta para beneficiário de transferência só se aplica a transações da mesma conta (`handlePayeeNameChange`, 2336444).

## 7. Sincronização

### As pontas no protocolo — visto

- As duas pontas são **entidades separadas**, cada uma no seu `be_transaction_groups` (`entityToServerGroup`, 1912255): `{id, be_transaction, be_subtransactions}`. O servidor do Tostão desmonta os grupos em `be_transactions` e `be_subtransactions` (`src/server.rs`, `GROUP_KINDS`).
- O cliente cria e atualiza as duas pontas na **mesma** mudança local (`performAsSingleChangeSet` em `delete` e `approveTransaction`; a função `Q` roda no salvamento). **Inferido:** as duas vão no mesmo envio, porque o aparelho manda tudo o que mudou desde o último conhecimento (`get_changed_budget_entities_to_sync`, `src/sync.rs`).
- Não há operação de "transferência" no protocolo. **Não achado:** qualquer validação do par feita pelo servidor do YNAB.

### Dois aparelhos editando pontas diferentes — inferido a partir do `src/server.rs`

O servidor do Tostão guarda a **última versão inteira** de cada entidade e aplica o lote inteiro numa transação do banco. Como cada aparelho, ao editar qualquer ponta, regrava as duas, o resultado é este:

| Caso | O que acontece | Par fica coerente? |
|---|---|---|
| A edita o memo na ponta X; B edita o valor na ponta Y; A sincroniza antes | B manda X e Y inteiras, com o memo antigo; o memo de A some | sim, mas perde a edição de A |
| A edita só o estado de compensação de X; B edita Y | Y vem de B (estado de B); X vem de B, que regravou X com o estado antigo | sim, perde o estado de A |
| A remove X (remove Y junto); B edita Y depois, sem ter recebido a remoção | B manda X e Y sem a marca de removida: **a transferência volta** | sim, mas "ressuscita" |
| A troca o destino (Y vai para o Cartão); B edita Y ainda na Poupança | B manda Y na Poupança e X apontando para a Poupança: **a troca de destino se perde** | sim |
| B tem X mas não tem Y no banco local | o cliente cria uma Y nova com outro id (§2); o servidor fica com **duas espelhos** | **não**: a Y antiga continua apontando para X. No Tostão, recusado (decisão 5) |

Com as decisões da §11, no Tostão: a linha "A remove X" não ressuscita (decisão 4). As outras linhas continuam perdendo edições concorrentes, porque a última versão inteira vence (decisão 3).

- O `src/sync.rs` ignora a resposta de uma sincronização incremental se houve mudança local durante a requisição ("Incoming changes from delta sync were ignored…"). Isso não quebra o par, porque a próxima sincronização traz as duas pontas juntas.
- As duas pontas de um mesmo envio recebem o mesmo conhecimento do servidor, então `changed_since` as devolve juntas aos outros aparelhos. Isso já vale hoje no `src/server.rs`.

### O que o servidor do Tostão teria que garantir

1. **Lote atômico** (já faz): as duas pontas entram juntas ou nenhuma.
2. **Entregar as pontas juntas** (já faz): mesmo conhecimento para tudo o que veio no mesmo lote.
3. **Beneficiário de transferência por conta**: o aparelho manda a conta e o beneficiário no mesmo lote (decisão 2). O servidor não cria nada; só exige, na validação do par, que o beneficiário da outra conta esteja no lote.
4. **Invariantes do par**, recusados no lote inteiro se quebrados (decisão 1):
   - X aponta para Y ⇔ Y aponta de volta para X (por `transfer_transaction_id` ou `transfer_subtransaction_id`);
   - valores opostos; mesma data; conta de cada ponta = conta de transferência da outra;
   - beneficiário de cada ponta = beneficiário de transferência da conta da outra;
   - removida uma, removida a outra;
   - entre orçamentárias, nenhuma ponta com categoria; entre orçamentária e acompanhamento, categoria só na orçamentária.
5. **Cálculo**: como o servidor somos nós (análise 01 §1), o cálculo tem que:
   - ignorar transferências entre orçamentárias na receita e nas categorias;
   - contar as entradas no cartão vindas de conta orçamentária como pagamentos (saída da categoria de pagamento);
   - tratar a ponta orçamentária de uma transferência para acompanhamento como transação comum.
6. **Remoção vence** (decisão 4): o servidor guarda a marca de removida e recusa a regravação de uma versão não removida que chegue depois, quando o aparelho não conhecia a remoção.
7. **Última versão inteira vence** (decisão 3): sem mesclagem por campo. Uma edição concorrente em outro campo pode apagar a do outro aparelho; o servidor não tenta juntar.
8. **Espelho ausente** (decisão 5): o aparelho não salva uma ponta cuja espelho não está no banco local, então o servidor só recebe pares completos.

## 8. Resumo das regras

| Tema | Regra | Confiança |
|---|---|---|
| Modelo | Duas transações, cada uma com o id da outra; parte de divisão ↔ transação inteira | visto |
| Modelo | Agendada guarda uma ponta só; a outra é desenhada na tela (na outra conta e em "All Accounts") | visto + conferido na tela |
| Modelo | Beneficiário "Transfer : conta" define a transferência | visto |
| Modelo | Beneficiário de transferência criado com a conta | inferido (não achado no web) |
| Criação | Espelho: −valor, mesma data e memo, não compensada, aceite igual | visto |
| Edição | Valor, memo, data, flag e conta propagam; compensação e aceite não | visto + conferido na tela (valor, memo, data, compensação) |
| Edição | Trocar destino move a mesma espelho de conta | visto |
| Edição | Aprovar uma ponta aprova a outra | visto |
| Edição | Data futura converte a transferência em agendada e remove a espelho | visto + conferido na tela |
| Remoção | Remover uma ponta remove a outra; espelho de parte não pode ser removida | visto + conferido na tela (primeira parte) |
| Categoria | Entre orçamentárias: nenhuma ("Category not needed"); com acompanhamento: só na orçamentária | visto + conferido na tela (primeira parte) |
| Categoria | Categoria lançada na conta de acompanhamento vai para a ponta orçamentária | visto |
| A atribuir | Transferência entre orçamentárias não é receita | conferido na tela |
| Cartão | Entrada no cartão vinda de orçamentária = pagamento, sai da categoria de pagamento | visto + conferido na tela (análise 01, 2B) |
| Cartão | Escolher a categoria de pagamento vira transferência para o cartão | visto |
| Divisão | Principal não pode ser transferência entre orçamentárias e dividida | visto |
| Divisão | Valor da espelho de uma parte não pode ser editado | visto |
| Agendada | Ids da geração: `{ag}_{data}`, `{ag}_t_{data}`, `{ag}_st_{i}_{data}` | visto |
| Importação | Casa por valor igual em até 10 dias; nunca transferência com transferência | visto |
| Sincronização | Pontas são entidades separadas; mesma mudança local | visto + inferido |

## 9. Roteiros para conferir na tela

Orçamento de teste novo, mês de outubro:
- **Corrente** (orçamentária) com saldo inicial de 1.000, classificado como "Pronto para atribuir" (receita);
- **Poupança** (orçamentária), saldo 0;
- **Cartão** (cartão de crédito, orçamentária), saldo 0;
- **Investimento** (acompanhamento / "tracking"), saldo 0;
- categorias: Mercado, Investimentos.

No início: "A atribuir" = 1.000.
Valores esperados marcados com "?" são inferidos; os outros seguem do código lido. Anote o que a tela mostrar quando for diferente.

### Roteiro 3 — entre contas orçamentárias

| Passo | Ação | Corrente | Poupança | A atribuir | O que olhar |
|---|---|---|---|---|---|
| 3A | Na Corrente, saída de 200 com beneficiário "Transfer : Poupança", memo "reserva" | 800 | 200 | 1.000 | Na Poupança aparece a entrada de 200, beneficiário "Transfer : Corrente", memo "reserva", **não compensada**. Nenhuma das duas tem campo de categoria. |
| 3B | Na **Poupança**, muda o valor para 250 e o memo para "reserva 2" | 750 | 250 | 1.000 | A ponta da Corrente vira saída de 250 com memo "reserva 2". |
| 3C | Na Poupança, marca a ponta como compensada | 750 | 250 | 1.000 | A ponta da Corrente **continua não compensada**. |
| 3D | Na Corrente, muda a data da ponta para **3 dias antes** (data passada) | 750 | 250 | 1.000 | A data da ponta da Poupança acompanha. |
| 3E | Na Poupança, apaga a ponta | 1.000 | 0 | 1.000 | A ponta da Corrente some junto. |
| 3F | Refaz a transferência e muda a data da ponta da Corrente para uma data **futura** | 1.000 | 0 | 1.000 | A transação vira agendada na Corrente e a ponta da Poupança some. |

Nenhuma categoria muda em nenhum passo.

**Conferência na tela (2026-10-13), roteiro 3 inteiro:**
- 3A: Corrente 800, Poupança 200, "A atribuir" 1.000. **Conferido.**
- 3B: a ponta da Poupança ficou com 250 e memo "reserva 2", categoria "Category not needed". A Corrente foi para 750 e o "A atribuir" continuou 1.000. **Conferido**: valor e memo propagam da espelho para a origem.
- 3C: a ponta da Poupança ficou compensada (saldo compensado da Poupança 250). Na Corrente, a ponta continuou **não compensada** (compensado 1.000, não compensado −250). **Conferido**: o estado de compensação não propaga.
- 3D, feito com data **futura** (de 13/10 para 18/10): a transação da Corrente virou **agendada** (250, "Transfer: Poupança", memo "reserva 2", "Category not needed"). A ponta da Poupança sumiu, e a Corrente voltou a 1.000 e a Poupança a 0. **Conferido**: é a conversão em agendada da §2 (`convertToScheduledTransaction`), que remove a espelho. O roteiro foi corrigido para usar data passada no 3D, e o caso futuro virou o 3F.
- A espelho desenhada da agendada (§6) **não aparece** na tela da conta Poupança, mas **aparece em "All Accounts"**: Poupança, 18/10, entrada de 250, "Transfer: Conta Corrente", memo "reserva 2", "Category not needed". **Conferido.** Bate com o código, que a desenha só na lista de todas as contas (2920700). **Corrigido no roteiro 7:** lá a espelho apareceu também na tela da conta Poupança. No roteiro 3, talvez faltasse recarregar a tela.
- 3D (refeito com transferência nova, data passada): mudar a data da ponta da Corrente para 10/10 levou a ponta da Poupança para 10/10. A ponta da Poupança continuou compensada. **Conferido.**
- 3E: apagar a ponta da Poupança apagou a da Corrente. **Conferido** (relato do usuário, sem captura).

### Roteiro 4 — orçamentária e acompanhamento

Antes: atribui 300 a Investimentos ("A atribuir" 700).

| Passo | Ação | Corrente | Investimento | Investimentos (disponível) | A atribuir | O que olhar |
|---|---|---|---|---|---|---|
| 4A | Na Corrente, saída de 300 para "Transfer : Investimento", categoria Investimentos | 700 | 300 | 0 | 700 | Na conta Investimento, a entrada de 300 **não tem** campo de categoria. Activity de Investimentos: −300. |
| 4B | Na conta **Investimento**, saída de 100 para "Transfer : Corrente" | 800 | 200 | 0 | 700 ? | Na Corrente, a entrada de 100 aparece **sem categoria**, pedindo categoria. Anotar se o "A atribuir" mudou. |
| 4C | Na Corrente, classifica essa entrada de 100 como "Pronto para atribuir" | 800 | 200 | 0 | 800 | Vira receita do mês. |
| 4D | Na conta Investimento, apaga a ponta do 4B | 700 | 300 | 0 | 700 | A entrada da Corrente some junto, e a receita de 100 sai do "A atribuir". |
| 4E | Na Corrente, abre a ponta do 4A e tenta apagar a categoria; salva | 700 | 300 | 300 ? | 700 | A ponta vira "sem categoria" (−300). Esperado: Investimentos volta a 300 e aparece a linha "sem categoria". Em novembro, "A atribuir" cai 300 (estouro em dinheiro, análise 01 §2) ? |

**Conferência na tela (2026-10-13), roteiro 4.** Os 300 **não** foram atribuídos a Investimentos antes. Por isso o "A atribuir" ficou 300 acima da tabela e Investimentos ficou estourada.
- 4A: Corrente 700 e Investimento 300. A ponta da Corrente tem a categoria "Mensal: Investimento". A tela da conta Investimento **nem tem coluna de categoria**. No plano, Investimentos ficou com atividade −300 e disponível −300 (estourada), e o "A atribuir" continuou 1.000. **Conferido**: a ponta orçamentária é um gasto comum da categoria.
- 4B: Corrente 800 e Investimento 200. Na Corrente, a entrada de 100 apareceu com "This needs a category" e o aviso "1 new transaction to approve or categorize". No plano apareceu a linha "Uncategorized Transactions" com atividade +100 e disponível +100, e o **"A atribuir" continuou 1.000**. **Conferido**: entrada sem categoria vinda de acompanhamento **não** entra no "A atribuir". Ela fica parada na linha "sem categoria", com saldo positivo.
- 4C (refeito): uma saída de 100 do Investimento para a Corrente, classificada na Corrente como "Inflow: Ready to Assign", levou o "A atribuir" de outubro a **1.100**. O Investimento ficou em −100. **Conferido**: a entrada vinda de acompanhamento com categoria de receita é receita do mês.
- 4D: apagar a ponta na conta Investimento apagou a da Corrente (Corrente 700, Investimento 300). **Conferido.**
- 4E: tirar a categoria da ponta do 4A deixou "This needs a category" (−300) e manteve Corrente 700. O plano depois disso não foi visto. **Inferido** pela regra já conferida na análise 01 (passo 1B: saída sem categoria em conta de dinheiro é estouro em dinheiro e desconta do "A atribuir" do mês seguinte). Não há indício de que a transferência mude essa regra.
- As pontas criadas pelos dois lados nasceram **não compensadas**.

### Roteiro 5 — cartão de crédito

Orçamento novo (ou desfaça os roteiros anteriores): Corrente 1.000; atribui 100 a Mercado ("A atribuir" 900).

| Passo | Ação | Corrente | Cartão | Mercado | Pagamento (Activity / disponível) | A atribuir | O que olhar |
|---|---|---|---|---|---|---|---|
| 5A | Gasta 60 no Cartão, em Mercado | 1.000 | −60 | 40 | +60 / 60 | 900 | (igual à análise 01, 2A) |
| 5B | **No Cartão**, entrada de 60 com beneficiário "Transfer : Corrente" | 940 | 0 | 40 | 0 / 0 | 900 | A ponta da Corrente nasce sem categoria. O pagamento lançado pelo lado do cartão conta igual. |
| 5C | Na Corrente, nova saída de 10, sem beneficiário, escolhendo como **categoria** "Pagamento do Cartão" | 930 | +10 | 40 | −10 / −10 ? | 900 | O beneficiário vira "Transfer : Cartão" e a categoria some ao salvar. A categoria de pagamento fica negativa (pagou sem dinheiro reservado). Anotar o "A atribuir" de **novembro**: esperado 890 ? |
| 5D | No Cartão, saída de 10 para "Transfer : Corrente" (devolve o crédito) | 940 | 0 | 40 | 0 / 0 ? | 900 | Anotar se a saída do cartão volta para a categoria de pagamento (Activity) ou fica fora. |

**Conferência na tela (2026-10-13), roteiro 5.** Os 100 **não** foram atribuídos a Mercado antes. Por isso o gasto de 60 virou estouro no crédito e o pagamento ficou sem dinheiro reservado.

| Passo | Corrente | Cartão | Mercado | Pagamento (atividade / disponível) | A atribuir |
|---|---|---|---|---|---|
| 5A | 1.000 | −60 | −60 (amarelo, crédito) | 0 / 0 (amarelo) | 1.000 |
| 5B | 940 | 0 | −60 | −60 / −60 (vermelho) | 1.000 |
| 5C | 930 | +10 | −60 | −70 / −70 | **1.010** |
| 5D | 940 | 0 | −60 | −70 / −70 | 1.010 |

- **Nome na tela**: o YNAB mostra "**Payment:** (conta)" nas entradas no cartão vindas da corrente, e "Payment: Cartão" na ponta da corrente. Na saída do cartão para a corrente (5D), mostra "**Transfer:** Conta Corrente". Isso bate exatamente com `isCreditCardPaymentTransfer` (§4): entrada no cartão ou saída para o cartão é pagamento; saída do cartão não é. **Conferido.**
- A coluna de categoria mostra "Credit Card Payments: Cartão" nas duas pontas, inclusive na saída do 5D. Pelo código é só exibição (§4, 2852178). **Inferido**, não dá para ver o campo gravado.
- 5B: o pagamento lançado **pelo lado do cartão** contou como pagamento (atividade −60). Como o gasto não estava coberto, a categoria de pagamento ficou negativa em **vermelho** (estouro em dinheiro), e o cabeçalho do cartão mostrou "Payment −60". **Conferido**: pagamento sem dinheiro reservado = estouro em dinheiro da categoria de pagamento.
- 5C: escolher a categoria de pagamento numa saída da corrente sem beneficiário virou "Payment: Cartão". **Conferido.** O cartão ficou com saldo positivo de 10 e o "A atribuir" de outubro **subiu para 1.010**. **Não explicado**: não está no bundle (é soma do servidor).
- 5D: a saída de 10 do cartão para a corrente **não mexeu** na categoria de pagamento (−70) nem no "A atribuir" (1.010). **Conferido.**
- Nos quatro passos vale a conta "dinheiro nas contas orçamentárias = A atribuir + disponível das categorias sem estouro no crédito": 940 = 1.000 − 60 (5B), 940 = 1.010 − 70 (5C e 5D). O estouro no crédito de Mercado (−60) fica de fora. Pode ser a pista para o +10, mas é só uma conta que fecha.
- **Novembro** (estado do 5D): "A atribuir" **940** = 1.010 − 70. Pagamento e Mercado começam em 0. **Conferido**: o estouro em dinheiro da categoria de pagamento (−70) desconta do "A atribuir" do mês seguinte, como na análise 01. O estouro no crédito de Mercado (−60) não desconta. O +10 de outubro some na virada: em novembro, o "A atribuir" é igual ao dinheiro das contas (940). A regra do +10 continua não explicada, mas o efeito dela termina no mês seguinte.

### Roteiro 6 — divisão com transferência

Orçamento do roteiro 3 no início (Corrente 1.000, Poupança 0), 100 atribuídos a Mercado ("A atribuir" 900).

| Passo | Ação | Corrente | Poupança | Mercado | A atribuir | O que olhar |
|---|---|---|---|---|---|---|
| 6A | Na Corrente, saída de 100 dividida: 70 em Mercado e 30 com beneficiário "Transfer : Poupança" (memo "parte") | 900 | 30 | 30 | 900 | Na Poupança, uma transação **simples** de 30, memo "parte". |
| 6B | Na Poupança, tenta mudar o valor da ponta para 40 | 900 | 30 | 30 | 900 | Erro "There is an error in the transaction values." |
| 6C | Na Poupança, tenta apagar a ponta | 900 | 30 | 30 | 900 | Erro "…transfers to a sub-transaction and cannot be deleted." |
| 6D | Na Poupança, muda a data da ponta | 900 | 30 | 30 | 900 | A transação dividida da Corrente **inteira** muda de data. |
| 6E | Na Corrente, apaga a transação dividida | 1.000 | 0 | 100 | 900 | A ponta da Poupança some. |
| 6F | Na Corrente, tenta criar uma transação com beneficiário "Transfer : Poupança" e dividi-la | — | — | — | — | Erro: não pode ser transferência e dividida. |

**Primeira tentativa na tela (2026-10-13), a refazer.** A Corrente mostrou duas linhas com data, caixa de seleção e memo "parte": Mercado 70 e "Transfer: Poupança" 30. Não apareceu uma linha principal de divisão ("Split"). Os saldos bateram com o 6A: Corrente 900, Poupança 30 (memo "parte" copiado), Mercado 30, "A atribuir" 900. Mas, na Poupança:
- mudar o valor para 40 **funcionou** (Poupança 40, Corrente 890);
- apagar a ponta **funcionou** (Poupança 0, Corrente 930, Mercado continuou com 70 gastos).

Isso contradiz os bloqueios lidos no código (§5). A explicação mais provável é que foram lançadas **duas transações separadas**, e não uma divisão. Com a transferência numa transação comum, editar e apagar pela outra ponta é o esperado (roteiro 3). Corrente 890 depois do 6B também aponta para isso: numa divisão de 100, o valor total não mudaria só pela ponta da Poupança. Refazer o 6A com o botão de divisão ("Split") na própria transação.

**Segunda tentativa, com divisão (2026-10-13):**
- 6A: a Corrente mostrou a linha principal "Split (Multiple Categories)…" de 100, com as partes "Mensal: Mercado" 70 e "Transfer: Poupança" 30 ("Category not needed"). Corrente 900, Poupança 30. **Conferido.**
- 6B: na Poupança, o valor da ponta **não pode ser alterado**. **Conferido.**
- 6C: na Poupança, a ponta **não pode ser apagada**. **Conferido.**
- 6D: na Poupança, a **data também não pode ser mudada**. É diferente do que eu esperava pelo código: a função `Q` tem o caminho que mudaria a data da divisão inteira, mas a tela bloqueia a edição antes. Para o Tostão, vale a tela: a espelho de uma parte só muda pelo lado da divisão.
- 6F: transação com beneficiário "Transfer : Poupança" e dividida **não é aceita**. **Conferido.**
- Editando a divisão na Corrente, ao tirar a parte de 30, a soma das partes (70) ficou diferente do total (100). A tela abriu "Something Doesn't Add Up", com as opções "Auto-Distribute", "Update Total" e "Adjust Manually". No bundle lido só aparece o erro de soma (`validateTransactionAmountMatchesSubTransactionsAmount`); essas opções devem ser de outro chunk ou de versão mais nova. O resultado depois de salvar (a Poupança voltar a 0) ainda não foi visto.

### Roteiro 7 — agendada

| Passo | Ação | Corrente | Poupança | O que olhar |
|---|---|---|---|---|
| 7A | Na Corrente, agendada mensal de 50 para "Transfer : Poupança", data de amanhã | 1.000 | 0 | Na Poupança aparece uma agendada de +50 sem categoria. |
| 7B | Na Poupança, abre essa agendada e muda o memo | 1.000 | 0 | O memo muda na agendada da Corrente (só existe uma). |
| 7C | Na Corrente, "Enter now" (lançar agora) na agendada | 950 | 50 | As duas pontas nascem; a da Poupança não compensada. |
| 7D | Se alguma ponta vier "a aprovar", aprova só uma | 950 | 50 | A outra também fica aprovada. |

**Conferência na tela (2026-10-13), roteiro 7** (agendada mensal de 50 com data 14/10, um dia depois de hoje):
- 7A: na Corrente, a agendada "Transfer: Poupança" de 50 e "Category not needed". A Corrente continuou em 1.000 e o "A atribuir" em 900 (100 em Mercado). Em "All Accounts" e **na tela da Poupança** apareceu a espelho: 14/10, entrada de 50, "Transfer: Conta Corrente", com o botão "Enter Now". **Conferido.**
- 7B: o memo "oi", digitado na espelho da Poupança, apareceu na agendada da Corrente. **Conferido**: editar a espelho desenhada edita a agendada real.
- 7C: "Enter Now" criou as duas pontas com a **data de hoje (13/10)**, não a da ocorrência (14/10), com memo "oi" e não compensadas. Corrente 950, Poupança 50. A agendada andou para 14/11, e a espelho de 14/11 aparece na Poupança. **Conferido** (`fireOnceWithTodaysDate`).
- 7D: as pontas do "Enter Now" já vieram aprovadas, então não houve o que aprovar. A regra "aprovar uma aprova a outra" fica só **vista** no código. Para conferir, seria preciso uma geração automática (esperar a data).

### Roteiro 8 — o +10 do pagamento além da dívida

**Para que serve.** No roteiro 5C, pagar o cartão além da dívida aumentou o "A atribuir" em 10. O motor usa uma regra inferida para isso (`pagamento_alem_da_divida`, §12): *a parte do pagamento que deixa o cartão com saldo positivo vai para o "A atribuir" do mês*. Este roteiro confere a regra e a separa de três alternativas:
- **por ordem**: o +10 conta na hora do pagamento e nunca volta, mesmo se o cartão gastar depois;
- **só o mês**: o pagamento é comparado só com os gastos do mês, sem a dívida do mês anterior;
- **só pagamento**: um estorno no cartão nunca vai para o "A atribuir".

**Quem paga quem.** O dinheiro do pagamento **sai da Conta Corrente** e **entra no Cartão**. Todo pagamento é lançado **na Conta Corrente**, com o beneficiário de transferência do Cartão. Nunca lance o pagamento como saída comum: sem o beneficiário de transferência, o YNAB trata como gasto sem categoria (foi o que aconteceu na primeira tentativa, no fim deste roteiro).

Todas as datas de outubro são **13/10 (hoje)**. Data futura vira agendada (roteiro 3F).

#### Ponto de partida

**Como saber se o pagamento virou transferência:** o saldo do **Cartão** muda junto. Depois do 8B, o Cartão vai de −50 para 0 **com o gasto de setembro ainda lá**. Se o Cartão não mudar, ou se a linha "Uncategorized Transactions" de outubro aparecer, o pagamento não é transferência: apague e lance de novo.

O orçamento TESTE como está:
- Conta Corrente com saldo inicial de 1.000 (13/10, "Inflow: Ready to Assign");
- Cartão com um gasto de 50 em Mercado em 15/09;
- setembro com 50 atribuídos a Mercado.

**Passo 0 — voltar ao ponto de partida.**
1. Na **Conta Corrente**: apague as saídas de 13/10 **sem beneficiário** (50 e 10, "This needs a category"). Na Corrente deve sobrar só o "Starting Balance" de 1.000.
2. No **Cartão**: apague tudo de outubro. Se o gasto de **15/09, 50, Mercado** não estiver lá, lance de novo (Add Transaction no Cartão, data 15/09/2026, Category Mercado, Outflow 50). No Cartão deve sobrar só esse gasto.
3. No plano de **outubro**: atribuído de Mercado = 0. Em setembro, Mercado continua com 50 atribuídos.

Confira:

| Onde | Esperado |
|---|---|
| Conta Corrente | 1.000 |
| Cartão | −50 |
| Outubro, Pagamento do Cartão (Activity / Available) | 0 / 50 |
| Outubro, Mercado | 0 |
| "Ready to Assign" de outubro / novembro | 950 / 950 |
| Setembro | −50 ("You assigned more than you have"); não muda nos passos seguintes |

#### Como lançar um pagamento (passos 8B e 8C)

1. Abra a **Conta Corrente** e clique em **Add Transaction**.
2. **Date**: 13/10/2026.
3. **Payee**: digite "Cartão" e escolha a opção da lista de **transferências** ("Transfer : Cartão" ou "Payment: Cartão"). Não escolha "Create payee".
4. **Category**: deixe em branco. Depois de escolher o beneficiário, deve aparecer "Category not needed".
5. **Outflow**: o valor do passo. Salve.

Antes de anotar, confira:
- na Conta Corrente, a linha mostra "Payment: Cartão" e **não** mostra "This needs a category";
- abrindo o **Cartão**, aparece a entrada do mesmo valor em 13/10, com beneficiário "Payment: Conta Corrente" (ou "Transfer : Conta Corrente").

Se não aparecer no Cartão, apague e lance de novo. Outra forma: abrir o **Cartão** e usar o botão **Record Payment**, escolhendo a Conta Corrente como origem.

#### Passos

Em cada passo, anote: saldo da Conta Corrente e do Cartão; Activity e Available do Pagamento do Cartão em **outubro** (e a cor); "Ready to Assign" de **outubro** e de **novembro**.

**8B — paga a dívida de setembro.** Pagamento de **50** (na Conta Corrente, Payee "Transfer : Cartão", Outflow 50).

| Corrente | Cartão | Pagamento out (Activity / Available) | Ready to Assign out / nov | Se der… |
|---|---|---|---|---|
| 950 | 0 | −50 / 0 | 950 / 950 | out 1.000 → vale **só o mês** |

**8C — paga 10 a mais do que deve.** Outro pagamento, de **10**, igual ao 8B.

| Corrente | Cartão | Pagamento out | Ready to Assign out / nov | Se der… |
|---|---|---|---|---|
| 940 | +10 | −60 / −10 (vermelho) | **960** / 950 | out 950 → o +10 não existe com dívida anterior |

**8D — gasta o crédito.** No plano de outubro, atribua **10** a Mercado. Depois, no **Cartão**: Add Transaction, data 13/10, Payee qualquer (ex.: "Mercado X"), **Category: Mercado**, **Outflow 10**.

| Corrente | Cartão | Mercado out | Pagamento out | Ready to Assign out / nov | Se der… |
|---|---|---|---|---|---|
| 940 | 0 | 0 | −50 / 0 | **940** / 940 | out 950 → vale **por ordem** |

A atribuição sozinha leva o "Ready to Assign" de 960 a 950; o gasto, pela regra, leva a 940.

**8E — estorno no cartão.** No **Cartão**, apague o gasto do 8D. Lance no **Cartão**: data 13/10, Payee qualquer, **Category: Mercado**, **Inflow 20** (entrada, não saída). Mercado continua com 10 atribuídos.

| Corrente | Cartão | Mercado out | Pagamento out | Ready to Assign out / nov | Se der… |
|---|---|---|---|---|---|
| 940 | +30 | 30 | −80 / −30 | **970** / 940 | out 960 → vale **só pagamento** |

**8F — gasto sem dinheiro.** No **Cartão**, apague o estorno do 8E. No plano de outubro, atribuído de Mercado = **0**. Lance no **Cartão**: data 13/10, Payee qualquer, **Category: Mercado**, **Outflow 10**.

| Corrente | Cartão | Mercado out | Pagamento out | Ready to Assign out / nov | Se der… |
|---|---|---|---|---|---|
| 940 | 0 | −10 (amarelo) | −60 / −10 | **950** / 940 | out 960 → vale **por ordem** |

Novembro é sempre o "Ready to Assign" de outubro mais o disponível negativo (vermelho) do Pagamento em outubro. Ex.: 8C, 960 − 10 = 950. Se outubro bater e novembro não, o erro está na virada, não na regra. Se outubro não bater com nenhuma coluna, anote o valor: ele ajuda a achar a regra certa.

#### Terceira tentativa (2026-10-13)

- Passo 0: Corrente 1.000, Cartão −50, setembro com Pagamento +50 / 50 e "A atribuir" −50, outubro com Pagamento 0 / 50 e "A atribuir" 950. **Conferido.**
- 8B (pagamento de 50 como transferência): Corrente 950, Cartão 0, Pagamento de outubro −50 / 0, "A atribuir" 950 em outubro e em novembro; setembro não mudou. **Conferido.** A alternativa **só o mês** (1.000) está descartada: o pagamento da dívida de setembro não vai para o "A atribuir".
- 8C (mais 10 da Corrente para o Cartão): Corrente 940, Cartão +10, Pagamento de outubro −60 / −10 (vermelho), "A atribuir" **960** em outubro e 950 em novembro; setembro não mudou. **Conferido.** O +10 do 5C vale também com dívida do mês anterior: só a parte do pagamento que passa da dívida vai para o "A atribuir". Teste `roteiro_8_pagamento_alem_da_divida_do_mes_anterior`.
- 8D (atribui 10 a Mercado e gasta 10 no Cartão em Mercado): Corrente 940, Cartão 0, Mercado −10 / 0, Pagamento −50 / 0, "A atribuir" **940** em outubro e em novembro. **Conferido.** A alternativa **por ordem** (950) está descartada: gastar o crédito no mesmo mês desfaz o +10.
- 8E (apaga o gasto do 8D; estorno de 20 no Cartão em Mercado): Corrente 940, Cartão +30, Mercado +20 / 30, Pagamento −80 / −30 (vermelho), "A atribuir" **970** em outubro e 940 em novembro (Mercado 30 e Pagamento 0 em novembro). **Conferido.** A alternativa **só pagamento** (960) está descartada.
- Com o 8E, os três estornos vistos fecham numa regra (`estorno_limitado` em `src/motor.rs`): **o estorno coberto só sai da categoria de pagamento na parte que reduz a dívida do cartão ou que vai para o "A atribuir"**; a parte que só deixa o cartão positivo fica na categoria do estorno. Primeira tentativa: reduziu a dívida, saiu 20. Segunda: cartão em 0 e sem pagamento no mês, não saiu nada. Terceira: depois de pagar além da dívida, saiu 20 e foi para o "A atribuir". O teste da segunda tentativa deixou de ser `#[ignore]`.
- 8F (apaga o estorno; Mercado com 0 em outubro; 10 no Cartão em Mercado, sem dinheiro): Corrente 940, Cartão 0, Mercado −10 (amarelo, crédito), Pagamento −60 / −10 (vermelho), "A atribuir" **950** em outubro e 940 em novembro. **Conferido.** Gastar o crédito sem dinheiro na categoria também desfaz o +10.
- **Roteiro 8 concluído**: as previsões da regra bateram nos seis passos, e as três alternativas foram descartadas.

#### Segunda tentativa (2026-10-13)

Os pagamentos **de novo não viraram transferência**: o Cartão ficou em 0 e a linha "Uncategorized Transactions" de outubro ficou em −50 e −60. O gasto de setembro tinha sido apagado (Mercado começou outubro com 50 e o Cartão com 0). O +10 continua **não testado**. Bate com o motor (teste `roteiro_8_segunda_tentativa_sem_transferencia`):
- 8D: atribuir 10 e gastar 10 no cartão em Mercado: Mercado −10 / 50, Pagamento +10 / 10, cartão −10, "A atribuir" 940;
- 8F como lançado (sem atribuição em outubro, 10 no cartão em Mercado, coberto pela sobra de setembro): Mercado −10 / 40, Pagamento +10 / 10, "A atribuir" 950.

**Divergia do motor — 8E** (resolvido na terceira tentativa, acima): estorno de 20 em Mercado num cartão **sem dívida** (saldo 0, Pagamento 0 / 0). Na tela: Mercado +20 / 80, cartão **+20**, "A atribuir" 940, e o **Pagamento ficou 0 / 0**. O motor tira os 20 do Pagamento (−20 / −20), como na primeira tentativa, em que o cartão tinha dívida de 50 e o Pagamento foi de 50 para 30. Teste `roteiro_8e_estorno_com_cartao_sem_divida`, marcado `#[ignore]` até achar a regra. Duas explicações cabem nos dois casos:
- o estorno só tira da categoria de pagamento o que ela tem disponível (não a deixa negativa);
- o estorno só tira da categoria de pagamento até o tamanho da dívida do cartão.

Na tela, o dinheiro fechou: Corrente 940 + Cartão 20 = 960 = "A atribuir" 940 + Mercado 80 + Pagamento 0 + sem categoria −60.

#### Primeira tentativa

**Primeira tentativa na tela (2026-10-13), a refazer.** Os "pagamentos" do 8B e do 8C ficaram como saídas da Corrente **sem beneficiário e sem categoria** ("This needs a category"), e não como transferência; a conta Cartão não recebeu nada. O gasto do 8F também ficou sem categoria. Então o +10 **não foi testado**. O que a tela mostrou bate com o motor (teste `roteiro_8_primeira_tentativa_sem_transferencia`):
- setembro: Mercado 50 gastos no cartão, Pagamento +50 / 50, "A atribuir" −50 (saldo inicial ficou em 13/10); outubro herda 50 no Pagamento;
- saídas sem categoria na Corrente (−50, depois −60): "A atribuir" de outubro continua 950;
- 8D: atribuir 10 e gastar 10 no cartão em Mercado: Pagamento +10 / 60, "A atribuir" 940;
- 8E: estorno de 20 no cartão em Mercado: Mercado +20 / 30, **Pagamento −20 / 30**, cartão −30, "A atribuir" 940. **Conferido**: o estorno coberto no cartão sai da categoria de pagamento (`budgeted_spending` positivo);
- 8F como lançado (10 sem categoria no cartão): sem categoria −70, Pagamento 0 / 50 (amarelo), cartão −60, "A atribuir" 950.

## 10. Implicações para o Tostão

- **Modelo**: manter o do YNAB (duas transações com id cruzado; parte de divisão ↔ transação inteira; agendada com uma ponta só). Ele já passa pelo protocolo sem mudança e o `src/server.rs` guarda tudo sem conhecer transferências.
- **Núcleo compartilhado**: precisamos de uma função "salvar transação" que faça o papel da `Q` (criar, propagar, mover, remover a espelho), e que crie o beneficiário de transferência junto com a conta (decisão 2). Ela é do aparelho, não do servidor, para funcionar offline. Ela recusa salvar uma ponta cuja espelho não está no banco local (decisão 5) e não copia o estado de compensação (decisão 6).
- **Cálculo**: feito em `src/motor.rs` (`classificar` e `calcular`), que classifica cada transação ou parte (transferência entre orçamentárias fora da receita e das categorias, entrada no cartão como pagamento, ponta orçamentária de transferência com acompanhamento como transação comum) e chama as regras de `src/calc.rs`. Os roteiros 3 a 7 estão refeitos a partir de transações em `tests/motor.rs`.
- **Ids determinísticos** das pontas geradas pela agendada (`{ag}_t_{data}`): reproduzir, pelo mesmo motivo da análise 02 (evitar duplicata entre aparelhos).
- **Sincronização** (decisões 1, 3, 4 e 5): fica "última versão inteira vence", com a remoção vencendo. O aparelho manda as duas pontas juntas e o servidor recusa o lote que deixa o par quebrado. Edições concorrentes em campos diferentes ainda se perdem; é o custo aceito pela família. O caso perigoso de antes (criar espelho nova por não ter a antiga) é recusado no aparelho.
- **Testes** antes de implementar (em `tests/`): criação, edição de cada campo da tabela da §2, troca de destino, remoção pelos dois lados, divisão, agendada com ids, dois aparelhos editando pontas diferentes contra o `SyncServer` em memória, e também: lote que quebra o par é recusado inteiro (decisão 1), remoção vence edição atrasada (decisão 4), salvar ponta sem espelho local é recusado (decisão 5) e compensação não propaga (decisão 6).

## 11. Decisões

Fechadas com o usuário em 2026-10-13. Nenhuma ficou adiada.

1. ~~**O servidor valida o par?**~~ **Decidido (b): o servidor recusa o lote que quebra os invariantes da §7.** Motivo: estado ruim nunca entra no banco, e o aparelho pode tentar de novo.
2. ~~**Quem cria o beneficiário de transferência**~~ **Decidido: o aparelho, no mesmo lote da conta.** Motivo: o servidor continua só guardando, e a decisão 1 precisa que o beneficiário esteja no lote para validar a ponta.
3. ~~**Conflitos de edição**~~ **Decidido: a última versão inteira vence.** Motivo: para uso da família, começar simples; a perda de memo ou de estado numa edição concorrente é aceita.
4. ~~**Remoção contra edição**~~ **Decidido: a remoção vence.** Motivo: a edição feita offline numa transferência já apagada some, e o aparelho atrasado só recebe a remoção. Depende da 3: o servidor precisa guardar a marca de removida para recusar a regravação.
5. ~~**Espelho que não está no banco local**~~ **Decidido: recusar o salvamento até sincronizar.** Motivo: nunca nascem duas espelhos, o que a decisão 1 não aceitaria de qualquer forma.
6. ~~**Estado de compensação não propaga**~~ **Decidido: não propaga (reproduz o YNAB).** Motivo: cada ponta é conferida contra o extrato da sua conta, e o comportamento já foi visto na tela (roteiro 3C).
7. ~~Editar a espelho de uma parte muda a data da divisão inteira~~: resolvido pela tela (roteiro 6D). O YNAB bloqueia a edição da data na espelho; o Tostão faz o mesmo.

## 12. O que ainda não achamos

- Onde o beneficiário de transferência é criado (provavelmente no servidor do YNAB).
- Quem chama `setTransferCounterpartClearedStatus` e `getOtherSideReconciledTransfers` (talvez outro chunk do bundle).
- O caso "B existe mas não aponta de volta para A": quando acontece.
- A soma do servidor para receita e categorias com transferências (só inferida pelos painéis do cliente).
- ~~Se uma entrada sem categoria vinda de acompanhamento entra no "A atribuir"~~: não entra, conferido no roteiro 4B. Ela fica na linha "sem categoria" com saldo positivo. Falta saber o que acontece com esse saldo positivo na virada do mês.
- Por que pagar o cartão além da dívida (saldo do cartão positivo) aumentou o "A atribuir" em 10 (roteiro 5C), e por que ele não voltou quando o crédito saiu do cartão (5D).
  - **Conferido no roteiro 8** (8B a 8E): o pagamento da dívida do mês anterior não soma; o que passa da dívida soma; gastar o crédito no mesmo mês desfaz; estorno depois do excesso soma; gasto sem dinheiro desfaz (8F).
- Estorno no cartão: só sai da categoria de pagamento na parte que reduz a dívida ou vai para o "A atribuir" (roteiro 8, três tentativas; `estorno_limitado`).
  - Regra usada em `src/motor.rs` (`pagamento_alem_da_divida`): a parte dos pagamentos do mês que deixa o cartão com saldo positivo, além do saldo positivo que ele já tinha no fim do mês anterior, vai para o "A atribuir" do mês (`additional_to_be_budgeted`). Ela continua saindo da categoria de pagamento. Os gastos do cartão contam; as transferências que saem dele para conta orçamentária não. Fecha com 5A–5D e novembro, mas só esse caso foi visto. Falta conferir na tela: pagar a mais com dívida de mês anterior, e gastar no cartão com saldo positivo.
- Os nomes internos `transfer_on_budget`, `transfer`, `transfer_off_budget` (66689): onde são usados.
- Regras de empréstimo (`setDebtTransactionTypeIfNeeded`, categoria da dívida).
- Se o servidor do YNAB valida ou conserta pares quebrados.
