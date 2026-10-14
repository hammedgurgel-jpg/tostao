//! Leitura e escrita de valores e meses no aparelho de linha de comando.

use tostao::aparelho::{formatar_valor, ler_mes, ler_valor};
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
