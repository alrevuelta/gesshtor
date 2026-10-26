//! La lista de «Años»: arriba un gráfico (lo que ganas y pagas por conceptos, lo mismo en sencillo, o lo que salió
//! en cada Renta) y debajo una tabla, un año por fila: lo que ganaste por partes, lo que pagaste, lo que te quedó y
//! lo que salió en tu Renta. Los años sin Renta van en gris: sus cifras no son las de Hacienda.
use crate::dominio::fmt::DIF;
use super::anios::{Anios, COLOR_EMPRESA, COLOR_PAGAS, COLOR_QUEDA};
use super::anios_grafico::{self as grafico, Modo};
use super::anios_renta as renta;
use crate::dominio::fmt::eur0;
use crate::dominio::PRIMER_ANIO;
use crate::informe::anio::{Anio, Concepto};
use crate::tui::Ctx;
use crate::tui::widgets::{bloque, derecha, tema};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

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

/// «1.240 € a pagar», «310 € a devolver», «a cero».
fn resultado(x: f64) -> (String, Style) {
    if x > DIF {
        (format!("{} a pagar", eur0(x)), Style::new().fg(tema::MAL))
    } else if x < -DIF {
        (format!("{} a devolver", eur0(-x)), Style::new().fg(tema::BIEN))
    } else {
        ("a cero".into(), Style::new())
    }
}

impl Anios {
    /// El gráfico y la tabla.
    pub(super) fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if self.serie.is_empty() {
            let t = vec![
                Line::styled(format!("Aún no hay ningún año acabado con datos desde {PRIMER_ANIO}."), Style::new().add_modifier(Modifier::BOLD)),
                Line::from(""),
                Line::styled("Lo de este año está en «Este año». Los acabados salen de tus Rentas, la Seguridad Social y lo que apuntes.", tema::apagado()),
            ];
            f.render_widget(Paragraph::new(t).wrap(ratatui::widgets::Wrap { trim: true }).block(bloque("Años")), area);
            return;
        }
        let n = self.serie.len() as u16;
        let alto_tabla = (n + 3).min(area.height / 2).max(area.height.min(6));
        let alto_graf = area.height - alto_tabla;
        let (alto_tabla, alto_graf) = if alto_graf < 6 { (area.height, 0) } else { (alto_tabla, alto_graf) };
        let [a_graf, a_tabla] = Layout::vertical([Constraint::Length(alto_graf), Constraint::Length(alto_tabla)]).areas(area);
        if alto_graf > 0 {
            let pie = format!(" g: {} ", self.grafico.siguiente().nombre());
            match self.grafico {
                Grafico::Conceptos => grafico::dibujar(f, a_graf, &self.serie, self.sel, Modo::Conceptos, &pie),
                Grafico::Sencillo => grafico::dibujar(f, a_graf, &self.serie, self.sel, Modo::Sencillo, &pie),
                Grafico::Renta => {
                    let datos: Vec<(i32, Option<f64>)> = self.serie.iter().zip(&self.resultados).map(|(a, r)| (a.y, *r)).collect();
                    renta::dibujar_grafico(f, a_graf, &datos, self.serie.get(self.sel).map(|a| a.y), &pie);
                }
            }
        }
        self.dibujar_tabla(f, a_tabla, ctx);
    }

    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let serie = &self.serie;
        let hay = |x: &dyn Fn(&Anio) -> f64| serie.iter().any(|a| x(a).abs() > DIF);
        let importe = |x: f64, st: Style| if x.abs() <= DIF { ("—".to_string(), tema::apagado()) } else { (eur0(x), st) };
        // Columnas: (título, prioridad: 0 siempre, a la derecha, texto y estilo de cada año).
        type Col<'a> = (&'static str, u8, bool, Box<dyn Fn(usize, &Anio) -> (String, Style) + 'a>);
        let mut cols: Vec<Col> = Vec::new();
        for c in Concepto::GANAS {
            if hay(&|a| a.de(c)) {
                let prio = if matches!(c, Concepto::Sueldo | Concepto::Actividad) { 2 } else { 3 };
                cols.push((c.corto(), prio, true, Box::new(move |_, a: &Anio| importe(a.de(c), Style::new()))));
            }
        }
        cols.push(("Ganas", 0, true, Box::new(move |_, a: &Anio| importe(a.ganas, Style::new().add_modifier(Modifier::BOLD)))));
        cols.push(("IRPF", 3, true, Box::new(move |_, a: &Anio| importe(a.de(Concepto::Irpf), Style::new()))));
        if hay(&|a| a.seguridad_social()) {
            cols.push(("Seg. Social", 3, true, Box::new(move |_, a: &Anio| importe(a.seguridad_social(), Style::new()))));
        }
        cols.push(("Pagas", 0, true, Box::new(move |_, a: &Anio| importe(a.pagas, Style::new().fg(COLOR_PAGAS).add_modifier(Modifier::BOLD)))));
        cols.push(("Te queda", 1, true, Box::new(move |_, a: &Anio| importe(a.queda, Style::new().fg(if a.queda < -DIF { tema::MAL } else { COLOR_QUEDA })))));
        cols.push(("%", 4, true, Box::new(|_, a: &Anio| if a.ganas > DIF { (format!("{:.0} %", a.queda / a.ganas * 100.0), tema::apagado()) } else { (String::new(), tema::apagado()) })));
        let resultados = &self.resultados;
        let pendiente = self.pendiente.as_ref().map(|p| p.0);
        cols.push((
            "Renta",
            1,
            true,
            Box::new(move |i, a: &Anio| match resultados[i] {
                Some(x) => resultado(x),
                None if Some(a.y) == pendiente => ("pendiente".into(), Style::new().fg(tema::AVISO)),
                None => ("sin Renta".into(), tema::apagado()),
            }),
        ));
        if hay(&|a| a.empresa) {
            cols.push(("Tu empresa", 4, true, Box::new(move |_, a: &Anio| importe(a.empresa, Style::new().fg(COLOR_EMPRESA)))));
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
        cols.push(("Notas", 5, false, Box::new(notas)));

        let b = match &self.pendiente {
            Some((y, fin)) => bloque("Años").title_bottom(Line::styled(format!(" Renta {y}: se presenta hasta el {} ", crate::dominio::fmt::fecha_corta(fin)), tema::apagado())),
            None => bloque("Años"),
        };
        let dentro = b.inner(area);
        f.render_widget(b, area);
        let celdas: Vec<Vec<(String, Style)>> = serie.iter().enumerate().map(|(i, a)| cols.iter().map(|c| (c.3)(i, a)).collect()).collect();
        let anchos: Vec<usize> = (0..cols.len()).map(|i| celdas.iter().map(|c| c[i].0.chars().count()).max().unwrap_or(0).max(cols[i].0.chars().count())).collect();
        // Las columnas que quepan, por prioridad (año y símbolo de la elegida: 8).
        let mut usado = 8;
        let mut ver = vec![false; cols.len()];
        let mut orden: Vec<usize> = (0..cols.len()).collect();
        orden.sort_by_key(|&i| cols[i].1);
        for i in orden {
            if cols[i].1 == 0 || usado + anchos[i] + 2 <= dentro.width as usize {
                ver[i] = true;
                usado += anchos[i] + 2;
            }
        }
        let vis: Vec<usize> = (0..cols.len()).filter(|&i| ver[i]).collect();
        let cab = Row::new(std::iter::once(Cell::from("Año")).chain(vis.iter().map(|&i| Cell::from(if cols[i].2 { derecha(cols[i].0) } else { Line::from(cols[i].0) })))).style(tema::cabecera_tabla());
        let filas: Vec<Row> = serie
            .iter()
            .zip(&celdas)
            .zip(resultados)
            .map(|((a, c), r)| {
                // Sin Renta, en gris: no son las cifras de Hacienda.
                let oficial = r.is_some();
                let marca = if oficial { "" } else { "*" };
                let st = if a.y == ctx.anio { Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD) } else if oficial { Style::new().add_modifier(Modifier::BOLD) } else { tema::apagado() };
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
        let t = Table::new(filas, w).header(cab).column_spacing(2).row_highlight_style(tema::seleccion()).highlight_symbol("› ");
        self.estado.select(Some(self.sel));
        f.render_stateful_widget(t, dentro, &mut self.estado);
    }
}
