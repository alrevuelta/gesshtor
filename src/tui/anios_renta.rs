//! La Renta de un año (modelo 100, leída de su PDF) para el detalle de «Años»: su recibo («por qué» sale lo que
//! sale, con puntos guía) y la Renta casilla a casilla; y el gráfico de lo que salió en cada Renta. Aquí solo se
//! mira.
use crate::dominio::fmt::DIF;
use super::declaraciones::primera_que_cabe;
use crate::tui::texto::cuando;
use crate::dominio::renta::Renta;
use crate::dominio::fmt;
use crate::hacienda;
use crate::hacienda::modelo_100::{numeracion, Numeracion};
use crate::tui::texto::{dos_columnas, partir};
use super::anios::color_resultado;
use crate::tui::widgets::{barra, guia, linea_atajos, muestra, tema, titulo_bloque};
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

/// Cómo se pagó o se cobró: «domiciliado, en dos plazos: 108,12 € el 30 jun 2024 y 72,08 € el 5 nov 2024»; None si
/// no se sabe.
fn forma_de_pago(r: &Renta) -> Option<String> {
    let plazos: Vec<String> = r
        .plazos
        .iter()
        .map(|p| if p.fecha.is_empty() { fmt::eur(p.importe) } else { format!("{} el {}", fmt::eur(p.importe), fmt::fecha_corta(&p.fecha)) })
        .collect();
    let forma = super::declaraciones_detalle::forma(Some(crate::hacienda::Modelo::M100), r.forma);
    if forma.is_empty() {
        return None;
    }
    let mut t = forma.to_string();
    if plazos.len() > 1 {
        t.push_str(&format!(", en dos plazos: {}", plazos.join(" y ")));
    } else if let (Some(p), "domiciliado") = (plazos.first(), forma) {
        t.push_str(&format!(", {p}"));
    }
    Some(t)
}

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
    l.push(h.fila_con(et, fmt::eur(x.abs()), color_resultado(x).add_modifier(Modifier::BOLD), h.cas.resultado, Tipo::Fuerte));
    if let Some(t) = forma_de_pago(r) {
        l.extend(h.nota(&format!("Forma de pago: {t}.")));
    }
    ("resultado y pagos", l)
}

fn seccion_trabajo(h: &Hoja, r: &Renta) -> Seccion {
    let Some(t) = &r.trabajo else { return ("trabajo", h.nota("Sin rendimientos del trabajo (o no se pudieron leer).")) };
    let mut l = vec![h.fila("Ingresos íntegros", Some(t.integros), h.cas.trabajo.integros, Tipo::Normal)];
    l.push(h.fila("en dinero", Some(t.dinerario), h.cas.trabajo.dinerario, Tipo::Sub));
    if t.especie.abs() > DIF {
        l.push(h.fila("en especie", Some(t.especie), h.cas.trabajo.especie, Tipo::Sub));
    }
    l.push(h.fila("Seguridad Social que te descuentan", Some(t.ss), h.cas.trabajo.ss, Tipo::Normal));
    l.push(h.fila("Rendimiento neto", t.neto, h.cas.trabajo.neto, Tipo::Fuerte));
    l.push(h.fila("Rendimiento neto reducido", t.neto_reducido, h.cas.trabajo.reducido, Tipo::Normal));
    l.push(h.fila("Retenciones", t.retenciones, h.cas.ret_trabajo, Tipo::Normal));
    ("trabajo", l)
}

fn seccion_actividad(h: &Hoja, r: &Renta) -> Seccion {
    let Some(a) = &r.actividad else { return ("autónomo", h.nota("Sin rendimientos como autónomo (o no se pudieron leer).")) };
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
    ("autónomo", l)
}

fn seccion_capital(h: &Hoja, r: &Renta) -> Seccion {
    let Some(k) = &r.capital else { return ("capital mobiliario", h.nota("Sin intereses ni dividendos (o no se pudieron leer).")) };
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
    ("capital mobiliario", l)
}

fn seccion_ganancias(h: &Hoja, r: &Renta) -> Seccion {
    let Some(g) = &r.ganancias else { return ("ganancias patrimoniales", h.nota("Sin ganancias ni pérdidas (o no se pudieron leer).")) };
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
    ("ganancias patrimoniales", l)
}

fn seccion_inmuebles(h: &Hoja, r: &Renta) -> Seccion {
    let Some(i) = &r.inmuebles else {
        let t = "Sin alquileres ni inmuebles.";
        return ("inmuebles y alquileres", h.nota(t));
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
    ("inmuebles y alquileres", l)
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
    ("bases imponibles", l)
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
        l.push(titulo_bloque(t));
        l.extend(s);
    }
    l
}

/// La Renta de un año entera: lo que salió, cuándo se presentó y sus secciones (en dos columnas de `w` si `dos`).
pub(super) fn lineas(y: i32, r: &Renta, presentada: Option<&str>, w: usize, dos: bool) -> Vec<Line<'static>> {
    let ancho = if dos { w * 2 + 3 } else { w };
    let partido = |t: &str, st: Style| partir(t, ancho).into_iter().map(move |l| Line::styled(l, st)).collect::<Vec<_>>();
    let mut l = vec![titulo_bloque(format!("tu Renta de {y}, casilla a casilla"))];
    let x = r.resultado;
    let et = if x > DIF { "A pagar" } else if x < -DIF { "A devolver" } else { "Resultado" };
    let mut v = vec![Span::styled(format!("{et} {}", fmt::eur(x.abs())), color_resultado(x).add_modifier(Modifier::BOLD))];
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

/* ---------------- el recibo: por qué sale lo que sale ---------------- */

/// Dónde acaban las cifras del recibo y de la banda del detalle (contando desde la izquierda)…
pub(super) const FIN: u16 = 46;
/// …y dónde empieza lo que va al lado (las notas de la banda, las barras del recibo).
pub(super) const AL_LADO: usize = 50;

/// «Por qué» sale lo que sale en tu Renta, como un recibo con puntos guía: lo que ganaste (`ganaste`, con su
/// `nota`), lo que te cuenta Hacienda (las bases), lo que te tocaba pagar y lo que ya habías adelantado; debajo de
/// una raya, lo que salió. Si `w` deja sitio, al lado de lo que te tocaba y de lo adelantado, dos barras finas que
/// los comparan (lo verde es lo que te devuelven).
pub(super) fn recibo(r: &Renta, ganaste: f64, nota: &str, w: usize) -> Vec<Line<'static>> {
    let fin = FIN.min(w as u16);
    let gris = tema::apagado();
    let sub = |t: &str| Line::styled(format!("  {t}"), gris);
    let mut l = vec![guia("ganaste", crate::informe::anio::euro(ganaste), Style::new(), fin)];
    if !nota.is_empty() {
        l.push(sub(nota));
    }
    let bases = r.bases.as_ref().map(|b| (b.general, b.ahorro)).or(r.cuotas.as_ref().map(|q| (q.base_general, q.base_ahorro)));
    if let Some((general, ahorro)) = bases {
        l.push(guia("a Hacienda le cuentan", fmt::eur(general), Style::new(), fin));
        l.push(sub("quitando gastos y Seguridad Social"));
        if ahorro.abs() > DIF {
            l.push(guia("y del ahorro", fmt::eur(ahorro), Style::new(), fin));
        }
    }
    l.push(Line::from(""));
    let (toca, adelantado) = (r.cuota, pagos_cuenta(r));
    let mut tocaba = guia("te tocaba pagar", fmt::eur(toca), Style::new().fg(tema::AVISO), fin);
    let mut habias = guia("ya habías adelantado", fmt::eur(adelantado), Style::new().fg(tema::AZUL), fin);
    // Las barras: lo que te tocaba, y lo adelantado (lo que pasa de lo que te tocaba, en verde: es lo que vuelve).
    let mayor = toca.max(adelantado);
    let ancho = w.saturating_sub(AL_LADO + 2).min(46);
    if ancho >= 10 && toca > DIF && adelantado > DIF {
        let largo = |x: f64| ((x / mayor) * ancho as f64).round().max(1.0) as u16;
        for (linea, tramos, x) in [(&mut tocaba, vec![(toca, tema::AVISO)], toca), (&mut habias, vec![(toca.min(adelantado), tema::AZUL), ((adelantado - toca).max(0.0), tema::BIEN)], adelantado)] {
            let hueco = AL_LADO.saturating_sub(linea.width());
            linea.push_span(Span::raw(" ".repeat(hueco)));
            linea.spans.extend(barra(&tramos, largo(x)).spans);
        }
    }
    l.extend([tocaba, habias]);
    // Lo que no sale de restar las dos de arriba: otras deducciones o lo de la declaración anterior.
    if (toca - adelantado - r.diferencial).abs() > DIF {
        l.push(guia("cuota diferencial", fmt::eur(r.diferencial), Style::new(), fin));
    }
    if r.rectificativa {
        for (et, x) in [("ya pagado con la anterior", r.pagado_antes), ("ya devuelto con la anterior", r.devuelto_antes)] {
            if x.abs() > DIF {
                l.push(guia(et, fmt::eur(x), Style::new(), fin));
            }
        }
    }
    let x = r.resultado;
    let cifra = fmt::eur(x.abs());
    let raya = cifra.chars().count().max(11);
    l.push(Line::styled(format!("{}{}", " ".repeat((fin as usize).saturating_sub(raya)), "─".repeat(raya)), Style::new().fg(tema::PUNTOS)));
    let st = color_resultado(x);
    let et = if x > DIF { "a pagar" } else if x < -DIF { "para ti" } else { "a cero" };
    l.push(guia(Span::styled(et, st.add_modifier(Modifier::BOLD)), cifra, st, fin));
    l
}

/// Lo que va al lado de la banda del detalle: cómo se pagó o se cobró y, si corrige otra, cuál.
pub(super) fn notas_banda(r: &Renta) -> Vec<String> {
    let mut v = Vec::new();
    // Si te devuelven, ya lo dice la banda.
    if let Some(t) = forma_de_pago(r).filter(|_| !r.forma.is_some_and(|f| f.devuelve())) {
        v.push(format!("forma de pago: {t}."));
    }
    if r.rectificativa {
        v.push(if r.anterior.is_empty() { "Rectificativa.".into() } else { format!("Rectificativa de la {}.", r.anterior) });
    } else if !r.anterior.is_empty() {
        v.push(format!("Corrige la {}.", r.anterior));
    }
    v
}

/// Barras finas por año («▄»): a la derecha del eje, lo que salió a pagar (en AVISO); a la izquierda, lo devuelto
/// (en verde); sin Renta, «—». Sin caja: arriba, el título en gris y, a la derecha, `g` con `siguiente`. Si no
/// cabe, se quitan las cifras y luego los años que no se ven.
pub(super) fn dibujar_grafico(f: &mut Frame, area: Rect, datos: &[(i32, Option<f64>)], sel: Option<i32>, siguiente: &str) {
    let titulo = "lo que salió en tu Renta";
    let a_tit = Rect { height: area.height.min(1), ..area };
    let dentro = Rect { y: area.y + a_tit.height, height: area.height - a_tit.height, ..area };
    f.render_widget(Paragraph::new(titulo_bloque(titulo)), a_tit);
    let teclas = linea_atajos(&[("g", siguiente)]);
    if titulo.chars().count() + 3 + teclas.width() <= a_tit.width as usize {
        f.render_widget(Paragraph::new(teclas.right_aligned()), a_tit);
    }
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
    // Lo que ocupa una barra, en celdas enteras (al menos una, si hay algo).
    let celdas = |largo: f64, tope: usize| (largo.round() as usize).max(1).min(tope);

    let mut lineas: Vec<Line> = Vec::new();
    for (anio, x) in datos.iter().skip(ini).take(filas) {
        let elegido = Some(*anio) == sel;
        let st_anio = if elegido { tema::titulo() } else { tema::apagado() };
        let mut v = vec![Span::styled(format!("{anio} "), st_anio)];
        let color = x.map_or(tema::apagado(), color_resultado);
        let largo = x.map(|x| x.abs() * escala).unwrap_or(0.0);
        // Lado izquierdo (devoluciones), pegado al eje.
        let llenos_i = if x.is_some_and(|x| x < -0.005) { celdas(largo, izq) } else { 0 };
        v.push(Span::raw(" ".repeat(izq - llenos_i)));
        v.push(Span::styled("▄".repeat(llenos_i), color));
        v.push(Span::styled("│", Style::new().fg(tema::MARCO)));
        // Lado derecho (a pagar).
        let ocupado = if x.is_some_and(|x| x > 0.005) { celdas(largo, der) } else { 0 };
        v.push(Span::styled("▄".repeat(ocupado), color));
        if val_w > 0 {
            v.push(Span::raw(" ".repeat(der.saturating_sub(ocupado) + 1)));
            let t = cifra(*x, larga);
            let st = if elegido { color.add_modifier(Modifier::BOLD) } else { color };
            v.push(Span::styled(format!("{t:>val_w$}"), st));
        }
        lineas.push(Line::from(v));
    }
    if leyenda {
        let mut l = Line::from(vec![muestra(tema::AVISO), Span::styled("a pagar   ", tema::apagado()), muestra(tema::BIEN), Span::styled("a devolver", tema::apagado())]);
        l.spans.insert(0, Span::raw(" ".repeat(5)));
        lineas.push(l);
    }
    f.render_widget(Paragraph::new(lineas), dentro);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn el_recibo_de_una_renta() {
        // Como en el boceto: te tocaban 3.121,24 €, habías adelantado 3.736,42 € y te devuelven 615,18 €.
        let pagos = crate::dominio::renta::Pagos { total: 3736.42, ..Default::default() };
        let mut r = Renta { cuota: 3121.24, pagos_cuenta: 3736.42, diferencial: -615.18, resultado: -615.18, pagos: Some(pagos), ..Default::default() };
        r.bases = Some(crate::dominio::renta::Bases { general: 20010.64, ahorro: 0.0 });
        let l: Vec<String> = recibo(&r, 29686.40, "antes de impuestos", 96).iter().map(texto).collect();
        let fila = |et: &str| l.iter().find(|x| x.starts_with(et)).unwrap_or_else(|| panic!("falta «{et}»: {l:#?}")).clone();
        // Las cifras acaban todas en la misma columna, con puntos guía hasta ellas.
        for (et, cifra) in [("ganaste", "29.686,40 €"), ("a Hacienda le cuentan", "20.010,64 €"), ("te tocaba pagar", "3.121,24 €"), ("ya habías adelantado", "3.736,42 €"), ("para ti", "615,18 €")] {
            let f = fila(et);
            let fin = f.find(cifra).map(|i| f[..i].chars().count() + cifra.chars().count());
            assert_eq!(fin, Some(FIN as usize), "{f}");
            assert!(f.contains('·'), "{f}");
        }
        // Al lado, las barras; sin la base del ahorro (es cero) ni la cuota diferencial (sale de restar).
        assert!(fila("te tocaba pagar").contains('▄') && fila("ya habías adelantado").contains('▄'));
        assert!(!l.iter().any(|x| x.starts_with("y del ahorro") || x.starts_with("cuota diferencial")), "{l:#?}");
        // Estrecho: sin barras y sin pasarse.
        let estrecho: Vec<String> = recibo(&r, 29686.40, "", 40).iter().map(texto).collect();
        assert!(estrecho.iter().all(|x| x.chars().count() <= 40 && !x.contains('▄')), "{estrecho:#?}");
        // A pagar, y con algo que no sale de restar: se enseña la cuota diferencial.
        let r = Renta { cuota: 2000.0, pagos_cuenta: 1500.0, diferencial: 400.0, resultado: 400.0, ..r };
        let l: Vec<String> = recibo(&r, 29686.40, "", 96).iter().map(texto).collect();
        assert!(l.iter().any(|x| x.starts_with("a pagar") && x.ends_with("400,00 €")), "{l:#?}");
        assert!(l.iter().any(|x| x.starts_with("cuota diferencial")), "{l:#?}");
    }

    #[test]
    fn grafico_divergente() {
        // Con resultados de los dos signos, el eje queda en medio y cada barra va a su lado (el título, sin caja, en
        // la primera fila).
        let datos = [(2025, Some(1000.0)), (2024, Some(-500.0)), (2023, None)];
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 6)).unwrap();
        t.draw(|f| dibujar_grafico(f, f.area(), &datos, Some(2024), "por conceptos")).unwrap();
        let b = t.backend().buffer();
        let fila = |y: u16| (0..60).map(|x| b[(x, y)].symbol().to_string()).collect::<String>();
        let (l25, l24) = (fila(1), fila(2));
        let eje = |s: &str| s.chars().position(|c| c == '│').unwrap();
        assert_eq!(eje(&l25[3..]), eje(&l24[3..]), "{l25}\n{l24}");
        assert!(l25.contains("1.000 € a pagar") && l24.contains("500 € a devolver"), "{l25}\n{l24}");
        let i25 = l25.chars().position(|c| c == '▄').unwrap();
        let i24 = l24.chars().position(|c| c == '▄').unwrap();
        assert!(i24 < i25, "{l25}\n{l24}");
    }
}
