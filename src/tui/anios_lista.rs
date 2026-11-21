//! La lista de «Años», sin cajas: arriba un gráfico (lo que ganas y pagas por conceptos, lo mismo en sencillo, o lo
//! que salió en cada Renta) y debajo una tabla, un año por fila: lo que ganaste por partes, lo que pagaste, lo que
//! te quedó y lo que salió en tu Renta. Los años sin Renta (también el que está en curso) van en gris y con «*»:
//! sus cifras no son las de Hacienda.
use crate::dominio::fmt::DIF;
use super::anios::{color_resultado, minuscula, Anios, COLOR_EMPRESA, COLOR_PAGAS, COLOR_QUEDA};
use super::anios_grafico::{self as grafico, Modo};
use super::anios_renta as renta;
use crate::dominio::fmt::eur0;
use crate::dominio::PRIMER_ANIO;
use crate::informe::anio::{Anio, Concepto};
use crate::tui::Ctx;
use crate::tui::widgets::{derecha, tema};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};

/// Lo más alto que se hace el gráfico: lo justo para leerlo, sin que se coma la pantalla.
const ALTO_GRAFICO: u16 = 16;

/// Qué enseña el gráfico (`g` pasa al siguiente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Grafico {
    #[default]
    Conceptos,
    Sencillo,
    Renta,
}

impl Grafico {
    pub(super) fn siguiente(self) -> Grafico {
        match self {
            Grafico::Conceptos => Grafico::Sencillo,
            Grafico::Sencillo => Grafico::Renta,
            Grafico::Renta => Grafico::Conceptos,
        }
    }

    pub(super) fn nombre(self) -> &'static str {
        match self {
            Grafico::Conceptos => "por conceptos",
            Grafico::Sencillo => "sencillo",
            Grafico::Renta => "lo de la Renta",
        }
    }
}

/// «1.240 € a pagar» (en AVISO), «310 € a devolver» (en verde), «a cero».
fn resultado(x: f64) -> (String, Style) {
    if x > DIF {
        (format!("{} a pagar", eur0(x)), color_resultado(x))
    } else if x < -DIF {
        (format!("{} a devolver", eur0(-x)), color_resultado(x))
    } else {
        ("a cero".into(), Style::new())
    }
}

impl Anios {
    /// El gráfico y, debajo, la tabla (con una nota debajo de lo que lleva «*» y de la Renta que falta).
    pub(super) fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if self.serie.is_empty() {
            let t = vec![
                Line::styled(format!("Aún no hay ningún año con datos desde {PRIMER_ANIO}."), Style::new().add_modifier(Modifier::BOLD)),
                Line::from(""),
                Line::styled("Salen de tus Rentas, la Seguridad Social y lo que apuntes (nóminas, facturas, gastos).", tema::apagado()),
            ];
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), area);
            return;
        }
        let n = self.serie.len() as u16;
        let nota = self.nota();
        // La tabla entera (cabecera, años y la nota) si cabe; el gráfico, en lo que quede (con una fila de aire).
        let alto_tabla = (n + 1 + u16::from(nota.is_some())).min(area.height / 2).max(area.height.min(6));
        let libre = area.height.saturating_sub(alto_tabla + 1);
        // El de la Renta, lo justo: el título, un año por fila y la leyenda.
        let tope = if self.grafico == Grafico::Renta { n + 2 } else { ALTO_GRAFICO };
        let alto_graf = if libre < 6 { 0 } else { libre.min(tope) };
        let hueco = u16::from(alto_graf > 0);
        let [a_graf, _, a_tabla] = Layout::vertical([Constraint::Length(alto_graf), Constraint::Length(hueco), Constraint::Min(0)]).areas(area);
        if alto_graf > 0 {
            let siguiente = self.grafico.siguiente().nombre();
            match self.grafico {
                Grafico::Conceptos => grafico::dibujar(f, a_graf, &self.serie, self.sel, Modo::Conceptos, siguiente),
                Grafico::Sencillo => grafico::dibujar(f, a_graf, &self.serie, self.sel, Modo::Sencillo, siguiente),
                Grafico::Renta => {
                    let datos: Vec<(i32, Option<f64>)> = self.serie.iter().zip(&self.resultados).map(|(a, r)| (a.y, *r)).collect();
                    renta::dibujar_grafico(f, a_graf, &datos, self.serie.get(self.sel).map(|a| a.y), siguiente);
                }
            }
        }
        // La nota, justo debajo de la tabla (si hay sitio para ella y algún año).
        let con_nota = nota.is_some() && a_tabla.height >= 3;
        let alto_filas = (n + 1).min(a_tabla.height - u16::from(con_nota));
        let [a_filas, a_nota] = Layout::vertical([Constraint::Length(alto_filas), Constraint::Length(u16::from(con_nota))]).areas(a_tabla);
        self.dibujar_tabla(f, a_filas, ctx);
        if let Some(l) = nota.filter(|_| con_nota) {
            f.render_widget(Paragraph::new(l), a_nota);
        }
    }

    /// Debajo de la tabla: qué quiere decir «*» y, si falta, hasta cuándo se presenta la Renta del año pasado.
    fn nota(&self) -> Option<Line<'static>> {
        let mut v: Vec<Span> = Vec::new();
        if self.serie.iter().any(|a| !a.con_renta) {
            v.push(Span::styled("* sin Renta: con la Seguridad Social y lo apuntado", tema::apagado()));
        }
        if let Some((y, fin)) = &self.pendiente {
            if !v.is_empty() {
                v.push(Span::styled("   ·   ", Style::new().fg(tema::PUNTOS)));
            }
            v.push(Span::styled(format!("Renta {y}: se presenta hasta el {}", crate::dominio::fmt::fecha_corta(fin)), Style::new().fg(tema::AVISO)));
        }
        (!v.is_empty()).then(|| Line::from(v))
    }

    /// La tabla, sin caja: la cabecera en gris y en minúscula, las cifras a la derecha y el año elegido en el fondo
    /// suave de la sección. Si no caben todas las columnas, se quitan las menos importantes.
    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let serie = &self.serie;
        let hay = |x: &dyn Fn(&Anio) -> f64| serie.iter().any(|a| x(a).abs() > DIF);
        let importe = |x: f64, st: Style| if x.abs() <= DIF { ("—".to_string(), tema::apagado()) } else { (eur0(x), st) };
        // Columnas: (título, prioridad: 0 siempre, a la derecha, texto y estilo de cada año).
        type Col<'a> = (String, u8, bool, Box<dyn Fn(usize, &Anio) -> (String, Style) + 'a>);
        let mut cols: Vec<Col> = Vec::new();
        for c in Concepto::GANAS {
            if hay(&|a| a.de(c)) {
                let prio = if matches!(c, Concepto::Sueldo | Concepto::Actividad) { 2 } else { 3 };
                cols.push((minuscula(c.corto()), prio, true, Box::new(move |_, a: &Anio| importe(a.de(c), Style::new()))));
            }
        }
        cols.push(("ganas".into(), 0, true, Box::new(move |_, a: &Anio| importe(a.ganas, Style::new().add_modifier(Modifier::BOLD)))));
        cols.push(("IRPF".into(), 3, true, Box::new(move |_, a: &Anio| importe(a.de(Concepto::Irpf), Style::new()))));
        if hay(&|a| a.seguridad_social()) {
            cols.push(("Seg. Social".into(), 3, true, Box::new(move |_, a: &Anio| importe(a.seguridad_social(), Style::new()))));
        }
        cols.push(("pagas".into(), 0, true, Box::new(move |_, a: &Anio| importe(a.pagas, Style::new().fg(COLOR_PAGAS)))));
        cols.push(("te queda".into(), 1, true, Box::new(move |_, a: &Anio| importe(a.queda, Style::new().fg(if a.queda < -DIF { tema::AVISO } else { COLOR_QUEDA })))));
        cols.push(("%".into(), 4, true, Box::new(|_, a: &Anio| if a.ganas > DIF { (format!("{:.0} %", a.queda / a.ganas * 100.0), tema::apagado()) } else { (String::new(), tema::apagado()) })));
        let resultados = &self.resultados;
        let pendiente = self.pendiente.as_ref().map(|p| p.0);
        let hoy = crate::dominio::fmt::anio_actual();
        cols.push((
            "Renta".into(),
            1,
            true,
            Box::new(move |i, a: &Anio| match resultados[i] {
                Some(x) => resultado(x),
                None if Some(a.y) == pendiente => ("pendiente".into(), Style::new().fg(tema::AVISO)),
                None if a.y >= hoy => ("en curso".into(), Style::new().fg(tema::acento())),
                None => ("sin Renta".into(), tema::apagado()),
            }),
        ));
        if hay(&|a| a.empresa) {
            cols.push(("tu empresa".into(), 4, true, Box::new(move |_, a: &Anio| importe(a.empresa, Style::new().fg(COLOR_EMPRESA)))));
        }
        let notas = |_, a: &Anio| {
            let mut v = Vec::new();
            if a.conjunta {
                v.push("conjunta");
            }
            if a.falta_nomina {
                v.push("sin el IRPF de tu nómina");
            }
            if a.sueldo_aprox {
                v.push("sueldo aproximado");
            }
            (v.join(" · "), tema::apagado())
        };
        cols.push(("notas".into(), 5, false, Box::new(notas)));

        let celdas: Vec<Vec<(String, Style)>> = serie.iter().enumerate().map(|(i, a)| cols.iter().map(|c| (c.3)(i, a)).collect()).collect();
        let anchos: Vec<usize> = (0..cols.len()).map(|i| celdas.iter().map(|c| c[i].0.chars().count()).max().unwrap_or(0).max(cols[i].0.chars().count())).collect();
        // Las columnas que quepan, por prioridad (el año ocupa 5).
        let mut usado = 5;
        let mut ver = vec![false; cols.len()];
        let mut orden: Vec<usize> = (0..cols.len()).collect();
        orden.sort_by_key(|&i| cols[i].1);
        for i in orden {
            if cols[i].1 == 0 || usado + anchos[i] + 2 <= area.width as usize {
                ver[i] = true;
                usado += anchos[i] + 2;
            }
        }
        let vis: Vec<usize> = (0..cols.len()).filter(|&i| ver[i]).collect();
        let cab = Row::new(std::iter::once(Cell::from("año")).chain(vis.iter().map(|&i| Cell::from(if cols[i].2 { derecha(cols[i].0.clone()) } else { Line::from(cols[i].0.clone()) })))).style(tema::cabecera_tabla());
        let filas: Vec<Row> = serie
            .iter()
            .zip(&celdas)
            .zip(resultados)
            .map(|((a, c), r)| {
                // Sin Renta, en gris: no son las cifras de Hacienda.
                let oficial = r.is_some();
                let marca = if oficial { "" } else { "*" };
                let st = if a.y == ctx.anio { tema::titulo() } else if oficial { Style::new().add_modifier(Modifier::BOLD) } else { tema::apagado() };
                let mut v = vec![Cell::from(Span::styled(format!("{}{marca}", a.y), st))];
                for &i in &vis {
                    let (t, st) = c[i].clone();
                    let st = if oficial || cols[i].0 == "Renta" { st } else { tema::apagado() };
                    v.push(Cell::from(if cols[i].2 { derecha(t).style(st) } else { Line::styled(t, st) }));
                }
                Row::new(v)
            })
            .collect();
        let w: Vec<Constraint> = std::iter::once(Constraint::Length(5)).chain(vis.iter().map(|&i| Constraint::Length(anchos[i] as u16))).collect();
        let t = Table::new(filas, w).header(cab).column_spacing(2).row_highlight_style(tema::seleccion());
        self.estado.select(Some(self.sel));
        f.render_stateful_widget(t, area, &mut self.estado);
    }
}
