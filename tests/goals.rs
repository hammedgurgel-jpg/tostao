use tostao::calc::*;
use tostao::dates::*;
use tostao::goals::*;

const REAL: u32 = 2;

fn d(y: i32, m: u32, day: u32) -> Date {
    Date::from_ymd_opt(y, m, day).unwrap()
}

fn id(v: i64) -> i64 {
    v
}

/// Cálculo de outubro/2026 com atribuído e saídas, saldo já calculado.
fn october(budgeted: i64, outflows: i64, previous: Option<&SubCategoryMonthCalc>) -> SubCategoryMonthCalc {
    let mut c = SubCategoryMonthCalc { month: d(2026, 10, 1), budgeted, cash_outflows: Some(outflows), ..Default::default() };
    calculate_balance(&mut c, previous, id);
    c
}

fn september(balance: i64, goal: GoalCalc) -> SubCategoryMonthCalc {
    SubCategoryMonthCalc { month: d(2026, 9, 1), balance: Some(balance), goal, ..Default::default() }
}

#[test]
fn dates_follow_moment_rules() {
    assert_eq!(months_apart(d(2026, 1, 31), d(2026, 2, 28)), 1);
    assert_eq!(months_apart(d(2026, 1, 15), d(2026, 3, 14)), 1);
    assert_eq!(months_apart(d(2026, 10, 1), d(2027, 3, 31)), 5);
    assert_eq!(add_months(d(2026, 1, 31), 1), d(2026, 2, 28));
    assert_eq!(day_of_week(d(2026, 10, 13)), 2, "terça");
    assert_eq!(next_or_same_day_of_week(d(2026, 10, 13), 6), d(2026, 10, 17));
    assert_eq!(next_or_same_day_of_week(d(2026, 10, 17), 6), d(2026, 10, 17));
    assert_eq!(days_of_week_in_month(d(2026, 10, 1), 6), 5, "sábados de outubro/2026");
    assert_eq!(days_of_week_in_month(d(2026, 10, 13), 6), 3);
}

#[test]
fn cadence_months() {
    assert_eq!(Cadence::Every3Months.months(None), 3);
    assert_eq!(Cadence::Every11Months.months(None), 11);
    assert_eq!(Cadence::Yearly.months(None), 12);
    assert_eq!(Cadence::Every2Years.months(None), 24);
    assert_eq!(Cadence::Monthly.months(Some(2)), 2);
    assert_eq!(Cadence::Yearly.months(Some(2)), 24);
    assert_eq!(Cadence::from_code(4), Some(Cadence::Every3Months));
    assert_eq!(GoalType::from_code("TBD"), Some(GoalType::TargetBalanceOnDate));
}

#[test]
fn monthly_funding_goal() {
    let goal = Goal {
        goal_type: Some(GoalType::MonthlyFunding),
        created_on: Some(d(2026, 9, 1)),
        monthly_funding: 100_000,
        ..Default::default()
    };
    let mut c = october(60_000, 0, None);
    calculate_goals(&mut c, None, &goal, CategoryType::Default, None, REAL);
    assert_eq!(
        c.goal,
        GoalCalc {
            overall_funded: 60_000,
            overall_outflows: 0,
            target: 100_000,
            under_funded: 40_000,
            overall_left: 40_000,
            expected_completion: Some(1),
            percentage_complete: Some(60),
        }
    );
}

#[test]
fn target_balance_by_date_spreads_over_remaining_months() {
    let goal = Goal {
        goal_type: Some(GoalType::TargetBalanceOnDate),
        created_on: Some(d(2026, 10, 1)),
        target_date: Some(d(2027, 3, 31)),
        target_amount: 1_200_000,
        ..Default::default()
    };
    let mut c = october(150_000, 0, None);
    calculate_goals(&mut c, None, &goal, CategoryType::Default, None, REAL);
    assert_eq!(c.goal.expected_completion, Some(6), "outubro a março");
    assert_eq!(c.goal.target, 200_000);
    assert_eq!(c.goal.under_funded, 50_000);
    assert_eq!(c.goal.overall_left, 1_050_000);
    assert_eq!(c.goal.percentage_complete, Some(12));

    // 1.000,00 em 6 meses = 166,666…: arredonda para cima no centavo
    let goal = Goal { target_amount: 1_000_000, ..goal };
    let mut c = october(0, 0, None);
    calculate_goals(&mut c, None, &goal, CategoryType::Default, None, REAL);
    assert_eq!(c.goal.target, 166_670);
}

#[test]
fn target_balance_without_date_estimates_completion() {
    let goal = Goal {
        goal_type: Some(GoalType::TargetBalance),
        created_on: Some(d(2026, 9, 1)),
        target_amount: 500_000,
        ..Default::default()
    };
    let prev = september(100_000, GoalCalc { overall_funded: 100_000, ..Default::default() });
    let mut c = october(50_000, 0, Some(&prev));
    calculate_goals(&mut c, Some(&prev), &goal, CategoryType::Default, None, REAL);
    assert_eq!(c.balance, Some(150_000));
    assert_eq!(c.goal.overall_funded, 150_000);
    assert_eq!(c.goal.overall_left, 350_000);
    assert_eq!(c.goal.expected_completion, Some(7));
    assert_eq!((c.goal.target, c.goal.under_funded), (0, 0), "saldo alvo sem data não pede valor por mês");
    assert_eq!(c.goal.percentage_complete, Some(30));
}

#[test]
fn month_before_goal_creation_is_zeroed() {
    let goal = Goal {
        goal_type: Some(GoalType::MonthlyFunding),
        created_on: Some(d(2026, 11, 5)),
        monthly_funding: 100_000,
        ..Default::default()
    };
    let mut c = october(60_000, 0, None);
    calculate_goals(&mut c, None, &goal, CategoryType::Default, None, REAL);
    assert_eq!(
        c.goal,
        GoalCalc { expected_completion: Some(0), percentage_complete: Some(0), ..Default::default() }
    );
}

#[test]
fn weekly_need_counts_weekdays_in_month() {
    let goal = Goal {
        goal_type: Some(GoalType::Needed),
        created_on: Some(d(2026, 9, 1)),
        target_amount: 50_000,
        cadence: Cadence::Weekly,
        cadence_frequency: Some(1),
        day: Some(6),
        ..Default::default()
    };
    assert_eq!(effective_target_amount(&goal, CategoryType::Default, None, d(2026, 10, 1)), 250_000);

    let prev = september(20_000, GoalCalc::default());
    let mut c = october(100_000, 0, Some(&prev));
    calculate_goals(&mut c, Some(&prev), &goal, CategoryType::Default, None, REAL);
    assert_eq!(c.goal.overall_funded, 120_000, "atribuído + saldo herdado");
    assert_eq!(c.goal.overall_left, 130_000);
    assert_eq!(c.goal.target, 230_000);
    assert_eq!(c.goal.under_funded, 130_000);
    assert_eq!(c.goal.percentage_complete, Some(48));

    let whole = Goal { needs_whole_amount: true, ..goal };
    let mut c = october(100_000, 0, Some(&prev));
    calculate_goals(&mut c, Some(&prev), &whole, CategoryType::Default, None, REAL);
    assert_eq!(c.goal.overall_funded, 100_000, "o saldo herdado não conta");
    assert_eq!(c.goal.under_funded, 150_000);
    assert_eq!(c.goal.percentage_complete, Some(40));
}

#[test]
fn goal_periods() {
    let month = d(2026, 10, 1);
    let weekly = Goal { goal_type: Some(GoalType::Needed), cadence: Cadence::Weekly, day: Some(6), ..Default::default() };
    let p = current_goal_period(&weekly, month);
    assert_eq!((p.start, p.finish), (d(2026, 10, 1), d(2026, 10, 3)), "semana recortada ao mês");

    let debt = Goal { goal_type: Some(GoalType::DebtPayment), day: Some(15), ..Default::default() };
    let p = current_goal_period(&debt, month);
    assert_eq!((p.start, p.finish, p.months), (d(2026, 10, 1), d(2026, 10, 15), 1));

    // a cada 3 meses, primeira data alvo em 30/06: ciclos jul–set e out–dez
    let quarterly = Goal {
        goal_type: Some(GoalType::Needed),
        created_on: Some(d(2026, 4, 1)),
        target_date: Some(d(2026, 6, 30)),
        cadence: Cadence::Every3Months,
        ..Default::default()
    };
    let p = current_goal_period(&quarterly, month);
    assert_eq!((p.start, p.finish, p.months), (d(2026, 10, 1), d(2026, 12, 30), 3));
    let p = current_goal_period(&quarterly, d(2026, 8, 1));
    assert_eq!((p.start, p.finish), (d(2026, 7, 1), d(2026, 9, 30)));
}

#[test]
fn credit_card_payment_target_is_the_card_debt() {
    let goal = Goal { goal_type: Some(GoalType::TargetBalance), target_amount: 1, ..Default::default() };
    let t = effective_target_amount(&goal, CategoryType::CreditCardPayment, Some(-300_000), d(2026, 10, 1));
    assert_eq!(t, 300_000);
    let t = effective_target_amount(&goal, CategoryType::CreditCardPayment, Some(50_000), d(2026, 10, 1));
    assert_eq!(t, 0, "cartão com saldo positivo não tem dívida");
}

#[test]
fn percentage_handles_division_edge_cases() {
    assert_eq!(normalize_percentage_complete(0.0 / 0.0), 100);
    assert_eq!(normalize_percentage_complete(1.0 / 0.0), 100);
    assert_eq!(normalize_percentage_complete(-1.0 / 0.0), 0);
    assert_eq!(normalize_percentage_complete(1.7), 100);
    assert_eq!(normalize_percentage_complete(0.129), 12);
}

#[test]
fn total_underfunded_includes_upcoming_shortfall() {
    assert_eq!(total_underfunded_amount(-10_000, 30_000, -50_000, false), 60_000);
    assert_eq!(total_underfunded_amount(100_000, 30_000, -50_000, false), 30_000);
    assert_eq!(total_underfunded_amount(-10_000, 30_000, -50_000, true), 60_000, "adiada zera a meta, não o agendado");
}
