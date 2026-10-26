//! «Años»: un año por fila (el último, el que está en curso), con lo que ganaste por partes, lo que pagaste y lo que
//! te quedó, y lo que salió en tu Renta; Enter abre uno entero (su balance y su Renta casilla a casilla). Las cifras
//! salen de `informe::anio`; aquí solo se pintan, y se rehacen solo cuando cambian los datos (`ctx.generacion`) o el
//! año.
//!
//! - `anios_lista`: el gráfico y la tabla.
//! - `anios_balance`: un año: sus cifras y su Renta casilla a casilla.
//! - `anios_renta`: la Renta de un año, casilla a casilla, y el gráfico de lo que salió cada año.
//! - `anios_grafico`: las barras apiladas del gráfico.
//! - `anios_reparto`: un año, cada fuente paso a paso y de dónde sale cada cifra.

use crate::dominio::fmt;
use crate::informe::anio::{Anio, Concepto};
use crate::dominio::PRIMER_ANIO;
use crate::informe;
use crate::tui::texto::envolver;
use crate::tui::widgets::tema;
use crate::tui::{Ctx, Pantalla};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::TableState;
use std::hash::{DefaultHasher, Hash, Hasher};
use super::{anios_balance as balance, anios_lista as lista, anios_renta as renta};


/// El color de cada concepto, siempre el mismo y en este orden (lo que ganas en fríos, lo que pagas en cálidos).
/// Son colores con nombre del terminal: cada tema (claro u oscuro) los ajusta para que se lean.
pub(super) fn color(c: Concepto) -> Color {
    match c {
        Concepto::Sueldo => Color::Blue,
        Concepto::Actividad => tema::ACENTO,
        Concepto::Capital => tema::BIEN,
        Concepto::Ganancias => Color::LightGreen,
        Concepto::OtrasRentas => Color::Magenta,
        Concepto::Irpf => tema::MAL,
        Concepto::TuNomina => tema::AVISO,
        Concepto::Cuota => Color::LightRed,
    }
}

/// Lo que pagó tu empresa, en gris.
pub(super) const COLOR_EMPRESA: Color = tema::APAGADO;
/// En el gráfico sencillo: lo que ganas, lo que pagas y lo que te queda.
pub(super) const COLOR_GANAS: Color = Color::Blue;
pub(super) const COLOR_PAGAS: Color = tema::MAL;
pub(super) const COLOR_QUEDA: Color = tema::BIEN;

/// Lo último que se pintó de una pantalla, para repetirlo tal cual mientras no cambie nada: la app repinta cada
/// medio segundo y pintar una pantalla entera cuesta bastante más que copiarla.
#[derive(Default)]
pub(super) struct Lienzo {
    clave: Option<u64>,
    buf: Buffer,
}

impl Lienzo {
    /// Si `clave` (todo lo que cambia lo que se ve) y el área son las de la última vez, copia lo pintado; si no,
    /// pinta con `dibujar` y lo guarda.
    pub(super) fn pintar(&mut self, f: &mut Frame, area: Rect, clave: impl Hash, dibujar: impl FnOnce(&mut Frame)) {
        let mut h = DefaultHasher::new();
        (clave, area).hash(&mut h);
        let h = h.finish();
        let area = area.intersection(f.buffer_mut().area);
        if self.clave == Some(h) && self.buf.area == area {
            copiar(&self.buf, f.buffer_mut(), area);
            return;
        }
        dibujar(f);
        self.buf.resize(area);
        copiar(f.buffer_mut(), &mut self.buf, area);
        self.clave = Some(h);
    }

    /// Para pintar de nuevo la próxima vez.
    pub(super) fn olvidar(&mut self) {
        self.clave = None;
    }
}

/// Copia las celdas de `area` de un buffer a otro, fila a fila (los dos la contienen).
fn copiar(de: &Buffer, a: &mut Buffer, area: Rect) {
    if area.is_empty() {
        return;
    }
    for y in area.top()..area.bottom() {
        let (i, j) = (de.index_of(area.x, y), a.index_of(area.x, y));
        let n = area.width as usize;
        a.content[j..j + n].clone_from_slice(&de.content[i..i + n]);
    }
}

/// Unas notas, cada una con «• » delante.
pub(super) fn vinetas(notas: Vec<(String, Style)>, w: usize) -> Vec<Line<'static>> {
    notas
        .into_iter()
        .flat_map(|(t, st)| {
            let mut l = envolver(&[(t, st)], w, 2);
            if let Some(p) = l.first_mut()
                && let Some(s) = p.spans.first_mut()
            {
                *s = Span::styled("• ", tema::apagado());
            }
            l
        })
        .collect()
}

/* ---------------- la pantalla ---------------- */

#[derive(Default)]
pub struct Anios {
    /// La generación de los datos con que se calculó la serie.
    pub(super) clave: Option<u64>,
    /// Los años con algo, del primero al actual.
    pub(super) serie: Vec<Anio>,
    /// Lo que salió en la Renta de cada año de `serie` (None: no hay Renta).
    pub(super) resultados: Vec<Option<f64>>,
    /// Hasta cuándo se puede presentar la Renta del año pasado, si aún no está.
    pub(super) pendiente: Option<(i32, String)>,
    /// El año elegido en la tabla (índice en `serie`).
    pub(super) sel: usize,
    pub(super) estado: TableState,
    pub(super) grafico: lista::Grafico,
    pub(super) lienzo: Lienzo,
    /// Se ve un año entero (el de `ctx.anio`).
    pub(super) detalle: bool,
    pub(super) vista: balance::Vista,
}

impl Anios {
    fn al_dia(&mut self, ctx: &Ctx) {
        if self.clave != Some(ctx.generacion) {
            let hoy = fmt::anio_actual();
            self.serie = informe::anio::serie(&ctx.datos);
            self.resultados = self.serie.iter().map(|a| informe::renta_de(&ctx.datos, a.y).map(|r| r.resultado)).collect();
            self.pendiente = informe::renta_pendiente(&ctx.datos, hoy - 1).map(|fin| (hoy - 1, fin));
            self.clave = Some(ctx.generacion);
            self.seguir(ctx.anio);
        }
        self.sel = self.sel.min(self.serie.len().saturating_sub(1));
    }

    /// La fila elegida va al año de arriba, si está en la serie.
    fn seguir(&mut self, y: i32) {
        if let Some(i) = self.serie.iter().position(|a| a.y == y) {
            self.sel = i;
        } else if self.serie.last().is_some_and(|a| y > a.y) {
            self.sel = self.serie.len() - 1;
        } else if self.serie.first().is_some_and(|a| y < a.y) {
            self.sel = 0;
        }
    }

    fn mover(&mut self, d: isize) {
        let n = self.serie.len();
        if n > 0 {
            self.sel = (self.sel as isize + d).clamp(0, n as isize - 1) as usize;
        }
    }

    /// El PDF de la Renta de ese año; si no la hay, a elegir entre los del año.
    fn pdf(ctx: &mut Ctx, y: i32) {
        if informe::renta_de(&ctx.datos, y).is_some() {
            renta::abrir_pdf(ctx, y);
        } else {
            ctx.elegir_pdf(y);
        }
    }
}

impl Pantalla for Anios {
    fn titulo(&self) -> &'static str {
        "Años"
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if self.detalle {
            self.vista.dibujar(f, area, ctx);
            return;
        }
        self.al_dia(ctx);
        // Mientras no cambie nada (datos, año, fila elegida, gráfico, tamaño), se repite lo pintado.
        let clave = (self.clave, ctx.anio, self.sel, self.estado.offset(), self.grafico);
        let mut lienzo = std::mem::take(&mut self.lienzo);
        lienzo.pintar(f, area, clave, |f| self.dibujar_lista(f, area, ctx));
        self.lienzo = lienzo;
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if self.detalle {
            match k.code {
                KeyCode::Esc | KeyCode::Backspace => self.detalle = false,
                KeyCode::Char('o') => Self::pdf(ctx, ctx.anio),
                _ => return self.vista.tecla(k),
            }
            return true;
        }
        match k.code {
            KeyCode::Up | KeyCode::Left => self.mover(-1),
            KeyCode::Down | KeyCode::Right => self.mover(1),
            KeyCode::PageUp => self.mover(-10),
            KeyCode::PageDown => self.mover(10),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = self.serie.len().saturating_sub(1),
            KeyCode::Char('g') => self.grafico = self.grafico.siguiente(),
            KeyCode::Enter | KeyCode::Char('o') => match self.serie.get(self.sel).map(|a| a.y) {
                Some(y) if k.code == KeyCode::Enter => {
                    ctx.anio = y;
                    self.detalle = true;
                    self.vista.olvidar();
                }
                Some(y) => Self::pdf(ctx, y),
                None => ctx.error(format!("Aún no hay ningún año con datos desde {PRIMER_ANIO}.")),
            },
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.detalle {
            let mut v = vec![("Esc", "volver"), ("↑↓", "bajar")];
            if self.vista.con_renta() {
                v.push(("o", "PDF de la Renta"));
            }
            v
        } else {
            vec![("↑↓", "año"), ("Enter", "ver el año"), ("o", "PDF"), ("g", self.grafico.siguiente().nombre())]
        }
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        // Al entrar, o al cambiar de año con - y +: se recalcula (es poco) y la fila sigue al año.
        self.clave = None;
        self.lienzo.olvidar();
        self.vista.olvidar();
        self.al_dia(ctx);
        self.seguir(ctx.anio);
    }
}
