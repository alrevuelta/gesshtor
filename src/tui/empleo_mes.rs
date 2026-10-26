//! Un mes de tu sueldo (la ventana que abre Enter en Sueldo): tu nómina apuntada y lo que se liquidó a la
//! Seguridad Social, liquidación a liquidación y concepto a concepto.
use crate::dominio::fmt::DIF;
use super::empleo::{fila, filas, parrafo, pct1, Fila};
use crate::dominio::Datos;
use crate::dominio::fmt::{self, r2};
use crate::dominio::fmt::nombre_mes;
use crate::informe::empleo::MesSueldo;
use crate::tui::texto::frase;
use crate::tui::widgets::tema;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

/// Ancho de las columnas de importes.
const COL: usize = 12;

/// Todo lo del mes.
pub(super) fn lineas(d: &Datos, m: &MesSueldo, w: usize) -> Vec<Fila> {
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut v = Vec::new();
    v.push(Line::styled("Tu nómina", tema::titulo()).into());
    match &m.nomina {
        Some(n) => {
            let importe = |x: f64, st: Style| [(fmt::eur(x), COL + 2, st)];
            if !n.empresas.is_empty() {
                v.extend(filas(parrafo(&n.empresas.join(", "), w, gris)));
            }
            v.push(fila("Bruto", Style::new(), &importe(n.bruto, Style::new()), w));
            if n.especie.abs() > DIF {
                v.push(fila("  de él, en especie", gris, &importe(n.especie, gris), w));
            }
            v.push(fila(&format!("− IRPF ({})", pct1(n.retenido_pct())), Style::new(), &importe(n.irpf, Style::new()), w));
            v.push(fila("− Seguridad Social", Style::new(), &importe(n.ss, Style::new()), w));
            v.push(fila("= Te llegó en dinero", negrita, &importe(r2(n.bruto - n.especie - n.irpf - n.ss), negrita), w));
        }
        None => v.extend(filas(parrafo(&format!("No has apuntado la nómina de {}: pulsa n para apuntarla.", nombre_mes(&m.ym)), w, gris))),
    }
    v.push(Fila::default());
    v.push(Line::styled("Lo liquidado a la Seguridad Social", tema::titulo()).into());
    match d.tgss.nomina.meses.get(&m.ym).filter(|l| !l.is_empty()) {
        Some(_) => v.extend(liquidaciones(d, &m.ym, w)),
        None => v.extend(filas(parrafo("La Seguridad Social no tiene ninguna liquidación de tu nómina de este mes.", w, gris))),
    }
    v
}

/// Cada liquidación del mes con sus conceptos (sin las líneas de «líquido», que repiten).
fn liquidaciones(d: &Datos, ym: &str, w: usize) -> Vec<Fila> {
    let Some(lista) = d.tgss.nomina.meses.get(ym) else { return Vec::new() };
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut v = Vec::new();
    // Las columnas que caben: la base y el total, si hay sitio.
    let con_base = w >= 64;
    let con_total = w >= 90;
    for (i, l) in lista.iter().enumerate() {
        if i > 0 {
            v.push(Fila::default());
        }
        let tipo = if l.complementaria() { format!("complementaria: {}", l.descripcion.trim().to_lowercase()) } else { "liquidación normal".into() };
        v.extend(filas(parrafo(&format!("{} · {tipo}", l.empresa), w, negrita)));
        let mut sub = Vec::new();
        if !l.ccc.is_empty() {
            sub.push(format!("CCC {}", l.ccc));
        }
        if !l.presentada.is_empty() {
            sub.push(format!("presentada en {}", nombre_mes(&l.presentada)));
        }
        if !l.calculada.is_empty() {
            sub.push(format!("calculada el {}", fmt::dmy(&l.calculada)));
        }
        v.extend(filas(parrafo(&sub.join(" · "), w, gris)));
        let cols = |base: String, empresa: String, tu: String, total: String, st: Style| {
            let mut c = Vec::new();
            if con_base {
                c.push((base, COL, if st == negrita { st } else { gris }));
            }
            c.push((empresa, COL, st));
            c.push((tu, COL, st));
            if con_total {
                c.push((total, COL, st));
            }
            c
        };
        let cab = tema::cabecera_tabla();
        v.push(fila("Concepto", cab, &cols("Base".into(), "Tu empresa".into(), "Tú".into(), "Total".into(), cab), w));
        let o = |x: f64| if x.abs() > DIF { fmt::eur(x) } else { "—".into() };
        for c in l.conceptos.iter().filter(|c| !c.es_total()) {
            v.push(fila(&frase(&c.concepto), Style::new(), &cols(o(c.base), fmt::eur(c.empresa), o(c.trabajador), fmt::eur(c.total), Style::new()), w));
        }
        let total = r2(l.cuota_empresa + l.cuota_trabajador);
        v.push(fila("Total", negrita, &cols(String::new(), fmt::eur(l.cuota_empresa), fmt::eur(l.cuota_trabajador), fmt::eur(total), negrita), w));
    }
    v
}
