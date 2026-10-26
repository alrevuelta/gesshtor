//! Un año entero (Enter en «Años»): arriba, lo que generaste, lo que se llevaron impuestos y cotizaciones y lo que
//! te quedó; dentro, cada fuente paso a paso, con de dónde sale cada cifra (`anios_reparto`); y debajo, la Renta
//! casilla a casilla. Con Renta, es el resumen de tu modelo 100 (con `o` se abre su PDF). El año en curso, hasta
//! hoy y con lo que saldría en tu Renta, si se puede estimar.
use crate::dominio::fmt::DIF;
use super::anios::Lienzo;
use super::anios_renta as renta;
use super::anios_reparto;
use crate::informe::reparto::{self, Reparto};
use crate::dominio::renta::Renta;
use crate::dominio::fmt::{self, eur, eur0};
use crate::informe::anio::{self, Detalle, euro, pct};
use crate::informe::prevision::{self, Prevision};
use crate::informe::{self};
use crate::tui::widgets::{kpi, tema};
use crate::tui::Ctx;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Desde este ancho, la Renta casilla a casilla va en dos columnas.
const ANCHO_DOS_COLUMNAS: u16 = 100;

/// Lo de un año, ya calculado, y por dónde se va bajando.
#[derive(Default)]
pub(super) struct Vista {
    /// (generación de los datos, año) con que se calculó.
    clave: Option<(u64, i32)>,
    /// El año que se veía (al cambiar de año se vuelve arriba).
    visto: Option<i32>,
    d: Detalle,
    /// A dónde fue lo de cada fuente.
    rep: Reparto,
    /// Su Renta, cuándo se presentó y, si aún no está, hasta cuándo se puede presentar.
    renta: Option<Renta>,
    presentada: Option<String>,
    pendiente: Option<String>,
    /// El año en curso: lo que saldría en su Renta.
    prevision: Option<Prevision>,
    scroll: u16,
    scroll_max: u16,
    lienzo: Lienzo,
}

fn gris() -> Style {
    tema::apagado()
}

impl Vista {
    /// Rehace las cifras si han cambiado los datos o el año.
    pub(super) fn al_dia(&mut self, ctx: &Ctx) {
        let k = (ctx.generacion, ctx.anio);
        if self.clave == Some(k) {
            return;
        }
        if self.visto.is_some_and(|v| v != ctx.anio) {
            self.scroll = 0;
        }
        self.visto = Some(ctx.anio);
        self.d = anio::detalle(&ctx.datos, ctx.anio);
        self.renta = informe::renta_de(&ctx.datos, ctx.anio).cloned();
        self.presentada = crate::hacienda::presentada(&ctx.datos, ctx.anio, 0, crate::hacienda::Modelo::M100).map(|d| d.fecha);
        self.pendiente = informe::renta_pendiente(&ctx.datos, ctx.anio);
        self.rep = reparto::reparto(&ctx.datos, &self.d, self.renta.as_ref());
        self.prevision = self.en_curso().then(|| prevision::prevision(&ctx.datos, ctx.anio, &self.d));
        self.clave = Some(k);
    }

    /// Para recalcular al volver (al entrar en la pestaña).
    pub(super) fn olvidar(&mut self) {
        self.clave = None;
        self.lienzo.olvidar();
    }

    pub(super) fn con_renta(&self) -> bool {
        self.d.anio.con_renta
    }

    /// El año en curso (sin Renta todavía).
    fn en_curso(&self) -> bool {
        self.d.anio.y >= fmt::anio_actual() && !self.con_renta()
    }

    pub(super) fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.al_dia(ctx);
        // Mientras no cambie nada (datos, año, por dónde se va, tamaño), se repite lo pintado. (Si se ha bajado de
        // más, al pintar se ajusta y la vez siguiente se pinta otra vez, ya ajustado.)
        let clave = (self.clave, self.scroll);
        let mut lienzo = std::mem::take(&mut self.lienzo);
        lienzo.pintar(f, area, clave, |f| self.pintar(f, area));
        self.lienzo = lienzo;
    }

    fn pintar(&mut self, f: &mut Frame, area: Rect) {
        let grande = area.height >= 22;
        let [a_kpi, a_cuerpo] = Layout::vertical([Constraint::Length(if grande { 5 } else { 2 }), Constraint::Min(0)]).areas(area);
        self.kpis(f, a_kpi, grande);
        self.cuerpo(f, a_cuerpo);
    }

    /// Las tres cifras de arriba, con las mismas palabras que «Todo junto»; en el año en curso, también lo que
    /// saldría en tu Renta.
    fn kpis(&self, f: &mut Frame, area: Rect, grande: bool) {
        let (a, t) = (&self.d.anio, &self.rep.total);
        let n = if self.prevision.is_some() { 4 } else { 3 };
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, n); n as usize]).split(area);
        let w = zonas[0].width.saturating_sub(2) as usize;
        let cabe = |largo: String, corto: &str| if largo.chars().count() <= w { largo } else { corto.to_string() };
        let generado = fmt::r2(t.generado - t.iva);
        let nota_generado = if a.sueldo_aprox {
            cabe("con el sueldo aproximado".into(), "aproximado")
        } else if a.empresa > DIF {
            cabe("con lo que pagó tu empresa por ti".into(), "con tu empresa")
        } else if self.en_curso() {
            "hasta hoy".into()
        } else {
            cabe("antes de impuestos".into(), "bruto")
        };
        let nota_impuestos = if a.falta_nomina {
            cabe("sin el IRPF de tu nómina".into(), "sin IRPF nómina")
        } else if t.base() > DIF {
            cabe(format!("el {} de lo que generaste", pct(t.impuestos(), t.base())), &pct(t.impuestos(), t.base()))
        } else {
            String::new()
        };
        let nota_queda = if a.especie > DIF { cabe(format!("{} en especie", eur0(a.especie)), "con especie") } else { String::new() };
        let mut cifras = vec![
            (cabe("Lo que generaste".into(), "Generaste"), euro(generado), nota_generado, Style::new()),
            (cabe("Impuestos y cotizaciones".into(), "Impuestos"), eur(t.impuestos()), nota_impuestos, Style::new().fg(tema::MAL)),
            ("Te queda".to_string(), euro(t.queda), nota_queda, Style::new().fg(if t.queda < -DIF { tema::MAL } else { tema::BIEN })),
        ];
        match &self.prevision {
            Some(Prevision::Estimada { resultado: x, .. }) => {
                let que = if *x > DIF { "a pagar" } else if *x < -DIF { "a devolver" } else { "a cero" };
                cifras.push((cabe(format!("Tu Renta de {}", a.y), "Tu Renta"), format!("≈{}", eur0(x.abs())), cabe(format!("{que}, estimado"), que), tema::importe(*x)));
            }
            Some(Prevision::NoSe(_)) => cifras.push((cabe(format!("Tu Renta de {}", a.y), "Tu Renta"), "—".into(), cabe(format!("en abril de {}", a.y + 1), "más adelante"), gris())),
            None => {}
        }
        for ((etq, v, nota, st), z) in cifras.into_iter().zip(zonas.iter()) {
            if grande {
                kpi(f, *z, &etq, &v, &nota, st);
            } else {
                let t = vec![Line::styled(format!(" {etq}"), gris()), Line::styled(format!(" {v}"), st.add_modifier(Modifier::BOLD))];
                f.render_widget(Paragraph::new(t), *z);
            }
        }
    }

    /// «2024 · resumen de tu Renta: modelo 100, rectificativa, presentada el 15 abr 2025»; sin Renta, con qué se ha
    /// hecho la cuenta.
    fn titulo(&self) -> Line<'static> {
        let y = self.d.anio.y;
        let mut v = vec![Span::styled(format!(" {y} "), tema::titulo()), Span::styled("· ", gris())];
        if self.en_curso() {
            v.push(Span::styled("en curso", Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)));
            v.push(Span::styled(": hasta hoy, con la Seguridad Social y lo apuntado ", gris()));
        } else if let Some(r) = self.renta.as_ref().filter(|_| self.d.anio.con_renta) {
            let rect = if r.rectificativa { ", rectificativa" } else { "" };
            let cuando = self.presentada.as_deref().map(|p| format!(", presentada el {}", fmt::fecha_corta(p))).unwrap_or_default();
            v.push(Span::styled("resumen de tu Renta", Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD)));
            v.push(Span::styled(format!(": modelo 100{rect}{cuando} "), gris()));
        } else {
            let (modo, resto) = match &self.pendiente {
                Some(fin) => ("Renta pendiente", format!("se presenta hasta el {}: hasta entonces, con la Seguridad Social y lo apuntado", fmt::fecha_corta(fin))),
                None => ("sin Renta", "con la Seguridad Social y lo apuntado".into()),
            };
            v.push(Span::styled(modo, Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)));
            v.push(Span::styled(format!(": {resto} "), gris()));
        }
        Line::from(v)
    }

    /// Un recuadro con su título y estas líneas, que se bajan con ↑↓ si no caben. Con Renta, abajo, cómo abrir su PDF.
    fn recuadro(&mut self, f: &mut Frame, area: Rect, titulo: Line<'static>, lineas: Vec<Line<'static>>) {
        let mut b = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(gris()).title(titulo);
        if self.con_renta() {
            b = b.title_bottom(Line::styled(" o: abrir el PDF del modelo 100 ", gris()));
        }
        let dentro = b.inner(area);
        let alto = dentro.height;
        self.scroll_max = (lineas.len() as u16).saturating_sub(alto);
        self.scroll = self.scroll.min(self.scroll_max);
        if self.scroll_max > 0 {
            let t = if self.scroll >= self.scroll_max { " ↑ más arriba " } else { " ↓ más abajo (↑↓) " };
            b = b.title_bottom(Line::styled(t, gris()).right_aligned());
        }
        f.render_widget(b, area);
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)), dentro);
    }

    /// Cada fuente paso a paso y, debajo, la Renta casilla a casilla.
    fn cuerpo(&mut self, f: &mut Frame, area: Rect) {
        let titulo = self.titulo();
        let dentro = Block::new().borders(Borders::ALL).inner(area);
        if dentro.height == 0 || dentro.width < 10 {
            self.recuadro(f, area, titulo, Vec::new());
            return;
        }
        let w = dentro.width as usize;
        let mut lineas = anios_reparto::lineas(&self.rep, &self.d, self.prevision.as_ref(), w.min(100));
        if let Some(r) = &self.renta {
            let dos = dentro.width >= ANCHO_DOS_COLUMNAS;
            let wr = if dos { ((w - 3) / 2).min(72) } else { w.min(90) };
            lineas.push(Line::from(""));
            lineas.extend(renta::lineas(self.d.anio.y, r, self.presentada.as_deref(), wr, dos));
        }
        self.recuadro(f, area, titulo, lineas);
    }

    /// Bajar por el año. Devuelve true si ha usado la tecla.
    pub(super) fn tecla(&mut self, k: KeyEvent) -> bool {
        match k.code {
            // Al bajar no se mira el tope: se ajusta al pintar, que es cuando se sabe cuánto cabe.
            KeyCode::Up => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = u16::MAX,
            _ => return false,
        }
        true
    }
}
