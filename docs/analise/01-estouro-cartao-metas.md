# Análise 01 — estouro, cartão de crédito, metas e meses futuros

Fonte: bundle web `chunk.7bd67ec4d1cff4ed86e3.js` (o mesmo de `src/lib.rs`), lido em 2026-10-13.
Os números entre parênteses são ids de módulo webpack ou offsets no arquivo, para reencontrar o trecho.
As regras abaixo estão descritas com nossas palavras; o código do YNAB não foi copiado.

Legenda de confiança:
- **visto**: a fórmula está no bundle e foi lida;
- **reproduzido**: a regra foi escrita em Rust e testada com casos concretos (`tests/calc.rs`, `tests/goals.rs`);
- **conferido na tela**: o resultado foi observado num orçamento de teste do YNAB e bate com a regra;
- **inferido**: deduzido de nomes, textos de tela e uso, sem a fórmula à vista;
- **não achado**: não está no bundle web.

## 1. Onde os cálculos rodam

- No app web, `performCalculationsLocallyOrOnServer` só dispara uma sincronização (offset 2790499). **visto**
  Quem calcula de verdade é o servidor, que devolve as entidades `be_*_calculations`.
- O bundle traz uma classe de cálculo de categorias compartilhada com os apps móveis (`SubcategoryCalculations`, módulo 80211).
  Os métodos que carregam dados do banco local (`loadData`, `loadPaymentSubCategoryData`, `calculateAggregatesFromPriorMonths`) não estão implementados no web. **visto**
- Há também uma biblioteca em Kotlin compilada para JS (nomes como `x3f_1`, offsets ~1,30M–1,45M) com estruturas `Calculations(...)` e campos do cartão.
  Os nomes estão embaralhados e ela não foi lida a fundo.

Consequência para o Tostão: o nosso servidor (ou o núcleo compartilhado) precisa calcular tudo.
Não dá para copiar o "deixa o servidor calcular" do YNAB, porque o servidor somos nós.

## 2. Estouro

### Por categoria — visto (80211, 87364)

- **Disponível da categoria** = saldo do cálculo do mês (`balance`, já em `src/calc.rs`).
- **Estouro em dinheiro** = `min(max(saídas_em_dinheiro_não_cobertas, disponível), 0)`.
- **Estouro no crédito** = `min(max(saídas_no_crédito_não_cobertas, disponível), 0)`.
  - As "não cobertas" vêm de `calculate_budgeted_cash_credit_outflows`, já implementada.
- Uma categoria "está estourada" quando o estouro em dinheiro ou no crédito é < 0.
  - A tela mostra "estourou" quando o disponível é < 0.
- Mensagens da tela:
  - estouro em dinheiro "será descontado do que você tem para atribuir no mês seguinte";
  - estouro no crédito "vira dívida no cartão".

### Do mês (`overSpent` do `monthlyBudgetCalculation`) — soma não achada no cliente; regra conferida na tela

- **Visto**: o cliente nunca calcula `overSpent`. Ele só lê e copia o campo do servidor `over_spent` (offsets 1795626, 1884453, 1886616, 1887272). O `BudgetMonth` da biblioteca Kotlin (offset 1374956, construtor em ~1065362) também não tem o campo. Por isso a soma **não está no bundle**.
- **Visto**: o disponível do mês seguinte usa o `overSpent` do mês anterior: `availableToBudget = o + u + (receita + adicional − atribuído)`, com `u = overSpent` (offset 1795822); e `overSpentInPreviousMonth` (offset 2726708).
- **Visto**: a linha "sem categoria" (`budgetMonthDisplayItemForUncategorizedTransactions`, criada em 2993749 e 3007392 com a mesma classe de item das categorias) entra em `hasAnyCashOverSpending` (offset 2993229). Essa checagem só olha `isOverSpentOnCash`, ou seja, o estouro **em dinheiro**. A linha só aparece quando `uncategorizedBalance` ou `uncategorizedOutflows` é diferente de zero (offset 2990948).
- **Regra (conferida na tela em 2026-10-13, ver abaixo)**:

  > `overSpent(mês)` = soma do **estouro em dinheiro** das categorias do mês, mais o estouro em dinheiro da linha "sem categoria" (≤ 0). O estouro no crédito não entra: vira dívida no cartão. A linha "sem categoria" tem atribuído 0, então o estouro dela é `min(saldo positivo do mês anterior + saídas em dinheiro sem categoria, 0)`. **Conferido na tela** (análise 03, roteiro 9B): como uma categoria, ela carrega o saldo positivo para o mês seguinte ("Left Over from Last Month"); o negativo não passa.

- **Saídas sem categoria**: entram, só as **em dinheiro**. A tela mostra uma linha só ("Uncategorized Transactions") somando dinheiro e cartão, mas só a parte em dinheiro passa para o mês seguinte.
- Em `src/calc.rs`: `month_over_spent` recebe a linha "sem categoria" (`uncategorized_row`). Testes em `tests/calc.rs`: `uncategorized_cash_outflows_count_as_cash_overspending` e `month_overspending_matches_ynab_screen_roteiro_1`.

**Conferência na tela (2026-10-13)**, orçamento de teste com corrente de 1.000 (receita), cartão com saldo 0 e nada atribuído em novembro:

| Passo | Ação em outubro | Outubro na tela | "A atribuir" de novembro |
|---|---|---|---|
| — | Só o saldo inicial | A atribuir 1.000 | — |
| 1A | Atribui 50 a Mercado e gasta 80 na corrente | Mercado −30; A atribuir 950 | 920 |
| 1B | Gasta 40 na corrente, sem categoria | Sem categoria −40; A atribuir 950 | 880 |
| 1C | Gasta 60 no cartão, em Lazer (nada atribuído) | Lazer −60 (aviso de crédito); cartão −60 | 880 |
| 1D | Gasta 25 no cartão, sem categoria | Sem categoria −65; cartão −85 | 880 |

Também se viu que:
- o "A atribuir" de outubro não muda com o estouro;
- em novembro, Mercado e Lazer começam em 0, porque o saldo negativo não passa adiante;
- a categoria de pagamento do cartão fica em 0, marcada em amarelo (falta), com a dívida de 85 só no saldo da conta.

## 3. Cartão de crédito

### Tipos de categoria — visto (34644)

- `DFT` é a categoria normal.
- `DBT` é a categoria de **pagamento do cartão**, uma por conta de cartão (`subCategory.accountId`).
  - Ela recebe metas próprias.
  - Os campos `budgetedSpending`, `allSpending`, `allSpendingSinceLastPayment`, `paymentAverage`, `positiveCashOutflows` e `additionalToBeBudgeted` só valem para ela; nas outras são zerados.
- No ciclo de cálculo (80211), as categorias normais são calculadas primeiro e as de pagamento do cartão depois, porque dependem dos gastos das outras.

### Painel da categoria de pagamento — visto (21200)

Considera as transações da conta do cartão no mês, ignorando as que ainda não foram aceitas (importadas cruas).

| Valor | Regra |
|---|---|
| saldo do cartão | saldo da conta (negativo = dívida) |
| disponível para pagar | saldo da categoria de pagamento no mês |
| gasto coberto no mês | −`budgetedSpending` do cálculo |
| pagamentos no mês | soma das transferências recebidas de contas do orçamento (entradas que são transferência) |
| sem categoria | \|min(0, soma das partes de transação sem categoria e que não são transferência)\| |
| estouro no crédito | \|min(0, soma das partes com categoria — exceto transferências, receita e juros do cartão — + gasto coberto)\| |
| falta atribuir (total) | `underfundedAmount` da categoria |
| falta atribuir (restante) | max(0, total − sem categoria − estouro no crédito) |
| juros no mês | saídas classificadas na categoria de juros do cartão |

Estado do pagamento (66883), na ordem em que é decidido:
1. disponível < 0 → `UnfundedPaymentMade` (pagou mais do que tinha reservado);
2. saldo do cartão ≥ 0 → `FullyPaid`;
3. disponível ≥ dívida → `FullyOrOverFunded`;
4. há valor sem categoria → `UncategorizedTransactions`;
5. há estouro no crédito → `CreditOverspending`;
6. senão → `Underfunded`.

### Saldo da categoria de pagamento — visto em parte, regra conferida na tela

O que foi visto no bundle:
- **Saldo genérico** (módulo 80211, `calculateBalance`, offset 1810221; a conta em 1810483): `saldo = max(saldo do mês anterior, 0) + atribuído + saídas em dinheiro + saídas no crédito`, arredondado com `roundMilliDollars`. Vale para todas as categorias, inclusive a de pagamento; o saldo negativo do mês anterior não passa.
- **Só na categoria de pagamento** (offset 1809945): `budgetedSpending`, `allSpending`, `allSpendingSinceLastPayment`, `paymentAverage`, `positiveCashOutflows` e `additionalToBeBudgeted` são mantidos. Nas outras categorias são zerados (`null`).
- **Gasto coberto**: a tela mostra `−budgetedSpending` como "gasto financiado no cartão" (`fundedSpendingOnCard`, offset 2192967; `fundedSpendingThisMonth`, offset 2133352). Então `budgetedSpending` é um valor ≤ 0 vindo do servidor.

O que não foi visto:
- o servidor monta as saídas no crédito da categoria de pagamento (quanto dos gastos no cartão passou para ela) e o `budgetedSpending` a partir das transações. O bundle só recebe esses valores;
- o sinal dos pagamentos dentro da categoria (saída, valor ≤ 0). O painel mostra o módulo do valor.

**Regra (conferida na tela em 2026-10-13, ver abaixo)**:

> saldo(pagamento do cartão X, mês) = saldo positivo do mês anterior + atribuído + saídas em dinheiro da categoria (pagamentos, ≤ 0) − `budgetedSpending` (gasto coberto, ≤ 0 → soma seu módulo).

Ela é igual à fórmula genérica do bundle se as saídas no crédito da categoria de pagamento forem −`budgetedSpending`. Isso não aparece no bundle, mas a tela confirma: a coluna "Activity" da categoria de pagamento mostra o gasto coberto (positivo) mais os pagamentos (negativos). Em `src/calc.rs` (`calculate_credit_card_payment_balance`), testes em `tests/calc.rs` (`credit_card_payment_collects_funded_spending_and_pays_out` e `credit_card_payment_matches_ynab_screen_roteiro_2`).

Só a parte **coberta** do gasto passa para o pagamento. Quando a categoria tem menos dinheiro que o gasto, o resto vira estouro no crédito (passo 2D).

**Conferência na tela (2026-10-13)**, orçamento de teste com corrente de 1.000 (receita), cartão com saldo 0 e 100 atribuídos a Mercado (A atribuir 900):

| Passo | Ação | Mercado | Pagamento (Activity / disponível) | Cartão | A atribuir |
|---|---|---|---|---|---|
| 2A | Gasta 30 no cartão, em Mercado | 70 | +30 / 30 | −30 | 900 |
| 2B | Paga 30 da corrente para o cartão | 70 | 0 / 0 | 0 | 900 |
| 2C | Gasta 30 no cartão, em Lazer (sem dinheiro) | 70 | 0 / 0 (amarelo) | −30 | 900 |
| 2D | Atribui 20 a Farmácia e gasta 50 no cartão nela | 70 | +20 / 20 (amarelo) | −80 | 880 |
| 2E | Novembro, sem novos gastos | 70 | 0 / 20 | −80 | 880 |

Também se viu que:
- em novembro, Lazer (−30) e Farmácia (−30) começam em 0, e o estouro no crédito não desconta do "A atribuir";
- "Left Over from Last Month" de novembro é 90: os 70 do Mercado mais os 20 do pagamento;
- a "Activity" de outubro no resumo (−90) inclui a da categoria de pagamento (+20).

## 4. Metas

### Tipos — visto (32047)

| Código | Nome (proposta em português) | Ideia |
|---|---|---|
| `MF` | Valor mensal | atribuir X todo mês |
| `TB` | Saldo alvo | chegar a um saldo X, sem prazo |
| `TBD` | Saldo alvo até a data | chegar a X até uma data |
| `NEED` | Necessário para gastar | precisar de X por período (semana, mês, ano…), com repetição |
| `DEBT` | Pagamento de dívida | pagar X por mês |

- Cadências (20771): nenhuma, mensal, semanal, a cada 2–11 meses, anual, a cada 2 anos.
  - Com `goalCadenceFrequency`: semanal conta como 1 mês, anual vale 12×freq meses, e as outras valem freq meses.
- Dia padrão da semana: 6 (19386), provavelmente sábado.
- Campos da categoria usados nas metas:
  - tipo, criada em, data alvo, valor alvo, valor mensal;
  - cadência, frequência, dia;
  - "precisa do valor inteiro" (`goalNeedsWholeAmount`, só para NEED);
  - adiada (`isGoalSnoozed`).

### Cálculo por categoria e mês — visto (80211, `calculateGoals`)

Entradas: o cálculo do mês da categoria (atribuído, saldo, saídas) e o do mês anterior.

1. Se não há data de criação, ou o mês é anterior ao mês de criação, todos os campos de meta ficam em 0.
2. **Período atual** (`calculateCurrentGoalPeriod`), dado o mês M:
   - MF: o mês M inteiro.
   - DEBT, ou mensal sem data alvo: o mês M, terminando no dia `goalDay` se ele vier antes do fim do mês.
   - Semanal sem data alvo: a semana que termina no próximo dia `goalDay`, recortada aos limites do mês.
   - Com cadência e data alvo já passada: o período anda em passos da cadência a partir da data alvo.
     - Por meses: começa no mês seguinte ao da data alvo, ou no último múltiplo da cadência antes de M.
     - Semanal: alinhado em semanas.
   - Senão: do mês de criação até a data alvo.
   - **meses restantes** = 0 se M já passou do fim do período; senão, meses de M até o fim + 1.
3. **Alvo efetivo**:
   - MF: o valor mensal.
   - DEBT: o valor alvo.
   - Categoria de pagamento do cartão (outros tipos): |min(saldo da conta, 0)|, ou seja, a dívida inteira.
   - NEED semanal: valor × número de ocorrências do dia da semana no mês, contadas a partir da criação se a meta foi criada no mês.
   - Senão: o valor alvo.
4. **Financiado e saídas acumulados no período**:
   - MF e DEBT: só o mês (atribuído e saídas do mês).
   - Outros, se M está dentro do período: atribuído do mês mais o do mês anterior.
     - No primeiro mês do período, soma-se o saldo positivo do mês anterior, exceto em NEED com "valor inteiro".
     - Nos meses seguintes, soma-se o financiado acumulado do mês anterior.
     - As saídas acumulam do mesmo jeito.
   - Fora do período: 0.
5. **Quanto falta no total**:
   - MF e DEBT: max(0, alvo − atribuído).
   - NEED: max(0, alvo − financiado acumulado).
   - Outros: max(0, alvo − saldo).
6. **Previsão de conclusão** (em meses):
   - sem tipo: nenhuma;
   - TB: arredondar para cima (falta / atribuído), se ambos > 0, senão 0;
   - outros: os meses restantes.
7. **Quanto atribuir este mês** (`goalTarget`):
   - MF e DEBT: o alvo.
   - TBD e NEED: o que faltava antes deste mês dividido pelos meses restantes, arredondado para cima na moeda.
     - O "que faltava" é alvo − saldo + atribuído; em NEED, alvo − (financiado − atribuído).
   - TB: 0. Saldo alvo sem data não pede valor por mês.
8. **Falta atribuir no mês** (`goalUnderFunded`):
   - MF e DEBT: o que falta no total.
   - TBD e NEED: 0 se não restam meses. Se o atribuído é negativo e o que faltava também, usa o que falta no total. Senão, max(0, quanto atribuir − atribuído).
   - TB: 0.
9. **Percentual** (0–100, truncado; divisão inválida vale 100):
   - MF, DEBT e NEED: financiado / alvo.
   - TB e TBD na categoria de pagamento do cartão: financiado / (falta + financiado).
   - TB e TBD nas outras: saldo / alvo.

Totais usados na tela (80211):
- **Falta total**:
  - Meta adiada vale 0.
  - Senão, é o "falta atribuir no mês" (na categoria de pagamento do cartão, a falta do painel do cartão), aumentado pelo que o saldo + transações agendadas do mês ficarem negativos.
- **Excesso**:
  - Com valor por mês > 0, é o que foi atribuído acima dele.
  - Sem valor por mês, é o financiado acima do alvo efetivo. Em TB, conta o saldo do mês anterior mais o atribuído contra o alvo mais as saídas.
  - Nos dois casos, o excesso fica limitado ao que sobra de saldo depois das transações agendadas.

### Arredondamento da moeda (`roundMilliDollars`) — visto, reproduzido

- Definição (`CurrencyFormatter`, offset 2249444): `roundMilliDollars(t, e = Math.round) = e(t·n) / n`, com `n = 1 / passo`.
- Passo (offset 2251904, `getMinimumAmount`): `10^(3 − min(3, casas decimais))` milésimos. Para o real (2 casas): **10 milésimos**, ou seja, centavo. Moeda com 3 casas ou mais: passo 1 (não arredonda).
- **Modo padrão: `Math.round` do JS**, que arredonda a metade para o lado de +∞ (−2,5 vira −2; −1,5 vira −1). Não é o arredondamento "para longe de zero" do Rust (`f64::round`).
- Usos vistos:
  - saldo da categoria (`calculateBalance`, 1810483) e disponível do mês (`availableToBudget`, 1795822): modo padrão;
  - "quanto atribuir" das metas TBD e NEED (`goalTarget`, 1813399 e 1980571): **`Math.ceil`**. Antes disso, o valor já passa por `Math.ceil(necessário / meses restantes)`.
- Em Rust: `calc::js_round` (= `Math.round`) e `calc::round_to_currency` (= `roundMilliDollars`). Testes: `round_mode_of_the_ynab_is_math_round_half_toward_positive_infinity`, `rounds_to_currency_digits` (tests/calc.rs) e `target_balance_by_date_spreads_over_remaining_months` (tests/goals.rs, 166.670 = 1.000,00 em 6 meses arredondado para cima no centavo).
- Assumido: o real tem 2 casas decimais (moeda do app).

## 5. Meses futuros e "A atribuir" em cada mês — visto (944)

- O painel de planejamento lista os meses depois do mês em foco até o último mês com alguma atribuição ou até o primeiro com "A atribuir" negativo, o que vier depois.
  - Se não há meses futuros, mostra só o próximo.
- Para cada mês:
  - atribuído total;
  - "A atribuir" do mês (mesma fórmula de `src/calc.rs`);
  - **proporção financiada** = Σ min(max(atribuído, 0), alvo do mês) ÷ Σ alvo do mês, nas categorias com meta válida no mês.
    - Arredondada para 2 casas.
    - Valores entre 0,99 e 1 viram 0,99 e valores acima de 0 que arredondariam para 0 viram 0,01, para não mostrar 100% ou 0% enganosos.
- Só entram categorias visíveis, não removidas e "orçáveis".
- "Há estouro em dinheiro" no mês: alguma categoria com saldo < 0 e estouro em dinheiro < 0.

## 6. Movimentação de dinheiro — visto (entidades, offsets 1942984 e 1947179)

- **Movimentação**: de um atribuído de categoria-mês para outro, com valor, origem, quem fez, quando começou e quando foi aceita, nota e grupo.
  - Origem, destino, valor e início não podem mudar depois de criados.
  - É um registro histórico, não uma edição.
- **Grupo de movimentações**: mês, origem, quem fez, quando, nota e, opcionalmente, a categoria removida que originou o grupo.
  - Ao remover uma categoria, o dinheiro dela é movido em grupo.
- O atribuído das duas pontas muda junto, e a movimentação é o registro (resolvido na análise 02, §1).

## 7. Transações agendadas — visto (offsets 1835062 e 1980068)

- Frequências: nunca, diária, semanal, a cada 2 semanas, duas vezes por mês, a cada 4 semanas, mensal, a cada 2 meses, a cada 3 meses, a cada 4 meses, duas vezes por ano, anual, a cada 2 anos.
- Guardam as próximas datas calculadas e podem ter subtransações (divisão).
- Transações têm uma **origem**: agendador, importação crua, pendente, importada, casada etc.
  - O painel do cartão ignora as importadas ainda não aceitas.
- As "transações futuras" (`upcomingTransactions`) entram na falta total e no excesso das metas.
- Geração das datas, identidade das transações geradas e divergência entre planejamento e geração: análise 02, §2.

## 8. O que falta e próximos alvos

Já implementado no Tostão, com testes em `tests/goals.rs` e `tests/calc.rs`:
- **Metas** (seção 4), em `src/goals.rs`. Os totais de falta também, sem a parte do painel do cartão.
- **Estouro** por categoria e do mês (seção 2), em `src/calc.rs`.
  - O estouro do mês soma o estouro em dinheiro das categorias e da linha "sem categoria" (conferido na tela).
- **Saldo da categoria de pagamento do cartão** (seção 3), em `src/calc.rs`.
  - Quando uma categoria usa vários cartões, a parte coberta é dividida entre eles na proporção do gasto. Essa regra é nossa.
- **Arredondamento** (seção 4): `js_round` (Math.round) e `round_to_currency`, em `src/calc.rs`.
- **Datas** com as regras do moment.js, em `src/dates.rs`.
- Diferença conhecida: na meta semanal com frequência > 1, o YNAB pode chegar a frações de milésimo; o Tostão arredonda.

Estado dos três pontos em aberto (2026-10-13):

1. **`overSpent` do mês** — **conferido na tela** (roteiro 1, seção 2). Regra: estouro em dinheiro das categorias + estouro em dinheiro da linha "sem categoria". O estouro no crédito fica de fora. A soma não está no bundle (o servidor calcula).
2. **Saldo da categoria de pagamento do cartão** — **conferido na tela** (roteiro 2, seção 3); a parte genérica é **visto** (`calculateBalance`, offset 1810221). Regra: saldo positivo do mês anterior + atribuído + pagamentos (saídas em dinheiro) − `budgetedSpending`. Só a parte coberta do gasto passa para o pagamento.
3. **Arredondamento (`roundMilliDollars`)** — **visto e reproduzido**. Modo: `Math.round` (metade para +∞) no saldo e no disponível; `Math.ceil` na meta TBD/NEED. Passo: 10 milésimos para o real. Seção 4.

Resolvidos na análise 02: a geração das datas das agendadas (§2), como a movimentação altera o atribuído (§1) e a reconciliação de contas (§3).

Os três pontos estão fechados. O motor que monta essas somas a partir das transações está em `src/motor.rs`; os roteiros 1 e 2 estão refeitos a partir de transações em `tests/motor.rs`. Ainda não testamos na tela uma categoria que gasta em **dois cartões** no mesmo mês. A divisão proporcional da parte coberta entre eles é regra nossa e **não bateu com a tela** (análise 03, roteiro 9E: 60 cobertos de 30 + 70 viraram 10 e 50, não 18 e 42). A regra do YNAB ainda não foi achada. A biblioteca Kotlin (offsets ~1,30M–1,45M) foi lida em parte: tem `Calculations` com os campos do cartão (1312078–1312411), mas a lógica da soma do mês não aparece nela.

Outras funcionalidades ainda não olhadas:
- regras de empréstimo/hipoteca além da reconciliação (`isLoanAccount`);
- regras de renomear beneficiário (`be_payee_rename_conditions`);
- idade do dinheiro (`ageOfMoney`);
- receita esperada (`be_expected_income`).

Olhadas depois: transferências entre contas, inclusive o pagamento do cartão como transferência, na análise 03 (`03-transferencias.md`). Os detalhes de empréstimo que apareceram lá (`setDebtTransactionTypeIfNeeded`, categoria da dívida) continuam no primeiro item acima.
