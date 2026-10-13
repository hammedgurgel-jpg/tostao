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

#[test]
fn rounds_to_currency_digits() {
    assert_eq!(round_to_currency(166_667, 2, f64::ceil), 166_670);
    assert_eq!(round_to_currency(166_664, 2, f64::round), 166_660);
    assert_eq!(round_to_currency(1_499, 0, f64::round), 1_000);
    assert_eq!(round_to_currency(5, 3, f64::ceil), 5);
}

#[test]
fn round_mode_of_the_ynab_is_math_round_half_toward_positive_infinity() {
    // roundMilliDollars com o padrão Math.round; real = 2 casas = passo de 10 milésimos
    assert_eq!(round_to_currency(15, 2, js_round), 20);
    assert_eq!(round_to_currency(-15, 2, js_round), -10, "-1,5 centavo-passo vai para -1");
    assert_eq!(round_to_currency(-25, 2, js_round), -20);
    assert_eq!(round_to_currency(-16, 2, js_round), -20);
    // f64::round arredonda a metade para longe de zero: não é o do YNAB
    assert_eq!(round_to_currency(-15, 2, f64::round), -20);
}

#[test]
fn uncategorized_cash_outflows_count_as_cash_overspending() {
    // mercado: atribuído 10, gasto 15 em dinheiro (estouro 5)
    let mut groceries = SubCategoryMonthCalc { budgeted: 10_000, cash_outflows: Some(-15_000), ..Default::default() };
    calculate_balance(&mut groceries, None, id);
    calculate_budgeted_cash_credit_outflows(&mut groceries);
    // lazer: atribuído 3, gasto 8 no cartão (estouro só no crédito)
    let mut fun = SubCategoryMonthCalc { budgeted: 3_000, credit_outflows: Some(-8_000), ..Default::default() };
    calculate_balance(&mut fun, None, id);
    calculate_budgeted_cash_credit_outflows(&mut fun);

    // 4 em dinheiro sem categoria entram; 9 no crédito sem categoria ficam de fora
    assert_eq!(month_over_spent([&groceries, &fun], &uncategorized_row(-4_000, 0)), -9_000);
    assert_eq!(month_over_spent([&groceries, &fun], &uncategorized_row(0, -9_000)), -5_000);
    // entrada sem categoria (estorno) não estoura
    assert_eq!(month_over_spent([&groceries, &fun], &uncategorized_row(3_000, 0)), -5_000);
}

#[test]
fn month_overspending_matches_ynab_screen_roteiro_1() {
    // Roteiro 1 conferido no YNAB em 2026-10-13 (orçamento TESTE, out → nov/2026).
    // Mercado: atribuído 50, gasto 80 na corrente.
    let mut groceries = SubCategoryMonthCalc { budgeted: 50_000, cash_outflows: Some(-80_000), ..Default::default() };
    calculate_balance(&mut groceries, None, id);
    calculate_budgeted_cash_credit_outflows(&mut groceries);
    // Lazer: nada atribuído, gasto 60 no cartão.
    let mut fun = SubCategoryMonthCalc { credit_outflows: Some(-60_000), ..Default::default() };
    calculate_balance(&mut fun, None, id);
    calculate_budgeted_cash_credit_outflows(&mut fun);
    // Sem categoria: 40 na corrente e 25 no cartão (a tela mostra uma linha só, −65).
    let uncategorized = uncategorized_row(-40_000, -25_000);
    assert_eq!(uncategorized.balance, Some(-65_000));

    let over_spent = month_over_spent([&groceries, &fun], &uncategorized);
    assert_eq!(over_spent, -70_000, "só os 30 do Mercado e os 40 sem categoria em dinheiro");

    let mut oct = MonthCalc { immediate_income: 1_000_000, budgeted: 50_000, ..Default::default() };
    calculate_available_to_budget(&mut oct, None, id);
    oct.over_spent = over_spent;
    assert_eq!(oct.available_to_budget, 950_000, "A atribuir de outubro na tela");

    let mut nov = MonthCalc::default();
    calculate_available_to_budget(&mut nov, Some(&oct), id);
    assert_eq!(nov.available_to_budget, 880_000, "A atribuir de novembro na tela");
}

#[test]
fn overspending_is_split_between_cash_and_credit() {
    // mercado: atribuído 10, gasto 15 em dinheiro
    let mut groceries = SubCategoryMonthCalc { budgeted: 10_000, cash_outflows: Some(-15_000), ..Default::default() };
    calculate_balance(&mut groceries, None, id);
    calculate_budgeted_cash_credit_outflows(&mut groceries);
    // lazer: atribuído 3, gasto 8 no cartão
    let mut fun = SubCategoryMonthCalc { budgeted: 3_000, credit_outflows: Some(-8_000), ..Default::default() };
    calculate_balance(&mut fun, None, id);
    calculate_budgeted_cash_credit_outflows(&mut fun);

    assert_eq!((groceries.cash_overspent(), groceries.credit_overspent()), (-5_000, 0));
    assert_eq!((fun.cash_overspent(), fun.credit_overspent()), (0, -5_000));
    assert!(groceries.is_overspent() && fun.is_overspent());
    assert_eq!(fun.budgeted_credit_outflows, -3_000);

    // só o estouro em dinheiro sai do disponível do mês seguinte
    assert_eq!(month_over_spent([&groceries, &fun], &uncategorized_row(0, 0)), -5_000);
    assert_eq!(cash_overspent_amount(-5_000, 2_000), 0, "categoria com saldo positivo não estoura");
}

#[test]
fn credit_card_payment_collects_funded_spending_and_pays_out() {
    let parts = [
        // categoria usou dois cartões: 10 no total, 6 cobertos, 4 neste cartão
        CardSpending { category_credit_outflows: -10_000, category_budgeted_credit_outflows: -6_000, on_this_card: -4_000 },
        // categoria toda neste cartão e toda coberta
        CardSpending { category_credit_outflows: -3_000, category_budgeted_credit_outflows: -3_000, on_this_card: -3_000 },
        CardSpending::default(),
    ];
    let funded = funded_spending_on_card(&parts);
    assert_eq!(funded, -5_400);

    let prev = SubCategoryMonthCalc { balance: Some(1_000), ..Default::default() };
    let mut payment = SubCategoryMonthCalc {
        budgeted: 2_000,
        cash_outflows: Some(-4_000), // pagamento da fatura
        budgeted_spending: funded,
        ..Default::default()
    };
    calculate_credit_card_payment_balance(&mut payment, Some(&prev), id);
    assert_eq!(payment.balance, Some(4_400));
    assert_eq!(CategoryType::from_code("DBT"), Some(CategoryType::CreditCardPayment));
}
