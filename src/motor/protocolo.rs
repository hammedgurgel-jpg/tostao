//! Lê as entidades do protocolo (`be_accounts`, `be_subcategories`,
//! `be_monthly_budgets`, `be_monthly_subcategory_budgets`, `be_transactions`,
//! `be_subtransactions`) e monta o [`Orcamento`] do motor.
//!
//! Nomes dos campos como no mapeamento do servidor no bundle
//! (`serverObjectToEntity` de cada entidade).
//!
//! No caminho de volta, [`entidades_dos_calculos`] escreve os cálculos como
//! entidades `be_*_calculations` (formato não conferido).

use std::collections::HashMap;

use serde_json::Value;

use super::{Atribuicao, Categoria, Compensacao, Conta, Orcamento, Parte, TipoCategoria, TipoConta, Transacao};
use crate::dates::Date;
use crate::goals::{Cadence, Goal, GoalType};
use crate::protocol::IncomingChangedEntities;

/// Entidade que não dá para ler: tipo, id e o campo com problema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntidadeInvalida {
    pub tipo: &'static str,
    pub id: String,
    pub campo: &'static str,
}

/// Categoria de receita ("Inflow: Ready to Assign").
pub const CATEGORIA_RECEITA: &str = "Category/__ImmediateIncome__";

/// Fontes de transação que entram no saldo e no cálculo (`K3`, análise 02
/// §3). Sem fonte também entra. As outras (`raw_import`, `raw_pending`,
/// `matched_import`, `matched_pending`…) são cruas.
pub const FONTES_VISIVEIS: &[&str] = &["Scheduler", "Matched", "Imported", "ImportedPending", "Pending"];

/// Monta o orçamento a partir das entidades. Entidades removidas ficam de
/// fora, exceto transações e subtransações, que entram marcadas.
pub fn orcamento_das_entidades(e: &IncomingChangedEntities) -> Result<Orcamento, EntidadeInvalida> {
    let lista = |l: &Option<Vec<Value>>| l.clone().unwrap_or_default();
    let mut orcamento = Orcamento::default();

    for v in lista(&e.be_accounts).iter().filter(|v| !removida(v)) {
        let tipo = match texto(v, "account_type") {
            Some("CreditCard" | "LineOfCredit") => TipoConta::Credito,
            _ => TipoConta::Dinheiro,
        };
        let orcamentaria = v.get("on_budget").and_then(Value::as_bool).unwrap_or(true);
        orcamento.contas.push(Conta { id: id("be_accounts", v)?, tipo, orcamentaria });
    }

    for v in lista(&e.be_subcategories).iter().filter(|v| !removida(v)) {
        let id = id("be_subcategories", v)?;
        let tipo = match (texto(v, "internal_name"), texto(v, "entities_account_id")) {
            (Some(CATEGORIA_RECEITA), _) => TipoCategoria::Receita,
            // Divisão, "nenhuma" e outras internas não são categorias de verdade.
            (Some(nome), _) if nome.starts_with("Category/__") => continue,
            // A categoria de pagamento do cartão (`DBT`) aponta para a conta.
            (_, Some(cartao)) => TipoCategoria::PagamentoCartao { cartao: cartao.into() },
            _ => TipoCategoria::Normal,
        };
        let meta = meta("be_subcategories", &id, v)?;
        orcamento.categorias.push(Categoria { id, tipo, meta });
    }

    let mut meses: HashMap<String, Date> = HashMap::new();
    for v in lista(&e.be_monthly_budgets).iter() {
        let id = id("be_monthly_budgets", v)?;
        let mes = data("be_monthly_budgets", &id, v, "month")?;
        meses.insert(id, mes);
    }
    for v in lista(&e.be_monthly_subcategory_budgets).iter().filter(|v| !removida(v)) {
        let id = id("be_monthly_subcategory_budgets", v)?;
        let invalida = |campo| EntidadeInvalida { tipo: "be_monthly_subcategory_budgets", id: id.clone(), campo };
        let mes = match v.get("month") {
            Some(_) => data("be_monthly_subcategory_budgets", &id, v, "month")?,
            None => texto(v, "entities_monthly_budget_id")
                .and_then(|m| meses.get(m))
                .copied()
                .ok_or_else(|| invalida("entities_monthly_budget_id"))?,
        };
        let categoria = texto(v, "entities_subcategory_id").ok_or_else(|| invalida("entities_subcategory_id"))?;
        let valor = v.get("budgeted").and_then(Value::as_i64).unwrap_or(0);
        orcamento.atribuicoes.push(Atribuicao { categoria: categoria.into(), mes, valor });
    }

    let mut posicao: HashMap<String, usize> = HashMap::new();
    for v in lista(&e.be_transactions).iter() {
        let id = id("be_transactions", v)?;
        let invalida = |campo| EntidadeInvalida { tipo: "be_transactions", id: id.clone(), campo };
        let conta = texto(v, "entities_account_id").ok_or_else(|| invalida("entities_account_id"))?;
        let compensacao = match texto(v, "cleared") {
            Some("Cleared") => Compensacao::Compensada,
            Some("Reconciled") => Compensacao::Reconciliada,
            _ => Compensacao::NaoCompensada,
        };
        let crua = texto(v, "source").is_some_and(|f| !FONTES_VISIVEIS.contains(&f));
        let transacao = Transacao {
            id: id.clone(),
            conta: conta.into(),
            data: data("be_transactions", &id, v, "date")?,
            valor: v.get("amount").and_then(Value::as_i64).ok_or_else(|| invalida("amount"))?,
            categoria: texto(v, "entities_subcategory_id").map(Into::into),
            conta_transferencia: texto(v, "transfer_account_id").map(Into::into),
            compensacao,
            crua,
            removida: removida(v),
            partes: Vec::new(),
        };
        posicao.insert(id, orcamento.transacoes.len());
        orcamento.transacoes.push(transacao);
    }
    for v in lista(&e.be_subtransactions).iter() {
        let id = id("be_subtransactions", v)?;
        let invalida = |campo| EntidadeInvalida { tipo: "be_subtransactions", id: id.clone(), campo };
        let i = texto(v, "entities_transaction_id")
            .and_then(|t| posicao.get(t))
            .copied()
            .ok_or_else(|| invalida("entities_transaction_id"))?;
        orcamento.transacoes[i].partes.push(Parte {
            valor: v.get("amount").and_then(Value::as_i64).ok_or_else(|| invalida("amount"))?,
            categoria: texto(v, "entities_subcategory_id").map(Into::into),
            conta_transferencia: texto(v, "transfer_account_id").map(Into::into),
            removida: removida(v),
        });
    }

    Ok(orcamento)
}

/// Tipo das entidades com os cálculos do mês.
pub const CALCULOS_DO_MES: &str = "be_monthly_budget_calculations";
/// Tipo das entidades com os cálculos de categoria por mês.
pub const CALCULOS_DE_CATEGORIA: &str = "be_monthly_subcategory_budget_calculations";
/// Tipo das entidades com os saldos das contas.
pub const CALCULOS_DE_CONTA: &str = "be_account_calculations";

/// Os cálculos como entidades `be_*_calculations`, para o servidor devolver
/// aos aparelhos: `(tipo, entidade)`, cada entidade com `id` e
/// `is_tombstone: false`.
///
/// **Não conferido.** Os nomes dos tipos vêm da captura de rede (ver
/// [`IncomingChangedEntities`]), mas os campos e os ids não foram vistos:
/// os campos são os nomes de `SubCategoryMonthCalc`, `MonthCalc`,
/// `GoalCalc` e `SaldoConta` em snake_case, como o servidor manda
/// `over_spent`; os ids são `mbc/2026-10`, `mcbc/2026-10/<categoria>` e
/// `ac/<conta>`, escolhidos aqui. O "A atribuir" não vai: o aparelho o
/// calcula com [`crate::calc::calculate_left_to_budget_for_monthly_budget`],
/// como o cliente do YNAB.
pub fn entidades_dos_calculos(calculos: &super::Calculos) -> Vec<(&'static str, Value)> {
    use serde_json::json;
    let mes = |d: Date| d.format("%Y-%m").to_string();
    let mut out = Vec::new();
    for m in &calculos.meses {
        let c = &m.calculo;
        let s = &m.sem_categoria;
        out.push((
            CALCULOS_DO_MES,
            json!({
                "id": format!("mbc/{}", mes(m.mes)),
                "month": m.mes.to_string(),
                "immediate_income": c.immediate_income,
                "additional_to_be_budgeted": c.additional_to_be_budgeted,
                "budgeted": c.budgeted,
                "over_spent": c.over_spent,
                "available_to_budget": c.available_to_budget,
                "uncategorized_cash_outflows": s.cash_outflows.unwrap_or(0),
                "uncategorized_credit_outflows": s.credit_outflows.unwrap_or(0),
                "uncategorized_balance": s.balance.unwrap_or(0),
                "is_tombstone": false,
            }),
        ));
        for cat in &m.categorias {
            let c = &cat.calculo;
            out.push((
                CALCULOS_DE_CATEGORIA,
                json!({
                    "id": format!("mcbc/{}/{}", mes(m.mes), cat.categoria),
                    "month": m.mes.to_string(),
                    "entities_subcategory_id": cat.categoria,
                    "budgeted": c.budgeted,
                    "cash_outflows": c.cash_outflows.unwrap_or(0),
                    "credit_outflows": c.credit_outflows.unwrap_or(0),
                    "balance": c.balance.unwrap_or(0),
                    "balance_previous_month": c.balance_previous_month,
                    "budgeted_cash_outflows": c.budgeted_cash_outflows,
                    "unbudgeted_cash_outflows": c.unbudgeted_cash_outflows,
                    "budgeted_credit_outflows": c.budgeted_credit_outflows,
                    "unbudgeted_credit_outflows": c.unbudgeted_credit_outflows,
                    "budgeted_spending": c.budgeted_spending,
                    "goal_target": c.goal.target,
                    "goal_under_funded": c.goal.under_funded,
                    "goal_overall_funded": c.goal.overall_funded,
                    "goal_overall_left": c.goal.overall_left,
                    "goal_percentage_complete": c.goal.percentage_complete,
                    "goal_expected_completion": c.goal.expected_completion,
                    "is_tombstone": false,
                }),
            ));
        }
    }
    for s in &calculos.contas {
        out.push((
            CALCULOS_DE_CONTA,
            json!({
                "id": format!("ac/{}", s.conta),
                "entities_account_id": s.conta,
                "balance": s.saldo,
                "cleared_balance": s.saldo_compensado,
                "uncleared_balance": s.saldo - s.saldo_compensado,
                "is_tombstone": false,
            }),
        ));
    }
    out
}

fn removida(v: &Value) -> bool {
    v.get("is_tombstone").and_then(Value::as_bool).unwrap_or(false)
}

fn texto<'a>(v: &'a Value, campo: &str) -> Option<&'a str> {
    v.get(campo).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn id(tipo: &'static str, v: &Value) -> Result<String, EntidadeInvalida> {
    texto(v, "id").map(Into::into).ok_or(EntidadeInvalida { tipo, id: String::new(), campo: "id" })
}

/// Data ISO (`2026-10-13`, com ou sem hora depois).
fn data(tipo: &'static str, id: &str, v: &Value, campo: &'static str) -> Result<Date, EntidadeInvalida> {
    data_opcional(tipo, id, v, campo)?.ok_or_else(|| EntidadeInvalida { tipo, id: id.into(), campo })
}

fn data_opcional(tipo: &'static str, id: &str, v: &Value, campo: &'static str) -> Result<Option<Date>, EntidadeInvalida> {
    let Some(s) = texto(v, campo) else { return Ok(None) };
    s.get(..10)
        .and_then(|d| Date::parse_from_str(d, "%Y-%m-%d").ok())
        .map(Some)
        .ok_or_else(|| EntidadeInvalida { tipo, id: id.into(), campo })
}

fn meta(tipo: &'static str, id: &str, v: &Value) -> Result<Option<Goal>, EntidadeInvalida> {
    let Some(codigo) = texto(v, "goal_type") else { return Ok(None) };
    let invalida = |campo| EntidadeInvalida { tipo, id: id.into(), campo };
    let goal_type = Some(GoalType::from_code(codigo).ok_or_else(|| invalida("goal_type"))?);
    let cadence = match v.get("goal_cadence").and_then(Value::as_i64) {
        Some(c) => Cadence::from_code(c).ok_or_else(|| invalida("goal_cadence"))?,
        None => Cadence::None,
    };
    Ok(Some(Goal {
        goal_type,
        created_on: data_opcional(tipo, id, v, "goal_created_on")?,
        target_date: data_opcional(tipo, id, v, "goal_target_date")?,
        target_amount: v.get("goal_target_amount").and_then(Value::as_i64).unwrap_or(0),
        monthly_funding: v.get("monthly_funding").and_then(Value::as_i64).unwrap_or(0),
        cadence,
        cadence_frequency: v.get("goal_cadence_frequency").and_then(Value::as_i64),
        day: v.get("goal_day").and_then(Value::as_u64).map(|d| d as u32),
        needs_whole_amount: v.get("goal_needs_whole_amount").and_then(Value::as_bool).unwrap_or(false),
    }))
}
