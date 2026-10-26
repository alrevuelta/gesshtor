//! El camino del dinero de un año (la ventana «c» de Sueldo): de lo que pagó tu empresa por ti a lo que te llegó,
//! paso a paso, de cada 100 € y lo cotizado por conceptos.
use crate::dominio::fmt::DIF;
use super::empleo::{con_importes, fila, filas, parrafo, pct1, sangrar, Fila};
use crate::dominio::fmt;
use crate::informe::empleo::{Camino, Cifra, EmpleoAnio};
use crate::tui::widgets::tema;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Ancho de las columnas de importes y de porcentajes.
const COL: usize = 13;
const PCT: usize = 7;

/// Colores de los trozos de la barra (los mismos en la leyenda).
fn color_ss() -> Color {
    tema::AVISO
}
fn color_irpf() -> Color {
    tema::MAL
}
fn color_dinero() -> Color {
    tema::BIEN
}
fn color_especie() -> Color {
    tema::ACENTO
}

/// El título de la ventana.
pub(super) fn titulo(c: &Camino) -> &'static str {
    if c.coste.is_some() { "De lo que pagó tu empresa a lo que te llegó" } else { "De tu sueldo bruto a lo que te llegó" }
}

/// Todo lo de la ventana.
pub(super) fn lineas(c: &Camino, a: &EmpleoAnio, w: usize) -> Vec<Fila> {
    let mut v = filas(parrafo(&format!("Según {}{}.", c.de.texto(), if a.empresa.is_some() { " y la Seguridad Social" } else { "" }), w, tema::apagado()));
    v.push(Fila::default());
    v.extend(pasos(c, a.anio, w));
    if a.con_nomina().is_some() {
        v.push(Fila::default());
        v.push(Line::styled("La Seguridad Social, por conceptos", tema::titulo()).into());
        v.extend(conceptos(a, w));
    }
    v
}

/// Paso a paso: de lo que pagó tu empresa (o de tu bruto) a lo que te llegó, y la barra de cada 100 €.
fn pasos(c: &Camino, y: i32, w: usize) -> Vec<Fila> {
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    // Los pasos, juntos (no a todo el ancho): la etiqueta cerca de su importe.
    let et_w = w.min(76).saturating_sub(COL + PCT + 2).max(10);
    let paso = |et: &str, st: Style, x: f64, signo: &str| -> Fila {
        let mut et: String = et.chars().take(et_w).collect();
        let n = et.chars().count();
        et.extend(std::iter::repeat_n(' ', et_w - n));
        let importe = format!("{signo}{}", fmt::eur(x));
        Line::from(vec![Span::styled(et, st), Span::styled(format!(" {importe:>COL$}"), st), Span::styled(format!(" {:>PCT$}", pct1(c.pct(x))), gris)]).into()
    };
    let mut v = Vec::new();
    if let (Some(coste), Some(e)) = (c.coste, c.ss_empresa) {
        v.push(paso("Lo que pagó tu empresa por ti", negrita, coste, ""));
        v.push(paso("  − Seguridad Social que paga tu empresa", Style::new().fg(color_ss()), e, "−"));
    }
    v.push(paso(if c.coste.is_some() { "= Tu sueldo bruto" } else { "Tu sueldo bruto" }, negrita, c.bruto, ""));
    v.push(paso("  − Tu Seguridad Social", Style::new().fg(color_ss()), c.tu_ss, "−"));
    let irpf = if c.irpf_especie > DIF { "  − IRPF de tu sueldo" } else { "  − IRPF que te retuvieron" };
    v.push(paso(irpf, Style::new().fg(color_irpf()), c.irpf_sueldo, "−"));
    if c.irpf_especie > DIF {
        v.push(paso("  − IRPF de lo cobrado en especie", Style::new().fg(color_irpf()), c.irpf_especie, "−"));
        let quien = if c.especie_repercutida { "te lo descontaron de la nómina" } else { "lo pagó tu empresa por ti" };
        v.push(Line::styled(format!("      ({quien})"), gris).into());
    }
    v.push(paso("= Te llegó en dinero", negrita.fg(color_dinero()), c.dinero, ""));
    if c.especie > DIF {
        v.push(paso("+ En especie (acciones, seguro, coche…)", Style::new().fg(color_especie()), c.especie, ""));
    }
    v.push(Fila::default());
    // La barra: de cada 100 € de lo que pagó tu empresa (o de tu bruto).
    let ancho = w.saturating_sub(2).min(60);
    let trozos = [(c.ss_empresa.unwrap_or(0.0) + c.tu_ss, color_ss()), (c.irpf(), color_irpf()), (c.dinero, color_dinero()), (c.especie, color_especie())];
    let mut spans = Vec::new();
    let mut usado = 0usize;
    for (i, (x, col)) in trozos.iter().enumerate() {
        let n = if i == trozos.len() - 1 { ancho.saturating_sub(usado) } else { (c.pct(*x) / 100.0 * ancho as f64).round().max(0.0) as usize };
        let n = n.min(ancho.saturating_sub(usado));
        usado += n;
        spans.push(Span::styled("█".repeat(n), Style::new().fg(*col)));
    }
    v.push(Line::from(spans).into());
    let mut ley = vec![Span::styled("■", Style::new().fg(color_ss())), Span::styled(" Seguridad Social  ", gris), Span::styled("■", Style::new().fg(color_irpf())), Span::styled(" IRPF  ", gris)];
    ley.extend([Span::styled("■", Style::new().fg(color_dinero())), Span::styled(" en dinero  ", gris)]);
    if c.especie > DIF {
        ley.extend([Span::styled("■", Style::new().fg(color_especie())), Span::styled(" en especie", gris)]);
    }
    v.push(Line::from(ley).into());
    v.push(Fila::default());
    let (imp, din, esp) = c.de_cada_100();
    let quien = if c.coste.is_some() { "que pagó tu empresa por ti" } else { "de tu sueldo bruto" };
    let especie = if esp > 0 { format!(" y {esp} en especie") } else { String::new() };
    let cotiz = if c.coste.is_some() { "impuestos y cotizaciones (las tuyas y las de tu empresa)" } else { "IRPF y tu Seguridad Social" };
    v.extend(filas(con_importes(&format!("De cada 100 € {quien}, {imp} fueron {cotiz}, {din} te llegaron en dinero{especie}."), w, Style::new())));
    if c.coste.is_none() {
        v.extend(filas(parrafo(&format!("Lo que paga tu empresa a la Seguridad Social por ti no lo tiene la app para {y}."), w, gris)));
    }
    v
}

/// Lo cotizado en el año por grupos de conceptos, lo de tu empresa y lo tuyo.
fn conceptos(a: &EmpleoAnio, w: usize) -> Vec<Fila> {
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let cab = tema::cabecera_tabla();
    let mut v = vec![fila("", Style::new(), &[("Tu empresa".into(), COL, cab), ("Tú".into(), COL, cab)], w)];
    let grupos = a.con_nomina().map(|n| n.grupos_con_algo()).unwrap_or_default();
    for (nombre, explica, p) in grupos {
        let tu = if p.trabajador.abs() > DIF { fmt::eur(p.trabajador) } else { "—".into() };
        v.push(fila(nombre, Style::new(), &[(fmt::eur(p.empresa), COL, Style::new()), (tu, COL, Style::new())], w));
        v.extend(sangrar(parrafo(explica, w.saturating_sub(2), gris), 2));
    }
    let total = |c: Option<Cifra>| c.map(|c| fmt::eur(c.v)).unwrap_or_default();
    v.push(fila("Total", negrita, &[(total(a.empresa), COL, negrita), (total(a.tu_ss), COL, negrita)], w));
    v
}
