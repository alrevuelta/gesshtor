//! «Utilidades»: herramientas que no dependen de tus datos. De momento, la calculadora de sueldos, en dos
//! pestañas: «Calcula tu sueldo» (de un sueldo bruto, lo que te queda, lo que te quitan y lo que le cuestas a tu
//! empresa) y «Calcula como autónomo» (de lo que facturas, lo que te queda, tu IRPF y tu cuota). Los datos (el
//! sueldo, las pagas, el año, la comunidad…), uno debajo de otro: ↑↓ para ir de uno a otro, ← → para cambiar los
//! de elegir y Enter para escribir los importes; todo se recalcula al momento. En autónomo, también la cuota (la
//! mínima de tu tramo o una más alta, con ← →: las bases de referencia de `reglas::Autonomos::bases`) y la tarifa
//! plana del primer año; los años, de 2020 en adelante (los que la app tiene enteros). Debajo, lo mismo en cada comunidad,
//! de más a menos, y cuánto cambia respecto a la elegida; el detalle, concepto a concepto, con `d`. Las cuentas, en
//! `informe::calculadora`.
//!
//! Estilo C2, sin adornos y todo en una vertical: los datos; el reparto en escalera (de lo que cuestas a lo que te
//! queda al año), lo que te queda al mes en verde y el % que se va en impuestos y cotizaciones; y las comunidades,
//! en una lista sin caja con la elegida resaltada.
use crate::dominio::fmt::{self, DIF, eur, eur0};
use crate::hacienda::irpf::nombre_comunidad;
use crate::informe::calculadora::{
    Asalariado, Autonomo, Concepto, Cotiza, EnComunidad, anio_por_defecto, anios_con_reglas, asalariado, asalariado_por_comunidad, autonomo,
    autonomo_por_comunidad,
};
use crate::tgss::reglas;
use crate::tui::texto::envolver;
use crate::tui::widgets::{Entrada, derecha, guia, tema, titulo_bloque};
use crate::tui::{Ctx, Pantalla};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Pestana {
    #[default]
    Asalariado,
    Autonomo,
}

/// Cada dato de la calculadora: los importes se escriben (Enter) y los demás se eligen con ← →.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Dato {
    Bruto,
    Pagas,
    Contrato,
    Facturado,
    Gastos,
    Cuota,
    TarifaPlana,
    Anio,
    Comunidad,
}

impl Dato {
    fn nombre(self) -> &'static str {
        match self {
            Dato::Bruto => "bruto",
            Dato::Pagas => "pagas",
            Dato::Contrato => "contrato",
            Dato::Facturado => "facturas",
            Dato::Gastos => "gastos",
            Dato::Cuota => "cuota",
            Dato::TarifaPlana => "tarifa plana",
            Dato::Anio => "año",
            Dato::Comunidad => "comunidad",
        }
    }

    /// ¿Un importe, que se escribe? (los demás se eligen)
    fn importe(self) -> bool {
        matches!(self, Dato::Bruto | Dato::Facturado | Dato::Gastos)
    }
}

const DE_ASALARIADO: [Dato; 5] = [Dato::Bruto, Dato::Pagas, Dato::Contrato, Dato::Anio, Dato::Comunidad];
const DE_AUTONOMO: [Dato; 6] = [Dato::Facturado, Dato::Gastos, Dato::Cuota, Dato::TarifaPlana, Dato::Anio, Dato::Comunidad];

/// Las columnas de los datos: la marca del foco, el nombre (el más largo, «tarifa plana») y el valor (el más largo,
/// «‹ 1.541,58 € al mes, la máxima ›», con un espacio de fondo a cada lado).
const ANCHO_NOMBRE: u16 = 12;
const ANCHO_VALOR: u16 = 34;
const ANCHO_DATOS: u16 = 2 + ANCHO_NOMBRE + 2 + ANCHO_VALOR;
/// La columna donde acaban las cifras del reparto (la etiqueta más larga, «− Seguridad Social pagada por la
/// empresa», con unos puntos y la cifra).
const FIN: u16 = 56;

/// Los datos de la calculadora (no se guardan: son para probar).
pub struct Calculadora {
    pestana: Pestana,
    /// El año de las reglas (el suyo: no es el de arriba); solo uno que la app tenga entero.
    anio: i32,
    bruto: f64,
    pagas: u32,
    temporal: bool,
    facturado: f64,
    gastos: f64,
    /// La base de autónomo que eliges (None: la mínima de tu tramo) y si tienes la tarifa plana.
    base: Option<f64>,
    tarifa_plana: bool,
    /// La clave de la comunidad elegida; None: la de tus datos (la de tu última Renta).
    comunidad: Option<String>,
    /// El dato con el foco (en `datos_de()`).
    foco: usize,
    /// Escribiendo un importe: lo escrito y lo que había (para Esc). Se recalcula a cada tecla.
    escribiendo: Option<(Entrada, f64)>,
    /// Se ve el detalle, concepto a concepto, en vez de las comunidades.
    detalle: bool,
    scroll: u16,
    tabla: TableState,
}

impl Default for Calculadora {
    fn default() -> Self {
        Calculadora {
            pestana: Pestana::default(),
            anio: anio_por_defecto(),
            bruto: 30000.0,
            pagas: 14,
            temporal: false,
            facturado: 40000.0,
            gastos: 5000.0,
            base: None,
            tarifa_plana: false,
            comunidad: None,
            foco: 0,
            escribiendo: None,
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
    if x < -DIF {
        format!("−{}", eur(-x))
    } else {
        eur(x)
    }
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
    if sobre.abs() > DIF {
        pct1(x / sobre * 100.0)
    } else {
        "—".into()
    }
}

/// Lo escrito en un importe, si es un número («30000», «30.000,50 €»); si no (o vacío), None.
fn leer_importe(s: &str) -> Option<f64> {
    let s = s.trim();
    let es_numero = !s.is_empty()
        && s.chars().any(|c| c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_digit() || ".,€ ".contains(c));
    es_numero.then(|| fmt::parse_num(s).max(0.0))
}

/// Un importe para escribir encima: «30000», «1234,5».
fn para_escribir(x: f64) -> String {
    fmt::num(x).replace('.', "").trim_end_matches(",00").to_string()
}

/// La línea de lo que te queda al mes, sobre su fondo: la etiqueta, con dos de margen, y la cifra acabando en `fin`,
/// sin puntos; las dos en negrita y en `st`.
fn en_banda(et: &str, cifra: String, st: Style, fin: u16) -> Line<'static> {
    let hueco = (fin as usize)
        .saturating_sub(2 + et.chars().count() + cifra.chars().count())
        .max(1);
    Line::from(vec![
        Span::raw("  "),
        Span::styled(et.to_string(), st),
        Span::raw(" ".repeat(hueco)),
        Span::styled(cifra, st),
    ])
}

/// «pagas el 41 % en impuestos (12 %) y cotizaciones (29 %)», de `sobre` (lo que cuestas, o lo que ganas en
/// autónomo): el total, en naranja; los impuestos son el IRPF, y las cotizaciones, lo que falta hasta el total
/// (así las dos partes suman lo que se ve, aunque cada una redondee).
fn lo_que_pagas(irpf: f64, cotizaciones: f64, sobre: f64) -> Line<'static> {
    if sobre.abs() <= DIF {
        return Line::styled("pagas — en impuestos y cotizaciones", gris());
    }
    // + 0.0: sin «-0 %».
    let pct = |x: f64| (x / sobre * 100.0).round() + 0.0;
    let (total, impuestos) = (pct(irpf + cotizaciones), pct(irpf));
    Line::from(vec![
        Span::styled("pagas el ", gris()),
        Span::styled(format!("{total} %"), negrita().fg(tema::AVISO)),
        Span::styled(" en impuestos (", gris()),
        Span::styled(format!("{impuestos} %"), negrita()),
        Span::styled(") y cotizaciones (", gris()),
        Span::styled(format!("{} %", total - impuestos + 0.0), negrita()),
        Span::styled(")", gris()),
    ])
}

/// Un escalón del reparto: el signo («−», «=») en su columna, la etiqueta, los puntos y la cifra acabando en `fin`.
fn escalon(signo: &str, et: &str, x: f64, st: Style, fin: u16) -> Line<'static> {
    guia(
        Line::from(vec![
            Span::styled(format!("{signo:<2}"), gris()),
            Span::styled(et.to_string(), st),
        ]),
        eur(x),
        st,
        fin,
    )
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
            Pestana::Autonomo => autonomo_por_comunidad(self.facturado, self.gastos, self.cotiza(), self.anio),
        }
    }

    /// La comunidad elegida (su posición en `v`): la que eligieras, la de tus datos o, si no, la primera.
    fn elegida(&self, v: &[EnComunidad], ctx: &Ctx) -> usize {
        let tuya =
            || crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).map(|c| c.0.to_string());
        let c = self.comunidad.clone().or_else(tuya).unwrap_or_default();
        v.iter().position(|x| x.clave == c).unwrap_or(0)
    }

    fn cuenta(&self, comunidad: &str) -> Cuenta {
        match self.pestana {
            Pestana::Asalariado => {
                Cuenta::Asalariado(asalariado(self.bruto, self.temporal, comunidad, self.anio))
            }
            Pestana::Autonomo => {
                Cuenta::Autonomo(autonomo(self.facturado, self.gastos, self.cotiza(), comunidad, self.anio))
            }
        }
    }

    fn cotiza(&self) -> Cotiza {
        Cotiza { base: self.base, tarifa_plana: self.tarifa_plana }
    }

    /// La cuenta del autónomo para lo que no depende de la comunidad (la cuota).
    fn de_autonomo(&self) -> Autonomo {
        autonomo(self.facturado, self.gastos, self.cotiza(), "", self.anio)
    }

    /// Los datos de esta pestaña, en orden.
    fn datos_de(&self) -> &'static [Dato] {
        match self.pestana {
            Pestana::Asalariado => &DE_ASALARIADO,
            Pestana::Autonomo => &DE_AUTONOMO,
        }
    }

    fn dato(&self) -> Dato {
        let v = self.datos_de();
        v[self.foco.min(v.len() - 1)]
    }

    /// Lo que vale un importe.
    fn importe_de(&self, d: Dato) -> f64 {
        match d {
            Dato::Bruto => self.bruto,
            Dato::Facturado => self.facturado,
            Dato::Gastos => self.gastos,
            _ => 0.0,
        }
    }

    fn poner_importe(&mut self, d: Dato, x: f64) {
        match d {
            Dato::Bruto => self.bruto = x,
            Dato::Facturado => self.facturado = x,
            Dato::Gastos => self.gastos = x,
            _ => {}
        }
        self.scroll = 0;
    }

    /// Cómo se ve un dato: «30.000 € al año», «14», «Comunidad de Madrid»…
    fn valor(&self, d: Dato, comunidad: &str) -> String {
        match d {
            Dato::Bruto => format!("{} al año", eur0(self.bruto)),
            Dato::Pagas => self.pagas.to_string(),
            Dato::Contrato => if self.temporal { "temporal" } else { "indefinido" }.into(),
            Dato::Facturado => format!("{} al año, sin IVA", eur0(self.facturado)),
            Dato::Gastos => format!("{} al año", eur0(self.gastos)),
            Dato::Cuota => {
                let a = self.de_autonomo();
                let cual = match () {
                    _ if a.tarifa_plana => ", tarifa plana",
                    _ if a.base <= a.base_minima + DIF => ", la mínima",
                    _ if a.base >= a.base_maxima - DIF => ", la máxima",
                    _ => "",
                };
                format!("{} al mes{cual}", eur(a.cuota_mes))
            }
            Dato::TarifaPlana if !self.se_elige(d) => format!("no la hay en {}", self.anio),
            Dato::TarifaPlana => if self.tarifa_plana { "sí" } else { "no" }.into(),
            Dato::Anio => self.anio.to_string(),
            Dato::Comunidad => comunidad.to_string(),
        }
    }

    /// ¿Se puede cambiar con ← →? (el año, solo si hay más de uno con todas sus reglas; la tarifa plana, si ese año
    /// la hay)
    fn se_elige(&self, d: Dato) -> bool {
        match d {
            Dato::Anio => anios_con_reglas().len() > 1,
            Dato::TarifaPlana => reglas::autonomos(self.anio).tarifa_plana.is_some(),
            Dato::Cuota => !self.tarifa_plana,
            d => !d.importe(),
        }
    }

    /// ← → en un dato de elegir: las pagas y el contrato van y vienen; el año, la cuota (sus bases de referencia) y
    /// la comunidad (en el orden de la lista de abajo), al anterior o al siguiente.
    fn cambiar(&mut self, d: Dato, mas: bool, ctx: &Ctx) {
        self.fijar_comunidad(ctx);
        let paso = |i: usize, n: usize| if mas { (i + 1).min(n - 1) } else { i.saturating_sub(1) };
        match d {
            Dato::Pagas => self.pagas = if self.pagas == 12 { 14 } else { 12 },
            Dato::Contrato => self.temporal = !self.temporal,
            Dato::TarifaPlana => self.tarifa_plana = !self.tarifa_plana,
            Dato::Cuota => {
                let a = self.de_autonomo();
                let v = reglas::autonomos(a.anio_ss).bases(a.tramo);
                let i = v.iter().position(|&b| b >= a.base - DIF).unwrap_or(0);
                let j = paso(i, v.len());
                self.base = (j > 0).then(|| v[j]);
            }
            Dato::Anio => {
                let v = anios_con_reglas();
                if let Some(i) = v.iter().position(|&y| y == self.anio) {
                    self.anio = v[paso(i, v.len())];
                }
            }
            Dato::Comunidad => {
                let v = self.comunidades();
                let i = self.elegida(&v, ctx);
                self.comunidad = Some(v[paso(i, v.len())].clave.to_string());
            }
            _ => {}
        }
        self.scroll = 0;
    }

    /// La comunidad que se ve, fija antes de cambiar un dato: si no, la lista se reordena y, sin la tuya, la
    /// elegida (la primera) sería otra.
    fn fijar_comunidad(&mut self, ctx: &Ctx) {
        if self.comunidad.is_none() {
            let v = self.comunidades();
            self.comunidad = Some(v[self.elegida(&v, ctx)].clave.to_string());
        }
    }

    /// El foco al dato de abajo (o al de arriba), sin dar la vuelta.
    fn mover(&mut self, abajo: bool) {
        let n = self.datos_de().len();
        self.foco = if abajo { (self.foco + 1).min(n - 1) } else { self.foco.saturating_sub(1) };
    }

    /// Deja de escribir: con lo escrito, si es un importe (si no, o con `cancelar`, lo que había).
    fn terminar(&mut self, cancelar: bool) {
        if let Some((e, antes)) = self.escribiendo.take() {
            let x = if cancelar { None } else { leer_importe(&e.valor) };
            self.poner_importe(self.dato(), x.unwrap_or(antes));
        }
    }

    /// Los datos, uno debajo de otro: el nombre en gris y el valor en negrita, del color de la sección; el del foco,
    /// con «›» y su valor sobre el fondo suave de la sección (los de elegir, entre ‹ ›). Escribiendo, ahí el cursor.
    fn pintar_datos(&self, f: &mut Frame, area: Rect, comunidad: &str) {
        let acento = negrita().fg(tema::acento());
        for (i, &d) in self.datos_de().iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.bottom() {
                break;
            }
            let fila = Rect { y, height: 1, ..area };
            let foco = i == self.foco && !self.detalle;
            let nombre = format!("{:<w$}", d.nombre(), w = ANCHO_NOMBRE as usize);
            let l = if foco {
                Line::from(vec![Span::styled("› ", acento), Span::styled(nombre, acento)])
            } else {
                Line::from(vec![Span::raw("  "), Span::styled(nombre, gris())])
            };
            f.render_widget(Paragraph::new(l), fila);
            let x = area.x + 2 + ANCHO_NOMBRE + 2;
            let caja = Rect { x, width: ANCHO_VALOR.min(area.right().saturating_sub(x)), ..fila };
            let dentro = Rect { x: caja.x + 1, width: caja.width.saturating_sub(2), ..caja };
            if !foco {
                f.render_widget(Paragraph::new(Span::styled(self.valor(d, comunidad), acento)), dentro);
                continue;
            }
            let texto = match &self.escribiendo {
                Some((e, _)) => e.valor.clone(),
                None if self.se_elige(d) => format!("‹ {} ›", self.valor(d, comunidad)),
                None => self.valor(d, comunidad),
            };
            let largo = (texto.chars().count() as u16 + 2).max(12).min(caja.width);
            f.buffer_mut().set_style(Rect { width: largo, ..caja }, Style::new().bg(tema::suave(tema::acento())));
            match &self.escribiendo {
                Some((e, _)) => e.dibujar(f, Rect { width: largo.saturating_sub(1), ..dentro }, true, negrita()),
                None => f.render_widget(Paragraph::new(Span::styled(texto, negrita())), dentro),
            }
        }
    }

    /// Si la app no tiene el IRPF de tu comunidad (País Vasco y Navarra, forales; Ceuta y Melilla, con su deducción).
    fn aviso(&self, ctx: &Ctx) -> Option<Line<'static>> {
        let (c, _) = crate::informe::empleo::comunidad(&ctx.datos, ctx.anio)
            .filter(|(c, _)| crate::hacienda::irpf::reglas(self.anio, c).autonomica.is_none())?;
        Some(Line::styled(format!("{}: la app no tiene su IRPF", nombre_comunidad(c)), Style::new().fg(tema::AVISO)))
    }

    /// El reparto, en escalera: de lo que le cuestas a tu empresa (o de lo que facturas, en autónomo), restando paso a
    /// paso, hasta lo que te queda al año; luego, lo que te queda al mes (en las
    /// pagas elegidas), lo único en verde, y debajo cuánto se va en impuestos y cotizaciones. Sin `entero` (si no hay
    /// sitio), solo estas dos. Devuelve las líneas y cuál es la del mes (va sobre el fondo verde).
    fn reparto(&self, c: &Cuenta, fin: u16, entero: bool) -> (Vec<Line<'static>>, usize) {
        let n = Style::new();
        let fuerte = negrita();
        let (mut v, al_mes, se_va) = match c {
            Cuenta::Asalariado(s) => {
                let v = vec![
                    escalon("", "coste para tu empresa", s.coste, fuerte, fin),
                    escalon("−", "Seguridad Social pagada por la empresa", s.ss_empresa, n, fin),
                    escalon("=", "tu sueldo bruto", s.bruto, fuerte, fin),
                    escalon("−", "Seguridad Social pagada por ti", s.ss_trabajador, n, fin),
                    escalon("−", "IRPF pagado por ti", s.irpf.total, n, fin),
                    escalon("=", "te queda al año", s.neto, fuerte, fin),
                ];
                (v, s.neto / self.pagas as f64, lo_que_pagas(s.irpf.total, s.ss_empresa + s.ss_trabajador, s.coste))
            }
            Cuenta::Autonomo(a) => {
                let mut v = vec![
                    escalon("", "lo que facturas", a.facturado, n, fin),
                    escalon("−", "tus gastos", a.gastos, n, fin),
                    escalon("=", "lo que ganas", a.antes(), fuerte, fin),
                    escalon("−", "tu cuota de autónomo", a.ss, n, fin),
                    la_cuota(a),
                ];
                v.push(escalon("−", "IRPF", a.irpf.total, n, fin));
                v.push(escalon("=", "te queda al año", a.neto, fuerte, fin));
                (v, a.neto / 12.0, lo_que_pagas(a.irpf.total, a.ss, a.antes()))
            }
        };
        // Sin sitio, solo lo del mes y lo que pagas; en autónomo, también la cuota (lo que pagas al mes, la mínima y
        // tu base).
        if !entero {
            v = match c {
                Cuenta::Autonomo(a) => {
                    vec![escalon("", "tu cuota de autónomo", a.ss, n, fin), la_cuota(a), Line::from("")]
                }
                Cuenta::Asalariado(_) => Vec::new(),
            };
        }
        let mes = v.len();
        v.push(en_banda("te queda al mes", eur(al_mes), negrita().fg(tema::BIEN), fin));
        v.push(Line::from(""));
        v.push(se_va);
        (v, mes)
    }

    /// Lo que te queda en cada comunidad y cuánto más o menos que en la elegida, sin caja: la cabecera en gris; la
    /// tuya (la de tus datos), con un «●» del color de la sección delante; y la elegida resaltada (en el fondo suave
    /// de la sección si tiene el foco la comunidad, que se cambia con ← →).
    fn pintar_comunidades(
        &mut self,
        f: &mut Frame,
        area: Rect,
        v: &[EnComunidad],
        sel: usize,
        ctx: &Ctx,
    ) {
        let tuya = crate::informe::empleo::comunidad(&ctx.datos, ctx.anio).map(|c| c.0);
        let elegida = &v[sel];
        let al_mes = match self.pestana {
            Pestana::Asalariado => self.pagas as f64,
            Pestana::Autonomo => 12.0,
        };
        //f.render_widget(Paragraph::new(titulo_bloque("comunidad")), titulo);
        let filas: Vec<Row> = v
            .iter()
            .map(|c| {
                // La tuya, con su marca delante (las demás, con el mismo hueco, para que los nombres casen).
                let marca = if Some(c.clave) == tuya { Span::styled("● ", negrita().fg(tema::acento())) } else { Span::raw("  ") };
                let nombre = Line::from(vec![marca, Span::raw(c.nombre)]);
                let dif = if c.clave == elegida.clave {
                    Span::raw("")
                } else {
                    Span::styled(diferencia((c.neto - elegida.neto) / al_mes), gris())
                };
                Row::new([
                    Cell::from(nombre),
                    Cell::from(derecha(eur0(c.neto / al_mes))),
                    Cell::from(Line::from(dif).right_aligned()),
                ])
            })
            .collect();
        let cab = Row::new([
            Cell::from("  comunidad"),
            Cell::from(derecha("neto/mes")),
            Cell::from(derecha("diferencia al mes")),
        ])
        .style(tema::cabecera_tabla());
        let anchos = [
            Constraint::Length(32),
            Constraint::Length(9),
            Constraint::Length(18),
        ];
        let resalte = if self.dato() == Dato::Comunidad && !self.detalle {
            tema::seleccion()
        } else {
            negrita().fg(tema::acento())
        };
        let tabla = Table::new(filas, anchos)
            .header(cab)
            .column_spacing(2)
            .row_highlight_style(resalte);
        self.tabla.select(Some(sel));
        f.render_stateful_widget(tabla, area, &mut self.tabla);
    }

    /// El detalle, concepto a concepto, en la comunidad elegida, con puntos guía. Se baja con ↑↓.
    fn pintar_detalle(&mut self, f: &mut Frame, area: Rect, c: &Cuenta, nombre: &str) {
        let w = area.width.min(100) as usize;
        let lineas = match c {
            Cuenta::Asalariado(s) => lineas_asalariado(s, self.pagas, w),
            Cuenta::Autonomo(a) => lineas_autonomo(a, w),
        };
        let alto = area.height.saturating_sub(1);
        let max = (lineas.len() as u16).saturating_sub(alto);
        self.scroll = self.scroll.min(max);
        let titulo = Rect {
            height: area.height.min(1),
            ..area
        };
        f.render_widget(
            Paragraph::new(titulo_bloque(format!("el detalle · {nombre}"))),
            titulo,
        );
        if max > 0 {
            let mas = if self.scroll >= max {
                "↑ más arriba"
            } else {
                "↓ más abajo"
            };
            f.render_widget(
                Paragraph::new(Line::styled(mas, gris()).right_aligned()),
                titulo,
            );
        }
        let cuerpo = Rect {
            y: area.y + 1,
            height: alto,
            ..area
        };
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)), cuerpo);
    }
}

/// Lo de la cuota de autónomo, bajo su escalón y en gris, sin puntos ni en la columna de las cifras (no es algo que
/// se reste): lo que pagas al mes y tu base (con la tarifa plana, que es la tarifa plana). La mínima ya se ve al
/// elegir la cuota.
fn la_cuota(a: &Autonomo) -> Line<'static> {
    let que = if a.tarifa_plana { "tarifa plana".to_string() } else { format!("base {}", eur(a.base)) };
    Line::styled(format!("    {} al mes ({que})", eur(a.cuota_mes)), gris())
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
        Recibo {
            fin: w.min(76) as u16,
            w,
            lineas: Vec::new(),
        }
    }

    /// Un paso de la cuenta: «= Tu sueldo bruto ······ 30.000,00 €».
    fn paso(&mut self, et: &str, st: Style, x: f64, signo: &str) {
        self.lineas.push(guia(
            Line::from(Span::styled(et.to_string(), st)),
            format!("{signo}{}", importe(x)),
            st,
            self.fin,
        ));
    }

    /// Un concepto de lo cotizado, en gris y más adentro (sin negrita: es el detalle del paso).
    fn concepto(&mut self, et: &str, x: f64) {
        let mut l = guia(
            Span::styled(format!("      {et}"), gris()),
            importe(x),
            gris(),
            self.fin,
        );
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
    p.paso("Lo que le cuestas a tu empresa",
        negrita(),
        s.coste,
        "",
    );
    p.paso("  − Seguridad Social que paga tu empresa",
        n,
        s.ss_empresa,
        "−",
    );
    p.conceptos(&s.empresa);
    p.paso("= Tu sueldo bruto", negrita(), s.bruto, "");
    p.paso("  − Tu Seguridad Social", n, s.ss_trabajador, "−");
    p.conceptos(&s.trabajador);
    p.paso("  − IRPF", n, s.irpf.total, "−");
    let mut t = format!(
        "base {} · estatal {} · autonómica {}",
        eur(s.irpf.base),
        eur(s.irpf.estatal),
        eur(s.irpf.autonomica)
    );
    if s.irpf.deduccion > DIF {
        t += &format!(" · deducción por sueldo bajo {}", eur(s.irpf.deduccion));
    }
    p.nota(format!(
        "{t} · el {} de tu bruto",
        de(s.irpf.total, s.bruto)
    ));
    p.nota(format!(
        "con tu Seguridad Social, te quitan {}",
        eur(s.ss_trabajador + s.irpf.total)
    ));
    p.paso("= Te queda",
        negrita().fg(tema::BIEN),
        s.neto,
        "",
    );
    p.nota(format!("al mes: {} en {pagas} pagas", eur(s.neto / pagas as f64)));
    let g = reglas::general(s.anio_ss);
    let mut notas = vec![format!(
        "Cotizaciones con las reglas de {} ({}); IRPF con las de {}.",
        s.anio_ss, g.norma, s.anio_irpf
    )];
    if s.tope && g.solidaridad.is_empty() {
        notas.push(format!("Tu sueldo pasa de la base máxima ({} al mes): por encima no se cotiza.", eur(g.base_maxima)));
    } else if s.tope {
        notas.push(format!("Tu sueldo pasa de la base máxima ({} al mes): por encima solo se paga la cotización de solidaridad.", eur(g.base_maxima)));
    }
    notas.push(format!(
        "Accidentes de trabajo: el {} de un trabajo de oficina (lo paga la empresa).",
        tipo(reglas::AT_EP_OFICINA)
    ));
    notas.push("El IRPF es el que te toca en la Renta, sin hijos ni otras rentas; lo que te retienen cada mes se le parece.".into());
    notas.push(SIN_COMUNIDADES.into());
    p.vinetas(notas)
}

fn lineas_autonomo(a: &Autonomo, w: usize) -> Vec<Line<'static>> {
    let n = Style::new();
    let mut p = Recibo::new(w);
    p.paso("Lo que facturas (sin IVA)",
        negrita(),
        a.facturado,
        "",
    );
    if a.gastos > DIF {
        p.paso("  − Gastos de tu actividad", gris(), a.gastos, "−");
        p.paso("= Lo que ganas", negrita(), a.antes(), "");
    }
    p.paso("  − Tu cuota de autónomo", n, a.ss, "−");
    let tramo = if a.con_tramos {
        format!("tramo {}: ganas {} al mes (con el {} de gastos genéricos)", a.tramo, eur(a.mensual_ss), tipo(a.gastos_genericos))
    } else {
        "sin tramos (llegaron en 2023)".to_string()
    };
    let minima = format!("la mínima, {} × {} = {} al mes", eur(a.base_minima), tipo(a.tipo), eur(a.cuota_minima));
    if a.tarifa_plana {
        p.nota(format!("tarifa plana: {} al mes · sin ella, {tramo}: {minima}", eur(a.cuota_mes)));
    } else if a.base > a.base_minima + DIF {
        p.nota(format!("{tramo} · tu base, {} × {} = {} al mes ({minima}; la máxima, {})", eur(a.base), tipo(a.tipo), eur(a.cuota_mes), eur(a.base_maxima)));
    } else {
        p.nota(format!("{tramo} → {minima}"));
    }
    p.conceptos(&a.cuota);
    p.paso("  − IRPF", n, a.irpf.total, "−");
    let mut t = format!("rendimiento {}", importe(a.rendimiento));
    if a.gdj > DIF {
        t += &format!(" − difícil justificación {}", eur(a.gdj));
    }
    if a.reduccion > DIF {
        t += &format!(" − reducción por rentas bajas {}", eur(a.reduccion));
    }
    p.nota(format!(
        "{t} = base {} · el {} de lo que ganas",
        eur(a.irpf.base),
        de(a.irpf.total, a.antes())
    ));
    p.nota(format!("con la cuota, te quitan {}", eur(a.impuestos())));
    p.paso("= Te queda",
        negrita().fg(tema::BIEN),
        a.neto,
        "",
    );
    p.nota(format!("al mes: {}", eur(a.neto / 12.0)));
    let cuota = if a.tarifa_plana {
        format!("La tarifa plana, la de los doce primeros meses de alta: {}", reglas::autonomos(a.anio_ss).tarifa_plana_que)
    } else if a.con_tramos {
        format!("La base, entre la mínima y la máxima de tu tramo ({} y {} al mes)", eur(a.base_minima), eur(a.base_maxima))
    } else {
        format!("Antes de 2023, cada uno elegía su base entre la mínima y la máxima ({} y {} al mes)", eur(a.base_minima), eur(a.base_maxima))
    };
    let mut notas = vec![
        format!("Cotizaciones con las reglas de {} ({}); IRPF con las de {}.", a.anio_ss, a.norma, a.anio_irpf),
        format!("{cuota}; el IVA no cuenta (se lo pasas a Hacienda)."),
        "IRPF en estimación directa simplificada, sin otras rentas: lo del 130 y las retenciones se descuentan en la Renta.".into(),
        SIN_COMUNIDADES.into(),
    ];
    let del_anio = reglas::autonomos(a.anio_ss).nota;
    if !del_anio.is_empty() {
        notas.insert(1, del_anio.into());
    }
    p.vinetas(notas)
}

impl Calculadora {
    /// La de un sueldo bruto (la pestaña 1 de Utilidades).
    pub fn asalariado() -> Self {
        Self::default()
    }

    /// La de lo que factura un autónomo (la pestaña 2).
    pub fn autonomo() -> Self {
        Calculadora { pestana: Pestana::Autonomo, ..Self::default() }
    }
}

impl Pantalla for Calculadora {
    fn titulo(&self) -> &'static str {
        match self.pestana {
            Pestana::Asalariado => "Calcula tu sueldo",
            Pestana::Autonomo => "Calcula como autónomo",
        }
    }

    /// Todo en una vertical: los datos, uno debajo de otro (y, si la app no tiene el IRPF de tu comunidad, se dice);
    /// el reparto, hasta lo que te queda al mes (si no deja sitio a unas cuantas comunidades, solo lo del mes y lo que
    /// se va: lo demás está en el detalle), y las comunidades; o, con `d`, el detalle.
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let v = self.comunidades();
        let sel = self.elegida(&v, ctx);
        let cuenta = self.cuenta(v[sel].clave);
        let fila = |y: u16, alto: u16| Rect {
            y,
            height: alto.min(area.bottom().saturating_sub(y)),
            ..area
        };
        let alto_datos = self.datos_de().len() as u16;
        self.pintar_datos(f, Rect { width: ANCHO_DATOS.min(area.width), ..fila(area.y, alto_datos) }, v[sel].nombre);
        let mut y = area.y + alto_datos;
        if let Some(l) = self.aviso(ctx) {
            f.render_widget(Paragraph::new(l), fila(y, 1));
            y += 1;
        }
        y += 1;
        if !self.detalle {
            // Las cifras acaban en la misma columna; el fondo verde del mes, dos más allá.
            let fin = FIN.min(area.width);
            let ancho = (fin + 2).min(area.width);
            let (mut reparto, mut mes) = self.reparto(&cuenta, fin, true);
            if area.bottom().saturating_sub(y) < reparto.len() as u16 + 1 + 6 {
                (reparto, mes) = self.reparto(&cuenta, fin, false);
            }
            let alto = reparto.len() as u16;
            let r = fila(y, alto);
            f.render_widget(Paragraph::new(reparto), r);
            let fila_mes = r.y + mes as u16;
            if fila_mes < r.bottom() {
                f.buffer_mut().set_style(Rect { y: fila_mes, height: 1, width: ancho, ..r }, Style::new().bg(tema::SUAVE_BIEN));
            }
            y += alto + 1;
        }
        let resto = fila(y, area.bottom().saturating_sub(y));
        if self.detalle {
            self.pintar_detalle(f, resto, &cuenta, v[sel].nombre);
        } else {
            self.pintar_comunidades(f, resto, &v, sel, ctx);
        }
    }

    /// ↑↓ de un dato a otro; ← → cambian los de elegir; Enter, a escribir un importe (se recalcula a cada tecla;
    /// Enter o ↑↓ lo dejan y Esc vuelve a lo que había). En el detalle, ↑↓ lo bajan.
    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if let Some((e, _)) = &mut self.escribiendo {
            match k.code {
                KeyCode::Esc => self.terminar(true),
                KeyCode::Enter => self.terminar(false),
                KeyCode::Up | KeyCode::Down => {
                    self.terminar(false);
                    self.mover(k.code == KeyCode::Down);
                }
                _ => {
                    let x = if e.tecla(k) { leer_importe(&e.valor) } else { None };
                    if let Some(x) = x {
                        self.poner_importe(self.dato(), x);
                    }
                }
            }
            return true;
        }
        match k.code {
            KeyCode::Char('d') => {
                self.detalle = !self.detalle;
                self.scroll = 0;
            }
            KeyCode::Esc if self.detalle => self.detalle = false,
            KeyCode::Up | KeyCode::PageUp if self.detalle => {
                self.scroll = self
                    .scroll
                    .saturating_sub(if k.code == KeyCode::Up { 1 } else { 10 })
            }
            KeyCode::Down | KeyCode::PageDown if self.detalle => {
                self.scroll =
                    self.scroll
                        .saturating_add(if k.code == KeyCode::Down { 1 } else { 10 })
            }
            _ if self.detalle => return false,
            KeyCode::Up | KeyCode::Down => self.mover(k.code == KeyCode::Down),
            KeyCode::Home => self.foco = 0,
            KeyCode::End => self.foco = self.datos_de().len() - 1,
            KeyCode::Left | KeyCode::Right if self.se_elige(self.dato()) => {
                self.cambiar(self.dato(), k.code == KeyCode::Right, ctx)
            }
            KeyCode::Enter if self.dato().importe() => {
                self.fijar_comunidad(ctx);
                let x = self.importe_de(self.dato());
                self.escribiendo = Some((Entrada::new(para_escribir(x)), x));
            }
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.escribiendo.is_some() {
            return vec![("Enter", "calcular"), ("Esc", "cancelar")];
        }
        if self.detalle {
            return vec![("↑↓", "bajar"), ("Esc", "volver")];
        }
        let d = self.dato();
        let mut v = vec![("↑↓", "dato")];
        if d.importe() {
            v.push(("Enter", "escribir"));
        } else if self.se_elige(d) {
            v.push(("← →", "cambiar"));
        }
        v.push(("d", "detalle"));
        v
    }

    fn capturando(&self) -> bool {
        self.escribiendo.is_some()
    }

    /// Tiene su propio año (en «Tu sueldo»): el de arriba no cuenta.
    fn usa_anio(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::dominio::fmt::eur;
    use crate::informe::calculadora::{Cotiza, anio_por_defecto, anios_con_reglas, asalariado, asalariado_por_comunidad, autonomo};
    use crate::tui::prueba::{app_con, cargar, caso, ir, pantalla, teclas};

    /// Cómo se ve un dato: «año         2025»; con el foco, «› año         ‹ 2025 ›» (los de elegir).
    fn dato(nombre: &str, valor: &str) -> String {
        format!("{nombre:<w$}   {valor}", w = super::ANCHO_NOMBRE as usize)
    }

    fn con_foco(nombre: &str, valor: &str) -> String {
        format!("› {}", dato(nombre, &format!("‹ {valor} ›")))
    }

    /// Los datos, uno debajo de otro; la comunidad, la última, se cambia con ← → (lo que te queda es lo suyo); si la
    /// tuya no la tiene la app, se dice. El reparto, sin notas ni %. Lo de al mes,
    /// solo en las pagas elegidas (si se quiere en 12, se eligen 12).
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
        let anio = anio_por_defecto();
        let v = asalariado_por_comunidad(30000.0, false, anio);
        let p = pantalla(&mut a, 120, 36);
        assert!(
            p.contains("Navarra (régimen foral): la app no tiene su IRPF"),
            "{p}"
        );
        assert!(
            p.contains(&dato("año", &anio.to_string())) && p.contains(&dato("comunidad", v[0].nombre)),
            "uno debajo de otro y, sin la tuya, la primera:\n{p}"
        );
        let s = asalariado(30000.0, false, v[0].clave, anio);
        assert!(p.contains(&eur(s.coste)) && !p.contains("100 %"), "sin % en la escalera:\n{p}");
        assert!(!p.contains("lo que le cuestas") && !p.contains("elegir otra") && !p.contains("de ellos"), "sin notas:\n{p}");
        // Al final, lo que te queda al año, al mes y lo que se va en impuestos y cotizaciones.
        let al_año = p.find("te queda al año").expect("al año");
        let al_mes = p.find("te queda al mes").expect("al mes");
        assert!(p[al_mes..].lines().next().is_some_and(|l| l.contains(&eur(s.neto / 14.0))), "{p}");
        let pct = |x: f64| (x / s.coste * 100.0).round();
        let (total, impuestos) = (pct(s.impuestos()), pct(s.irpf.total));
        let pagas = format!("pagas el {total} % en impuestos ({impuestos} %) y cotizaciones ({} %)", total - impuestos);
        assert!(al_año < al_mes && p.contains(&pagas), "{pagas}\n{p}");
        teclas(&mut a, "<end><right>");
        let p = pantalla(&mut a, 120, 36);
        assert!(
            p.contains(&con_foco("comunidad", v[1].nombre)),
            "{p}"
        );
        assert!(
            p.contains(&eur(v[1].neto / 14.0)),
            "lo de la elegida, al mes:\n{p}"
        );
        assert!(!p.contains("en 12 pagas") && !p.contains("aún no están"), "{p}");
        // El detalle, de la elegida; Esc vuelve a las comunidades.
        teclas(&mut a, "d");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&format!("el detalle · {}", v[1].nombre)), "{p}");
        teclas(&mut a, "<pgdn>");
        let p = pantalla(&mut a, 120, 36);
        assert!(
            p.contains(&format!("al mes: {} en 14 pagas", eur(v[1].neto / 14.0))) && !p.contains("en 12 pagas"),
            "más abajo, al mes, solo en las pagas elegidas:\n{p}"
        );
        teclas(&mut a, "<esc>");
        assert!(pantalla(&mut a, 120, 36).contains("diferencia al mes"));
    }

    /// Tu comunidad (la de tu última Renta), marcada en la lista con un «●» delante, sin texto.
    #[test]
    fn tu_comunidad_marcada() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        d.lecturas.rentas.entry(2025).or_default().comunidad = "madrid".into();
        let mut a = app_con(d, &dir);
        ir(&mut a, "Calcula tu sueldo");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("● Comunidad de Madrid") && !p.contains("tuya") && p.matches('●').count() == 1, "{p}");
        assert!(p.contains("  La Rioja "), "las demás, con el mismo hueco:\n{p}");
    }

    /// Los importes se escriben en su sitio (Enter) y se recalcula a cada tecla; Enter lo deja, Esc vuelve a lo que
    /// había y lo que no es un número no cuenta. ↑↓ van de un dato a otro y ← → cambian los de elegir.
    #[test]
    fn se_escribe_en_su_sitio() {
        let dir = caso("a-de-todo");
        let mut a = app_con(cargar(&dir), &dir);
        ir(&mut a, "Calcula tu sueldo");
        let anio = anio_por_defecto();
        // Escribiendo, la comunidad no cambia aunque la lista se reordene (con importes en los que todas empatan).
        let comunidad = |p: &str| p.lines().find_map(|l| Some(l.split_once(&dato("comunidad", ""))?.1.split("  ").next()?.to_string()));
        let antes = comunidad(&pantalla(&mut a, 120, 36));
        teclas(&mut a, "<enter><c-u>1");
        let p = pantalla(&mut a, 120, 36);
        teclas(&mut a, "<esc>");
        assert!(antes.as_ref().is_some_and(|c| !c.is_empty()) && comunidad(&p) == antes, "{antes:?}:\n{p}");
        // La comunidad, la primera de la lista (sin la tuya de por medio).
        teclas(&mut a, &format!("<end>{}<home>", "<left>".repeat(25)));
        let primera = asalariado_por_comunidad(30000.0, false, anio)[0].clave;
        let al_mes = |bruto: f64, pagas: f64| eur(asalariado(bruto, false, primera, anio).neto / pagas);
        teclas(&mut a, "<enter><c-u>45000");
        assert!(a.pantalla().capturando());
        assert!(pantalla(&mut a, 120, 36).contains(&al_mes(45000.0, 14.0)), "a cada tecla");
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(!a.pantalla().capturando() && p.contains("45.000 € al año") && p.contains(&al_mes(45000.0, 14.0)), "{p}");
        teclas(&mut a, "<enter><c-u>20000");
        assert!(pantalla(&mut a, 120, 36).contains(&al_mes(20000.0, 14.0)));
        teclas(&mut a, "<esc>");
        assert!(pantalla(&mut a, 120, 36).contains("45.000 € al año"), "Esc: lo que había");
        teclas(&mut a, "<enter><c-u>mucho<down><right>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("45.000 € al año") && p.contains(&con_foco("pagas", "12")), "{p}");
        assert!(p.contains(&al_mes(45000.0, 12.0)) && p.contains("te queda al mes"), "{p}");
    }

    /// En autónomo, la cuota: la mínima de tu tramo o, con ← →, una base más alta de las de referencia (la mínima de
    /// los tramos de encima, hasta la máxima del tuyo); debajo de su escalón, en una nota (no son cifras que se
    /// resten), lo que pagas al mes y tu base. Con la tarifa plana, la suya.
    #[test]
    fn la_cuota_de_autonomo() {
        let dir = caso("a-de-todo");
        let mut a = app_con(cargar(&dir), &dir);
        ir(&mut a, "Calcula como autónomo");
        let anio = anio_por_defecto();
        let min = autonomo(40000.0, 5000.0, Cotiza::default(), "", anio);
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&dato("cuota", &format!("{} al mes, la mínima", eur(min.cuota_minima)))), "{p}");
        let nota = format!("    {} al mes (base {})", eur(min.cuota_minima), eur(min.base));
        assert!(p.contains(&nota), "una nota, sin puntos:\n{p}");
        // Enter no escribe: se elige con ← →.
        teclas(&mut a, "<down><down><enter>");
        assert!(!a.pantalla().capturando());
        teclas(&mut a, "<right>");
        let bases = crate::tgss::reglas::autonomos(anio).bases(min.tramo);
        let otra = autonomo(40000.0, 5000.0, Cotiza { base: Some(bases[1]), tarifa_plana: false }, "", anio);
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&con_foco("cuota", &format!("{} al mes", eur(otra.cuota_mes)))), "{p}");
        assert!(p.contains(&format!("    {} al mes (base {})", eur(otra.cuota_mes), eur(bases[1]))), "{p}");
        teclas(&mut a, &"<right>".repeat(20));
        assert!(pantalla(&mut a, 120, 36).contains(", la máxima ›"), "hasta la máxima del tramo");
        teclas(&mut a, &"<left>".repeat(20));
        assert!(pantalla(&mut a, 120, 36).contains(", la mínima ›"));
        teclas(&mut a, "<down><right>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&con_foco("tarifa plana", "sí")) && p.contains(" al mes (tarifa plana)"), "{p}");
    }

    /// Solo se eligen años con todas sus reglas (cotizaciones e IRPF de ese año); se abre con el último hasta hoy.
    #[test]
    fn solo_anios_con_reglas() {
        let v = anios_con_reglas();
        assert!(!v.is_empty() && v.contains(&anio_por_defecto()));
        for y in v {
            let s = asalariado(30000.0, false, "", y);
            assert_eq!((s.anio_ss, s.anio_irpf), (y, y));
        }
    }
}
