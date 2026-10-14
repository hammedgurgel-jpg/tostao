//! Motor de cálculo a partir das entidades do orçamento. Os roteiros
//! repetem as conferências feitas na tela do YNAB (análises 01 e 03), agora
//! lançando as transações em vez de passar somas prontas.

use tostao::calc::SubCategoryMonthCalc;
use tostao::dates::Date;
use tostao::motor::*;

fn dia(m: u32, d: u32) -> Date {
    Date::from_ymd_opt(2026, m, d).unwrap()
}

const OUT: u32 = 10;
const NOV: u32 = 11;

/// Monta um orçamento passo a passo, como na tela.
struct Roteiro {
    orc: Orcamento,
    proximo_id: usize,
}

impl Roteiro {
    /// Corrente com 1.000 de saldo inicial classificado como receita, e as
    /// categorias Mercado, Lazer, Farmácia e Investimentos.
    fn novo() -> Self {
        let mut r = Roteiro { orc: Orcamento { ultimo_mes: Some(dia(NOV, 1)), ..Default::default() }, proximo_id: 0 };
        r.conta("corrente", TipoConta::Dinheiro, true);
        r.categoria("receita", TipoCategoria::Receita);
        for c in ["mercado", "lazer", "farmacia", "investimentos"] {
            r.categoria(c, TipoCategoria::Normal);
        }
        r.lanca("corrente", 1_000_000, Some("receita"));
        r
    }

    fn conta(&mut self, id: &str, tipo: TipoConta, orcamentaria: bool) {
        self.orc.contas.push(Conta { id: id.into(), tipo, orcamentaria });
        if tipo == TipoConta::Credito {
            let cartao = id.to_string();
            self.categoria(&format!("pagamento-{id}"), TipoCategoria::PagamentoCartao { cartao });
        }
    }

    fn cartao(&mut self, id: &str) {
        self.conta(id, TipoConta::Credito, true);
    }

    fn categoria(&mut self, id: &str, tipo: TipoCategoria) {
        self.orc.categorias.push(Categoria { id: id.into(), tipo, meta: None });
    }

    fn atribui(&mut self, categoria: &str, valor: i64) {
        self.orc.atribuicoes.push(Atribuicao { categoria: categoria.into(), mes: dia(OUT, 1), valor });
    }

    fn id(&mut self) -> String {
        self.proximo_id += 1;
        format!("t{}", self.proximo_id)
    }

    /// Transação comum em 13/10.
    fn lanca(&mut self, conta: &str, valor: i64, categoria: Option<&str>) -> String {
        let id = self.id();
        self.orc.transacoes.push(Transacao {
            id: id.clone(),
            conta: conta.into(),
            data: dia(OUT, 13),
            valor,
            categoria: categoria.map(Into::into),
            ..Default::default()
        });
        id
    }

    /// As duas pontas de uma transferência de `valor` (> 0) de `de` para
    /// `para`, em 13/10. `categoria` vai na ponta de origem.
    fn transfere(&mut self, de: &str, para: &str, valor: i64, categoria: Option<&str>) -> (String, String) {
        let (a, b) = (self.id(), self.id());
        self.orc.transacoes.push(Transacao {
            id: a.clone(),
            conta: de.into(),
            data: dia(OUT, 13),
            valor: -valor,
            categoria: categoria.map(Into::into),
            conta_transferencia: Some(para.into()),
            ..Default::default()
        });
        self.orc.transacoes.push(Transacao {
            id: b.clone(),
            conta: para.into(),
            data: dia(OUT, 13),
            valor,
            conta_transferencia: Some(de.into()),
            ..Default::default()
        });
        (a, b)
    }

    fn transacao(&mut self, id: &str) -> &mut Transacao {
        self.orc.transacoes.iter_mut().find(|t| t.id == id).unwrap()
    }

    fn remove(&mut self, id: &str) {
        self.transacao(id).removida = true;
    }

    fn calcula(&self) -> Calculos {
        calcular(&self.orc)
    }
}

trait Tela {
    fn a_atribuir(&self, mes: u32) -> i64;
    fn cat(&self, mes: u32, categoria: &str) -> &SubCategoryMonthCalc;
    fn disponivel(&self, mes: u32, categoria: &str) -> i64;
    fn conta(&self, conta: &str) -> i64;
    /// Coluna "Activity": saídas, mais o gasto coberto na categoria de pagamento.
    fn atividade(&self, mes: u32, categoria: &str) -> i64;
}

impl Tela for Calculos {
    fn a_atribuir(&self, mes: u32) -> i64 {
        self.mes(dia(mes, 1)).unwrap().a_atribuir
    }
    fn cat(&self, mes: u32, categoria: &str) -> &SubCategoryMonthCalc {
        self.mes(dia(mes, 1)).unwrap().categoria(categoria).unwrap()
    }
    fn disponivel(&self, mes: u32, categoria: &str) -> i64 {
        self.cat(mes, categoria).balance.unwrap()
    }
    fn conta(&self, conta: &str) -> i64 {
        self.saldo(conta).unwrap().saldo
    }
    fn atividade(&self, mes: u32, categoria: &str) -> i64 {
        let c = self.cat(mes, categoria);
        c.cash_outflows.unwrap_or(0) + c.credit_outflows.unwrap_or(0) - c.budgeted_spending
    }
}

#[test]
fn roteiro_1_estouro_do_mes_a_partir_de_transacoes() {
    // Análise 01, §2: corrente com 1.000, cartão com 0, nada em novembro.
    let mut r = Roteiro::novo();
    r.cartao("cartao");
    assert_eq!(r.calcula().a_atribuir(OUT), 1_000_000);

    // 1A: atribui 50 a Mercado e gasta 80 na corrente.
    r.atribui("mercado", 50_000);
    r.lanca("corrente", -80_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "mercado"), -30_000);
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 920_000));

    // 1B: gasta 40 na corrente, sem categoria.
    r.lanca("corrente", -40_000, None);
    let c = r.calcula();
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-40_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 880_000));

    // 1C: gasta 60 no cartão, em Lazer (nada atribuído).
    r.lanca("cartao", -60_000, Some("lazer"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "lazer"), -60_000);
    assert_eq!(c.cat(OUT, "lazer").credit_overspent(), -60_000, "aviso de crédito");
    assert_eq!(c.conta("cartao"), -60_000);
    assert_eq!(c.a_atribuir(NOV), 880_000);

    // 1D: gasta 25 no cartão, sem categoria.
    r.lanca("cartao", -25_000, None);
    let c = r.calcula();
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-65_000));
    assert_eq!(c.conta("cartao"), -85_000);
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 880_000));
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().calculo.over_spent, -70_000);

    // Em novembro, Mercado e Lazer começam em 0; o pagamento do cartão fica em 0.
    assert_eq!(c.disponivel(NOV, "mercado"), 0);
    assert_eq!(c.disponivel(NOV, "lazer"), 0);
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!(c.disponivel(NOV, "pagamento-cartao"), 0);
}

#[test]
fn roteiro_2_pagamento_do_cartao_a_partir_de_transacoes() {
    // Análise 01, §3: corrente com 1.000, cartão com 0, 100 em Mercado.
    let mut r = Roteiro::novo();
    r.cartao("cartao");
    r.atribui("mercado", 100_000);
    assert_eq!(r.calcula().a_atribuir(OUT), 900_000);

    // 2A: gasta 30 no cartão, em Mercado.
    r.lanca("cartao", -30_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "mercado"), 70_000);
    assert_eq!(c.atividade(OUT, "pagamento-cartao"), 30_000);
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 30_000);
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-30_000, 900_000));

    // 2B: paga 30 da corrente para o cartão.
    r.transfere("corrente", "cartao", 30_000, None);
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "mercado"), 70_000);
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (0, 900_000));

    // 2C: gasta 30 no cartão, em Lazer, sem dinheiro.
    r.lanca("cartao", -30_000, Some("lazer"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-30_000, 900_000));

    // 2D: atribui 20 a Farmácia e gasta 50 no cartão nela.
    r.atribui("farmacia", 20_000);
    r.lanca("cartao", -50_000, Some("farmacia"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "mercado"), 70_000);
    assert_eq!(c.atividade(OUT, "pagamento-cartao"), 20_000);
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 20_000);
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-80_000, 880_000));

    // 2E: novembro, sem novos gastos.
    assert_eq!(c.disponivel(NOV, "mercado"), 70_000);
    assert_eq!(c.atividade(NOV, "pagamento-cartao"), 0);
    assert_eq!(c.disponivel(NOV, "pagamento-cartao"), 20_000);
    assert_eq!(c.a_atribuir(NOV), 880_000);
    // Lazer (−30) e Farmácia (−30) começam em 0; o estouro no crédito não desconta.
    assert_eq!((c.disponivel(NOV, "lazer"), c.disponivel(NOV, "farmacia")), (0, 0));
    // "Left Over from Last Month" de novembro: 70 do Mercado + 20 do pagamento.
    let sobra: i64 = c.mes(dia(NOV, 1)).unwrap().categorias.iter().map(|c| c.calculo.balance_previous_month.unwrap().max(0)).sum();
    assert_eq!(sobra, 90_000);
}

#[test]
fn roteiro_3_transferencia_entre_contas_orcamentarias() {
    let mut r = Roteiro::novo();
    r.conta("poupanca", TipoConta::Dinheiro, true);

    // 3A: 200 da Corrente para a Poupança.
    let (corrente, poupanca) = r.transfere("corrente", "poupanca", 200_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca"), c.a_atribuir(OUT)), (800_000, 200_000, 1_000_000));
    let classes: Vec<_> = classificar(&r.orc).into_iter().filter(|l| l.transacao != "t1").map(|l| l.classe).collect();
    assert_eq!(classes, [Classe::TransferenciaEntreOrcamentarias, Classe::TransferenciaEntreOrcamentarias]);

    // 3B: na Poupança, o valor vai a 250 (a espelho acompanha).
    r.transacao(&poupanca).valor = 250_000;
    r.transacao(&corrente).valor = -250_000;
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca"), c.a_atribuir(OUT)), (750_000, 250_000, 1_000_000));

    // 3C: a ponta da Poupança fica compensada; a da Corrente, não.
    r.transacao(&poupanca).compensacao = Compensacao::Compensada;
    r.transacao("t1").compensacao = Compensacao::Compensada;
    let c = r.calcula();
    assert_eq!(c.saldo("poupanca").unwrap().saldo_compensado, 250_000);
    assert_eq!(c.saldo("corrente").unwrap().saldo_compensado, 1_000_000);
    assert_eq!(c.conta("corrente"), 750_000);

    // 3D: data passada, as duas pontas mudam juntas; nada muda.
    r.transacao(&corrente).data = dia(OUT, 10);
    r.transacao(&poupanca).data = dia(OUT, 10);
    assert_eq!(r.calcula().a_atribuir(OUT), 1_000_000);

    // 3E: apagar uma ponta apaga a outra.
    r.remove(&corrente);
    r.remove(&poupanca);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca"), c.a_atribuir(OUT)), (1_000_000, 0, 1_000_000));
    for cat in &c.mes(dia(OUT, 1)).unwrap().categorias {
        assert_eq!(cat.calculo.balance, Some(0), "nenhuma categoria muda: {}", cat.categoria);
    }
}

#[test]
fn roteiro_4_orcamentaria_e_acompanhamento() {
    // Na tela, os 300 não foram atribuídos a Investimentos antes.
    let mut r = Roteiro::novo();
    r.conta("investimento", TipoConta::Dinheiro, false);

    // 4A: 300 da Corrente para o Investimento, categoria Investimentos.
    let (ida, _) = r.transfere("corrente", "investimento", 300_000, Some("investimentos"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (700_000, 300_000));
    assert_eq!(c.cat(OUT, "investimentos").cash_outflows, Some(-300_000));
    assert_eq!(c.disponivel(OUT, "investimentos"), -300_000);
    assert_eq!(c.a_atribuir(OUT), 1_000_000);

    // 4B: 100 do Investimento para a Corrente, sem categoria: fica na linha
    // "sem categoria" com +100 e não entra no "A atribuir".
    let (_, volta) = r.transfere("investimento", "corrente", 100_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (800_000, 200_000));
    let out = c.mes(dia(OUT, 1)).unwrap();
    assert_eq!((out.sem_categoria.cash_outflows, out.sem_categoria.balance), (Some(100_000), Some(100_000)));
    assert_eq!(c.a_atribuir(OUT), 1_000_000);

    // 4C: classificada como "Pronto para atribuir", vira receita.
    r.transacao(&volta).categoria = Some("receita".into());
    let c = r.calcula();
    assert_eq!(c.a_atribuir(OUT), 1_100_000);
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(0));

    // 4D: apagar a ponta do Investimento apaga a da Corrente.
    let ponta_investimento = r.orc.transacoes.iter().find(|t| t.conta == "investimento" && t.valor < 0).unwrap().id.clone();
    r.remove(&ponta_investimento);
    r.remove(&volta);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento"), c.a_atribuir(OUT)), (700_000, 300_000, 1_000_000));

    // 4E (inferido, plano não visto): sem categoria, a ponta vira "sem
    // categoria" em dinheiro e desconta do "A atribuir" de novembro.
    r.transacao(&ida).categoria = None;
    let c = r.calcula();
    assert_eq!(c.conta("corrente"), 700_000);
    assert_eq!(c.disponivel(OUT, "investimentos"), 0);
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-300_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (1_000_000, 700_000));
}

#[test]
fn roteiro_5_cartao_de_credito() {
    // Na tela, os 100 não foram atribuídos a Mercado antes.
    let mut r = Roteiro::novo();
    r.cartao("cartao");

    // 5A: gasta 60 no Cartão, em Mercado.
    r.lanca("cartao", -60_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (1_000_000, -60_000));
    assert_eq!(c.disponivel(OUT, "mercado"), -60_000);
    assert_eq!(c.cat(OUT, "mercado").credit_overspent(), -60_000, "amarelo, crédito");
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!(c.a_atribuir(OUT), 1_000_000);

    // 5B: no Cartão, entrada de 60 vinda da Corrente: é pagamento.
    let (pagamento, _) = r.transfere("corrente", "cartao", 60_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!(c.atividade(OUT, "pagamento-cartao"), -60_000);
    let pag = c.cat(OUT, "pagamento-cartao");
    assert_eq!((pag.cash_outflows, pag.balance), (Some(-60_000), Some(-60_000)));
    assert_eq!(pag.cash_overspent(), -60_000, "vermelho, estouro em dinheiro");
    assert_eq!(c.a_atribuir(OUT), 1_000_000);
    let classes: Vec<_> = classificar(&r.orc).into_iter().filter(|l| l.transacao == pagamento || l.conta == "cartao").map(|l| l.classe).collect();
    assert!(classes.contains(&Classe::PagamentoCartao { cartao: "cartao".into() }));

    // 5C: saída de 10 da Corrente para o Cartão (escolhendo a categoria de
    // pagamento, que vira transferência e some ao salvar).
    r.transfere("corrente", "cartao", 10_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (930_000, 10_000));
    assert_eq!(c.disponivel(OUT, "mercado"), -60_000);
    assert_eq!(c.atividade(OUT, "pagamento-cartao"), -70_000);
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), -70_000);
    assert_eq!(c.a_atribuir(OUT), 1_010_000, "o +10 do pagamento além da dívida (regra inferida)");

    // 5D: saída de 10 do Cartão para a Corrente: não é pagamento.
    r.transfere("cartao", "corrente", 10_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), -70_000);
    assert_eq!(c.a_atribuir(OUT), 1_010_000);

    // Novembro: o estouro em dinheiro do pagamento (−70) desconta; o de
    // crédito do Mercado (−60) não.
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().calculo.over_spent, -70_000);
    assert_eq!(c.a_atribuir(NOV), 940_000);
    assert_eq!((c.disponivel(NOV, "pagamento-cartao"), c.disponivel(NOV, "mercado")), (0, 0));
}

/// Divisão de 100 na Corrente: 70 em Mercado e 30 para a Poupança.
fn divisao_com_transferencia(r: &mut Roteiro) -> (String, String) {
    let (divisao, espelho) = (r.id(), r.id());
    r.orc.transacoes.push(Transacao {
        id: divisao.clone(),
        conta: "corrente".into(),
        data: dia(OUT, 13),
        valor: -100_000,
        partes: vec![
            Parte { valor: -70_000, categoria: Some("mercado".into()), ..Default::default() },
            Parte { valor: -30_000, conta_transferencia: Some("poupanca".into()), ..Default::default() },
        ],
        ..Default::default()
    });
    r.orc.transacoes.push(Transacao {
        id: espelho.clone(),
        conta: "poupanca".into(),
        data: dia(OUT, 13),
        valor: 30_000,
        conta_transferencia: Some("corrente".into()),
        ..Default::default()
    });
    (divisao, espelho)
}

#[test]
fn roteiro_6_divisao_com_transferencia() {
    let mut r = Roteiro::novo();
    r.conta("poupanca", TipoConta::Dinheiro, true);
    r.atribui("mercado", 100_000);
    assert_eq!(r.calcula().a_atribuir(OUT), 900_000);

    // 6A: a parte de 30 é transferência; a de 70 sai de Mercado.
    let (divisao, espelho) = divisao_com_transferencia(&mut r);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca")), (900_000, 30_000));
    assert_eq!(c.disponivel(OUT, "mercado"), 30_000);
    assert_eq!(c.a_atribuir(OUT), 900_000);
    let partes: Vec<_> = classificar(&r.orc).into_iter().filter(|l| l.transacao == divisao).map(|l| (l.valor, l.classe)).collect();
    assert_eq!(
        partes,
        [
            (-70_000, Classe::Comum { categoria: "mercado".into(), cartao: None }),
            (-30_000, Classe::TransferenciaEntreOrcamentarias),
        ]
    );

    // 6E: apagar a divisão apaga a espelho.
    r.remove(&divisao);
    r.remove(&espelho);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca")), (1_000_000, 0));
    assert_eq!(c.disponivel(OUT, "mercado"), 100_000);
    assert_eq!(c.a_atribuir(OUT), 900_000);
}

#[test]
fn roteiro_7_pontas_geradas_pela_agendada() {
    // A agendada em si não entra no cálculo (só as transações geradas).
    let mut r = Roteiro::novo();
    r.conta("poupanca", TipoConta::Dinheiro, true);
    r.atribui("mercado", 100_000);

    // 7A/7B: só a agendada existe; nada muda.
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca"), c.a_atribuir(OUT)), (1_000_000, 0, 900_000));

    // 7C: "Enter Now" cria as duas pontas, não compensadas.
    r.transfere("corrente", "poupanca", 50_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("poupanca"), c.a_atribuir(OUT)), (950_000, 50_000, 900_000));
    assert_eq!(c.saldo("poupanca").unwrap().saldo_compensado, 0);
    assert_eq!(c.disponivel(OUT, "mercado"), 100_000);
}

#[test]
fn categoria_que_usa_dois_cartoes_cobre_primeiro_o_maior_gasto() {
    // Conferido na tela (análise 03, roteiros 9E e 10): o cartão com o maior
    // gasto da categoria é coberto primeiro; o estouro fica no menor.
    let mut r = Roteiro::novo();
    r.cartao("visa");
    r.cartao("master");
    r.atribui("mercado", 60_000);
    r.lanca("visa", -30_000, Some("mercado"));
    r.lanca("master", -70_000, Some("mercado"));
    let c = r.calcula();
    // Mercado: 100 no crédito, 60 cobertos, 40 de estouro no crédito.
    assert_eq!(c.cat(OUT, "mercado").budgeted_credit_outflows, -60_000);
    assert_eq!(c.cat(OUT, "mercado").credit_overspent(), -40_000);
    // 60 cobertos, todos no Master (70); o Visa (30) fica com o estouro.
    assert_eq!(c.disponivel(OUT, "pagamento-visa"), 0);
    assert_eq!(c.disponivel(OUT, "pagamento-master"), 60_000);
    // O Lazer, coberto inteiro, vai todo para o cartão em que foi gasto.
    r.atribui("lazer", 10_000);
    r.lanca("visa", -10_000, Some("lazer"));
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "pagamento-visa"), 10_000);
    assert_eq!(c.disponivel(OUT, "pagamento-master"), 60_000);
    assert_eq!(c.a_atribuir(OUT), 930_000);
    assert_eq!(c.a_atribuir(NOV), 930_000, "estouro no crédito não desconta");
}

#[test]
fn divisao_com_transferencia_para_acompanhamento_e_parte_removida() {
    let mut r = Roteiro::novo();
    r.conta("investimento", TipoConta::Dinheiro, false);
    r.atribui("mercado", 100_000);
    let id = r.id();
    r.orc.transacoes.push(Transacao {
        id: id.clone(),
        conta: "corrente".into(),
        data: dia(OUT, 13),
        valor: -150_000,
        partes: vec![
            Parte { valor: -40_000, categoria: Some("mercado".into()), ..Default::default() },
            // Parte com acompanhamento: comum na ponta orçamentária.
            Parte { valor: -100_000, categoria: Some("investimentos".into()), conta_transferencia: Some("investimento".into()), ..Default::default() },
            // Parte com acompanhamento e sem categoria: "sem categoria".
            Parte { valor: -10_000, conta_transferencia: Some("investimento".into()), ..Default::default() },
            // Parte removida: ignorada.
            Parte { valor: -999_000, categoria: Some("lazer".into()), removida: true, ..Default::default() },
        ],
        ..Default::default()
    });
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "mercado"), 60_000);
    assert_eq!(c.disponivel(OUT, "investimentos"), -100_000);
    assert_eq!(c.disponivel(OUT, "lazer"), 0);
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-10_000));
    assert_eq!(c.conta("corrente"), 850_000);
    assert_eq!(c.a_atribuir(NOV), 900_000 - 110_000);
}

#[test]
fn transacao_crua_e_conta_de_acompanhamento_sao_ignoradas() {
    let mut r = Roteiro::novo();
    r.conta("investimento", TipoConta::Dinheiro, false);
    let crua = r.lanca("corrente", -50_000, Some("mercado"));
    r.transacao(&crua).crua = true;
    r.lanca("investimento", -20_000, Some("lazer"));
    let c = r.calcula();
    assert_eq!(c.conta("corrente"), 1_000_000, "a crua não entra no saldo");
    assert_eq!(c.conta("investimento"), -20_000);
    assert_eq!(c.disponivel(OUT, "mercado"), 0);
    assert_eq!(c.disponivel(OUT, "lazer"), 0);
    assert_eq!(c.a_atribuir(NOV), 1_000_000);
    let classes: Vec<_> = classificar(&r.orc).into_iter().filter(|l| l.transacao != "t1").map(|l| l.classe).collect();
    assert_eq!(classes, [Classe::Ignorada, Classe::Ignorada]);
}

#[test]
fn saldo_negativo_nao_passa_e_atribuido_no_futuro_sai_do_a_atribuir() {
    let mut r = Roteiro::novo();
    r.atribui("mercado", 50_000);
    r.lanca("corrente", -20_000, Some("mercado"));
    r.orc.atribuicoes.push(Atribuicao { categoria: "lazer".into(), mes: dia(NOV, 1), valor: 30_000 });
    let c = r.calcula();
    assert_eq!(c.disponivel(NOV, "mercado"), 30_000);
    assert_eq!(c.cat(NOV, "mercado").balance_previous_month, Some(30_000));
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().calculo.available_to_budget, 950_000);
    assert_eq!(c.a_atribuir(OUT), 920_000, "30 já atribuídos em novembro");
    assert_eq!(c.a_atribuir(NOV), 920_000);
}

#[test]
fn metas_sao_calculadas_por_mes() {
    use tostao::goals::{Goal, GoalType};
    let mut r = Roteiro::novo();
    r.orc.categorias.iter_mut().find(|c| c.id == "mercado").unwrap().meta = Some(Goal {
        goal_type: Some(GoalType::MonthlyFunding),
        created_on: Some(dia(OUT, 1)),
        monthly_funding: 80_000,
        ..Default::default()
    });
    r.atribui("mercado", 50_000);
    let c = r.calcula();
    assert_eq!(c.cat(OUT, "mercado").goal.under_funded, 30_000);
    assert_eq!(c.cat(NOV, "mercado").goal.under_funded, 80_000);
}

#[test]
fn adaptador_le_as_entidades_do_protocolo() {
    use serde_json::json;
    use tostao::motor::protocolo::orcamento_das_entidades;
    use tostao::protocol::IncomingChangedEntities;

    // Roteiro 6A e o roteiro 2A em entidades do protocolo, mais uma crua e
    // uma categoria removida.
    let e = IncomingChangedEntities {
        be_accounts: Some(vec![
            json!({"id": "corrente", "account_type": "Checking", "on_budget": true, "is_tombstone": false}),
            json!({"id": "poupanca", "account_type": "Savings", "on_budget": true}),
            json!({"id": "cartao", "account_type": "CreditCard", "on_budget": true}),
            json!({"id": "fechada", "account_type": "Checking", "on_budget": true, "is_tombstone": true}),
        ]),
        be_subcategories: Some(vec![
            json!({"id": "receita", "internal_name": "Category/__ImmediateIncome__"}),
            json!({"id": "divisao", "internal_name": "Category/__Split__"}),
            json!({"id": "mercado", "type": "DFT", "goal_type": "MF", "goal_created_on": "2026-10-01", "monthly_funding": 150_000}),
            json!({"id": "pagamento", "type": "DBT", "entities_account_id": "cartao"}),
            json!({"id": "velha", "is_tombstone": true}),
        ]),
        be_monthly_budgets: Some(vec![json!({"id": "mb/2026-10", "month": "2026-10-01"})]),
        be_monthly_subcategory_budgets: Some(vec![
            json!({"id": "mcb/2026-10/mercado", "entities_monthly_budget_id": "mb/2026-10", "entities_subcategory_id": "mercado", "budgeted": 100_000}),
        ]),
        be_transactions: Some(vec![
            json!({"id": "inicial", "entities_account_id": "corrente", "date": "2026-10-01", "amount": 1_000_000, "entities_subcategory_id": "receita", "cleared": "Cleared"}),
            json!({"id": "div", "entities_account_id": "corrente", "date": "2026-10-13", "amount": -100_000, "entities_subcategory_id": "divisao", "source": "Matched"}),
            json!({"id": "esp", "entities_account_id": "poupanca", "date": "2026-10-13", "amount": 30_000, "transfer_account_id": "corrente", "transfer_subtransaction_id": "p2"}),
            json!({"id": "cart", "entities_account_id": "cartao", "date": "2026-10-13", "amount": -30_000, "entities_subcategory_id": "mercado"}),
            json!({"id": "crua", "entities_account_id": "corrente", "date": "2026-10-13", "amount": -5_000, "entities_subcategory_id": "mercado", "source": "raw_import"}),
            json!({"id": "fora", "entities_account_id": "corrente", "date": "2026-10-13", "amount": -7_000, "entities_subcategory_id": "velha"}),
        ]),
        be_subtransactions: Some(vec![
            json!({"id": "p1", "entities_transaction_id": "div", "amount": -70_000, "entities_subcategory_id": "mercado"}),
            json!({"id": "p2", "entities_transaction_id": "div", "amount": -30_000, "transfer_account_id": "poupanca", "transfer_transaction_id": "esp"}),
        ]),
        ..Default::default()
    };
    let orc = orcamento_das_entidades(&e).unwrap();
    assert_eq!(orc.contas.len(), 3);
    let c = calcular(&orc);
    assert_eq!(c.conta("corrente"), 1_000_000 - 100_000 - 7_000);
    assert_eq!(c.conta("poupanca"), 30_000);
    assert_eq!(c.saldo("corrente").unwrap().saldo_compensado, 1_000_000);
    let mercado = c.cat(OUT, "mercado");
    assert_eq!((mercado.cash_outflows, mercado.credit_outflows, mercado.balance), (Some(-70_000), Some(-30_000), Some(0)));
    assert_eq!(mercado.goal.under_funded, 50_000);
    assert_eq!(c.disponivel(OUT, "pagamento"), 30_000);
    // A categoria removida conta como "sem categoria".
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-7_000));
    assert_eq!(c.a_atribuir(OUT), 900_000);

    let sem_mes = IncomingChangedEntities {
        be_monthly_subcategory_budgets: Some(vec![json!({"id": "x", "entities_monthly_budget_id": "mb/2099-01", "entities_subcategory_id": "mercado"})]),
        ..Default::default()
    };
    let erro = orcamento_das_entidades(&sem_mes).unwrap_err();
    assert_eq!((erro.tipo, erro.campo), ("be_monthly_subcategory_budgets", "entities_monthly_budget_id"));
}

#[test]
fn roteiro_8_primeira_tentativa_sem_transferencia() {
    // Conferido na tela em 2026-10-13. Os "pagamentos" da Corrente foram
    // lançados sem beneficiário de transferência (saídas sem categoria), e o
    // gasto do 8F ficou sem categoria. O saldo inicial ficou em 13/10.
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.cartao("cartao");
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });

    // 8A: 50 no Cartão, em Mercado, em 15/09.
    let gasto_setembro = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto_setembro).data = dia(SET, 15);
    let c = r.calcula();
    assert_eq!((c.atividade(SET, "mercado"), c.disponivel(SET, "mercado")), (-50_000, 0));
    assert_eq!((c.atividade(SET, "pagamento-cartao"), c.disponivel(SET, "pagamento-cartao")), (50_000, 50_000));
    assert_eq!(c.a_atribuir(SET), -50_000, "You assigned more than you have");
    assert_eq!((c.disponivel(OUT, "pagamento-cartao"), c.a_atribuir(OUT)), (50_000, 950_000));

    // 8B e 8C como foram lançados: saídas de 50 e 10 na Corrente, sem categoria.
    r.lanca("corrente", -50_000, None);
    let c = r.calcula();
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-50_000));
    assert_eq!((c.conta("corrente"), c.conta("cartao"), c.a_atribuir(OUT)), (950_000, -50_000, 950_000));
    r.lanca("corrente", -10_000, None);
    let c = r.calcula();
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-60_000));
    assert_eq!((c.conta("corrente"), c.disponivel(OUT, "pagamento-cartao"), c.a_atribuir(OUT)), (940_000, 50_000, 950_000));

    // 8D: atribui 10 a Mercado e gasta 10 no Cartão em Mercado.
    r.atribui("mercado", 10_000);
    let gasto = r.lanca("cartao", -10_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, 0));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (10_000, 60_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-60_000, 940_000));

    // 8E: apaga o gasto e lança um estorno de 20 no Cartão em Mercado: o
    // estorno sai da categoria de pagamento.
    r.remove(&gasto);
    let estorno = r.lanca("cartao", 20_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (20_000, 30_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-20_000, 30_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-30_000, 940_000));

    // 8F como foi lançado: apaga o estorno, tira a atribuição e gasta 10 no
    // Cartão sem categoria.
    r.remove(&estorno);
    r.orc.atribuicoes.retain(|a| a.mes != dia(OUT, 1));
    r.lanca("cartao", -10_000, None);
    let c = r.calcula();
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-70_000));
    assert_eq!((c.disponivel(OUT, "mercado"), c.disponivel(OUT, "pagamento-cartao")), (0, 50_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-60_000, 950_000));
}

/// Segunda tentativa do roteiro 8, como foi lançada: o gasto de setembro
/// foi apagado, e os "pagamentos" continuaram como saídas sem categoria.
fn roteiro_8_segunda_tentativa() -> Roteiro {
    let mut r = Roteiro::novo();
    r.cartao("cartao");
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(9, 1), valor: 50_000 });
    r.lanca("corrente", -50_000, None);
    r.lanca("corrente", -10_000, None);
    r
}

#[test]
fn roteiro_8_segunda_tentativa_sem_transferencia() {
    // Conferido na tela em 2026-10-13.
    let mut r = roteiro_8_segunda_tentativa();
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!(c.disponivel(OUT, "mercado"), 50_000, "sobra de setembro");
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-60_000));
    assert_eq!((c.disponivel(OUT, "pagamento-cartao"), c.a_atribuir(OUT)), (0, 950_000));

    // 8D: atribui 10 e gasta 10 no cartão em Mercado.
    r.atribui("mercado", 10_000);
    r.lanca("cartao", -10_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, 50_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (10_000, 10_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-10_000, 940_000));

    // 8F como lançado: sem atribuição em outubro, 10 no cartão em Mercado,
    // cobertos pela sobra de setembro.
    r.orc.atribuicoes.retain(|a| a.mes != dia(OUT, 1));
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, 40_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (10_000, 10_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (-10_000, 950_000));
}

#[test]
fn roteiro_8e_estorno_com_cartao_sem_divida() {
    // Na tela: estorno de 20 em Mercado num cartão com saldo 0 e categoria
    // de pagamento com 0. Mercado +20 / 80, cartão +20, A atribuir 940, e a
    // categoria de pagamento ficou em 0 / 0.
    let mut r = roteiro_8_segunda_tentativa();
    r.atribui("mercado", 10_000);
    r.lanca("cartao", 20_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (20_000, 80_000));
    assert_eq!((c.conta("cartao"), c.a_atribuir(OUT)), (20_000, 940_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (0, 0));
}

#[test]
fn roteiro_8_pagamento_alem_da_divida_do_mes_anterior() {
    // Terceira tentativa do roteiro 8, conferida na tela em 2026-10-13.
    let mut r = Roteiro::novo();
    r.cartao("cartao");
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(9, 1), valor: 50_000 });
    let gasto_setembro = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto_setembro).data = dia(9, 15);

    // Passo 0.
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (1_000_000, -50_000));
    assert_eq!((c.atividade(9, "pagamento-cartao"), c.disponivel(9, "pagamento-cartao"), c.a_atribuir(9)), (50_000, 50_000, -50_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (0, 50_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 950_000));

    // 8B: paga os 50 da dívida de setembro: não vai para o A atribuir.
    r.transfere("corrente", "cartao", 50_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (950_000, 0));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-50_000, 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 950_000));

    // 8C: paga 10 além da dívida: +10 no A atribuir de outubro.
    r.transfere("corrente", "cartao", 10_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 10_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-60_000, -10_000));
    assert_eq!(c.cat(OUT, "pagamento-cartao").cash_overspent(), -10_000, "vermelho");
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (960_000, 950_000));
    assert_eq!((c.disponivel(NOV, "pagamento-cartao"), c.a_atribuir(9)), (0, -50_000));

    // 8D: atribui 10 a Mercado e gasta 10 no cartão em Mercado: o +10 volta.
    r.atribui("mercado", 10_000);
    r.lanca("cartao", -10_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, 0));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-50_000, 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (940_000, 940_000));

    // 8E: apaga o gasto do 8D e lança um estorno de 20 no cartão em Mercado.
    let gasto = r.orc.transacoes.iter().rev().find(|t| t.conta == "cartao" && t.valor == -10_000).unwrap().id.clone();
    r.remove(&gasto);
    r.lanca("cartao", 20_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 30_000));
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (20_000, 30_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-80_000, -30_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (970_000, 940_000));
    assert_eq!((c.disponivel(NOV, "mercado"), c.disponivel(NOV, "pagamento-cartao")), (30_000, 0));

    // 8F: apaga o estorno, tira a atribuição de outubro e gasta 10 no cartão
    // em Mercado, sem dinheiro: o +10 sai.
    let estorno = r.orc.transacoes.iter().rev().find(|t| t.conta == "cartao" && t.valor == 20_000).unwrap().id.clone();
    r.remove(&estorno);
    r.orc.atribuicoes.retain(|a| a.mes != dia(OUT, 1));
    r.lanca("cartao", -10_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, -10_000));
    assert_eq!(c.cat(OUT, "mercado").credit_overspent(), -10_000, "amarelo");
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (-60_000, -10_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 940_000));
}

/// Muda a data das duas pontas de uma transferência.
fn data_das_pontas(r: &mut Roteiro, (a, b): (String, String), data: Date) {
    r.transacao(&a).data = data;
    r.transacao(&b).data = data;
}

#[test]
fn roteiro_9_primeira_tentativa() {
    // Conferido na tela em 2026-10-13, como foi lançado: o "pagamento" do
    // 9A foi uma saída da Corrente em Mercado (sem gasto no Cartão), o gasto
    // do 9C foi uma saída sem categoria na Corrente, a transferência do 9D
    // ficou com a categoria "Inflow: Ready to Assign", e a Farmácia do 9E
    // recebeu os 60 em novembro.
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.transacao("t1").data = dia(SET, 1);
    r.cartao("cartao");
    r.conta("investimento", TipoConta::Dinheiro, false);
    let c = r.calcula();
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (1_000_000, 1_000_000, 1_000_000));

    // 9A como lançado: 50 em Mercado em setembro e 60 da Corrente em Mercado.
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });
    let saida = r.lanca("corrente", -60_000, Some("mercado"));
    r.transacao(&saida).data = dia(SET, 11);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (940_000, 0));
    assert_eq!((c.atividade(SET, "mercado"), c.disponivel(SET, "mercado")), (-60_000, -10_000));
    assert_eq!(c.cat(SET, "mercado").cash_overspent(), -10_000, "vermelho");
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT)), (950_000, 940_000));

    // 9B: 100 do Investimento para a Corrente em 01/09, sem categoria.
    let volta = r.transfere("investimento", "corrente", 100_000, None);
    data_das_pontas(&mut r, volta, dia(SET, 1));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (1_040_000, -100_000));
    let sem = &c.mes(dia(SET, 1)).unwrap().sem_categoria;
    assert_eq!((sem.cash_outflows, sem.balance), (Some(100_000), Some(100_000)));
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT)), (950_000, 940_000));
    // Outubro: o positivo é carregado ("Left Over from Last Month" 100).
    let sem = &c.mes(dia(OUT, 1)).unwrap().sem_categoria;
    assert_eq!((sem.balance_previous_month, sem.balance), (Some(100_000), Some(100_000)));

    // 9C como lançado: saída de 10 sem categoria na Corrente, em outubro. A
    // linha fica com 90 e não estoura.
    r.lanca("corrente", -10_000, None);
    let c = r.calcula();
    assert_eq!(c.conta("corrente"), 1_030_000);
    let sem = &c.mes(dia(OUT, 1)).unwrap().sem_categoria;
    assert_eq!((sem.cash_outflows, sem.balance, sem.cash_overspent()), (Some(-10_000), Some(90_000), 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (940_000, 940_000));

    // 9D como lançado: 300 da Corrente para o Investimento com a categoria
    // "Inflow: Ready to Assign": sai do A atribuir no mesmo mês.
    r.transfere("corrente", "investimento", 300_000, Some("receita"));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (730_000, 200_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (640_000, 640_000));
    let sem = &c.mes(dia(NOV, 1)).unwrap().sem_categoria;
    assert_eq!((sem.balance_previous_month, sem.balance), (Some(90_000), Some(90_000)));

    // 9E como lançado: Farmácia com 60 em novembro; 30 no Cartão e 70 no
    // Cartão 2 em outubro, sem dinheiro.
    r.cartao("cartao2");
    r.orc.atribuicoes.push(Atribuicao { categoria: "farmacia".into(), mes: dia(NOV, 1), valor: 60_000 });
    r.lanca("cartao", -30_000, Some("farmacia"));
    r.lanca("cartao2", -70_000, Some("farmacia"));
    let c = r.calcula();
    assert_eq!((c.conta("cartao"), c.conta("cartao2")), (-30_000, -70_000));
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-100_000, -100_000));
    assert_eq!(c.cat(OUT, "farmacia").credit_overspent(), -100_000, "amarelo");
    assert_eq!((c.disponivel(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao2")), (0, 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (580_000, 580_000), "60 atribuídos em novembro");
    assert_eq!(c.disponivel(NOV, "farmacia"), 60_000);
    assert_eq!(c.mes(dia(NOV, 1)).unwrap().sem_categoria.balance, Some(90_000));
}

#[test]
fn roteiro_9_segunda_tentativa_previsao() {
    // Segunda tentativa do roteiro 9 (análise 03), conferida na tela em
    // 2026-10-13/14, menos a divisão entre os cartões no 9E. Parte da limpeza: saldo inicial em 01/09,
    // 50 em Mercado em setembro e a entrada sem categoria do 9B.
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.transacao("t1").data = dia(SET, 1);
    r.cartao("cartao");
    r.cartao("cartao2");
    r.conta("investimento", TipoConta::Dinheiro, false);
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });
    let volta = r.transfere("investimento", "corrente", 100_000, None);
    data_das_pontas(&mut r, volta, dia(SET, 1));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (1_100_000, -100_000));
    assert_eq!((c.disponivel(SET, "mercado"), c.disponivel(OUT, "mercado")), (50_000, 50_000));
    let sem = |c: &Calculos, m| c.mes(dia(m, 1)).unwrap().sem_categoria.balance.unwrap();
    assert_eq!((sem(&c, SET), sem(&c, OUT), sem(&c, NOV)), (100_000, 100_000, 100_000));
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 950_000, 950_000));

    // 9A: 50 no Cartão em Mercado (15/09) e pagamento de 60 em 20/09: +10
    // no A atribuir de setembro; outubro herda o vermelho do Pagamento.
    let gasto = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto).data = dia(SET, 15);
    let pagamento = r.transfere("corrente", "cartao", 60_000, None);
    data_das_pontas(&mut r, pagamento, dia(SET, 20));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (1_040_000, 10_000));
    assert_eq!((c.atividade(SET, "mercado"), c.disponivel(SET, "mercado")), (-50_000, 0));
    assert_eq!((c.atividade(SET, "pagamento-cartao"), c.disponivel(SET, "pagamento-cartao")), (-10_000, -10_000));
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (960_000, 950_000, 950_000));

    // 9C (conferido na tela): atribui 10 a Mercado e gasta em outubro o
    // crédito do cartão: não desfaz o +10 de setembro, e o gasto não passa
    // para a categoria de pagamento (não vira dívida).
    r.atribui("mercado", 10_000);
    r.lanca("cartao", -10_000, Some("mercado"));
    let c = r.calcula();
    assert_eq!(c.conta("cartao"), 0);
    assert_eq!((c.atividade(OUT, "mercado"), c.disponivel(OUT, "mercado")), (-10_000, 0));
    assert_eq!((c.atividade(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao")), (0, 0));
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (950_000, 940_000, 940_000));
    assert_eq!(c.disponivel(NOV, "pagamento-cartao"), 0);
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(100_000));

    // 9D (roteiro 4E): 300 da Corrente para o Investimento, sem categoria. A
    // linha "sem categoria" de outubro vai de 100 a −200 e desconta em novembro.
    r.transfere("corrente", "investimento", 300_000, None);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("investimento")), (740_000, 200_000));
    assert_eq!((sem(&c, OUT), sem(&c, NOV)), (-200_000, 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (940_000, 740_000));

    // 9E: Farmácia com 60 em outubro gasta 30 no Cartão e 70 no Cartão 2.
    r.atribui("farmacia", 60_000);
    assert_eq!(r.calcula().a_atribuir(OUT), 880_000);
    r.lanca("cartao", -30_000, Some("farmacia"));
    assert_eq!(r.calcula().disponivel(OUT, "pagamento-cartao"), 30_000);
    r.lanca("cartao2", -70_000, Some("farmacia"));
    let c = r.calcula();
    assert_eq!((c.conta("cartao"), c.conta("cartao2")), (-30_000, -70_000));
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-100_000, -40_000));
    assert_eq!(c.cat(OUT, "farmacia").credit_overspent(), -40_000, "amarelo");
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (890_000, 880_000, 680_000));
    // A divisão dos 60 cobertos entre os cartões (10 e 50): ver
    // `roteiro_9e_divisao_entre_dois_cartoes`.
    let soma = c.disponivel(OUT, "pagamento-cartao") + c.disponivel(OUT, "pagamento-cartao2");
    assert_eq!(soma, 60_000);
}

#[test]
fn roteiro_9_segunda_tentativa_etapa_2_como_lancada() {
    // Conferido na tela em 2026-10-13. A1 certo (50 no Cartão em Mercado,
    // 15/09). A2 foi lançado como gasto comum: 50 da Corrente em Mercado
    // (20/09), sem transferência para o Cartão.
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.transacao("t1").data = dia(SET, 1);
    r.cartao("cartao");
    r.cartao("cartao2");
    r.conta("investimento", TipoConta::Dinheiro, false);
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });
    let volta = r.transfere("investimento", "corrente", 100_000, None);
    data_das_pontas(&mut r, volta, dia(SET, 1));

    // A1: o gasto coberto passa para o Pagamento, que vai para outubro.
    let gasto = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto).data = dia(SET, 15);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (1_100_000, -50_000));
    assert_eq!((c.atividade(SET, "mercado"), c.disponivel(SET, "mercado")), (-50_000, 0));
    assert_eq!((c.atividade(SET, "pagamento-cartao"), c.disponivel(SET, "pagamento-cartao")), (50_000, 50_000));
    assert_eq!((c.disponivel(OUT, "pagamento-cartao"), c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance), (50_000, Some(100_000)));
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT)), (950_000, 950_000));

    // A2 como lançado: mais 50 em Mercado, em dinheiro. O dinheiro da
    // categoria cobre primeiro o gasto em dinheiro; o do cartão fica sem
    // cobertura (amarelo) e sai do Pagamento.
    let saida = r.lanca("corrente", -50_000, Some("mercado"));
    r.transacao(&saida).data = dia(SET, 20);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao")), (1_050_000, -50_000));
    assert_eq!((c.atividade(SET, "mercado"), c.disponivel(SET, "mercado")), (-100_000, -50_000));
    let mercado = c.cat(SET, "mercado");
    assert_eq!((mercado.cash_overspent(), mercado.credit_overspent()), (0, -50_000), "amarelo, crédito");
    assert_eq!((c.atividade(SET, "pagamento-cartao"), c.disponivel(SET, "pagamento-cartao")), (0, 0));
    assert_eq!(c.disponivel(OUT, "pagamento-cartao"), 0);
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT)), (950_000, 950_000));
}

#[test]
fn roteiro_9e_divisao_entre_dois_cartoes() {
    // Na tela (2026-10-14): Cartão com +10 vindo de setembro e gasto de 10 em
    // Mercado em outubro (pagamento 0). Farmácia com 60 gasta 30 no Cartão
    // (pagamento do Cartão 30) e depois 70 no Cartão 2. Resultado: pagamento
    // do Cartão 10 e do Cartão 2 50 (os dois amarelos), Farmácia −40.
    // Explicado no roteiro 10: o crédito de 10 do Cartão paga 10 do gasto
    // de 30 na Farmácia (o maior do dia), que contam como dinheiro e são
    // cobertos primeiro; dos 50 que sobram para o crédito, o maior gasto
    // (70, Cartão 2) é coberto primeiro. Os 10 de Mercado viram dívida e
    // passam para o pagamento do Cartão.
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.transacao("t1").data = dia(SET, 1);
    r.cartao("cartao");
    r.cartao("cartao2");
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });
    let gasto = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto).data = dia(SET, 15);
    let pagamento = r.transfere("corrente", "cartao", 60_000, None);
    data_das_pontas(&mut r, pagamento, dia(SET, 20));
    r.atribui("mercado", 10_000);
    r.lanca("cartao", -10_000, Some("mercado"));
    r.atribui("farmacia", 60_000);
    r.lanca("cartao", -30_000, Some("farmacia"));
    r.lanca("cartao2", -70_000, Some("farmacia"));
    let c = r.calcula();
    assert_eq!((c.disponivel(OUT, "farmacia"), c.cat(OUT, "farmacia").credit_overspent()), (-40_000, -40_000));
    assert_eq!((c.disponivel(OUT, "pagamento-cartao"), c.disponivel(OUT, "pagamento-cartao2")), (10_000, 50_000));
    assert_eq!((c.disponivel(NOV, "pagamento-cartao"), c.disponivel(NOV, "pagamento-cartao2")), (10_000, 50_000));
}

/// Ponto de partida do roteiro 10: o fim do roteiro 9 sem os dois gastos
/// da Farmácia, e com o Cartão 3.
fn roteiro_10_partida() -> Roteiro {
    const SET: u32 = 9;
    let mut r = Roteiro::novo();
    r.transacao("t1").data = dia(SET, 1);
    r.cartao("cartao");
    r.cartao("cartao2");
    r.conta("investimento", TipoConta::Dinheiro, false);
    r.orc.atribuicoes.push(Atribuicao { categoria: "mercado".into(), mes: dia(SET, 1), valor: 50_000 });
    let volta = r.transfere("investimento", "corrente", 100_000, None);
    data_das_pontas(&mut r, volta, dia(SET, 1));
    let gasto = r.lanca("cartao", -50_000, Some("mercado"));
    r.transacao(&gasto).data = dia(SET, 15);
    let pagamento = r.transfere("corrente", "cartao", 60_000, None);
    data_das_pontas(&mut r, pagamento, dia(SET, 20));
    r.atribui("mercado", 10_000);
    r.lanca("cartao", -10_000, Some("mercado"));
    r.transfere("corrente", "investimento", 300_000, None);
    r.atribui("farmacia", 60_000);
    r.cartao("cartao3");
    r
}

/// Gasto de `valor` (< 0) em Farmácia em 14/10, como no roteiro 10.
fn farmacia_14(r: &mut Roteiro, cartao: &str, valor: i64) -> String {
    let id = r.lanca(cartao, valor, Some("farmacia"));
    r.transacao(&id).data = dia(OUT, 14);
    id
}

#[test]
fn roteiro_10_conferido() {
    // Conferido na tela em 2026-10-14 (análise 03, roteiro 10). Nenhuma das
    // quatro candidatas (proporção, estouro igual por cartão com e sem
    // limite, por transação) bateu: o maior gasto da categoria é coberto
    // primeiro. O 10E foi lançado sem categoria.
    const SET: u32 = 9;
    let pagamento = |c: &Calculos, m: u32, cartao: &str| c.disponivel(m, &format!("pagamento-{cartao}"));
    let mut r = roteiro_10_partida();
    let c = r.calcula();
    assert_eq!((c.conta("cartao"), c.conta("cartao2"), c.conta("cartao3")), (0, 0, 0));
    assert_eq!(c.disponivel(OUT, "farmacia"), 60_000);
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT), c.a_atribuir(NOV)), (890_000, 880_000, 680_000));

    // 10A: 10 no Cartão 2, depois 90 no Cartão 3: os 60 vão para o Cartão 3.
    let a1 = farmacia_14(&mut r, "cartao2", -10_000);
    let a2 = farmacia_14(&mut r, "cartao3", -90_000);
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-100_000, -40_000));
    assert_eq!(c.cat(OUT, "farmacia").credit_overspent(), -40_000, "amarelo");
    assert_eq!((pagamento(&c, OUT, "cartao2"), pagamento(&c, OUT, "cartao3")), (0, 60_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao2"), c.atividade(OUT, "pagamento-cartao3")), (0, 60_000));
    assert_eq!((pagamento(&c, NOV, "cartao2"), pagamento(&c, NOV, "cartao3")), (0, 60_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));

    // 10B: 60 no Cartão 3, depois 15 e 15 no Cartão 2: os 60 ficam no Cartão 3.
    r.remove(&a1);
    r.remove(&a2);
    let b1 = farmacia_14(&mut r, "cartao3", -60_000);
    let b2 = farmacia_14(&mut r, "cartao2", -15_000);
    let b3 = farmacia_14(&mut r, "cartao2", -15_000);
    let c = r.calcula();
    assert_eq!((c.conta("cartao2"), c.conta("cartao3")), (-30_000, -60_000));
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-90_000, -30_000));
    assert_eq!((pagamento(&c, OUT, "cartao2"), pagamento(&c, OUT, "cartao3")), (0, 60_000));
    assert_eq!((pagamento(&c, NOV, "cartao2"), pagamento(&c, NOV, "cartao3")), (0, 60_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));

    // 10C: 10 no Cartão 2, 50 no Cartão 3 e 60 no Cartão: os 60 vão para o
    // Cartão. O gasto de Mercado (13/10) já tinha levado o crédito de setembro.
    for t in [b1, b2, b3] {
        r.remove(&t);
    }
    let c1 = farmacia_14(&mut r, "cartao2", -10_000);
    let c2 = farmacia_14(&mut r, "cartao3", -50_000);
    let c3 = farmacia_14(&mut r, "cartao", -60_000);
    let c = r.calcula();
    assert_eq!((c.conta("cartao"), c.conta("cartao2"), c.conta("cartao3")), (-60_000, -10_000, -50_000));
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-120_000, -60_000));
    let pagamentos = (pagamento(&c, OUT, "cartao"), pagamento(&c, OUT, "cartao2"), pagamento(&c, OUT, "cartao3"));
    assert_eq!(pagamentos, (60_000, 0, 0));
    assert_eq!(pagamento(&c, NOV, "cartao"), 60_000);
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));

    // 10D: pagamento de 20 ao Cartão 3 em 20/09, sem dívida: +20 no A
    // atribuir de setembro, o vermelho desconta de outubro.
    for t in [c1, c2, c3] {
        r.remove(&t);
    }
    let p = r.transfere("corrente", "cartao3", 20_000, None);
    data_das_pontas(&mut r, p, dia(SET, 20));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao3")), (720_000, 20_000));
    assert_eq!((c.atividade(SET, "pagamento-cartao3"), pagamento(&c, SET, "cartao3")), (-20_000, -20_000));
    assert_eq!(pagamento(&c, OUT, "cartao3"), 0);
    assert_eq!((c.a_atribuir(SET), c.a_atribuir(OUT)), (910_000, 880_000));
    // Estorno de 10 na Farmácia em outubro: não sai do pagamento.
    let d1 = farmacia_14(&mut r, "cartao3", 10_000);
    let c = r.calcula();
    assert_eq!((c.conta("cartao3"), pagamento(&c, OUT, "cartao3")), (30_000, 0));

    // 10E como lançado: saída de 30 sem categoria no Cartão 3 (14/10). Os 20
    // de crédito de setembro pagam parte: estouram em dinheiro na linha "sem
    // categoria"; os 10 que viram dívida ficam no crédito.
    r.remove(&d1);
    let e1 = r.lanca("cartao3", -30_000, None);
    r.transacao(&e1).data = dia(OUT, 14);
    let c = r.calcula();
    assert_eq!((c.conta("cartao3"), pagamento(&c, OUT, "cartao3")), (-10_000, 0));
    // Pagamento de 30 em outubro: recria os +20, que vão para o A atribuir.
    let p = r.transfere("corrente", "cartao3", 30_000, None);
    data_das_pontas(&mut r, p, dia(OUT, 14));
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao3")), (690_000, 20_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao3"), pagamento(&c, OUT, "cartao3")), (-30_000, -30_000));
    let sem = &c.mes(dia(OUT, 1)).unwrap().sem_categoria;
    assert_eq!((sem.cash_outflows.unwrap() + sem.credit_outflows.unwrap(), sem.balance), (-330_000, Some(-230_000)));
    assert_eq!(c.disponivel(OUT, "farmacia"), 60_000);
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (900_000, 650_000));
    assert_eq!((pagamento(&c, NOV, "cartao3"), c.disponivel(NOV, "farmacia")), (0, 60_000));
}

#[test]
fn roteiro_11_conferido() {
    // Conferido na tela em 2026-10-14 (análise 03, roteiro 11), lançado
    // pelo navegador (11A a 11C) e pelo usuário (11D). Parte do fim do 10E
    // como lançado.
    const SET: u32 = 9;
    let pagamento = |c: &Calculos, m: u32, cartao: &str| c.disponivel(m, &format!("pagamento-{cartao}"));
    let mut r = roteiro_10_partida();
    let p = r.transfere("corrente", "cartao3", 20_000, None);
    data_das_pontas(&mut r, p, dia(SET, 20));
    let e1 = r.lanca("cartao3", -30_000, None);
    r.transacao(&e1).data = dia(OUT, 14);
    let p2 = r.transfere("corrente", "cartao3", 30_000, None);
    data_das_pontas(&mut r, p2.clone(), dia(OUT, 14));

    // 11A: o gasto de 30 do 10E recebe a categoria Farmácia. Os 20 pagos
    // com o crédito de setembro são dinheiro coberto; os 10 de dívida passam
    // para o pagamento, e o pagamento de 30 recria os +20 (A atribuir +20).
    r.transacao(&e1).categoria = Some("farmacia".into());
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-30_000, 30_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao3"), pagamento(&c, OUT, "cartao3")), (-20_000, -20_000));
    assert_eq!(c.mes(dia(OUT, 1)).unwrap().sem_categoria.balance, Some(-200_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (900_000, 680_000));
    assert_eq!((pagamento(&c, NOV, "cartao3"), c.disponivel(NOV, "farmacia")), (0, 30_000));

    // Limpeza: apaga o gasto e o pagamento de outubro.
    r.remove(&e1);
    r.remove(&p2.0);
    r.remove(&p2.1);
    let c = r.calcula();
    assert_eq!((c.conta("corrente"), c.conta("cartao3")), (720_000, 20_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));

    // 11B: 40 + 40 no Cartão 2 e 70 no Cartão. A maior transação (70) é
    // coberta primeiro, mesmo com o Cartão 2 somando mais (80).
    let b = [
        farmacia_14(&mut r, "cartao2", -40_000),
        farmacia_14(&mut r, "cartao2", -40_000),
        farmacia_14(&mut r, "cartao", -70_000),
    ];
    let c = r.calcula();
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-150_000, -90_000));
    assert_eq!((pagamento(&c, OUT, "cartao"), pagamento(&c, OUT, "cartao2")), (60_000, 0));
    assert_eq!((pagamento(&c, NOV, "cartao"), pagamento(&c, NOV, "cartao2")), (60_000, 0));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));

    // 11C: empate, 50 no Cartão 2 (lançado antes) e 50 no Cartão: o lançado
    // antes é coberto primeiro.
    for t in b {
        r.remove(&t);
    }
    let c1 = farmacia_14(&mut r, "cartao2", -50_000);
    let c2 = farmacia_14(&mut r, "cartao", -50_000);
    let c = r.calcula();
    assert_eq!(c.disponivel(OUT, "farmacia"), -40_000);
    assert_eq!((pagamento(&c, OUT, "cartao"), pagamento(&c, OUT, "cartao2")), (10_000, 50_000));
    assert_eq!((pagamento(&c, NOV, "cartao"), pagamento(&c, NOV, "cartao2")), (10_000, 50_000));

    // 11D: Cartão 3 com +20 de setembro. Mercado (sem dinheiro) gasta 10:
    // paga com o crédito, estoura em dinheiro (vermelho).
    r.remove(&c1);
    r.remove(&c2);
    let d1 = r.lanca("cartao3", -10_000, Some("mercado"));
    r.transacao(&d1).data = dia(OUT, 14);
    let c = r.calcula();
    assert_eq!((c.disponivel(OUT, "mercado"), c.cat(OUT, "mercado").cash_overspent()), (-10_000, -10_000), "vermelho");
    assert_eq!(pagamento(&c, OUT, "cartao3"), 0);
    // Depois, 30 na Farmácia no mesmo dia: a maior saída usa o crédito, e o
    // Mercado passa a estourar no crédito (amarelo).
    farmacia_14(&mut r, "cartao3", -30_000);
    let c = r.calcula();
    assert_eq!(c.conta("cartao3"), -20_000);
    let mercado = c.cat(OUT, "mercado");
    assert_eq!((mercado.balance, mercado.cash_overspent(), mercado.credit_overspent()), (Some(-10_000), 0, -10_000), "amarelo");
    assert_eq!((c.atividade(OUT, "farmacia"), c.disponivel(OUT, "farmacia")), (-30_000, 30_000));
    assert_eq!((c.atividade(OUT, "pagamento-cartao3"), pagamento(&c, OUT, "cartao3")), (10_000, 10_000));
    assert_eq!((c.a_atribuir(OUT), c.a_atribuir(NOV)), (880_000, 680_000));
    assert_eq!((pagamento(&c, NOV, "cartao3"), c.disponivel(NOV, "farmacia")), (10_000, 30_000));
}
