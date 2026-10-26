//! Cuotas: tus recibos de autónomo del año que miras (cambia de año con - y +): lo pagado, la cuota y la base, la
//! regularización y los recibos mes a mes. El desglose de un recibo (pensión, cese, formación, MEI…), con Enter.
//! Solo se mira: los datos vienen de la Seguridad Social.

use crate::dominio::fmt;
use crate::informe::autonomo::{self, CuotasAnio};
use crate::tui::widgets::{bloque, centrado, derecha, kpi, tema};
use crate::tui::{Ctx, Pantalla};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, Wrap};
use textos::{CABECERAS, Cifra, Fila, Nota};
use super::cuotas_textos as textos;

pub fn pantallas() -> Vec<Box<dyn Pantalla>> {
    vec![Box::new(Cuotas::default())]
}

/// Ancho de la tabla de meses (con su recuadro).
const ANCHO_TABLA: u16 = 40;

/// Lo calculado para un año (se rehace al cambiar los datos, el año o el día).
struct Calculo {
    clave: (u64, i32, String),
    c: CuotasAnio,
    filas: Vec<Fila>,
    total: [String; 3],
    hay_tabla: bool,
    cifras: Vec<Cifra>,
    notas: Vec<Nota>,
}

#[derive(Default)]
pub struct Cuotas {
    calculo: Option<Calculo>,
    /// La fila elegida y la primera que se ve (del último dibujo).
    sel: usize,
    desde: usize,
    alto: usize,
    /// El desglose del recibo elegido, abierto con Enter.
    abierto: bool,
    /// Cuántas veces se ha calculado (para las pruebas).
    #[cfg(test)]
    pub(crate) calculos: usize,
}

impl Cuotas {
    fn preparar(&mut self, ctx: &Ctx) {
        let hoy = fmt::hoy();
        let clave = (ctx.generacion, ctx.anio, hoy.clone());
        if self.calculo.as_ref().is_some_and(|k| k.clave == clave) {
            return;
        }
        let d = &ctx.datos;
        let otro_anio = self.calculo.as_ref().is_none_or(|k| k.clave.1 != ctx.anio);
        let c = autonomo::cuotas_anio(d, ctx.anio, &hoy);
        let k = Calculo {
            filas: textos::filas(&c, &hoy),
            total: textos::total(&c),
            hay_tabla: textos::hay_tabla(&c),
            cifras: textos::cifras(&c, &d.tgss, &hoy),
            notas: textos::notas(&c, &d.tgss, &hoy),
            c,
            clave,
        };
        if otro_anio {
            // En otro año: el último recibo elegido (el más reciente).
            self.sel = k.filas.iter().rposition(|f| f.recibo.is_some()).unwrap_or(0);
            self.desde = 0;
            self.abierto = false;
        }
        self.sel = self.sel.min(k.filas.len().saturating_sub(1));
        self.calculo = Some(k);
        #[cfg(test)]
        {
            self.calculos += 1;
        }
    }

    fn pintar_cifras(f: &mut Frame, area: Rect, k: &Calculo, grande: bool) {
        let n = k.cifras.len().max(1) as u32;
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, n); n as usize]).split(area);
        for (c, z) in k.cifras.iter().zip(zonas.iter()) {
            let etiqueta = if z.width >= 22 { c.etiqueta.as_str() } else { c.corta.as_str() };
            if grande {
                kpi(f, *z, etiqueta, &c.valor, &c.nota, c.estilo);
            } else {
                let t = vec![Line::styled(format!(" {etiqueta}"), tema::apagado()), Line::styled(format!(" {}", c.valor), c.estilo.add_modifier(Modifier::BOLD))];
                f.render_widget(Paragraph::new(t), *z);
            }
        }
    }

    /// Los meses: mes, importe y cargo, con el elegido resaltado y el total siempre abajo.
    fn pintar_tabla(&mut self, f: &mut Frame, area: Rect, k: &Calculo) {
        let y = k.c.anio;
        let titulo = format!("Recibos de {y}");
        let b = bloque(&titulo);
        if !k.hay_tabla {
            let t = vec![Line::from(""), Line::styled(format!("Sin recibos de autónomo en {y}."), tema::apagado()).centered()];
            f.render_widget(Paragraph::new(t).block(b), area);
            return;
        }
        let dentro = b.inner(area);
        self.alto = (dentro.height as usize).saturating_sub(2).max(1);
        if self.sel < self.desde {
            self.desde = self.sel;
        } else if self.sel >= self.desde + self.alto {
            self.desde = self.sel + 1 - self.alto;
        }
        let celda = |i: usize, t: &str, st: Style| if i == 0 { Cell::from(Span::styled(t.to_string(), st)) } else { Cell::from(derecha(t.to_string())).style(st) };
        let mut filas: Vec<Row> = k
            .filas
            .iter()
            .enumerate()
            .skip(self.desde)
            .take(self.alto)
            .map(|(n, x)| {
                let fila = Row::new((0..3).map(|i| if i == 0 { celda(i, &x.celdas[i], x.estilo_mes) } else { celda(i, &x.celdas[i], x.estilo) })).style(x.estilo);
                if n == self.sel { fila.style(tema::seleccion()) } else { fila }
            })
            .collect();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        filas.push(Row::new((0..3).map(|i| celda(i, &k.total[i], negrita))));
        let cab = Row::new((0..3).map(|i| if i == 0 { Cell::from(CABECERAS[i]) } else { Cell::from(derecha(CABECERAS[i])) })).style(tema::cabecera_tabla());
        let b = b.title_bottom(Line::styled(" Enter: desglose ", tema::apagado()).right_aligned());
        let anchos = [Constraint::Length(10), Constraint::Length(12), Constraint::Min(6)];
        f.render_widget(Table::new(filas, anchos).header(cab).column_spacing(1).block(b), area);
    }

    fn pintar_notas(f: &mut Frame, area: Rect, notas: &[Nota]) {
        if area.height == 0 || notas.is_empty() {
            return;
        }
        let t: Vec<Line> = notas.iter().map(|n| Line::styled(format!("· {}", n.texto), n.estilo)).collect();
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), area);
    }

    /// El desglose del recibo elegido, en una ventana.
    fn pintar_desglose(&self, f: &mut Frame, area: Rect, k: &Calculo) {
        let Some(fila) = k.filas.get(self.sel) else { return };
        let Some(r) = &fila.recibo else { return };
        let partes = textos::desglose(r);
        let suma: f64 = partes.iter().map(|p| p.1).sum();
        let mut t = vec![Line::from(vec![Span::styled(format!("{} · ", r.concepto), tema::apagado()), Span::styled(fmt::eur(r.importe), Style::new().add_modifier(Modifier::BOLD))]), Line::from("")];
        for (nombre, x) in &partes {
            t.push(Line::from(vec![Span::raw(format!("{nombre:<42}")), Span::raw(format!("{:>12}", fmt::eur(*x)))]));
        }
        if partes.is_empty() {
            t.push(Line::styled("Este recibo no trae desglose.", tema::apagado()));
        } else if (suma - r.importe).abs() > 0.5 {
            t.push(Line::from(""));
            let que = if suma > r.importe { format!("Pagaste {} menos (tarifa plana o bonificación).", fmt::eur(suma - r.importe)) } else { format!("Hay {} sin desglose.", fmt::eur(r.importe - suma)) };
            t.push(Line::styled(que, tema::apagado()));
        }
        let fecha = if fmt::fecha(r.fecha_cargo()).is_some() { format!(" · cargo el {}", fmt::fecha_corta(r.fecha_cargo())) } else { String::new() };
        let titulo = format!("Recibo de {}{fecha}", fmt::nombre_mes(&r.periodo));
        let rect = centrado(area, 62, t.len() as u16 + 2);
        f.render_widget(Clear, rect);
        f.render_widget(Paragraph::new(t).block(bloque(&titulo).title_bottom(Line::styled(" Esc: cerrar ", tema::apagado()).right_aligned())), rect);
    }
}

impl Pantalla for Cuotas {
    fn titulo(&self) -> &'static str {
        "Cuotas"
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        // Al entrar se recalcula siempre (por si los datos cambiaron sin avisar).
        self.calculo = None;
        self.preparar(ctx);
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.preparar(ctx);
        let Some(k) = self.calculo.take() else { return };
        let grande = area.height >= 24;
        let [a_cif, resto] = Layout::vertical([Constraint::Length(if grande { 5 } else { 2 }), Constraint::Min(0)]).areas(area);
        Self::pintar_cifras(f, a_cif, &k, grande);
        let alto_tabla = (if k.hay_tabla { k.filas.len() as u16 + 4 } else { 4 }).min(resto.height);
        if area.width >= ANCHO_TABLA + 30 {
            // La tabla a la izquierda y las notas a su lado.
            let [izq, der] = Layout::horizontal([Constraint::Length(ANCHO_TABLA), Constraint::Min(0)]).areas(resto);
            let [a_tabla, _] = Layout::vertical([Constraint::Length(alto_tabla), Constraint::Min(0)]).areas(izq);
            self.pintar_tabla(f, a_tabla, &k);
            Self::pintar_notas(f, Rect { x: der.x + 2, width: der.width.saturating_sub(2), y: der.y + 1, height: der.height.saturating_sub(1) }, &k.notas);
        } else {
            let [a_tabla, a_notas] = Layout::vertical([Constraint::Length(alto_tabla), Constraint::Min(0)]).areas(resto);
            self.pintar_tabla(f, a_tabla, &k);
            Self::pintar_notas(f, a_notas, &k.notas);
        }
        if self.abierto {
            self.pintar_desglose(f, area, &k);
        }
        self.calculo = Some(k);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        self.preparar(ctx);
        let n = self.calculo.as_ref().map_or(0, |c| c.filas.len());
        if self.abierto {
            match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace | KeyCode::Left => self.abierto = false,
                // Con el desglose abierto, ↑↓ pasan al recibo de al lado.
                KeyCode::Up => self.sel = self.sel.saturating_sub(1),
                KeyCode::Down => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
                _ => return false,
            }
            return true;
        }
        let pagina = self.alto.max(1);
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.sel = self.sel.saturating_sub(pagina),
            KeyCode::PageDown => self.sel = (self.sel + pagina).min(n.saturating_sub(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = n.saturating_sub(1),
            KeyCode::Enter | KeyCode::Right => {
                let con_recibo = self.calculo.as_ref().and_then(|c| c.filas.get(self.sel)).is_some_and(|f| f.recibo.is_some());
                if con_recibo {
                    self.abierto = true;
                } else {
                    ctx.info("Ese mes no tiene recibo.");
                }
            }
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.abierto {
            return vec![("Esc", "cerrar"), ("↑↓", "otro recibo")];
        }
        if self.calculo.as_ref().is_some_and(|c| c.hay_tabla) { vec![("↑↓", "mes"), ("Enter", "desglose")] } else { Vec::new() }
    }

    fn capturando(&self) -> bool {
        self.abierto
    }
}
