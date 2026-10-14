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
/// Como uma categoria, carrega o saldo positivo do mês anterior (`previous`).
///
/// O carregamento não está no bundle; foi conferido na tela (análise 03,
/// roteiro 9B): +100 sem categoria em setembro apareceram em outubro como
/// "Left Over from Last Month", e uma saída sem categoria de 10 em outubro
/// deixou a linha em 90, sem estouro.
pub fn uncategorized_row(
    cash_outflows: i64,
    credit_outflows: i64,
    previous: Option<&SubCategoryMonthCalc>,
) -> SubCategoryMonthCalc {
    let mut row = SubCategoryMonthCalc {
        cash_outflows: Some(cash_outflows),
        credit_outflows: Some(credit_outflows),
        ..Default::default()
    };
    calculate_balance(&mut row, previous, |v| v);
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

/// Divide o gasto coberto de uma categoria no crédito
/// (`budgeted_credit_outflows`, ≤ 0) entre as suas saídas no crédito do mês
/// (`outflows`, uma por transação, na ordem de lançamento). Devolve a parte
/// coberta de cada uma, na mesma ordem; quem chama soma por cartão.
///
/// Não está no bundle (é soma do servidor). **Conferido na tela** (análise
/// 03, roteiros 9E, 10 e 11): a **maior transação** é coberta primeiro, e o
/// estouro fica nas menores. Ex.: 60 cobertos de 10 + 90 ficam 0 e 60; de
/// 40 + 40 num cartão e 70 no outro, ficam 0, 0 e 60 (11B: a ordem é por
/// transação, não pela soma do cartão). No empate, a lançada antes é
/// coberta primeiro (11C).
///
/// Os estornos (entradas no crédito, > 0) não se compensam com as saídas:
/// cada um sai inteiro do pagamento do próprio cartão, e o gasto coberto
/// das saídas é `budgeted_credit_outflows` menos os estornos. Conferido no
/// roteiro 12: Farmácia com estornos de 10 e 50 em dois cartões e saída de
/// 30 num terceiro deu −10, −50 e +30 nos pagamentos, não o líquido de 30
/// tirado do maior estorno. Com a categoria estourada e estornos no mesmo
/// mês, segue da mesma regra, mas não foi visto.
pub fn funded_by_card(budgeted_credit_outflows: i64, outflows: &[i64]) -> Vec<i64> {
    let refunds: i64 = outflows.iter().filter(|&&v| v > 0).sum();
    let mut funded: Vec<i64> = outflows.iter().map(|&v| v.max(0)).collect();
    let mut order: Vec<usize> = (0..outflows.len()).filter(|&i| outflows[i] < 0).collect();
    order.sort_by_key(|&i| outflows[i]);
    let mut rest = refunds - budgeted_credit_outflows;
    for i in order {
        let part = rest.min(-outflows[i]).max(0);
        funded[i] = -part;
        rest -= part;
    }
    funded
}

/// Saldo da categoria de pagamento do cartão: saldo positivo do mês
/// anterior + atribuído + gasto coberto que passou para ela − pagamentos.
/// Os pagamentos ficam em `cash_outflows` (≤ 0) e o gasto coberto em
/// `budgeted_spending` (≤ 0).
///
/// O bundle calcula o saldo de toda categoria do mesmo jeito (visto: carrega
/// só o saldo positivo, soma atribuído, saídas em dinheiro e no crédito). A
/// parte do cartão (gasto coberto entra, pagamento sai) não está no bundle,
/// mas foi conferida na tela do YNAB. Ver `docs/analise/01-estouro-cartao-metas.md`, seção 3.
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
