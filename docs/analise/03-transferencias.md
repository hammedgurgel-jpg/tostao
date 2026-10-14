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
- **Conferido na tela (roteiro 9B):** esse saldo positivo **passa para o mês seguinte** na própria linha "sem categoria", como numa categoria. Saídas sem categoria no mês seguinte gastam dele antes de estourar.

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
- 4E: tirar a categoria da ponta do 4A deixou "This needs a category" (−300) e manteve Corrente 700. O plano depois disso não foi visto aqui; foi **conferido no roteiro 9D** (estouro em dinheiro que desconta do mês seguinte). Antes, inferido pela regra já conferida na análise 01 (passo 1B: saída sem categoria em conta de dinheiro é estouro em dinheiro e desconta do "A atribuir" do mês seguinte). Não há indício de que a transferência mude essa regra.
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
- Com o 8E, os três estornos vistos fecham numa regra (`gasto_coberto_limitado` em `src/motor.rs`, antes `estorno_limitado`): **o estorno coberto só sai da categoria de pagamento na parte que reduz a dívida do cartão ou que vai para o "A atribuir"**; a parte que só deixa o cartão positivo fica na categoria do estorno. Primeira tentativa: reduziu a dívida, saiu 20. Segunda: cartão em 0 e sem pagamento no mês, não saiu nada. Terceira: depois de pagar além da dívida, saiu 20 e foi para o "A atribuir". O teste da segunda tentativa deixou de ser `#[ignore]`.
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

### Roteiro 9 — virada do mês, "sem categoria" positivo e dois cartões

**Para que serve.** Conferir quatro regras do motor (`src/motor.rs`) que ainda são inferidas:
- **9A, virada com o cartão positivo**: pagar além da dívida em setembro e ver outubro sem nada lançado. O motor assume que nada muda na virada: o +10 fica em setembro, e o vermelho do Pagamento desconta de outubro. (Novembro do 8C já mostrou isso uma vez; aqui o mês seguinte é outubro, para o 9C poder lançar nele.)
- **9B, "sem categoria" positivo** (roteiro 4B): uma entrada sem categoria vinda do Investimento fica na linha "sem categoria" com saldo positivo e não entra no "A atribuir". O motor **não carregava nada** para o mês seguinte. **A tela carrega** (primeira tentativa, abaixo), e o motor foi ajustado.
- **9C, gastar no mês seguinte o crédito do cartão**: o motor compara com o saldo positivo que o cartão já tinha no fim do mês anterior, então o gasto **não** desfaz o +10 de setembro e passa para a categoria de pagamento, como um gasto comum.
- **9D, roteiro 4E**: transferência da Corrente para o Investimento sem categoria. Esperado: "sem categoria" −300 em vermelho, que desconta do "A atribuir" de novembro.
- **9E, uma categoria que gasta em dois cartões** com dinheiro só para parte do gasto (análise 01, §8). O motor dividia a parte coberta entre os cartões **na proporção do gasto**, regra nossa. Não bateu; a regra foi achada no roteiro 10 (o maior gasto é coberto primeiro).

Cabe tudo num orçamento só. Ficou de fora: um estorno no mês seguinte com o cartão positivo, e pagar além da dívida em dois meses seguidos.

Os valores esperados saem do próprio motor. Os passos se acumulam: cada um parte do anterior. Se um passo der diferente, anote o valor e siga; os seguintes ainda servem, mas a diferença pode se repetir neles.

#### Ponto de partida

Use o orçamento TESTE. **Antes de começar, mande uma captura** de cada conta (Conta Corrente, Cartão, Investimento) e do plano de setembro e outubro, para conferir o que sobrou do roteiro 8.

**Passo 0 — limpar.**
1. **Conta Corrente**: apague tudo, menos o "Starting Balance" de 1.000. Os pagamentos do roteiro 8 ("Payment: Cartão") somem também do Cartão. Abra o "Starting Balance" e mude a **Date** para **01/09/2026** (a categoria continua "Inflow: Ready to Assign").
2. **Cartão**: apague tudo (inclusive o gasto de 15/09). Saldo 0.
3. **Investimento**: apague tudo. Saldo 0.
4. **Poupança**: não é usada; deixe como está.
5. No plano de **setembro** e de **outubro**: atribuído = 0 em todas as categorias (Mercado tinha 50 em setembro).

Confira:

| Onde | Esperado |
|---|---|
| Conta Corrente / Cartão / Investimento | 1.000 / 0 / 0 |
| "Ready to Assign" de setembro / outubro / novembro | 1.000 / 1.000 / 1.000 |
| Linha "Uncategorized Transactions" | não aparece em nenhum mês |

#### Como lançar uma transferência

Toda transferência é lançada **na conta de onde o dinheiro sai**, com o beneficiário da lista de **transferências**:
1. Abra a conta de origem e clique em **Add Transaction**.
2. **Date**: a do passo. Nunca depois de 13/10/2026: data futura vira agendada.
3. **Payee**: digite o nome da outra conta e escolha a opção da lista de transferências ("Transfer : …" ou "Payment: …"). Não escolha "Create payee".
4. **Category**: a do passo.
5. **Outflow**: o valor. Salve.

Para saber se virou transferência: **o saldo da outra conta muda junto**, e a outra conta mostra a ponta com "Transfer : (conta de origem)" ou "Payment: (conta de origem)". Se a outra conta não mudar, apague e lance de novo.

No pagamento do cartão (9A), o pagamento é transferência entre orçamentárias: depois de escolher o Payee aparece "Category not needed", e a linha **não** pode mostrar "This needs a category". Outra forma: abrir o **Cartão** e usar **Record Payment**, com a Conta Corrente como origem.

Nas transferências com o Investimento (9B e 9D), a ponta da Conta Corrente **mostra** "This needs a category": é o esperado, porque o passo pede para deixar sem categoria. A ponta do Investimento não tem coluna de categoria.

#### Passos

Em cada passo, anote: saldo das contas; Activity e Available do que a tabela pede (e a cor); "Ready to Assign" dos meses da tabela. O "Ready to Assign" de setembro muda quando se atribui em outubro (ele desconta o que já foi atribuído nos meses seguintes); por isso só é pedido no 9A e no 9B.

**9A — paga além da dívida em setembro.**
1. No plano de **setembro**, atribua **50** a Mercado.
2. No **Cartão**: Add Transaction, Date **15/09/2026**, Payee qualquer (ex.: "Mercado X"), **Category: Mercado**, **Outflow 50**.
3. Na **Conta Corrente**: Add Transaction, Date **20/09/2026**, Payee **"Transfer : Cartão"** (ou "Payment: Cartão"), Category em branco ("Category not needed"), **Outflow 60**. Confira que o Cartão foi de −50 para **+10**.

| Onde | Esperado | Se der… |
|---|---|---|
| Corrente / Cartão | 940 / +10 | Cartão −50 → o pagamento não virou transferência; refaça o passo 3 |
| Setembro: Mercado (Activity / Available) | −50 / 0 | |
| Setembro: Pagamento do Cartão (Activity / Available) | −10 / −10 (vermelho) | |
| "Ready to Assign" de setembro | **960** | 950 → o +10 não vale quando o gasto e o pagamento são do mesmo mês |
| Outubro: Pagamento do Cartão | 0 / 0 | −10 → o vermelho passa para o mês seguinte na categoria |
| "Ready to Assign" de outubro / novembro | **950** / 950 | outubro 960 → o vermelho do Pagamento não desconta na virada |

**9B — entrada sem categoria vinda do Investimento, em setembro.** No **Investimento**: Add Transaction, Date **20/09/2026**, Payee **"Transfer : Conta Corrente"**, **Outflow 100**. Confira que a Corrente foi de 940 para **1.040** e que nela a entrada de 100 mostra "This needs a category". **Não categorize.**

| Onde | Esperado | Se der… |
|---|---|---|
| Corrente / Investimento | 1.040 / −100 | |
| Setembro: "Uncategorized Transactions" (Activity / Available) | +100 / +100 | |
| "Ready to Assign" de setembro | 960 (não muda) | 1.060 → a entrada sem categoria é receita (contradiz o 4B) |
| Outubro: "Uncategorized Transactions" | **não aparece** (ou 0) | +100 → o saldo positivo é carregado para o mês seguinte |
| "Ready to Assign" de outubro / novembro | **950** / 950 | outubro 1.050 → o positivo sem categoria vira "A atribuir" no mês seguinte |

Se outubro der 950 sem linha "sem categoria", os 100 somem do plano: a soma "dinheiro nas contas = A atribuir + disponível" deixa de fechar por 100 (1.050 nas contas, 950 no plano). Anote se a tela avisa alguma coisa.

**9C — gasta em outubro o crédito do cartão.** No plano de **outubro**, atribua **10** a Mercado. No **Cartão**: Add Transaction, Date **13/10/2026**, Payee qualquer, **Category: Mercado**, **Outflow 10**. O Cartão vai de +10 para 0.

| Onde | Esperado | Se der… |
|---|---|---|
| Corrente / Cartão | 1.040 / 0 | |
| Outubro: Mercado (Activity / Available) | −10 / 0 | |
| Outubro: Pagamento do Cartão (Activity / Available) | **+10 / 10** | 0 / 0 → o gasto usa o crédito do cartão sem passar pela categoria de pagamento |
| "Ready to Assign" de outubro / novembro | **940** / 940 | 930 → gastar o crédito no mês seguinte também desfaz o +10 de setembro |
| Novembro: Pagamento do Cartão | 10 | |

A atribuição sozinha leva o "Ready to Assign" de outubro de 950 a 940; pelo motor, o gasto não muda mais nada.

**9D — transferência da Corrente para o Investimento, sem categoria (roteiro 4E).** Na **Conta Corrente**: Add Transaction, Date **13/10/2026**, Payee **"Transfer : Investimento"**, **Category em branco**, **Outflow 300**. Confira que o Investimento foi de −100 para **200** e que a linha da Corrente mostra "This needs a category".

| Onde | Esperado | Se der… |
|---|---|---|
| Corrente / Investimento | 740 / 200 | |
| Outubro: "Uncategorized Transactions" (Activity / Available) | −300 / −300 (vermelho) | |
| "Ready to Assign" de outubro | 940 (não muda) | 640 → a transferência sem categoria desconta no mesmo mês |
| "Ready to Assign" de novembro | **640** | 940 → o "sem categoria" de transferência não é estouro em dinheiro |

**9E — uma categoria em dois cartões.**
1. Crie a conta **Cartão 2**: Add Account, sem vincular ao banco, tipo **Credit Card**, saldo **0**. Ela cria a categoria "Cartão 2" em "Credit Card Payments".
2. Crie a categoria **Farmácia** (qualquer grupo) e, no plano de **outubro**, atribua **60** a ela. Confira: "Ready to Assign" de outubro **880**.
3. No **Cartão**: Add Transaction, Date **13/10/2026**, Payee qualquer, **Category: Farmácia**, **Outflow 30**. Confira: Pagamento do Cartão em outubro **40** (10 do 9C + 30); Farmácia 30.
4. No **Cartão 2**: Add Transaction, Date **13/10/2026**, Payee qualquer, **Category: Farmácia**, **Outflow 70**.

Farmácia gastou 100 no crédito e tinha 60: estoura 40 no crédito. Os 60 cobertos vão para as categorias de pagamento.

| Onde | Esperado | Se der… |
|---|---|---|
| Cartão / Cartão 2 | −30 / −70 | |
| Outubro: Farmácia (Activity / Available) | −100 / −40 (amarelo) | |
| Outubro: Pagamento do Cartão (Available) | **28** (10 + 18) | 40 → o primeiro gasto é coberto primeiro (**por ordem**) |
| Outubro: Pagamento do Cartão 2 (Available) | **42** | 30 → **por ordem**; 60 → o último gasto (ou o maior) é coberto primeiro |
| "Ready to Assign" de outubro / novembro | 880 / **580** | novembro 540 → o estouro no crédito desconta (contradiz o roteiro 1) |
| Novembro: Pagamento do Cartão / Cartão 2 | 28 / 42 | |

Proporção: 60 × 30/100 = 18 no Cartão e 60 × 70/100 = 42 no Cartão 2. Se a tela der outra divisão, anote também o Activity das duas categorias de pagamento.

#### Segunda tentativa — o que refazer

Valores do teste `roteiro_9_segunda_tentativa_previsao`. Cada lançamento tem uma ficha: **em que conta abrir**, a data (e o mês), de onde o dinheiro sai, para onde vai, os campos e o que muda depois de salvar. Se o "Depois de salvar" não acontecer, apague o lançamento e faça de novo antes de seguir.

Toda atribuição é feita na tela **Plan**: escolha o mês no topo (setas ao lado de "Sep 2026" / "Oct 2026"), clique na coluna **Assigned** da categoria e digite o valor.

##### Etapa 1 — limpeza

| # | Onde | O que fazer |
|---|---|---|
| 1 | Conta Corrente | Apagar a saída de **60**, 11/09, "Mensal: Mercado" |
| 2 | Conta Corrente | Apagar a saída de **10**, 13/10, sem beneficiário ("This needs a category") |
| 3 | Conta Corrente | Apagar a saída de **300**, 13/10, "Transfer: Investimento" (a ponta do Investimento some junto) |
| 4 | Cartão | Apagar o gasto de **30** em Farmácia |
| 5 | Cartão 2 | Apagar o gasto de **70** em Farmácia |
| 6 | Plan, **Nov 2026** | Farmácia: Assigned = **0** |
| 7 | Plan, **Sep 2026** | Mercado: Assigned continua **50** (não mexer) |

Não apague a entrada de **100** de 01/09 ("Transfer: Investimento" na Corrente, "This needs a category"). Ela fica sem categoria.

Confira:

| Onde | Esperado |
|---|---|
| Conta Corrente / Investimento / Cartão / Cartão 2 | 1.100 / −100 / 0 / 0 |
| Sep 2026: Mercado (Available) / Uncategorized (Available) | 50 / 100 |
| Oct 2026: Mercado / Uncategorized | 50 / 100 |
| Ready to Assign: Sep / Oct / Nov | 950 / 950 / 950 |

##### Etapa 2 (9A) — gasto no cartão e pagamento a mais, em setembro

**Lançamento A1 — gasto no Cartão**

| Campo | Valor |
|---|---|
| Abrir a conta | **Cartão** (barra da esquerda, grupo CREDIT) → Add Transaction |
| Date | **15/09/2026** (setembro) |
| Payee | qualquer nome novo, ex.: "Mercado X" |
| Category | **Mensal: Mercado** |
| Outflow | **50** |
| Dinheiro sai de → chega em | Cartão (vira dívida) → ninguém, é gasto |
| Depois de salvar | **Cartão −50**. A Corrente **não muda** (1.100) |

**Lançamento A2 — pagamento do Cartão**

O jeito mais seguro (conferido no roteiro 5C): escolher a **categoria de pagamento do cartão** numa saída da Corrente. O YNAB troca o beneficiário sozinho para "Payment: Cartão".

| Campo | Valor |
|---|---|
| Abrir a conta | **Conta Corrente** (grupo CASH) → Add Transaction |
| Date | **20/09/2026** |
| Payee | **deixar vazio** (não digitar "Mercado x" nem outro nome) |
| Category | abrir a lista e escolher, no grupo **Credit Card Payments**, a categoria **Cartão**. Ao escolher, o Payee vira sozinho **"Payment: Cartão"** e a categoria vira "Category not needed" |
| Outflow | **60** |
| Sai de → chega em | Conta Corrente → Cartão |
| Depois de salvar | Corrente **1.040** e **Cartão +10** (era −50). Na tela "All Accounts" aparecem **duas** linhas de 20/09: a da Corrente ("Payment: Cartão", saída 60) e a do Cartão ("Payment: Conta Corrente", entrada 60). Se aparecer "Mensal: Mercado" na categoria, está errado |

Confira:

| Onde | Esperado | Se der… |
|---|---|---|
| Sep 2026: Mercado (Activity / Available) | −50 / 0 | −110 → o pagamento virou gasto em Mercado; refaça o A2 |
| Sep 2026: Credit Card Payments → Cartão (Activity / Available) | −10 / −10 (vermelho) | |
| Oct 2026: Cartão em Credit Card Payments | 0 / 0 | −10 → o vermelho passa para o mês seguinte |
| Ready to Assign: Sep / Oct / Nov | **960** / **950** / 950 | Sep 950 → o +10 não vale com gasto e pagamento no mesmo mês |

##### Etapa 3 (9C) — gastar em outubro o crédito do cartão

**Atribuição**: Plan, **Oct 2026**, Mercado, Assigned = **10**.

**Lançamento C1 — gasto no Cartão**

| Campo | Valor |
|---|---|
| Abrir a conta | **Cartão** → Add Transaction |
| Date | **13/10/2026** (outubro) |
| Payee | qualquer nome, ex.: "Mercado X" |
| Category | **Mensal: Mercado** |
| Outflow | **10** |
| Dinheiro sai de → chega em | Cartão (usa o crédito de +10) → ninguém, é gasto |
| Depois de salvar | **Cartão 0** (era +10). A Corrente **não muda** (1.040) |

Confira:

| Onde | Esperado | Se der… |
|---|---|---|
| Oct 2026: Mercado (Activity / Available) | −10 / 0 | |
| Oct 2026: Cartão em Credit Card Payments (Activity / Available) | ~~+10 / 10~~ **0 / 0** (tela; ver conferência) | |
| Ready to Assign: Sep / Oct / Nov | 950 / **940** / 940 | Oct 930 → o +10 de setembro é desfeito em outubro |
| Nov 2026: Cartão em Credit Card Payments | ~~10~~ **0** | |

##### Etapa 4 (9D) — transferência para o Investimento, sem categoria

**Lançamento D1**

| Campo | Valor |
|---|---|
| Abrir a conta | **Conta Corrente** → Add Transaction |
| Date | **13/10/2026** (outubro) |
| Payee | digite "Investimento" e escolha **"Transfer : Investimento"** na lista de transferências |
| Category | **deixar vazio**. Não escolher "Inflow: Ready to Assign" |
| Outflow | **300** |
| Dinheiro sai de → chega em | Conta Corrente → Investimento |
| Depois de salvar | Corrente **740**; **Investimento 200** (era −100). A linha da Corrente mostra **"This needs a category"** (é o esperado) |

Confira:

| Onde | Esperado | Se der… |
|---|---|---|
| Oct 2026: Uncategorized (Activity / Available) | −300 / **−200** (vermelho) | Available −300 → o positivo de setembro não é usado |
| Nov 2026: Uncategorized | 0 ou não aparece | |
| Ready to Assign: Oct / Nov | 940 / **740** | Oct 640 → ficou com "Inflow: Ready to Assign"; tire a categoria. Nov 940 → não é estouro em dinheiro |

##### Etapa 5 (9E) — Farmácia em dois cartões, em outubro

**Atribuição**: Plan, **Oct 2026** (não novembro), Farmácia, Assigned = **60**. Ready to Assign de outubro vai a **880**.

**Lançamento E1 — gasto no Cartão**

| Campo | Valor |
|---|---|
| Abrir a conta | **Cartão** → Add Transaction |
| Date | **13/10/2026** |
| Payee | qualquer nome, ex.: "Farmácia X" |
| Category | **Farmácia** |
| Outflow | **30** |
| Depois de salvar | **Cartão −30**; Oct 2026, Cartão em Credit Card Payments: Available **30** |

**Lançamento E2 — gasto no Cartão 2**

| Campo | Valor |
|---|---|
| Abrir a conta | **Cartão 2** → Add Transaction |
| Date | **13/10/2026** |
| Payee | qualquer nome, ex.: "Farmácia X" |
| Category | **Farmácia** |
| Outflow | **70** |
| Depois de salvar | **Cartão 2 −70** |

Confira (Farmácia gastou 100 e tinha 60):

| Onde | Esperado | Se der… |
|---|---|---|
| Oct 2026: Farmácia (Activity / Available) | −100 / −40 (amarelo) | |
| Oct 2026: Cartão em Credit Card Payments (Available) | **18** | 30 → o primeiro gasto é coberto primeiro |
| Oct 2026: Cartão 2 em Credit Card Payments (Available) | **42** | 30 → o primeiro primeiro; 60 → o último (ou o maior) primeiro |
| Ready to Assign: Sep / Oct / Nov | 890 / 880 / **680** | Nov 640 → o estouro no crédito desconta |
| Nov 2026: Cartão / Cartão 2 em Credit Card Payments | 18 / 42 | |

Mande uma captura do plano (Sep, Oct e Nov) e da tela "All Accounts" no fim de cada etapa.

##### Conferência da segunda tentativa (2026-10-13)

- **Etapa 1 (limpeza)**: Corrente 1.100, Investimento −100, Cartão 0, Cartão 2 0. Setembro: Uncategorized +100 / 100, Mercado 50 / 0 / 50, Ready to Assign 950. Outubro: Uncategorized 0 / **100**, Mercado 50, "Left Over from Last Month" 150 (100 sem categoria + 50 de Mercado), Ready to Assign 950. **Conferido**, inclusive o carregamento do "sem categoria" positivo pela segunda vez.
- **Etapa 2, A1** (50 no Cartão em Mercado, 15/09): Cartão −50, Corrente 1.100. Setembro: Mercado 50 / −50 / 0 ("Fully Spent"), Pagamento do Cartão +50 / 50, Ready to Assign 950. Outubro: Pagamento do Cartão 50, Uncategorized 100, Ready to Assign 950. **Conferido.**
- **Etapa 2, A2, lançado de outro jeito**: saída de **50** da Corrente com Payee "Mercado x" e categoria **Mensal: Mercado** (20/09), e não pagamento de 60 para o Cartão. Setembro: Mercado 50 / −100 / **−50 amarelo** (estouro no crédito), Pagamento do Cartão 0 / 0, Ready to Assign 950; outubro 950, Pagamento 0. **Conferido** (teste `roteiro_9_segunda_tentativa_etapa_2_como_lancada`): o dinheiro da categoria cobre primeiro o gasto em dinheiro; o gasto no cartão fica sem cobertura e sai do Pagamento. É a ordem de `calculate_budgeted_cash_credit_outflows` (dinheiro antes do crédito), agora vista na tela. O +10 **ainda não foi testado**. A ficha do A2 passou a usar a categoria de pagamento como atalho.
- **Etapa 2 (9A), A2 refeito** pela categoria de pagamento: a Corrente mostrou "Payment: Cartão", saída 60, e o Cartão "Payment: Conta Corrente", entrada 60 (as duas com "Credit Card Payments: Cartão" na coluna de categoria). Corrente 1.040, Cartão **+10**. Setembro: Mercado 50 / −50 / 0, Pagamento do Cartão 0 / −10 / **−10 vermelho**, Ready to Assign **960**. Outubro: Pagamento do Cartão 0, Uncategorized 100, "Left Over from Last Month" 100, Ready to Assign **950**. **Conferido**: o +10 vale também com o gasto e o pagamento no mesmo mês, e na virada o vermelho do Pagamento desconta do mês seguinte sem passar para a categoria (outubro sem nada lançado). É o item 1 do roteiro.
- **Etapa 3 (9C)**: 10 atribuídos a Mercado em outubro e gasto de 10 **no Cartão** em Mercado (13/10). Cartão 0, Corrente 1.040. Outubro: Mercado 10 / −10 / 0 ("Fully Spent"), **Pagamento do Cartão 0 / 0**, Uncategorized 100, Ready to Assign 940. Novembro: Pagamento 0, Ready to Assign 940. **Divergia do motor**, que previa Pagamento +10 / 10 (e a conta do dinheiro não fecharia: 1.040 nas contas contra 1.050 no plano). Na tela fecha: 1.040 = 940 + 100 sem categoria. **Regra**: o gasto coberto que só consome o saldo positivo trazido do mês anterior não passa para a categoria de pagamento, porque não vira dívida. O +10 de setembro **não** é desfeito (outubro 940 só pela atribuição). Motor ajustado (`gasto_coberto_limitado`); as previsões do 9E mudaram (Pagamento do Cartão 30 depois do E1, 18 no fim).
- **Etapa 4 (9D, roteiro 4E)**: na Corrente, 13/10, "Transfer: Investimento", saída 300, "This needs a category". Corrente 740, Investimento 200. Outubro: Uncategorized −300 / **−200** (vermelho, "1 Overspent"), Ready to Assign 940. Novembro: a linha "sem categoria" não aparece, "Left Over from Last Month" 0, Ready to Assign **740**. **Conferido**: a transferência sem categoria para acompanhamento é "sem categoria" em dinheiro; ela gasta primeiro o positivo carregado de setembro, e o estouro que sobra (−200) desconta do mês seguinte. Fecha o 4E.
- **Etapa 5 (9E)**: 60 atribuídos a Farmácia em outubro (Ready to Assign 880). Depois do E1 (30 no Cartão em Farmácia): Cartão −30, Farmácia 60 / −30 / 30, **Pagamento do Cartão 30**. **Conferido** (o gasto de Mercado do 9C continuou fora do pagamento). Depois do E2 (70 no Cartão 2 em Farmácia): Cartão 2 −70; Farmácia 60 / −100 / −40 (amarelo); **Pagamento do Cartão 10 e do Cartão 2 50** (os dois amarelos); Ready to Assign 890 / 880 / **680** em setembro, outubro e novembro; novembro com 10 e 50 nos pagamentos. Ready to Assign e estouro no crédito **conferidos**.
- **9E, divergia do motor — a divisão entre os cartões**: o motor divide os 60 cobertos na proporção do gasto (18 e 42); a tela deu **10 e 50**. Também não é "por ordem" (30 e 30) nem "o último primeiro" (0 e 60). O E2 tirou 20 do Cartão. Os 40 sem cobertura ficaram **20 em cada cartão** (dívida 30, pagamento 10; dívida 70, pagamento 50). Com um caso só, cabem várias regras (estouro dividido igualmente entre os cartões; igualmente por transação; outra), então o motor **não** foi mudado. Teste `roteiro_9e_divisao_entre_dois_cartoes`, marcado `#[ignore]`. Para separar as regras, falta um roteiro com gastos bem diferentes (ex.: 10 num cartão e 90 no outro com 60 atribuídos: dividir igualmente o estouro de 40 deixaria o primeiro cartão com −10, o que não pode).
- **Roteiro 9 concluído** nesta tentativa: 9A, 9B, 9C e 9D conferidos (o 9B e o 9C mudaram o motor); a divisão do 9E fica em aberto.

#### Primeira tentativa (2026-10-13)

Teste `roteiro_9_primeira_tentativa`.
- **Passo 0**: "Ready to Assign" de 1.000 em setembro, outubro e novembro. **Conferido.**
- **9A, lançado de outro jeito**: os 60 saíram da **Corrente em Mercado** (11/09), sem gasto no Cartão. Mercado em setembro: 50 / −60 / −10 (vermelho), Cartão 0, "Ready to Assign" 950 em setembro e 940 em outubro. Bate com o motor, mas é só o estouro em dinheiro já conhecido; o +10 **não foi testado**.
- **9B** (lançado em 01/09): Investimento −100, Corrente 1.040. Setembro: "Uncategorized Transactions" +100 / 100 (verde), "Ready to Assign" 950 (não mudou). **Conferido.**
- **9B, outubro — divergia do motor**: o resumo de outubro mostrou "Left Over from Last Month" **100**, e a linha "Uncategorized Transactions" começou outubro com 100. **O saldo positivo da linha "sem categoria" passa para o mês seguinte, como numa categoria.** Novembro também: linha com 90 e "Left Over" 90. O motor não carregava. Com o carregamento, a conta do dinheiro fecha: em outubro, Corrente 1.030 = "Ready to Assign" 940 + "sem categoria" 90. **Motor ajustado**: `uncategorized_row` em `src/calc.rs` recebe a linha do mês anterior e usa `calculate_balance`, a mesma regra das categorias (só o positivo passa). Bate com tudo o que já foi conferido: as linhas "sem categoria" negativas dos roteiros 1 e 8 não tinham positivo antes.
- **9C, lançado de outro jeito**: saída de 10 **sem categoria na Corrente** (13/10), sem atribuir a Mercado. A linha "sem categoria" de outubro ficou −10 / **90**, sem estouro, e o "Ready to Assign" continuou 940 em outubro. **Conferido**: a saída sem categoria gasta do positivo carregado antes de estourar.
- **9D, lançado de outro jeito**: a transferência de 300 para o Investimento ficou com a categoria **"Inflow: Ready to Assign"**. Corrente 730, Investimento 200, "Ready to Assign" **640** em outubro e em novembro. **Conferido**: saída com a categoria de receita desconta do "A atribuir" no mesmo mês (receita negativa). O roteiro 4E **não foi testado**.
- **9E, lançado de outro jeito**: os 60 da Farmácia foram atribuídos em **novembro**. Em outubro, Farmácia −100 / −100 (amarelo, crédito), Pagamento do Cartão e do Cartão 2 0 / 0, "Ready to Assign" **580** (desconta os 60 atribuídos no futuro, "Assigned in Future Months" 60). Novembro: Farmácia 60, "Ready to Assign" 580. **Conferido**, mas sem dinheiro na Farmácia em outubro a divisão entre os cartões **não foi testada**.

### Roteiro 10 — uma categoria que gasta em vários cartões

**Para que serve.** No 9E, Farmácia com 60 atribuídos gastou 30 no Cartão e 70 no Cartão 2. A tela deu **10 e 50** nas categorias de pagamento; o motor dá 18 e 42 (`funded_spending_on_card` em `src/calc.rs`). Regras que explicam o 9E (as previsões saem de um teste temporário que implementou cada uma):
- **P — proporção** (o motor hoje): a parte coberta se divide na proporção do gasto em cada cartão. Já descartada pelo 9E; fica nas tabelas como referência.
- **C — estouro igual por cartão**: o estouro da categoria (gasto no crédito − dinheiro dela) se divide em partes iguais entre os cartões; o resto do gasto de cada cartão é coberto. No 9E: estouro 40, 20 em cada cartão (30 − 20 = 10; 70 − 20 = 50). Pode deixar um cartão com cobertura negativa.
- **CL — estouro igual por cartão, limitado**: como o C, mas nenhum cartão fica com mais estouro do que gastou; a sobra se divide igualmente entre os outros.
- **T — estouro igual por transação**: como o CL, mas por transação, não por cartão.

Também caberiam "por ordem" (30 e 30) e "o último primeiro" (0 e 60), mas o 9E já descartou as duas. Nenhuma candidata usa o crédito de 10 que o Cartão consumiu em Mercado: pela regra do 9C, o gasto de Mercado e o crédito se anulam no pagamento do Cartão, e a Farmácia fica com 10 nele. O passo 10C confere isso com o Cartão de novo.

Os passos **10A a 10C** separam as quatro regras:
- **10A**: gastos bem diferentes (10 e 90). Separa P, C e CL/T.
- **10B**: dois gastos num cartão e um no outro. Separa CL de T. O cartão do gasto maior vem primeiro, na ordem inversa do 9E.
- **10C**: três cartões, um deles o Cartão (com o crédito consumido no 9C).

Os passos **10D e 10E** conferem duas regras de cartão com saldo positivo trazido do mês anterior, que hoje são inferidas (`gasto_coberto_limitado` em `src/motor.rs`):
- **10D, estorno no mês seguinte com o cartão positivo.** Regra do motor (**M**): o estorno fica na categoria e não sai do pagamento, como no 8E da segunda tentativa. Alternativa (**S**): sai do pagamento.
- **10E, gastar o crédito do mês anterior num mês com pagamento** (o 9C com pagamento). Regra do motor (**F**, fim do mês): o crédito consumido se mede pelo saldo do fim do mês, depois do pagamento. Alternativa (**A**, antes do pagamento): o gasto consome o crédito primeiro, e o pagamento que deixa o cartão positivo de novo vai para o "A atribuir", como no 5C.

Os valores que não dependem da regra saem do teste `roteiro_10_previsao`.

#### Ponto de partida

O TESTE como ficou no fim do roteiro 9 (segunda tentativa): Corrente 740, Investimento 200, Cartão −30, Cartão 2 −70; Farmácia com 60 em outubro, gastos de 30 no Cartão e de 70 no Cartão 2. **Antes de começar, mande uma captura** da tela "All Accounts" e do plano de outubro.

Todas as datas de outubro são **14/10/2026 (hoje)**. Só o pagamento do 10D é em setembro. Data futura vira agendada.

Toda atribuição é feita na tela **Plan**: escolha o mês no topo, clique na coluna **Assigned** da categoria e digite o valor. "Pagamento do Cartão X" é a linha **Cartão X** do grupo **Credit Card Payments** no plano.

**Passo 0 — limpar e criar o Cartão 3.**

| # | Onde | O que fazer |
|---|---|---|
| 1 | Cartão | Apagar o gasto de **30** em Farmácia (13/10). Não apague o de 10 em Mercado nem os de setembro |
| 2 | Cartão 2 | Apagar o gasto de **70** em Farmácia (13/10) |
| 3 | Barra da esquerda | **Add Account**, sem vincular ao banco, tipo **Credit Card**, nome **Cartão 3**, saldo **0**. Ela cria a categoria "Cartão 3" em "Credit Card Payments" |
| 4 | Plan, Oct 2026 | Farmácia continua com **60** atribuídos; Mercado com 10 |

Confira:

| Onde | Esperado |
|---|---|
| Cartão / Cartão 2 / Cartão 3 | 0 / 0 / 0 |
| Corrente / Investimento | 740 / 200 |
| Oct 2026: Farmácia (Assigned / Activity / Available) | 60 / 0 / 60 |
| Oct 2026: Pagamento do Cartão, do Cartão 2 e do Cartão 3 (Available) | 0 / 0 / 0 |
| Ready to Assign: Sep / Oct / Nov | 890 / 880 / 680 |

#### Como lançar um gasto no cartão (10A a 10E)

| Campo | Valor |
|---|---|
| Abrir a conta | o **cartão do passo** (barra da esquerda, grupo CREDIT) → Add Transaction |
| Date | **14/10/2026** |
| Payee | qualquer nome, ex.: "Farmácia X" |
| Category | **Farmácia** |
| Outflow | o valor do passo (no estorno do 10D, **Inflow**) |
| Dinheiro sai de → chega em | o cartão (vira dívida) → ninguém, é gasto |
| Depois de salvar | o saldo do cartão cai o valor do passo. A Corrente **não muda** |

Cada gasto é uma transação nova (não edite a anterior). Mande uma captura do plano de outubro depois de cada gasto: os valores do meio também ajudam a achar a regra.

#### Passos

Em cada passo, anote: saldo dos cartões; Activity e Available da Farmácia e dos pagamentos (e a cor); "Ready to Assign" de outubro e de novembro.

**10A — 10 num cartão e 90 no outro.**

| # | Cartão | Outflow | Depois de salvar |
|---|---|---|---|
| A1 | **Cartão 2** | **10** | Cartão 2 −10; Farmácia 50; Pagamento do Cartão 2: **10** |
| A2 | **Cartão 3** | **90** | Cartão 3 −90; Farmácia **−40** (amarelo) |

Depois do A2 (Farmácia gastou 100 e tinha 60: estouro de 40 no crédito):

| Onde | P | C | CL | T |
|---|---|---|---|---|
| Oct: Pagamento do Cartão 2 (Available) | 6 | **−10** | **0** | **0** |
| Oct: Pagamento do Cartão 3 (Available) | 54 | **70** | **60** | **60** |

Iguais em todas: Farmácia 60 / −100 / −40 (amarelo); Ready to Assign Oct / Nov 880 / 680; em novembro, os pagamentos com os mesmos valores de outubro. Se der outra divisão, anote o Activity dos dois pagamentos.

**10B — dois gastos num cartão, um no outro.** Primeiro apague os dois gastos do 10A (Cartão 2 e Cartão 3 voltam a 0; Farmácia 60).

| # | Cartão | Outflow | Depois de salvar |
|---|---|---|---|
| B1 | **Cartão 3** | **60** | Cartão 3 −60; Farmácia 0; Pagamento do Cartão 3: **60** |
| B2 | **Cartão 2** | **15** | Cartão 2 −15; Farmácia −15 (amarelo) |
| B3 | **Cartão 2** | **15** (outra transação) | Cartão 2 −30; Farmácia −30 (amarelo) |

Depois do B2 (estouro 15):

| Onde | P | C | CL | T |
|---|---|---|---|---|
| Oct: Pagamento do Cartão 2 | 12 | 7,50 | 7,50 | 7,50 |
| Oct: Pagamento do Cartão 3 | 48 | 52,50 | 52,50 | 52,50 |

Depois do B3 (estouro 30):

| Onde | P | C | CL | T |
|---|---|---|---|---|
| Oct: Pagamento do Cartão 2 | 20 | **15** | **15** | **10** |
| Oct: Pagamento do Cartão 3 | 40 | **45** | **45** | **50** |

Iguais em todas: Farmácia 60 / −90 / −30 (amarelo); Ready to Assign Oct / Nov 880 / 680.

**10C — três cartões.** Primeiro apague os três gastos do 10B (Cartão 2 e Cartão 3 voltam a 0; Farmácia 60).

| # | Cartão | Outflow | Depois de salvar |
|---|---|---|---|
| C1 | **Cartão 2** | **10** | Cartão 2 −10; Pagamento do Cartão 2: 10 |
| C2 | **Cartão 3** | **50** | Cartão 3 −50; Farmácia 0; Pagamento do Cartão 3: 50 |
| C3 | **Cartão** (o primeiro) | **60** | **Cartão −60** (o gasto de 10 em Mercado já levou o crédito de setembro); Farmácia −60 (amarelo) |

Depois do C3 (estouro 60):

| Onde | P | C | CL | T |
|---|---|---|---|---|
| Oct: Pagamento do Cartão | 30 | **40** | **35** | **35** |
| Oct: Pagamento do Cartão 2 | 5 | **−10** | **0** | **0** |
| Oct: Pagamento do Cartão 3 | 25 | **30** | **25** | **25** |

Iguais em todas: Farmácia 60 / −120 / −60 (amarelo); Ready to Assign Oct / Nov 880 / 680. No CL e no T, o Cartão 2 só aguenta 10 do estouro; os 10 que sobram dos 20 dele se dividem entre os outros dois (25 de estouro em cada).

**10D — estorno no mês seguinte com o cartão positivo.** Primeiro apague os três gastos do 10C (os três cartões voltam a 0; Farmácia 60).

**Lançamento D0 — pagamento do Cartão 3 em setembro, sem dívida**

| Campo | Valor |
|---|---|
| Abrir a conta | **Conta Corrente** (grupo CASH) → Add Transaction |
| Date | **20/09/2026** (setembro) |
| Payee | **deixar vazio** |
| Category | no grupo **Credit Card Payments**, a categoria **Cartão 3**. O Payee vira sozinho **"Payment: Cartão 3"** e a categoria "Category not needed" |
| Outflow | **20** |
| Sai de → chega em | Conta Corrente → Cartão 3 |
| Depois de salvar | Corrente **720** e **Cartão 3 +20**. Se a categoria ficar "Farmácia" ou a linha mostrar "This needs a category", está errado: apague e lance de novo |

Confira:

| Onde | Esperado | Se der… |
|---|---|---|
| Sep 2026: Pagamento do Cartão 3 (Activity / Available) | −20 / −20 (vermelho) | |
| Ready to Assign: Sep / Oct / Nov | **910** / 880 / 680 | Sep 890 → o pagamento sem dívida não soma (contradiz o 5C) |
| Oct 2026: Pagamento do Cartão 3 | 0 / 0 | −20 → o vermelho passa para o mês seguinte |

**Lançamento D1 — estorno no Cartão 3 em outubro**: ficha do gasto, no **Cartão 3**, com **Inflow 10** (entrada, não saída). Depois de salvar: **Cartão 3 +30**; Farmácia 60 / +10 / 70.

| Onde | M | S |
|---|---|---|
| Oct: Pagamento do Cartão 3 (Activity / Available) | **0 / 0** | −10 / −10 (vermelho) |
| Ready to Assign: Oct / Nov | 880 / **680** | 880 / 670 |

**10E — gastar o crédito de setembro num mês com pagamento.** Primeiro apague o estorno do D1 (Cartão 3 volta a +20; Farmácia 60). O pagamento de setembro (D0) fica.

**Lançamento E1 — gasto no Cartão 3**: ficha do gasto, no **Cartão 3**, **Outflow 30**. Depois de salvar: **Cartão 3 −10**; Farmácia 60 / −30 / 30.

| Onde | Esperado (F e A) | Se der… |
|---|---|---|
| Oct: Pagamento do Cartão 3 (Activity / Available) | **10 / 10** | 30 / 30 → o crédito de setembro não é descontado quando o gasto passa dele; 0 / 0 → o gasto inteiro fica fora do pagamento |

Os 20 de crédito pagam parte do gasto; só os 10 que viram dívida passam para o pagamento.

**Lançamento E2 — pagamento do Cartão 3 em outubro**

| Campo | Valor |
|---|---|
| Abrir a conta | **Conta Corrente** → Add Transaction |
| Date | **14/10/2026** |
| Payee | **deixar vazio** |
| Category | **Credit Card Payments: Cartão 3** (o Payee vira "Payment: Cartão 3") |
| Outflow | **30** |
| Sai de → chega em | Conta Corrente → Cartão 3 |
| Depois de salvar | Corrente **690** e **Cartão 3 +20** (era −10) |

| Onde | F | A |
|---|---|---|
| Oct: Pagamento do Cartão 3 (Activity / Available) | **0 / 0** | −20 / −20 (vermelho) |
| Ready to Assign: Oct / Nov | **880** / 680 | 900 / 680 |

No F, o cartão termina outubro com o mesmo +20 com que começou: nada do crédito foi gasto, os 30 da Farmácia passam inteiros para o pagamento e o pagamento de 30 os leva. No A, o gasto consome os 20 e o pagamento recria um positivo de 20, que vai para o "A atribuir" de outubro e desconta de novembro.

Mande uma captura do plano (Sep, Oct e Nov) e da tela "All Accounts" no fim de cada passo.

#### Conferência (2026-10-14)

Teste `roteiro_10_conferido`.
- **Passo 0**: Cartão 0, Cartão 2 0, Cartão 3 0 (criado com "Starting Balance" de 0), Corrente 740, Investimento 200. **Conferido** junto com o 10A (o plano antes do 10A não veio).
- **10A** (10 no Cartão 2, depois 90 no Cartão 3): Farmácia 60 / −100 / −40 (amarelo); **Pagamento do Cartão 2 0 / 0** (amarelo) e **do Cartão 3 60 / 60** (amarelo); Ready to Assign 880 / 680; novembro com 0 e 60. Bate com CL e T (e com "o último primeiro"); **P e C descartadas**.
- **10B** (60 no Cartão 3, depois 15 e 15 no Cartão 2): Cartão 2 −30, Cartão 3 −60; Farmácia 60 / −90 / −30; **Pagamento do Cartão 2 0** (amarelo) e **do Cartão 3 60 / 60** (verde); 880 / 680; novembro 0 e 60. **Nenhuma candidata bateu** (CL previa 15 / 45; T, 10 / 50). "O último primeiro" também cai (daria 30 / 30).
- **10C** (10 no Cartão 2, 50 no Cartão 3, 60 no Cartão): Farmácia 60 / −120 / −60; **Pagamento do Cartão 60 / 60** (verde), do Cartão 2 e do Cartão 3 **0** (amarelos); 880 / 680; novembro 60, 0 e 0. Nenhuma candidata bateu.
- **Regra que fecha os quatro casos: o maior gasto da categoria é coberto primeiro**; o estouro fica nos menores. 10A: 90 coberto com 60, 10 sem nada. 10B: 60 coberto inteiro, 15 + 15 sem nada. 10C: 60 coberto inteiro. 9E: o Cartão trazia +10 de setembro; o crédito pagou 10 do gasto de 30 na Farmácia (o maior do dia 13/10, que o Mercado de 10 também usava), e essa parte conta como **dinheiro** da Farmácia, coberto antes do crédito. Sobraram 50 para o crédito (20 no Cartão, 70 no Cartão 2): o Cartão 2 recebe os 50 e o Cartão nada da Farmácia. Os 10 de Mercado viraram dívida coberta e foram para o pagamento do Cartão: **10 e 50**, como na tela.
- **10D, D0** (pagamento de 20 ao Cartão 3 em 20/09, pela categoria de pagamento): Corrente 720, Cartão 3 +20; setembro: Pagamento do Cartão 3 −20 / **−20 vermelho**, Ready to Assign **910**; outubro: Pagamento do Cartão 3 0, Ready to Assign 880. **Conferido** (o +20 do 5C, de novo, e o vermelho descontando do mês seguinte).
- **10D, D1** (estorno de 10 na Farmácia no Cartão 3, Payee vazio): Cartão 3 +30; o cabeçalho do Cartão 3 mostrou **Payment 0** (verde). **Conferido M** (o estorno não sai do pagamento); S descartada. O plano não veio, então o Ready to Assign deste passo não foi visto.
- **10E, lançado de outro jeito**: o gasto do E1 ficou **sem categoria** (saída de 30 no Cartão 3, Payee vazio, "This needs a category"). Depois do E1: Cartão 3 −10, Payment 0 (amarelo). Depois do E2 (pagamento de 30, 14/10, "Payment: Cartão 3"): Corrente 690, Cartão 3 +20. Outubro: Uncategorized −330 / −230 (vermelho), **Pagamento do Cartão 3 −30 / −30 (vermelho)**, Farmácia 60, Ready to Assign **900**. Novembro: Ready to Assign **650**, Pagamento do Cartão 3 0. Pelo motor de antes daria 880 em outubro: **a regra F (fim do mês) está descartada**, e vale a **A**. O pagamento recria o positivo que o gasto consumiu, e esses 20 vão para o "A atribuir" de outubro. Novembro 650 mostra também que os 20 pagos com o crédito estouram **em dinheiro** na linha "sem categoria" (900 − 200 − 20 − 30 vermelho do pagamento): é a mesma regra do 9E, a parte paga com o crédito trazido conta como dinheiro.
- **Motor ajustado**:
  - `funded_by_card` em `src/calc.rs` substitui a proporção: o maior gasto é coberto primeiro.
  - `credito_consumido` em `src/motor.rs`: as saídas no cartão consomem o positivo trazido do mês anterior por data e, no mesmo dia, da maior para a menor; essa parte vira gasto em dinheiro da categoria ou da linha "sem categoria".
  - O pagamento que recria esse positivo vai para o "A atribuir" (`saldo_positivo_novo`).

  O teste `roteiro_9e_divisao_entre_dois_cartoes` deixou de ser `#[ignore]`.
- Os pontos que ficaram em aberto (empate, ordem por cartão ou por transação, qual saída consome o crédito trazido, o 10E com a categoria) foram conferidos no roteiro 11.

### Roteiro 11 — empate, ordem por transação e crédito trazido

**Para que serve.** Conferir o que o roteiro 10 deixou em aberto. Foi o primeiro roteiro lançado pelo navegador (Claude in Chrome, com autorização do usuário para a lista de lançamentos abaixo); o 11D foi terminado pelo usuário. Todas as datas são 14/10/2026. Parte do fim do 10E como lançado (Cartão 3 com +20 de setembro, gasto de 30 sem categoria e pagamento de 30 em outubro). Teste `roteiro_11_conferido`.

| Passo | Lançamentos | Motor antes | Alternativa | Tela |
|---|---|---|---|---|
| 11A | o gasto de 30 do 10E recebe a categoria Farmácia | Pagamento do Cartão 3 −20 / −20, Ready to Assign 900 / 680 | — | **igual ao motor** |
| Limpeza | apaga o gasto e o pagamento de 30 de outubro | 880 / 680, Cartão 3 +20 | — | **igual** |
| 11B | Farmácia (60): 40 + 40 no Cartão 2, 70 no Cartão | por cartão: Cartão 2 60, Cartão 0 | por transação: Cartão 60, Cartão 2 0 | **Cartão 60, Cartão 2 0** (novembro igual) |
| 11C | Farmácia: 50 no Cartão 2 (antes), 50 no Cartão | Cartão 50, Cartão 2 10 (ordem das contas) | o lançado antes: Cartão 2 50 | **Cartão 2 50, Cartão 10** |
| 11D, D1 | Mercado (sem dinheiro) 10 no Cartão 3 | Mercado −10 vermelho (pago com o crédito) | — | **igual** |
| 11D, D2 | Farmácia 30 no Cartão 3, mesmo dia | a maior usa o crédito: Mercado −10 amarelo, Pagamento 10 / 10, 880 / 680 | a primeira lançada: Mercado −10 vermelho, Pagamento 20, 880 / 670 | **igual ao motor** |

Conclusões:
- **A ordem é por transação**: a maior transação no crédito é coberta primeiro, não o cartão com a maior soma (11B). No empate, a lançada antes (11C; a ordem das contas daria o contrário). `funded_by_card` passou a receber as transações, e o motor soma por cartão.
- O 10E com a categoria fecha com a regra do roteiro 10: os 20 pagos com o crédito trazido são dinheiro coberto, os 10 de dívida passam para o pagamento, e o pagamento recria os +20 no "A atribuir".
- **No mesmo dia, o crédito trazido vai para a maior saída** (11D), como o motor já fazia a partir do 9E. A saída que perde o crédito volta a estourar no crédito (amarelo).
- No navegador, a transação nova lançada pela tela **All Accounts** trocando a conta no campo Account **não é gravada**: aparece na lista, mas o saldo não muda e ela some ao recarregar (o YNAB dá erro `deviceAttributes` no console). Pela página da conta, grava. Recarregar a página logo depois de salvar também pode perder a transação.

### Roteiro 12 — estorno coberto em vários cartões

**Para que serve.** Quando as entradas no crédito (estornos) de uma categoria somam mais do que as saídas no crédito do mês, a diferença é um "estorno coberto" (`budgeted_credit_outflows` > 0) e sai das categorias de pagamento. Os roteiros 10 e 11 acharam como o **gasto** coberto se divide entre os cartões (a maior transação primeiro). Um **estorno** coberto em mais de um cartão nunca foi visto. O motor usa a mesma ordem do gasto (`funded_by_card`). Regras candidatas (as previsões saem de um teste temporário que implementou cada uma):
- **M — o maior estorno primeiro** (o motor hoje): o líquido da categoria (estornos − saídas) sai do pagamento dos cartões dos estornos, do maior estorno para o menor. As saídas da categoria no crédito não passam nada para o pagamento do cartão delas.
- **O — ordem de lançamento**: como o M, mas o estorno lançado antes vem primeiro. Neste roteiro coincide com "o menor primeiro".
- **P — proporção**: o líquido se divide na proporção dos estornos.
- **B — cada transação no próprio cartão**: cada estorno sai inteiro do pagamento do seu cartão, e cada saída passa inteira para o pagamento do seu.

Nas quatro vale, por cartão, o limite do roteiro 8 (`gasto_coberto_limitado`): o estorno só sai do pagamento na parte que reduz a dívida do cartão ou que vai para o "A atribuir"; a parte que só deixa o cartão positivo fica na categoria. Para o limite não esconder a divisão, os dois cartões dos estornos começam com dívida (gastos cobertos em Mercado). O 12C tira a dívida de um deles.

Passos:
- **12A**: estornos na Farmácia de 10 no Cartão e de 50 no Cartão 2, sem saída na Farmácia. As quatro dão o mesmo (os estornos saem inteiros); confere o ponto de partida.
- **12B**: saída de 30 na Farmácia no Cartão 3. Líquido 60 − 30 = 30. Separa as quatro.
- **12C**: apaga o gasto de Mercado do Cartão 2, que fica positivo (sem dívida). Confere o limite junto com a divisão.

Todas as datas são **14/10/2026 (hoje)**. Lance cada transação pela **página da própria conta** (barra da esquerda → a conta → Add Transaction), nunca pela tela All Accounts. Mande uma captura do plano de outubro depois de cada passo (e de novembro no fim do 12B e do 12C).

#### Ponto de partida

O TESTE no fim do roteiro 11: Corrente 720, Investimento 200; Cartão 0 (o Mercado de 10 em 13/10 gastou o +10 de setembro); Cartão 2 0; Cartão 3 −20 (+20 de setembro, Mercado 10 e Farmácia 30 em 14/10). Outubro com Farmácia 60 e Mercado 10 atribuídos.

**Passo 0 — limpeza.**

| # | Onde | O que fazer |
|---|---|---|
| 1 | Cartão 3 | Apagar o gasto de **10 em Mercado** (14/10) e o de **30 na Farmácia** (14/10) |
| 2 | Conta Corrente | Apagar o pagamento de **20** de **20/09** ("Payment: Cartão 3"). A ponta do Cartão 3 some junto |

Confira:

| Onde | Esperado |
|---|---|
| Corrente / Investimento | 740 / 200 |
| Cartão / Cartão 2 / Cartão 3 | 0 / 0 / 0 |
| Oct 2026: Farmácia (Assigned / Activity / Available) | 60 / 0 / 60 |
| Oct 2026: Mercado | 10 / −10 / 0 |
| Oct 2026: Pagamento do Cartão, do Cartão 2 e do Cartão 3 (Available) | 0 / 0 / 0 |
| Ready to Assign: Sep / Oct / Nov | 890 / 880 / 680 |

**Preparo — dívida nos cartões dos estornos.**

| # | Onde | O que fazer | Depois |
|---|---|---|---|
| P1 | Plan, Oct 2026 | Coluna **Assigned** de **Mercado**: de 10 para **150** | Mercado 150 / −10 / 140; Ready to Assign Oct / Nov **740 / 540** |

| # | Abrir a conta | Date | Payee | Category | Valor | Sai de → chega em | Depois de salvar |
|---|---|---|---|---|---|---|---|
| P2 | **Cartão 2** | 14/10/2026 | "Mercado X" | **Mercado** | **Outflow 100** | Cartão 2 (vira dívida) → ninguém | Cartão 2 **−100**; Mercado 150 / −110 / 40; Pagamento do Cartão 2 **100** |
| P3 | **Cartão** | 14/10/2026 | "Mercado X" | **Mercado** | **Outflow 40** | Cartão (vira dívida) → ninguém | Cartão **−40**; Mercado 150 / −150 / 0; Pagamento do Cartão **40** |

#### Passos

Em cada passo, anote: saldo dos três cartões; Activity e Available da Farmácia, do Mercado e dos três pagamentos (e a cor); Ready to Assign de outubro e de novembro.

**12A — dois estornos, sem saída na Farmácia.** Lance nesta ordem (o do Cartão antes).

| # | Abrir a conta | Date | Payee | Category | Valor | Sai de → chega em | Depois de salvar |
|---|---|---|---|---|---|---|---|
| A1 | **Cartão** | 14/10/2026 | "Farmácia X" | **Farmácia** | **Inflow 10** (entrada, não saída) | a loja → Cartão (reduz a dívida) | Cartão **−30**; Farmácia 60 / +10 / 70; Pagamento do Cartão **30** |
| A2 | **Cartão 2** | 14/10/2026 | "Farmácia X" | **Farmácia** | **Inflow 50** | a loja → Cartão 2 | Cartão 2 **−50**; Farmácia 60 / +60 / 120; Pagamento do Cartão 2 **50** |

Igual nas quatro candidatas; Ready to Assign 740 / 540. Se o pagamento continuar 40 ou 100, o estorno não saiu do pagamento mesmo com dívida (contradiz a primeira tentativa do 8E).

**12B — uma saída na Farmácia, num terceiro cartão.**

| # | Abrir a conta | Date | Payee | Category | Valor | Sai de → chega em | Depois de salvar |
|---|---|---|---|---|---|---|---|
| B1 | **Cartão 3** | 14/10/2026 | "Farmácia X" | **Farmácia** | **Outflow 30** | Cartão 3 (vira dívida) → ninguém | Cartão 3 **−30**; Farmácia 60 / +30 / 90 |

| Oct: Available | M | O | P | B |
|---|---|---|---|---|
| Pagamento do Cartão (estorno 10) | **40** | **30** | **35** | **30** |
| Pagamento do Cartão 2 (estorno 50) | **70** | **80** | **75** | **50** |
| Pagamento do Cartão 3 (saída 30) | **0** | **0** | **0** | **30** |

Iguais nas quatro: Farmácia 60 / +30 / 90 (Nov 90); Mercado 150 / −150 / 0; Ready to Assign 740 / 540; em novembro, os pagamentos com os mesmos valores de outubro. No M, o líquido de 30 sai todo do estorno de 50, e o de 10 não sai do pagamento. No B, os dois estornos saem inteiros e a saída de 30 passa para o Cartão 3: a soma dos pagamentos (110) fica igual à dívida dos três cartões (30 + 50 + 30). Se der outra divisão, anote o Activity dos três pagamentos.

**12C — o cartão do estorno maior fica sem dívida.**

| # | Onde | O que fazer | Depois |
|---|---|---|---|
| C1 | Cartão 2 | Apagar o gasto de **100 em Mercado** (P2) | Cartão 2 **+50**; Mercado 150 / −50 / 100 |

| Oct: Available | M | O | P | B |
|---|---|---|---|---|
| Pagamento do Cartão | **40** | **30** | **35** | **30** |
| Pagamento do Cartão 2 | 0 | 0 | 0 | 0 |
| Pagamento do Cartão 3 | **0** | **0** | **0** | **30** |

Iguais nas quatro: Farmácia 60 / +30 / 90; Ready to Assign 740 / 540. O Cartão 2 fica positivo, então o estorno dele não sai do pagamento (limite do roteiro 8 e do 10D). No M, os 30 do líquido vão para o estorno do Cartão 2 e ficam presos no limite; se o 12B der M e aqui der 30 no Cartão, o que o limite segura passa para o estorno seguinte. Se o Pagamento do Cartão 2 der −50 (vermelho), o limite não vale quando a categoria tem estornos em vários cartões.

#### Conferência (2026-10-14)

Teste `roteiro_12_conferido`. Lançado pelo usuário; vieram capturas do fim do 12B e do fim do 12C (o 0, o preparo e o 12A não foram capturados à parte, mas o estado final os confirma: o pagamento de setembro do Cartão 3 sumiu e a Corrente ficou em 740).
- **12B**: Cartão −30, Cartão 2 −50, Cartão 3 −30. Outubro: Farmácia 60 / +30 / 90, Mercado 150 / −150 / 0, **Pagamento do Cartão 30, do Cartão 2 50 e do Cartão 3 30** (verdes), Ready to Assign **740**; novembro com 30, 50 e 30, Farmácia 90, Ready to Assign **540**. Setembro: Ready to Assign 750 (890 − 140 atribuídos a mais em outubro). **Bate com B**; M (40 / 70 / 0), O (30 / 80 / 0) e P (35 / 75 / 0) descartadas.
- **12C**: Cartão 2 **+50**, e o cabeçalho dele mostrou Payment 0. Outubro: Mercado 150 / −50 / 100, Farmácia 90, **Pagamento do Cartão 30, do Cartão 2 0 e do Cartão 3 30**, Ready to Assign 740; novembro com 30, 0 e 30, Ready to Assign 540. **Bate com B** e com o limite do roteiro 8 por cartão.
- **Regra**: na categoria, os estornos no crédito **não se compensam** com as saídas. Cada estorno sai inteiro do pagamento do próprio cartão (até o limite de `gasto_coberto_limitado`), e cada saída coberta passa inteira para o pagamento do seu. A soma dos pagamentos fica igual à dívida dos cartões (12B: 30 + 50 + 30).
- **Motor ajustado**: `funded_by_card` em `src/calc.rs` dá a cada estorno o valor inteiro, e o gasto coberto (o `budgeted_credit_outflows` menos os estornos) vai para as saídas, da maior para a menor. Com a categoria estourada e estornos no mesmo mês, a regra é a mesma, mas esse caso **não foi visto** (roteiro 13).

### Roteiro 13 — categoria estourada com estorno no mesmo mês

**Para que serve.** O roteiro 12 mostrou que, numa categoria com saldo, cada estorno sai inteiro do pagamento do próprio cartão. Falta ver a categoria **estourada no crédito** com um estorno no mesmo mês. Candidatas (previsões de um teste temporário):
- **B** (o motor hoje): o estorno sai inteiro do pagamento do cartão dele, e o gasto coberto das saídas é o dinheiro da categoria **mais** o estorno, da maior saída para a menor.
- **N** (o motor antes do 12): o estorno não mexe em pagamento nenhum; o gasto coberto é só o dinheiro da categoria.
- **D**: o estorno sai do pagamento do cartão dele, mas o gasto coberto continua só o dinheiro da categoria (o estorno conta duas vezes contra os pagamentos).

Todas as datas são **14/10/2026**. Lance pela página da própria conta, nunca pela All Accounts.

#### Ponto de partida

O fim do roteiro 12: Cartão −30 (Mercado 40, estorno de 10 na Farmácia), Cartão 2 +50 (estorno de 50 na Farmácia), Cartão 3 −30 (Farmácia 30); Mercado 150 / −50 / 100; Farmácia 60 / +30 / 90.

**Passo 0 — limpeza.**

| # | Onde | O que fazer |
|---|---|---|
| 1 | Cartão 2 | Apagar o estorno de **50** na Farmácia (14/10) |
| 2 | Cartão 3 | Apagar a saída de **30** na Farmácia (14/10) |

Confira: Cartão / Cartão 2 / Cartão 3 **−30 / 0 / 0**; Farmácia 60 / +10 / **70**; Pagamento do Cartão **30**, do Cartão 2 e do Cartão 3 **0**; Ready to Assign 740 / 540. (Fica o estorno de 10 no Cartão, que tem dívida pelo Mercado de 40.)

#### Passos

**13A — a Farmácia estoura no crédito.**

| # | Abrir a conta | Date | Payee | Category | Valor | Sai de → chega em | Depois de salvar |
|---|---|---|---|---|---|---|---|
| A1 | **Cartão 3** | 14/10/2026 | "Farmácia X" | **Farmácia** | **Outflow 100** | Cartão 3 (vira dívida) → ninguém | Cartão 3 **−100**; Farmácia 60 / −90 / **−30** (amarelo) |

| Oct: Available | B | N | D |
|---|---|---|---|
| Pagamento do Cartão (estorno 10) | **30** | **40** | **30** |
| Pagamento do Cartão 3 (saída 100) | **70** | **60** | **60** |

Iguais nas três: Farmácia −30 amarelo (estouro no crédito); Ready to Assign 740 / 540; novembro com os mesmos pagamentos. No B, a soma dos pagamentos (100) é a dívida dos cartões (130) menos o estouro (30).

**13B — mais um estorno, num cartão sem dívida.**

| # | Abrir a conta | Date | Payee | Category | Valor | Sai de → chega em | Depois de salvar |
|---|---|---|---|---|---|---|---|
| B1 | **Cartão 2** | 14/10/2026 | "Farmácia X" | **Farmácia** | **Inflow 20** (entrada) | a loja → Cartão 2 | Cartão 2 **+20**; Farmácia 60 / −70 / **−10** (amarelo) |

| Oct: Available | B | N | D |
|---|---|---|---|
| Pagamento do Cartão | 30 | 40 | 30 |
| Pagamento do Cartão 2 | 0 | 0 | 0 |
| Pagamento do Cartão 3 | **90** | **60** | **60** |

Iguais nas três: Ready to Assign 740 / 540. O estorno no Cartão 2 não sai do pagamento (o cartão fica positivo, limite do roteiro 8). No B, ele ainda assim cobre 20 da saída do Cartão 3. Se der **70** no Cartão 3, o estorno que o limite segura não cobre as saídas.

Mande uma captura do plano de outubro e de novembro e da tela All Accounts no fim de cada passo.

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
- ~~Se uma entrada sem categoria vinda de acompanhamento entra no "A atribuir"~~: não entra, conferido no roteiro 4B. Ela fica na linha "sem categoria" com saldo positivo, que passa para o mês seguinte (roteiro 9B).
- Por que pagar o cartão além da dívida (saldo do cartão positivo) aumentou o "A atribuir" em 10 (roteiro 5C), e por que ele não voltou quando o crédito saiu do cartão (5D).
  - **Conferido no roteiro 8** (8B a 8E): o pagamento da dívida do mês anterior não soma; o que passa da dívida soma; gastar o crédito no mesmo mês desfaz; estorno depois do excesso soma; gasto sem dinheiro desfaz (8F).
- Estorno no cartão: só sai da categoria de pagamento na parte que reduz a dívida ou vai para o "A atribuir" (roteiro 8, três tentativas; `gasto_coberto_limitado`).
- Gasto no cartão que consome o saldo positivo trazido do mês anterior: essa parte conta como gasto em dinheiro da categoria e não passa para a categoria de pagamento (roteiros 9C, 9E e 10E; `credito_consumido`). O pagamento que recria o positivo no mesmo mês vai para o "A atribuir" (10E).
  - Regra usada em `src/motor.rs` (`pagamento_alem_da_divida`): a parte dos pagamentos do mês que deixa o cartão com saldo positivo, além do saldo positivo que ele já tinha no fim do mês anterior, vai para o "A atribuir" do mês (`additional_to_be_budgeted`). Ela continua saindo da categoria de pagamento. Os gastos do cartão contam; as transferências que saem dele para conta orçamentária não. Fecha com 5A–5D e novembro, mas só esse caso foi visto. Falta conferir na tela: pagar a mais com dívida de mês anterior, e gastar no cartão com saldo positivo.
- Os nomes internos `transfer_on_budget`, `transfer`, `transfer_off_budget` (66689): onde são usados.
- Regras de empréstimo (`setDebtTransactionTypeIfNeeded`, categoria da dívida).
- Se o servidor do YNAB valida ou conserta pares quebrados.
