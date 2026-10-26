//! «Este año»: solo el año en curso, que aún no tiene Renta. Arriba lo que llevas (ganado, IRPF adelantado,
//! cotizado) y lo que saldría en tu Renta si se puede estimar; debajo cada cifra, separada por de dónde sale: lo que
//! consta en Hacienda y en la Seguridad Social, lo que has apuntado y lo estimado (con «≈»). Los años acabados
//! están en «Años» (`-` va allí).
use super::anios::{explicada, vinetas};
use crate::dominio::fmt::{self, eur, eur0};
use crate::informe::anio::{Fila, Origen, euro};
use crate::informe::este_anio::{EsteAnio as Datos, Prevision, este_anio};
use crate::dominio::PRIMER_ANIO;
use crate::tui::texto::{dos_columnas, envolver, fila};
use crate::tui::widgets::{kpi, tema};
use crate::tui::{Ctx, Pantalla};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Desde este ancho, lo que consta va a la izquierda y lo apuntado y lo estimado, a la derecha.
const ANCHO_DOS_COLUMNAS: u16 = 100;

#[derive(Default)]
pub struct EsteAnio {
    /// (generación de los datos, año) con que se calculó.
    clave: Option<(u64, i32)>,
    e: Option<Datos>,
    scroll: u16,
    scroll_max: u16,
}

fn gris() -> Style {
    tema::apagado()
}

/// «Consta en Hacienda y la Seguridad Social», «Lo que has apuntado», «Estimado».
fn grupo(o: Origen) -> &'static str {
    match o {
        Origen::Consta => "Consta en Hacienda y la Seguridad Social",
        Origen::Apuntado => "Lo que has apuntado",
        Origen::Estimado => "Estimado",
    }
}

/// Lo que se dice si un grupo está vacío.
fn nada(o: Origen) -> &'static str {
    match o {
        Origen::Consta => "Nada todavía: se trae con tu certificado (S).",
        Origen::Apuntado => "Nada apuntado: tus nóminas, en Empleo; tus facturas y gastos, en Autónomo.",
        Origen::Estimado => "",
    }
}

impl EsteAnio {
    fn al_dia(&mut self, ctx: &Ctx) {
        let k = (ctx.generacion, fmt::anio_actual());
        if self.clave != Some(k) {
            self.e = Some(este_anio(&ctx.datos, k.1));
            self.clave = Some(k);
        }
    }

    /// Las cuatro cifras de arriba.
    fn kpis(e: &Datos, f: &mut Frame, area: Rect, grande: bool) {
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, 4); 4]).split(area);
        let w = zonas[0].width.saturating_sub(2) as usize;
        let cabe = |largo: String, corto: &str| if largo.chars().count() <= w { largo } else { corto.to_string() };
        let ganado = if e.sueldo_aprox { format!("≈{}", euro(e.ganado)) } else { euro(e.ganado) };
        let nota_ganado = if e.sueldo_aprox { cabe("el sueldo, aproximado".into(), "aproximado") } else { cabe("antes de impuestos".into(), "bruto") };
        let nota_irpf = if e.falta_nomina { cabe("sin el IRPF de tu nómina".into(), "sin IRPF nómina") } else { cabe("retenciones y 130".into(), "adelantado") };
        let (renta, nota_renta, st_renta) = match &e.renta {
            Prevision::Estimada { resultado: x, .. } => {
                let que = if *x > 0.5 { "a pagar" } else if *x < -0.5 { "a devolver" } else { "a cero" };
                (format!("≈{}", eur0(x.abs())), cabe(format!("{que}, estimado"), que), tema::importe(*x))
            }
            Prevision::NoSe(_) => ("—".to_string(), cabe(format!("en abril de {}", e.y + 1), "más adelante"), gris()),
        };
        let cifras = [
            (cabe("Llevas ganado".into(), "Ganado"), ganado, nota_ganado, Style::new()),
            (cabe("IRPF adelantado".into(), "IRPF"), eur(e.adelantado), nota_irpf, Style::new().fg(tema::MAL)),
            (cabe("Cotizado".into(), "Cotizado"), eur(e.cotizado), cabe("a la Seguridad Social".into(), "Seg. Social"), Style::new().fg(tema::AVISO)),
            (cabe(format!("Tu Renta de {}", e.y), "Tu Renta"), renta, nota_renta, st_renta),
        ];
        for ((etq, v, nota, st), z) in cifras.into_iter().zip(zonas.iter()) {
            if grande {
                kpi(f, *z, &etq, &v, &nota, st);
            } else {
                let t = vec![Line::styled(format!(" {etq}"), gris()), Line::styled(format!(" {v}"), st.add_modifier(Modifier::BOLD))];
                f.render_widget(Paragraph::new(t), *z);
            }
        }
    }

    /// Las cifras de un grupo, con su título.
    fn grupo(e: &Datos, o: Origen, w: usize) -> Vec<Line<'static>> {
        let filas: Vec<&Fila> = e.filas.iter().filter(|f| f.origen == o).collect();
        let mut l = vec![Line::styled(grupo(o), tema::titulo())];
        if filas.is_empty() && !nada(o).is_empty() {
            l.extend(envolver(&[(nada(o).into(), gris())], w, 0));
        }
        for x in filas {
            l.extend(explicada(x, w, 0, false));
        }
        l
    }

    /// Lo que saldría en la Renta (o por qué aún no se sabe).
    fn renta(e: &Datos, w: usize) -> Vec<Line<'static>> {
        let nombre = format!("Tu Renta de {}", e.y);
        match &e.renta {
            Prevision::Estimada { resultado: x, supuestos } => {
                let valor = if *x > 0.5 { format!("≈{} a pagar", eur0(*x)) } else if *x < -0.5 { format!("≈{} a devolver", eur0(-x)) } else { "≈0 €".into() };
                let mut l = vec![fila(&nombre, &valor, w, Style::new(), tema::importe(*x).add_modifier(Modifier::BOLD))];
                l[0].spans.insert(0, Span::raw("  "));
                l.extend(envolver(&[("Si sigue así: lo que te tocaría por tu sueldo menos lo que te retienen. ".into(), gris()), (supuestos.clone(), gris())], w, 2));
                l
            }
            Prevision::NoSe(por) => {
                let mut l = vec![Line::from(vec![Span::raw("  "), Span::raw(nombre)])];
                l.extend(envolver(&[(por.clone(), gris())], w, 2));
                l
            }
        }
    }

    fn cuerpo(&mut self, f: &mut Frame, area: Rect) {
        let Some(e) = &self.e else { return };
        let titulo = Line::from(vec![
            Span::styled(format!(" {}, hasta hoy ", e.y), tema::titulo()),
            Span::styled("· ", gris()),
            Span::styled("aún sin Renta ", Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)),
        ]);
        let mut b = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(gris()).title(titulo);
        let dentro = b.inner(area);
        if dentro.height == 0 || dentro.width < 10 {
            f.render_widget(b, area);
            return;
        }
        let w = dentro.width as usize;
        let dos = dentro.width >= ANCHO_DOS_COLUMNAS;
        let wc = if dos { ((w - 3) / 2).min(72) } else { w.min(90) };
        let mut estimado = Self::grupo(e, Origen::Estimado, wc);
        estimado.extend(Self::renta(e, wc));
        let mut lineas = if dos {
            let mut der = Self::grupo(e, Origen::Apuntado, wc);
            der.push(Line::from(""));
            der.extend(estimado);
            dos_columnas(Self::grupo(e, Origen::Consta, wc), der, wc)
        } else {
            let mut v = Self::grupo(e, Origen::Consta, wc);
            for g in [Self::grupo(e, Origen::Apuntado, wc), estimado] {
                v.push(Line::from(""));
                v.extend(g);
            }
            v
        };
        let mut notas: Vec<(String, Style)> = e.notas.iter().map(|n| (n.clone(), if n.starts_with("Falta") { Style::new().fg(tema::AVISO) } else { gris() })).collect();
        if e.cotizado > 0.005 {
            notas.push(("Las cotizaciones no son impuestos: dan derecho a pensión, paro y bajas.".into(), gris()));
        }
        if !notas.is_empty() {
            lineas.push(Line::from(""));
            lineas.extend(vinetas(notas, if dos { (wc * 2 + 3).min(w) } else { wc }));
        }
        self.scroll_max = (lineas.len() as u16).saturating_sub(dentro.height);
        self.scroll = self.scroll.min(self.scroll_max);
        if self.scroll_max > 0 {
            let t = if self.scroll >= self.scroll_max { " ↑ más arriba " } else { " ↓ más abajo (↑↓) " };
            b = b.title_bottom(Line::styled(t, gris()).right_aligned());
        }
        f.render_widget(b, area);
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)), dentro);
    }
}

impl Pantalla for EsteAnio {
    fn titulo(&self) -> &'static str {
        "Este año"
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.al_dia(ctx);
        let Some(e) = &self.e else { return };
        let grande = area.height >= 22;
        let [a_kpi, a_cuerpo] = Layout::vertical([Constraint::Length(if grande { 5 } else { 2 }), Constraint::Min(0)]).areas(area);
        Self::kpis(e, f, a_kpi, grande);
        self.cuerpo(f, a_cuerpo);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        match k.code {
            // Al bajar no se mira el tope: se ajusta al pintar, que es cuando se sabe cuánto cabe.
            KeyCode::Up => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = u16::MAX,
            KeyCode::Char('o') => ctx.elegir_pdf(fmt::anio_actual()),
            // Los años acabados están en «Años».
            KeyCode::Char('-' | '_' | '[') => {
                ctx.anio = (fmt::anio_actual() - 1).max(PRIMER_ANIO);
                ctx.ir_a = Some("Años");
            }
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "bajar"), ("o", "PDF del año")]
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        // Es siempre el año en curso: el de arriba también.
        ctx.anio = fmt::anio_actual();
        self.clave = None;
    }
}
