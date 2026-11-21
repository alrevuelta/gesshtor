//! «Utilidades»: herramientas que no dependen de tus datos. De momento, la calculadora de sueldos, con dos
//! pestañas (← →): Asalariado (de un sueldo bruto, lo que te queda, lo que te quitan y lo que le cuestas a tu
//! empresa) y Autónomo (de lo que facturas, lo que te queda, tu IRPF y tu cuota). Debajo, lo mismo en cada
//! comunidad, de más a menos: con ↑↓ se elige una y se ve cuánto cambia en las demás. Los datos (el sueldo, el
//! año…) se cambian con Enter; el detalle, concepto a concepto, con `d`. Las cuentas, en `informe::calculadora`.
//!
//! Estilo C2: lo que te queda al mes, en una banda, y a su lado el % que se va en impuestos y cotizaciones; el
//! reparto, en escalera (de lo que cuestas a lo que te queda, cada paso con qué es y su %); las comunidades, solo
//! con lo que te queda al mes y la diferencia, en una lista sin caja con la elegida en el fondo suave de la sección.
use crate::dominio::fmt::{self, eur, eur0, DIF};
use crate::informe::calculadora::{asalariado, asalariado_por_comunidad, autonomo, autonomo_por_comunidad, Asalariado, Autonomo, Concepto, EnComunidad};
use crate::hacienda::irpf::nombre_comunidad;
use crate::tgss::reglas;
use crate::tui::texto::envolver;
use crate::tui::widgets::{atajo, banda, barra, derecha, guia, linea_atajos, muestra, tema, titulo_bloque, Formulario, Resultado, ALTO_BANDA};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
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
    /// La clave de la comunidad elegida; None: la de tus datos (la de tu última Renta).
    comunidad: Option<String>,
    form: Option<Formulario>,
    /// Se ve el detalle, concepto a concepto, en vez de las comunidades.
    detalle: bool,
    scroll: u16,
    tabla: TableState,
}

impl Default for Calculadora {
    fn default() -> Self {
        Calculadora {
            pestana: Pestana::default(),
            anio: fmt::anio_actual(),
            bruto: 30000.0,
            pagas: 14,
            temporal: false,
            facturado: 40000.0,
            gastos: 5000.0,
            comunidad: None,
            form: None,
            detalle: false,
            scroll: 0,
            tabla: TableState::default(),
        }
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

/// «39,6 %» (un decimal, sin «,0»).
fn pct1(x: f64) -> String {
    let s = format!("{:.1}", (x * 10.0).round() / 10.0).replace('.', ",");
    format!("{} %", s.trim_end_matches(",0"))
}

/// «1.234,56 €», «−2.746,00 €»
fn importe(x: f64) -> String {
    if x < -DIF { format!("−{}", eur(-x)) } else { eur(x) }
}

/// «+1.234 €», «−80 €» (redondeado).
fn diferencia(x: f64) -> String {
    match x {
        x if x > 0.5 => format!("+{}", eur0(x)),
        x if x < -0.5 => format!("−{}", eur0(-x)),
        _ => "igual".into(),
    }
}

/// «el 21,8 %» de `sobre`.
fn de(x: f64, sobre: f64) -> String {
    if sobre.abs() > DIF { pct1(x / sobre * 100.0) } else { "—".into() }
}

/// Los colores del reparto (la barra fina y el «█» de cada línea): lo que te queda, la Seguridad Social (o la cuota
/// de autónomo), el IRPF y lo que paga tu empresa.
const QUEDA: Color = tema::BIEN;
const SS: Color = tema::AZUL;
const IRPF: Color = tema::AVISO;
const EMPRESA: Color = tema::MARCO;

/// Los datos («bruto 30.000 € al año», «pagas 14»…; sin nombre, solo el valor) en líneas de `w` columnas, sin
/// partir ninguno: el nombre en gris y el valor en negrita, del color de la sección.
fn repartir(campos: Vec<(&str, String)>, w: usize) -> Vec<Line<'static>> {
    let mut lineas = Vec::new();
    let mut l: Vec<Span> = Vec::new();
    let mut usado = 0;
    for (k, v) in campos {
        let k = if k.is_empty() { String::new() } else { format!("{k} ") };
        let largo = k.chars().count() + v.chars().count();
        if usado > 0 && usado + 3 + largo > w {
            lineas.push(Line::from(std::mem::take(&mut l)));
            usado = 0;
        }
        if usado > 0 {
            l.push(Span::raw("   "));
            usado += 3;
        }
        l.push(Span::styled(k, gris()));
        l.push(Span::styled(v, negrita().fg(tema::acento())));
        usado += largo;
    }
    if !l.is_empty() {
        lineas.push(Line::from(l));
    }
    lineas
}

/// La línea de una banda: la etiqueta, con dos de margen, y la cifra acabando en `fin`, sin puntos (como en los
/// bocetos); las dos en negrita y en `st`.
fn en_banda(et: &str, cifra: String, st: Style, fin: u16) -> Line<'static> {
    let hueco = (fin as usize).saturating_sub(2 + et.chars().count() + cifra.chars().count()).max(1);
    Line::from(vec![Span::raw("  "), Span::styled(et.to_string(), st), Span::raw(" ".repeat(hueco)), Span::styled(cifra, st)])
}

/// «41 %»: un porcentaje redondeado, el que se entiende de un vistazo.
fn pct0(x: f64, sobre: f64) -> String {
    if sobre.abs() > DIF { format!("{} %", (x / sobre * 100.0).round()) } else { "—".into() }
}

/// Un escalón del reparto: el «█» de su tramo (o nada), el signo («−», «=»), la etiqueta, los puntos y la cifra
/// acabando en `fin`; a la derecha, qué es (en gris) y, pegado al borde de `ancho`, su porcentaje de `sobre`. Si no
/// cabe la nota, solo el porcentaje.
#[allow(clippy::too_many_arguments)]
fn escalon(color: Option<Color>, signo: &str, et: &str, x: f64, st: Style, fin: u16, que_es: &str, sobre: Option<f64>, ancho: u16) -> Line<'static> {
    let marca = match color {
        Some(c) => muestra(c),
        None => Span::raw("  "),
    };
    let signo = if signo.is_empty() { String::new() } else { format!("{signo} ") };
    let mut l = guia(Line::from(vec![marca, Span::styled(signo, gris()), Span::styled(et.to_string(), st)]), eur(x), st, fin);
    let pct = sobre.map(|s| pct0(x, s)).unwrap_or_default();
    let usado = l.width();
    let libre = (ancho as usize).saturating_sub(usado + 3 + 6);
    if que_es.chars().count() + 2 <= libre {
        l.push_span(Span::styled(format!("   {que_es}"), gris()));
    }
    let hueco = (ancho as usize).saturating_sub(l.width() + pct.chars().count());
    l.push_span(Span::raw(" ".repeat(hueco.max(1))));
    l.push_span(Span::styled(pct, if st.add_modifier.contains(Modifier::BOLD) { st } else { gris() }));
    l
}

/// Lo que se calcula con los datos de la pestaña, en la comunidad elegida.
enum Cuenta {
    Asalariado(Asalariado),
    Autonomo(Autonomo),
}

impl Calculadora {
    /// Lo mismo en cada comunidad, de más a menos.
    fn comunidades(&self) -> Vec<EnComunidad> {
        match self.pestana {
            Pestana::Asalariado => asalariado_por_comunidad(self.bruto, self.temporal, self.anio),
            Pestana::Autonomo => autonomo_por_comunidad(self.facturado, self.gastos, self.anio),
        }
    }

    /// La comunidad elegida (su posición en `v`): la que eligieras, la de tus datos o, si no, la primera.
    fn elegida(&self, v: &[EnComunidad], ctx: &Ctx) -> usize {
        let tuya = || crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).map(|c| c.0.to_string());
        let c = self.comunidad.clone().or_else(tuya).unwrap_or_default();
        v.iter().position(|x| x.clave == c).unwrap_or(0)
    }

    fn cuenta(&self, comunidad: &str) -> Cuenta {
        match self.pestana {
            Pestana::Asalariado => Cuenta::Asalariado(asalariado(self.bruto, self.temporal, comunidad, self.anio)),
            Pestana::Autonomo => Cuenta::Autonomo(autonomo(self.facturado, self.gastos, comunidad, self.anio)),
        }
    }

    fn formulario(&self) -> Formulario {
        // Los años con reglas de cotización en la app, y el siguiente (con las últimas que tenga).
        let anios: Vec<String> = (2025..=fmt::anio_actual() + 1).map(|y| y.to_string()).collect();
        let anios: Vec<(&str, &str)> = anios.iter().map(|y| (y.as_str(), y.as_str())).collect();
        let fm = match self.pestana {
            Pestana::Asalariado => Formulario::new("Tu sueldo")
                .numero("bruto", "Sueldo bruto al año", Some(self.bruto))
                .ayuda("Todo lo del año, con las pagas extra.")
                .opciones("pagas", "Pagas", &[("12", "12"), ("14", "14")], &self.pagas.to_string())
                .ayuda("← → para cambiarlo. Solo cambia lo que cobras cada mes.")
                .opciones("contrato", "Contrato", &[("indefinido", "Indefinido"), ("temporal", "Temporal")], if self.temporal { "temporal" } else { "indefinido" })
                .ayuda("Con uno temporal, el desempleo es algo más caro."),
            Pestana::Autonomo => Formulario::new("Tu actividad")
                .numero("facturado", "Facturas al año (sin IVA)", Some(self.facturado))
                .numero("gastos", "Gastos de la actividad", Some(self.gastos))
                .ayuda("Sin la cuota de autónomo, que se calcula."),
        };
        fm.opciones("anio", "Año", &anios, &self.anio.to_string()).ayuda("← → para cambiarlo: las reglas de ese año.")
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
        self.anio = fm.get("anio").parse().unwrap_or(self.anio);
        self.scroll = 0;
    }

    /// Las pestañas (← → para pasar de una a otra), con la elegida en FONDO sobre el color de la sección; a la
    /// derecha, las teclas de cambiar los datos y del detalle.
    fn pintar_pestanas(&self, f: &mut Frame, fila: Rect) {
        let pestana = |t: &str, si: bool| Span::styled(format!(" {t} "), if si { tema::pestana_activa() } else { gris() });
        let l = Line::from(vec![
            Span::styled("← → ", negrita()),
            pestana("Asalariado", self.pestana == Pestana::Asalariado),
            Span::raw("  "),
            pestana("Autónomo", self.pestana == Pestana::Autonomo),
        ]);
        let usado = l.width();
        f.render_widget(Paragraph::new(l), fila);
        let teclas = linea_atajos(&[("Enter", "cambiar"), ("d", if self.detalle { "comunidades" } else { "detalle" })]);
        if usado + 5 + teclas.width() <= fila.width as usize {
            f.render_widget(Paragraph::new(teclas.right_aligned()), fila);
        }
    }

    /// Con qué se calcula: (nombre, valor).
    fn campos(&self, comunidad: &str) -> Vec<(&'static str, String)> {
        let mut campos: Vec<(&'static str, String)> = match self.pestana {
            Pestana::Asalariado => vec![
                ("bruto", format!("{} al año", eur0(self.bruto))),
                ("pagas", self.pagas.to_string()),
                ("contrato", if self.temporal { "temporal" } else { "indefinido" }.to_string()),
            ],
            Pestana::Autonomo => vec![("facturas", format!("{} al año, sin IVA", eur0(self.facturado))), ("gastos", eur0(self.gastos))],
        };
        campos.push(("año", self.anio.to_string()));
        // El nombre ya dice qué es («Comunidad de Madrid», «Andalucía»).
        campos.push(("", comunidad.to_string()));
        campos
    }

    /// Lo que se calcula, siempre a la vista (Enter para cambiarlo); debajo, si la app no tiene el IRPF de tu
    /// comunidad (País Vasco y Navarra, forales; Ceuta y Melilla, con su deducción), y si las reglas no son las del
    /// año elegido.
    fn datos(&self, comunidad: &str, ctx: &Ctx, w: u16) -> Vec<Line<'static>> {
        let mut v = repartir(self.campos(comunidad), w as usize);
        if let Some((c, _)) = crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).filter(|(c, _)| crate::hacienda::irpf::reglas(self.anio, c).autonomica.is_none()) {
            v.push(Line::styled(format!("{}: la app no tiene su IRPF", nombre_comunidad(c)), Style::new().fg(tema::AVISO)));
        }
        if let Some(n) = self.nota_reglas() {
            v.push(Line::styled(n, gris()));
        }
        v
    }

    /// Lo importante: lo que te queda al mes, en una banda (y, a la derecha si cabe, cuánto se va en impuestos y
    /// cotizaciones). Con 14 pagas, debajo,
    /// lo que sale en 12 (el neto del año entre 12: otras calculadoras lo dan así). Devuelve las filas usadas.
    fn pintar_banda(&self, f: &mut Frame, area: Rect, c: &Cuenta, fin: u16) -> u16 {
        let verde = negrita().fg(tema::BIEN);
        // A la derecha, lo que más importa después: cuánto se va en impuestos y cotizaciones.
        let (etiqueta, al_mes, se_va) = match c {
            Cuenta::Asalariado(s) => (format!("te queda al mes, en {} pagas", self.pagas), s.neto / self.pagas as f64, pct0(s.impuestos(), s.coste)),
            Cuenta::Autonomo(a) => ("te queda al mes".to_string(), a.neto / 12.0, pct0(a.impuestos(), a.antes())),
        };
        let fondo = Rect { width: (fin + 2).min(area.width), height: ALTO_BANDA.min(area.height), ..area };
        banda(f, fondo, tema::SUAVE_BIEN, en_banda(&etiqueta, eur(al_mes), verde, fin));
        let lado = Line::from(vec![Span::styled("se va el ", gris()), Span::styled(se_va, negrita().fg(tema::AVISO)), Span::styled(" en impuestos y cotizaciones", gris())]);
        let x = fondo.right() + 3;
        if area.height >= 2 && x + lado.width() as u16 <= area.right() {
            f.render_widget(Paragraph::new(lado), Rect { x, y: area.y + 1, width: area.right() - x, height: 1 });
        }
        let mut alto = fondo.height;
        if let Cuenta::Asalariado(s) = c
            && self.pagas == 14
            && area.height > alto
        {
            let l = guia(Line::from(vec![Span::raw("  "), Span::styled("en 12 pagas serían", gris())]), eur(s.neto / 12.0), Style::new(), fin);
            f.render_widget(Paragraph::new(l), Rect { y: area.y + alto, height: 1, ..area });
            alto += 1;
        }
        alto
    }

    /// El reparto, en escalera y en una sola vertical: de lo que le cuestas a tu empresa (o de lo que facturas, en
    /// autónomo), restando paso a paso, hasta lo que te queda; y debajo, lo que se llevan impuestos y cotizaciones,
    /// lo más importante. A la derecha de cada cifra, qué es y adónde va; y al final, su porcentaje. Encima, la barra
    /// fina con los colores de cada tramo (el «█» de su línea).
    fn reparto(&self, c: &Cuenta, fin: u16, ancho: u16) -> Vec<Line<'static>> {
        let n = Style::new();
        let fuerte = negrita();
        let queda = negrita().fg(tema::BIEN);
        let se_va = negrita().fg(tema::AVISO);
        let mut v = Vec::new();
        match c {
            Cuenta::Asalariado(s) => {
                let base = s.coste;
                v.push(barra(&[(s.neto, QUEDA), (s.ss_trabajador, SS), (s.irpf.total, IRPF), (s.ss_empresa, EMPRESA)], fin));
                v.push(escalon(None, "", "coste para tu empresa", s.coste, fuerte, fin, "lo que le cuestas en un año", Some(base), ancho));
                v.push(escalon(Some(EMPRESA), "−", "Seg. Social de la empresa", s.ss_empresa, n, fin, "la paga la empresa, aparte de tu sueldo", Some(base), ancho));
                v.push(escalon(None, "=", "tu sueldo bruto", s.bruto, fuerte, fin, "el de tu contrato", Some(base), ancho));
                v.push(escalon(Some(SS), "−", "tu Seguridad Social", s.ss_trabajador, n, fin, "de tu nómina: pensión, paro, formación", Some(base), ancho));
                v.push(escalon(Some(IRPF), "−", "IRPF", s.irpf.total, n, fin, "a cuenta de tu Renta: va a Hacienda", Some(base), ancho));
                v.push(escalon(Some(QUEDA), "=", "te queda", s.neto, queda, fin, "lo que llega a tu cuenta", Some(base), ancho));
                v.push(Line::from(""));
                v.push(escalon(None, "", "impuestos y cotizaciones", s.impuestos(), se_va, fin, "las dos Seguridades Sociales y el IRPF", Some(base), ancho));
                v.push(escalon(None, " ", "de ellos, en tu nómina", s.ss_trabajador + s.irpf.total, n, fin, "lo que te quitan: tu Seg. Social e IRPF", Some(base), ancho));
            }
            Cuenta::Autonomo(a) => {
                let base = a.antes();
                v.push(barra(&[(a.neto, QUEDA), (a.ss, SS), (a.irpf.total, IRPF)], fin));
                v.push(escalon(None, "", "lo que facturas", a.facturado, n, fin, "en un año, sin IVA (el IVA no es tuyo)", None, ancho));
                v.push(escalon(None, "−", "tus gastos", a.gastos, n, fin, "los de tu actividad", None, ancho));
                v.push(escalon(None, "=", "lo que ganas", base, fuerte, fin, "antes de cuota e IRPF", Some(base), ancho));
                let cuota = format!("{} al mes, tramo {}: pensión y bajas", eur0(a.ss / 12.0), a.tramo);
                v.push(escalon(Some(SS), "−", "tu cuota de autónomo", a.ss, n, fin, &cuota, Some(base), ancho));
                v.push(escalon(Some(IRPF), "−", "IRPF", a.irpf.total, n, fin, "con el 130 y en tu Renta: Hacienda", Some(base), ancho));
                v.push(escalon(Some(QUEDA), "=", "te queda", a.neto, queda, fin, "para ti", Some(base), ancho));
                v.push(Line::from(""));
                v.push(escalon(None, "", "impuestos y cotizaciones", a.impuestos(), se_va, fin, "tu cuota y el IRPF", Some(base), ancho));
            }
        }
        v
    }

    /// Lo que te queda en cada comunidad y cuánto más o menos que en la elegida, sin caja: la cabecera en gris y la
    /// elegida en el fondo suave de la sección.
    fn pintar_comunidades(&mut self, f: &mut Frame, area: Rect, v: &[EnComunidad], sel: usize, ctx: &Ctx) {
        let tuya = crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).map(|c| c.0);
        let elegida = &v[sel];
        let al_mes = match self.pestana {
            Pestana::Asalariado => self.pagas as f64,
            Pestana::Autonomo => 12.0,
        };
        let titulo = Rect { height: area.height.min(1), ..area };
        f.render_widget(Paragraph::new(titulo_bloque("te queda en cada comunidad")), titulo);
        let teclas = Line::from(atajo("↑↓", "elegir otra").to_vec());
        if 26 + 5 + teclas.width() <= area.width as usize {
            f.render_widget(Paragraph::new(teclas.right_aligned()), titulo);
        }
        let filas: Vec<Row> = v
            .iter()
            .map(|c| {
                let nombre = if Some(c.clave) == tuya { Line::from(vec![Span::raw(c.nombre), Span::styled(" · tuya", gris())]) } else { Line::from(c.nombre) };
                let dif = if c.clave == elegida.clave { Span::raw("") } else { Span::styled(diferencia((c.neto - elegida.neto) / al_mes), gris()) };
                Row::new([Cell::from(nombre), Cell::from(derecha(eur0(c.neto / al_mes))), Cell::from(Line::from(dif).right_aligned())])
            })
            .collect();
        let cab = Row::new([Cell::from("comunidad"), Cell::from(derecha("al mes")), Cell::from(derecha("diferencia al mes"))]).style(tema::cabecera_tabla());
        let anchos = [Constraint::Length(30), Constraint::Length(9), Constraint::Length(18)];
        let tabla = Table::new(filas, anchos).header(cab).column_spacing(2).row_highlight_style(tema::seleccion());
        self.tabla.select(Some(sel));
        let lista = Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area };
        f.render_stateful_widget(tabla, lista, &mut self.tabla);
    }

    /// Si las reglas del IRPF o de la Seguridad Social no son las del año elegido, de cuál son.
    fn nota_reglas(&self) -> Option<String> {
        let (ss, irpf) = match self.cuenta("") {
            Cuenta::Asalariado(s) => (s.anio_ss, s.anio_irpf),
            Cuenta::Autonomo(a) => (a.anio_ss, a.anio_irpf),
        };
        match (ss == self.anio, irpf == self.anio) {
            (true, true) => None,
            (true, false) => Some(format!("IRPF con las reglas de {irpf}: las de {} aún no están", self.anio)),
            _ => Some(format!("Seguridad Social de {ss} e IRPF de {irpf}: las de {} aún no están", self.anio)),
        }
    }

    /// El detalle, concepto a concepto, en la comunidad elegida: con puntos guía y, delante de lo que va a cada
    /// sitio, el «█» de su color en el reparto. Se baja con ↑↓.
    fn pintar_detalle(&mut self, f: &mut Frame, area: Rect, c: &Cuenta, nombre: &str) {
        let w = area.width.min(100) as usize;
        let lineas = match c {
            Cuenta::Asalariado(s) => lineas_asalariado(s, self.pagas, w),
            Cuenta::Autonomo(a) => lineas_autonomo(a, w),
        };
        let alto = area.height.saturating_sub(1);
        let max = (lineas.len() as u16).saturating_sub(alto);
        self.scroll = self.scroll.min(max);
        let titulo = Rect { height: area.height.min(1), ..area };
        f.render_widget(Paragraph::new(titulo_bloque(format!("el detalle · {nombre}"))), titulo);
        if max > 0 {
            let mas = if self.scroll >= max { "↑ más arriba" } else { "↓ más abajo" };
            f.render_widget(Paragraph::new(Line::styled(mas, gris()).right_aligned()), titulo);
        }
        let cuerpo = Rect { y: area.y + 1, height: alto, ..area };
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)), cuerpo);
    }
}

/* ---------------- el detalle ---------------- */

const SIN_COMUNIDADES: &str = "Sin País Vasco ni Navarra (con su propio IRPF) ni Ceuta y Melilla (con su deducción): la app no los calcula.";

/// Las líneas del detalle, con las cifras acabando en la misma columna (puntos guía).
struct Recibo {
    fin: u16,
    w: usize,
    lineas: Vec<Line<'static>>,
}

impl Recibo {
    fn new(w: usize) -> Recibo {
        Recibo { fin: w.min(76) as u16, w, lineas: Vec::new() }
    }

    /// Un paso de la cuenta: «= Tu sueldo bruto ······ 30.000,00 €». Con `color`, el «█» de su tramo delante.
    fn paso(&mut self, color: Option<Color>, et: &str, st: Style, x: f64, signo: &str) {
        let etiqueta = match color {
            Some(c) => Line::from(vec![muestra(c), Span::styled(et.trim_start().to_string(), st)]),
            None => Line::from(Span::styled(et.to_string(), st)),
        };
        self.lineas.push(guia(etiqueta, format!("{signo}{}", importe(x)), st, self.fin));
    }

    /// Un concepto de lo cotizado, en gris y más adentro (sin negrita: es el detalle del paso).
    fn concepto(&mut self, et: &str, x: f64) {
        let mut l = guia(Span::styled(format!("      {et}"), gris()), importe(x), gris(), self.fin);
        if let Some(s) = l.spans.last_mut() {
            s.style = gris();
        }
        self.lineas.push(l);
    }

    /// Las líneas de lo cotizado por conceptos, cada una con su tipo.
    fn conceptos(&mut self, v: &[Concepto]) {
        for c in v.iter().filter(|c| c.importe.abs() > DIF) {
            let et = match c.tipo {
                Some(t) => format!("{} · {}", c.nombre, tipo(t)),
                None => c.nombre.clone(),
            };
            self.concepto(&et, c.importe);
        }
    }

    /// Una nota en gris bajo un paso.
    fn nota(&mut self, t: String) {
        self.lineas.extend(envolver(&[(t, gris())], self.w, 4));
    }

    /// Las notas del final, cada una con «• » delante.
    fn vinetas(mut self, notas: Vec<String>) -> Vec<Line<'static>> {
        self.lineas.push(Line::from(""));
        for t in notas {
            let mut l = envolver(&[(t, gris())], self.w, 2);
            if let Some(s) = l.first_mut().and_then(|p| p.spans.first_mut()) {
                *s = Span::styled("• ", gris());
            }
            self.lineas.extend(l);
        }
        self.lineas
    }
}

fn lineas_asalariado(s: &Asalariado, pagas: u32, w: usize) -> Vec<Line<'static>> {
    let n = Style::new();
    let mut p = Recibo::new(w);
    p.paso(None, "Lo que le cuestas a tu empresa", negrita(), s.coste, "");
    p.paso(Some(EMPRESA), "  − Seguridad Social que paga tu empresa", n, s.ss_empresa, "−");
    p.conceptos(&s.empresa);
    p.paso(None, "= Tu sueldo bruto", negrita(), s.bruto, "");
    p.paso(Some(SS), "  − Tu Seguridad Social", n, s.ss_trabajador, "−");
    p.conceptos(&s.trabajador);
    p.paso(Some(IRPF), "  − IRPF", n, s.irpf.total, "−");
    let mut t = format!("base {} · estatal {} · autonómica {}", eur(s.irpf.base), eur(s.irpf.estatal), eur(s.irpf.autonomica));
    if s.irpf.deduccion > DIF {
        t += &format!(" · deducción por sueldo bajo {}", eur(s.irpf.deduccion));
    }
    p.nota(format!("{t} · el {} de tu bruto", de(s.irpf.total, s.bruto)));
    p.nota(format!("con tu Seguridad Social, te quitan {}", eur(s.ss_trabajador + s.irpf.total)));
    p.paso(Some(QUEDA), "= Te queda", negrita().fg(tema::BIEN), s.neto, "");
    // Con 14 pagas, también lo que sale en 12 (el neto del año entre 12), que es como lo dan otras calculadoras.
    let en_12 = if pagas == 14 { format!(" · {} en 12 pagas", eur(s.neto / 12.0)) } else { String::new() };
    p.nota(format!("al mes: {} en {pagas} pagas{en_12}", eur(s.neto / pagas as f64)));
    let g = reglas::general(s.anio_ss);
    let mut notas = vec![format!("Cotizaciones con las reglas de {} ({}); IRPF con las de {}.", s.anio_ss, g.norma, s.anio_irpf)];
    if s.tope {
        notas.push(format!("Tu sueldo pasa de la base máxima ({} al mes): por encima solo se paga la cotización de solidaridad.", eur(g.base_maxima)));
    }
    notas.push(format!("Accidentes de trabajo: el {} de un trabajo de oficina (lo paga la empresa).", tipo(reglas::AT_EP_OFICINA)));
    notas.push("El IRPF es el que te toca en la Renta, sin hijos ni otras rentas; lo que te retienen cada mes se le parece.".into());
    notas.push(SIN_COMUNIDADES.into());
    p.vinetas(notas)
}

fn lineas_autonomo(a: &Autonomo, w: usize) -> Vec<Line<'static>> {
    let n = Style::new();
    let mut p = Recibo::new(w);
    p.paso(None, "Lo que facturas (sin IVA)", negrita(), a.facturado, "");
    if a.gastos > DIF {
        p.paso(None, "  − Gastos de tu actividad", gris(), a.gastos, "−");
        p.paso(None, "= Lo que ganas", negrita(), a.antes(), "");
    }
    p.paso(Some(SS), "  − Tu cuota de autónomo", n, a.ss, "−");
    p.nota(format!("tramo {}: ganas {} al mes (con el {} de gastos genéricos) → base mínima {} × {} · tu cuota, {} al mes", a.tramo, eur(a.mensual_ss), tipo(a.gastos_genericos), eur(a.base), tipo(a.tipo), eur(a.ss / 12.0)));
    p.conceptos(&a.cuota);
    p.paso(Some(IRPF), "  − IRPF", n, a.irpf.total, "−");
    let mut t = format!("rendimiento {}", importe(a.rendimiento));
    if a.gdj > DIF {
        t += &format!(" − difícil justificación {}", eur(a.gdj));
    }
    if a.reduccion > DIF {
        t += &format!(" − reducción por rentas bajas {}", eur(a.reduccion));
    }
    p.nota(format!("{t} = base {} · el {} de lo que ganas", eur(a.irpf.base), de(a.irpf.total, a.antes())));
    p.nota(format!("con la cuota, te quitan {}", eur(a.impuestos())));
    p.paso(Some(QUEDA), "= Te queda", negrita().fg(tema::BIEN), a.neto, "");
    p.nota(format!("al mes: {}", eur(a.neto / 12.0)));
    let notas = vec![
        format!("Cotizaciones con las reglas de {} ({}); IRPF con las de {}.", a.anio_ss, a.norma, a.anio_irpf),
        "La cuota, con la base mínima de tu tramo y sin tarifa plana; el IVA no cuenta (se lo pasas a Hacienda).".into(),
        "IRPF en estimación directa simplificada, sin otras rentas: lo del 130 y las retenciones se descuentan en la Renta.".into(),
        SIN_COMUNIDADES.into(),
    ];
    p.vinetas(notas)
}

impl Pantalla for Calculadora {
    fn titulo(&self) -> &'static str {
        "Calcula tu sueldo"
    }

    /// Arriba, las pestañas y los datos; luego lo que te queda al mes, en una banda; el reparto (si cabe) y, debajo,
    /// las comunidades o el detalle (`d`).
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let v = self.comunidades();
        let sel = self.elegida(&v, ctx);
        let cuenta = self.cuenta(v[sel].clave);
        let fila = |y: u16, alto: u16| Rect { y, height: alto.min(area.bottom().saturating_sub(y)), ..area };
        let mut y = area.y;
        self.pintar_pestanas(f, fila(y, 1));
        y += 2;
        let datos = self.datos(v[sel].nombre, ctx, area.width);
        let alto_datos = datos.len() as u16;
        f.render_widget(Paragraph::new(datos), fila(y, alto_datos));
        y += alto_datos + 1;
        // Las cifras del reparto (y la de la banda) acaban en la misma columna.
        let fin = 46.min(area.width);
        y += self.pintar_banda(f, fila(y, area.bottom().saturating_sub(y)), &cuenta, fin) + 1;
        if self.detalle {
            self.pintar_detalle(f, fila(y, area.bottom().saturating_sub(y)), &cuenta, v[sel].nombre);
        } else {
            // El reparto, si deja sitio para unas cuantas comunidades (si no, está en el detalle).
            let reparto = self.reparto(&cuenta, fin, area.width);
            let alto = reparto.len() as u16;
            if area.bottom().saturating_sub(y) >= alto + 1 + 6 {
                f.render_widget(Paragraph::new(reparto), fila(y, alto));
                y += alto + 1;
            }
            self.pintar_comunidades(f, fila(y, area.bottom().saturating_sub(y)), &v, sel, ctx);
        }
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
            KeyCode::Enter => self.form = Some(self.formulario()),
            KeyCode::Char('d') => {
                self.detalle = !self.detalle;
                self.scroll = 0;
            }
            KeyCode::Esc if self.detalle => self.detalle = false,
            KeyCode::Up | KeyCode::PageUp if self.detalle => self.scroll = self.scroll.saturating_sub(if k.code == KeyCode::Up { 1 } else { 10 }),
            KeyCode::Down | KeyCode::PageDown if self.detalle => self.scroll = self.scroll.saturating_add(if k.code == KeyCode::Down { 1 } else { 10 }),
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                let v = self.comunidades();
                let i = self.elegida(&v, ctx);
                let j = match k.code {
                    KeyCode::Up => i.saturating_sub(1),
                    KeyCode::Down => (i + 1).min(v.len() - 1),
                    KeyCode::Home => 0,
                    _ => v.len() - 1,
                };
                self.comunidad = Some(v[j].clave.to_string());
            }
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.form.is_some() {
            return vec![("Tab ↓", "siguiente"), ("← →", "elegir"), ("Ctrl+S", "calcular"), ("Esc", "cancelar")];
        }
        let otra = if self.pestana == Pestana::Asalariado { "autónomo" } else { "asalariado" };
        if self.detalle {
            return vec![("↑↓", "bajar"), ("Esc", "volver"), ("← →", otra)];
        }
        vec![("↑↓", "comunidad"), ("Enter", "cambiar"), ("d", "detalle"), ("← →", otra)]
    }

    fn capturando(&self) -> bool {
        self.form.is_some()
    }

    /// Tiene su propio año (en «Tu sueldo»): el de arriba no cuenta.
    fn usa_anio(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::dominio::fmt::{anio_actual, eur};
    use crate::informe::calculadora::asalariado_por_comunidad;
    use crate::tui::prueba::{app_con, cargar, caso, ir, pantalla, teclas};

    /// Con ↑↓ se elige otra comunidad (sale en los datos y lo que te queda es lo suyo); si la tuya no la tiene la
    /// app, se dice. Con 14 pagas, también se ve lo que sale en 12.
    #[test]
    fn otra_comunidad() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        for r in d.lecturas.rentas.values_mut() {
            r.comunidad = "navarra".into();
        }
        d.lecturas.rentas.entry(2025).or_default().comunidad = "navarra".into();
        let mut a = app_con(d, &dir);
        ir(&mut a, "Calcula tu sueldo");
        let v = asalariado_por_comunidad(30000.0, false, anio_actual());
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Navarra (régimen foral): la app no tiene su IRPF"), "{p}");
        assert!(p.contains(&format!("año {}   {}", anio_actual(), v[0].nombre)), "sin la tuya, la primera:\n{p}");
        teclas(&mut a, "<down>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&format!("año {}   {}", anio_actual(), v[1].nombre)), "{p}");
        assert!(p.contains(&eur(v[1].neto / 14.0)), "lo de la elegida, al mes:\n{p}");
        assert!(p.contains("en 12 pagas serían ") && p.contains(&eur(v[1].neto / 12.0)), "en 12 pagas:\n{p}");
        // El detalle, de la elegida; Esc vuelve a las comunidades.
        teclas(&mut a, "d");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&format!("el detalle · {}", v[1].nombre)), "{p}");
        teclas(&mut a, "<pgdn>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&format!("en 14 pagas · {} en 12 pagas", eur(v[1].neto / 12.0))), "más abajo, también en 12 pagas:\n{p}");
        teclas(&mut a, "<esc>");
        assert!(pantalla(&mut a, 120, 36).contains("te queda en cada comunidad"));
    }
}
