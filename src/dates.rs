//! Datas sem hora, com as mesmas regras do `DateWithoutTime` do YNAB (que usa
//! o moment.js): diferenças truncadas, soma de meses que para no último dia
//! do mês e dia da semana com domingo = 0.

use chrono::{Datelike, Days, Months, NaiveDate};

pub type Date = NaiveDate;

/// Dia da semana, domingo = 0 … sábado = 6.
pub fn day_of_week(d: Date) -> u32 {
    d.weekday().num_days_from_sunday()
}

pub fn start_of_month(d: Date) -> Date {
    d.with_day(1).expect("dia 1 sempre existe")
}

pub fn end_of_month(d: Date) -> Date {
    add_months(start_of_month(d), 1) - Days::new(1)
}

/// Soma meses; se o dia não existe no mês de destino, para no último dia.
pub fn add_months(d: Date, months: i64) -> Date {
    let n = Months::new(months.unsigned_abs() as u32);
    if months >= 0 { d + n } else { d - n }
}

pub fn add_days(d: Date, days: i64) -> Date {
    let n = Days::new(days.unsigned_abs());
    if days >= 0 { d + n } else { d - n }
}

pub fn same_month(a: Date, b: Date) -> bool {
    a.year() == b.year() && a.month() == b.month()
}

/// `a` é anterior ao mês de `b` (`isBeforeMonth`).
pub fn is_before_month(a: Date, b: Date) -> bool {
    a < start_of_month(b)
}

pub fn days_apart(a: Date, b: Date) -> i64 {
    (a - b).num_days().abs()
}

pub fn weeks_apart(a: Date, b: Date) -> i64 {
    days_apart(a, b) / 7
}

/// Meses inteiros entre as datas, truncado (como `moment.diff(..., "months")`).
pub fn months_apart(a: Date, b: Date) -> i64 {
    let (early, late) = if a <= b { (a, b) } else { (b, a) };
    let whole = (late.year() - early.year()) as i64 * 12 + late.month() as i64 - early.month() as i64;
    if late < add_months(early, whole) {
        whole - 1
    } else {
        whole
    }
}

/// A própria data, se ela cair em `weekday`, ou a próxima que cair.
pub fn next_or_same_day_of_week(d: Date, weekday: u32) -> Date {
    let ahead = (weekday as i64 - day_of_week(d) as i64).rem_euclid(7);
    add_days(d, ahead)
}

/// Quantas vezes `weekday` aparece de `d` até o fim do mês de `d`.
pub fn days_of_week_in_month(d: Date, weekday: u32) -> i64 {
    let first = next_or_same_day_of_week(d, weekday);
    let end = end_of_month(d);
    if first > end {
        return 0;
    }
    let mut last = next_or_same_day_of_week(end, weekday);
    if last > end {
        last = add_days(last, -7);
    }
    days_apart(first, last) / 7 + 1
}
