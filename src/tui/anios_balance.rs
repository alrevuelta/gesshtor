//! Un año entero (Enter en «Años»): lo que ganaste, lo que pagaste y lo que te quedó, cada cifra con de dónde sale
//! (en los años con Renta, sus casillas); y debajo, la Renta casilla a casilla.
use crate::dominio::fmt::DIF;
use super::anios::{COLOR_EMPRESA, COLOR_PAGAS, COLOR_QUEDA, Lienzo, explicada, vinetas};
use super::anios_renta as renta;
use crate::dominio::renta::Renta;
use crate::dominio::fmt::{self, eur, eur0};
use crate::informe::anio::{self, Concepto, Detalle, Fila, Fuente, euro, pct};
use crate::informe::{self};
use crate::tui::texto::{dos_columnas, envolver, fila};
use crate::tui::widgets::{kpi, tema};
use crate::tui::Ctx;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

/// Desde este ancho, lo que ganas y lo que pagas van en dos columnas.
const ANCHO_DOS_COLUMNAS: u16 = 100;

/// Lo de un año, ya calculado, y por dónde se va bajando.
#[derive(Default)]
pub(super) struct Vista {
    /// (generación de los datos, año) con que se calculó.
    clave: Option<(u64, i32)>,
    /// El año que se veía (al cambiar de año se vuelve arriba).
    visto: Option<i32>,
    d: Detalle,
    /// Su Renta, cuándo se presentó y, si aún no está, hasta cuándo se puede presentar.
    renta: Option<Renta>,
    presentada: Option<String>,
    pendiente: Option<String>,
    /// Tiene o tuvo nómina: se enseña lo que pagó su empresa.
    empleo: bool,
    scroll: u16,
    scroll_max: u16,
    lienzo: Lienzo,
}

fn gris() -> Style {
    tema::apagado()
}

fn negrita() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
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
        self.empleo = informe::situacion(&ctx.datos).empleo;
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

    /// Las cuatro cifras de arriba (tres si nunca tuvo nómina).
    fn kpis(&self, f: &mut Frame, area: Rect, grande: bool) {
        let a = &self.d.anio;
        let con_empresa = self.empleo || a.empresa > DIF;
        let n = if con_empresa { 4 } else { 3 };
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, n); n as usize]).split(area);
        let w = zonas[0].width.saturating_sub(2) as usize;
        let cabe = |largo: String, corto: &str| if largo.chars().count() <= w { largo } else { corto.to_string() };
        let ss = a.seguridad_social();
        let nota_ganas = if a.sueldo_aprox {
            "el sueldo, aproximado".to_string()
        } else if a.especie > DIF {
            cabe(format!("{} en especie", eur0(a.especie)), "con especie")
        } else {
            cabe("antes de impuestos".into(), "bruto")
        };
        let nota_pagas = if a.falta_nomina {
            cabe("sin el IRPF de tu nómina".into(), "sin IRPF nómina")
        } else {
            cabe(format!("IRPF {} · SS {}", eur0(a.de(Concepto::Irpf)), eur0(ss)), "IRPF y Seg. Social")
        };
        let nota_queda = if a.ganas > DIF { cabe(format!("el {} de lo que ganas", pct(a.queda, a.ganas)), &pct(a.queda, a.ganas)) } else { String::new() };
        let mut cifras = vec![
            ("Lo que ganas".to_string(), euro(a.ganas), nota_ganas, Style::new()),
            ("Lo que pagas".to_string(), eur(a.pagas), nota_pagas, Style::new().fg(COLOR_PAGAS)),
            (cabe("Lo que te queda".into(), "Te queda"), euro(a.queda), nota_queda, Style::new().fg(if a.queda < -DIF { tema::MAL } else { COLOR_QUEDA })),
        ];
        if con_empresa {
            let (v, nota) = if a.empresa > DIF { (eur(a.empresa), cabe("su Seguridad Social por ti".into(), "Seg. Social")) } else { ("—".to_string(), cabe(format!("sin nómina en {}", a.y), "sin nómina")) };
            cifras.push((cabe("Tu empresa pagó por ti".into(), "Tu empresa"), v, nota, Style::new().fg(COLOR_EMPRESA)));
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

    /// El recuadro con el desglose: el año y de dónde sale arriba; dentro, lo que ganas y lo que pagas, y la Renta.
    fn cuerpo(&mut self, f: &mut Frame, area: Rect) {
        let d = &self.d;
        let y = d.anio.y;
        let (modo, st_modo, resto) = if d.anio.con_renta {
            ("con Renta", Style::new().fg(tema::BIEN), format!("según tu Renta de {y} y la Seguridad Social"))
        } else if let Some(fin) = &self.pendiente {
            ("Renta pendiente", Style::new().fg(tema::AVISO), format!("se presenta hasta el {}: hasta entonces, con la Seguridad Social y lo apuntado", fmt::fecha_corta(fin)))
        } else {
            ("sin Renta", Style::new().fg(tema::AVISO), "con la Seguridad Social y lo apuntado".into())
        };
        let titulo = Line::from(vec![
            Span::styled(format!(" {y} "), tema::titulo()),
            Span::styled("· ", gris()),
            Span::styled(modo, st_modo.add_modifier(Modifier::BOLD)),
            Span::styled(format!(": {resto} "), gris()),
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
        let mut lineas = if dos {
            dos_columnas(self.ganas(wc), self.pagas(wc), wc)
        } else {
            let mut v = self.ganas(wc);
            v.push(Line::from(""));
            v.extend(self.pagas(wc));
            v
        };
        let notas = self.notas(if dos { (wc * 2 + 3).min(w) } else { wc });
        if !notas.is_empty() {
            lineas.push(Line::from(""));
            lineas.extend(notas);
        }
        if let Some(r) = &self.renta {
            lineas.push(Line::from(""));
            lineas.extend(renta::lineas(y, r, self.presentada.as_deref(), wc, dos));
        }

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

    fn cabecera(nombre: &str, valor: &str, w: usize) -> Line<'static> {
        fila(nombre, valor, w, tema::titulo(), negrita())
    }

    /// «Lo que ganas», concepto a concepto.
    fn ganas(&self, w: usize) -> Vec<Line<'static>> {
        let d = &self.d;
        let a = &d.anio;
        let mut l = vec![Self::cabecera("Lo que ganas", &euro(a.ganas), w)];
        if d.ganado.filas.is_empty() {
            l.push(Line::styled("Sin ingresos.", gris()));
        }
        for x in &d.ganado.filas {
            l.extend(explicada(x, w, 0, false));
        }
        l.extend(envolver(
            &[("Antes de impuestos y cotizaciones. De tu actividad cuenta lo facturado menos tus gastos, sin la cuota de autónomo (va en lo que pagas).".into(), gris())],
            w,
            0,
        ));
        l
    }

    /// «Lo que pagas»: el IRPF (lo que te tocó y cómo se pagó) y la Seguridad Social; y lo que pagó tu empresa.
    fn pagas(&self, w: usize) -> Vec<Line<'static>> {
        let d = &self.d;
        let a = &d.anio;
        let ir = &d.irpf;
        let mut l = vec![Self::cabecera("Lo que pagas", &eur(a.pagas), w)];

        // IRPF.
        let nombre = if ir.con_renta { "Hacienda: IRPF".to_string() } else { "Hacienda: IRPF adelantado".to_string() };
        let irpf = Fila { nombre, importe: Some(ir.total), fuentes: ir.fuente.clone(), concepto: Some(Concepto::Irpf), ..Default::default() };
        l.extend(explicada(&irpf, w, 0, false));
        if !ir.toco.is_empty() {
            for x in &ir.toco {
                l.extend(explicada(x, w, 2, true));
            }
        }
        if !ir.filas.is_empty() {
            l.push(Line::styled(if ir.con_renta { "  Cómo lo pagaste:" } else { "  Lo adelantado:" }, gris().add_modifier(Modifier::BOLD)));
            for x in &ir.filas {
                l.extend(explicada(x, w, 2, true));
            }
        } else if !ir.con_renta {
            l.push(Line::styled(format!("  Todavía no hay nada de IRPF en {}.", a.y), gris()));
        }
        if !ir.con_renta && a.y >= fmt::anio_actual() - 1 {
            let t = format!("La Renta de {} se presenta de abril a junio de {}: entonces sale tu IRPF del año.", a.y, a.y + 1);
            l.extend(envolver(&[(t, gris())], w, 2));
        }

        // Seguridad Social: tu parte de la nómina y tu cuota de autónomo.
        if let Some(n) = d.nomina.as_ref().filter(|n| n.con_nomina > 0) {
            let meses = n.texto_meses();
            let sol = if a.tu_solidaridad > DIF { format!("{} de solidaridad", eur(a.tu_solidaridad)) } else { String::new() };
            let x = Fila {
                nombre: "Seguridad Social: tu parte de la nómina".into(),
                importe: Some(n.cuota.trabajador),
                nota: sol,
                fuentes: vec![Fuente::Texto(format!("Seguridad Social · {meses}"))],
                concepto: Some(Concepto::TuNomina),
                ..Default::default()
            };
            l.extend(explicada(&x, w, 0, false));
        }
        if let Some(c) = &d.cuota {
            let x = Fila {
                nombre: "Seguridad Social: tu cuota de autónomo".into(),
                importe: Some(c.total),
                fuentes: vec![Fuente::Texto(c.de.clone())],
                concepto: Some(Concepto::Cuota),
                ..Default::default()
            };
            l.extend(explicada(&x, w, 0, false));
        }

        // Lo que pagó tu empresa (no suma: no sale de lo que ganas).
        if let Some(n) = d.nomina.as_ref().filter(|n| n.con_nomina > 0) {
            l.push(Line::from(""));
            l.push(fila("Tu empresa pagó por ti", &eur(n.cuota.empresa), w, tema::titulo(), Style::new().fg(COLOR_EMPRESA).add_modifier(Modifier::BOLD)));
            for (nombre, _, p) in n.grupos_con_algo() {
                let valor = if p.trabajador.abs() > DIF { format!("{}  tú {}", eur(p.empresa), eur(p.trabajador)) } else { eur(p.empresa) };
                let mut x = fila(nombre, &valor, w.saturating_sub(2), gris(), Style::new());
                x.spans.insert(0, Span::raw("  "));
                l.push(x);
            }
            if !n.empresas.is_empty() {
                let empresas: Vec<&str> = n.empresas.keys().map(String::as_str).collect();
                l.extend(envolver(&[(empresas.join(", "), gris().add_modifier(Modifier::ITALIC))], w, 2));
            }
            l.extend(envolver(&[("Es la parte de la Seguridad Social que paga tu empresa: no la ves en tu nómina, pero se gasta en ti.".into(), gris())], w, 2));
        }
        l
    }

    /// Lo que conviene saber, a todo lo ancho.
    fn notas(&self, w: usize) -> Vec<Line<'static>> {
        let d = &self.d;
        let mut v: Vec<(String, Style)> = d
            .notas
            .iter()
            .map(|n| {
                let fuerte = n.starts_with("Declaración conjunta") || n.starts_with("Falta");
                (n.clone(), if fuerte { Style::new().fg(tema::AVISO) } else { gris() })
            })
            .collect();
        if let Some(c) = &d.cuna {
            let de = if c.sueldo_de_renta { format!("tu Renta de {}", d.anio.y) } else { "por el que cotizaste".into() };
            let especie = if c.especie > DIF { format!(", de los que {} fueron en especie", eur(c.especie)) } else { String::new() };
            v.push((
                format!(
                    "Tu nómina le costó a tu empresa unos {}: {} de sueldo bruto ({de}) y {} de su Seguridad Social. De eso, {} ({}) fueron a Hacienda y a la Seguridad Social. A ti te quedaron {}{especie}.",
                    eur(c.coste),
                    eur(c.sueldo),
                    eur(c.empresa),
                    eur(c.impuestos),
                    pct(c.impuestos, c.coste),
                    eur(fmt::r2(c.coste - c.impuestos)),
                ),
                gris(),
            ));
        }
        if d.cuota.is_some() || d.nomina.as_ref().is_some_and(|n| n.con_nomina > 0) {
            v.push(("Las cotizaciones no son impuestos: dan derecho a pensión, paro y bajas. Aun así se suman a lo que pagas.".into(), gris()));
        }
        vinetas(v, w)
    }

    /// Bajar por el desglose. Devuelve true si ha usado la tecla.
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
