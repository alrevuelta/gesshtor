//! La Renta de un año (modelo 100, leída de su PDF), casilla a casilla, para el detalle de «Años»; y el gráfico de
//! lo que salió en cada Renta. Aquí solo se mira.
use crate::dominio::fmt::DIF;
use super::declaraciones::primera_que_cabe;
use crate::tui::texto::cuando;
use crate::dominio::renta::Renta;
use crate::dominio::fmt;
use crate::hacienda;
use crate::hacienda::modelo_100::{numeracion, Numeracion};
use crate::tui::texto::{dos_columnas, partir};
use crate::tui::widgets::{bloque, tema};
use crate::tui::Ctx;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;


/* ---------------- lo que hay en la Renta leída ---------------- */

/// Las retenciones y pagos a cuenta (o, si no están, su suma por origen).
fn pagos_cuenta(r: &Renta) -> f64 {
    if r.pagos_cuenta.abs() > DIF { r.pagos_cuenta } else { r.pagos.as_ref().map_or(0.0, |p| p.total) }
}

/* ---------------- el detalle, en secciones ---------------- */

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tipo {
    Normal,
    /// Un desglose de la fila de arriba (sangrado y en gris).
    Sub,
    Fuerte,
}

/// Cómo se escriben las filas de una columna del detalle.
struct Hoja {
    w: usize,
    /// Con el número de casilla a la derecha.
    con_casillas: bool,
    /// Las casillas de la Renta que se enseña.
    cas: &'static Numeracion,
}

impl Hoja {
    fn fila(&self, etiqueta: &str, v: Option<f64>, cas: impl Into<Option<u32>>, tipo: Tipo) -> Line<'static> {
        let estilo = match (v, tipo) {
            (None, _) => tema::apagado(),
            (_, Tipo::Fuerte) => Style::new().add_modifier(Modifier::BOLD),
            _ => Style::new(),
        };
        self.fila_con(etiqueta, v.map(fmt::eur).unwrap_or_else(|| "—".into()), estilo, cas, tipo)
    }

    fn fila_con(&self, etiqueta: &str, valor: String, estilo: Style, cas: impl Into<Option<u32>>, tipo: Tipo) -> Line<'static> {
        let c = match (self.con_casillas, cas.into()) {
            (false, _) => String::new(),
            (true, Some(n)) => format!("  {n:04}"),
            (true, None) => " ".repeat(6),
        };
        let mut et = if tipo == Tipo::Sub { format!("  {etiqueta}") } else { etiqueta.to_string() };
        let libre = self.w.saturating_sub(valor.chars().count() + c.chars().count() + 1);
        if et.chars().count() > libre {
            et = et.chars().take(libre.saturating_sub(1)).collect::<String>() + "…";
        }
        let hueco = self.w.saturating_sub(et.chars().count() + valor.chars().count() + c.chars().count());
        let st_et = match tipo {
            Tipo::Sub => tema::apagado(),
            Tipo::Fuerte => Style::new().add_modifier(Modifier::BOLD),
            Tipo::Normal => Style::new(),
        };
        Line::from(vec![Span::styled(et, st_et), Span::raw(" ".repeat(hueco)), Span::styled(valor, estilo), Span::styled(c, tema::apagado())])
    }

    /// Una nota en gris, partida para que quepa.
    fn nota(&self, s: &str) -> Vec<Line<'static>> {
        partir(s, self.w).into_iter().map(|l| Line::styled(l, tema::apagado())).collect()
    }
}

type Seccion = (&'static str, Vec<Line<'static>>);

fn seccion_resultado(h: &Hoja, r: &Renta) -> Seccion {
    let mut l = Vec::new();
    if let Some(q) = &r.cuotas {
        l.push(h.fila("Cuota íntegra", Some(q.integra), None, Tipo::Normal));
        if (q.integra - q.liquida).abs() > DIF {
            l.push(h.fila("deducciones", Some(q.liquida - q.integra), None, Tipo::Sub));
        }
        l.push(h.fila("Cuota líquida", Some(q.liquida), None, Tipo::Normal));
    }
    l.push(h.fila("Cuota resultante", Some(r.cuota), h.cas.cuota, Tipo::Fuerte));
    l.push(h.fila("Retenciones y pagos a cuenta", Some(pagos_cuenta(r)), h.cas.pagos, Tipo::Normal));
    let p = r.pagos.as_ref();
    l.push(h.fila("de tu nómina", p.map(|p| p.trabajo).or_else(|| r.trabajo.as_ref().and_then(|t| t.retenciones)), h.cas.ret_trabajo, Tipo::Sub));
    l.push(h.fila("de tu banco (capital)", p.map(|p| p.capital), h.cas.ret_capital, Tipo::Sub));
    l.push(h.fila("de tus clientes (autónomo)", p.map(|p| p.actividades), h.cas.ret_actividades, Tipo::Sub));
    l.push(h.fila("pagos fraccionados (130)", p.map(|p| p.fraccionados).or(r.fraccionados), h.cas.fraccionados, Tipo::Sub));
    l.push(h.fila("Cuota diferencial", Some(r.diferencial), h.cas.diferencial, Tipo::Normal));
    if r.rectificativa {
        l.push(h.fila("ya pagado con la anterior", Some(r.pagado_antes), h.cas.pagado_antes, Tipo::Sub));
        l.push(h.fila("ya devuelto con la anterior", Some(r.devuelto_antes), h.cas.devuelto_antes, Tipo::Sub));
        l.push(h.fila("Regularización", r.regularizacion, h.cas.regularizacion, Tipo::Normal));
    }
    let x = r.resultado;
    let et = if x > DIF { "A pagar" } else if x < -DIF { "A devolver" } else { "Resultado" };
    l.push(h.fila_con(et, fmt::eur(x.abs()), tema::importe(x).add_modifier(Modifier::BOLD), h.cas.resultado, Tipo::Fuerte));
    // Cómo se pagó o se cobró.
    let plazos: Vec<String> = r
        .plazos
        .iter()
        .map(|p| if p.fecha.is_empty() { fmt::eur(p.importe) } else { format!("{} el {}", fmt::eur(p.importe), fmt::fecha_corta(&p.fecha)) })
        .collect();
    let forma = super::declaraciones_detalle::forma(Some(crate::hacienda::Modelo::M100), r.forma);
    if !forma.is_empty() {
        let mut t = format!("Forma de pago: {forma}");
        if plazos.len() > 1 {
            t.push_str(&format!(", en dos plazos: {}", plazos.join(" y ")));
        } else if let (Some(p), "domiciliado") = (plazos.first(), forma) {
            t.push_str(&format!(", {p}"));
        }
        l.extend(h.nota(&format!("{t}.")));
    }
    ("Resultado y pagos", l)
}

fn seccion_trabajo(h: &Hoja, r: &Renta) -> Seccion {
    let Some(t) = &r.trabajo else { return ("Trabajo", h.nota("Sin rendimientos del trabajo (o no se pudieron leer).")) };
    let mut l = vec![h.fila("Ingresos íntegros", Some(t.integros), h.cas.trabajo.integros, Tipo::Normal)];
    l.push(h.fila("en dinero", Some(t.dinerario), h.cas.trabajo.dinerario, Tipo::Sub));
    if t.especie.abs() > DIF {
        l.push(h.fila("en especie", Some(t.especie), h.cas.trabajo.especie, Tipo::Sub));
    }
    l.push(h.fila("Seguridad Social que te descuentan", Some(t.ss), h.cas.trabajo.ss, Tipo::Normal));
    l.push(h.fila("Rendimiento neto", t.neto, h.cas.trabajo.neto, Tipo::Fuerte));
    l.push(h.fila("Rendimiento neto reducido", t.neto_reducido, h.cas.trabajo.reducido, Tipo::Normal));
    l.push(h.fila("Retenciones", t.retenciones, h.cas.ret_trabajo, Tipo::Normal));
    ("Trabajo", l)
}

fn seccion_actividad(h: &Hoja, r: &Renta) -> Seccion {
    let Some(a) = &r.actividad else { return ("Autónomo", h.nota("Sin rendimientos como autónomo (o no se pudieron leer).")) };
    let mut l = Vec::new();
    let iae = if a.iae.is_empty() { String::new() } else { format!("IAE {}", a.iae) };
    let que: Vec<&str> = [a.modalidad.as_str(), iae.as_str(), a.tipo.as_str()].into_iter().filter(|x| !x.is_empty()).collect();
    if !que.is_empty() {
        l.extend(h.nota(&que.join(" · ")));
    }
    l.push(h.fila("Ingresos", Some(a.ingresos), h.cas.actividad.ingresos, Tipo::Normal));
    l.push(h.fila("Gastos deducibles", Some(a.gastos), h.cas.actividad.gastos[0], Tipo::Normal));
    l.push(h.fila("Seguridad Social del titular", a.ss_titular, None, Tipo::Sub));
    l.push(h.fila("de difícil justificación", a.gdj, h.cas.actividad.gdj, Tipo::Sub));
    l.push(h.fila("Rendimiento neto", Some(a.neto), h.cas.actividad.neto, Tipo::Fuerte));
    if a.reduccion.is_some_and(|x| x.abs() > DIF) {
        l.push(h.fila("Reducción", a.reduccion, h.cas.actividad.reduccion, Tipo::Normal));
    }
    l.push(h.fila("Rendimiento neto reducido", a.neto_reducido, h.cas.actividad.reducido, Tipo::Normal));
    ("Autónomo", l)
}

fn seccion_capital(h: &Hoja, r: &Renta) -> Seccion {
    let Some(k) = &r.capital else { return ("Capital mobiliario", h.nota("Sin intereses ni dividendos (o no se pudieron leer).")) };
    let mut l = Vec::new();
    if k.solo_total {
        l.extend(h.nota("Conjunta con rendimientos de los dos: solo se sabe el total."));
    } else {
        l.push(h.fila("Intereses", Some(k.intereses), h.cas.capital.intereses, Tipo::Normal));
        l.push(h.fila("Dividendos", Some(k.dividendos), h.cas.capital.dividendos, Tipo::Normal));
        l.push(h.fila("Otros rendimientos", Some(k.otros), None, Tipo::Normal));
        l.push(h.fila("Ingresos íntegros", Some(k.integros), h.cas.capital.integros, Tipo::Normal));
        l.push(h.fila("Gastos", Some(k.gastos), h.cas.capital.gastos, Tipo::Sub));
    }
    l.push(h.fila("Rendimiento neto", Some(k.neto), h.cas.capital.ahorro, Tipo::Fuerte));
    ("Capital mobiliario", l)
}

fn seccion_ganancias(h: &Hoja, r: &Renta) -> Seccion {
    let Some(g) = &r.ganancias else { return ("Ganancias patrimoniales", h.nota("Sin ganancias ni pérdidas (o no se pudieron leer).")) };
    let mut l = vec![
        h.fila("Ganancias", Some(g.ganancias), h.cas.ganancias.ganancias, Tipo::Normal),
        h.fila("Pérdidas", Some(g.perdidas), h.cas.ganancias.perdidas, Tipo::Normal),
        h.fila("Saldo neto", Some(g.neto), h.cas.ganancias.neto, Tipo::Fuerte),
    ];
    // Por tipo de bien (solo lo traen las leídas con la numeración actual).
    if let (Some(c), Some(o)) = (&g.cripto, &g.otros) {
        let resto = (g.ganancias - c.ganancias - o.ganancias, g.perdidas - c.perdidas - o.perdidas);
        for (nombre, (a, b)) in [("criptomonedas", (c.ganancias, c.perdidas)), ("fondos, acciones y demás", resto), ("otros bienes", (o.ganancias, o.perdidas))] {
            if a.abs() > DIF || b.abs() > DIF {
                l.push(h.fila(nombre, Some(a - b), None, Tipo::Sub));
            }
        }
        let compensado = g.ganancias - g.perdidas - g.neto;
        if compensado > DIF {
            l.push(h.fila("compensado con otros años", Some(-compensado), None, Tipo::Sub));
        }
    }
    ("Ganancias patrimoniales", l)
}

fn seccion_inmuebles(h: &Hoja, r: &Renta) -> Seccion {
    let Some(i) = &r.inmuebles else {
        let t = "Sin alquileres ni inmuebles.";
        return ("Inmuebles y alquileres", h.nota(t));
    };
    let mut l = vec![
        h.fila("Alquileres cobrados", Some(i.ingresos), h.cas.inmuebles.ingresos, Tipo::Normal),
        h.fila("Rendimiento neto", Some(i.neto), h.cas.inmuebles.neto, Tipo::Normal),
        h.fila("reducción por alquiler de vivienda", Some(-i.reduccion), None, Tipo::Sub),
        h.fila("Rendimiento reducido", Some(i.reducidos), h.cas.inmuebles.suma, Tipo::Fuerte),
        h.fila("Renta imputada", Some(i.imputaciones), h.cas.inmuebles.imputaciones, Tipo::Normal),
    ];
    if i.imputaciones > DIF {
        l.extend(h.nota("La renta imputada es de los inmuebles que no tienes alquilados: suma a la base, pero no es dinero que cobres."));
    }
    ("Inmuebles y alquileres", l)
}

fn seccion_bases(h: &Hoja, r: &Renta) -> Seccion {
    let q = r.cuotas.as_ref();
    let mut l = vec![
        h.fila("Base imponible general", r.bases.as_ref().map(|b| b.general).or(q.map(|q| q.base_general)), h.cas.cuotas.base_general, Tipo::Normal),
        h.fila("Base imponible del ahorro", r.bases.as_ref().map(|b| b.ahorro).or(q.map(|q| q.base_ahorro)), h.cas.cuotas.base_ahorro, Tipo::Normal),
    ];
    if let Some(q) = q {
        let medio = |c: f64, b: f64| if b > DIF { format!("tipo medio {}", fmt::pct((c / b * 10000.0).round() / 100.0)) } else { String::new() };
        l.push(h.fila("Base liquidable general", Some(q.liquidable_general), h.cas.cuotas.liquidable_general, Tipo::Normal));
        l.push(h.fila("Base liquidable del ahorro", Some(q.liquidable_ahorro), h.cas.cuotas.liquidable_ahorro, Tipo::Normal));
        l.push(h.fila("Cuota por la base general", Some(q.general), None, Tipo::Normal));
        let m = medio(q.general, q.liquidable_general);
        if !m.is_empty() {
            l.push(h.fila_con(&m, String::new(), Style::new(), None, Tipo::Sub));
        }
        l.push(h.fila("Cuota por el ahorro", Some(q.ahorro), None, Tipo::Normal));
        let m = medio(q.ahorro, q.liquidable_ahorro);
        if !m.is_empty() {
            l.push(h.fila_con(&m, String::new(), Style::new(), None, Tipo::Sub));
        }
    }
    if !r.comunidad.is_empty() {
        let nombre = crate::hacienda::irpf::nombre_comunidad(&r.comunidad);
        l.extend(h.nota(&format!("Comunidad: {nombre}")));
    }
    ("Bases imponibles", l)
}

fn secciones(h: &Hoja, r: &Renta) -> Vec<Seccion> {
    vec![seccion_resultado(h, r), seccion_trabajo(h, r), seccion_actividad(h, r), seccion_capital(h, r), seccion_ganancias(h, r), seccion_inmuebles(h, r), seccion_bases(h, r)]
}

/// Las secciones, una detrás de otra, con su título y una línea en blanco entre ellas.
fn juntar(v: Vec<Seccion>) -> Vec<Line<'static>> {
    let mut l = Vec::new();
    for (i, (t, s)) in v.into_iter().enumerate() {
        if i > 0 {
            l.push(Line::from(""));
        }
        l.push(Line::styled(t, tema::titulo()));
        l.extend(s);
    }
    l
}

/// La Renta de un año entera: lo que salió, cuándo se presentó y sus secciones (en dos columnas de `w` si `dos`).
pub(super) fn lineas(y: i32, r: &Renta, presentada: Option<&str>, w: usize, dos: bool) -> Vec<Line<'static>> {
    let ancho = if dos { w * 2 + 3 } else { w };
    let partido = |t: &str, st: Style| partir(t, ancho).into_iter().map(move |l| Line::styled(l, st)).collect::<Vec<_>>();
    let mut l = vec![Line::styled(format!("Tu Renta de {y}, casilla a casilla"), tema::titulo())];
    let x = r.resultado;
    let et = if x > DIF { "A pagar" } else if x < -DIF { "A devolver" } else { "Resultado" };
    let mut v = vec![Span::styled(format!("{et} {}", fmt::eur(x.abs())), tema::importe(x).add_modifier(Modifier::BOLD))];
    if let Some(fecha) = presentada {
        v.push(Span::styled(format!(" · presentada {}", cuando(fecha)), tema::apagado()));
    }
    l.push(Line::from(v));
    let mut extra: Vec<String> = Vec::new();
    if r.rectificativa {
        let ant = &r.anterior;
        extra.push(if ant.is_empty() { "Rectificativa".into() } else { format!("Rectificativa de la {ant}") });
    } else if !r.anterior.is_empty() {
        extra.push(format!("Corrige la {}", r.anterior));
    }
    if r.conjunta {
        extra.push("Conjunta".into());
    }
    if !extra.is_empty() {
        l.extend(partido(&extra.join(" · "), tema::apagado()));
    }
    let h = Hoja { w, con_casillas: w >= 52, cas: numeracion(&r.numeracion) };
    if h.con_casillas {
        l.extend(partido("En gris a la derecha, la casilla de la declaración.", tema::apagado()));
    }
    l.push(Line::from(""));
    let s = secciones(&h, r);
    if dos {
        // Repartidas (en su orden) para que las dos columnas queden parejas.
        let total: usize = s.iter().map(|x| x.1.len() + 2).sum();
        let (mut izq, mut der) = (Vec::new(), Vec::new());
        let mut llevo = 0;
        for x in s {
            if llevo < total / 2 {
                llevo += x.1.len() + 2;
                izq.push(x);
            } else {
                der.push(x);
            }
        }
        l.extend(dos_columnas(juntar(izq), juntar(der), w));
    } else {
        l.extend(juntar(s));
    }
    l
}

/// Abre el PDF de la Renta de ese año (el de `justificantes/`).
pub(super) fn abrir_pdf(ctx: &mut Ctx, anio: i32) {
    match hacienda::pdf_periodo(&ctx.dir, anio, 0, hacienda::Modelo::M100.codigo()) {
        Some(p) => ctx.abrir_ruta(&p),
        None => ctx.error(format!("No está el PDF de la Renta {anio} en la carpeta de datos (justificantes/).")),
    }
}

/// Barras horizontales por año: a la derecha del eje lo que salió a pagar (rojo), a la izquierda lo devuelto (verde);
/// sin Renta, «—». Si no cabe, se quitan las cifras y luego los años que no se ven. `pie`, abajo a la derecha.
pub(super) fn dibujar_grafico(f: &mut Frame, area: Rect, datos: &[(i32, Option<f64>)], sel: Option<i32>, pie: &str) {
    let b = bloque("Lo que salió en tu Renta").title_bottom(Line::styled(pie.to_string(), tema::apagado()).right_aligned());
    let dentro = b.inner(area);
    f.render_widget(b, area);
    if dentro.height == 0 || dentro.width < 12 || datos.is_empty() {
        return;
    }
    let leyenda = dentro.height as usize > datos.len();
    let filas = (dentro.height as usize - usize::from(leyenda)).min(datos.len());
    // Que se vea el año elegido.
    let i_sel = sel.and_then(|s| datos.iter().position(|x| x.0 == s)).unwrap_or(0);
    let ini = if i_sel >= filas { i_sel + 1 - filas } else { 0 };

    let cifra = |x: Option<f64>, larga: bool| match x {
        None => "—".to_string(),
        Some(x) if larga => {
            if x > 0.005 {
                format!("{} a pagar", fmt::eur0(x))
            } else if x < -0.005 {
                format!("{} a devolver", fmt::eur0(-x))
            } else {
                "0 €".into()
            }
        }
        Some(x) => fmt::eur0(x),
    };
    let w = dentro.width as usize;
    let largo = |larga: bool| datos.iter().map(|x| cifra(x.1, larga).chars().count()).max().unwrap_or(0);
    let (larga, val_w) = primera_que_cabe(w, &[(5 + 12 + largo(true) + 1, (true, largo(true))), (5 + 8 + largo(false) + 1, (false, largo(false))), (0, (false, 0))]);
    // Sin pasarse de largo, para que las cifras queden cerca de sus barras.
    let barras = w.saturating_sub(5 + val_w + usize::from(val_w > 0)).clamp(2, 64);

    let neg = datos.iter().filter_map(|x| x.1).filter(|x| *x < 0.0).map(f64::abs).fold(0.0, f64::max);
    let pos = datos.iter().filter_map(|x| x.1).filter(|x| *x > 0.0).fold(0.0, f64::max);
    let util = barras - 1; // menos el eje
    let izq = if neg <= 0.0 {
        0
    } else if pos <= 0.0 {
        util
    } else {
        // Que quede sitio a los dos lados del eje.
        let i = (util as f64 * neg / (neg + pos)).round() as usize;
        if util >= 2 { i.clamp(1, util - 1) } else { i.min(util) }
    };
    let der = util - izq;
    let escala = if neg + pos > 0.0 { if neg > 0.0 && pos > 0.0 { util as f64 / (neg + pos) } else { util as f64 / neg.max(pos) } } else { 0.0 };
    const OCTAVOS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

    let mut lineas: Vec<Line> = Vec::new();
    for (anio, x) in datos.iter().skip(ini).take(filas) {
        let elegido = Some(*anio) == sel;
        let st_anio = if elegido { Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD) } else { tema::apagado() };
        let mut v = vec![Span::styled(format!("{anio} "), st_anio)];
        let color = x.map_or(Style::new(), tema::importe);
        let largo = x.map(|x| x.abs() * escala).unwrap_or(0.0);
        // Lado izquierdo (devoluciones): bloques llenos pegados al eje, con medio bloque si sobra.
        let (llenos_i, medio) = if x.is_some_and(|x| x < -0.005) {
            let ll = (largo.floor() as usize).min(izq);
            (ll, ll < izq && (largo - ll as f64 >= 0.5 || ll == 0))
        } else {
            (0, false)
        };
        let usados = llenos_i + usize::from(medio);
        v.push(Span::raw(" ".repeat(izq - usados)));
        if medio {
            v.push(Span::styled("▐", color));
        }
        v.push(Span::styled("█".repeat(llenos_i), color));
        v.push(Span::styled("│", tema::apagado()));
        // Lado derecho (a pagar): bloques llenos y el octavo que sobre.
        let mut ocupado = 0;
        if x.is_some_and(|x| x > 0.005) {
            let ll = (largo.floor() as usize).min(der);
            let oct = if ll < der { ((largo - ll as f64) * 8.0).round().clamp(0.0, 7.0) as usize } else { 0 };
            let oct = if ll == 0 && oct == 0 { 1 } else { oct };
            v.push(Span::styled(format!("{}{}", "█".repeat(ll), OCTAVOS[oct]), color));
            ocupado = ll + usize::from(oct > 0);
        }
        if val_w > 0 {
            v.push(Span::raw(" ".repeat(der.saturating_sub(ocupado) + 1)));
            let t = cifra(*x, larga);
            let st = if elegido { color.add_modifier(Modifier::BOLD) } else { color };
            v.push(Span::styled(format!("{t:>val_w$}"), st));
        }
        lineas.push(Line::from(v));
    }
    if leyenda {
        lineas.push(Line::from(vec![
            Span::styled("█", Style::new().fg(tema::MAL)),
            Span::styled(" a pagar  ", tema::apagado()),
            Span::styled("█", Style::new().fg(tema::BIEN)),
            Span::styled(" a devolver", tema::apagado()),
        ]));
    }
    f.render_widget(Paragraph::new(lineas), dentro);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafico_divergente() {
        // Con resultados de los dos signos, el eje queda en medio y cada barra va a su lado.
        let datos = [(2025, Some(1000.0)), (2024, Some(-500.0)), (2023, None)];
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 6)).unwrap();
        t.draw(|f| dibujar_grafico(f, f.area(), &datos, Some(2024), " g ")).unwrap();
        let b = t.backend().buffer();
        let fila = |y: u16| (0..60).map(|x| b[(x, y)].symbol().to_string()).collect::<String>();
        let (l25, l24) = (fila(1), fila(2));
        let eje = |s: &str| s.chars().position(|c| c == '│').unwrap();
        assert_eq!(eje(&l25[3..]), eje(&l24[3..]), "{l25}\n{l24}");
        assert!(l25.contains("1.000 € a pagar") && l24.contains("500 € a devolver"), "{l25}\n{l24}");
        let i25 = l25.chars().position(|c| c == '█').unwrap();
        let i24 = l24.chars().position(|c| c == '█').unwrap();
        assert!(i24 < i25, "{l25}\n{l24}");
    }
}
