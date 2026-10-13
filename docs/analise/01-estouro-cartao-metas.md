# Análise 01 — estouro, cartão de crédito, metas e meses futuros

Fonte: bundle web `chunk.7bd67ec4d1cff4ed86e3.js` (o mesmo de `src/lib.rs`), lido em 2026-10-13.
Os números entre parênteses são ids de módulo webpack ou offsets no arquivo, para reencontrar o trecho.
As regras abaixo estão descritas com nossas palavras; o código do YNAB não foi copiado.

Legenda de confiança:
- **visto**: a fórmula está no bundle e foi lida;
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

### Do mês (`overSpent` do `monthlyBudgetCalculation`) — não achado, inferido

- O valor só chega pronto do servidor (`over_spent`). Nenhum código do bundle o calcula.
- Pelo uso, `disponível(mês) = disponível(mês-1) + overSpent(mês-1) + receita + adicional − atribuído`, e pela tela "Cash overspending in {mês anterior}", a regra provável é:

  > `overSpent(mês)` = soma, nas categorias do mês, do **estouro em dinheiro** (valor ≤ 0). O estouro no crédito não entra; ele fica como dívida no cartão.

- Falta decidir se as saídas sem categoria (`uncategorizedCashOutflows`, guardadas no mês) também entram. A sugestão é que sim: dinheiro gasto sem categoria também sai do "A atribuir".

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

### Movimento do dinheiro para o pagamento — inferido

O mecanismo central do cartão não está no web:
- quando se gasta no cartão numa categoria que tem dinheiro, o valor coberto (`budgetedCreditOutflows`) passa da categoria para a categoria de pagamento;
- quando se paga o cartão, o valor sai da categoria de pagamento.

Proposta para o Tostão:

> saldo(pagamento do cartão X, mês) = saldo positivo do mês anterior + atribuído + Σ |saídas no crédito cobertas| das transações da conta X no mês − Σ pagamentos da conta X no mês.

Precisa de teste com casos reais antes de fixar.

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
- Não está claro se o atribuído das duas pontas muda junto (o mais provável) ou se é derivado das movimentações. Precisa ser verificado no fluxo de "mover dinheiro" da tela.

## 7. Transações agendadas — visto (offsets 1835062 e 1980068)

- Frequências: nunca, diária, semanal, a cada 2 semanas, duas vezes por mês, a cada 4 semanas, mensal, a cada 2 meses, a cada 3 meses, a cada 4 meses, duas vezes por ano, anual, a cada 2 anos.
- Guardam as próximas datas calculadas e podem ter subtransações (divisão).
- Transações têm uma **origem**: agendador, importação crua, pendente, importada, casada etc.
  - O painel do cartão ignora as importadas ainda não aceitas.
- As "transações futuras" (`upcomingTransactions`) entram na falta total e no excesso das metas.

## 8. O que falta e próximos alvos

Ainda não achamos:

1. a fórmula do `overSpent` do mês (provável regra acima);
2. o cálculo do saldo da categoria de pagamento do cartão (proposta acima);
3. a geração das próximas datas das transações agendadas;
4. como a movimentação de dinheiro altera o atribuído;
5. o modo de arredondamento da moeda (`roundMilliDollars`).

Para os itens 2 a 4, o próximo passo é ler a biblioteca Kotlin do bundle (offsets ~1,30M–1,45M) com o REA (`trace_javascript_semantics`) e observar a tela num orçamento de teste.

Outras funcionalidades ainda não olhadas:
- contas de empréstimo/hipoteca (`isLoanAccount`, reconciliação);
- regras de renomear beneficiário (`be_payee_rename_conditions`);
- idade do dinheiro (`ageOfMoney`);
- receita esperada (`be_expected_income`);
- reconciliação de contas.
