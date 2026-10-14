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
    calculate_credit_card_payment_balance, calculate_left_to_budget_for_monthly_budget, funded_by_card, js_round,
    month_over_spent, round_to_currency, uncategorized_row, CategoryType, MonthCalc, MonthlyBudget,
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
    /// Saídas no crédito, uma por transação (ou parte), na ordem de
    /// lançamento: (cartão, valor).
    no_credito: Vec<(String, i64)>,
}

/// Somas de um cartão num mês.
#[derive(Default, Clone, Copy)]
struct Cartao {
    /// Entradas que são pagamento (≥ 0).
    pagamentos: i64,
    /// Lançamentos que não são transferência entre orçamentárias nem pagamento.
    gastos: i64,
    /// Quanto dos gastos foi pago com o saldo positivo que o cartão trouxe do
    /// mês anterior (≥ 0). Ver [`credito_consumido`].
    consumido: i64,
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
    let saldos = SaldosPorMes::new(orcamento);
    let consumido = credito_consumido(&lancamentos, &saldos);

    // Etapa 2: somas por categoria, cartão e mês.
    let mut saidas: HashMap<(&str, Date), Saidas> = HashMap::new();
    let mut cartoes: HashMap<(&str, Date), Cartao> = HashMap::new();
    let mut somas: BTreeMap<Date, Mes> = BTreeMap::new();
    let mut meses: BTreeSet<Date> = BTreeSet::new();
    for (i, l) in lancamentos.iter().enumerate() {
        let mes = start_of_month(l.data);
        if l.classe != Classe::Ignorada {
            meses.insert(mes);
        }
        // A parte paga com o crédito trazido do mês anterior conta como dinheiro.
        let em_dinheiro = -consumido.get(&i).copied().unwrap_or(0);
        let m = somas.entry(mes).or_default();
        match &l.classe {
            Classe::Comum { categoria, cartao } => {
                let s = saidas.entry((categoria.as_str(), mes)).or_default();
                match cartao {
                    Some(cartao) => {
                        s.credito += l.valor - em_dinheiro;
                        s.dinheiro += em_dinheiro;
                        s.no_credito.push((cartao.clone(), l.valor - em_dinheiro));
                    }
                    None => s.dinheiro += l.valor,
                }
            }
            Classe::SemCategoria { credito: true } => {
                m.sem_categoria_credito += l.valor - em_dinheiro;
                m.sem_categoria_dinheiro += em_dinheiro;
            }
            Classe::SemCategoria { credito: false } => m.sem_categoria_dinheiro += l.valor,
            Classe::Receita => m.receita += l.valor,
            Classe::PagamentoCartao { cartao } => cartoes.entry((cartao.as_str(), mes)).or_default().pagamentos += l.valor,
            Classe::TransferenciaEntreOrcamentarias | Classe::Ignorada => {}
        }
        if matches!(l.classe, Classe::Comum { .. } | Classe::SemCategoria { .. } | Classe::Receita) {
            let c = cartoes.entry((l.conta.as_str(), mes)).or_default();
            c.gastos += l.valor;
            c.consumido -= em_dinheiro;
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

        // Gasto coberto de cada categoria, dividido entre os cartões.
        let mut coberto_no_cartao: HashMap<&str, i64> = HashMap::new();
        let mut positivo: HashMap<&str, i64> = de_pagamento
            .iter()
            .map(|(_, cartao)| {
                let movimento = cartoes.get(&(*cartao, mes)).copied().unwrap_or_default();
                let anterior = saldos.no_fim_de(cartao, add_months(mes, -1));
                (*cartao, saldo_positivo_novo(anterior, movimento) - pagamento_alem_da_divida(anterior, movimento))
            })
            .collect();
        for (n, calc) in normais.iter().zip(&calculadas) {
            let Some(s) = saidas.get(&(n.id.as_str(), mes)) else { continue };
            let mut gastos: Vec<i64> = s.no_credito.iter().map(|(_, valor)| *valor).collect();
            let mut coberto = calc.calculo.budgeted_credit_outflows;
            if calc.calculo.balance.unwrap_or(0) < 0 {
                coberto -= estornos_na_categoria_estourada(&s.no_credito, &mut gastos, &mut positivo);
            }
            let cobertos = funded_by_card(coberto, &gastos);
            for ((cartao, _), coberto) in s.no_credito.iter().zip(cobertos) {
                *coberto_no_cartao.entry(cartao.as_str()).or_default() += coberto;
            }
        }

        let mut adicional = 0;
        for (c, cartao) in &de_pagamento {
            let coberto = coberto_no_cartao.get(cartao).copied().unwrap_or(0);
            let movimento = cartoes.get(&(*cartao, mes)).copied().unwrap_or_default();
            // Lançado direto na categoria de pagamento (sem ser transferência):
            // conta como saída dela.
            let direto = saidas.get(&(c.id.as_str(), mes)).map_or(0, |s| s.dinheiro + s.credito);
            let saldo_anterior = saldos.no_fim_de(cartao, add_months(mes, -1));
            let excesso = pagamento_alem_da_divida(saldo_anterior, movimento);
            let mut calc = SubCategoryMonthCalc {
                month: mes,
                budgeted: atribuido.get(&(c.id.as_str(), mes)).copied().unwrap_or(0),
                cash_outflows: Some(direto - movimento.pagamentos),
                budgeted_spending: gasto_coberto_limitado(coberto, saldo_anterior, movimento, excesso),
                ..Default::default()
            };
            let previa = anterior_da(&c.id);
            calculate_credit_card_payment_balance(&mut calc, previa, round);
            calculate_budgeted_cash_credit_outflows(&mut calc);
            if let Some(meta) = &c.meta {
                let saldo_do_cartao = Some(saldos.no_fim_de(cartao, mes));
                let tipo = CategoryType::CreditCardPayment;
                calculate_goals(&mut calc, previa, meta, tipo, saldo_do_cartao, orcamento.casas_decimais);
            }
            adicional += excesso;
            calculadas.push(CalculoCategoria { categoria: c.id.clone(), calculo: calc });
        }
        // Na ordem de `orcamento.categorias`.
        let ordem: HashMap<&str, usize> = orcamento.categorias.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
        calculadas.sort_by_key(|c| ordem[c.categoria.as_str()]);

        let soma = somas.get(&mes);
        let mut sem_categoria = uncategorized_row(
            soma.map_or(0, |s| s.sem_categoria_dinheiro),
            soma.map_or(0, |s| s.sem_categoria_credito),
            anterior.map(|a| &a.sem_categoria),
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
/// Não está no bundle (é soma do servidor). **Conferido na tela** nos
/// roteiros 5 e 8 da análise 03: pagar além da dívida (inclusive a do mês
/// anterior) soma o excesso ao "A atribuir"; devolver o crédito para a
/// corrente não o desfaz; gastar o crédito no mesmo mês desfaz; um estorno
/// no cartão depois de pagar além da dívida também soma. O campo
/// `additionalToBeBudgeted` existe só na categoria de pagamento do cartão
/// (análise 01, §3).
fn pagamento_alem_da_divida(saldo_anterior: i64, mes: Cartao) -> i64 {
    saldo_positivo_novo(saldo_anterior, mes).min(mes.pagamentos.max(0))
}

/// Quanto o cartão ficou positivo no mês além do saldo positivo que já tinha
/// e que não foi gasto. O crédito trazido que os gastos consumiram
/// ([`credito_consumido`]) e que um pagamento recria é positivo novo:
/// conferido no 10E da análise 03 (cartão com +20 de setembro, gasto de 30 e
/// pagamento de 30 em outubro: +20 no "A atribuir" de outubro).
fn saldo_positivo_novo(saldo_anterior: i64, mes: Cartao) -> i64 {
    let depois = saldo_anterior + mes.pagamentos + mes.gastos;
    (depois.max(0) - (saldo_anterior.max(0) - mes.consumido)).max(0)
}

/// Gasto coberto que passa para a categoria de pagamento (`budgeted_spending`).
///
/// O gasto pago com o crédito trazido do mês anterior já chega aqui fora do
/// coberto, porque conta como dinheiro ([`credito_consumido`]). O estorno
/// (`coberto > 0`) só sai da categoria de pagamento na parte que reduz a
/// dívida do cartão ou que vai para o "A atribuir"
/// ([`pagamento_alem_da_divida`]); a parte que só deixa o cartão positivo
/// fica na categoria do estorno. Conferido no roteiro 8 (estorno de 20 com
/// dívida de 50 tirou 20 do pagamento; com cartão em 0 e sem pagamento no
/// mês, não tirou nada; depois de pagar além da dívida, tirou 20 e o
/// "A atribuir" subiu 20) e no 10D (cartão com +20 de setembro, estorno de
/// 10 em outubro: não tirou nada). Com estornos em vários cartões, vale por
/// cartão (roteiro 12C: o estorno de 50 no cartão que ficou positivo não
/// tirou nada; o de 10 no cartão com dívida tirou 10).
fn gasto_coberto_limitado(coberto: i64, saldo_anterior: i64, mes: Cartao, excesso: i64) -> i64 {
    if coberto <= 0 {
        return coberto;
    }
    let so_positivo = saldo_positivo_novo(saldo_anterior, mes) - excesso;
    (coberto - so_positivo).max(0)
}

/// Estornos no crédito de uma categoria estourada: nenhum sai do pagamento
/// do próprio cartão (zera o valor em `gastos`), e só a parte que deixa o
/// cartão positivo (`positivo`, consumido aqui) cobre as saídas da
/// categoria. Devolve quanto cobre (≥ 0).
///
/// Conferido no roteiro 13 da análise 03 (Farmácia com 60 e saída de 100
/// no Cartão 3): o estorno de 10 no Cartão, que tinha dívida, não mexeu em
/// pagamento nenhum (Cartão 40, Cartão 3 60); o de 20 no Cartão 2, que
/// ficou positivo, passou para o pagamento do Cartão 3 (80). Com a
/// categoria sem estouro, cada estorno sai do pagamento do próprio cartão
/// (roteiro 12, [`funded_by_card`]). Regra tirada de dois passos; a parte
/// que reduz a dívida pode ter outra explicação.
fn estornos_na_categoria_estourada(
    no_credito: &[(String, i64)],
    gastos: &mut [i64],
    positivo: &mut HashMap<&str, i64>,
) -> i64 {
    let mut cobre = 0;
    for ((cartao, _), valor) in no_credito.iter().zip(gastos.iter_mut()) {
        if *valor > 0 {
            if let Some(p) = positivo.get_mut(cartao.as_str()) {
                let parte = (*valor).min(*p).max(0);
                *p -= parte;
                cobre += parte;
            }
            *valor = 0;
        }
    }
    cobre
}

/// Quanto de cada saída no cartão (pelo índice em `lancamentos`, ≥ 0) foi
/// pago com o saldo positivo que o cartão trouxe do fim do mês anterior.
/// Essa parte não vira dívida: conta como gasto em dinheiro da categoria
/// (ou da linha "sem categoria"), e os pagamentos do mês não a mudam.
///
/// Conferido na análise 03: no 9C (gasto coberto de 10 com +10 de setembro,
/// pagamento 0 / 0); no 9E (o crédito de 10 foi para o gasto de 30 na
/// Farmácia, não para o de 10 em Mercado, do mesmo dia); no 10E (saída sem
/// categoria de 30 com +20 de setembro: 20 estouram em dinheiro e descontam
/// do mês seguinte); no 11D (no mesmo dia, Mercado 10 lançado antes e
/// Farmácia 30: o crédito foi para a Farmácia). As saídas são consumidas por
/// data e, no mesmo dia, da maior para a menor.
fn credito_consumido(lancamentos: &[Lancamento], saldos: &SaldosPorMes) -> HashMap<usize, i64> {
    let mut saidas: BTreeMap<(&str, Date), Vec<usize>> = BTreeMap::new();
    for (i, l) in lancamentos.iter().enumerate() {
        let no_credito = matches!(&l.classe, Classe::Comum { cartao: Some(_), .. } | Classe::SemCategoria { credito: true });
        if no_credito && l.valor < 0 {
            saidas.entry((l.conta.as_str(), start_of_month(l.data))).or_default().push(i);
        }
    }
    let mut consumido = HashMap::new();
    for ((cartao, mes), mut indices) in saidas {
        let mut credito = saldos.no_fim_de(cartao, add_months(mes, -1));
        indices.sort_by_key(|&i| (lancamentos[i].data, lancamentos[i].valor));
        for i in indices {
            if credito <= 0 {
                break;
            }
            let parte = credito.min(-lancamentos[i].valor);
            consumido.insert(i, parte);
            credito -= parte;
        }
    }
    consumido
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
