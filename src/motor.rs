//! Motor de cálculo: das entidades do orçamento (contas, categorias,
//! atribuições e transações) aos cálculos de categoria por mês e do mês, que
//! no YNAB chegam prontos do servidor (`be_*_calculations`).
//!
//! [`calcular`] é uma função pura sobre tipos de domínio simples: não conhece
//! o SQLite nem o protocolo, e roda igual no aparelho e no servidor. Quem lê
//! as entidades do protocolo é o adaptador [`protocolo`].
//!
//! Etapas:
//! 1. [`classificar`] cada transação, ou cada parte de uma divisão (análise
//!    03, §3, §4 e §5; análise 02, §3);
//! 2. somar por categoria e mês, por cartão e por mês;
//! 3. encadear os meses em ordem: categorias normais, depois as de pagamento
//!    do cartão, depois o disponível do mês e o "A atribuir" (análise 01).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::calc::{
    calculate_available_to_budget, calculate_balance, calculate_budgeted_cash_credit_outflows,
    calculate_credit_card_payment_balance, calculate_left_to_budget_for_monthly_budget, funded_spending_on_card, js_round,
    month_over_spent, round_to_currency, uncategorized_row, CardSpending, CategoryType, MonthCalc, MonthlyBudget,
    SubCategoryMonthCalc,
};
use crate::dates::{add_months, start_of_month, Date};
use crate::goals::{calculate_goals, Goal};

pub mod protocolo;

/// Tipo de conta, só no que importa ao cálculo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TipoConta {
    /// Corrente, poupança, dinheiro vivo e as demais.
    #[default]
    Dinheiro,
    /// Cartão de crédito ou linha de crédito: tem categoria de pagamento.
    Credito,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conta {
    pub id: String,
    pub tipo: TipoConta,
    /// Conta orçamentária; se não, é de acompanhamento e fica fora do orçamento.
    pub orcamentaria: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TipoCategoria {
    Normal,
    /// Categoria de pagamento do cartão `cartao` (id da conta).
    PagamentoCartao { cartao: String },
    /// "Pronto para atribuir": as transações nela são receita do mês.
    Receita,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Categoria {
    pub id: String,
    pub tipo: TipoCategoria,
    pub meta: Option<Goal>,
}

/// Atribuição de uma categoria num mês (`budgeted`).
#[derive(Debug, Clone, PartialEq)]
pub struct Atribuicao {
    pub categoria: String,
    /// Qualquer dia do mês.
    pub mes: Date,
    pub valor: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compensacao {
    #[default]
    NaoCompensada,
    Compensada,
    Reconciliada,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transacao {
    pub id: String,
    pub conta: String,
    pub data: Date,
    /// Em milésimos; saída é negativa.
    pub valor: i64,
    pub categoria: Option<String>,
    /// Conta da outra ponta, se é transferência.
    pub conta_transferencia: Option<String>,
    pub compensacao: Compensacao,
    /// Importada e ainda não aceita (fonte fora das "visíveis", análise 02 §3).
    pub crua: bool,
    pub removida: bool,
    /// Partes de uma divisão. Com alguma parte não removida, a categoria e a
    /// transferência da transação são ignoradas e valem as das partes.
    pub partes: Vec<Parte>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parte {
    pub valor: i64,
    pub categoria: Option<String>,
    pub conta_transferencia: Option<String>,
    pub removida: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Orcamento {
    pub contas: Vec<Conta>,
    pub categorias: Vec<Categoria>,
    pub atribuicoes: Vec<Atribuicao>,
    pub transacoes: Vec<Transacao>,
    /// Casas decimais da moeda (2 para o real).
    pub casas_decimais: u32,
    /// Calcula ao menos até este mês, mesmo sem nada lançado nele.
    pub ultimo_mes: Option<Date>,
}

impl Default for Orcamento {
    fn default() -> Self {
        Orcamento {
            contas: Vec::new(),
            categorias: Vec::new(),
            atribuicoes: Vec::new(),
            transacoes: Vec::new(),
            casas_decimais: 2,
            ultimo_mes: None,
        }
    }
}

/// Como uma transação (ou parte) entra no cálculo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classe {
    /// Saída (ou entrada) de uma categoria; `cartao` é a conta, quando é no crédito.
    Comum { categoria: String, cartao: Option<String> },
    /// Sem categoria, em conta de dinheiro (`credito` falso) ou de crédito.
    SemCategoria { credito: bool },
    /// Receita do mês ("A atribuir").
    Receita,
    /// Entre contas orçamentárias: fora da receita e das categorias.
    TransferenciaEntreOrcamentarias,
    /// Entrada no cartão vinda de conta orçamentária: sai da categoria de
    /// pagamento do cartão.
    PagamentoCartao { cartao: String },
    /// Removida, crua ou em conta de acompanhamento.
    Ignorada,
}

/// Uma transação, ou uma parte dela, já classificada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancamento {
    pub transacao: String,
    pub conta: String,
    pub data: Date,
    pub valor: i64,
    pub classe: Classe,
}

/// Classifica cada transação, ou cada parte não removida de uma divisão.
///
/// - Removida, crua, ou em conta de acompanhamento ou desconhecida: ignorada.
///   A ponta de acompanhamento de uma transferência não conta; a ponta
///   orçamentária é transação comum.
/// - Transferência entre orçamentárias: fora da receita e das categorias,
///   exceto a entrada no cartão, que é pagamento (`isCreditCardPaymentTransfer`,
///   análise 03 §4). A saída do cartão para outra conta orçamentária não é
///   pagamento (roteiro 5D).
/// - Demais, pela categoria: receita, comum (em dinheiro ou no crédito), ou
///   sem categoria. Categoria desconhecida (removida, divisão, "nenhuma")
///   conta como sem categoria.
pub fn classificar(orcamento: &Orcamento) -> Vec<Lancamento> {
    let contas: HashMap<&str, &Conta> = orcamento.contas.iter().map(|c| (c.id.as_str(), c)).collect();
    let categorias: HashMap<&str, &Categoria> = orcamento.categorias.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut lancamentos = Vec::new();

    for t in &orcamento.transacoes {
        let conta = contas.get(t.conta.as_str()).copied().filter(|c| c.orcamentaria && !t.removida && !t.crua);
        let partes: Vec<_> = t.partes.iter().filter(|p| !p.removida).collect();
        let itens: Vec<(i64, Option<&String>, Option<&String>)> = if partes.is_empty() {
            vec![(t.valor, t.categoria.as_ref(), t.conta_transferencia.as_ref())]
        } else {
            partes.iter().map(|p| (p.valor, p.categoria.as_ref(), p.conta_transferencia.as_ref())).collect()
        };
        for (valor, categoria, transferencia) in itens {
            let classe = match conta {
                None => Classe::Ignorada,
                Some(conta) => classe_do_item(conta, valor, categoria, transferencia, &contas, &categorias),
            };
            lancamentos.push(Lancamento { transacao: t.id.clone(), conta: t.conta.clone(), data: t.data, valor, classe });
        }
    }
    lancamentos
}

fn classe_do_item(
    conta: &Conta,
    valor: i64,
    categoria: Option<&String>,
    transferencia: Option<&String>,
    contas: &HashMap<&str, &Conta>,
    categorias: &HashMap<&str, &Categoria>,
) -> Classe {
    let outra_orcamentaria = transferencia
        .filter(|outra| **outra != conta.id)
        .and_then(|outra| contas.get(outra.as_str()))
        .is_some_and(|outra| outra.orcamentaria);
    if outra_orcamentaria {
        return if conta.tipo == TipoConta::Credito && valor >= 0 {
            Classe::PagamentoCartao { cartao: conta.id.clone() }
        } else {
            Classe::TransferenciaEntreOrcamentarias
        };
    }
    let credito = conta.tipo == TipoConta::Credito;
    match categoria.and_then(|c| categorias.get(c.as_str())) {
        Some(c) if c.tipo == TipoCategoria::Receita => Classe::Receita,
        Some(c) => Classe::Comum { categoria: c.id.clone(), cartao: credito.then(|| conta.id.clone()) },
        None => Classe::SemCategoria { credito },
    }
}

/// Cálculo de uma categoria num mês.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculoCategoria {
    pub categoria: String,
    pub calculo: SubCategoryMonthCalc,
}

/// Cálculo de um mês do orçamento.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculoMes {
    /// Primeiro dia do mês.
    pub mes: Date,
    pub calculo: MonthCalc,
    /// O que a tela mostra como "A atribuir": o disponível menos o atribuído
    /// nos meses seguintes.
    pub a_atribuir: i64,
    /// Na ordem de [`Orcamento::categorias`], sem a de receita.
    pub categorias: Vec<CalculoCategoria>,
    /// Linha "sem categoria".
    pub sem_categoria: SubCategoryMonthCalc,
}

impl CalculoMes {
    pub fn categoria(&self, id: &str) -> Option<&SubCategoryMonthCalc> {
        self.categorias.iter().find(|c| c.categoria == id).map(|c| &c.calculo)
    }
}

/// Saldos de uma conta, com todas as transações não removidas e não cruas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaldoConta {
    pub conta: String,
    /// Saldo de trabalho.
    pub saldo: i64,
    /// Compensadas e reconciliadas.
    pub saldo_compensado: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calculos {
    /// Do primeiro mês com transação ou atribuição até o último (ou até
    /// [`Orcamento::ultimo_mes`]), sem buracos.
    pub meses: Vec<CalculoMes>,
    pub contas: Vec<SaldoConta>,
}

impl Calculos {
    pub fn mes(&self, mes: Date) -> Option<&CalculoMes> {
        let mes = start_of_month(mes);
        self.meses.iter().find(|m| m.mes == mes)
    }

    pub fn saldo(&self, conta: &str) -> Option<&SaldoConta> {
        self.contas.iter().find(|s| s.conta == conta)
    }
}

/// Somas de uma categoria num mês.
#[derive(Default)]
struct Saidas {
    dinheiro: i64,
    credito: i64,
    /// Saídas no crédito, por cartão.
    por_cartao: BTreeMap<String, i64>,
}

/// Somas de um cartão num mês.
#[derive(Default, Clone, Copy)]
struct Cartao {
    /// Entradas que são pagamento (≥ 0).
    pagamentos: i64,
    /// Lançamentos que não são transferência entre orçamentárias nem pagamento.
    gastos: i64,
}

/// Somas do mês.
#[derive(Default)]
struct Mes {
    receita: i64,
    sem_categoria_dinheiro: i64,
    sem_categoria_credito: i64,
}

/// Calcula todos os meses do orçamento.
pub fn calcular(orcamento: &Orcamento) -> Calculos {
    let round = |v: i64| round_to_currency(v, orcamento.casas_decimais, js_round);
    let lancamentos = classificar(orcamento);

    // Etapa 2: somas por categoria, cartão e mês.
    let mut saidas: HashMap<(&str, Date), Saidas> = HashMap::new();
    let mut cartoes: HashMap<(&str, Date), Cartao> = HashMap::new();
    let mut somas: BTreeMap<Date, Mes> = BTreeMap::new();
    let mut meses: BTreeSet<Date> = BTreeSet::new();
    for l in &lancamentos {
        let mes = start_of_month(l.data);
        if l.classe != Classe::Ignorada {
            meses.insert(mes);
        }
        let m = somas.entry(mes).or_default();
        match &l.classe {
            Classe::Comum { categoria, cartao } => {
                let s = saidas.entry((categoria.as_str(), mes)).or_default();
                match cartao {
                    Some(cartao) => {
                        s.credito += l.valor;
                        *s.por_cartao.entry(cartao.clone()).or_default() += l.valor;
                    }
                    None => s.dinheiro += l.valor,
                }
            }
            Classe::SemCategoria { credito: true } => m.sem_categoria_credito += l.valor,
            Classe::SemCategoria { credito: false } => m.sem_categoria_dinheiro += l.valor,
            Classe::Receita => m.receita += l.valor,
            Classe::PagamentoCartao { cartao } => cartoes.entry((cartao.as_str(), mes)).or_default().pagamentos += l.valor,
            Classe::TransferenciaEntreOrcamentarias | Classe::Ignorada => {}
        }
        if matches!(l.classe, Classe::Comum { .. } | Classe::SemCategoria { .. } | Classe::Receita) {
            cartoes.entry((l.conta.as_str(), mes)).or_default().gastos += l.valor;
        }
    }

    let categorias: HashMap<&str, &Categoria> = orcamento.categorias.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut atribuido: HashMap<(&str, Date), i64> = HashMap::new();
    for a in &orcamento.atribuicoes {
        let orcavel = categorias.get(a.categoria.as_str()).is_some_and(|c| c.tipo != TipoCategoria::Receita);
        if orcavel {
            let mes = start_of_month(a.mes);
            meses.insert(mes);
            *atribuido.entry((a.categoria.as_str(), mes)).or_default() += a.valor;
        }
    }
    meses.extend(orcamento.ultimo_mes.map(start_of_month));

    let saldos = SaldosPorMes::new(orcamento);
    let contas = saldos_finais(orcamento);
    let (Some(&primeiro), Some(&ultimo)) = (meses.first(), meses.last()) else {
        return Calculos { meses: Vec::new(), contas };
    };

    // Etapa 3: os meses em ordem.
    let normais: Vec<&Categoria> = orcamento.categorias.iter().filter(|c| c.tipo == TipoCategoria::Normal).collect();
    let de_pagamento: Vec<(&Categoria, &str)> = orcamento
        .categorias
        .iter()
        .filter_map(|c| match &c.tipo {
            TipoCategoria::PagamentoCartao { cartao } => Some((c, cartao.as_str())),
            _ => None,
        })
        .collect();

    let mut resultado: Vec<CalculoMes> = Vec::new();
    let mut mes = primeiro;
    while mes <= ultimo {
        let anterior = resultado.last();
        let anterior_da = |id: &str| anterior.and_then(|a| a.categoria(id));
        let vazio = Saidas::default();
        let mut calculadas: Vec<CalculoCategoria> = Vec::new();

        for c in &normais {
            let s = saidas.get(&(c.id.as_str(), mes)).unwrap_or(&vazio);
            let mut calc = SubCategoryMonthCalc {
                month: mes,
                budgeted: atribuido.get(&(c.id.as_str(), mes)).copied().unwrap_or(0),
                cash_outflows: Some(s.dinheiro),
                credit_outflows: Some(s.credito),
                ..Default::default()
            };
            let previa = anterior_da(&c.id);
            calculate_balance(&mut calc, previa, round);
            calculate_budgeted_cash_credit_outflows(&mut calc);
            if let Some(meta) = &c.meta {
                calculate_goals(&mut calc, previa, meta, CategoryType::Default, None, orcamento.casas_decimais);
            }
            calculadas.push(CalculoCategoria { categoria: c.id.clone(), calculo: calc });
        }

        let mut adicional = 0;
        for (c, cartao) in &de_pagamento {
            let partes: Vec<CardSpending> = normais
                .iter()
                .zip(&calculadas)
                .filter_map(|(n, calc)| {
                    let no_cartao = saidas.get(&(n.id.as_str(), mes))?.por_cartao.get(*cartao).copied()?;
                    Some(CardSpending {
                        category_credit_outflows: calc.calculo.credit_outflows.unwrap_or(0),
                        category_budgeted_credit_outflows: calc.calculo.budgeted_credit_outflows,
                        on_this_card: no_cartao,
                    })
                })
                .collect();
            let movimento = cartoes.get(&(*cartao, mes)).copied().unwrap_or_default();
            // Lançado direto na categoria de pagamento (sem ser transferência):
            // conta como saída dela.
            let direto = saidas.get(&(c.id.as_str(), mes)).map_or(0, |s| s.dinheiro + s.credito);
            let mut calc = SubCategoryMonthCalc {
                month: mes,
                budgeted: atribuido.get(&(c.id.as_str(), mes)).copied().unwrap_or(0),
                cash_outflows: Some(direto - movimento.pagamentos),
                budgeted_spending: funded_spending_on_card(&partes),
                ..Default::default()
            };
            let previa = anterior_da(&c.id);
            calculate_credit_card_payment_balance(&mut calc, previa, round);
            calculate_budgeted_cash_credit_outflows(&mut calc);
            let saldo_anterior = saldos.no_fim_de(cartao, add_months(mes, -1));
            if let Some(meta) = &c.meta {
                let saldo_do_cartao = Some(saldos.no_fim_de(cartao, mes));
                let tipo = CategoryType::CreditCardPayment;
                calculate_goals(&mut calc, previa, meta, tipo, saldo_do_cartao, orcamento.casas_decimais);
            }
            adicional += pagamento_alem_da_divida(saldo_anterior, movimento);
            calculadas.push(CalculoCategoria { categoria: c.id.clone(), calculo: calc });
        }
        // Na ordem de `orcamento.categorias`.
        let ordem: HashMap<&str, usize> = orcamento.categorias.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
        calculadas.sort_by_key(|c| ordem[c.categoria.as_str()]);

        let soma = somas.get(&mes);
        let mut sem_categoria = uncategorized_row(
            soma.map_or(0, |s| s.sem_categoria_dinheiro),
            soma.map_or(0, |s| s.sem_categoria_credito),
        );
        sem_categoria.month = mes;

        let mut calculo = MonthCalc {
            immediate_income: soma.map_or(0, |s| s.receita),
            additional_to_be_budgeted: adicional,
            budgeted: calculadas.iter().map(|c| c.calculo.budgeted).sum(),
            over_spent: month_over_spent(calculadas.iter().map(|c| &c.calculo), &sem_categoria),
            available_to_budget: 0,
        };
        calculate_available_to_budget(&mut calculo, anterior.map(|a| &a.calculo), round);

        resultado.push(CalculoMes { mes, calculo, a_atribuir: 0, categorias: calculadas, sem_categoria });
        mes = add_months(mes, 1);
    }

    let todos: Vec<MonthlyBudget<'_, Date>> =
        resultado.iter().map(|m| MonthlyBudget { month: m.mes, calculation: Some(&m.calculo) }).collect();
    let a_atribuir: Vec<i64> = todos.iter().map(|m| calculate_left_to_budget_for_monthly_budget(m, &todos)).collect();
    for (m, valor) in resultado.iter_mut().zip(a_atribuir) {
        m.a_atribuir = valor;
    }

    Calculos { meses: resultado, contas }
}

/// Parte dos pagamentos do mês que deixa o cartão com saldo positivo além do
/// que ele já tinha: vai para o "A atribuir" do mês
/// (`additional_to_be_budgeted`), mas continua saindo da categoria de
/// pagamento.
///
/// **Inferido.** Não está no bundle (é soma do servidor). Fecha com o
/// roteiro 5 da análise 03: pagar 70 de uma dívida de 60 levou o "A atribuir"
/// de 1.000 a 1.010, e devolver os 10 do cartão para a corrente não o trouxe
/// de volta. O campo `additionalToBeBudgeted` existe só na categoria de
/// pagamento do cartão (análise 01, §3), o que sugere que o acréscimo vem
/// dela. Só um caso foi visto na tela.
fn pagamento_alem_da_divida(saldo_anterior: i64, mes: Cartao) -> i64 {
    let depois = saldo_anterior + mes.pagamentos + mes.gastos;
    (depois.max(0) - saldo_anterior.max(0)).clamp(0, mes.pagamentos.max(0))
}

/// Transações que entram no saldo da conta: nem removidas nem cruas.
fn entra_no_saldo(t: &Transacao) -> bool {
    !t.removida && !t.crua
}

/// Saldo de cada conta no fim de cada mês.
struct SaldosPorMes<'a>(HashMap<&'a str, BTreeMap<Date, i64>>);

impl<'a> SaldosPorMes<'a> {
    fn new(orcamento: &'a Orcamento) -> Self {
        let mut por_conta: HashMap<&str, BTreeMap<Date, i64>> = HashMap::new();
        for t in orcamento.transacoes.iter().filter(|t| entra_no_saldo(t)) {
            *por_conta.entry(t.conta.as_str()).or_default().entry(start_of_month(t.data)).or_default() += t.valor;
        }
        for meses in por_conta.values_mut() {
            let mut acumulado = 0;
            for valor in meses.values_mut() {
                acumulado += *valor;
                *valor = acumulado;
            }
        }
        SaldosPorMes(por_conta)
    }

    fn no_fim_de(&self, conta: &str, mes: Date) -> i64 {
        self.0.get(conta).and_then(|meses| meses.range(..=mes).next_back()).map_or(0, |(_, saldo)| *saldo)
    }
}

fn saldos_finais(orcamento: &Orcamento) -> Vec<SaldoConta> {
    orcamento
        .contas
        .iter()
        .map(|conta| {
            let transacoes = orcamento.transacoes.iter().filter(|t| t.conta == conta.id && entra_no_saldo(t));
            let (mut saldo, mut saldo_compensado) = (0, 0);
            for t in transacoes {
                saldo += t.valor;
                if t.compensacao != Compensacao::NaoCompensada {
                    saldo_compensado += t.valor;
                }
            }
            SaldoConta { conta: conta.id.clone(), saldo, saldo_compensado }
        })
        .collect()
}
