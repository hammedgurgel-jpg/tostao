//! Cálculos do orçamento feitos no cliente. Valores em milésimos da moeda
//! (milliunits), como o YNAB guarda.
//!
//! `roundMilliDollars` depende do `formattingManager` (casas decimais da
//! moeda). O modo de arredondamento não foi lido, então quem chama passa a
//! função de arredondamento.

/// Cálculo de uma categoria num mês (`monthlySubCategoryBudgetCalculation`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubCategoryMonthCalc {
    pub budgeted: i64,
    pub cash_outflows: Option<i64>,
    pub credit_outflows: Option<i64>,
    pub balance: Option<i64>,
    pub balance_previous_month: Option<i64>,
    pub budgeted_cash_outflows: i64,
    pub unbudgeted_cash_outflows: i64,
    pub budgeted_credit_outflows: i64,
    pub unbudgeted_credit_outflows: i64,
}

/// `calculateBalance`: só saldo positivo do mês anterior passa adiante.
pub fn calculate_balance(
    current: &mut SubCategoryMonthCalc,
    previous: Option<&SubCategoryMonthCalc>,
    round: impl Fn(i64) -> i64,
) {
    let mut carried = 0;
    if let Some(previous) = previous {
        let previous_balance = previous.balance.unwrap_or(0);
        current.balance_previous_month = Some(previous_balance);
        if previous_balance > 0 {
            carried = previous_balance;
        }
    }
    let balance = carried
        + current.budgeted
        + current.cash_outflows.unwrap_or(0)
        + current.credit_outflows.unwrap_or(0);
    current.balance = Some(round(balance));
}

/// `calculateBudgetedCashCreditOutflows`: separa quanto das saídas foi
/// coberto por dinheiro atribuído e quanto estourou.
pub fn calculate_budgeted_cash_credit_outflows(t: &mut SubCategoryMonthCalc) {
    let r = t.cash_outflows.unwrap_or(0);
    let s = t.credit_outflows.unwrap_or(0);
    let a = t.balance.unwrap_or(0) - r - s;
    t.budgeted_cash_outflows = r - (min3(-a, r, 0) + a.max(0));
    t.unbudgeted_cash_outflows = (a + r).min(0);
    t.budgeted_credit_outflows = s - (min3(-(a + r), s, 0) + (a + r).max(0));
    t.unbudgeted_credit_outflows = min3(-(a + r), s, 0) + (a + r).max(0);
}

fn min3(a: i64, b: i64, c: i64) -> i64 {
    a.min(b).min(c)
}

/// Cálculo do mês (`monthlyBudgetCalculation`), só os campos usados aqui.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonthCalc {
    pub immediate_income: i64,
    pub additional_to_be_budgeted: i64,
    pub budgeted: i64,
    pub over_spent: i64,
    pub available_to_budget: i64,
}

/// `availableToBudget = o + u + (immediateIncome + additionalToBeBudgeted - budgeted)`,
/// com `o`/`u` = disponível e estouro do mês anterior (0 se não houver).
pub fn calculate_available_to_budget(
    current: &mut MonthCalc,
    previous: Option<&MonthCalc>,
    round: impl Fn(i64) -> i64,
) {
    let (o, u) = previous.map_or((0, 0), |c| (c.available_to_budget, c.over_spent));
    let value = o + u + (current.immediate_income + current.additional_to_be_budgeted - current.budgeted);
    current.available_to_budget = round(value);
}

/// Um mês do orçamento: o mês (comparável, ex. `"2026-10-01"` ou `(ano, mês)`)
/// e seu cálculo, se existir.
pub struct MonthlyBudget<'a, M: PartialOrd> {
    pub month: M,
    pub calculation: Option<&'a MonthCalc>,
}

/// `calculateBudgetedInFutureMonths`: soma do atribuído nos meses depois de `month`.
pub fn calculate_budgeted_in_future_months<M: PartialOrd>(month: &M, all: &[MonthlyBudget<'_, M>]) -> i64 {
    all.iter()
        .filter(|b| b.month > *month)
        .filter_map(|b| b.calculation)
        .map(|c| c.budgeted)
        .sum()
}

/// `calculateFundedInFuture`.
pub fn calculate_funded_in_future(available: i64, budgeted_in_future: i64) -> i64 {
    if available <= 0 || budgeted_in_future <= 0 {
        0
    } else {
        available.min(budgeted_in_future)
    }
}

/// `calculateLeftToBudget`.
pub fn calculate_left_to_budget(available: i64, budgeted_in_future: i64) -> i64 {
    available - calculate_funded_in_future(available, budgeted_in_future)
}

/// `calculateLeftToBudgetForMonthlyBudget`: o "Ready to Assign" do mês.
pub fn calculate_left_to_budget_for_monthly_budget<M: PartialOrd>(
    month: &MonthlyBudget<'_, M>,
    all: &[MonthlyBudget<'_, M>],
) -> i64 {
    let available = month.calculation.map_or(0, |c| c.available_to_budget);
    let future = calculate_budgeted_in_future_months(&month.month, all);
    calculate_left_to_budget(available, future)
}
