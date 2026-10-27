//! Un año, paso a paso (Enter en «Años»): cada fuente de dinero (sueldo, actividad, alquileres, ahorro), de lo que
//! generó a lo que te quedó, con lo que se fue en cada paso y de dónde sale (las casillas de tu modelo 100, la
//! Seguridad Social, tus 303…); la Seguridad Social de tu nómina, por conceptos; todo junto; y tu IRPF: lo que te
//! tocó y cómo lo pagaste; en el año en curso, lo que saldría en tu Renta. Arriba, los avisos (cifras aproximadas,
//! declaración conjunta…). Las cifras salen de `informe::reparto`, `informe::anio` e `informe::prevision`; aquí solo
//! se pintan.
use super::anios::vinetas;
use super::empleo::pct1;
use crate::dominio::fmt::{eur, r2, DIF};
use crate::informe::anio::{self, Concepto, Detalle};
use crate::informe::prevision::Prevision;
use crate::informe::reparto::{Fuente, Reparto, ACTIVIDAD, AHORRO, SUELDO};
use crate::tui::texto::envolver;
use crate::tui::widgets::tema;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Ancho de la columna de importes.
const COL: usize = 13;

fn gris() -> Style {
    tema::apagado()
}

fn negrita() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

fn ss() -> Style {
    Style::new().fg(tema::AVISO)
}

fn irpf() -> Style {
    Style::new().fg(tema::MAL)
}

fn queda(x: f64) -> Style {
    negrita().fg(if x < -DIF { tema::MAL } else { tema::BIEN })
}

/// «1.234,56 €», «−2.746,00 €»
pub(super) fn importe(x: f64) -> String {
    if x < -DIF { format!("−{}", eur(-x)) } else { eur(x) }
}

/// Los pasos de un bloque: «etiqueta   importe», juntos (no a todo el ancho). También para la calculadora.
pub(super) struct Pasos {
    et_w: usize,
    pub(super) lineas: Vec<Line<'static>>,
}

impl Pasos {
    pub(super) fn new(w: usize) -> Pasos {
        Pasos { et_w: w.min(76).saturating_sub(COL + 1).max(12), lineas: Vec::new() }
    }

    pub(super) fn paso(&mut self, et: &str, st: Style, x: f64, signo: &str) {
        let mut et: String = et.chars().take(self.et_w).collect();
        let n = et.chars().count();
        et.extend(std::iter::repeat_n(' ', self.et_w - n));
        let valor = format!("{signo}{}", importe(x));
        self.lineas.push(Line::from(vec![Span::styled(et, st), Span::styled(format!(" {valor:>COL$}"), st)]));
    }

    /// Un paso de detalle, en gris y más adentro.
    pub(super) fn detalle(&mut self, et: &str, x: f64) {
        self.paso(&format!("      {et}"), gris(), x, "");
    }

    pub(super) fn nota(&mut self, t: String, w: usize) {
        self.lineas.extend(envolver(&[(t, gris())], w, 4));
    }
}

/// «De dónde sale: modelo 100, casillas 0012 0013 · Seguridad Social, 10 meses», de lo que ganaste con esos conceptos
/// y lo que se diga además. Si algo es aproximado, se avisa.
fn de_donde(det: &Detalle, conceptos: &[Concepto], mas: &[String], w: usize) -> Vec<Line<'static>> {
    let filas: Vec<&anio::Fila> = det.ganado.filas.iter().filter(|f| f.concepto.is_some_and(|c| conceptos.contains(&c))).collect();
    let (mut casillas, mut textos): (Vec<u32>, Vec<String>) = (Vec::new(), Vec::new());
    for fuente in filas.iter().flat_map(|f| f.fuentes.iter()) {
        match fuente {
            anio::Fuente::Renta(_, cs) => {
                for c in cs {
                    if !casillas.contains(c) {
                        casillas.push(*c);
                    }
                }
            }
            anio::Fuente::Texto(t) if !textos.contains(t) => textos.push(t.clone()),
            anio::Fuente::Texto(_) => {}
        }
    }
    let mut partes = Vec::new();
    if !casillas.is_empty() {
        partes.push(format!("modelo 100, casillas {}", casillas.iter().map(|c| format!("{c:04}")).collect::<Vec<_>>().join(" ")));
    }
    partes.extend(textos);
    // Sin repetir lo que ya dicen las cifras («Seguridad Social · 10 meses» y «Seguridad Social, 10 meses»).
    let igual = |a: &str, b: &str| a.replace(" · ", ", ") == b.replace(" · ", ", ");
    for m in mas {
        if !partes.iter().any(|p| igual(p, m)) {
            partes.push(m.clone());
        }
    }
    if partes.is_empty() {
        return Vec::new();
    }
    let aprox = filas.iter().any(|f| f.origen == anio::Origen::Estimado);
    let (t, st) = if aprox { (format!("Aproximado. De dónde sale: {}", partes.join(" · ")), Style::new().fg(tema::AVISO)) } else { (format!("De dónde sale: {}", partes.join(" · ")), gris().add_modifier(Modifier::ITALIC)) };
    envolver(&[(t, st)], w, 0)
}

/// Lo que ganaste por un concepto, tal como lo explica el balance del año (con su nota).
fn filas_de(det: &Detalle, c: Concepto) -> impl Iterator<Item = &anio::Fila> {
    det.ganado.filas.iter().filter(move |f| f.concepto == Some(c) && !f.aparte)
}

/// La nota de lo que ganaste por un concepto («intereses 600 € · dividendos 120 €…»), si la hay.
fn nota_de(det: &Detalle, c: Concepto) -> Option<String> {
    filas_de(det, c).map(|f| f.nota.clone()).find(|n| !n.is_empty())
}

/// «Impuestos y cotizaciones: 14.450,00 € (38,9 % de lo que pagó tu empresa)».
fn resumen(f: &Fuente, sobre: &str, w: usize) -> Vec<Line<'static>> {
    let que = if f.seguridad_social > DIF { "Impuestos y cotizaciones" } else { "Impuestos" };
    let pct = f.tipo().map(|t| if sobre.is_empty() { format!(" ({})", pct1(t * 100.0)) } else { format!(" ({} {sobre})", pct1(t * 100.0)) }).unwrap_or_default();
    envolver(&[(format!("{que}: {}{pct}", eur(f.impuestos())), negrita())], w, 0)
}

/// La etiqueta del IRPF de una fuente (con Renta).
fn et_irpf(r: &Reparto) -> &'static str {
    match (r.fuentes.len() > 1, r.deducciones > DIF) {
        (true, true) => "  − IRPF (su parte, antes de deducciones)",
        (false, true) => "  − IRPF (antes de deducciones)",
        (true, false) => "  − IRPF (su parte de tu Renta)",
        (false, false) => "  − IRPF (el de tu Renta)",
    }
}

/// Hay que enseñar el total: con varias fuentes, o con deducciones o ajustes de la Renta (que no son de ninguna).
fn con_total(r: &Reparto) -> bool {
    r.fuentes.len() > 1 || r.deducciones.abs() > DIF || r.ajustes.abs() > DIF
}

fn sueldo(f: &Fuente, r: &Reparto, w: usize) -> Vec<Line<'static>> {
    let (bruto, empresa) = r.sueldo.unwrap_or((f.generado, 0.0));
    let con_empresa = empresa > DIF;
    let mut p = Pasos::new(w);
    if con_empresa {
        p.paso("Lo que pagó tu empresa por ti", negrita(), f.generado, "");
        p.paso("  − Seguridad Social que paga tu empresa", ss(), empresa, "−");
        for (nombre, e, _) in r.ss_grupos.iter().filter(|g| g.1.abs() > DIF) {
            p.detalle(nombre, *e);
        }
        p.paso("= Tu sueldo bruto", negrita(), bruto, "");
    } else {
        p.paso("Tu sueldo bruto", negrita(), bruto, "");
    }
    let tuya = r.tu_ss.0;
    if tuya > DIF {
        p.paso("  − Tu Seguridad Social", ss(), tuya, "−");
        for (nombre, _, t) in r.ss_grupos.iter().filter(|g| g.2.abs() > DIF) {
            p.detalle(nombre, *t);
        }
    }
    if f.irpf > DIF || r.irpf_retenido.is_some() {
        p.paso(if r.con_renta { et_irpf(r) } else { "  − IRPF que te retuvieron" }, irpf(), f.irpf, "−");
        if r.con_renta
            && let Some(ret) = r.irpf_retenido
        {
            p.nota(format!("En tus nóminas te retuvieron {}; lo demás se ajustó al hacer la Renta.", eur(ret)), w);
        }
    }
    p.paso("= Te queda", queda(f.queda), f.queda, "");
    if r.especie > DIF {
        p.nota(format!("De ello, {} en especie (acciones, seguro, coche…).", eur(r.especie)), w);
    }
    let mut l = p.lineas;
    l.push(Line::from(""));
    l.extend(resumen(f, if con_empresa { "de lo que pagó tu empresa" } else { "de tu bruto" }, w));
    if !con_empresa {
        l.extend(envolver(&[("Lo que paga tu empresa a la Seguridad Social por ti no lo tiene la app para este año.".into(), gris())], w, 0));
    }
    l
}

fn actividad(f: &Fuente, r: &Reparto, w: usize) -> Vec<Line<'static>> {
    let facturado = r2(f.generado - f.iva);
    let rendimiento = r2(facturado - f.gastos);
    let mut p = Pasos::new(w);
    p.paso("Lo que facturaste (sin IVA)", negrita(), facturado, "");
    if f.gastos > DIF {
        p.paso("  − Gastos de tu actividad", gris(), f.gastos, "−");
        p.paso("= Rendimiento", negrita(), rendimiento, "");
    }
    if f.seguridad_social > DIF {
        p.paso("  − Tu cuota de autónomo", ss(), f.seguridad_social, "−");
    }
    if f.irpf > DIF {
        p.paso(if r.con_renta { et_irpf(r) } else { "  − IRPF adelantado (130 y retenciones)" }, irpf(), f.irpf, "−");
    }
    p.paso("= Te queda", queda(f.queda), f.queda, "");
    let mut l = p.lineas;
    l.push(Line::from(""));
    l.extend(resumen(f, if f.gastos > DIF { "del rendimiento" } else { "de lo facturado" }, w));
    if let Some((cobrado, deducido, n)) = r.iva {
        let de = if n == 4 { "tus 303".to_string() } else { format!("{n} de tus 303") };
        let neto = r2(cobrado - deducido);
        let resto = if neto > DIF { format!("los {} de diferencia fueron a Hacienda", eur(neto)) } else { format!("Hacienda te debía {} (compensado o devuelto)", eur(-neto)) };
        let t = format!(
            "Además, el IVA ({de}): cobraste {} a tus clientes y dedujiste {} de tus compras; {resto}. Lo paga tu cliente encima de tu precio, así que no sale de lo que te queda (si es un particular, puede que en parte sí salga de tu margen).",
            eur(cobrado),
            eur(deducido)
        );
        l.extend(envolver(&[(t, gris())], w, 0));
    }
    l
}

/// Alquileres y ahorro: lo que generó (con su detalle), el IRPF y lo que te queda.
fn otra(f: &Fuente, r: &Reparto, det: &Detalle, w: usize) -> Vec<Line<'static>> {
    let mut p = Pasos::new(w);
    let con_nota = |p: &mut Pasos, c: Concepto| {
        if let Some(n) = nota_de(det, c) {
            p.nota(n, w);
        }
    };
    match (f.nombre, r.ahorro) {
        (AHORRO, Some((capital, ganancias))) if capital.abs() > DIF && ganancias.abs() > DIF => {
            p.paso("Intereses y dividendos", Style::new(), capital, "");
            con_nota(&mut p, Concepto::Capital);
            p.paso("+ Ganancias patrimoniales", Style::new(), ganancias, "");
            con_nota(&mut p, Concepto::Ganancias);
            p.paso("= Tu ahorro", negrita(), f.generado, "");
        }
        (AHORRO, Some((capital, _))) if capital.abs() > DIF => {
            p.paso("Intereses y dividendos", negrita(), f.generado, "");
            con_nota(&mut p, Concepto::Capital);
        }
        (AHORRO, _) => {
            p.paso("Ganancias patrimoniales", negrita(), f.generado, "");
            con_nota(&mut p, Concepto::Ganancias);
        }
        _ => {
            p.paso("Lo que generaron", negrita(), f.generado, "");
            for x in filas_de(det, Concepto::OtrasRentas) {
                p.detalle(&x.nombre, x.importe.unwrap_or(0.0));
                if !x.nota.is_empty() {
                    p.nota(x.nota.clone(), w);
                }
            }
            for x in det.ganado.filas.iter().filter(|x| x.aparte) {
                p.nota(format!("{} ({}): {}", x.nombre, importe(x.importe.unwrap_or(0.0)), x.nota), w);
            }
        }
    }
    if f.irpf > DIF {
        p.paso(et_irpf(r), irpf(), f.irpf, "−");
    }
    p.paso("= Te queda", queda(f.queda), f.queda, "");
    let mut l = p.lineas;
    l.push(Line::from(""));
    l.extend(resumen(f, "", w));
    l
}

/// Todo junto, si hay más de una fuente.
fn total(t: &Fuente, r: &Reparto, w: usize) -> Vec<Line<'static>> {
    let generado = r2(t.generado - t.iva);
    let mut p = Pasos::new(w);
    let empresa = r.sueldo.map_or(0.0, |s| s.1);
    p.paso(if empresa > DIF { "Lo que generaste (con lo de tu empresa)" } else { "Lo que generaste" }, negrita(), generado, "");
    if t.gastos > DIF {
        p.paso("  − Gastos de tu actividad", gris(), t.gastos, "−");
    }
    if t.seguridad_social > DIF {
        p.paso("  − Seguridad Social", ss(), t.seguridad_social, "−");
        for (et, x) in [("de tu empresa", empresa), ("tuya, de la nómina", r.tu_ss.0), ("tu cuota de autónomo", r.tu_ss.1)] {
            if x > DIF {
                p.detalle(et, x);
            }
        }
    }
    // El IRPF de las fuentes y, aparte, lo que cambiaron las deducciones y los ajustes de la Renta.
    let de_fuentes = r2(t.irpf + r.deducciones - r.ajustes);
    let aparte = r.deducciones.abs() > DIF || r.ajustes.abs() > DIF;
    if de_fuentes.abs() > DIF {
        let et = match (r.con_renta, aparte) {
            (false, _) => "  − IRPF adelantado",
            (true, true) => "  − IRPF de tus fuentes",
            (true, false) => "  − IRPF",
        };
        p.paso(et, irpf(), de_fuentes, "−");
    }
    if r.deducciones.abs() > DIF {
        p.paso("  + Deducciones de tu Renta", Style::new().fg(tema::BIEN), r.deducciones, "+");
    }
    if r.ajustes.abs() > DIF {
        p.paso("  ± Otros ajustes de tu Renta", irpf(), -r.ajustes, "");
    }
    p.paso("= Te queda", queda(t.queda), t.queda, "");
    let mut l = p.lineas;
    l.push(Line::from(""));
    l.extend(resumen(t, if t.gastos > DIF { "de lo que generaste menos los gastos" } else { "de lo que generaste" }, w));
    l
}

/// Tu IRPF del año: con Renta, lo que te tocó por cada base, las deducciones y la cuota (como en tu modelo 100), y
/// cómo lo pagaste; sin ella, lo que adelantaste.
fn irpf_del_anio(det: &Detalle, w: usize) -> Vec<Line<'static>> {
    let ir = &det.irpf;
    if ir.filas.is_empty() && ir.toco.is_empty() {
        return Vec::new();
    }
    let mut p = Pasos::new(w);
    // Ya se sabe que es el modelo 100: de él, solo las casillas.
    let fuente = |fs: &[anio::Fuente]| {
        fs.iter()
            .map(|f| match f {
                anio::Fuente::Renta(_, cs) if cs.len() == 1 => format!("casilla {:04}", cs[0]),
                anio::Fuente::Renta(_, cs) => format!("casillas {}", cs.iter().map(|c| format!("{c:04}")).collect::<Vec<_>>().join(" ")),
                anio::Fuente::Texto(t) => t.clone(),
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };
    if ir.con_renta {
        for x in &ir.toco {
            p.paso(&format!("  {}", x.nombre), Style::new(), x.importe.unwrap_or(0.0), "");
            if !x.nota.is_empty() {
                p.nota(x.nota.clone(), w);
            }
        }
        p.paso("= Tu IRPF del año", negrita().fg(tema::MAL), ir.total, "");
        let f = fuente(&ir.fuente);
        if !f.is_empty() {
            p.nota(f, w);
        }
        p.lineas.push(Line::styled("Cómo lo pagaste:", gris().add_modifier(Modifier::BOLD)));
    }
    for x in &ir.filas {
        p.paso(&format!("  {}", x.nombre), Style::new(), x.importe.unwrap_or(0.0), "");
        let mut t = x.nota.clone();
        let f = fuente(&x.fuentes);
        // Sin repetir lo que ya dice la nota («en tus facturas · tus facturas»).
        if !f.is_empty() && !t.contains(&f) {
            t = if t.is_empty() { f } else { format!("{t} · {f}") };
        }
        if !t.is_empty() {
            p.nota(t, w);
        }
    }
    if !ir.con_renta {
        p.paso("= Lo adelantado", negrita().fg(tema::MAL), ir.total, "");
    }
    p.lineas
}

/// Lo que saldría en la Renta del año en curso (o por qué aún no se sabe).
fn prevision(p: &Prevision, w: usize) -> Vec<Line<'static>> {
    let mut l = Pasos::new(w);
    match p {
        Prevision::Estimada { resultado: x, supuestos } => {
            let (et, st) = if *x > DIF {
                ("  Te saldría a pagar", irpf())
            } else if *x < -DIF {
                ("  Te saldría a devolver", Style::new().fg(tema::BIEN))
            } else {
                ("  Te saldría a cero", Style::new())
            };
            l.paso(et, st.add_modifier(Modifier::BOLD), x.abs(), "≈");
            l.nota(format!("Si sigue así: lo que te tocaría por tu sueldo menos lo que te retienen. {supuestos}"), w);
        }
        Prevision::NoSe(por) => l.nota(por.clone(), w),
    }
    l.lineas
}

/// Todo lo que se ve, en `w` columnas: los avisos, cada fuente, todo junto, tu IRPF y, en el año en curso, lo que
/// saldría en tu Renta.
pub(super) fn lineas(r: &Reparto, det: &Detalle, prev: Option<&Prevision>, w: usize) -> Vec<Line<'static>> {
    let mut l: Vec<Line<'static>> = Vec::new();
    // Los avisos, arriba: lo que cambia cómo se leen las cifras.
    let avisos: Vec<(String, Style)> = det
        .notas
        .iter()
        .map(|n| {
            let fuerte = n.starts_with("Declaración conjunta") || n.starts_with("Falta") || n.starts_with("El sueldo es aproximado");
            (n.clone(), if fuerte { Style::new().fg(tema::AVISO) } else { gris() })
        })
        .collect();
    if r.fuentes.is_empty() {
        if avisos.is_empty() {
            l.push(Line::styled(format!("No hay datos de {}.", r.y), gris()));
        }
        l.extend(vinetas(avisos, w));
        return l;
    }
    if !avisos.is_empty() {
        l.extend(vinetas(avisos, w));
        l.push(Line::from(""));
    }
    let titulo = |t: &str| Line::styled(t.to_string(), tema::titulo());
    let meses = det.nomina.as_ref().filter(|n| n.con_nomina > 0).map(|n| format!("Seguridad Social, {}", n.texto_meses()));
    for f in &r.fuentes {
        let (t, origen, cuerpo) = match f.nombre {
            SUELDO => ("Tu sueldo", de_donde(det, &[Concepto::Sueldo], &meses.clone().into_iter().collect::<Vec<_>>(), w), sueldo(f, r, w)),
            ACTIVIDAD => {
                let mut mas: Vec<String> = det.cuota.iter().map(|c| format!("cuota: {}", c.de)).collect();
                if r.iva.is_some() {
                    mas.push("IVA: tus 303".into());
                }
                ("Como autónomo", de_donde(det, &[Concepto::Actividad], &mas, w), actividad(f, r, w))
            }
            AHORRO => ("Tu ahorro", de_donde(det, &[Concepto::Capital, Concepto::Ganancias], &[], w), otra(f, r, det, w)),
            _ => ("Alquileres y otras rentas", de_donde(det, &[Concepto::OtrasRentas], &[], w), otra(f, r, det, w)),
        };
        l.push(titulo(t));
        l.extend(origen);
        l.extend(cuerpo);
        l.push(Line::from(""));
    }
    if con_total(r) {
        l.push(titulo("Todo junto"));
        l.extend(total(&r.total, r, w));
        l.push(Line::from(""));
    }
    let irpf = irpf_del_anio(det, w);
    if !irpf.is_empty() {
        l.push(titulo(if r.con_renta { "Tu IRPF, según tu modelo 100" } else { "Tu IRPF adelantado" }));
        l.extend(irpf);
        l.push(Line::from(""));
    }
    if let Some(p) = prev {
        let estimada = if matches!(p, Prevision::Estimada { .. }) { ", estimada" } else { "" };
        l.push(titulo(&format!("Tu Renta de {}{estimada}", r.y)));
        l.extend(prevision(p, w));
        l.push(Line::from(""));
    }
    l.extend(vinetas(notas(r), w));
    l
}

/// Cómo se ha hecho la cuenta.
fn notas(r: &Reparto) -> Vec<(String, Style)> {
    let mut v = vec![("Las cotizaciones no son impuestos: dan derecho a pensión, paro y bajas; aun así, se cuentan con ellos.".to_string(), gris())];
    if r.con_renta {
        if r.fuentes.len() > 1 {
            v.push(("El IRPF de cada fuente es su parte de tu Renta: el del ahorro, el de su escala; el de la base general, repartido entre sueldo, actividad y alquileres según lo que aporta cada uno a ella.".into(), gris()));
        }
        if r.deducciones > DIF {
            v.push(("Las deducciones de tu Renta (por invertir en empresas nuevas, donativos, vivienda…) rebajan el IRPF de todo, no el de una fuente: por eso van aparte, en «Todo junto», y el IRPF de cada fuente es el de antes de deducciones.".into(), gris()));
        }
    } else {
        v.push((format!("Sin Renta de {}: el IRPF es lo adelantado (lo retenido en tu nómina, tus 130 y lo que te retienen tus clientes), no el del año.", r.y), Style::new().fg(tema::AVISO)));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un año con sueldo, actividad con IVA y ahorro, con Renta.
    fn con_todo() -> Reparto {
        let f = |nombre, generado, gastos, iva, seguridad_social, irpf| Fuente { nombre, generado, gastos, iva, seguridad_social, irpf, queda: r2(generado - gastos - iva - seguridad_social - irpf) };
        let fuentes = vec![
            f(SUELDO, 37100.0, 0.0, 0.0, 9050.0, 5400.0),
            f(ACTIVIDAD, 19050.0, 4000.0, 1050.0, 3500.0, 2900.0),
            f(AHORRO, 2600.0, 0.0, 0.0, 0.0, 600.0),
        ];
        let total = f("Total", 58750.0, 4000.0, 1050.0, 12550.0, 8900.0);
        Reparto {
            y: 2024,
            fuentes,
            total,
            sueldo: Some((30000.0, 7100.0)),
            ss_grupos: vec![("Pensiones y bajas", 5700.0, 1450.0), ("Desempleo, FOGASA y formación", 1250.0, 500.0), ("Accidentes de trabajo y MEI", 150.0, 0.0)],
            irpf_retenido: Some(5100.0),
            especie: 0.0,
            tu_ss: (1950.0, 3500.0),
            ahorro: Some((600.0, 2000.0)),
            deducciones: 0.0,
            ajustes: 0.0,
            iva: Some((4200.0, 3150.0, 4)),
            con_renta: true,
        }
    }

    fn pintado(r: &Reparto, w: usize) -> Vec<String> {
        lineas(r, &Detalle::default(), None, w).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>().trim_end().to_string()).collect()
    }

    #[test]
    fn de_donde_sale_y_el_irpf_del_modelo_100() {
        use crate::informe::anio::{Fila, Irpf, Origen};
        let mut det = Detalle::default();
        det.anio.y = 2024;
        det.ganado.filas = vec![Fila { nombre: "Sueldo".into(), importe: Some(30000.0), fuentes: vec![anio::Fuente::Renta(2024, vec![12, 13])], concepto: Some(Concepto::Sueldo), ..Default::default() }];
        det.irpf = Irpf {
            total: 5400.0,
            con_renta: true,
            fuente: vec![anio::Fuente::Renta(2024, vec![595])],
            toco: vec![Fila { nombre: "Por tu sueldo, tu actividad y demás".into(), importe: Some(5400.0), nota: "base general 24.000,00 € · tipo medio 22,5 %".into(), ..Default::default() }],
            filas: vec![Fila { nombre: "Retenido en tu nómina".into(), importe: Some(5100.0), fuentes: vec![anio::Fuente::Renta(2024, vec![596])], ..Default::default() }],
            ..Default::default()
        };
        let mut r = con_todo();
        r.fuentes.truncate(1);
        let t = lineas(&r, &det, None, 100).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect::<Vec<_>>().join("\n");
        for esperado in ["De dónde sale: modelo 100, casillas 0012 0013", "Tu IRPF, según tu modelo 100", "tipo medio 22,5 %", "= Tu IRPF del año", "casilla 0595", "Cómo lo pagaste:", "casilla 0596"] {
            assert!(t.contains(esperado), "falta «{esperado}»:\n{t}");
        }
        // Lo que ya dicen las cifras no se repite.
        let ss = vec![anio::Fuente::Texto("Seguridad Social · 10 meses".into())];
        let det_ss = Detalle { ganado: anio::Ganado { filas: vec![Fila { fuentes: ss, concepto: Some(Concepto::Sueldo), ..Default::default() }], ..Default::default() }, ..Default::default() };
        let t = de_donde(&det_ss, &[Concepto::Sueldo], &["Seguridad Social, 10 meses".into()], 100).iter().map(|l| l.to_string()).collect::<String>();
        assert_eq!(t, "De dónde sale: Seguridad Social · 10 meses");
        // Lo aproximado se avisa.
        det.ganado.filas[0].origen = Origen::Estimado;
        let t = lineas(&r, &det, None, 100).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect::<Vec<_>>().join("\n");
        assert!(t.contains("Aproximado. De dónde sale"), "{t}");
    }

    #[test]
    fn un_anio_con_todo() {
        let t = pintado(&con_todo(), 100);
        println!("{}", t.join("\n"));
        let todo = t.join("\n");
        for esperado in [
            "Lo que pagó tu empresa por ti",
            "= Tu sueldo bruto",
            "Impuestos y cotizaciones: 14.450,00 € (38,9 % de lo que pagó tu empresa)",
            "Impuestos y cotizaciones: 6.400,00 € (45,7 % del rendimiento)",
            "Impuestos: 600,00 € (23,1 %)",
            "En tus nóminas te retuvieron 5.100,00 €",
        ] {
            assert!(todo.contains(esperado), "falta «{esperado}»");
        }
        // Cada paso, con su importe (sin porcentajes: solo el del resumen).
        let fila = |et: &str| t.iter().find(|l| l.trim_start().starts_with(et)).unwrap_or_else(|| panic!("falta {et}")).clone();
        assert!(fila("− Seguridad Social que paga tu empresa").ends_with("−7.100,00 €"), "{}", fila("− Seguridad Social que paga tu empresa"));
        assert!(fila("Pensiones y bajas").ends_with("5.700,00 €"), "{}", fila("Pensiones y bajas"));
        // Con deducciones: cada fuente, antes de ellas; las deducciones, en el total.
        let mut r = con_todo();
        r.deducciones = 2000.0;
        r.total.irpf = 6900.0;
        r.total.queda = 34250.0;
        let todo = pintado(&r, 100).join("\n");
        for esperado in ["− IRPF (su parte, antes de deducciones)", "− IRPF de tus fuentes", "+ Deducciones de tu Renta", "+2.000,00 €"] {
            assert!(todo.contains(esperado), "falta «{esperado}»");
        }
        assert!(pintado(&r, 100).iter().any(|l| l.starts_with("  − IRPF de tus fuentes") && l.ends_with("−8.900,00 €")));
        // En el año en curso, lo que saldría en la Renta.
        let p = Prevision::Estimada { resultado: -310.0, supuestos: "Calculado para un sueldo así.".into() };
        let todo = lineas(&con_todo(), &Detalle::default(), Some(&p), 100).iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(todo.contains("Tu Renta de 2024, estimada") && todo.contains("Calculado para un sueldo"), "{todo}");
        assert!(todo.lines().any(|l| l.starts_with("  Te saldría a devolver") && l.ends_with("≈310,00 €")), "{todo}");
        // En una terminal estrecha no se sale nada.
        let estrecho = pintado(&con_todo(), 40);
        assert!(estrecho.iter().all(|l| l.chars().count() <= 40), "{estrecho:#?}");
    }
}
