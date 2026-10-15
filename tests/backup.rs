//! Backup do orçamento (ADR 0002): ida e volta e as recusas, pelo núcleo.

use serde_json::{json, Value};
use tostao::aparelho::backup::{resumo_do_backup, FORMATO};
use tostao::aparelho::{Aparelho, Destino, Plano, TipoDeConta};
use tostao::dates::Date;

const CRIADO_EM: &str = "2026-10-15T12:00:00.000Z";

fn dia() -> Date {
    Date::from_ymd_opt(2026, 10, 13).unwrap()
}

fn outubro() -> Date {
    Date::from_ymd_opt(2026, 10, 1).unwrap()
}

fn novembro() -> Date {
    Date::from_ymd_opt(2026, 11, 1).unwrap()
}

/// Roteiro 1 da análise 01 (seção 2), com uma transferência para a fatura do
/// cartão, para que a categoria de pagamento tenha movimento.
fn roteiro_1() -> Aparelho {
    let categoria = |nome: &str| Destino::Categoria(nome.into());
    let mut a = Aparelho::novo_orcamento("Família");
    a.criar_conta("Corrente", TipoDeConta::Corrente).unwrap();
    a.criar_conta("Cartão", TipoDeConta::Cartao).unwrap();
    a.criar_conta("Reserva", TipoDeConta::Poupanca).unwrap();
    a.criar_grupo("Dia a dia").unwrap();
    a.criar_categoria("Mercado", "Dia a dia").unwrap();
    a.criar_categoria("Lazer", "Dia a dia").unwrap();
    a.lancar("Corrente", dia(), 1_000_000, &categoria("A atribuir")).unwrap();
    a.atribuir("Mercado", outubro(), 50_000).unwrap();
    a.lancar("Corrente", dia(), -80_000, &categoria("Mercado")).unwrap();
    a.lancar("Corrente", dia(), -40_000, &Destino::SemCategoria).unwrap();
    a.lancar("Cartão", dia(), -60_000, &categoria("Lazer")).unwrap();
    a.lancar("Cartão", dia(), -25_000, &Destino::SemCategoria).unwrap();
    a.lancar("Corrente", dia(), -100_000, &Destino::Transferencia("Cartão".into())).unwrap();
    a.movimentar("Mercado", "Lazer", outubro(), 10_000).unwrap();
    a
}

fn plano_e_contas(a: &Aparelho) -> (Plano, Plano, Vec<tostao::aparelho::LinhaDeConta>) {
    (a.plano(outubro()).unwrap(), a.plano(novembro()).unwrap(), a.contas().unwrap())
}

/// O backup como JSON, para editar nas recusas.
fn backup_de(a: &Aparelho) -> Value {
    serde_json::from_str(&a.exportar_backup(CRIADO_EM, "2026-10").unwrap()).unwrap()
}

fn importar_json(a: &Aparelho, valor: &Value) -> Result<Aparelho, tostao::aparelho::Erro> {
    a.importar_backup(&valor.to_string())
}

#[test]
fn exportar_e_importar_devolve_o_mesmo_plano_e_os_mesmos_saldos() {
    let original = roteiro_1();
    let arquivo = original.exportar_backup(CRIADO_EM, "2026-10").unwrap();

    // Um navegador limpo: um orçamento novo, sem nada, recebe o backup.
    let limpo = Aparelho::novo_orcamento("Família");
    let importado = limpo.importar_backup(&arquivo).unwrap();

    assert_eq!(plano_e_contas(&importado), plano_e_contas(&original));
    assert!(original.plano(outubro()).unwrap().a_atribuir.is_some(), "o roteiro tem A atribuir de outubro");
}

#[test]
fn a_importacao_substitui_o_orcamento_inteiro() {
    let mut destino = Aparelho::novo_orcamento("Família");
    destino.criar_conta("Antiga", TipoDeConta::Dinheiro).unwrap();
    destino.criar_grupo("Velho").unwrap();
    destino.criar_categoria("Sobra", "Velho").unwrap();

    let importado = destino.importar_backup(&roteiro_1().exportar_backup(CRIADO_EM, "2026-10").unwrap()).unwrap();

    let contas: Vec<String> = importado.contas().unwrap().into_iter().map(|c| c.nome).collect();
    assert!(!contas.contains(&"Antiga".to_string()), "a conta antiga ficou: {contas:?}");
    let plano = importado.plano(outubro()).unwrap();
    assert!(plano.grupos.iter().all(|g| g.nome != "Velho"), "o grupo antigo ficou");
    assert!(plano.grupos.iter().flat_map(|g| &g.categorias).all(|c| c.nome != "Sobra"), "a categoria antiga ficou");
}

#[test]
fn o_arquivo_traz_so_entidades_em_inteiros_de_milesimos() {
    let texto = roteiro_1().exportar_backup(CRIADO_EM, "2026-10").unwrap();
    let v: Value = serde_json::from_str(&texto).unwrap();

    assert_eq!(v["formato"], FORMATO);
    assert_eq!(v["versao_de_esquema"], 1);
    assert_eq!(v["criado_em"], CRIADO_EM);
    assert_eq!(v["ultimo_mes"], "2026-10");
    assert_eq!(v["casas_decimais"], 2);
    for lista in ["contas", "grupos", "categorias", "atribuicoes", "transacoes"] {
        assert!(v[lista].is_array(), "{lista} não é lista");
    }
    assert_eq!(v["contas"].as_array().unwrap().len(), 3);
    assert!(v["transacoes"].as_array().unwrap().iter().any(|t| t["valor"] == json!(-80_000)));
    // Mercado ficou com 40 depois de mover 10 para Lazer.
    assert!(v["atribuicoes"].as_array().unwrap().iter().any(|a| a["valor"] == json!(40_000)));
    assert!(v["atribuicoes"].as_array().unwrap().iter().any(|a| a["valor"] == json!(10_000)));
    // Sem cálculos nem campos de sincronização, e sem o grupo interno.
    for proibido in ["conhecimento", "is_tombstone", "be_", "grupo-interno"] {
        assert!(!texto.contains(proibido), "o arquivo tem {proibido:?}");
    }
    assert!(!texto.contains("\"disponivel\"") && !texto.contains("\"saldo\""));
}

#[test]
fn o_resumo_mostra_data_e_contagens_sem_importar() {
    let resumo = resumo_do_backup(&roteiro_1().exportar_backup(CRIADO_EM, "2026-10").unwrap()).unwrap();
    assert_eq!(resumo.criado_em, CRIADO_EM);
    assert_eq!(resumo.ultimo_mes, "2026-10");
    // Grupos: "Dia a dia" e o grupo de pagamentos do cartão, criado com ele.
    assert_eq!((resumo.contas, resumo.grupos, resumo.categorias), (3, 2, 3));
    assert_eq!(resumo.atribuicoes, 2, "Mercado e Lazer em outubro, depois de mover 10");
    assert_eq!(resumo.transacoes, 7, "as duas pontas da transferência entram");
}

#[test]
fn exportar_exige_um_mes_valido() {
    assert!(roteiro_1().exportar_backup(CRIADO_EM, "outubro").is_err());
}

#[test]
fn transferencia_sobrevive_a_ida_e_volta() {
    let original = roteiro_1();
    let importado = Aparelho::novo_orcamento("Família")
        .importar_backup(&original.exportar_backup(CRIADO_EM, "2026-10").unwrap())
        .unwrap();
    // Reexportar o importado dá o mesmo arquivo (a ordem é estável).
    assert_eq!(backup_de(&importado), backup_de(&original));
}

#[test]
fn recusa_json_quebrado_sem_mexer_no_orcamento() {
    let a = roteiro_1();
    let antes = plano_e_contas(&a);
    assert!(a.importar_backup("{ isto não é json").is_err());
    assert!(a.importar_backup("").is_err());
    assert_eq!(plano_e_contas(&a), antes);
}

#[test]
fn recusa_formato_errado() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    v["formato"] = json!("outra-coisa");
    let erro = importar_json(&a, &v).unwrap_err().to_string();
    assert!(erro.contains("formato"), "{erro}");

    let mut v = backup_de(&a);
    v.as_object_mut().unwrap().remove("formato");
    assert!(importar_json(&a, &v).is_err());
}

#[test]
fn recusa_versao_de_esquema_desconhecida() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    v["versao_de_esquema"] = json!(99);
    let erro = importar_json(&a, &v).unwrap_err().to_string();
    assert!(erro.contains("versão"), "{erro}");

    let mut v = backup_de(&a);
    v.as_object_mut().unwrap().remove("versao_de_esquema");
    assert!(importar_json(&a, &v).is_err());
}

#[test]
fn recusa_lista_faltando_ou_com_tipo_errado() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    v.as_object_mut().unwrap().remove("transacoes");
    assert!(importar_json(&a, &v).is_err());

    let mut v = backup_de(&a);
    v["transacoes"][0]["valor"] = json!(1.5);
    assert!(importar_json(&a, &v).is_err(), "valor fracionado não é milésimo inteiro");
}

#[test]
fn recusa_referencia_quebrada() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    v["transacoes"][0]["conta_id"] = json!("conta-que-nao-existe");
    assert!(importar_json(&a, &v).is_err());

    let mut v = backup_de(&a);
    v["atribuicoes"][0]["categoria_id"] = json!("categoria-que-nao-existe");
    assert!(importar_json(&a, &v).is_err());
}

#[test]
fn recusa_ponta_de_transferencia_sem_espelho() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    let transferencia = v["transacoes"].as_array().unwrap().iter().position(|t| t.get("transferencia_id").is_some()).unwrap();
    v["transacoes"][transferencia]["transferencia_id"] = json!("nao-existe");
    assert!(importar_json(&a, &v).is_err());
}

#[test]
fn recusa_mes_e_data_invalidos_e_casas_diferentes() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    v["ultimo_mes"] = json!("2026-13");
    assert!(importar_json(&a, &v).is_err());

    let mut v = backup_de(&a);
    v["transacoes"][0]["data"] = json!("13/10/2026");
    assert!(importar_json(&a, &v).is_err());

    let mut v = backup_de(&a);
    v["casas_decimais"] = json!(3);
    assert!(importar_json(&a, &v).is_err());
}

#[test]
fn recusa_nome_repetido_e_nome_reservado() {
    let a = roteiro_1();
    let mut v = backup_de(&a);
    let primeiro = v["contas"][0]["nome"].clone();
    v["contas"][1]["nome"] = primeiro;
    assert!(importar_json(&a, &v).is_err(), "nomes de conta repetidos");

    let mut v = backup_de(&a);
    v["categorias"][0]["nome"] = json!("a atribuir");
    assert!(importar_json(&a, &v).is_err(), "nome reservado de A atribuir");
}
