//! Lo que enseña la pantalla de cuotas, ya en texto: las filas de la tabla de meses, las del historial, las cifras
//! de arriba y las notas. Se calcula una vez (al entrar o al cambiar los datos o el año), no en cada dibujo.
use crate::dominio::fmt::DIF;
use crate::dominio::tgss::Tgss;
use crate::dominio::fmt::{self, eur, eur0};
use crate::dominio::tgss::Recibo;
use crate::informe::autonomo::{self, CuotasAnio, EstadoMes};
use crate::tui::impuestos_trimestre::{dia_mes, dm, y_lista};
use crate::tui::widgets::tema;
use ratatui::style::{Modifier, Style};


/// Columnas de la tabla de meses (el desglose por conceptos, al abrir un recibo).
pub const CABECERAS: [&str; 3] = ["Mes", "Importe", "Cargo"];

/// Una fila de la tabla de meses.
pub struct Fila {
    pub celdas: [String; 3],
    pub estilo: Style,
    /// El estilo de la primera celda (los atrasos van en cursiva).
    pub estilo_mes: Style,
    /// El recibo de la fila, si lo hay (para su desglose).
    pub recibo: Option<Recibo>,
}

/// Una cifra de arriba: etiqueta, valor, nota y estilo del valor.
pub struct Cifra {
    pub etiqueta: String,
    pub corta: String,
    pub valor: String,
    pub nota: String,
    pub estilo: Style,
}

/// Una nota de debajo de la tabla.
pub struct Nota {
    pub texto: String,
    pub estilo: Style,
}

/// «2025-09» → «sep 25».
fn mes_corto(ym: &str) -> String {
    let m = fmt::mes_de(&format!("{ym}-01"));
    match (m, ym.get(2..4)) {
        (1..=12, Some(a)) => format!("{} {a}", fmt::MESES_C[m as usize - 1]),
        _ => ym.to_string(),
    }
}

/// Lo que se pone en la columna del mes para un recibo que no es la cuota normal.
fn etiqueta_otro(r: &Recibo) -> String {
    if r.atrasos() && r.periodo.len() >= 7 {
        format!("+ {}", mes_corto(&r.periodo))
    } else {
        let p: String = r.concepto.split_whitespace().next().unwrap_or("otro").to_lowercase().chars().take(8).collect();
        format!("+ {p}")
    }
}

fn cargo(r: &Recibo) -> String {
    let c = r.fecha_cargo();
    if fmt::fecha(c).is_some() { dm(c) } else { c.to_string() }
}

/// Las filas de la tabla de meses (sin la cabecera ni el total), a fecha de `hoy`.
pub fn filas(c: &CuotasAnio, hoy: &str) -> Vec<Fila> {
    let mut v = Vec::new();
    for m in &c.meses {
        let mes = fmt::MESES_C[m.mes as usize - 1].to_string();
        if m.recibos.is_empty() {
            let (t, st) = match &m.estado {
                EstadoMes::SinRecibo => ("sin recibo".to_string(), Style::new().fg(tema::AVISO)),
                EstadoMes::Pendiente => ("aún no".into(), tema::apagado()),
                EstadoMes::OtroMes(cargo) => (format!("en {}", mes_corto(cargo.get(..7).unwrap_or(""))), tema::apagado()),
                _ => ("—".into(), tema::apagado()),
            };
            v.push(Fila { celdas: [mes, t, String::new()], estilo: st, estilo_mes: Style::new(), recibo: None });
            continue;
        }
        let mut puesto = false;
        for r in &m.recibos {
            let etiqueta = if r.es_cuota() && !puesto { mes.clone() } else if r.es_cuota() { String::new() } else { etiqueta_otro(r) };
            puesto = true;
            let celdas = [etiqueta, eur(r.importe), cargo(r)];
            let estilo = if r.por_cargar(hoy) { tema::apagado() } else { Style::new() };
            let estilo_mes = if r.es_cuota() { Style::new() } else { tema::apagado().add_modifier(Modifier::ITALIC) };
            v.push(Fila { celdas, estilo, estilo_mes, recibo: Some(r.clone()) });
        }
    }
    v
}

/// La fila del total del año.
pub fn total(c: &CuotasAnio) -> [String; 3] {
    ["Total".into(), eur(c.total), String::new()]
}

/// El desglose de un recibo: (concepto, importe), sin los que van a cero.
pub fn desglose(r: &Recibo) -> Vec<(&'static str, f64)> {
    let k = &r.conceptos;
    [
        ("Pensión y bajas (contingencias comunes)", k.cc),
        ("Accidentes de trabajo", k.cp),
        ("Cese de actividad (tu «paro»)", k.cese),
        ("Formación profesional", k.fp),
        ("Equidad intergeneracional (MEI)", k.mei),
    ]
    .into_iter()
    .filter(|(_, x)| x.abs() > DIF)
    .collect()
}

/// ¿Hay algo que enseñar en la tabla? (recibos, o meses de alta sin recibo o por llegar)
pub fn hay_tabla(c: &CuotasAnio) -> bool {
    c.n_recibos > 0 || c.meses.iter().any(|m| matches!(m.estado, EstadoMes::SinRecibo | EstadoMes::Pendiente))
}


/// La base que se ve en los recibos, si la Seguridad Social no la da.
fn base_de(c: &CuotasAnio) -> Option<f64> {
    c.ultima().and_then(Recibo::base)
}

/// Las cuatro cifras de arriba: lo pagado, la cuota y la base, la base próxima (o los meses) y la regularización.
pub fn cifras(c: &CuotasAnio, ss: &Tgss, hoy: &str) -> Vec<Cifra> {
    let y = c.anio;
    let actual = y == fmt::anio_de(hoy);
    let normal = Style::new();
    let mut v = Vec::new();
    let nota = if c.por_cargar > DIF {
        format!("incluye {} del {}", eur0(c.por_cargar), dm(&c.por_cargar_el))
    } else if c.n_recibos == 0 {
        "sin recibos".into()
    } else {
        format!("{} recibo{}", c.n_recibos, if c.n_recibos == 1 { "" } else { "s" })
    };
    let valor = if c.n_recibos == 0 { "—".into() } else { eur(c.total) };
    v.push(Cifra { etiqueta: format!("Pagado en {y}"), corta: format!("Pagado {y}"), valor, nota, estilo: Style::new().fg(tema::ACENTO) });

    let cot = &ss.cotizacion;
    if actual && (cot.ultima_cuota.is_some() || c.ultima().is_some()) {
        let cuota = cot.ultima_cuota.or(c.ultima().map(|r| r.importe)).unwrap_or(0.0);
        let nota = match (cot.base, base_de(c)) {
            (Some(b), _) => format!("base {}", eur(b)),
            (None, Some(b)) => format!("base ≈ {} (recibo)", eur(b)),
            _ => String::new(),
        };
        let de = if cot.ultima_cuota_mes.is_empty() { String::new() } else { format!(" de {}", cot.ultima_cuota_mes) };
        v.push(Cifra { etiqueta: format!("Cuota{de}"), corta: "Cuota".into(), valor: eur(cuota), nota, estilo: normal });
    } else {
        let nota = c.base_media().map(|b| format!("base ≈ {}", eur0(b))).unwrap_or_default();
        let valor = if c.meses_con_cuota == 0 { "—".into() } else { eur(c.media()) };
        v.push(Cifra { etiqueta: "Cuota media".into(), corta: "Media".into(), valor, nota, estilo: normal });
    }

    let proxima = cot.base_proxima.filter(|_| actual && cot.base_proxima_desde.as_str() > hoy);
    if let Some(bp) = proxima {
        // La cuota que saldría con el mismo tipo que la de ahora.
        let tipo = match (cot.ultima_cuota, cot.base) {
            (Some(q), Some(b)) if b > 0.0 => Some(q / b),
            _ => None,
        };
        let mut nota = format!("desde el {}", dia_mes(&cot.base_proxima_desde));
        if let Some(t) = tipo {
            nota.push_str(&format!(" · ≈ {}", eur0(bp * t)));
        }
        v.push(Cifra { etiqueta: "Base próxima".into(), corta: "Base próx.".into(), valor: eur(bp), nota, estilo: Style::new().fg(tema::AVISO) });
    } else {
        let esperados = c.meses.iter().filter(|m| !matches!(m.estado, EstadoMes::Fuera)).count();
        let (nota, estilo) = if !c.sin_recibo.is_empty() {
            (format!("{} sin recibo", c.sin_recibo.len()), Style::new().fg(tema::AVISO))
        } else if c.atrasos > DIF {
            (format!("+ {} de atrasos", eur0(c.atrasos)), normal)
        } else {
            (String::new(), normal)
        };
        let valor = if esperados == 0 { "—".into() } else { format!("{} de {esperados}", c.meses_con_cuota) };
        v.push(Cifra { etiqueta: "Meses con cuota".into(), corta: "Meses".into(), valor, nota, estilo });
    }

    let (valor, nota, estilo) = match &c.regularizacion {
        Some(g) => {
            let t = autonomo::texto_regularizacion(g);
            let estilo = if t.starts_with("A ingresar") { Style::new().fg(tema::MAL) } else { Style::new().fg(tema::BIEN) };
            let nota = if g.pagado == Some(true) && !t.starts_with("Sin") { "ya pagada".to_string() } else { "según tu Renta".to_string() };
            (t, nota, estilo)
        }
        None if y >= fmt::anio_de(hoy) - 2 && c.n_recibos > 0 => ("Aún no".to_string(), format!("con tu Renta de {y}"), tema::apagado()),
        None => ("—".to_string(), "no consta".to_string(), tema::apagado()),
    };
    let etiqueta = format!("Regularización de {y}");
    v.push(Cifra { etiqueta, corta: "Regulariz.".into(), valor, nota, estilo });
    v
}

/// Las notas de debajo de la tabla, solo lo que hace falta para entenderla: lo que falta por cargar, los atrasos,
/// la reducción (tarifa plana) y los meses sin recibo.
pub fn notas(c: &CuotasAnio, ss: &Tgss, _hoy: &str) -> Vec<Nota> {
    let y = c.anio;
    let gris = tema::apagado();
    let mut v = Vec::new();
    let mut nota = |texto: String, estilo: Style| v.push(Nota { texto, estilo });
    if ss.recibos.is_empty() && ss.consultado.is_empty() {
        nota("No hay datos de la Seguridad Social: tráelos con tu certificado (S).".into(), gris);
        return v;
    }
    if c.por_cargar > DIF {
        nota(format!("Falta por cargar {} (el {}).", eur(c.por_cargar), dm(&c.por_cargar_el)), gris);
    }
    if c.atrasos > DIF {
        nota(format!("Con «+», atrasos de otros meses: {} en {y}.", eur(c.atrasos)), gris);
    }
    let red = c.reduccion();
    if red > 0.5 {
        nota(format!("Pagaste {} menos de la cuota completa (tarifa plana o bonificación).", eur(red)), gris);
    }
    if !c.sin_recibo.is_empty() {
        let meses: Vec<&str> = c.sin_recibo.iter().map(|m| fmt::MESES_C[*m as usize - 1]).collect();
        nota(format!("De alta y sin recibo en {}.", y_lista(&meses)), Style::new().fg(tema::AVISO));
    }
    v
}
