//! Números, importes y fechas en español (es-ES; fechas ISO «YYYY-MM-DD» en hora local).
use chrono::{Datelike, Duration, Local, NaiveDate};

/// Redondeo comercial a céntimos (mitad hacia arriba, simétrico en negativos).
/// Por debajo de esto, un importe es cero (y dos importes que se diferencian menos, iguales).
pub const DIF: f64 = 0.005;

pub fn r2(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    x.signum() * ((x.abs() * 100.0 + 1e-7).round()) / 100.0
}

/// 1234.5 → «1.234,50»
pub fn num(x: f64) -> String {
    let x = r2(x);
    let neg = x < 0.0;
    let cent = (x.abs() * 100.0).round() as u64;
    let ent = cent / 100;
    let dec = cent % 100;
    let mut s = String::new();
    let e = ent.to_string();
    for (i, c) in e.chars().enumerate() {
        if i > 0 && (e.len() - i).is_multiple_of(3) {
            s.push('.');
        }
        s.push(c);
    }
    format!("{}{},{:02}", if neg && cent > 0 { "-" } else { "" }, s, dec)
}

/// 1234.5 → «1.234,50 €»
pub fn eur(x: f64) -> String {
    format!("{} €", num(x))
}

/// 1234.5 → «1.235 €»
pub fn eur0(x: f64) -> String {
    let s = num(x.round());
    format!("{} €", s.trim_end_matches(",00"))
}

/// 21.0 → «21 %», 7.5 → «7,5 %»
pub fn pct(x: f64) -> String {
    let s = format!("{}", (x * 100.0).round() / 100.0).replace('.', ",");
    format!("{s} %")
}

/// Acepta «1.234,56», «1234,56», «1234.56», «1,234.56».
pub fn parse_num(v: &str) -> f64 {
    let mut s: String = v.trim().chars().filter(|c| !c.is_whitespace() && *c != '€').collect();
    if s.is_empty() {
        return 0.0;
    }
    let lc = s.rfind(',');
    let ld = s.rfind('.');
    match (lc, ld) {
        (Some(c), Some(d)) => {
            if c > d {
                s = s.replace('.', "").replacen(',', ".", 1);
            } else {
                s = s.replace(',', "");
            }
        }
        (Some(_), None) => s = s.replacen(',', ".", 1),
        (None, Some(_)) => {
            // «1.234» o «1.234.567»: miles
            let t = s.trim_start_matches('-');
            let partes: Vec<&str> = t.split('.').collect();
            if partes.len() > 1 && partes[0].len() <= 3 && !partes[0].is_empty() && partes[1..].iter().all(|p| p.len() == 3) {
                s = s.replace('.', "");
            }
        }
        _ => {}
    }
    s.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(0.0)
}

/// Lo escrito en un campo numérico opcional: vacío → None.
pub fn num_opcional(s: &str) -> Option<f64> {
    if s.trim().is_empty() { None } else { Some(parse_num(s)) }
}

/// ¿Es un número escrito válido (o vacío)?
pub fn es_num(v: &str) -> bool {
    let t = v.trim();
    t.is_empty() || t.chars().all(|c| c.is_ascii_digit() || ".,-€ ".contains(c)) && t.chars().any(|c| c.is_ascii_digit())
}

pub fn hoy() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn anio_actual() -> i32 {
    Local::now().year()
}

pub fn fecha(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// Lo que escribe la persona («2026-10-19», «19/10/2026», «19-10-2026», «19/10») → ISO, o None si no es una fecha.
pub fn leer_fecha(v: &str) -> Option<String> {
    let t = v.trim();
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some(d.format("%Y-%m-%d").to_string());
    }
    for f in ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            return Some(d.format("%Y-%m-%d").to_string());
        }
    }
    let p: Vec<&str> = t.split(['/', '-']).collect();
    if p.len() == 2
        && let (Ok(d), Ok(m)) = (p[0].parse::<u32>(), p[1].parse::<u32>()) {
            return NaiveDate::from_ymd_opt(anio_actual(), m, d).map(|x| x.format("%Y-%m-%d").to_string());
        }
    None
}

pub fn sumar_dias(s: &str, n: i64) -> String {
    match fecha(s) {
        Some(d) => (d + Duration::days(n)).format("%Y-%m-%d").to_string(),
        None => s.to_string(),
    }
}

/// Días de `a` a `b` (b − a).
pub fn dias_entre(a: &str, b: &str) -> i64 {
    match (fecha(a), fecha(b)) {
        (Some(x), Some(y)) => (y - x).num_days(),
        _ => 0,
    }
}

/// Ahora, en hora local: «2026-10-21T10:03:00». Es como se guarda cuándo se hizo algo (una consulta, una factura).
pub fn ahora() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// «2026-10-21» → «2026-10» (el mes de una fecha).
pub fn ym(s: &str) -> &str {
    s.get(..7).unwrap_or("")
}

/// «2026-10» + n meses (n puede ser negativo): «2026-10» + 3 → «2027-01».
pub fn mes_mas(ym: &str, n: i32) -> String {
    let t = anio_de(ym) * 12 + mes_de(ym) as i32 - 1 + n;
    format!("{}-{:02}", t.div_euclid(12), t.rem_euclid(12) + 1)
}

/// El último día de un mes: (2026, 2) → «2026-02-28».
pub fn fin_de_mes(anio: i32, m: u32) -> String {
    sumar_dias(&format!("{}-01", mes_mas(&format!("{anio}-{m:02}"), 1)), -1)
}

pub fn anio_de(s: &str) -> i32 {
    s.get(..4).and_then(|x| x.parse().ok()).unwrap_or(0)
}

pub fn mes_de(s: &str) -> u32 {
    s.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(0)
}

pub fn trimestre_de(s: &str) -> u32 {
    mes_de(s).div_ceil(3)
}

/// Primer y último día del trimestre.
pub fn periodo(anio: i32, q: u32) -> (String, String) {
    let ini = NaiveDate::from_ymd_opt(anio, (q - 1) * 3 + 1, 1).unwrap();
    let fin = if q == 4 {
        NaiveDate::from_ymd_opt(anio, 12, 31).unwrap()
    } else {
        NaiveDate::from_ymd_opt(anio, q * 3 + 1, 1).unwrap() - Duration::days(1)
    };
    (ini.format("%Y-%m-%d").to_string(), fin.format("%Y-%m-%d").to_string())
}

pub fn en_rango(s: &str, a: &str, b: &str) -> bool {
    s >= a && s <= b
}

pub const MESES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
pub const MESES_C: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

/// «2025-12» → «diciembre de 2025» (tal cual si no es un mes).
pub fn nombre_mes(ym: &str) -> String {
    match mes_de(ym) {
        m @ 1..=12 => format!("{} de {}", MESES[m as usize - 1], anio_de(ym)),
        _ => ym.to_string(),
    }
}

/// «2026-08-31» → «31 ago 2026»
pub fn fecha_corta(s: &str) -> String {
    match fecha(s) {
        Some(d) => format!("{} {} {}", d.day(), MESES_C[d.month0() as usize], d.year()),
        None => s.to_string(),
    }
}

/// «2026-08-31» → «31/08/2026»
pub fn dmy(s: &str) -> String {
    match fecha(s) {
        Some(d) => d.format("%d/%m/%Y").to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meses() {
        assert_eq!(fin_de_mes(2026, 2), "2026-02-28");
        assert_eq!(fin_de_mes(2024, 2), "2024-02-29");
        assert_eq!(fin_de_mes(2026, 12), "2026-12-31");
        assert_eq!((mes_mas("2026-10", 3), mes_mas("2026-01", -1), mes_mas("2026-03", -7)), ("2027-01".into(), "2025-12".into(), "2025-08".into()));
        assert_eq!((ym("2026-10-21"), ym("26")), ("2026-10", ""));
        assert_eq!(nombre_mes("2026-10"), "octubre de 2026");
        let a = ahora();
        assert_eq!((a.len(), &a[10..11]), (19, "T"), "{a}");
    }
    #[test]
    fn numeros() {
        assert_eq!(num(1234.5), "1.234,50");
        assert_eq!(num(-0.004), "0,00");
        assert_eq!(num(1234567.891), "1.234.567,89");
        assert_eq!(r2(1.005), 1.01);
        assert_eq!(r2(-1.005), -1.01);
        assert_eq!(parse_num("1.234,56"), 1234.56);
        assert_eq!(parse_num("1,234.56"), 1234.56);
        assert_eq!(parse_num("1234.56"), 1234.56);
        assert_eq!(parse_num("1.234"), 1234.0);
        assert_eq!(parse_num("12,5 €"), 12.5);
        assert_eq!(eur0(1234.5), "1.235 €");
        assert_eq!(pct(7.5), "7,5 %");
    }
    #[test]
    fn fechas() {
        assert_eq!(periodo(2026, 1), ("2026-01-01".into(), "2026-03-31".into()));
        assert_eq!(periodo(2024, 1).1, "2024-03-31");
        assert_eq!(periodo(2026, 4).1, "2026-12-31");
        assert_eq!(leer_fecha("19/10/2026").as_deref(), Some("2026-10-19"));
        assert_eq!(fecha_corta("2026-08-31"), "31 ago 2026");
        assert_eq!(sumar_dias("2026-12-31", 1), "2027-01-01");
        assert_eq!(trimestre_de("2026-08-31"), 3);
    }
}
