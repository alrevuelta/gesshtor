//! Empleo: dos entradas, una para ver y otra para apuntar. Las cifras salen de `informe::empleo`; aquí se pintan.
//!
//! - `empleo_sueldo`: el año (de tu Renta, tus nóminas o la Seguridad Social), si te retienen bien y mes a mes; con
//!   el detalle de cada mes (`empleo_mes`).
//! - `empleo_nominas`: las nóminas apuntadas, una por mes, en cualquier año desde 2020.

use crate::informe::empleo::{empleo_anio, EmpleoAnio};
use crate::tui::widgets::tema;
use crate::tui::texto::partir;
use crate::tui::{Ctx, Pantalla};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::Frame;
use super::{empleo_nominas as nominas, empleo_sueldo as sueldo};


pub fn pantallas() -> Vec<Box<dyn Pantalla>> {
    vec![Box::new(sueldo::Sueldo::default()), Box::new(nominas::Nominas::default())]
}

/// Lo calculado para el año que se ve, guardado hasta que cambien los datos (`ctx.generacion`) o el año.
#[derive(Default)]
pub(super) struct Cache {
    pub(super) clave: Option<(u64, i32)>,
    pub(super) anio: EmpleoAnio,
}

impl Cache {
    /// Recalcula lo que haga falta. Devuelve true si ha cambiado el año o los datos.
    pub(super) fn al_dia(&mut self, ctx: &Ctx) -> bool {
        let clave = (ctx.generacion, ctx.anio);
        if self.clave == Some(clave) {
            return false;
        }
        self.clave = Some(clave);
        self.anio = empleo_anio(&ctx.datos, ctx.anio);
        true
    }

}

/// «39,6 %» (un decimal, sin «,0»).
pub(super) fn pct1(x: f64) -> String {
    let s = format!("{:.1}", (x * 10.0).round() / 10.0).replace('.', ",");
    format!("{} %", s.trim_end_matches(",0"))
}

/// Las líneas de un texto partido, con un estilo.
pub(super) fn parrafo(s: &str, w: usize, estilo: Style) -> Vec<Line<'static>> {
    partir(s, w).into_iter().map(|l| Line::styled(l, estilo)).collect()
}

/// Un importe que no se parte al cortar las líneas (espacio duro antes del €), para `con_importes`.
pub(super) fn eur_duro(x: f64) -> String {
    crate::dominio::fmt::eur(x).replace(' ', "\u{a0}")
}

/// Como `parrafo`, con los importes escritos con `eur_duro` en negrita.
pub(super) fn con_importes(s: &str, w: usize, estilo: Style) -> Vec<Line<'static>> {
    partir(s, w)
        .into_iter()
        .map(|l| {
            let mut v = Vec::new();
            for (i, p) in l.split(' ').enumerate() {
                if i > 0 {
                    v.push(Span::styled(" ", estilo));
                }
                let st = if p.contains('\u{a0}') { estilo.add_modifier(ratatui::style::Modifier::BOLD) } else { estilo };
                v.push(Span::styled(p.replace('\u{a0}', " "), st));
            }
            Line::from(v)
        })
        .collect()
}

/// Una fila de un panel: un texto a la izquierda y, si hay, columnas alineadas a la derecha. Se pintan por
/// separado y solo las filas que se ven, sin los espacios de en medio (pintar cuesta por carácter).
#[derive(Debug, Clone, Default)]
pub(super) struct Fila {
    izq: Line<'static>,
    der: Option<Line<'static>>,
}

impl From<Line<'static>> for Fila {
    fn from(izq: Line<'static>) -> Self {
        Fila { izq, der: None }
    }
}

/// Las líneas de un texto, como filas.
pub(super) fn filas(v: Vec<Line<'static>>) -> Vec<Fila> {
    v.into_iter().map(Fila::from).collect()
}

/// Una fila de `w` caracteres: a la izquierda un texto (recortado si no cabe) y a la derecha unas columnas de
/// ancho fijo.
pub(super) fn fila(izq: &str, estilo: Style, cols: &[(String, usize, Style)], w: usize) -> Fila {
    let derecha: usize = cols.iter().map(|c| c.1 + 1).sum();
    let libre = w.saturating_sub(derecha);
    let mut et: String = izq.to_string();
    if et.chars().count() > libre {
        et = et.chars().take(libre.saturating_sub(1)).collect::<String>() + "…";
    }
    let der = cols.iter().map(|(txt, ancho, st)| Span::styled(format!(" {txt:>ancho$}"), *st)).collect::<Vec<_>>();
    Fila { izq: Line::styled(et, estilo), der: Some(Line::from(der).right_aligned()) }
}

/// Pinta un recuadro con las filas que caben, desde la `desde`.
pub(super) fn pintar_filas(f: &mut Frame, area: Rect, b: ratatui::widgets::Block, v: &[Fila], desde: usize) {
    let dentro = b.inner(area);
    f.render_widget(b, area);
    for (i, fl) in v.iter().skip(desde).take(dentro.height as usize).enumerate() {
        let r = Rect { y: dentro.y + i as u16, height: 1, ..dentro };
        pintar_linea(f, r, &fl.izq);
        if let Some(d) = &fl.der {
            pintar_linea(f, r, d);
        }
    }
}

/// ¿Ocupa una sola columna? (letras, números y signos de los que se usan aquí: tildes, €, «», —, …, flechas,
/// barras). Lo demás se deja a ratatui.
fn ancho_uno(c: char) -> bool {
    matches!(c, ' '..='~' | '\u{a0}'..='\u{2ff}' | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205e}' | '\u{20a0}'..='\u{20bf}' | '\u{2190}'..='\u{21ff}' | '\u{2500}'..='\u{259f}')
}

/// Pinta una línea en la primera fila de `r`, con su alineación. Como `Line::render`, pero carácter a carácter
/// cuando todos ocupan una columna: así no hay que partir el texto en grafemas, que en debug es lo que más
/// cuesta de pintar una pantalla.
pub(super) fn pintar_linea(f: &mut Frame, r: Rect, l: &Line) {
    pintar_alineada(f, r, l, false);
}

/// `pintar_linea`, a la derecha si `derecha` (aunque la línea no lo diga).
fn pintar_alineada(f: &mut Frame, r: Rect, l: &Line, derecha: bool) {
    if r.width == 0 || r.height == 0 {
        return;
    }
    let alineacion = if derecha { Some(ratatui::layout::Alignment::Right) } else { l.alignment };
    if !l.spans.iter().all(|s| s.content.chars().all(ancho_uno)) {
        let mut l = l.clone();
        l.alignment = alineacion;
        f.render_widget(&l, Rect { height: 1, ..r });
        return;
    }
    let ancho: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
    let sobra = r.width.saturating_sub(ancho.min(u16::MAX as usize) as u16);
    let mut x = match alineacion {
        Some(ratatui::layout::Alignment::Right) => r.x + sobra,
        Some(ratatui::layout::Alignment::Center) => r.x + sobra / 2,
        _ => r.x,
    };
    let fin = r.x + r.width;
    let buf = f.buffer_mut();
    for s in &l.spans {
        let st = l.style.patch(s.style);
        for c in s.content.chars() {
            if x >= fin {
                return;
            }
            if let Some(celda) = buf.cell_mut((x, r.y)) {
                celda.set_char(c).set_style(st);
            }
            x += 1;
        }
    }
}

/// Una columna de una `tabla`: su ancho (None: lo que sobre) y si va a la derecha.
#[derive(Debug, Clone, Copy)]
pub(super) struct Col {
    ancho: Option<u16>,
    derecha: bool,
}

pub(super) const fn col(ancho: u16) -> Col {
    Col { ancho: Some(ancho), derecha: false }
}
pub(super) const fn col_der(ancho: u16) -> Col {
    Col { ancho: Some(ancho), derecha: true }
}
pub(super) const RESTO: Col = Col { ancho: None, derecha: false };

/// Una fila de una `tabla`: cada celda con sus líneas (las filas pueden tener más de una).
#[derive(Debug, Clone, Default)]
pub(super) struct FilaT {
    celdas: Vec<Vec<Line<'static>>>,
    estilo: Style,
}

impl FilaT {
    pub(super) fn new(celdas: Vec<Line<'static>>) -> Self {
        FilaT { celdas: celdas.into_iter().map(|l| vec![l]).collect(), estilo: Style::new() }
    }
    pub(super) fn estilo(mut self, st: Style) -> Self {
        self.estilo = st;
        self
    }
    fn alto(&self) -> u16 {
        self.celdas.iter().map(|c| c.len()).max().unwrap_or(1).max(1) as u16
    }
}

/// Una tabla: sus columnas, la cabecera y las filas.
pub(super) struct Tabla<'a> {
    pub(super) cols: &'a [Col],
    pub(super) cab: &'a [&'a str],
    pub(super) filas: Vec<FilaT>,
}

/// Pinta una tabla con la fila elegida resaltada y desplazada para que se vea (`desde` es la primera fila que se
/// ve). Hecha a mano: lo justo de `Table`, pero mucho más rápida en debug.
pub(super) fn tabla(f: &mut Frame, area: Rect, b: ratatui::widgets::Block, t: &Tabla, sel: Option<usize>, desde: &mut usize) {
    let (cols, cab, filas) = (t.cols, t.cab, &t.filas);
    let dentro = b.inner(area);
    f.render_widget(b, area);
    if dentro.height == 0 || dentro.width == 0 {
        return;
    }
    // Las x de cada columna (un espacio entre columnas; la de «lo que sobre» se lleva el resto).
    let fijo: u16 = cols.iter().map(|c| c.ancho.unwrap_or(0)).sum::<u16>() + cols.len().saturating_sub(1) as u16;
    let resto = dentro.width.saturating_sub(fijo);
    let mut xs = Vec::with_capacity(cols.len());
    let mut x = dentro.x;
    let fin = dentro.x + dentro.width;
    for c in cols {
        let w = c.ancho.unwrap_or(resto).min(fin.saturating_sub(x));
        xs.push((x, w));
        x = (x + w + 1).min(fin);
    }
    let celda = |f: &mut Frame, i: usize, y: u16, l: &Line| {
        let (x, w) = xs[i];
        pintar_alineada(f, Rect { x, y, width: w, height: 1 }, l, cols[i].derecha);
    };
    for (i, t) in cab.iter().enumerate().take(cols.len()) {
        celda(f, i, dentro.y, &Line::styled(*t, tema::cabecera_tabla()));
    }
    let n = filas.len();
    if n == 0 {
        return;
    }
    // Que la elegida se vea.
    let alto = dentro.height.saturating_sub(1);
    let sel = sel.map(|s| s.min(n - 1));
    *desde = (*desde).min(n - 1);
    if let Some(s) = sel {
        *desde = (*desde).min(s);
        while *desde < s && filas[*desde..=s].iter().map(FilaT::alto).sum::<u16>() > alto {
            *desde += 1;
        }
    }
    let mut y = dentro.y + 1;
    for (i, fl) in filas.iter().enumerate().skip(*desde) {
        let h = fl.alto();
        if y + h > dentro.y + dentro.height {
            break;
        }
        let r = Rect { x: dentro.x, y, width: dentro.width, height: h };
        if fl.estilo != Style::new() {
            f.buffer_mut().set_style(r, fl.estilo);
        }
        for (j, lineas) in fl.celdas.iter().enumerate().take(cols.len()) {
            for (k, l) in lineas.iter().enumerate() {
                celda(f, j, y + k as u16, l);
            }
        }
        if sel == Some(i) {
            f.buffer_mut().set_style(r, tema::seleccion());
        }
        y += h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piezas() {
        assert_eq!([pct1(39.56), pct1(20.0), pct1(0.0)], ["39,6 %", "20 %", "0 %"]);
        let l = fila("Contingencias comunes", Style::new(), &[("1,00 €".into(), 8, Style::new())], 20);
        let izq: String = l.izq.spans.iter().map(|s| s.content.as_ref()).collect();
        let der: String = l.der.unwrap().spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!((izq.as_str(), der.as_str()), ("Contingenc…", "   1,00 €"));
    }
}
