//! «Utilidades»: herramientas que no dependen de tus datos. De momento, la calculadora de sueldos, con dos
//! pestañas (← →): Asalariado (de un sueldo bruto, lo que le cuestas a tu empresa y lo que te queda) y Autónomo (de
//! lo que facturas, tu cuota, tu IRPF y lo que te queda). Los datos (el año, la comunidad, el sueldo…) se ven siempre
//! arriba, en «Tus datos», y se cambian con Enter. Las cuentas, en `informe::calculadora`.
use super::anios_reparto::{importe, Pasos};
use super::empleo::pct1;
use crate::dominio::fmt::{self, eur, DIF};
use crate::hacienda::irpf::{nombre_comunidad, COMUNIDADES};
use crate::informe::calculadora::{asalariado, autonomo, Asalariado, Autonomo, Concepto};
use crate::tgss::reglas;
use crate::tui::anios::vinetas;
use crate::tui::widgets::{bloque, kpi, tema, Formulario, Resultado};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Pestana {
    #[default]
    Asalariado,
    Autonomo,
}

/// Los datos de la calculadora (no se guardan: son para probar).
pub struct Calculadora {
    pestana: Pestana,
    /// El año de las reglas (el suyo: no es el de arriba).
    anio: i32,
    bruto: f64,
    pagas: u32,
    temporal: bool,
    facturado: f64,
    gastos: f64,
    /// La clave de la comunidad; None: la de tus datos (la de tu última Renta).
    comunidad: Option<String>,
    form: Option<Formulario>,
    scroll: u16,
    scroll_max: u16,
}

impl Default for Calculadora {
    fn default() -> Self {
        Calculadora { pestana: Pestana::default(), anio: fmt::anio_actual(), bruto: 30000.0, pagas: 14, temporal: false, facturado: 40000.0, gastos: 5000.0, comunidad: None, form: None, scroll: 0, scroll_max: 0 }
    }
}

fn gris() -> Style {
    tema::apagado()
}

fn negrita() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// «23,60 %», «7 %»
fn tipo(t: f64) -> String {
    format!("{} %", fmt::num(t).trim_end_matches(",00"))
}

/// Las líneas de lo cotizado por conceptos, cada una con su tipo.
fn conceptos(p: &mut Pasos, v: &[Concepto]) {
    for c in v.iter().filter(|c| c.importe.abs() > DIF) {
        let et = match c.tipo {
            Some(t) => format!("{} · {}", c.nombre, tipo(t)),
            None => c.nombre.clone(),
        };
        p.detalle(&et, c.importe);
    }
}

/// «el 40,8 % de lo que le cuestas a tu empresa y el 21,8 % de tu bruto»
fn de(x: f64, sobre: f64) -> String {
    if sobre.abs() > DIF { pct1(x / sobre * 100.0) } else { "—".into() }
}

/// Los datos («Año 2026», «Pagas 14»…) en líneas de `w` columnas, sin partir ninguno.
fn repartir(campos: Vec<(&str, String)>, w: usize) -> Vec<Line<'static>> {
    let mut lineas = Vec::new();
    let mut l: Vec<Span> = Vec::new();
    let mut usado = 0;
    for (k, v) in campos {
        let largo = k.chars().count() + 1 + v.chars().count();
        if usado > 0 && usado + 3 + largo > w {
            lineas.push(Line::from(std::mem::take(&mut l)));
            usado = 0;
        }
        if usado > 0 {
            l.push(Span::raw("   "));
            usado += 3;
        }
        l.push(Span::styled(format!("{k} "), gris()));
        l.push(Span::styled(v, negrita().fg(tema::ACENTO)));
        usado += largo;
    }
    if !l.is_empty() {
        lineas.push(Line::from(l));
    }
    lineas
}

impl Calculadora {
    /// La comunidad con que se calcula: la elegida o la de tus datos.
    fn comunidad(&self, ctx: &Ctx) -> String {
        self.comunidad.clone().unwrap_or_else(|| crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).map(|c| c.0.to_string()).unwrap_or_default())
    }

    fn formulario(&self, ctx: &Ctx) -> Formulario {
        let com: Vec<(&str, &str)> = std::iter::once(("", "Sin elegir (escala estatal)")).chain(COMUNIDADES.iter().map(|(k, n)| (*k, *n))).collect();
        let c = self.comunidad(ctx);
        // Los años con reglas de cotización en la app, y el siguiente (con las últimas que tenga).
        let anios: Vec<String> = (2025..=fmt::anio_actual() + 1).map(|y| y.to_string()).collect();
        let anios: Vec<(&str, &str)> = anios.iter().map(|y| (y.as_str(), y.as_str())).collect();
        let fm = Formulario::new(match self.pestana {
            Pestana::Asalariado => "Tus datos · asalariado",
            Pestana::Autonomo => "Tus datos · autónomo",
        })
        .opciones("anio", "Año", &anios, &self.anio.to_string())
        .ayuda("← → para cambiarlo: las reglas de ese año.")
        .opciones("comunidad", "Comunidad", &com, &c)
        .ayuda("← → para elegirla: la parte autonómica del IRPF.");
        match self.pestana {
            Pestana::Asalariado => fm
                .numero("bruto", "Sueldo bruto al año", Some(self.bruto))
                .ayuda("Todo lo del año, con las pagas extra.")
                .opciones("pagas", "Pagas", &[("12", "12"), ("14", "14")], &self.pagas.to_string())
                .ayuda("← → para cambiarlo. Solo cambia lo que cobras cada mes.")
                .opciones("contrato", "Contrato", &[("indefinido", "Indefinido"), ("temporal", "Temporal")], if self.temporal { "temporal" } else { "indefinido" })
                .ayuda("Con uno temporal, el desempleo es algo más caro."),
            Pestana::Autonomo => fm
                .numero("facturado", "Facturas al año (sin IVA)", Some(self.facturado))
                .numero("gastos", "Gastos de la actividad", Some(self.gastos))
                .ayuda("Sin la cuota de autónomo, que se calcula."),
        }
    }

    /// Lo del formulario, a la calculadora.
    fn aplicar(&mut self, fm: &Formulario) {
        let n = |k: &str, antes: f64| fm.num(k).unwrap_or(antes).max(0.0);
        match self.pestana {
            Pestana::Asalariado => {
                self.bruto = n("bruto", self.bruto);
                self.pagas = if fm.get("pagas") == "12" { 12 } else { 14 };
                self.temporal = fm.get("contrato") == "temporal";
            }
            Pestana::Autonomo => {
                self.facturado = n("facturado", self.facturado);
                self.gastos = n("gastos", self.gastos);
            }
        }
        self.comunidad = Some(fm.get("comunidad"));
        self.anio = fm.get("anio").parse().unwrap_or(self.anio);
        self.scroll = 0;
    }

    /// Las pestañas.
    fn pestanas(&self) -> Line<'static> {
        let pestana = |t: &str, si: bool| Span::styled(format!(" {t} "), if si { tema::pestana_activa() } else { gris() });
        Line::from(vec![pestana("Asalariado", self.pestana == Pestana::Asalariado), Span::raw(" "), pestana("Autónomo", self.pestana == Pestana::Autonomo)])
    }

    /// Con qué se calcula: (nombre, valor).
    fn campos(&self, ctx: &Ctx) -> Vec<(&'static str, String)> {
        let c = self.comunidad(ctx);
        let com = if c.is_empty() { "sin elegir".to_string() } else { nombre_comunidad(&c).to_string() };
        let mut campos: Vec<(&'static str, String)> = vec![("Año", self.anio.to_string()), ("Comunidad", com)];
        match self.pestana {
            Pestana::Asalariado => campos.extend([
                ("Sueldo bruto", format!("{} al año", eur(self.bruto))),
                ("Pagas", self.pagas.to_string()),
                ("Contrato", if self.temporal { "temporal" } else { "indefinido" }.to_string()),
            ]),
            Pestana::Autonomo => campos.extend([("Facturas", format!("{} sin IVA", eur(self.facturado))), ("Gastos", eur(self.gastos))]),
        }
        campos
    }

    /// «Tus datos»: con qué se calcula, siempre a la vista (Enter para cambiarlos).
    fn pintar_datos(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let campos = self.campos(ctx);
        let b = bloque("Tus datos").title_bottom(Line::from(vec![Span::styled(" Enter ", tema::pestana_activa()), Span::styled(" cambiarlos ", gris())]).right_aligned());
        let b = b.padding(ratatui::widgets::Padding::horizontal(1));
        let lineas = repartir(campos, b.inner(area).width as usize);
        f.render_widget(Paragraph::new(lineas).block(b), area);
    }

    fn kpis(f: &mut Frame, area: Rect, c: &[(String, String, String, Style)]) {
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, c.len() as u32); c.len()]).split(area);
        for ((etq, v, nota, st), z) in c.iter().zip(zonas.iter()) {
            kpi(f, *z, etq, v, nota, *st);
        }
    }
}

/* ---------------- asalariado ---------------- */

/// Lo que se va de tu bruto: tu Seguridad Social y el IRPF.
fn de_tu_bruto(s: &Asalariado) -> f64 {
    fmt::r2(s.bruto - s.neto)
}

fn cifras_asalariado(s: &Asalariado, pagas: u32) -> Vec<(String, String, String, Style)> {
    let mal = Style::new().fg(tema::MAL);
    vec![
        ("Paga tu empresa".into(), eur(s.coste), "al año".into(), Style::new()),
        ("Tu bruto".into(), eur(s.bruto), format!("{} × {pagas}", eur(s.bruto / pagas as f64)), Style::new()),
        ("Te queda".into(), eur(s.neto), format!("{} × {pagas}", eur(s.neto / pagas as f64)), Style::new().fg(tema::BIEN)),
        ("Del total".into(), de(s.impuestos(), s.coste), "impuestos y SS".into(), mal),
        ("De tu bruto".into(), de(de_tu_bruto(s), s.bruto), "tu SS e IRPF".into(), mal),
    ]
}

fn lineas_asalariado(s: &Asalariado, pagas: u32, anio: i32, w: usize) -> Vec<Line<'static>> {
    let ss = Style::new().fg(tema::AVISO);
    let ir = Style::new().fg(tema::MAL);
    let mut p = Pasos::new(w);
    p.lineas.push(Line::styled(format!("De lo que paga tu empresa a lo que te queda, en {anio}"), tema::titulo()));
    p.paso("Lo que le cuestas a tu empresa", negrita(), s.coste, "");
    p.paso("  − Seguridad Social que paga tu empresa", ss, s.ss_empresa, "−");
    conceptos(&mut p, &s.empresa);
    p.paso("= Tu sueldo bruto", negrita(), s.bruto, "");
    p.paso("  − Tu Seguridad Social", ss, s.ss_trabajador, "−");
    conceptos(&mut p, &s.trabajador);
    p.paso("  − IRPF", ir, s.irpf.total, "−");
    let medio = if s.bruto > DIF { pct1(s.irpf.total / s.bruto * 100.0) } else { "—".into() };
    let mut t = format!("base {} · estatal {} · autonómica {}", eur(s.irpf.base), eur(s.irpf.estatal), eur(s.irpf.autonomica));
    if s.irpf.deduccion > DIF {
        t += &format!(" · deducción por sueldo bajo {}", eur(s.irpf.deduccion));
    }
    p.nota(format!("{t} · el {medio} de tu bruto"), w);
    p.paso("= Te queda", negrita().fg(tema::BIEN), s.neto, "");
    let otras = if pagas == 14 { 12 } else { 14 };
    p.nota(format!("al mes: {} en {pagas} pagas (o {} en {otras})", eur(s.neto / pagas as f64), eur(s.neto / otras as f64)), w);
    let mut l = p.lineas;
    l.push(Line::from(""));
    let total = format!(
        "Impuestos y cotizaciones: {}, el {} de lo que le cuestas a tu empresa. De tu bruto se van {} (tu Seguridad Social y el IRPF): el {}.",
        eur(s.impuestos()),
        de(s.impuestos(), s.coste),
        eur(de_tu_bruto(s)),
        de(de_tu_bruto(s), s.bruto)
    );
    l.extend(crate::tui::texto::envolver(&[(total, negrita())], w, 0));
    l.push(Line::from(""));
    let g = reglas::general(s.anio_ss);
    let mut notas = vec![cuales(s.anio_ss, s.anio_irpf, anio, g.norma)];
    if s.tope {
        notas.push(format!(
            "Tu sueldo pasa de la base máxima ({} al mes): por encima solo se paga la cotización de solidaridad (desde 2025), por tramos y repartida como las contingencias comunes.",
            eur(g.base_maxima)
        ));
    } else {
        notas.push(format!("Se cotiza por todo tu sueldo hasta {} al mes (la base máxima); por encima, solo la cotización de solidaridad.", eur(g.base_maxima)));
    }
    notas.push(format!("Accidentes de trabajo: el {} de un trabajo de oficina; en otras actividades es más (lo paga solo la empresa).", tipo(reglas::AT_EP_OFICINA)));
    notas.push("El IRPF es el que te toca en la Renta, sin hijos ni discapacidad y sin otras rentas; lo que te retienen cada mes se le parece, pero no es igual.".into());
    if s.sin_comunidad {
        notas.push("Sin comunidad (o con una que la app no tiene), la escala estatal hace también de autonómica: es una aproximación.".into());
    }
    notas.push("Las cotizaciones no son impuestos: dan derecho a pensión, paro y bajas; aun así, se cuentan con ellos.".into());
    l.extend(vinetas(notas.into_iter().map(|n| (n, gris())).collect(), w));
    l
}

/// De qué año son las reglas, si no son las del año elegido.
fn cuales(ss: i32, irpf: i32, anio: i32, norma: &str) -> String {
    let mut t = format!("Cotizaciones con las reglas de {ss} ({norma})");
    if ss != anio {
        t += &format!(", que son las más cercanas a {anio} que tiene la app");
    }
    t += &format!("; IRPF con las de {irpf}");
    if irpf != anio {
        t += &format!(" (las de {anio} aún no las tiene la app)");
    }
    t + "."
}

/* ---------------- autónomo ---------------- */

fn cifras_autonomo(a: &Autonomo) -> Vec<(String, String, String, Style)> {
    let mal = Style::new().fg(tema::MAL);
    vec![
        ("Facturas".into(), eur(a.facturado), "al año, sin IVA".into(), Style::new()),
        ("Tu cuota".into(), eur(a.ss), format!("{} × 12", eur(a.ss / 12.0)), Style::new().fg(tema::AVISO)),
        ("Te queda".into(), eur(a.neto), format!("{} × 12", eur(a.neto / 12.0)), Style::new().fg(tema::BIEN)),
        ("De lo que ganas".into(), de(a.impuestos(), a.antes()), "cuota e IRPF".into(), mal),
        ("De lo facturado".into(), de(a.impuestos(), a.facturado), "cuota e IRPF".into(), mal),
    ]
}

fn lineas_autonomo(a: &Autonomo, anio: i32, w: usize) -> Vec<Line<'static>> {
    let ss = Style::new().fg(tema::AVISO);
    let ir = Style::new().fg(tema::MAL);
    let mut p = Pasos::new(w);
    p.lineas.push(Line::styled(format!("De lo que facturas a lo que te queda, en {anio}"), tema::titulo()));
    p.paso("Lo que facturas (sin IVA)", negrita(), a.facturado, "");
    if a.gastos > DIF {
        p.paso("  − Gastos de tu actividad", gris(), a.gastos, "−");
        p.paso("= Lo que ganas", negrita(), a.antes(), "");
    }
    p.paso("  − Tu cuota de autónomo", ss, a.ss, "−");
    p.nota(
        format!(
            "tramo {}: ganas {} al mes (con el {} de gastos genéricos) → base mínima {} × {}",
            a.tramo,
            eur(a.mensual_ss),
            tipo(a.gastos_genericos),
            eur(a.base),
            tipo(a.tipo)
        ),
        w,
    );
    conceptos(&mut p, &a.cuota);
    p.paso("  − IRPF", ir, a.irpf.total, "−");
    let mut t = format!("rendimiento {}", importe(a.rendimiento));
    if a.gdj > DIF {
        t += &format!(" − difícil justificación {}", eur(a.gdj));
    }
    if a.reduccion > DIF {
        t += &format!(" − reducción por rentas bajas {}", eur(a.reduccion));
    }
    let medio = de(a.irpf.total, a.antes());
    p.nota(format!("{t} = base {} · el {medio} de lo que ganas", eur(a.irpf.base)), w);
    p.paso("= Te queda", negrita().fg(tema::BIEN), a.neto, "");
    p.nota(format!("al mes: {}", eur(a.neto / 12.0)), w);
    let mut l = p.lineas;
    l.push(Line::from(""));
    let total = format!(
        "Impuestos y cotizaciones: {}, el {} de lo que ganas (lo facturado menos los gastos) y el {} de lo que facturas.",
        eur(a.impuestos()),
        de(a.impuestos(), a.antes()),
        de(a.impuestos(), a.facturado)
    );
    l.extend(crate::tui::texto::envolver(&[(total, negrita())], w, 0));
    l.push(Line::from(""));
    let mut notas = vec![cuales(a.anio_ss, a.anio_irpf, anio, a.norma)];
    notas.push("La cuota, con la base mínima de tu tramo todo el año (puedes elegir una más alta) y sin tarifa plana ni otras bonificaciones; como persona física, no societario.".into());
    notas.push("El IVA no cuenta: lo cobras a tus clientes y se lo pasas a Hacienda.".into());
    notas.push("IRPF en estimación directa simplificada, sin otras rentas. Lo que adelantas en el 130 y lo que te retienen tus clientes se descuenta en la Renta: no es más impuesto.".into());
    if a.sin_comunidad {
        notas.push("Sin comunidad (o con una que la app no tiene), la escala estatal hace también de autonómica: es una aproximación.".into());
    }
    notas.push("Las cotizaciones no son impuestos: dan derecho a pensión, paro y bajas; aun así, se cuentan con ellos.".into());
    l.extend(vinetas(notas.into_iter().map(|n| (n, gris())).collect(), w));
    l
}

impl Pantalla for Calculadora {
    fn titulo(&self) -> &'static str {
        "Calcula tu sueldo"
    }

    fn tecla_menu(&self) -> Option<char> {
        Some('C')
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let anio = self.anio;
        let com = self.comunidad(ctx);
        let grande = area.height >= 24;
        // «Tus datos», con las líneas que haga falta para no partir ninguno.
        let ancho_datos = area.width.saturating_sub(4) as usize;
        let alto_datos = repartir(self.campos(ctx), ancho_datos).len() as u16 + 2;
        let [a_pest, a_datos, a_kpi, a_cuerpo] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(alto_datos), Constraint::Length(if grande { 5 } else { 0 }), Constraint::Min(0)]).areas(area);
        f.render_widget(Paragraph::new(self.pestanas()), a_pest);
        self.pintar_datos(f, a_datos, ctx);
        let b = bloque("Calculadora de sueldos");
        let dentro = b.inner(a_cuerpo);
        let w = (dentro.width as usize).min(100);
        let (cifras, lineas) = match self.pestana {
            Pestana::Asalariado => {
                let s = asalariado(self.bruto, self.temporal, &com, anio);
                (cifras_asalariado(&s, self.pagas), lineas_asalariado(&s, self.pagas, anio, w))
            }
            Pestana::Autonomo => {
                let a = autonomo(self.facturado, self.gastos, &com, anio);
                (cifras_autonomo(&a), lineas_autonomo(&a, anio, w))
            }
        };
        if grande {
            Self::kpis(f, a_kpi, &cifras);
        }
        self.scroll_max = (lineas.len() as u16).saturating_sub(dentro.height);
        self.scroll = self.scroll.min(self.scroll_max);
        let b = if self.scroll_max > 0 {
            let t = if self.scroll >= self.scroll_max { " ↑ más arriba " } else { " ↓ más abajo (↑↓) " };
            b.title_bottom(Line::styled(t, gris()).right_aligned())
        } else {
            b
        };
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)).block(b), a_cuerpo);
        if let Some(fm) = &self.form {
            fm.dibujar_ventana(f, area, 64);
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if let Some(fm) = &mut self.form {
            match fm.tecla(k) {
                Resultado::Guardar => {
                    let fm = self.form.take().expect("abierto");
                    self.aplicar(&fm);
                }
                Resultado::Cancelar => self.form = None,
                Resultado::Sigue | Resultado::Cambio => {}
            }
            return true;
        }
        match k.code {
            KeyCode::Left | KeyCode::Right => {
                self.pestana = if self.pestana == Pestana::Asalariado { Pestana::Autonomo } else { Pestana::Asalariado };
                self.scroll = 0;
            }
            KeyCode::Enter => self.form = Some(self.formulario(ctx)),
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.form.is_some() {
            return vec![("Tab ↓", "siguiente"), ("← →", "elegir"), ("Ctrl+S", "calcular"), ("Esc", "cancelar")];
        }
        vec![("← →", if self.pestana == Pestana::Asalariado { "autónomo" } else { "asalariado" }), ("Enter", "cambiar los datos"), ("↑↓", "bajar")]
    }

    fn capturando(&self) -> bool {
        self.form.is_some()
    }

    /// Tiene su propio año (en «Tus datos»): el de arriba no cuenta.
    fn usa_anio(&self) -> bool {
        false
    }
}
