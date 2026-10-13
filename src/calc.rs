//! Cálculos do orçamento feitos no cliente. Valores em milésimos da moeda
//! (milliunits), como o YNAB guarda.
//!
//! `roundMilliDollars` depende das casas decimais da moeda; o modo padrão é
//! `Math.round` (ver [`round_to_currency`] e [`js_round`]). Quem chama passa a
//! função de arredondamento, mas o padrão do YNAB é `js_round`.

use crate::dates::Date;
use crate::goals::GoalCalc;

/// Tipo de categoria (códigos `DFT` e `DBT` no protocolo).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CategoryType {
    #[default]
    Default,
    /// Categoria de pagamento de um cartão de crédito.
    CreditCardPayment,
}

impl CategoryType {
    pub fn code(self) -> &'static str {
        match self {
            Self::Default => "DFT",
            Self::CreditCardPayment => "DBT",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "DFT" => Some(Self::Default),
            "DBT" => Some(Self::CreditCardPayment),
            _ => None,
        }
    }
}

/// `Math.round` do JS: arredonda a metade para o lado de +∞ (−2,5 vira −2).
/// É o modo padrão de `roundMilliDollars` (`e=Math.round`).
pub fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `roundMilliDollars`: arredonda milésimos para o passo da moeda com a função
/// dada. O passo é `10^(3 − min(3, casas))` milésimos: 10 para o real (2
/// casas), 1000 para moeda sem centavo, 1 para moeda com 3 casas ou mais.
/// Feito como no bundle (`mode(t * n) / n`, com `n = 1/passo`).
pub fn round_to_currency(milliunits: i64, decimal_digits: u32, mode: impl Fn(f64) -> f64) -> i64 {
    let n = 1.0 / 10_f64.powi(3 - decimal_digits.min(3) as i32);
    (mode(milliunits as f64 * n) / n).round() as i64
}

/// Cálculo de uma categoria num mês (`monthlySubCategoryBudgetCalculation`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubCategoryMonthCalc {
    /// Primeiro dia do mês do orçamento.
    pub month: Date,
    pub budgeted: i64,
    pub cash_outflows: Option<i64>,
    pub credit_outflows: Option<i64>,
    pub balance: Option<i64>,
    pub balance_previous_month: Option<i64>,
    pub budgeted_cash_outflows: i64,
    pub unbudgeted_cash_outflows: i64,
    pub budgeted_credit_outflows: i64,
    pub unbudgeted_credit_outflows: i64,
    /// Só na categoria de pagamento do cartão: gastos no cartão cobertos por
    /// outras categorias (valor ≤ 0), que passam para esta.
    pub budgeted_spending: i64,
    pub goal: GoalCalc,
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

/// `calculateCashOverSpentAmount`: estouro em dinheiro da categoria (≤ 0).
pub fn cash_overspent_amount(unbudgeted_cash_outflows: i64, available: i64) -> i64 {
    unbudgeted_cash_outflows.max(available).min(0)
}

/// `calculateCreditOverSpentAmount`: estouro no crédito da categoria (≤ 0).
pub fn credit_overspent_amount(unbudgeted_credit_outflows: i64, available: i64) -> i64 {
    unbudgeted_credit_outflows.max(available).min(0)
}

impl SubCategoryMonthCalc {
    pub fn cash_overspent(&self) -> i64 {
        cash_overspent_amount(self.unbudgeted_cash_outflows, self.balance.unwrap_or(0))
    }

    pub fn credit_overspent(&self) -> i64 {
        credit_overspent_amount(self.unbudgeted_credit_outflows, self.balance.unwrap_or(0))
    }

    pub fn is_overspent(&self) -> bool {
        self.cash_overspent() < 0 || self.credit_overspent() < 0
    }
}

/// Linha "sem categoria" do mês (`budgetMonthDisplayItemForUncategorizedTransactions`
/// no bundle): uma pseudo-categoria sem atribuído, com as saídas sem categoria.
pub fn uncategorized_row(cash_outflows: i64, credit_outflows: i64) -> SubCategoryMonthCalc {
    let mut row = SubCategoryMonthCalc {
        cash_outflows: Some(cash_outflows),
        credit_outflows: Some(credit_outflows),
        balance: Some(cash_outflows + credit_outflows),
        ..Default::default()
    };
    calculate_budgeted_cash_credit_outflows(&mut row);
    row
}

/// `overSpent` do mês (≤ 0): soma dos estouros em dinheiro das categorias e da
/// linha "sem categoria". O estouro no crédito fica de fora porque vira dívida
/// no cartão.
///
/// A soma não está no bundle (o `overSpent` chega pronto do servidor), mas a
/// regra foi conferida na tela do YNAB, inclusive a linha "sem categoria". Ver
/// `docs/analise/01-estouro-cartao-metas.md`, seção 2.
pub fn month_over_spent<'a>(
    categories: impl IntoIterator<Item = &'a SubCategoryMonthCalc>,
    uncategorized: &SubCategoryMonthCalc,
) -> i64 {
    categories.into_iter().map(SubCategoryMonthCalc::cash_overspent).sum::<i64>() + uncategorized.cash_overspent()
}

/// Gasto de uma categoria num mês, visto de um cartão de crédito.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CardSpending {
    /// Saídas no crédito da categoria no mês, em todos os cartões (≤ 0).
    pub category_credit_outflows: i64,
    /// A parte delas coberta pelo dinheiro da categoria (≤ 0).
    pub category_budgeted_credit_outflows: i64,
    /// Saídas da categoria neste cartão no mês (≤ 0).
    pub on_this_card: i64,
}

/// Quanto do gasto neste cartão estava coberto pelas categorias (≤ 0): o
/// valor de `budgeted_spending` da categoria de pagamento do cartão.
///
/// Regra nossa: quando uma categoria usa vários cartões no mês, a parte
/// coberta é dividida entre eles na proporção do gasto em cada um.
pub fn funded_spending_on_card(parts: &[CardSpending]) -> i64 {
    parts
        .iter()
        .filter(|p| p.category_credit_outflows != 0)
        .map(|p| {
            let share = p.category_budgeted_credit_outflows as i128 * p.on_this_card as i128;
            (share as f64 / p.category_credit_outflows as f64).round() as i64
        })
        .sum()
}

/// Saldo da categoria de pagamento do cartão: saldo positivo do mês
/// anterior + atribuído + gasto coberto que passou para ela − pagamentos.
/// Os pagamentos ficam em `cash_outflows` (≤ 0) e o gasto coberto em
/// `budgeted_spending` (≤ 0).
///
/// O bundle calcula o saldo de toda categoria do mesmo jeito (visto: carrega
/// só o saldo positivo, soma atribuído, saídas em dinheiro e no crédito). A
/// parte do cartão depende de o servidor pôr o gasto coberto como saídas no
/// crédito da categoria de pagamento, com sinal oposto a `budgeted_spending`.
/// Isso é inferido, não visto. Ver `docs/analise/01-estouro-cartao-metas.md`, seção 3.
pub fn calculate_credit_card_payment_balance(
    current: &mut SubCategoryMonthCalc,
    previous: Option<&SubCategoryMonthCalc>,
    round: impl Fn(i64) -> i64,
) {
    let mut carried = 0;
    if let Some(previous) = previous {
        let previous_balance = previous.balance.unwrap_or(0);
        current.balance_previous_month = Some(previous_balance);
        carried = previous_balance.max(0);
    }
    let balance = carried + current.budgeted + current.cash_outflows.unwrap_or(0) - current.budgeted_spending;
    current.balance = Some(round(balance));
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
