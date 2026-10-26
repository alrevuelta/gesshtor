//! La ficha de una declaración (Enter en la lista): cómo la ha leído la app y sus casillas tal como salen en el
//! PDF (la lectura genérica, de cualquier modelo y año). Las casillas se sacan al abrir la ficha y se guardan por
//! PDF: la segunda vez es instantánea.
use super::declaraciones::{codigo, nombre_modelo, que_es, Lectura};
use crate::tui::texto::cuando;
use crate::dominio::fmt;
use crate::dominio::hacienda::Tipo;
use crate::hacienda::{Decl, Modelo};
use crate::hacienda::{generico, pdf};
use crate::tui::widgets::{bloque, derecha, tema};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

type Casillas = Result<Rc<Vec<generico::CasillaImpresa>>, String>;

/// Las casillas impresas de cada PDF ya mirado: ruta → (tamaño, casillas o por qué no se pueden sacar).
#[derive(Default)]
pub(super) struct Memo(HashMap<PathBuf, (u64, Casillas)>);

impl Memo {
    pub fn casillas(&mut self, p: &Path) -> Casillas {
        let tam = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if let Some((t, c)) = self.0.get(p)
            && *t == tam
        {
            return c.clone();
        }
        let c = pdf::abrir(p).map(|d| Rc::new(generico::casillas(&d)));
        self.0.insert(p.to_path_buf(), (tam, c.clone()));
        c
    }
}

/// La ficha abierta.
pub(super) struct Detalle {
    pub d: Decl,
    pub pdf: Option<PathBuf>,
    pub exacto: bool,
    pub lectura: Lectura,
    /// Primera casilla que se ve.
    pub desplaz: usize,
    /// Cuántas caben (de la última vez que se pintó), para PgUp/PgDn.
    pub alto: usize,
}

/// Cómo se presentó, según el tipo leído (en la Renta, cómo se pagó o se cobró).
pub(super) fn forma(modelo: Option<Modelo>, tipo: Option<Tipo>) -> &'static str {
    let Some(tipo) = tipo else { return "" };
    match (modelo, tipo) {
        (_, Tipo::Ingreso) => "pago en el banco (NRC)",
        (_, Tipo::Domiciliado) => "domiciliado",
        (_, Tipo::CuentaCorriente) => "ingreso en cuenta corriente tributaria",
        (_, Tipo::ReconocimientoDeuda) => "reconocimiento de deuda",
        (Some(Modelo::M303), Tipo::Compensar) => "a compensar",
        (Some(Modelo::M303 | Modelo::M100), Tipo::Devolver) => "a devolver",
        (Some(Modelo::M303), Tipo::DevolverEnCuentaCorriente) => "devolución en cuenta corriente tributaria",
        (Some(Modelo::M303), Tipo::DevolverAlExtranjero) => "devolución por transferencia al extranjero",
        (Some(Modelo::M303), Tipo::Cero) => "sin actividad o resultado cero",
        (Some(Modelo::M130 | Modelo::M100), Tipo::Cero) => "resultado cero",
        (Some(Modelo::M130), Tipo::Negativa) => "negativa",
        _ => "",
    }
}

impl Detalle {
    pub fn mover(&mut self, d: isize, total: usize) {
        let max = total.saturating_sub(self.alto.max(1));
        self.desplaz = (self.desplaz as isize + d).clamp(0, max as isize) as usize;
    }

    pub fn dibujar(&mut self, f: &mut Frame, area: Rect, casillas: Option<&Casillas>, tipo: Option<Tipo>) {
        let d = &self.d;
        let m = codigo(&d.modelo);
        let titulo = format!("{} · {}", que_es(d), nombre_modelo(&d.modelo, false));
        let b = bloque(&titulo);
        let dentro = b.inner(area);
        f.render_widget(b, area);

        let mut arriba = vec![Line::styled(format!("Presentada {} · justificante {}", cuando(&d.fecha), d.justificante), tema::apagado())];
        let nombre = self.pdf.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        arriba.push(match (&self.pdf, self.exacto) {
            (Some(_), true) => Line::from(vec![Span::styled("● ", Style::new().fg(tema::BIEN)), Span::styled(format!("PDF {nombre}"), tema::apagado())]),
            (Some(_), false) => Line::from(vec![Span::styled("○ ", Style::new().fg(tema::AVISO)), Span::styled(format!("PDF {nombre}, del mismo periodo (puede que de otra versión)"), tema::apagado())]),
            (None, _) => Line::styled("— Sin PDF: se descarga al traer lo último con tu certificado, y aquí se verán sus casillas.", tema::apagado()),
        });
        let como = forma(Modelo::de_codigo(m), tipo);
        let como = if como.is_empty() { String::new() } else { format!(" · {como}") };
        match &self.lectura {
            Lectura::Leida => arriba.push(Line::from(vec![Span::styled("✓ ", Style::new().fg(tema::BIEN)), Span::raw(format!("Leída: la app usa sus casillas{como}"))])),
            Lectura::Fallo(motivo) => arriba.push(Line::from(vec![
                Span::styled("✗ ", Style::new().fg(tema::MAL)),
                Span::styled(format!("No se puede leer: {motivo}"), Style::new().fg(tema::MAL)),
            ])),
            Lectura::Pendiente(true) => arriba.push(Line::styled("… Leyendo tus PDF de Hacienda", tema::apagado())),
            Lectura::Pendiente(false) => arriba.push(Line::styled("… La app aún no la ha leído", tema::apagado())),
            Lectura::NoLee | Lectura::SinPdf => {}
        }
        let [a_arriba, _, a_tabla, a_pie] =
            Layout::vertical([Constraint::Length(arriba.len() as u16), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(dentro);
        f.render_widget(Paragraph::new(arriba), a_arriba);

        let Some(c) = casillas else { return };
        let c = match c {
            Ok(c) => c,
            Err(e) => {
                f.render_widget(Paragraph::new(Line::styled(format!("No se pueden sacar las casillas: {e}"), Style::new().fg(tema::MAL))).wrap(Wrap { trim: true }), a_tabla);
                return;
            }
        };
        if c.is_empty() {
            f.render_widget(Paragraph::new(Line::styled("En este PDF no se ve ninguna casilla con importe.", tema::apagado())).wrap(Wrap { trim: true }), a_tabla);
            return;
        }
        // Una fila es la cabecera.
        self.alto = (a_tabla.height as usize).saturating_sub(1).max(1);
        self.mover(0, c.len());
        let pag = c.iter().any(|x| x.pagina != c[0].pagina);
        let ancho_imp = c.iter().map(|x| fmt::num(x.importe).chars().count()).max().unwrap_or(4).max(7) as u16;
        let filas: Vec<Row> = c
            .iter()
            .skip(self.desplaz)
            .take(self.alto)
            .map(|x| {
                let mut v = Vec::new();
                if pag {
                    v.push(Cell::from(Span::styled(x.pagina.to_string(), tema::apagado())));
                }
                v.push(Cell::from(Span::styled(x.numero.clone(), Style::new().add_modifier(Modifier::BOLD))));
                v.push(Cell::from(derecha(fmt::num(x.importe))));
                v.push(Cell::from(Span::raw(x.etiqueta.clone())));
                Row::new(v)
            })
            .collect();
        let mut cab = Vec::new();
        let mut anchos = Vec::new();
        if pag {
            cab.push(Cell::from("Pág."));
            anchos.push(Constraint::Length(4));
        }
        cab.extend([Cell::from("Casilla"), Cell::from(derecha("Importe")), Cell::from("Texto del impreso")]);
        anchos.extend([Constraint::Length(7), Constraint::Length(ancho_imp), Constraint::Min(8)]);
        f.render_widget(Table::new(filas, anchos).header(Row::new(cab).style(tema::cabecera_tabla())).column_spacing(1), a_tabla);
        let n = c.len();
        let pie = if n > self.alto {
            format!("{n} casillas con importe en el PDF · {}–{} · ↑↓ para ver más", self.desplaz + 1, (self.desplaz + self.alto).min(n))
        } else {
            format!("{n} casilla{} con importe en el PDF", if n == 1 { "" } else { "s" })
        };
        f.render_widget(Paragraph::new(Line::styled(pie, tema::apagado())), a_pie);
    }
}
