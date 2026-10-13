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
- **Não verificado:** se uma entrada sem categoria (de acompanhamento para orçamentária) entra no "A atribuir". Roteiro 4B.

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
  - transferência do cartão para a corrente (saída do cartão): se volta para a categoria de pagamento (Activity positiva). Roteiro 5D.
  - pagamento vindo de conta de acompanhamento: não é transferência entre orçamentárias, então pede categoria no cartão e não conta como pagamento no painel. Roteiro não incluído.
  - pagamento maior do que o disponível: a categoria de pagamento fica negativa (estado `UnfundedPaymentMade`, análise 01 §3). Pela regra do estouro em dinheiro, deveria descontar do "A atribuir" do mês seguinte. Roteiro 5C.

## 5. Subtransações (divisão com transferência)

### Regras — visto (2327190, 2327706, 2329200, 2316535)

- Uma **parte** da divisão pode ser transferência. A espelho é uma transação inteira na outra conta, com `transfer_subtransaction_id` = a parte. O memo da espelho é o da parte.
- A transação principal **não pode** ser transferência entre orçamentárias e dividida ao mesmo tempo:
  - "This transaction is already a split transaction, so it can't be turned into a transfer…" (`OnBudgetTransferSplitError`);
  - "This transaction is already a transfer, so it cannot be split…" (`InvalidCategoryError`).
- A principal **pode** ser transferência para conta de acompanhamento e dividida entre categorias. Nesse caso nenhuma parte pode ser transferência ("This split transaction is already a transfer. Please remove the transfers from the sub-transactions.", `OffBudgetTransferSplitDupeError`).
- Na espelho de uma parte, o **valor não pode ser editado** ("There is an error in the transaction values.", `AmountEditedError`, `validateTransferToSubTransactionAmountNotModified`). O mesmo vale para a espelho de uma principal dividida (`validateTransferToSplitAmountNotModified`).
- A espelho de uma parte **não pode ser removida** (§2).
- Editar data ou conta na espelho de uma parte muda a data ou a conta da **divisão inteira** (§2).
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
| B tem X mas não tem Y no banco local | o cliente cria uma Y nova com outro id (§2); o servidor fica com **duas espelhos** | **não**: a Y antiga continua apontando para X |

- O `src/sync.rs` ignora a resposta de uma sincronização incremental se houve mudança local durante a requisição ("Incoming changes from delta sync were ignored…"). Isso não quebra o par, porque a próxima sincronização traz as duas pontas juntas.
- As duas pontas de um mesmo envio recebem o mesmo conhecimento do servidor, então `changed_since` as devolve juntas aos outros aparelhos. Isso já vale hoje no `src/server.rs`.

### O que o servidor do Tostão teria que garantir

1. **Lote atômico** (já faz): as duas pontas entram juntas ou nenhuma.
2. **Entregar as pontas juntas** (já faz): mesmo conhecimento para tudo o que veio no mesmo lote.
3. **Beneficiário de transferência por conta**: criar na criação da conta e renomear com ela, se o servidor for o dono disso (o bundle web não cria; ver decisões).
4. **Invariantes do par**, se decidirmos validar (ver decisões):
   - X aponta para Y ⇔ Y aponta de volta para X (por `transfer_transaction_id` ou `transfer_subtransaction_id`);
   - valores opostos; mesma data; conta de cada ponta = conta de transferência da outra;
   - beneficiário de cada ponta = beneficiário de transferência da conta da outra;
   - removida uma, removida a outra;
   - entre orçamentárias, nenhuma ponta com categoria; entre orçamentária e acompanhamento, categoria só na orçamentária.
5. **Cálculo**: como o servidor somos nós (análise 01 §1), o cálculo tem que:
   - ignorar transferências entre orçamentárias na receita e nas categorias;
   - contar as entradas no cartão vindas de conta orçamentária como pagamentos (saída da categoria de pagamento);
   - tratar a ponta orçamentária de uma transferência para acompanhamento como transação comum.

## 8. Resumo das regras

| Tema | Regra | Confiança |
|---|---|---|
| Modelo | Duas transações, cada uma com o id da outra; parte de divisão ↔ transação inteira | visto |
| Modelo | Agendada guarda uma ponta só; a outra é desenhada só em "All Accounts" | visto + conferido na tela |
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
- A espelho desenhada da agendada (§6) **não aparece** na tela da conta Poupança, mas **aparece em "All Accounts"**: Poupança, 18/10, entrada de 250, "Transfer: Conta Corrente", memo "reserva 2", "Category not needed". **Conferido.** Bate com o código, que a desenha só na lista de todas as contas (2920700).
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

### Roteiro 5 — cartão de crédito

Orçamento novo (ou desfaça os roteiros anteriores): Corrente 1.000; atribui 100 a Mercado ("A atribuir" 900).

| Passo | Ação | Corrente | Cartão | Mercado | Pagamento (Activity / disponível) | A atribuir | O que olhar |
|---|---|---|---|---|---|---|---|
| 5A | Gasta 60 no Cartão, em Mercado | 1.000 | −60 | 40 | +60 / 60 | 900 | (igual à análise 01, 2A) |
| 5B | **No Cartão**, entrada de 60 com beneficiário "Transfer : Corrente" | 940 | 0 | 40 | 0 / 0 | 900 | A ponta da Corrente nasce sem categoria. O pagamento lançado pelo lado do cartão conta igual. |
| 5C | Na Corrente, nova saída de 10, sem beneficiário, escolhendo como **categoria** "Pagamento do Cartão" | 930 | +10 | 40 | −10 / −10 ? | 900 | O beneficiário vira "Transfer : Cartão" e a categoria some ao salvar. A categoria de pagamento fica negativa (pagou sem dinheiro reservado). Anotar o "A atribuir" de **novembro**: esperado 890 ? |
| 5D | No Cartão, saída de 10 para "Transfer : Corrente" (devolve o crédito) | 940 | 0 | 40 | 0 / 0 ? | 900 | Anotar se a saída do cartão volta para a categoria de pagamento (Activity) ou fica fora. |

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

### Roteiro 7 — agendada

| Passo | Ação | Corrente | Poupança | O que olhar |
|---|---|---|---|---|
| 7A | Na Corrente, agendada mensal de 50 para "Transfer : Poupança", data de amanhã | 1.000 | 0 | Na Poupança aparece uma agendada de +50 sem categoria. |
| 7B | Na Poupança, abre essa agendada e muda o memo | 1.000 | 0 | O memo muda na agendada da Corrente (só existe uma). |
| 7C | Na Corrente, "Enter now" (lançar agora) na agendada | 950 | 50 | As duas pontas nascem; a da Poupança não compensada. |
| 7D | Se alguma ponta vier "a aprovar", aprova só uma | 950 | 50 | A outra também fica aprovada. |

## 10. Implicações para o Tostão

- **Modelo**: manter o do YNAB (duas transações com id cruzado; parte de divisão ↔ transação inteira; agendada com uma ponta só). Ele já passa pelo protocolo sem mudança e o `src/server.rs` guarda tudo sem conhecer transferências.
- **Núcleo compartilhado**: precisamos de uma função "salvar transação" que faça o papel da `Q` (criar, propagar, mover, remover a espelho). Ela é do aparelho, não do servidor, para funcionar offline.
- **Cálculo** (`src/calc.rs`): hoje recebe somas prontas. Falta a etapa que classifica cada transação: transferência entre orçamentárias (fora da receita e das categorias), pagamento de cartão (saída da categoria de pagamento), transferência com acompanhamento (transação comum com categoria).
- **Ids determinísticos** das pontas geradas pela agendada (`{ag}_t_{data}`): reproduzir, pelo mesmo motivo da análise 02 (evitar duplicata entre aparelhos).
- **Sincronização**: o modelo "última versão inteira vence" preserva a coerência do par quando o aparelho manda as duas pontas juntas, mas perde edições e pode ressuscitar transferências removidas. O caso perigoso é o aparelho criar uma espelho nova por não ter a antiga no banco local.
- **Testes** antes de implementar (em `tests/`): criação, edição de cada campo da tabela da §2, troca de destino, remoção pelos dois lados, divisão, agendada com ids, e dois aparelhos editando pontas diferentes contra o `SyncServer` em memória.

## 11. Decisões para você

1. **O servidor valida o par?** Opções: (a) confiar no aparelho, como o YNAB parece fazer; (b) recusar o lote que quebra os invariantes da §7; (c) consertar sozinho (criar ou remover a espelho). Eu recomendaria (b): é simples, evita estado ruim e o aparelho pode tentar de novo.
2. **Quem cria o beneficiário de transferência** de cada conta: o aparelho (junto com a conta, no mesmo lote) ou o servidor? Recomendo o aparelho, porque o servidor do Tostão hoje não cria entidades, só guarda.
3. **Conflitos de edição**: manter "última versão inteira vence" ou mesclar por campo? Por campo evitaria perder o memo e o estado, mas é bem mais trabalho. Para uso da família, dá para começar com a versão inteira.
4. **Remoção contra edição**: aceitar que uma edição atrasada ressuscite uma transferência removida, ou fazer a remoção vencer sempre no servidor? Recomendo fazer a remoção vencer.
5. **Espelho que não está no banco local**: reproduzir o YNAB (cria outra) ou recusar o salvamento até sincronizar? Recomendo recusar e sincronizar antes.
6. **Estado de compensação não propaga**: reproduzir? Recomendo sim, porque cada ponta é conferida contra o extrato da sua conta.
7. **Editar a espelho de uma parte muda a data da divisão inteira**: reproduzir ou bloquear a edição de data na espelho?

## 12. O que ainda não achamos

- Onde o beneficiário de transferência é criado (provavelmente no servidor do YNAB).
- Quem chama `setTransferCounterpartClearedStatus` e `getOtherSideReconciledTransfers` (talvez outro chunk do bundle).
- O caso "B existe mas não aponta de volta para A": quando acontece.
- A soma do servidor para receita e categorias com transferências (só inferida pelos painéis do cliente).
- Se uma entrada sem categoria vinda de acompanhamento entra no "A atribuir" (roteiro 4B).
- Se uma saída do cartão para a corrente volta para a categoria de pagamento (roteiro 5D).
- Os nomes internos `transfer_on_budget`, `transfer`, `transfer_off_budget` (66689): onde são usados.
- Regras de empréstimo (`setDebtTransactionTypeIfNeeded`, categoria da dívida).
- Se o servidor do YNAB valida ou conserta pares quebrados.
