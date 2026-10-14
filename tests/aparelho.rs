//! O aparelho de linha de comando: leitura e escrita de valores e meses, e o
//! cálculo feito no próprio aparelho, sem servidor.

use tostao::aparelho::{formatar_valor, ler_mes, ler_valor, Aparelho, Destino, Plano, TipoDeConta};
use tostao::dates::Date;

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
