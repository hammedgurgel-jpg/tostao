//! O aparelho de linha de comando: leitura e escrita de valores e meses, e o
//! cálculo feito no próprio aparelho, sem servidor.

use std::future::Future;
use std::task::{Context, Waker};

use futures_executor::block_on;
use tostao::aparelho::{formatar_valor, ler_mes, ler_valor, Aparelho, Destino, Estouro, Plano, TipoDeConta};
use tostao::api::{erro_do_servidor, ApiError, HttpResponse, Transport};
use tostao::dates::Date;
use tostao::protocol::{CatalogRequest, TOSTAO_SCHEMA_VERSION};

#[test]
fn le_valores_em_reais() {
    assert_eq!(ler_valor("80").unwrap(), 80_000);
    assert_eq!(ler_valor("-80").unwrap(), -80_000);
    assert_eq!(ler_valor("80,5").unwrap(), 80_500);
    assert_eq!(ler_valor("80.50").unwrap(), 80_500);
    assert_eq!(ler_valor("1.000").unwrap(), 1_000_000);
    assert_eq!(ler_valor("-1.234,56").unwrap(), -1_234_560);
    assert_eq!(ler_valor("1.000.000").unwrap(), 1_000_000_000);
    for invalido in ["", "abc", "1,2,3", "1.00.0", "80,1234", "--5"] {
        assert!(ler_valor(invalido).is_err(), "{invalido:?}");
    }
}

#[test]
fn formata_como_a_tela() {
    assert_eq!(formatar_valor(950_000), "950,00");
    assert_eq!(formatar_valor(1_000_000), "1.000,00");
    assert_eq!(formatar_valor(-1_234_560), "-1.234,56");
    assert_eq!(formatar_valor(-30_500), "-30,50");
    assert_eq!(formatar_valor(0), "0,00");
}

#[test]
fn le_mes() {
    let outubro = Date::from_ymd_opt(2026, 10, 1).unwrap();
    assert_eq!(ler_mes("2026-10").unwrap(), outubro);
    assert_eq!(ler_mes("2026-10-13").unwrap(), outubro);
    assert!(ler_mes("10/2026").is_err());
}

fn linha(plano: &Plano, nome: &str) -> (i64, i64, i64) {
    let l = plano
        .grupos
        .iter()
        .flat_map(|g| &g.categorias)
        .chain(&plano.sem_categoria)
        .find(|l| l.nome == nome)
        .unwrap_or_else(|| panic!("sem \"{nome}\" no plano: {plano:?}"));
    (l.atribuido, l.atividade, l.disponivel)
}

/// Roteiro 1 da análise 01 (seção 2) num aparelho só, sem servidor: o plano
/// e as contas saem do cálculo local, com as mudanças não enviadas.
#[test]
fn roteiro_1_sem_servidor() {
    let pasta = std::env::temp_dir().join(format!("tostao-aparelho-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let arquivo = pasta.join("a.json");
    let dia = Date::from_ymd_opt(2026, 10, 13).unwrap();
    let (outubro, novembro) = (Date::from_ymd_opt(2026, 10, 1).unwrap(), Date::from_ymd_opt(2026, 11, 1).unwrap());
    let categoria = |nome: &str| Destino::Categoria(nome.into());

    let mut a = Aparelho::criar_orcamento(&arquivo, "Família").unwrap();
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    a.criar_conta("Cartão", TipoDeConta::Cartao).unwrap();
    a.criar_grupo("Dia a dia").unwrap();
    a.criar_categoria("Mercado", "Dia a dia").unwrap();
    a.criar_categoria("Lazer", "Dia a dia").unwrap();
    a.lancar("Corrente", dia, 1_000_000, &categoria("A atribuir")).unwrap();
    a.salvar().unwrap();
    assert_eq!(a.plano(outubro).unwrap().a_atribuir, Some(1_000_000));

    // 1A: atribui 50 a Mercado e gasta 80 na corrente.
    let mut a = Aparelho::abrir(&arquivo).unwrap();
    a.atribuir("Mercado", outubro, 50_000).unwrap();
    a.lancar("Corrente", dia, -80_000, &categoria("Mercado")).unwrap();
    let plano = a.plano(outubro).unwrap();
    assert_eq!(linha(&plano, "Mercado"), (50_000, -80_000, -30_000));
    assert_eq!(plano.a_atribuir, Some(950_000));
    assert_eq!(a.plano(novembro).unwrap().a_atribuir, Some(920_000));

    // 1B, 1C e 1D: 40 na corrente sem categoria; 60 no cartão em Lazer; 25
    // no cartão sem categoria.
    a.lancar("Corrente", dia, -40_000, &Destino::SemCategoria).unwrap();
    a.lancar("Cartão", dia, -60_000, &categoria("Lazer")).unwrap();
    a.lancar("Cartão", dia, -25_000, &Destino::SemCategoria).unwrap();
    a.salvar().unwrap();

    let a = Aparelho::abrir(&arquivo).unwrap();
    assert!(a.mudancas_pendentes() > 0, "nada foi enviado a servidor nenhum");
    let plano = a.plano(outubro).unwrap();
    assert_eq!(plano.a_atribuir, Some(950_000), "A atribuir de outubro na tela");
    assert_eq!(linha(&plano, "Mercado"), (50_000, -80_000, -30_000));
    assert_eq!(linha(&plano, "Lazer"), (0, -60_000, -60_000));
    assert_eq!(linha(&plano, "Sem categoria"), (0, -65_000, -65_000));
    let plano = a.plano(novembro).unwrap();
    assert_eq!(plano.a_atribuir, Some(880_000), "A atribuir de novembro na tela");
    assert_eq!(linha(&plano, "Mercado"), (0, 0, 0), "o negativo não passa adiante");

    let contas = a.contas().unwrap();
    let saldo = |nome: &str| contas.iter().find(|c| c.nome == nome).map(|c| c.saldo);
    assert_eq!(saldo("Corrente"), Some(880_000));
    assert_eq!(saldo("Cartão"), Some(-85_000));

    let _ = std::fs::remove_dir_all(&pasta);
}

/// Transporte que não alcança servidor nenhum.
struct SemRede;

impl Transport for SemRede {
    async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        Err(ApiError::Transport("sem rede neste teste".into()))
    }
}

/// O token que a pessoa digita vale só para a chamada: não entra na cópia
/// do aparelho (que vai para o IndexedDB ou para o arquivo).
#[test]
fn o_token_nao_entra_na_copia_do_aparelho() {
    let mut a = Aparelho::novo_orcamento("Família");
    let erro = block_on(a.sincronizar(SemRede, "token-secreto-123")).unwrap_err();
    assert!(erro.to_string().contains("não deu para falar com o servidor"), "{erro}");
    assert!(!a.para_json().unwrap().contains("token-secreto-123"));
}

/// Falha de rede é reconhecida pelo tipo do erro, não pelo texto da mensagem.
#[test]
fn falha_de_rede_se_reconhece_pelo_tipo() {
    let mut a = Aparelho::novo_orcamento("Família");
    let erro = block_on(a.sincronizar(SemRede, "token")).unwrap_err();
    assert!(erro.e_falha_de_rede(), "{erro}");
}

/// Resposta do servidor (mesmo recusando o token) não é falha de rede.
#[test]
fn resposta_do_servidor_nao_e_falha_de_rede() {
    struct Recusa;
    impl Transport for Recusa {
        async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
            Err(ApiError::Server(erro_do_servidor(401, r#"{"error":{"id":"unauthorized"}}"#)))
        }
    }
    let mut a = Aparelho::novo_orcamento("Família");
    let erro = block_on(a.sincronizar(Recusa, "token")).unwrap_err();
    assert!(!erro.e_falha_de_rede(), "{erro}");
}

/// Resposta que chegou mas não é protocolo (corpo que não é JSON) não é
/// falha de rede: o servidor foi alcançado.
#[test]
fn resposta_fora_do_protocolo_nao_e_falha_de_rede() {
    struct Corpo;
    impl Transport for Corpo {
        async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
            Err(ApiError::Protocolo("expected value at line 1 column 1".into()))
        }
    }
    let mut a = Aparelho::novo_orcamento("Família");
    let erro = block_on(a.sincronizar(Corpo, "token")).unwrap_err();
    assert!(!erro.e_falha_de_rede(), "{erro}");
}

/// Transporte que responde uma sincronização válida, sem entidades do servidor.
struct Responde;

impl Transport for Responde {
    async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        Ok(resposta_valida())
    }
}

fn resposta_valida() -> HttpResponse {
    let data = serde_json::json!({
        "changed_entities": {},
        "current_server_knowledge": 5,
        "error": null,
        "schema_version_of_response": TOSTAO_SCHEMA_VERSION,
        "schema_version_of_server": TOSTAO_SCHEMA_VERSION,
        "server_knowledge_of_device": 0
    });
    HttpResponse { headers: vec![("X-Tostao-Server-Version".into(), "9.9.9".into())], data }
}

/// O caminho com sucesso também não põe o token na cópia do aparelho.
#[test]
fn sincronizacao_com_sucesso_nao_grava_o_token_na_copia() {
    let mut a = Aparelho::novo_orcamento("Família");
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    let resultado = block_on(a.sincronizar(Responde, "token-secreto-456")).unwrap();
    assert_eq!(resultado.server_app_version.as_deref(), Some("9.9.9"));
    assert!(!a.para_json().unwrap().contains("token-secreto-456"));
}

/// Transporte que trava na primeira chamada (a future fica pendente para
/// sempre) e responde nas seguintes.
struct TravadoNaPrimeira {
    chamadas: usize,
}

impl Transport for TravadoNaPrimeira {
    async fn post(&mut self, _: &str, _: &CatalogRequest) -> Result<HttpResponse, ApiError> {
        self.chamadas += 1;
        if self.chamadas == 1 {
            std::future::pending::<()>().await;
        }
        Ok(resposta_valida())
    }
}

/// Sincronização cancelada no meio do `await` (a future é descartada): o
/// aparelho continua com o orçamento e as mudanças pendentes, e a próxima
/// sincronização funciona.
#[test]
fn sincronizacao_cancelada_no_meio_nao_esvazia_o_aparelho() {
    let mut a = Aparelho::novo_orcamento("Família");
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    let pendentes = a.mudancas_pendentes();
    assert!(pendentes > 0);
    {
        let mut futura = std::pin::pin!(a.sincronizar(TravadoNaPrimeira { chamadas: 0 }, "token"));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(futura.as_mut().poll(&mut cx).is_pending());
    }

    assert!(!a.orcamento().is_empty(), "a cópia saiu da memória durante a espera");
    assert_eq!(a.mudancas_pendentes(), pendentes);
    let contas = a.contas().unwrap();
    assert!(contas.iter().any(|c| c.nome == "Corrente"), "o orçamento sumiu: {contas:?}");

    let resultado = block_on(a.sincronizar(Responde, "token")).unwrap();
    assert!(!resultado.sync_cancelled_because_another_in_flight);
    let contas = a.contas().unwrap();
    assert!(contas.iter().any(|c| c.nome == "Corrente"), "a sincronização seguinte perdeu o orçamento");
}

fn linha_completa<'a>(plano: &'a Plano, nome: &str) -> &'a tostao::aparelho::LinhaDoPlano {
    plano
        .grupos
        .iter()
        .flat_map(|g| &g.categorias)
        .chain(&plano.sem_categoria)
        .find(|l| l.nome == nome)
        .unwrap_or_else(|| panic!("sem \"{nome}\" no plano: {plano:?}"))
}

/// O plano diz o tipo do estouro (dinheiro ou crédito) e a falta da fatura;
/// cobrir o estouro e atribuir à categoria de pagamento zera as duas.
#[test]
fn estouro_falta_e_movimentacao_no_plano() {
    let dia = Date::from_ymd_opt(2026, 10, 13).unwrap();
    let outubro = Date::from_ymd_opt(2026, 10, 1).unwrap();
    let categoria = |nome: &str| Destino::Categoria(nome.into());

    let mut a = Aparelho::novo_orcamento("Família");
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    a.criar_conta("Cartão", TipoDeConta::Cartao).unwrap();
    a.criar_grupo("Dia a dia").unwrap();
    a.criar_categoria("Mercado", "Dia a dia").unwrap();
    a.criar_categoria("Lazer", "Dia a dia").unwrap();
    a.lancar("Corrente", dia, 1_000_000, &categoria("A atribuir")).unwrap();
    a.atribuir("Mercado", outubro, 50_000).unwrap();
    a.lancar("Corrente", dia, -80_000, &categoria("Mercado")).unwrap();
    a.lancar("Cartão", dia, -60_000, &categoria("Lazer")).unwrap();

    let plano = a.plano(outubro).unwrap();
    assert_eq!(linha_completa(&plano, "Mercado").estouro, Some(Estouro::Dinheiro), "estouro em dinheiro");
    assert_eq!(linha_completa(&plano, "Lazer").estouro, Some(Estouro::Credito), "estouro no crédito");
    let pagamento = linha_completa(&plano, "Cartão");
    assert!(pagamento.pagamento);
    assert_eq!(pagamento.falta, 60_000, "a fatura de 60 não tem dinheiro reservado");
    assert_eq!(pagamento.estouro, None);

    // Cobre o estouro de Lazer tirando do A atribuir, e reserva a fatura.
    a.movimentar("A atribuir", "Lazer", outubro, 60_000).unwrap();
    a.movimentar("A atribuir", "Cartão", outubro, 60_000).unwrap();
    let plano = a.plano(outubro).unwrap();
    assert_eq!(linha_completa(&plano, "Lazer").disponivel, 0, "coberto: disponível 0");
    assert_eq!(linha_completa(&plano, "Lazer").estouro, None);
    assert_eq!(linha_completa(&plano, "Cartão").falta, 0, "fatura coberta");
    assert_eq!(plano.a_atribuir, Some(1_000_000 - 50_000 - 60_000 - 60_000), "A atribuir depois de atribuir e cobrir");

    // Mover de outra categoria: Mercado cede 10 para Lazer.
    a.movimentar("Mercado", "Lazer", outubro, 10_000).unwrap();
    let plano = a.plano(outubro).unwrap();
    assert_eq!(linha(&plano, "Mercado").0, 40_000, "atribuído de Mercado diminui");
    assert_eq!(linha(&plano, "Lazer").0, 70_000, "atribuído de Lazer aumenta");
    assert_eq!(plano.a_atribuir, Some(1_000_000 - 50_000 - 60_000 - 60_000), "mover entre categorias não mexe no A atribuir");

    assert!(a.movimentar("Lazer", "Lazer", outubro, 1_000).is_err(), "origem e destino iguais");
    assert!(a.movimentar("Lazer", "A atribuir", outubro, 1_000).is_err(), "não se atribui a A atribuir");
}

/// Falta da fatura: com a fatura paga por transferência e o disponível da
/// categoria de pagamento negativo, a falta é zero (a dívida do cartão é zero).
#[test]
fn falta_nao_soma_disponivel_negativo_da_categoria_de_pagamento() {
    let dia = Date::from_ymd_opt(2026, 10, 13).unwrap();
    let outubro = Date::from_ymd_opt(2026, 10, 1).unwrap();
    let mut a = Aparelho::novo_orcamento("Família");
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    a.criar_conta("Cartão", TipoDeConta::Cartao).unwrap();
    a.lancar("Corrente", dia, 1_000_000, &Destino::Categoria("A atribuir".into())).unwrap();
    a.lancar("Cartão", dia, -25_000, &Destino::SemCategoria).unwrap();
    a.lancar("Corrente", dia, -25_000, &Destino::Transferencia("Cartão".into())).unwrap();

    let plano = a.plano(outubro).unwrap();
    let pagamento = linha_completa(&plano, "Cartão");
    assert_eq!(a.contas().unwrap().iter().find(|c| c.nome == "Cartão").unwrap().saldo, 0);
    assert_eq!(pagamento.falta, 0, "fatura paga: nada falta");
}
