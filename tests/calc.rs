use tostao::calc::*;

fn id(v: i64) -> i64 {
    v
}

#[test]
fn balance_carries_only_positive_previous_balance() {
    let prev = SubCategoryMonthCalc { balance: Some(-5_000), ..Default::default() };
    let mut cur = SubCategoryMonthCalc { budgeted: 20_000, cash_outflows: Some(-3_000), ..Default::default() };
    calculate_balance(&mut cur, Some(&prev), id);
    assert_eq!(cur.balance_previous_month, Some(-5_000));
    assert_eq!(cur.balance, Some(17_000));

    let prev = SubCategoryMonthCalc { balance: Some(4_000), ..Default::default() };
    calculate_balance(&mut cur, Some(&prev), id);
    assert_eq!(cur.balance, Some(21_000));
}

#[test]
fn overspent_cash_is_split_into_budgeted_and_unbudgeted() {
    // atribuído 10, gasto 15 em dinheiro: 10 coberto, 5 estourado
    let mut c = SubCategoryMonthCalc { budgeted: 10_000, cash_outflows: Some(-15_000), ..Default::default() };
    calculate_balance(&mut c, None, id);
    calculate_budgeted_cash_credit_outflows(&mut c);
    assert_eq!(c.balance, Some(-5_000));
    assert_eq!(c.unbudgeted_cash_outflows, -5_000);
    assert_eq!(c.budgeted_cash_outflows, -10_000);
}

#[test]
fn available_to_budget_rolls_from_previous_month() {
    let prev = MonthCalc { available_to_budget: 100_000, over_spent: -5_000, ..Default::default() };
    let mut cur = MonthCalc { immediate_income: 50_000, budgeted: 120_000, ..Default::default() };
    calculate_available_to_budget(&mut cur, Some(&prev), id);
    assert_eq!(cur.available_to_budget, 25_000);
}

#[test]
fn ready_to_assign_subtracts_money_assigned_to_future_months() {
    let oct = MonthCalc { available_to_budget: 30_000, ..Default::default() };
    let nov = MonthCalc { budgeted: 20_000, ..Default::default() };
    let dec = MonthCalc { budgeted: 15_000, ..Default::default() };
    let all = [
        MonthlyBudget { month: (2026, 10), calculation: Some(&oct) },
        MonthlyBudget { month: (2026, 11), calculation: Some(&nov) },
        MonthlyBudget { month: (2026, 12), calculation: Some(&dec) },
    ];
    // futuro = 35; financiado no futuro = min(30, 35) = 30; sobra 0
    assert_eq!(calculate_left_to_budget_for_monthly_budget(&all[0], &all), 0);
    // disponível negativo não desconta futuro
    assert_eq!(calculate_left_to_budget(-1_000, 35_000), -1_000);
}
