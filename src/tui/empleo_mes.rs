//! Un mes de tu sueldo (la ventana que abre Enter en Sueldo): tu nómina apuntada, como un recibo con puntos guía
//! que acaba en lo que te llegó en dinero, en una banda; y lo que se liquidó a la Seguridad Social, liquidación a
//! liquidación y concepto a concepto.
use crate::dominio::fmt::DIF;
use super::empleo::{en_banda, fila, filas, filas_banda, parrafo, pct1, Fila};
use crate::dominio::Datos;
use crate::dominio::fmt::{self, r2};
use crate::dominio::fmt::nombre_mes;
use crate::informe::empleo::MesSueldo;
use crate::tui::texto::frase;
use crate::tui::widgets::{guia, tema, titulo_bloque};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Ancho de las columnas de importes de lo liquidado (sin «€»: «tu empresa» y un espacio de aire).
const COL: usize = 11;
/// Lo más ancho que se lee bien: hasta aquí llegan las cifras del recibo y de lo liquidado.
const ANCHO: usize = 76;

/// La columna en la que acaban las cifras, en una ventana de `w` columnas (con una de aire a la derecha de la banda).
fn fin(w: usize) -> u16 {
    w.saturating_sub(1).min(ANCHO) as u16
}

/// Todo lo del mes.
pub(super) fn lineas(d: &Datos, m: &MesSueldo, w: usize) -> Vec<Fila> {
    let gris = tema::apagado();
    let fin = fin(w);
    let mut v = Vec::new();
    match &m.nomina {
        Some(n) => {
            let titulo = if n.empresas.is_empty() { "tu nómina".to_string() } else { format!("tu nómina · {}", n.empresas.join(", ")) };
            v.extend(filas(parrafo(&titulo, w, gris)));
            // El recibo, con cada cifra acabando en `fin`; y lo que te llegó en dinero (el bruto menos lo de en
            // especie, el IRPF y tu Seguridad Social), en una banda con la cifra en la misma columna.
            let linea = |etq: &str, x: f64, st: Style| Fila::from(guia(Span::styled(etq.to_string(), st), fmt::eur(x), st, fin));
            v.push(linea("bruto", n.bruto, Style::new()));
            if n.especie.abs() > DIF {
                v.push(linea("  de él, en especie", n.especie, gris));
            }
            v.push(linea(&format!("− IRPF ({})", pct1(n.retenido_pct())), n.irpf, Style::new()));
            v.push(linea("− Seguridad Social", n.ss, Style::new()));
            let neto = r2(n.bruto - n.especie - n.irpf - n.ss);
            v.extend(filas_banda(en_banda("te llegó en dinero", fmt::eur(neto), tema::BIEN, fin), tema::SUAVE_BIEN, (fin + 1).min(w as u16)));
        }
        None => {
            v.push(titulo_bloque("tu nómina").into());
            v.extend(filas(parrafo(&format!("No has apuntado la nómina de {}: pulsa n para apuntarla.", nombre_mes(&m.ym)), w, gris)));
        }
    }
    v.push(Fila::default());
    v.push(titulo_bloque("lo liquidado a la Seguridad Social").into());
    match d.tgss.nomina.meses.get(&m.ym).filter(|l| !l.is_empty()) {
        Some(_) => v.extend(liquidaciones(d, &m.ym, w)),
        None => v.extend(filas(parrafo("La Seguridad Social no tiene ninguna liquidación de tu nómina de este mes.", w, gris))),
    }
    v
}

/// Cada liquidación del mes con sus conceptos (sin las líneas de «líquido», que repiten), en una tabla sin caja: la
/// cabecera en gris y las cifras acabando, como las del recibo, en `fin` (el «€» del recibo queda fuera).
fn liquidaciones(d: &Datos, ym: &str, w: usize) -> Vec<Fila> {
    let Some(lista) = d.tgss.nomina.meses.get(ym) else { return Vec::new() };
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut v = Vec::new();
    // La tabla, con sus cifras bajo las del recibo; y las columnas que caben (con 21 para el concepto, lo que mide
    // «Contingencias comunes»): la base y el total, si hay sitio.
    let ancho = (fin(w) as usize).saturating_sub(2);
    let cabe = |n: usize| 21 + n * (COL + 1) <= ancho;
    let con_total = cabe(4);
    let con_base = cabe(if con_total { 4 } else { 3 });
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
        v.push(fila("concepto", cab, &cols("base".into(), "tu empresa".into(), "tú".into(), "total".into(), cab), ancho));
        let o = |x: f64| if x.abs() > DIF { fmt::num(x) } else { "—".into() };
        for c in l.conceptos.iter().filter(|c| !c.es_total()) {
            v.push(fila(&frase(&c.concepto), Style::new(), &cols(o(c.base), fmt::num(c.empresa), o(c.trabajador), fmt::num(c.total), Style::new()), ancho));
        }
        let total = r2(l.cuota_empresa + l.cuota_trabajador);
        v.push(fila("total", negrita, &cols(String::new(), fmt::num(l.cuota_empresa), fmt::num(l.cuota_trabajador), fmt::num(total), negrita), ancho));
    }
    v
}
