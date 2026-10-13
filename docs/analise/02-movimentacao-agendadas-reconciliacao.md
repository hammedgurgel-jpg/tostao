# Análise 02 — movimentação de dinheiro, datas das transações agendadas e reconciliação

Fonte: o mesmo bundle web `chunk.7bd67ec4d1cff4ed86e3.js` da análise 01, lido em 2026-10-13.
Os números entre parênteses são offsets aproximados no arquivo (para reencontrar o trecho com `node`).
As regras estão descritas com nossas palavras; o código do YNAB não foi copiado.

Legenda de confiança (igual à análise 01):
- **visto**: lido no bundle;
- **reproduzido**: o trecho foi portado para JS e rodado com casos de teste (ver §5);
- **inferido**: deduzido de nomes e uso, sem a lógica à vista;
- **não achado**: não está no bundle web.

## 1. Movimentação de dinheiro

### Entidades — visto (1942984 e 1947179)

- **Movimentação** (`MoneyMovement`): origem e destino (cada um um "atribuído" de categoria no mês, ou nulo), valor, fonte, quem fez, quando começou, quando foi aceita, nota e grupo.
- **Grupo de movimentações** (`MoneyMovementGroup`): mês, fonte, quem fez, quando criou, nota e, se for exclusão de categoria, a categoria removida.
- Origem, destino, valor e início **não mudam** depois de criados (os setters lançam erro). É um registro histórico.

### Nulo é "A atribuir" — visto (39168, 2371521)

- Origem ou destino nulo significa "A atribuir" (o dinheiro ainda não atribuído no mês).
- Categoria que não é orçável (`isBudgetable`) também é tratada como nula.
- Se origem e destino são nulos, a operação lança erro. Se são a mesma ponta, ou o valor é zero, nada acontece.

### Mover dinheiro — visto (2371521, `moveMoneyBetweenMonthlySubCategoryBudgets`)

Parâmetros: origem, destino, valor, grupo, nota, fonte, se deve atualizar o atribuído e a data de início.

1. Valor negativo: troca origem e destino e inverte o sinal. A movimentação guarda sempre valor positivo.
2. Fonte padrão: `manual_movement` se há origem; `manual_assign` se não há.
3. Cria o registro da movimentação.
4. Se "atualizar o atribuído" é verdadeiro: **origem.atribuído −= valor; destino.atribuído += valor**.

### Editar o atribuído direto — visto (2374333, `saveChanges` do editor da categoria-mês)

- O editor já grava o novo atribuído (`setBudgeted`). Depois, se mudou, chama o mesmo movimento com a diferença (`novo − antigo`), origem nula (se subiu) ou destino nulo (se desceu), e **não** atualiza o atribuído de novo (flag falsa).
- Ou seja: digitar um valor na tela cria uma movimentação de "A atribuir" para a categoria, com fonte `manual_assign`.
- **Resposta à pergunta da análise 01 (item 4):** o atribuído das duas pontas muda junto, e a movimentação é o registro. Não é derivado dela.

### Fontes — visto (75336)

- `manual_movement`, `manual_assign`.
- Automáticas de atribuição: vencimento (`auto_assign_due_date`), abaixo do previsto (`underfunded`), próximas transações (`upcoming`), meta (`goal_target`), atribuído/gasto/pago no mês anterior, média de atribuído/gasto/pago, zerar (`set_to_zero`), remover excesso (`remove_surplus`) e outras (lista parcial).
- Na exclusão de categoria: `CategoryDeletion`, com o grupo guardando a categoria removida (2382762). O dinheiro dela vai para "A atribuir" por um grupo.

### Grupos — visto (1781129, 2372493)

- Um grupo é criado quando a mesma operação mexe em mais de uma categoria (ex.: atribuição automática para várias categorias). Todas as movimentações da operação apontam para o mesmo grupo.
- A tela de "movimentações recentes" filtra por mês, por quem fez e por fonte (2522316, 2594047).

### Desfazer e refazer — visto (2730544)

- **Desfazer** (`revertMoneyMovement`): cria uma movimentação inversa **já marcada como removida** e marca a original como removida. Não altera o atribuído nesta função; quem chama é que deve ajustar (não localizei onde).
- **Refazer** (`reapplyMoneyMovement`): cria nova movimentação com os mesmos origem, destino, valor, grupo e nota, e substitui a criada antes.
- O grupo é desfeito e refeito da mesma forma (`revertMoneyMovementGroup`, `reapplyMoneyMovementGroup`).

### Limite — visto (erro `SaveAssignmentsWouldCreateATooComplexMoneyMoveError`)

- Existe um erro para salvar atribuições que gerariam movimentos complexos demais. O limite exato não foi lido.

### Implicações para o Tostão

- O atribuído é o valor de verdade; a movimentação é o histórico. Os dois são gravados na mesma operação.
- Movimentações têm que ser imutáveis; desfazer é marcar como removida e criar outra em seguida, nunca editar.
- Grupo é o que liga uma operação de várias categorias, e também a exclusão de categoria.

## 2. Datas das transações agendadas

### Campos da transação agendada — visto (1981578 e 2335464)

- `date`: a data original, usada como **âncora** do dia do mês.
- `frequency`: a frequência (enum na análise 01, §7).
- `firstUpcomingInstanceDate` e `upcomingInstanceDates`: a **próxima ocorrência** (uma data, na prática).
- Subtransações, memo, beneficiário, categoria, sinalizador e conta de transferência.

### Função de datas — visto (módulo 88953, função `_`)

Entrada: frequência, início, âncora, limite de quantidade, data final, se inclui o início, se só considera datas futuras.

Há três famílias:

| Família | Frequências | Como anda |
|---|---|---|
| Por dias | diária, semanal, a cada 2 semanas, a cada 4 semanas | soma 1, 7, 14 ou 28 dias ao início. A âncora não entra. |
| Dia fixo do mês | mensal, a cada 2, 3, 4 meses, duas vezes por ano | soma k meses e ajusta para o dia da âncora, limitado ao último dia do mês |
| Duas vezes ao mês | duas vezes por mês | alterna os dias `h` e `h+15` (com `h` = âncora, ou âncora − 15 se a âncora passa de 15) |
| Anual | anual, a cada 2 anos | soma 1 ou 2 anos ao início; sem âncora |
| Nunca | nunca | uma só ocorrência |

**Ponto importante:** na família de dia fixo, o código faz `h = min(dias do mês, h)` **dentro da caminhada**, e isso afeta as ocorrências seguintes da mesma caminhada (ver §2.4 e §5).

### Quando as datas são geradas

**Criação** — visto (2335464, `createScheduledTransaction`):
- Calcula a próxima ocorrência futura (depois do início).
- Se a data já passou ou é hoje, gera a transação da data agora e avança a próxima ocorrência.

**Edição** — visto (2336379, `updateScheduledTransaction`):
- Se a data ou a frequência mudou, recalcula a próxima ocorrência e, se a data já passou, gera a transação.
- **Quirk:** se só a frequência mudou e a próxima ocorrência é diferente da data, a data da agendada passa a ser essa próxima ocorrência (2336379: `r.setDate(i)`). Ou seja, trocar a frequência **reancora** a agendada.

**Disparo automático** — visto (módulo 88953, `fireAllPending`):
- Percorre todas as agendadas e gera todas as ocorrências até hoje.
- **Só gera as que estão a até 12 meses de distância de hoje** (`monthsApart ≤ 12`). As mais antigas são puladas sem criar transação, mas a próxima ocorrência avança.
- O método é exposto como `fireAnyPendingScheduledTransactions`. Quem o chama e quando não foi localizado neste bundle (**não verificado**).

**Enviar agora** — visto (`fireOnceWithTodaysDate`):
- Cria a transação com a data de hoje e registra a data original em `dateEnteredFromSchedule`. Avança a próxima ocorrência.

### Identidade da transação gerada — visto (`createTransactionInstance`)

- O id é `{id da agendada}_{data da ocorrência em ISO}`.
- Se já existe transação com esse id e ela não está removida, é reaproveitada: a geração é **idempotente**.
- Consequência: dois aparelhos que gerarem a mesma ocorrência produzem o mesmo id, e a sincronização não duplica. Esse é um ponto a favor para o Tostão.
- **Não verificado:** se uma transação gerada e removida pode ser recriada com o mesmo id quando a geração passa de novo por aquela data. Como o cursor só anda para frente, o risco parece baixo.

### O que a transação gerada recebe — visto

- Fonte: agendador (`Scheduler`).
- Data: a ocorrência (ou hoje, no "enviar agora").
- Estado de compensação: **compensada** se a conta é de dinheiro vivo; senão, o estado passado na geração, que é não compensada por padrão (`et`, 90369).
- Aceite: conta de empréstimo, aceita sempre; demais, aceita só quando o disparo é manual (`nt`, 90369, e `fireOnceWithTodaysDate`). **Inferido:** o disparo automático entra como "a aprovar".
- Subtransações: copiadas, sem as removidas.
- Frequência "nunca": a agendada e suas subtransações são removidas depois de gerar a única transação.
- Tenta casar com uma transação importada da mesma conta (`attemptMatchToImportedTransaction`). Os critérios de casamento (`S`, `T`, 2309677) não foram lidos.

### 2.4 Caminhada única × passo a passo — reproduzido

Há **dois caminhos** para as mesmas datas, e eles nem sempre dão o mesmo resultado:

1. **Passo a passo** (geração): cada chamada parte da última ocorrência, com a âncora original (`t.date`). Usado em `setNextUpcomingInstanceDate`, na geração e no avanço.
2. **Caminhada única** (planejamento): `getAllUpcomingInstanceDatesForMonth` caminha da primeira ocorrência até o fim do mês de interesse, em uma chamada só (2541796). Essa é a visão de meses futuros e a que entra nas metas (análise 01, §4 e §5).

No caso mensal com âncora 31 (ou 30, 29), a caminhada única **mantém o dia reduzido** depois de um mês curto, e o passo a passo não. Exemplo reproduzido (âncora 31/out/2026, transação mensal):

| Caminho | Sequência |
|---|---|
| Passo a passo (geração) | 30/nov, 31/dez, 31/jan, 28/fev, **31/mar**, 30/abr, 31/mai |
| Caminhada única (planejamento, até mar/2027) | 30/nov, 31/dez, 31/jan, 28/fev, **28/mar** |

- **Consequência:** a tela de meses futuros pode mostrar 28/mar enquanto a transação gerada cairá em 31/mar. **Inferido:** a lista de transações futuras que entra nas metas (análise 01, §4) usa a mesma caminhada única.
- Na família "duas vezes por mês", os dois caminhos deram o mesmo resultado nos casos testados (âncoras 15 e 20).
- Vale confirmar no app antes de decidir (ver §6).

Em "anual", a data também escorrega: 29/fev/2028 vira 28/fev/2029 e continua em 28/fev nos anos seguintes (`reproduzido`). Vale como decisão: reproduzir ou corrigir.

### Implicações para o Tostão

- Ter o id determinístico `{agendada}_{data}`: decisão a favor, quase de graça.
- Escolher **uma** forma de caminhar as datas e usá-la em todo lugar (geração e planejamento). Hoje o YNAB usa duas.
- Trocar a frequência reancora a data: decidir se reproduzimos.
- Limite de 12 meses para gerar atrasadas: decidir se reproduzimos.

## 3. Reconciliação

### Estados da transação — visto (24141)

- `Uncleared` (não compensada), `Cleared` (compensada), `Reconciled` (reconciliada).
- Alternar (`toggleClearedForTransaction`): não compensada ↔ compensada. Nos dois sentidos, a transação também é **aprovada** (`setTransactionClearedAndAccepted` chama `approveTransaction`).
- Reconciliada **não alterna**: o alternar não faz nada nela.
- Edição em massa (`setClearedOnTransactions`) pula as reconciliadas.
- A tela avisa antes de editar uma reconciliada (textos `editReconciledTransactionWarning`).

### Saldos da conta — visto (2633886, 2636290, 2695125, 2110990)

- **Saldo de trabalho** (`balance`) = saldo compensado + saldo não compensado. É o que o app mostra como saldo da conta.
- **Saldo compensado** (`clearedBalance`) = soma das compensadas e reconciliadas. **Inferido:** é calculado no servidor (no web, só há leitura e merge dos campos). Não encontrei cálculo local.
- Saldo compensado da lista de contas: soma dos saldos das contas da seleção.

### Diferença para o alvo — visto (2695485, `getReconcileBalanceDifference`)

- Diferença = alvo − saldo compensado.
- Quando o chamador pede (parâmetro de sinal), em conta que não é de ativo (cartão, dívida) com saldo compensado negativo, o alvo é forçado a ser negativo. Assim, digitar "500" para uma dívida conta como −500.

### Reconciliar conta comum — visto (63341, `K`)

Ordem:
1. Marca a data da última reconciliação com a hora atual.
2. Ajuste de saldo: **valor = novo saldo − saldo compensado** (`getAdjustmentAmount`, 2057092). Se o valor é zero, não cria nada.
   - Cria uma transação "Ajuste de reconciliação" (beneficiário `ReconciliationBalanceAdjustment`, memo de reconciliação), dated hoje, com estado **Reconciliada**.
   - Se a conta é orçamentária, a categoria é "Entrada imediata" (`ImmediateIncome`), ou seja, entra como receita (A atribuir). Se não, fica sem categoria.
   - Se a única transação da conta é o saldo inicial, de valor zero e de hoje, reaproveita essa transação.
3. **Todas as transações compensadas** da conta passam a reconciliadas. As não compensadas ficam como estão.

Resultado: o saldo compensado vira o alvo; as não compensadas continuam pendentes e somam no saldo de trabalho.

### Conta de empréstimo — visto (módulo 88169)

- Usa o mesmo passo 2, mas o ajuste é contra o **saldo de trabalho** (não compensado): `isLoanAccount ? balance : clearedBalance`.
- Depois converte as estimativas históricas de juros em transações, atualiza taxa de juros e parcela de escrow do mês, e marca **todas** as transações visíveis como reconciliadas (inclusive não compensadas).

### Ajuste de saldo (não é reconciliação) — visto (2056265, tipo `Other`)

- Ajuste manual do saldo: valor = novo saldo − **saldo de trabalho**.
- O estado da transação é **compensada** (não reconciliada).
- Beneficiário: `ManualBalanceAdjustment` se há outras transações; `StartingBalance` se não há.
- Zerar saldo (`zeroOutBalance`) usa esse tipo com o memo de conta fechada.
- Quando o novo saldo é zero, zera o saldo local (`zeroLocalCalcsIfNeeded`) para a tela não piscar.

### Transações visíveis — visto (1835393, `K3`)

- "Visíveis" são as de fontes do conjunto: agendador, casada, importada, importada pendente, pendente, nenhuma.
- Transações cruas (ainda não aceitas) não entram em saldo nem em reconciliação.

### Não lido

- Critérios de casamento de importadas (`S`, `T`, 2309677) e o que muda o estado ao casar.
- O helper `reconcile(t)` (2910708): reconcilia com "saldo compensado + t". Não identifiquei quem passa `t` diferente de zero.

### Implicações para o Tostão

- Reconciliar = ajuste pela diferença **contra o compensado** + promover compensadas a reconciliadas. Só dois passos.
- Ajuste de reconciliação é uma transação de verdade (fica no histórico), com categoria de entrada se for conta orçamentária.
- Reconciliada é imutável na interface (aviso antes de editar). Decidir se o servidor também recusa.

## 4. Resumo das regras

| Tema | Regra | Confiança |
|---|---|---|
| Movimentação | Atribuído muda nas duas pontas; registro é imutável | visto |
| Movimentação | Edição direta gera movimentação "A atribuir" ↔ categoria | visto |
| Movimentação | Desfazer = remover original + criar inversa já removida | visto |
| Agendada | Dia fixo: min(dias do mês, âncora), mas caminhada única reduz e não volta | reproduzido |
| Agendada | Geração passo a passo: âncora preservada | visto + reproduzido |
| Agendada | Gera só atrasadas de até 12 meses | visto |
| Agendada | Id determinístico `{agendada}_{data}` | visto |
| Agendada | Trocar frequência reancora a data | visto |
| Reconciliação | Ajuste = novo saldo − compensado; compensadas viram reconciliadas | visto |
| Reconciliação | Empréstimo: ajuste contra o de trabalho; todas viram reconciliadas | visto |
| Reconciliação | Saldo compensado calculado no servidor | inferido |

## 5. Como foram testadas as datas

O script está em `scratchpad` (fora do repo): `sched.js` é um port da função `_` com semântica moment (objeto mutável, soma de meses com clamp no último dia). `cmp.js` e `cmp2.js` comparam os dois caminhos.

Casos rodados:
- Mensal, âncora 31/jan/2026 (caminhada): 31/jan, 28/fev, 28/mar, 28/abr… (o dia reduzido persiste).
- Anual, 29/fev/2028: 29/fev, 28/fev/2029, 28/fev/2030…
- Duas vezes ao mês, dia 15: 15/jan, 30/jan, 15/fev, 28/fev, 15/mar, 30/mar…
- Duas vezes ao mês, dia 20: 20/jan, 5/fev, 20/fev, 5/mar…
- Passo a passo × caminhada, âncora 31: divergência de março (§2.4).
- Passo a passo × caminhada, duas vezes ao mês (âncoras 15 e 20): sem divergência.

Limite: o port segue o código lido. Não foi conferido no app da YNAB.

## 6. Próximos passos

1. **Conferir no app** (decisivo para o §2.4): com uma conta de teste, criar uma agendada mensal em 31 de outubro e ver as datas que aparecem na lista de meses futuros e as que são geradas nos meses seguintes. Se a tela mostra 28/mar e a transação cai em 31/mar, a divergência é real.
2. Ler os critérios de casamento de importadas (`S`, `T`) e o que muda ao casar.
3. Localizar quem chama `fireAnyPendingScheduledTransactions`.
4. Escrever os testes Rust com os casos do §5 em `tests/`, antes de implementar.

## 7. Decisões para você

- **Reproduzir ou corrigir** a divergência de datas mensais (31 → 28 na caminhada única) e o escorregão da anual (29/fev → 28/fev)? Reproduzir mantém o comportamento do YNAB; corrigir é o que o usuário espera. Eu recomendaria corrigir, com uma única forma de caminhar as datas.
- **Trocar a frequência reancora a data** (§2): reproduzir ou manter a data original?
- **Limite de 12 meses** para gerar atrasadas: reproduzir ou gerar todas?

## 8. O que ainda não achamos

- Critérios de casamento de importadas (`S`, `T`).
- Quem chama o disparo automático das agendadas.
- Limite exato de `SaveAssignmentsWouldCreateATooComplexMoneyMoveError`.
- Quem ajusta o atribuído depois de desfazer uma movimentação.
- Cálculo do saldo compensado no servidor (fora do web).
