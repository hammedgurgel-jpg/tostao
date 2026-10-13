//! Metas das categorias: período, alvo efetivo e o cálculo mensal
//! (`SubcategoryCalculations.calculateGoals` no bundle, módulo 80211).
//! Regras descritas em `docs/analise/01-estouro-cartao-metas.md`, seção 4.

use crate::calc::{round_to_currency, CategoryType, SubCategoryMonthCalc};
use chrono::Datelike;

use crate::dates::*;

/// Tipo de meta (códigos `MF`, `TB`, `TBD`, `NEED`, `DEBT` no protocolo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalType {
    /// Valor mensal: atribuir X todo mês.
    MonthlyFunding,
    /// Saldo alvo, sem prazo.
    TargetBalance,
    /// Saldo alvo até uma data.
    TargetBalanceOnDate,
    /// Necessário para gastar, por período.
    Needed,
    /// Pagamento de dívida.
    DebtPayment,
}

impl GoalType {
    pub fn code(self) -> &'static str {
        match self {
            Self::MonthlyFunding => "MF",
            Self::TargetBalance => "TB",
            Self::TargetBalanceOnDate => "TBD",
            Self::Needed => "NEED",
            Self::DebtPayment => "DEBT",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "MF" => Self::MonthlyFunding,
            "TB" => Self::TargetBalance,
            "TBD" => Self::TargetBalanceOnDate,
            "NEED" => Self::Needed,
            "DEBT" => Self::DebtPayment,
            _ => return None,
        })
    }

    fn repeats_every_month(self) -> bool {
        matches!(self, Self::MonthlyFunding | Self::DebtPayment)
    }
}

/// Cadência da meta; o valor numérico é o código do protocolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cadence {
    #[default]
    None = 0,
    Monthly = 1,
    Weekly = 2,
    Every2Months = 3,
    Every3Months = 4,
    Every4Months = 5,
    Every5Months = 6,
    Every6Months = 7,
    Every7Months = 8,
    Every8Months = 9,
    Every9Months = 10,
    Every10Months = 11,
    Every11Months = 12,
    Yearly = 13,
    Every2Years = 14,
}

impl Cadence {
    pub fn from_code(code: i64) -> Option<Self> {
        use Cadence::*;
        Some(match code {
            0 => None,
            1 => Monthly,
            2 => Weekly,
            3 => Every2Months,
            4 => Every3Months,
            5 => Every4Months,
            6 => Every5Months,
            7 => Every6Months,
            8 => Every7Months,
            9 => Every8Months,
            10 => Every9Months,
            11 => Every10Months,
            12 => Every11Months,
            13 => Yearly,
            14 => Every2Years,
            _ => return Option::None,
        })
    }

    /// `monthsInGoalCadence`: duração de um ciclo em meses.
    pub fn months(self, frequency: Option<i64>) -> i64 {
        if let Some(freq) = frequency {
            return match self {
                Cadence::Weekly => 1,
                Cadence::Yearly => 12 * freq,
                _ => freq,
            };
        }
        match self {
            Cadence::None => 0,
            Cadence::Monthly | Cadence::Weekly => 1,
            Cadence::Yearly => 12,
            Cadence::Every2Years => 24,
            other => other as i64 - 1, // Every2Months = 3 → 2 … Every11Months = 12 → 11
        }
    }
}

/// Dia padrão das metas semanais: sábado.
pub const DEFAULT_GOAL_DAY_OF_WEEK: u32 = 6;

/// Meta de uma categoria.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Goal {
    pub goal_type: Option<GoalType>,
    pub created_on: Option<Date>,
    pub target_date: Option<Date>,
    pub target_amount: i64,
    pub monthly_funding: i64,
    pub cadence: Cadence,
    pub cadence_frequency: Option<i64>,
    /// Dia do mês (metas mensais e de dívida) ou da semana (semanais, domingo = 0).
    pub day: Option<u32>,
    /// Só em "necessário para gastar": o saldo anterior não conta como financiado.
    pub needs_whole_amount: bool,
}

/// Resultado da meta numa categoria e mês.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalCalc {
    /// Atribuído acumulado no período (mais o saldo herdado no primeiro mês).
    pub overall_funded: i64,
    /// Saídas acumuladas no período.
    pub overall_outflows: i64,
    /// Quanto atribuir neste mês.
    pub target: i64,
    /// Quanto ainda falta atribuir neste mês.
    pub under_funded: i64,
    /// Quanto ainda falta no total.
    pub overall_left: i64,
    /// Meses previstos até concluir.
    pub expected_completion: Option<i64>,
    /// 0–100.
    pub percentage_complete: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalPeriod {
    pub start: Date,
    pub finish: Date,
    pub months: i64,
}

/// `calculateCurrentGoalPeriod`: o período da meta que contém `month`.
pub fn current_goal_period(goal: &Goal, month: Date) -> GoalPeriod {
    let month_start = start_of_month(month);
    let month_end = add_days(add_months(month_start, 1), -1);
    let created = goal.created_on.map_or(month_start, start_of_month);
    let cycle = goal.cadence.months(goal.cadence_frequency);

    let mut start = created;
    let mut finish = goal.target_date.unwrap_or(month_start);

    if goal.goal_type == Some(GoalType::MonthlyFunding) {
        start = month_start;
        finish = month_end;
    } else if goal.goal_type == Some(GoalType::DebtPayment)
        || (goal.cadence == Cadence::Monthly && goal.target_date.is_none())
    {
        start = month_start;
        finish = month_end;
        if let Some(day) = goal.day.filter(|d| *d < finish.day()) {
            finish = month_start.with_day(day).unwrap_or(finish);
        }
    } else if goal.cadence == Cadence::Weekly && goal.target_date.is_none() {
        let weekday = goal.day.unwrap_or(DEFAULT_GOAL_DAY_OF_WEEK);
        let week_end = next_or_same_day_of_week(month, weekday);
        let week_start = add_days(week_end, -6);
        start = week_start.max(month_start);
        finish = week_end.min(month_end);
    } else if let (true, Some(target)) = (goal.cadence != Cadence::None, goal.target_date) {
        let target_passed = month_start > target || (goal.cadence == Cadence::Weekly && month > target);
        if target_passed && goal.cadence == Cadence::Weekly {
            let every = goal.cadence_frequency.unwrap_or(1).max(1);
            let next = next_or_same_day_of_week(month, day_of_week(target));
            let offset = weeks_apart(next, target) % every;
            finish = add_days(next, 7 * offset);
            start = add_days(next, -(7 * (every - offset) - 1));
        } else if target_passed {
            let target_month = start_of_month(target);
            let elapsed = months_apart(month_start, target_month);
            start = if elapsed <= cycle {
                add_months(target_month, 1)
            } else {
                add_months(month_start, -((elapsed - 1) % cycle))
            };
            let cycle_end = add_months(start, cycle - 1);
            finish = add_months(target, months_apart(target_month, cycle_end));
        } else {
            start = created;
            finish = target;
        }
    }

    GoalPeriod { start, finish, months: months_apart(start, finish) + 1 }
}

/// `goalEffectiveTargetAmount`: o valor que a meta persegue neste mês.
pub fn effective_target_amount(
    goal: &Goal,
    category_type: CategoryType,
    credit_account_balance: Option<i64>,
    month: Date,
) -> i64 {
    match goal.goal_type {
        Some(GoalType::MonthlyFunding) => goal.monthly_funding,
        Some(GoalType::DebtPayment) => goal.target_amount,
        _ if category_type == CategoryType::CreditCardPayment => credit_account_balance.unwrap_or(0).min(0).abs(),
        Some(GoalType::Needed) if goal.cadence == Cadence::Weekly => weekly_need_amount(goal, month),
        _ => goal.target_amount,
    }
}

/// `goalEffectiveTargetAmountForWeeklyNeed`.
///
/// Com frequência maior que 1 o YNAB chega a valores fracionários de
/// milésimos; aqui o resultado é arredondado para o milésimo mais próximo.
fn weekly_need_amount(goal: &Goal, month: Date) -> i64 {
    let amount = goal.target_amount;
    let every = goal.cadence_frequency.unwrap_or(1).max(1);
    let with_extra_weeks = |weeks: i64| amount + ((amount * (weeks - 1).max(0)) as f64 / every as f64).round() as i64;

    let Some(target) = goal.target_date else {
        let weekday = goal.day.unwrap_or(DEFAULT_GOAL_DAY_OF_WEEK);
        return amount * goal_weeks_in_month(goal.created_on, month, weekday);
    };
    let weekday = day_of_week(target);
    if same_month(month, target) {
        with_extra_weeks(goal_weeks_in_month(Some(target), month, weekday))
    } else if target > month {
        amount
    } else {
        let next = next_or_same_day_of_week(month, weekday);
        let offset = weeks_apart(target, next) % every;
        let occurrence = add_days(next, 7 * offset);
        with_extra_weeks(goal_weeks_in_month(Some(occurrence), month, weekday))
    }
}

/// `calculateGoalWeeksInMonth`: ocorrências de `weekday` no mês, contando a
/// partir de `from` quando ele cai no próprio mês.
fn goal_weeks_in_month(from: Option<Date>, month: Date, weekday: u32) -> i64 {
    let baseline = from.filter(|f| same_month(month, *f)).unwrap_or(month);
    days_of_week_in_month(baseline, weekday)
}

/// `normalizeGoalPercentageComplete`: 0–100, truncado; divisão sem
/// resultado (0/0) ou infinita para cima vale 100.
pub fn normalize_percentage_complete(ratio: f64) -> i64 {
    if ratio.is_nan() || ratio == f64::INFINITY {
        return 100;
    }
    ((100.0 * ratio).floor().max(0.0)).min(100.0) as i64
}

/// `calculateGoals`: preenche `current.goal`.
///
/// `current` precisa já ter `balance` calculado (ver `calc::calculate_balance`)
/// e `month` preenchido; `previous` é o cálculo da mesma categoria no mês
/// anterior. `credit_account_balance` é o saldo da conta do cartão, só para
/// a categoria de pagamento do cartão.
pub fn calculate_goals(
    current: &mut SubCategoryMonthCalc,
    previous: Option<&SubCategoryMonthCalc>,
    goal: &Goal,
    category_type: CategoryType,
    credit_account_balance: Option<i64>,
    currency_decimal_digits: u32,
) {
    let Some(balance) = current.balance else { return };
    let month = current.month;
    let g = &mut current.goal;

    if goal.created_on.is_none_or(|created| is_before_month(month, created)) {
        *g = GoalCalc {
            expected_completion: Some(0),
            percentage_complete: Some(0),
            ..GoalCalc::default()
        };
        return;
    }

    let period = current_goal_period(goal, month);
    let months_left = if month > period.finish { 0 } else { months_apart(month, period.finish) + 1 };
    let target = effective_target_amount(goal, category_type, credit_account_balance, month);
    let budgeted = current.budgeted;
    let outflows = current.cash_outflows.unwrap_or(0) + current.credit_outflows.unwrap_or(0);
    let kind = goal.goal_type;
    let is = |t: GoalType| kind == Some(t);

    if kind.is_some_and(GoalType::repeats_every_month) {
        g.overall_funded = budgeted;
        g.overall_outflows = outflows;
    } else if same_month(month, period.start) || month > period.start {
        g.overall_funded = budgeted;
        g.overall_outflows = outflows;
        if let Some(prev) = previous {
            if same_month(prev.month, period.start) || prev.month > period.start {
                g.overall_outflows += prev.goal.overall_outflows;
            }
            if same_month(month, period.start) {
                if !(is(GoalType::Needed) && goal.needs_whole_amount) {
                    g.overall_funded += prev.balance.unwrap_or(0).max(0);
                }
            } else {
                g.overall_funded += prev.goal.overall_funded;
            }
        }
    } else {
        g.overall_funded = 0;
        g.overall_outflows = 0;
    }

    g.overall_left = if kind.is_some_and(GoalType::repeats_every_month) {
        (target - budgeted).max(0)
    } else if is(GoalType::Needed) {
        (target - g.overall_funded).max(0)
    } else {
        (target - balance).max(0)
    };

    g.expected_completion = match kind {
        None => None,
        Some(GoalType::TargetBalance) => Some(if g.overall_left > 0 && budgeted > 0 {
            div_ceil(g.overall_left, budgeted)
        } else {
            0
        }),
        Some(_) => Some(months_left),
    };

    // O que faltava antes da atribuição deste mês.
    let needed_before_this_month = if is(GoalType::Needed) {
        target - (g.overall_funded - budgeted)
    } else {
        target - balance + budgeted
    };

    g.target = if kind.is_some_and(GoalType::repeats_every_month) {
        target
    } else if (is(GoalType::TargetBalanceOnDate) || is(GoalType::Needed)) && months_left > 0 {
        let per_month = div_ceil(needed_before_this_month, months_left).max(0);
        round_to_currency(per_month, currency_decimal_digits, f64::ceil)
    } else {
        0
    };

    g.under_funded = if kind.is_some_and(GoalType::repeats_every_month) {
        g.overall_left
    } else if is(GoalType::TargetBalanceOnDate) || is(GoalType::Needed) {
        if months_left <= 0 {
            0
        } else if is(GoalType::TargetBalanceOnDate) && budgeted < 0 && needed_before_this_month < 0 {
            (target - balance).max(0)
        } else if is(GoalType::Needed) && budgeted < 0 && needed_before_this_month < 0 {
            (target - g.overall_funded).max(0)
        } else {
            (g.target - budgeted).max(0)
        }
    } else {
        0
    };

    let ratio = |a: i64, b: i64| a as f64 / b as f64;
    match kind {
        Some(GoalType::MonthlyFunding | GoalType::DebtPayment | GoalType::Needed) => {
            g.percentage_complete = Some(normalize_percentage_complete(ratio(g.overall_funded, target)));
        }
        Some(GoalType::TargetBalance | GoalType::TargetBalanceOnDate) => {
            g.percentage_complete = Some(if category_type == CategoryType::CreditCardPayment {
                normalize_percentage_complete(ratio(g.overall_funded, g.overall_left + g.overall_funded))
            } else {
                normalize_percentage_complete(ratio(balance, target))
            });
        }
        None => g.overall_funded = 0,
    }
}

/// Divisão arredondando para cima (`Math.ceil(a / b)` com `b > 0`).
fn div_ceil(a: i64, b: i64) -> i64 {
    (a as f64 / b as f64).ceil() as i64
}

/// `calculateTotalUnderfundedAmount` sem a parte do cartão: o que falta
/// atribuir, mais o que o saldo não cobre das transações agendadas do mês.
/// Meta adiada não conta como falta.
pub fn total_underfunded_amount(balance: i64, under_funded: i64, upcoming_transactions: i64, snoozed: bool) -> i64 {
    let under_funded = if snoozed { 0 } else { under_funded };
    let shortfall = balance + under_funded + upcoming_transactions;
    if shortfall < 0 {
        under_funded + shortfall.abs()
    } else {
        under_funded
    }
}
