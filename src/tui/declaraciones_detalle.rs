//! La ficha de una declaración (Enter en la lista): cómo la ha leído la app y sus casillas tal como salen en el
//! PDF (la lectura genérica, de cualquier modelo y año). Las casillas se sacan al abrir la ficha y se guardan por
//! PDF: la segunda vez es instantánea.
use super::declaraciones::{codigo, nombre_modelo, que_es, Lectura};
use crate::tui::texto::cuando;
use crate::dominio::fmt;
use crate::dominio::hacienda::Tipo;
use crate::hacienda::{Decl, Modelo};
use crate::hacienda::{generico, pdf};
use crate::tui::widgets::{derecha, filas_partidas, tema};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

type Casillas = Result<Rc<Vec<generico::CasillaImpresa>>, String>;

/// Lo más ancho que se pinta la ficha: más, y el importe queda lejos de su casilla.
const ANCHO: u16 = 88;

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

    /// La ficha, sin caja y no más ancha de lo que se lee bien: arriba, qué es (en el color de la sección), cuándo se
    /// presentó, su PDF y cómo lo ha leído la app; debajo, sus casillas (cabecera en gris, importes a la derecha) y
    /// cuántas son.
    pub fn dibujar(&mut self, f: &mut Frame, area: Rect, casillas: Option<&Casillas>, tipo: Option<Tipo>) {
        let area = Rect { width: area.width.min(ANCHO), ..area };
        let d = &self.d;
        let m = codigo(&d.modelo);
        let gris = tema::apagado();
        let mut arriba = vec![
            Line::styled(format!("{} · {}", que_es(d), nombre_modelo(&d.modelo, false)), tema::titulo()),
            Line::styled(format!("Presentada {} · justificante {}", cuando(&d.fecha), d.justificante), gris),
            Line::default(),
        ];
        let nombre = self.pdf.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        arriba.push(match (&self.pdf, self.exacto) {
            (Some(_), true) => Line::from(vec![Span::styled("● ", Style::new().fg(tema::BIEN)), Span::styled(format!("PDF {nombre}"), gris)]),
            (Some(_), false) => Line::from(vec![Span::styled("○ ", Style::new().fg(tema::AVISO)), Span::styled(format!("PDF {nombre}, del mismo periodo (puede que de otra versión)"), gris)]),
            (None, _) => Line::styled("— Sin PDF: se descarga al traer lo último con tu certificado, y aquí se verán sus casillas.", gris),
        });
        let como = forma(Modelo::de_codigo(m), tipo);
        let como = if como.is_empty() { String::new() } else { format!(" · {como}") };
        match &self.lectura {
            Lectura::Leida => arriba.push(Line::from(vec![Span::styled("✓ ", Style::new().fg(tema::BIEN)), Span::raw(format!("Leída: la app usa sus casillas{como}"))])),
            Lectura::Fallo(motivo) => arriba.push(Line::from(vec![
                Span::styled("✗ ", Style::new().fg(tema::MAL)),
                Span::styled(format!("No se puede leer: {motivo}"), Style::new().fg(tema::MAL)),
            ])),
            Lectura::Pendiente(true) => arriba.push(Line::styled("… Leyendo tus PDF de Hacienda", gris)),
            Lectura::Pendiente(false) => arriba.push(Line::styled("… La app aún no la ha leído", gris)),
            Lectura::NoLee | Lectura::SinPdf => {}
        }
        let alto_arriba = arriba.iter().map(|l| filas_partidas(&l.to_string(), area.width as usize)).sum::<u16>();
        let [a_arriba, _, a_tabla, a_pie] =
            Layout::vertical([Constraint::Length(alto_arriba), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(area);
        f.render_widget(Paragraph::new(arriba).wrap(Wrap { trim: false }), a_arriba);

        let Some(c) = casillas else { return };
        let c = match c {
            Ok(c) => c,
            Err(e) => {
                f.render_widget(Paragraph::new(Line::styled(format!("No se pueden sacar las casillas: {e}"), Style::new().fg(tema::MAL))).wrap(Wrap { trim: true }), a_tabla);
                return;
            }
        };
        if c.is_empty() {
            f.render_widget(Paragraph::new(Line::styled("En este PDF no se ve ninguna casilla con importe.", gris)).wrap(Wrap { trim: true }), a_tabla);
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
                    v.push(Cell::from(Span::styled(x.pagina.to_string(), gris)));
                }
                v.push(Cell::from(Span::styled(x.numero.clone(), Style::new().add_modifier(Modifier::BOLD))));
                v.push(Cell::from(Span::raw(x.etiqueta.clone())));
                v.push(Cell::from(derecha(fmt::num(x.importe))));
                Row::new(v)
            })
            .collect();
        let mut cab = Vec::new();
        let mut anchos = Vec::new();
        if pag {
            cab.push(Cell::from("pág."));
            anchos.push(Constraint::Length(4));
        }
        cab.extend([Cell::from("casilla"), Cell::from("concepto"), Cell::from(derecha("importe"))]);
        anchos.extend([Constraint::Length(7), Constraint::Fill(1), Constraint::Length(ancho_imp)]);
        f.render_widget(Table::new(filas, anchos).header(Row::new(cab).style(tema::cabecera_tabla())).column_spacing(2), a_tabla);
        let n = c.len();
        let pie = if n > self.alto {
            format!("{n} casillas con importe en el PDF · {}–{} · ↑↓ para ver más", self.desplaz + 1, (self.desplaz + self.alto).min(n))
        } else {
            format!("{n} casilla{} con importe en el PDF", if n == 1 { "" } else { "s" })
        };
        f.render_widget(Paragraph::new(Line::styled(pie, gris)), a_pie);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// La ficha, sin caja: qué es, cuándo se presentó, su PDF, cómo se ha leído y sus casillas (con la página si
    /// son de varias), que se recorren con ↑↓ sin salirse.
    #[test]
    fn la_ficha_con_sus_casillas() {
        crate::tui::widgets::tema::poner_seccion('r');
        let d = Decl { modelo: "303".into(), ejercicio: 2026, periodo: "3T".into(), justificante: "3030000000003".into(), fecha: "2026-10-14T11:05:00".into(), ..Default::default() };
        let mut x = Detalle { d, pdf: Some(PathBuf::from("justificantes/2026_3T_303_CSV.pdf")), exacto: true, lectura: Lectura::Leida, desplaz: 0, alto: 1 };
        let c: Vec<generico::CasillaImpresa> = (1..=40)
            .map(|k| generico::CasillaImpresa { pagina: 1 + k / 30, numero: format!("{k:02}"), etiqueta: format!("Casilla de prueba número {k}"), importe: k as f64 * 100.5 })
            .collect();
        let c: Casillas = Ok(Rc::new(c));
        for (w, h) in [(96, 30), (76, 19)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| x.dibujar(f, f.area(), Some(&c), Some(Tipo::Domiciliado))).unwrap();
            let p: String = t.backend().buffer().content.chunks(w as usize).map(|f| f.iter().map(|c| c.symbol()).collect::<String>().trim_end().to_string() + "\n").collect();
            if std::env::var("VER").is_ok() {
                println!("{p}");
            }
            let filas: Vec<&str> = p.lines().collect();
            assert!(filas[0].starts_with("303 3T 2026 · IVA"), "{p}");
            assert!(p.contains("Leída: la app usa sus casillas · domiciliado") && p.contains("● PDF 2026_3T_303_CSV.pdf"), "{p}");
            assert!(p.contains("pág.") && p.contains("casilla") && p.contains("concepto") && p.contains("importe"), "{p}");
            assert!(filas.iter().any(|l| l.contains("Casilla de prueba número 1 ") && l.ends_with("100,50")), "el importe, a la derecha:\n{p}");
            assert!(filas[h as usize - 1].starts_with("40 casillas con importe en el PDF · 1–"), "{p}");
            assert!(!p.contains('┌') && !p.contains('│'), "sin caja:\n{p}");
        }
        // Al final, la última se ve y no se pasa.
        x.mover(isize::MAX / 2, 40);
        assert_eq!(x.desplaz + x.alto, 40);
    }
}
