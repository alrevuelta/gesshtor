//! La pestaña «Año» de Impuestos: lo facturado y lo gastado, los avisos (plazos, facturas sin cobrar), los
//! trimestres y los meses en un gráfico. Solo se mira.
//!
//! Un año cerrado sin facturas ni gastos en la app se ve con su Renta (ingresos, gastos, cuota, rendimiento) y lo
//! presentado en cada trimestre (los 303 y 130 de Hacienda).
use crate::dominio::fmt::DIF;
use super::impuestos_trimestre::{self as trimestre, Estado};
use crate::dominio::{factura, gasto, Casillas, Datos, Factura};
use crate::hacienda::modelo::Memo;
use crate::hacienda::{irpf, modelo_130, Modelo, Trimestral};
use crate::dominio::fmt::{self, r2};
use crate::hacienda::{self, Plazo};
use crate::informe::autonomo::{self, ActividadAnio, Fuente};
use crate::tui::texto::compacto;
use crate::tui::widgets::{bloque, derecha, kpi, tema};
use crate::tui::Ctx;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, BorderType, Borders, Cell, Paragraph, Row, Table, Wrap};
use crate::dominio::Valor;

#[derive(Default)]
pub(super) struct Anio {
    /// Lo calculado del año que se ve (la cadena de 303 y 130 es cara: no se rehace en cada dibujo).
    calculo: Option<Calculo>,
    /// Cuántas veces se ha calculado (para las pruebas).
    #[cfg(test)]
    pub(crate) calculos: usize,
}

/// Lo calculado para (generación de los datos, año, hoy).
struct Calculo {
    clave: (u64, i32, String),
    vista: Vista,
    avisos: Vec<Aviso>,
}

enum Vista {
    /// Con tus facturas y gastos (el año en curso, o uno sin Renta).
    App(Box<Cifras>),
    /// Un año cerrado sin nada en la app: su Renta y lo presentado.
    Renta(Box<DeRenta>),
}

impl Calculo {
    fn nuevo(d: &Datos, anio: i32, hoy: &str, clave: (u64, i32, String)) -> Self {
        let mut memo = Memo::default();
        let a = autonomo::actividad_anio(d, anio, hoy);
        let vista = if a.fuente == Fuente::Renta { Vista::Renta(Box::new(de_renta(d, a, hoy))) } else { Vista::App(Box::new(cifras(d, anio, hoy, &mut memo))) };
        Calculo { clave, vista, avisos: avisos(d, hoy, &mut memo) }
    }
    fn trims(&self) -> &[Trim] {
        match &self.vista {
            Vista::App(c) => &c.trims,
            Vista::Renta(r) => &r.trims,
        }
    }
}

/* ================= cálculos ================= */

/// Ingresos (base de las facturas) y gastos deducibles de cada mes.
fn mensual(d: &Datos, anio: i32) -> ([f64; 12], [f64; 12]) {
    let mut ing = [0.0; 12];
    let mut gas = [0.0; 12];
    for f in factura::emitidas(d) {
        let fd = f.devengo();
        let m = fmt::mes_de(fd) as usize;
        if fmt::anio_de(fd) == anio && (1..=12).contains(&m) {
            ing[m - 1] = r2(ing[m - 1] + factura::totales_factura(f).base);
        }
    }
    for g in &d.gastos {
        let gc = gasto::gasto_calc(g, d);
        if gc.amortizable {
            // Los equipos se reparten mes a mes según su amortización.
            for m in 1..=12u32 {
                let v = gasto::amort_en_rango(g, &gc, &format!("{anio}-{m:02}-01"), &fmt::fin_de_mes(anio, m));
                gas[m as usize - 1] = r2(gas[m as usize - 1] + v);
            }
        } else if fmt::anio_de(&g.fecha) == anio {
            let m = fmt::mes_de(&g.fecha) as usize;
            if (1..=12).contains(&m) {
                gas[m - 1] = r2(gas[m - 1] + gc.coste);
            }
        }
    }
    (ing, gas)
}

/// Una fila de la tabla de trimestres. `None`: no se sabe (en un año cerrado, lo que no se presentó o se
/// presentó sin que tengamos sus casillas).
struct Trim {
    plazo: Plazo,
    ingresos: Option<f64>,
    gastos: Option<f64>,
    r303: Option<f64>,
    devolver: bool,
    r130: Option<f64>,
    exento: bool,
    e303: Estado,
    e130: Estado,
}

/// Todo lo que enseña el resumen de un año.
struct Cifras {
    facturado: f64,
    n_facturas: usize,
    iva: f64,
    retenciones: f64,
    /// Gastos como en el 130: con el 5 % de difícil justificación solo si también se resta ahí.
    gastos: f64,
    gdj: f64,
    con5: bool,
    rn: f64,
    /// Lo que falta por pagar de los trimestres sin presentar.
    reserva: f64,
    /// Lo que salió a ingresar en lo ya presentado del año.
    pagado: f64,
    ing: [f64; 12],
    gas: [f64; 12],
    trims: Vec<Trim>,
}

fn cifras(d: &Datos, anio: i32, hoy: &str, memo: &mut Memo) -> Cifras {
    let este = anio == fmt::anio_de(hoy);
    let corte = if este { hoy.to_string() } else { format!("{anio}-12-31") };
    let (mut facturado, mut iva, mut retenciones, mut n_facturas) = (0.0, 0.0, 0.0, 0);
    for f in factura::emitidas(d) {
        let fd = f.devengo();
        if fmt::anio_de(fd) == anio && fd <= corte.as_str() {
            let t = factura::totales_factura(f);
            facturado = r2(facturado + t.base);
            iva = r2(iva + t.cuota);
            retenciones = r2(retenciones + t.retencion);
            n_facturas += 1;
        }
    }
    let a = irpf::irpf_acumulado(d, anio, &corte);
    let con5 = modelo_130::con_gdj(d);
    let gastos = if con5 { a.gastos } else { r2(a.gastos - a.gdj) };
    let rn = r2(a.ingresos - gastos);

    // Lo pendiente: los trimestres empezados sin presentar (y este año, el 4T del pasado si sigue sin presentar).
    let mut pend = trimestre::pendientes(d, hoy, anio, memo);
    if este {
        pend.extend(trimestre::pendientes(d, hoy, anio - 1, memo).into_iter().filter(|p| p.plazo.q == 4));
    }
    let reserva = r2(pend.iter().map(|p| p.a_pagar()).sum());

    let (ing, gas) = mensual(d, anio);
    let mut pagado = 0.0;
    let mut trims = Vec::new();
    for plazo in hacienda::plazos_trimestres(anio) {
        let q = plazo.q;
        let (m303, m130) = (Trimestral::M303, Trimestral::M130);
        let c303 = m303.cuenta(d, anio, q, memo);
        let c130 = m130.cuenta(d, anio, q, memo);
        let e303 = trimestre::estado_modelo(d, hoy, anio, q, m303);
        let e130 = trimestre::estado_modelo(d, hoy, anio, q, m130);
        let (r303, r130) = (m303.resultado_de(&c303), m130.resultado_de(&c130));
        if matches!(e303, Estado::Presentado(_)) {
            pagado += r303.max(0.0);
        }
        if matches!(e130, Estado::Presentado(_)) {
            pagado += r130.max(0.0);
        }
        let meses = (q as usize - 1) * 3..q as usize * 3;
        trims.push(Trim {
            ingresos: Some(r2(ing[meses.clone()].iter().sum())),
            gastos: Some(r2(gas[meses].iter().sum())),
            r303: Some(r303),
            devolver: trimestre::devolver303(d, anio, q),
            r130: Some(r130),
            exento: e130 == Estado::NoToca("exento"),
            e303,
            e130,
            plazo,
        });
    }
    Cifras { facturado, n_facturas, iva, retenciones, gastos, gdj: a.gdj, con5, rn, reserva, pagado: r2(pagado), ing, gas, trims }
}

/// Un año cerrado visto con su Renta y con lo presentado en cada trimestre.
struct DeRenta {
    a: ActividadAnio,
    trims: Vec<Trim>,
    /// Lo que salió a ingresar en los 303 y en los 130 presentados (de los que se tienen las casillas).
    iva: f64,
    irpf: f64,
    /// No consta ningún 130 del año.
    sin130: bool,
    /// Trimestres presentados de los que no se tienen las casillas.
    sin_casillas: usize,
    /// Lo cargado ese año en recibos de autónomo, si los hay.
    recibos: Option<f64>,
}

/// Lo facturado en un 303 presentado: las bases de las ventas con IVA, a otros países y con inversión del sujeto pasivo.
fn ventas303(c: &Casillas) -> f64 {
    r2([1, 4, 7, 150, 153, 165, 59, 60, 120, 122].iter().map(|k| c.v(*k)).sum())
}

fn de_renta(d: &Datos, a: ActividadAnio, hoy: &str) -> DeRenta {
    let anio = a.anio;
    let pres = |q: u32, m: Modelo| trimestre::presentadas(d, anio, q, m).map(|x| x.0);
    let p130: Vec<Option<Casillas>> = (1..=4).map(|q| pres(q, Modelo::M130)).collect();
    let sin130 = (1..=4).all(|q| trimestre::presentado(d, anio, q, Modelo::M130).is_none());
    let (mut iva, mut irpf, mut sin_casillas) = (0.0, 0.0, 0);
    let mut trims = Vec::new();
    for plazo in hacienda::plazos_trimestres(anio) {
        let q = plazo.q;
        let i = q as usize - 1;
        let p303 = pres(q, Modelo::M303);
        // El 130 es acumulado desde el 1 de enero: lo del trimestre es la diferencia con el anterior.
        let delta = |k: u32| -> Option<f64> {
            let ahora = p130[i].as_ref()?.v(k);
            let antes = if i == 0 { 0.0 } else { p130[i - 1].as_ref()?.v(k) };
            Some(r2(ahora - antes))
        };
        let ingresos = delta(1).or_else(|| p303.as_ref().map(ventas303));
        let e303 = trimestre::estado_modelo(d, hoy, anio, q, Trimestral::M303);
        let mut e130 = trimestre::estado_modelo(d, hoy, anio, q, Trimestral::M130);
        if sin130 && !e130.hecho() {
            e130 = Estado::NoToca("sin 130");
        }
        let r303 = p303.as_ref().map(|c| Trimestral::M303.resultado_de(c));
        let r130 = p130[i].as_ref().map(|c| Trimestral::M130.resultado_de(c));
        iva += r303.unwrap_or(0.0).max(0.0);
        irpf += r130.unwrap_or(0.0).max(0.0);
        sin_casillas += usize::from(matches!(e303, Estado::Presentado(_)) && r303.is_none()) + usize::from(matches!(e130, Estado::Presentado(_)) && r130.is_none());
        trims.push(Trim {
            ingresos,
            gastos: delta(2),
            r303,
            devolver: trimestre::devolver303(d, anio, q),
            r130,
            exento: e130 == Estado::NoToca("exento"),
            e303,
            e130,
            plazo,
        });
    }
    let c = autonomo::cuotas_anio(d, anio, hoy);
    DeRenta { a, trims, iva: r2(iva), irpf: r2(irpf), sin130, sin_casillas, recibos: (c.n_recibos > 0).then_some(c.total) }
}

/* ================= avisos ================= */

#[derive(Debug, Clone, Copy, PartialEq)]
enum Urgencia {
    Alta,
    Media,
    Normal,
}

#[derive(Debug, Clone)]
struct Aviso {
    urgencia: Urgencia,
    texto: String,
}

/// Lo que hay que hacer ahora: trimestres en plazo o vencidos, la Renta del año pasado y
/// las facturas vencidas sin cobrar. Va por la fecha de hoy, no por el año elegido. El 4T del año
/// pasado sale todo el año si sigue sin presentar (no solo en enero).
fn avisos(d: &Datos, hoy: &str, memo: &mut Memo) -> Vec<Aviso> {
    let y = fmt::anio_de(hoy);
    let mut v = Vec::new();
    let mut aviso = |urgencia, texto: String| v.push(Aviso { urgencia, texto });

    let mut pend: Vec<_> = trimestre::pendientes(d, hoy, y - 1, memo).into_iter().filter(|p| p.plazo.q == 4).collect();
    pend.extend(trimestre::pendientes(d, hoy, y, memo));
    for p in pend.iter().filter(|p| p.abierto || p.vencido) {
        let (q, a, fin) = (p.plazo.q, p.plazo.anio, fmt::dmy(&p.plazo.fin));
        if p.vencido {
            aviso(Urgencia::Alta, format!("El {q}T de {a} ({}) se pasó de plazo el {fin}.", p.modelos()));
        } else {
            let n = fmt::dias_entre(hoy, &p.plazo.fin);
            let urg = if n <= 7 { Urgencia::Media } else { Urgencia::Normal };
            aviso(urg, format!("Toca presentar el {q}T de {a} ({}): hasta el {fin}, {}.", p.modelos(), trimestre::dias_txt(n)));
        }
    }

    match trimestre::estado_renta(d, hoy, y - 1) {
        (Estado::EnPlazo(n), _, fin) => {
            let urg = if n <= 14 { Urgencia::Media } else { Urgencia::Normal };
            aviso(urg, format!("Toca la Renta de {}: hasta el {}, {}.", y - 1, fmt::dmy(&fin), trimestre::dias_txt(n)));
        }
        (Estado::Vencido, _, fin) => aviso(Urgencia::Alta, format!("La Renta de {} se pasó de plazo el {}.", y - 1, fmt::dmy(&fin))),
        _ => {}
    }

    let mut vencidas: Vec<&Factura> = factura::emitidas(d).filter(|f| !f.cobrada() && !f.vencimiento.is_empty() && f.vencimiento.as_str() < hoy).collect();
    vencidas.sort_by(|a, b| a.vencimiento.cmp(&b.vencimiento));
    for f in vencidas.iter().take(3) {
        let cliente = if f.cliente.nombre.is_empty() { "sin cliente" } else { f.cliente.nombre.as_str() };
        let total = factura::totales_factura(f).total;
        aviso(Urgencia::Media, format!("La factura {} ({cliente}) venció el {} sin cobrar: {}.", f.numero, fmt::dmy(&f.vencimiento), fmt::eur(total)));
    }
    if vencidas.len() > 3 {
        let resto = &vencidas[3..];
        let total: f64 = resto.iter().map(|f| factura::totales_factura(f).total).sum();
        aviso(Urgencia::Media, format!("Y {} facturas más vencidas sin cobrar: {}.", resto.len(), fmt::eur(total)));
    }
    v
}

/// El próximo plazo, para cuando todo está al día: «el 4T de 2026 (303 y 130): 1 ene – 1 feb 2027».
fn proximo(hoy: &str) -> Option<String> {
    let y = fmt::anio_de(hoy);
    let mut v: Vec<(String, String)> = Vec::new();
    for a in [y, y + 1] {
        for p in hacienda::plazos_trimestres(a).into_iter().filter(|p| p.ini.as_str() > hoy) {
            v.push((p.ini.clone(), format!("el {}T de {a} (303 y 130): {}", p.q, trimestre::rango(&p.ini, &p.fin))));
        }
    }
    for a in [y - 1, y] {
        let (ini, fin) = hacienda::plazo_renta(a);
        if ini.as_str() > hoy {
            v.push((ini.clone(), format!("la Renta de {a}: {}", trimestre::rango(&ini, &fin))));
        }
    }
    v.sort();
    v.into_iter().next().map(|x| x.1)
}

/* ================= dibujo ================= */

/// Una cifra de arriba: (prioridad, etiqueta, etiqueta corta, valor, nota, estilo). Con poco ancho se quedan las
/// de prioridad más baja.
type Kpi = (usize, &'static str, &'static str, String, String, Style);

fn kpis_app(c: &Cifras, e: &dyn Fn(f64) -> String) -> Vec<Kpi> {
    let nota_gastos = if c.gdj > DIF {
        // El 5 % de difícil justificación: aquí ya restado, o solo en la Renta.
        if c.con5 { format!("con 5 %: {}", fmt::eur0(c.gdj)) } else { format!("+{} en Renta", fmt::eur0(c.gdj)) }
    } else {
        "sin IVA".into()
    };
    let pendiente = if c.pagado > DIF {
        format!("pagado {}", fmt::eur0(c.pagado))
    } else if c.reserva > DIF {
        "sin presentar".into()
    } else {
        "nada pendiente".into()
    };
    let normal = Style::new();
    vec![
        (0, "Facturado sin IVA", "Facturado", e(c.facturado), format!("{} factura{}", c.n_facturas, if c.n_facturas == 1 { "" } else { "s" }), normal),
        (4, "IVA repercutido", "IVA cobrado", e(c.iva), "en tus facturas".to_string(), normal),
        (1, "Gastos deducibles", "Gastos", e(c.gastos), nota_gastos, normal),
        (2, "Rendimiento neto", "Rendimiento", e(c.rn), "ingresos − gastos".into(), Style::new().fg(tema::ACENTO)),
        (5, "Retenciones", "Retenciones", e(c.retenciones), "a cuenta del IRPF".into(), normal),
        (3, "Pendiente Hacienda", "Pendiente", e(c.reserva), pendiente, if c.reserva > DIF { Style::new().fg(tema::AVISO) } else { Style::new().fg(tema::BIEN) }),
    ]
}

/// Las de un año cerrado: lo que dice su Renta y lo que pagaste con los 303 y 130.
fn kpis_renta(r: &DeRenta, e: &dyn Fn(f64) -> String) -> Vec<Kpi> {
    let a = &r.a;
    let normal = Style::new();
    let opt = |x: Option<f64>| x.map(e).unwrap_or_else(|| "—".into());
    let sin = if a.gdj > DIF { format!("sin cuota ni {}", fmt::pct(irpf::gdj_reglas(a.anio).0)) } else { "sin la cuota".into() };
    let pagado = r2(r.iva + r.irpf);
    vec![
        (0, "Ingresos", "Ingresos", e(a.ingresos), format!("Renta {}", a.anio), normal),
        (2, "Gastos deducibles", "Gastos", e(a.gastos), sin, normal),
        (3, "Cuota de autónomo", "Cuota", e(a.cuota), "casilla 0186".into(), normal),
        (1, "Rendimiento neto", "Rendimiento", e(a.neto), "casilla 0224".into(), Style::new().fg(tema::ACENTO)),
        (5, "Retenciones", "Retenciones", opt(a.retenciones), "de tus clientes".into(), normal),
        (4, "Pagado a Hacienda", "Pagado", e(pagado), "en 303 y 130".into(), normal),
    ]
}

fn pintar_kpis(f: &mut Frame, area: Rect, vista: &Vista, grande: bool) {
    let por_columna = if grande { 18 } else { 14 };
    let n = (area.width / por_columna).clamp(1, 6) as usize;
    let ancho = area.width as usize / n;
    let e = |x: f64| if ancho >= 16 { fmt::eur(x) } else { fmt::eur0(x) };
    let todos = match vista {
        Vista::App(c) => kpis_app(c, &e),
        Vista::Renta(r) => kpis_renta(r, &e),
    };
    let elegidos: Vec<_> = todos.iter().filter(|k| k.0 < n).collect();
    let zonas = Layout::horizontal(vec![Constraint::Ratio(1, elegidos.len() as u32); elegidos.len()]).split(area);
    for (k, zona) in elegidos.iter().zip(zonas.iter()) {
        let etiqueta = if zona.width >= 20 { k.1 } else { k.2 };
        if grande {
            kpi(f, *zona, etiqueta, &k.3, &k.4, k.5);
        } else {
            let t = vec![Line::styled(format!(" {etiqueta}"), tema::apagado()), Line::styled(format!(" {}", k.3), k.5.add_modifier(Modifier::BOLD))];
            f.render_widget(Paragraph::new(t), *zona);
        }
    }
}

/// «Todo al día» y el próximo plazo, para cuando no hay avisos.
fn al_dia(hoy: &str) -> String {
    match proximo(hoy) {
        Some(p) => format!("✓ Todo al día. Próximo: {p}."),
        None => "✓ Todo al día.".into(),
    }
}

/// Líneas que ocupa cada aviso con ese ancho (se parten al no caber).
fn alto_aviso(texto: &str, ancho: u16) -> u16 {
    ((texto.chars().count() + 2).div_ceil(ancho.max(1) as usize)).max(1) as u16
}

fn pintar_avisos(f: &mut Frame, area: Rect, av: &[Aviso], hoy: &str) {
    let mut t = Vec::new();
    if av.is_empty() {
        let txt = al_dia(hoy);
        let (marca, resto) = txt.split_at(txt.find('.').map(|i| i + 1).unwrap_or(txt.len()));
        t.push(Line::from(vec![Span::styled(marca.to_string(), Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD)), Span::styled(resto.to_string(), tema::apagado())]));
    } else {
        // Los que quepan; si no caben todos, el último hueco dice cuántos faltan.
        let ancho = area.width.saturating_sub(2);
        let mut caben = area.height.saturating_sub(2).max(1);
        let mut muestra = 0;
        for (i, a) in av.iter().enumerate() {
            let h = alto_aviso(&a.texto, ancho);
            let falta_uno = i + 1 < av.len();
            if h > caben || (falta_uno && h + 1 > caben) {
                break;
            }
            caben -= h;
            muestra += 1;
        }
        for a in &av[..muestra] {
            let st = match a.urgencia {
                Urgencia::Alta => Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD),
                Urgencia::Media => Style::new().fg(tema::AVISO),
                Urgencia::Normal => Style::new().fg(tema::ACENTO),
            };
            t.push(Line::from(vec![Span::styled("● ", st), Span::raw(a.texto.clone())]));
        }
        if muestra < av.len() {
            t.push(Line::styled(format!("  … y {} aviso{} más", av.len() - muestra, if av.len() - muestra == 1 { "" } else { "s" }), tema::apagado()));
        }
    }
    f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }).block(bloque("Avisos")), area);
}

fn pintar_trimestres(f: &mut Frame, area: Rect, trims: &[Trim], titulo: &str) {
    let corto = area.width < 110;
    let e = |x: f64| if x.abs() <= DIF { "—".to_string() } else if corto { fmt::eur0(x) } else { fmt::eur(x) };
    let eo = |x: Option<f64>| x.map(e).unwrap_or_else(|| "—".into());
    let gris = |x: Option<f64>| if x.is_none_or(|x| x.abs() <= DIF) { tema::apagado() } else { Style::new() };
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    // Presentado, pero sin sus casillas (no se ha leído el PDF): no se sabe el importe.
    let sin_pdf = |e: &Estado, r: Option<f64>| matches!(e, Estado::Presentado(_)) && r.is_none();
    // Primero los textos: los importes nunca se cortan (el ancho de su columna sale de lo que hay en ella).
    let mut filas: Vec<[(String, Style); 6]> = trims
        .iter()
        .map(|t| {
            // Lo que no ha empezado o no toca (antes del alta, después de la baja) no lleva importe.
            let sin = matches!(t.e303, Estado::Futuro | Estado::NoToca(_));
            let iva = match t.r303 {
                _ if sin_pdf(&t.e303, t.r303) => ("sin PDF".to_string(), tema::apagado()),
                Some(r) if !sin && r.abs() > DIF => (trimestre::texto303(r, t.devolver, corto), trimestre::estilo_resultado(r)),
                _ => ("—".to_string(), tema::apagado()),
            };
            let irpf = match t.r130 {
                _ if t.exento => ("exento".to_string(), tema::apagado()),
                _ if sin_pdf(&t.e130, t.r130) => ("sin PDF".to_string(), tema::apagado()),
                Some(r) if !sin && r.abs() > DIF => (trimestre::texto130(r, corto), trimestre::estilo_resultado(r)),
                _ => ("—".to_string(), tema::apagado()),
            };
            [
                (format!("{}T", t.plazo.q), negrita),
                (eo(t.ingresos), gris(t.ingresos)),
                (eo(t.gastos), gris(t.gastos)),
                iva,
                irpf,
                trimestre::estado_conjunto(&t.e303, &t.e130, &t.plazo, corto),
            ]
        })
        .collect();
    // En el año: la suma, si se saben todos los trimestres.
    let suma = |g: fn(&Trim) -> Option<f64>| trims.iter().map(g).sum::<Option<f64>>().map(r2);
    let iva = r2(trims.iter().map(|t| t.r303.unwrap_or(0.0).max(0.0)).sum());
    let irpf = r2(trims.iter().map(|t| if t.exento { 0.0 } else { t.r130.unwrap_or(0.0).max(0.0) }).sum());
    filas.push([
        ("Año".into(), negrita),
        (eo(suma(|t| t.ingresos)), negrita),
        (eo(suma(|t| t.gastos)), negrita),
        (e(iva), negrita),
        (e(irpf), negrita),
        ((if iva > DIF || irpf > DIF { "a ingresar en el año" } else { "" }).into(), tema::apagado()),
    ]);
    let cabeceras = ["", "Ingresos", "Gastos", "IVA (303)", "IRPF (130)", "Estado"];
    let ancho = |i: usize| filas.iter().map(|x| x[i].0.chars().count()).chain([cabeceras[i].chars().count()]).max().unwrap_or(0) as u16;
    let anchos = [
        Constraint::Length(ancho(0)),
        Constraint::Length(ancho(1)),
        Constraint::Length(ancho(2)),
        Constraint::Length(ancho(3)),
        Constraint::Length(ancho(4)),
        Constraint::Min(8),
    ];
    let cab = Row::new(cabeceras.map(|t| if t == "Estado" || t.is_empty() { Cell::from(t) } else { Cell::from(derecha(t)) })).style(tema::cabecera_tabla());
    let filas: Vec<Row> = filas
        .into_iter()
        .map(|x| Row::new(x.into_iter().enumerate().map(|(i, (t, st))| if (1..=4).contains(&i) { Cell::from(derecha(t)).style(st) } else { Cell::from(t).style(st) })))
        .collect();
    let espacio = if corto { 1 } else { 3 };
    f.render_widget(Table::new(filas, anchos).header(cab).column_spacing(espacio).block(bloque(titulo)), area);
}

/// «Etiqueta ……… valor» en `ancho` caracteres (si no cabe, se corta la etiqueta, nunca el importe).
fn kv(etq: &str, val: String, ancho: u16, estilo: Style) -> Line<'static> {
    let sitio = (ancho as usize).saturating_sub(val.chars().count() + 1);
    let etq: String = if etq.chars().count() > sitio { etq.chars().take(sitio.saturating_sub(1)).chain(['…']).collect() } else { etq.to_string() };
    let hueco = (ancho as usize).saturating_sub(etq.chars().count() + val.chars().count()).max(1);
    Line::from(vec![Span::styled(etq, tema::apagado()), Span::raw(" ".repeat(hueco)), Span::styled(val, estilo)])
}

/// Un año cerrado: lo de la Renta a la izquierda; lo pagado a cuenta y las cuotas a la derecha; y lo que no cuadra.
fn pintar_renta(f: &mut Frame, area: Rect, r: &DeRenta) {
    let a = &r.a;
    let b = bloque("Según tu Renta");
    let b = b.title(Span::styled(format!(" {} ", a.anio), tema::apagado()));
    let dentro = b.inner(area);
    f.render_widget(b, area);
    if dentro.height == 0 || dentro.width < 10 {
        return;
    }
    let dos = dentro.width >= 76;
    let w = if dos { (dentro.width - 3) / 2 } else { dentro.width };
    let eur = fmt::eur;
    let normal = Style::new();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut izq = vec![
        kv("Ingresos (0180)", eur(a.ingresos), w, normal),
        kv("Gastos deducibles", eur(a.gastos), w, normal),
        kv("Cuota de autónomo (0186)", eur(a.cuota), w, normal),
    ];
    if a.gdj > DIF {
        let etq = if w >= 40 { "Difícil justificación" } else { "Difícil justif." };
        izq.push(kv(&format!("{etq} ({})", fmt::pct(irpf::gdj_reglas(a.anio).0)), eur(a.gdj), w, normal));
    }
    izq.push(kv("Rendimiento neto (0224)", eur(a.neto), w, negrita.fg(tema::ACENTO)));
    if let (Some(red), Some(nr)) = (a.reduccion, a.neto_reducido)
        && red > DIF
    {
        izq.push(kv("Reducción", eur(red), w, normal));
        izq.push(kv("Rendimiento neto reducido", eur(nr), w, normal));
    }
    let mut der = vec![
        kv(if w >= 40 { "Retenciones de tus clientes" } else { "Retenciones" }, a.retenciones.map(eur).unwrap_or_else(|| "—".into()), w, normal),
        kv("Pagos fraccionados (130)", a.fraccionados.map(eur).unwrap_or_else(|| "—".into()), w, normal),
        kv("IVA ingresado (303)", eur(r.iva), w, normal),
    ];
    if let Some(c) = r.recibos {
        der.push(kv("Cuotas según tus recibos", eur(c), w, normal));
    }
    if !a.iae.is_empty() || !a.modalidad.is_empty() {
        let t = [if a.iae.is_empty() { String::new() } else { format!("IAE {}", a.iae) }, a.modalidad.to_lowercase()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
        der.push(kv("Actividad", t, w, tema::apagado()));
    }
    // Lo que conviene saber de ese año.
    let mut notas = Vec::new();
    if r.sin130 {
        notas.push(if a.fraccionados.unwrap_or(0.0) > DIF {
            "No tengo los 130 de este año (en la Renta sí hay pagos fraccionados).".to_string()
        } else {
            "No presentaste el 130 (tu Renta no tiene pagos fraccionados).".to_string()
        });
    }
    if r.sin_casillas > 0 {
        notas.push("«sin PDF»: presentado, pero sin el PDF en justificantes/ no sé sus casillas.".into());
    }
    if let Some(c) = r.recibos.filter(|c| (c - a.cuota).abs() > 1.0) {
        notas.push(format!("Tus recibos de {} suman {}; en la Renta pusiste {}.", a.anio, eur(c), eur(a.cuota)));
    }
    let lineas: Vec<Line> = if dos {
        let n = izq.len().max(der.len());
        let mut v: Vec<Line> = (0..n)
            .map(|i| {
                let mut s: Vec<Span> = izq.get(i).map(|l| l.spans.clone()).unwrap_or_else(|| vec![Span::raw(" ".repeat(w as usize))]);
                s.push(Span::raw("   "));
                s.extend(der.get(i).map(|l| l.spans.clone()).unwrap_or_default());
                Line::from(s)
            })
            .collect();
        v.extend(notas.into_iter().map(|t| Line::styled(t, tema::apagado())));
        v
    } else {
        izq.into_iter().chain(der).chain(notas.into_iter().map(|t| Line::styled(t, tema::apagado()))).collect()
    };
    f.render_widget(Paragraph::new(lineas).wrap(Wrap { trim: false }), dentro);
}

fn bloque_meses(anio: i32, abajo: Option<String>) -> Block<'static> {
    let titulo = Line::from(vec![
        Span::styled(format!(" Ingresos y gastos por mes · {anio} "), tema::titulo()),
        Span::styled("■", Style::new().fg(tema::ACENTO)),
        Span::styled(" ingresos sin IVA  ", tema::apagado()),
        Span::styled("■", Style::new().fg(tema::AVISO)),
        Span::styled(" gastos deducibles ", tema::apagado()),
    ]);
    let b = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(tema::apagado()).title(titulo);
    match abajo {
        Some(t) => b.title_bottom(Line::styled(format!(" {t} "), tema::apagado()).right_aligned()),
        None => b,
    }
}

/// Barras de ingresos y gastos, mes a mes, con el importe dentro si cabe («1,2k»).
fn pintar_grafico(f: &mut Frame, area: Rect, c: &Cifras, anio: i32) {
    let max = c.ing.iter().chain(&c.gas).fold(0.0_f64, |m, x| m.max(*x));
    let b = bloque_meses(anio, (max > 0.5).then(|| format!("máx. {}", compacto(max))));
    let dentro = b.inner(area);
    f.render_widget(b, area);
    if dentro.height < 2 || dentro.width < 4 {
        return;
    }
    if max <= 0.5 {
        let y = dentro.y + dentro.height / 2;
        f.render_widget(Paragraph::new(Line::styled(format!("Sin ingresos ni gastos en {anio}."), tema::apagado()).centered()), Rect { y, height: 1, ..dentro });
        return;
    }
    // 12 grupos de dos barras, con un hueco entre las dos barras y otro mayor entre meses.
    let w = (dentro.width.saturating_sub(46) / 24).min(6); // 12 × (2 barras + 2 huecos) + 11 × 2
    if w == 0 {
        let t = vec![
            Line::from(vec![Span::styled("Ingresos ", tema::apagado()), Span::raw(compacto(c.ing.iter().sum()))]),
            Line::from(vec![Span::styled("Gastos   ", tema::apagado()), Span::raw(compacto(c.gas.iter().sum()))]),
            Line::styled("Más ancho para ver los meses.", tema::apagado()),
        ];
        f.render_widget(Paragraph::new(t), dentro);
        return;
    }
    let barra = |x: f64, color: Color| {
        Bar::default()
            .value(x.max(0.0).round() as u64)
            .text_value(if x > 0.5 && compacto(x).chars().count() <= w as usize { compacto(x) } else { String::new() })
            .style(Style::new().fg(color))
            .value_style(Style::new().fg(Color::Black).bg(color))
    };
    let grupos: Vec<BarGroup> = (0..12)
        .map(|i| {
            let mes = fmt::MESES_C[i];
            let etiqueta = if w * 2 >= 3 { mes.to_string() } else { mes.chars().take(1).collect::<String>().to_uppercase() };
            BarGroup::default().label(Line::styled(etiqueta, tema::apagado())).bars(&[barra(c.ing[i], tema::ACENTO), barra(c.gas[i], tema::AVISO)])
        })
        .collect();
    let usado = (w * 24 + 46).min(dentro.width);
    let zona = Rect { x: dentro.x + (dentro.width - usado) / 2, width: usado, ..dentro };
    let grafico = BarChart::grouped(grupos).bar_width(w).bar_gap(1).group_gap(2).max(max.round().max(1.0) as u64);
    f.render_widget(grafico, zona);
}

/// Los meses en una tabla: ingresos, gastos y la diferencia (importes cortos). Para cuando el gráfico no cabe.
fn pintar_tabla_meses(f: &mut Frame, area: Rect, c: &Cifras, anio: i32) {
    let b = bloque_meses(anio, None);
    let dentro = b.inner(area);
    f.render_widget(b, area);
    let lw = 11usize;
    let cw = ((dentro.width as usize).saturating_sub(lw) / 13).clamp(4, 10);
    let celda = |s: String| format!("{s:>cw$}");
    let mut cab = " ".repeat(lw);
    for m in fmt::MESES_C {
        cab.push_str(&celda(m.to_string()));
    }
    cab.push_str(&celda("año".into()));
    let fila = |nombre: &str, v: [f64; 12], st: Style| -> Line<'static> {
        let mut s = vec![Span::styled(format!("{nombre:<lw$}"), tema::apagado())];
        for x in v {
            s.push(Span::styled(celda(if x.abs() <= DIF { "·".into() } else { compacto(x) }), st));
        }
        s.push(Span::styled(celda(compacto(v.iter().sum())), st.add_modifier(Modifier::BOLD)));
        Line::from(s)
    };
    let dif: [f64; 12] = std::array::from_fn(|i| r2(c.ing[i] - c.gas[i]));
    let t = vec![
        Line::styled(cab, tema::cabecera_tabla()),
        fila("Ingresos", c.ing, Style::new().fg(tema::ACENTO)),
        fila("Gastos", c.gas, Style::new().fg(tema::AVISO)),
        fila("Diferencia", dif, Style::new()),
    ];
    f.render_widget(Paragraph::new(t), dentro);
}

impl Anio {
    /// Lo calculado del año que se ve; se rehace si cambian los datos, el año o el día.
    fn preparar(&mut self, ctx: &Ctx) -> &Calculo {
        let hoy = fmt::hoy();
        let clave = (ctx.generacion, ctx.anio, hoy);
        if self.calculo.as_ref().is_none_or(|c| c.clave != clave) {
            let hoy = clave.2.clone();
            self.calculo = Some(Calculo::nuevo(&ctx.datos, ctx.anio, &hoy, clave));
            #[cfg(test)]
            {
                self.calculos += 1;
            }
        }
        self.calculo.as_ref().expect("recién calculado")
    }

    /// Para recalcular (al entrar en la pestaña, por si los datos cambiaron sin pasar por `guardar`).
    pub(super) fn olvidar(&mut self) {
        self.calculo = None;
    }

    pub(super) fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let c = self.preparar(ctx);
        let hoy = &c.clave.2;
        let av = &c.avisos;
        // Con poca altura, las cifras van en dos líneas sin recuadro.
        let grande = area.height >= 30;
        let ancho = area.width.saturating_sub(2);
        let n_av = if av.is_empty() { alto_aviso(&al_dia(hoy), ancho) } else { av.iter().map(|a| alto_aviso(&a.texto, ancho)).sum::<u16>() };
        let n_av = n_av.min(if grande { 6 } else { 3 });
        let [a_kpi, a_av, a_trim, a_mes] =
            Layout::vertical([Constraint::Length(if grande { 5 } else { 2 }), Constraint::Length(n_av + 2), Constraint::Length(8), Constraint::Min(0)]).areas(area);
        pintar_kpis(f, a_kpi, &c.vista, grande);
        pintar_avisos(f, a_av, av, hoy);
        let anio = ctx.anio;
        match &c.vista {
            Vista::App(cf) => {
                pintar_trimestres(f, a_trim, c.trims(), &format!("Trimestres de {anio}"));
                // Con poca altura, el gráfico no se lee: mejor la tabla.
                if a_mes.height >= 8 {
                    pintar_grafico(f, a_mes, cf, anio);
                } else if a_mes.height >= 5 {
                    pintar_tabla_meses(f, a_mes, cf, anio);
                }
            }
            Vista::Renta(r) => {
                pintar_trimestres(f, a_trim, c.trims(), &format!("Trimestres de {anio} · lo presentado"));
                if a_mes.height >= 3 {
                    pintar_renta(f, a_mes, r);
                }
            }
        }
    }
}
