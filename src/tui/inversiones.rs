//! Inversiones: tu ahorro (dividendos, intereses y lo que vendes). Tres pestañas, con las cifras de
//! `informe::inversiones`:
//!
//! 1. **Este año**: con lo traído de tu bróker (`b` lo trae; la primera vez, pide activar el conector), cuánto has
//!    ganado o perdido con lo que has vendido, tus dividendos e intereses y lo que te tocaría pagar, y por qué.
//! 2. **Declarado**: lo que declaraste en cada Renta y lo que pagaste por ello.
//! 3. **Modelo 720**: lo que tenías fuera de España a 31 de diciembre y si te toca presentar el 720 (o el 721).
//!
//! Estilo C2, sin cajas: lo que te tocaría pagar (o te devolverían) este año, en una banda; el resto, con puntos
//! guía, títulos de bloque en gris y listas con la cabecera en gris y las cifras a la derecha.
use super::inversiones_conectar::{Conexion, Para};
use super::texto::partir;
use super::widgets::{derecha, guia, sello, tema, titulo_bloque, ALTO_BANDA};
use super::{Ctx, Pantalla};
use crate::brokers;
use crate::dominio::fmt::{self, eur, eur0, DIF};
use crate::dominio::inversiones::Clase;
use crate::hacienda::{ahorro, modelo_720};
use crate::hacienda::modelo_720::Toca;
use crate::informe::inversiones::{self as inf, Anio, Declarado};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

pub fn pantallas() -> Vec<Box<dyn Pantalla>> {
    vec![Box::new(EsteAnio::default()), Box::new(Declarados::default()), Box::new(Fuera::default())]
}

/// Los nombres de los brókeres con conector («DEGIRO»).
fn nombres() -> String {
    brokers::CONECTORES.iter().map(|c| c.nombre).collect::<Vec<_>>().join(", ")
}

/// Lo de moverse por un texto largo: ↑ ↓, PgUp PgDn, Inicio y Fin.
fn desplazar(desde: &mut usize, k: KeyEvent) -> bool {
    match k.code {
        KeyCode::Down => *desde += 1,
        KeyCode::Up => *desde = desde.saturating_sub(1),
        KeyCode::PageDown => *desde += 10,
        KeyCode::PageUp => *desde = desde.saturating_sub(10),
        KeyCode::Home => *desde = 0,
        KeyCode::End => *desde = usize::MAX / 2,
        _ => return false,
    }
    true
}

/// Pinta líneas sin caja, desde la `desde` (que se corrige si se pasa).
fn pintar(f: &mut Frame, area: Rect, v: Vec<Line<'static>>, desde: &mut usize) {
    *desde = (*desde).min(v.len().saturating_sub(area.height as usize));
    f.render_widget(Paragraph::new(v).scroll((*desde as u16, 0)), area);
}

fn gris() -> Style {
    tema::apagado()
}

fn negrita() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// Las líneas de un texto partido, con un estilo.
fn parrafo(s: &str, w: usize, estilo: Style) -> Vec<Line<'static>> {
    partir(s, w).into_iter().map(|l| Line::styled(l, estilo)).collect()
}

/// Un texto partido en líneas, con sangría.
fn sangrado(s: &str, w: usize, st: Style) -> Vec<Line<'static>> {
    parrafo(s, w.saturating_sub(3), st).into_iter().map(|mut l| {
        l.spans.insert(0, Span::raw("   "));
        l
    }).collect()
}

/// El ancho al que se parten los textos: no más de lo que se lee bien.
fn ancho_texto(w: usize) -> usize {
    w.min(92)
}

/// «+1.234,56 €» / «−300,00 €».
fn con_signo(x: f64) -> String {
    if x > DIF { format!("+{}", eur(x)) } else if x < -DIF { format!("−{}", eur(-x)) } else { eur(0.0) }
}

/// «−12,00 €»: lo que se resta.
fn menos(x: f64) -> String {
    format!("−{}", eur(x))
}

fn estilo_resultado(x: f64) -> Style {
    if x > DIF { Style::new().fg(tema::BIEN) } else if x < -DIF { Style::new().fg(tema::MAL) } else { Style::new() }
}

/// Una cifra con puntos guía hasta `fin` y su nota en gris: detrás si cabe en `w`; si no, debajo y con sangría.
fn guia_nota(etiqueta: impl Into<Line<'static>>, cifra: String, st: Style, fin: u16, nota: &str, w: usize) -> Vec<Line<'static>> {
    let mut l = guia(etiqueta, cifra, st, fin);
    if nota.is_empty() {
        return vec![l];
    }
    if l.width() + 3 + nota.chars().count() <= w {
        l.push_span(Span::styled(format!("   {nota}"), gris()));
        return vec![l];
    }
    let mut v = vec![l];
    v.extend(sangrado(nota, ancho_texto(w), gris()));
    v
}

/// Donde acaban las cifras con puntos guía: la etiqueta más larga, unos puntos y la cifra más larga (sin pasar de `w`).
fn fin_de(filas: &[(&str, &str)], w: usize) -> u16 {
    let et = filas.iter().map(|f| f.0.chars().count()).max().unwrap_or(0);
    let cifra = filas.iter().map(|f| f.1.chars().count()).max().unwrap_or(0);
    (et + cifra + 5).max(36).min(w) as u16
}

/// «2026-01-15» → «15 ene»; con el año si no es `anio` («15 ene 2025»).
fn fecha_de(s: &str, anio: i32) -> String {
    let c = fmt::fecha_corta(s);
    match c.rsplit_once(' ') {
        Some((dm, _)) if fmt::anio_de(s) == anio => dm.to_string(),
        _ => c,
    }
}

/// Una fila de una lista en columnas: las dos primeras a la izquierda (la segunda, la del nombre, con lo que sobre,
/// recortada si no cabe) y las demás a la derecha; dos espacios entre columnas.
fn fila_tabla(celdas: &[(String, Style)], anchos: &[usize]) -> Line<'static> {
    let mut v = Vec::new();
    for (i, ((t, st), &a)) in celdas.iter().zip(anchos).enumerate() {
        if i > 0 {
            v.push(Span::raw("  "));
        }
        let t = if t.chars().count() > a { t.chars().take(a.saturating_sub(1)).collect::<String>() + "…" } else { t.clone() };
        let t = if i < 2 { format!("{t:<a$}") } else { format!("{t:>a$}") };
        v.push(Span::styled(t, *st));
    }
    Line::from(v)
}

/// Los anchos de una lista en `w` columnas: cada una, lo de su cabecera o su celda más larga; la del nombre (la
/// segunda), lo que sobre (hasta lo que necesite).
fn anchos_tabla(cab: &[&str], filas: &[Vec<(String, Style)>], w: usize) -> Vec<usize> {
    let mut a: Vec<usize> = cab.iter().map(|c| c.chars().count()).collect();
    for f in filas {
        for (i, (t, _)) in f.iter().enumerate() {
            a[i] = a[i].max(t.chars().count());
        }
    }
    let resto: usize = a.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, x)| x + 2).sum();
    a[1] = a[1].min(w.saturating_sub(resto).max(8));
    a
}

/// La cabecera de una lista, en gris.
fn cabecera(cab: &[&str], anchos: &[usize]) -> Line<'static> {
    fila_tabla(&cab.iter().map(|c| (c.to_string(), tema::cabecera_tabla())).collect::<Vec<_>>(), anchos)
}

/// «20 nov 10:05».
fn cuando(c: &str) -> String {
    let dia = fmt::fecha_corta(c);
    let dia = dia.rsplit_once(' ').map_or(dia.clone(), |(dm, y)| if y == fmt::anio_actual().to_string() { dm.to_string() } else { dia.clone() });
    format!("{dia} a las {}", c.get(11..16).unwrap_or(""))
}

/* ================= 1. Este año ================= */

#[derive(Default)]
pub struct EsteAnio {
    conexion: Conexion,
    desde: usize,
    calculado: Calculado,
}

/// Lo calculado para lo que se ve, hasta que cambien los datos, el año o el ancho: la banda y las dos cifras de debajo
/// (sin cartera, ninguna) y las líneas de más abajo, que se desplazan.
#[derive(Default)]
struct Calculado {
    clave: Option<(u64, i32, u16)>,
    cabeza: Option<Cabeza>,
    lineas: Vec<Line<'static>>,
}

/// Lo de arriba: lo que te tocaría pagar (o te devolverían), en una banda con su fondo; y lo vendido y los cobros, con
/// puntos guía.
struct Cabeza {
    banda: Line<'static>,
    fondo: Color,
    fin: u16,
    guias: Vec<Line<'static>>,
}

impl Calculado {
    fn al_dia(&mut self, ctx: &Ctx, w: u16) {
        let clave = (ctx.generacion, ctx.anio, w);
        if self.clave == Some(clave) {
            return;
        }
        self.clave = Some(clave);
        let w = w as usize;
        match inf::anio(&ctx.datos, ctx.anio, &fmt::hoy()) {
            None => (self.cabeza, self.lineas) = (None, sin_broker(ctx, w)),
            Some(a) => (self.cabeza, self.lineas) = (Some(cabeza(&a, ctx.anio, w)), detalle(&a, ctx, w)),
        }
    }
}

impl Pantalla for EsteAnio {
    fn titulo(&self) -> &'static str {
        "Este año"
    }

    /// Arriba, cada bróker (cuándo se trajo, si falló); luego la banda y las dos cifras de debajo, y lo demás (por qué,
    /// las ventas, los cobros), que se mueve con ↑↓. Sin nada traído, cómo conectar.
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.calculado.al_dia(ctx, area.width);
        let mut arriba = estado(ctx, &self.conexion);
        match &self.calculado.cabeza {
            None => {
                if !arriba.is_empty() {
                    arriba.push(Line::from(""));
                }
                arriba.extend(self.calculado.lineas.iter().cloned());
                pintar(f, area, arriba, &mut self.desde);
            }
            Some(c) => {
                let fila = |y: u16, alto: u16| Rect { y, height: alto.min(area.bottom().saturating_sub(y)), ..area };
                let mut y = area.y;
                if !arriba.is_empty() {
                    let n = arriba.len() as u16;
                    f.render_widget(Paragraph::new(arriba), fila(y, n));
                    y += n + 1;
                }
                let r = fila(y, ALTO_BANDA);
                super::widgets::banda(f, Rect { width: (c.fin + 2).min(area.width), ..r }, c.fondo, c.banda.clone());
                y += ALTO_BANDA + 1;
                let n = c.guias.len() as u16;
                f.render_widget(Paragraph::new(c.guias.clone()), fila(y, n));
                y += n + 1;
                pintar(f, fila(y, area.bottom().saturating_sub(y)), self.calculado.lineas.clone(), &mut self.desde);
            }
        }
        self.conexion.dibujar(f, area);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if self.conexion.abierta() {
            self.conexion.tecla(k, ctx);
            return true;
        }
        match k.code {
            KeyCode::Char('b') => self.conexion.elegir(Para::Traer, ctx),
            KeyCode::Char('x') => self.conexion.elegir(Para::Desconectar, ctx),
            _ => return desplazar(&mut self.desde, k),
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.conexion.abierta() {
            return vec![("Enter", "seguir"), ("Esc", "cancelar")];
        }
        vec![("b", "traer de tu bróker"), ("x", "desconectar"), ("↑↓", "moverse")]
    }

    fn capturando(&self) -> bool {
        self.conexion.abierta()
    }

    fn al_entrar(&mut self, _ctx: &mut Ctx) {
        self.desde = 0;
    }

    fn atender(&mut self, ctx: &mut Ctx) -> bool {
        self.conexion.atender(ctx)
    }
}

/// Arriba, cada bróker conectado: cuándo se trajo, si falló; y por dónde va lo que se está trayendo.
fn estado(ctx: &Ctx, conexion: &Conexion) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    for c in brokers::CONECTORES {
        let activado = ctx.datos.inversiones.activado(c.id);
        let fuente = ctx.datos.carteras.get(c.id);
        if !activado && fuente.is_none() {
            continue;
        }
        let mut l = vec![Span::styled(c.nombre.to_string(), negrita())];
        match fuente {
            Some(fu) if fu.dato.is_some() => l.push(Span::styled(format!(" · traído el {}", cuando(&fu.cuando)), gris())),
            _ => l.push(Span::styled(" · aún sin traer nada (b)", gris())),
        }
        if !activado {
            l.push(Span::styled(" · desconectado", Style::new().fg(tema::AVISO)));
        }
        v.push(Line::from(l));
        if let Some(e) = fuente.and_then(|f| f.error.as_ref()) {
            v.push(Line::styled(format!("El último intento falló: {}", e.mensaje), Style::new().fg(tema::MAL)));
        }
    }
    if let Some(p) = &conexion.progreso {
        v.push(Line::styled(p.clone(), Style::new().fg(tema::acento())));
    }
    v
}

/// Sin nada traído: qué se puede ver y cómo; y, si hay Renta de ese año, lo que declaraste.
fn sin_broker(ctx: &Ctx, w: usize) -> Vec<Line<'static>> {
    let t = ancho_texto(w);
    let mut v = parrafo(&format!("Conecta tu bróker para ver, al momento, lo que has ganado o perdido con lo que has vendido en {}, tus dividendos e intereses y lo que te tocaría pagar por ello en tu Renta.", ctx.anio), t, Style::new());
    v.push(Line::from(""));
    let activado = brokers::CONECTORES.iter().any(|c| ctx.datos.inversiones.activado(c.id));
    let que = if activado { format!("traer lo de tu bróker ({})", nombres()) } else { format!("conectar con tu bróker ({})", nombres()) };
    v.push(Line::from(vec![Span::styled("b", negrita().fg(tema::acento())), Span::raw(format!("  {que}"))]));
    if !activado {
        v.extend(sangrado("Está desactivado: la app no habla con ningún bróker hasta que actives su conexión, y antes te dice qué hace y qué riesgos tiene. Tu usuario y tu contraseña se piden cada vez y no se guardan.", t, gris()));
    }
    if let Some(r) = crate::informe::renta_de(&ctx.datos, ctx.anio) {
        let d = Declarado { anio: ctx.anio, renta: r };
        v.push(Line::from(""));
        v.push(Line::from(""));
        v.push(titulo_bloque(format!("lo que declaraste en tu Renta de {}", ctx.anio)));
        v.extend(declarado(&d, w));
        v.push(Line::from(""));
        v.push(Line::styled("Año a año, en 2 Declarado.", gris()));
    }
    v
}

/// La banda (lo que te tocaría pagar en tu Renta, en AVISO, o lo que te devolverían, en BIEN) y, debajo, con puntos
/// guía, lo ganado o perdido con lo vendido y los dividendos e intereses, con lo que te retuvieron.
fn cabeza(a: &Anio, anio: i32, w: usize) -> Cabeza {
    // Las que cuentan algo (de una sin todo su coste, la pérdida de antes que libera).
    let completas = a.ventas.iter().filter(|v| v.completa() || v.integradas < -DIF).count();
    let gp = a.ganancias - a.perdidas;
    let ventas = match completas {
        0 => "sin ventas".to_string(),
        1 => "1 venta".to_string(),
        n => format!("{n} ventas"),
    };
    let p = a.cuenta.a_pagar;
    let (etiqueta, cifra, color, fondo) = if p < -DIF {
        (format!("te devolverían en tu Renta de {anio}"), eur(-p), tema::BIEN, tema::SUAVE_BIEN)
    } else if p > DIF {
        (format!("te tocaría pagar en tu Renta de {anio}"), eur(p), tema::AVISO, tema::SUAVE_AVISO)
    } else {
        (format!("te tocaría pagar en tu Renta de {anio}"), eur(p), tema::TINTA, tema::SUAVE_BIEN)
    };
    let vendido = if gp < -DIF { "has perdido con lo que has vendido" } else { "has ganado con lo que has vendido" };
    let cobros = eur(a.dividendos + a.intereses);
    let fin = fin_de(&[(&format!("  {etiqueta}"), &cifra), (vendido, &con_signo(gp)), ("dividendos e intereses", &cobros)], w);
    let st = negrita().fg(color);
    let hueco = (fin as usize).saturating_sub(2 + etiqueta.chars().count() + cifra.chars().count()).max(1);
    let banda = Line::from(vec![Span::raw("  "), Span::styled(etiqueta, st), Span::raw(" ".repeat(hueco)), Span::styled(cifra, st)]);
    let mut guias = guia_nota(vendido, con_signo(gp), estilo_resultado(gp), fin, &ventas, w);
    guias.extend(guia_nota("dividendos e intereses", cobros, Style::new(), fin, &format!("retenido {}", eur(a.retenido_origen + a.retenido_espana)), w));
    Cabeza { banda, fondo, fin, guias }
}

/// Por qué: la cuenta paso a paso (con puntos guía), las ventas, los cobros y lo que hay que mirar.
fn detalle(a: &Anio, ctx: &Ctx, w: usize) -> Vec<Line<'static>> {
    let t = ancho_texto(w);
    let c = &a.cuenta;
    let mut v: Vec<Line> = vec![titulo_bloque("por qué")];
    let hasta: Vec<String> = a.brokers.iter().map(|(b, h)| if ctx.anio == fmt::anio_actual() { format!("{b}, hasta el {}", fmt::fecha_corta(h)) } else { b.to_string() }).collect();
    v.extend(parrafo(&format!("Con lo de {}. Solo lo de tus brókeres conectados: si tienes más ahorro (otras cuentas, otros brókeres), súmalo.", hasta.join(" y ")), t, gris()));
    v.push(Line::from(""));
    let vendido = a.ganancias > DIF || a.perdidas > DIF;
    let cobrado = a.dividendos > DIF || a.intereses > DIF || a.bonos.abs() > DIF;
    if !vendido && !cobrado && c.compensado <= DIF && c.para_otros_anios <= DIF {
        v.extend(parrafo(&format!("En {} no has vendido nada ni has cobrado dividendos o intereses: nada que pagar por esto.", a.anio), t, Style::new()));
    } else {
        // La cuenta, como un recibo: todas las cifras acaban en la misma columna.
        let pagar = if c.a_pagar < -DIF { "te devolverían en tu Renta".to_string() } else { format!("te tocaría pagar en tu Renta de {}", a.anio) };
        let fin = fin_de(&[(&pagar, &eur(c.a_pagar.abs())), ("dividendos e intereses", &eur(c.capital)), ("base del ahorro", &eur(c.base))], w);
        let n = Style::new();
        if vendido {
            v.extend(guia_nota("lo que has vendido", eur(a.ganancias - a.perdidas), n, fin, &format!("{} de ganancias y {} de pérdidas", eur(a.ganancias), eur(a.perdidas)), w));
        }
        if cobrado {
            let mut nota = format!("dividendos {} · intereses {}", eur(a.dividendos), eur(a.intereses));
            if a.bonos.abs() > DIF {
                nota += &format!(" · lo de vender bonos {}", eur(a.bonos));
            }
            v.extend(guia_nota("dividendos e intereses", eur(c.capital), n, fin, &nota, w));
        }
        if c.compensado > DIF {
            v.extend(guia_nota("compensado", eur(c.compensado), n, fin, "un saldo negativo se compensa con el otro hasta el 25 % de este (art. 49 de la Ley del IRPF)", w));
        }
        v.extend(guia_nota("base del ahorro", eur(c.base), n, fin, "", w));
        let pct = |t: f64| format!("{}\u{a0}%", fmt::num(t).trim_end_matches(",00"));
        let tipos: Vec<String> = ahorro::tipos(a.anio).iter().map(|(hasta, t)| if hasta.is_finite() { format!("{} hasta {}", pct(*t), eur0(*hasta)) } else { format!("{} desde ahí", pct(*t)) }).collect();
        let escala = if a.anio <= ahorro::COMPROBADA { format!("la de {}", a.anio) } else { format!("la de {}: la de {} no la conoce aún la app", ahorro::COMPROBADA, a.anio) };
        v.extend(guia_nota("cuota", eur(c.cuota), n, fin, &format!("con la escala del ahorro ({escala}): {}", tipos.join(", ")), w));
        if c.doble_imposicion > DIF {
            v.extend(guia_nota("lo que te retuvieron fuera", menos(c.doble_imposicion), n, fin, "deducción por doble imposición (art. 80): como mucho lo que pagarías aquí por esa renta y el 15 % que suele permitir el convenio", w));
        }
        if c.retenido_espana > DIF {
            v.extend(guia_nota("lo que te retuvieron en España", menos(c.retenido_espana), n, fin, "pagos a cuenta", w));
        }
        // La raya de la suma, bajo las cifras; y el resultado, en negrita y en su color.
        v.push(Line::from(vec![Span::raw(" ".repeat((fin as usize).saturating_sub(11))), Span::styled("─".repeat(11), Style::new().fg(tema::PUNTOS))]));
        let st = if c.a_pagar < -DIF { Style::new().fg(tema::BIEN) } else if c.a_pagar > DIF { Style::new().fg(tema::AVISO) } else { Style::new() };
        v.push(guia(Span::styled(pagar, negrita().patch(st)), eur(c.a_pagar.abs()), st, fin));
        if c.para_otros_anios > DIF {
            v.push(Line::from(""));
            v.extend(parrafo(&format!("Quedan {} negativos que podrás compensar en los cuatro años siguientes.", eur(c.para_otros_anios)), t, gris()));
        }
    }
    v.push(Line::from(""));
    if a.pendientes > DIF {
        v.extend(parrafo(&format!("Hay {} de pérdidas que aún no cuentan porque recompraste (art. 33.5.f): contarán cuando vendas lo recomprado.", eur(a.pendientes)), t, gris()));
    }
    v.extend(parrafo("Es una estimación: supone que el mínimo personal ya lo cubren tus otras rentas y no cuenta pérdidas de años anteriores pendientes de compensar.", t, gris()));
    if a.ventas.iter().any(|x| x.op.divisa != "EUR" && !x.op.divisa.is_empty() && x.op.producto == crate::dominio::inversiones::Producto::Accion) {
        v.extend(parrafo("Con acciones en otra divisa: aquí cada compra y cada venta van en euros al cambio de su día. Hacienda (consultas V0775-21 y V0282-22) calcula la diferencia en la divisa y la pasa a euros al cambio del día de la venta; con el dólar moviéndose, puede salir otra cifra.", t, Style::new().fg(tema::AVISO)));
    }
    // Las ventas, en euros: lo que te dieron (sin comisiones), lo que te costó y lo que cuenta.
    if !a.ventas.is_empty() {
        v.push(Line::from(""));
        v.push(titulo_bloque("ventas"));
        let cab = ["fecha", "valor", "cantidad", "te dieron", "te costó", "cuenta"];
        let filas: Vec<Vec<(String, Style)>> = a
            .ventas
            .iter()
            .map(|x| {
                let (cuenta, st) = if x.completa() { (signo_num(x.computa()), estilo_resultado(x.computa())) } else { ("?".into(), Style::new().fg(tema::AVISO)) };
                vec![
                    (fecha_de(&x.op.fecha, a.anio), gris()),
                    (x.op.nombre.clone(), Style::new()),
                    (fmt::num(x.op.cantidad).trim_end_matches(",00").to_string(), Style::new()),
                    (fmt::num(x.transmision), Style::new()),
                    (fmt::num(x.adquisicion), Style::new()),
                    (cuenta, st.add_modifier(Modifier::BOLD)),
                ]
            })
            .collect();
        let anchos = anchos_tabla(&cab, &filas, w);
        v.push(cabecera(&cab, &anchos));
        let sangria = anchos[0] + 2;
        for (x, celdas) in a.ventas.iter().zip(&filas) {
            v.push(fila_tabla(celdas, &anchos));
            let mut notas = Vec::new();
            if x.op.divisa != "EUR" && !x.op.divisa.is_empty() && x.op.cambio > 0.0 {
                notas.push(format!("a {} {} cada uno, al cambio de ese día ({} {} por euro)", fmt::num(x.op.precio), x.op.divisa, fmt::num(x.op.cambio), x.op.divisa));
            }
            if x.op.comisiones > DIF {
                notas.push(format!("con {} de comisiones", eur(x.op.comisiones)));
            }
            if x.no_computa > DIF {
                notas.push(format!("{} de la pérdida no cuentan: recompraste el {}", eur(x.no_computa), fmt::fecha_corta(x.recompra.unwrap_or_default())));
            }
            if x.integradas < -DIF {
                notas.push(format!("cuentan {} de una pérdida de antes", eur(-x.integradas)));
            }
            if x.provisional {
                notas.push(format!("si recompras antes de {} meses desde la venta, la pérdida dejaría de contar", ahorro::meses_recompra(x.op.producto)));
            }
            if !notas.is_empty() {
                let texto = format!("{}.", primera_mayuscula(&notas.join("; ")));
                v.extend(parrafo(&texto, ancho_texto(w).saturating_sub(sangria), gris()).into_iter().map(|mut l| {
                    l.spans.insert(0, Span::raw(" ".repeat(sangria)));
                    l
                }));
            }
        }
    }
    // Los cobros: el bruto y lo retenido, en euros (o en su moneda, si no se sabe el cambio).
    if !a.cobros.is_empty() {
        v.push(Line::from(""));
        v.push(titulo_bloque("dividendos e intereses"));
        let cab = ["fecha", "de", "bruto", "ret. fuera", "ret. España"];
        let filas: Vec<Vec<(String, Style)>> = a
            .cobros
            .iter()
            .map(|x| {
                let importe = |y: f64| match x.en_euros(y) {
                    Some(e) if y.abs() > DIF => (fmt::num(e), Style::new()),
                    Some(_) => ("—".into(), gris()),
                    None => (format!("{} {}", fmt::num(y), x.divisa), Style::new()),
                };
                let de = if x.clase == Clase::Interes { format!("{} · intereses", x.nombre) } else { x.nombre.clone() };
                vec![(fecha_de(&x.fecha, a.anio), gris()), (de, Style::new()), importe(x.bruto), importe(x.retenido_origen), importe(x.retenido_espana)]
            })
            .collect();
        let anchos = anchos_tabla(&cab, &filas, w);
        v.push(cabecera(&cab, &anchos));
        v.extend(filas.iter().map(|f| fila_tabla(f, &anchos)));
    }
    if !a.avisos.is_empty() {
        v.push(Line::from(""));
        v.push(Line::styled("para mirar", Style::new().fg(tema::AVISO)));
        for x in &a.avisos {
            v.extend(parrafo(&format!("· {x}"), t, Style::new().fg(tema::AVISO)));
        }
    }
    v
}

/// «+598,00» / «−302,00» (sin «€»: en las listas, ya lo dice la cabecera de arriba).
fn signo_num(x: f64) -> String {
    if x > DIF { format!("+{}", fmt::num(x)) } else if x < -DIF { format!("−{}", fmt::num(-x)) } else { fmt::num(0.0) }
}

fn primera_mayuscula(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/* ================= 2. Declarado ================= */

#[derive(Default)]
pub struct Declarados {
    sel: usize,
}

/// Lo del ahorro de una Renta, con puntos guía: los intereses y dividendos, lo vendido, la base del ahorro, lo que
/// pagaste por ella y lo que ya te habían retenido; cada cifra con su nota en gris.
fn declarado(d: &Declarado, w: usize) -> Vec<Line<'static>> {
    let n = Style::new();
    let mut filas: Vec<(&str, String, String)> = Vec::new();
    match d.capital() {
        Some(c) if c.solo_total => filas.push(("rendimientos del capital", eur(c.neto), "intereses y dividendos, de los dos".into())),
        Some(c) if c.integros.abs() > DIF => {
            let mut t = format!("intereses {} · dividendos {}", eur(c.intereses), eur(c.dividendos));
            if c.otros.abs() > DIF {
                t += &format!(" · otros {}", eur(c.otros));
            }
            if c.gastos > DIF {
                t += &format!(" · menos {} de gastos", eur(c.gastos));
            }
            filas.push(("intereses y dividendos", eur(c.integros), t));
        }
        _ => {}
    }
    if let Some(g) = d.ganancias().filter(|g| g.ganancias > DIF || g.perdidas > DIF) {
        filas.push(("ganancias por lo que vendiste", eur(g.ganancias), String::new()));
        let cripto = g.cripto.as_ref().filter(|c| c.ganancias > DIF || c.perdidas > DIF).map(|c| format!("de criptomonedas: {} y {}", eur(c.ganancias), eur(c.perdidas))).unwrap_or_default();
        filas.push(("pérdidas por lo que vendiste", eur(g.perdidas), cripto));
    }
    if filas.is_empty() {
        return parrafo(&format!("En tu Renta de {} no hay intereses, dividendos ni ventas.", d.anio), ancho_texto(w), n);
    }
    if let Some(b) = d.base() {
        filas.push(("base del ahorro", eur(b), String::new()));
        match d.cuota() {
            Some(c) => filas.push(("pagaste por ella", eur(c), "la cuota del ahorro, estatal y autonómica".into())),
            None => filas.push(("pagaste por ella", "—".into(), "la cuota no se ha podido leer".into())),
        }
        if let Some(r) = d.retenido().filter(|r| *r > DIF) {
            filas.push(("ya te habían retenido", eur(r), "de lo que pagaste".into()));
        }
    }
    if let Some(x) = d.doble_imposicion() {
        filas.push(("doble imposición internacional", eur(x), "te la dedujiste".into()));
    }
    let fin = fin_de(&filas.iter().map(|(e, c, _)| (*e, c.as_str())).collect::<Vec<_>>(), w);
    let mut v: Vec<Line> = filas.iter().flat_map(|(e, c, nota)| guia_nota(*e, c.clone(), n, fin, nota, w)).collect();
    if d.renta.conjunta {
        v.push(Line::styled("Fue conjunta: son las cifras de la familia.", gris()));
    }
    v
}

impl Pantalla for Declarados {
    fn titulo(&self) -> &'static str {
        "Declarado"
    }

    /// Arriba, la lista de tus Rentas (la elegida, en el fondo suave de la sección); debajo, lo de la elegida con
    /// puntos guía.
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let v = inf::declarado(&ctx.datos);
        let w = area.width as usize;
        if v.is_empty() {
            let t = parrafo("Aún no hay ninguna Renta leída. Se traen con tu certificado (S) y aquí se ve, año a año, lo que declaraste de intereses, dividendos y ventas, y lo que pagaste por ello.", ancho_texto(w), gris());
            f.render_widget(Paragraph::new(t), area);
            return;
        }
        self.sel = self.sel.min(v.len() - 1);
        // La lista: las cifras en su columna, a la derecha y sin «€»; lo que no está en la Renta, «—».
        let cifra = |x: Option<f64>| x.filter(|x| x.abs() > DIF).map_or("—".to_string(), fmt::num);
        let filas: Vec<[String; 6]> = v
            .iter()
            .map(|d| {
                let g = d.ganancias();
                [d.anio.to_string(), cifra(d.capital().map(|c| c.integros)), cifra(g.map(|g| g.ganancias)), cifra(g.map(|g| g.perdidas)), cifra(d.base()), cifra(d.cuota())]
            })
            .collect();
        // Las cabeceras largas, si caben; si no, las cortas.
        let anchos_de = |cab: &[&str; 6]| -> Vec<u16> { (0..6).map(|i| filas.iter().map(|f| f[i].chars().count()).chain([cab[i].chars().count()]).max().unwrap_or(0) as u16).collect() };
        let mut cab = ["año", "intereses y dividendos", "ganancias", "pérdidas", "base del ahorro", "pagaste"];
        if anchos_de(&cab).iter().sum::<u16>() + 2 * 5 > area.width {
            cab = ["año", "int. y div.", "ganancias", "pérdidas", "base", "pagaste"];
        }
        let anchos = anchos_de(&cab);
        let rows: Vec<Row> = filas
            .iter()
            .zip(&v)
            .map(|(f, d)| {
                let st = if d.hay_algo() { Style::new() } else { gris() };
                Row::new(f.iter().enumerate().map(|(i, t)| if i == 0 { Cell::from(t.clone()) } else { Cell::from(derecha(t.clone())) })).style(st)
            })
            .collect();
        let alto_tabla = (v.len() as u16 + 1).min(area.height / 2).max(3);
        let tabla_r = Rect { height: alto_tabla.min(area.height), ..area };
        let cabecera = Row::new(cab.iter().enumerate().map(|(i, c)| if i == 0 { Cell::from(*c) } else { Cell::from(derecha(*c)) })).style(tema::cabecera_tabla());
        let tabla = Table::new(rows, anchos.iter().map(|a| Constraint::Length(*a))).header(cabecera).column_spacing(2).row_highlight_style(tema::seleccion());
        let mut estado = TableState::default().with_selected(Some(self.sel));
        f.render_stateful_widget(tabla, tabla_r, &mut estado);
        // Lo de la Renta elegida.
        let d = &v[self.sel];
        let mut l = vec![titulo_bloque(format!("lo que declaraste en tu Renta de {}", d.anio))];
        l.extend(declarado(d, w));
        if d.capital().is_none() || d.ganancias().is_none() {
            l.push(Line::from(""));
            l.extend(parrafo("Lo que falta no estaba en tu Renta o no se ha podido leer (sus sumas no cuadraban): la ficha de la Renta, en Resumen › Años, tiene todas sus casillas.", ancho_texto(w), gris()));
        }
        let y = tabla_r.bottom() + 1;
        if y < area.bottom() {
            f.render_widget(Paragraph::new(l), Rect { y, height: area.bottom() - y, ..area });
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        let n = inf::declarado(&ctx.datos).len();
        match k.code {
            KeyCode::Down if self.sel + 1 < n => self.sel += 1,
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = n.saturating_sub(1),
            KeyCode::Down => {}
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "otro año")]
    }

    fn usa_anio(&self) -> bool {
        false
    }
}

/* ================= 3. Modelo 720 ================= */

#[derive(Default)]
pub struct Fuera {
    desde: usize,
}

/// El nombre de un país por su código («NL» → «Países Bajos»).
fn pais(codigo: &str) -> String {
    let n = match codigo {
        "DE" => "Alemania",
        "NL" => "Países Bajos",
        "IE" => "Irlanda",
        "LU" => "Luxemburgo",
        "FR" => "Francia",
        "BE" => "Bélgica",
        "GB" => "Reino Unido",
        "US" => "Estados Unidos",
        "PT" => "Portugal",
        "IT" => "Italia",
        "CH" => "Suiza",
        "LT" => "Lituania",
        "ES" => "España",
        c => c,
    };
    n.to_string()
}

/// Si te toca presentar un bloque, en un sello («▕ TE TOCA EL 720 ▏») y, detrás, por qué; con sangría.
fn toca(t: &Toca, modelo: &str, w: usize) -> Vec<Line<'static>> {
    let (s, por) = match t {
        Toca::Si(por) => (sello(&format!("TE TOCA EL {modelo}"), tema::AVISO, tema::SUAVE_AVISO), por.clone()),
        Toca::No(por) => (sello("NO TE TOCA", tema::BIEN, tema::SUAVE_BIEN), format!("por esto: {por}")),
        Toca::NoSe(por) => (sello(&format!("NO SÉ SI TE TOCA EL {modelo}"), tema::AVISO, tema::SUAVE_AVISO), por.clone()),
    };
    let mut l = Line::from(Span::raw("   "));
    l.spans.extend(s.spans);
    let libre = w.saturating_sub(l.width() + 1);
    let mut trozos = partir(&format!("{por}."), libre.max(20)).into_iter();
    if let Some(p) = trozos.next() {
        l.push_span(Span::styled(format!(" {p}"), gris()));
    }
    let mut v = vec![l];
    let resto: Vec<String> = trozos.collect();
    if !resto.is_empty() {
        v.extend(sangrado(&resto.join(" "), w, gris()));
    }
    v
}

impl Pantalla for Fuera {
    fn titulo(&self) -> &'static str {
        "Modelo 720"
    }

    /// Lo que tenías fuera a 31 de diciembre, bloque a bloque con puntos guía y, debajo de cada uno, si te toca
    /// presentarlo (en un sello) y si consta presentado; al final, en gris, qué es y cómo se cuenta.
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let w = area.width as usize;
        let t = ancho_texto(w);
        let anio = ctx.anio;
        let (desde, hasta) = modelo_720::plazo(anio);
        let que_es = "El 720 es la declaración de lo que tienes fuera de España (cuentas, valores, fondos, seguros, inmuebles), y el 721, la de criptomonedas guardadas fuera. Solo informan: no se paga nada por presentarlos.";
        let mut v: Vec<Line> = Vec::new();
        match inf::fuera(&ctx.datos, anio) {
            None => {
                v.extend(parrafo(que_es, t, gris()));
                v.push(Line::from(""));
                let texto = if inf::hay_cartera(&ctx.datos) {
                    format!("No hay nada traído de tus brókeres de {anio}.")
                } else {
                    "Conecta tu bróker (en 1 Este año, con b) y aquí verás, cada año, lo que tenías fuera a 31 de diciembre y si te toca presentarlo.".to_string()
                };
                v.extend(parrafo(&texto, t, Style::new()));
                for m in ["720", "721"] {
                    if let Some(x) = crate::hacienda::declaraciones(&ctx.datos, anio).into_iter().filter(|x| x.modelo == m).max_by(|a, b| a.fecha.cmp(&b.fecha)) {
                        v.extend(parrafo(&format!("✓ Consta presentado tu {m} de {anio} (el {}).", fmt::fecha_corta(&x.fecha)), t, Style::new().fg(tema::BIEN)));
                    }
                }
            }
            Some(fu) => {
                let titulo = if fu.al_cierre() { format!("Lo que tenías fuera a 31 de diciembre de {anio}") } else { format!("Lo que tienes fuera hoy ({}): si te toca el de {anio}, se sabrá el 31 de diciembre", fmt::fecha_corta(&fu.fecha)) };
                v.extend(parrafo(&titulo, t, negrita()));
                v.push(Line::from(""));
                let nombres: Vec<String> = fu.bloques.iter().map(|b| format!("{} ({})", b.bloque.nombre(), b.bloque.modelo())).collect();
                let cifras: Vec<String> = fu.bloques.iter().map(|b| eur0(b.euros)).collect();
                let fin = fin_de(&nombres.iter().zip(&cifras).map(|(n, c)| (n.as_str(), c.as_str())).collect::<Vec<_>>(), w).max(60.min(w as u16));
                for ((b, nombre), cifra) in fu.bloques.iter().zip(&nombres).zip(&cifras) {
                    if b.euros.abs() <= DIF && b.presentado.is_none() && b.ultimo.is_none() {
                        v.push(guia(Span::styled(nombre.clone(), gris()), "—", gris(), fin));
                        continue;
                    }
                    v.push(guia(nombre.clone(), cifra.clone(), Style::new(), fin));
                    for (p, d, e) in &b.por_deposito {
                        // El país y el depositario, con su cifra en la misma columna; si no caben, el depositario
                        // debajo.
                        let cifra = eur0(*e);
                        let libre = (fin as usize).saturating_sub(cifra.chars().count() + 5);
                        let junto = format!("   {} · {d}", pais(p));
                        let cabe = junto.chars().count() <= libre;
                        let mut l = guia(Span::styled(if cabe { junto } else { format!("   {}", pais(p)) }, gris()), cifra, gris(), fin);
                        if let Some(s) = l.spans.last_mut() {
                            s.style = gris();
                        }
                        v.push(l);
                        if !cabe {
                            v.extend(sangrado(d, t, gris()).into_iter().map(|mut l| {
                                l.spans.insert(0, Span::raw("   "));
                                l
                            }));
                        }
                    }
                    if let Some(x) = &b.toca {
                        v.extend(toca(x, b.bloque.modelo(), t));
                    }
                    match &b.presentado {
                        Some(x) => v.extend(sangrado(&format!("✓ Consta presentado tu {} de {anio} (el {}).", b.bloque.modelo(), fmt::fecha_corta(&x.fecha)), t, Style::new().fg(tema::BIEN))),
                        None if fu.al_cierre() && matches!(b.toca, Some(Toca::Si(_) | Toca::NoSe(_))) => v.extend(sangrado(&format!("No consta presentado en lo consultado a Hacienda. Se presenta del {} al {}.", fmt::fecha_corta(&desde), fmt::fecha_corta(&hasta)), t, Style::new().fg(tema::AVISO))),
                        None => {}
                    }
                    v.push(Line::from(""));
                }
                v.push(Line::from(""));
                v.push(titulo_bloque("qué es y cómo se cuenta"));
                v.extend(parrafo(que_es, t, gris()));
                v.extend(parrafo("Cuenta a partir de 50.000 € en cada bloque, cada uno por su lado; si ya lo presentaste, solo vuelve a tocar si un bloque sube más de 20.000 € desde la última vez (aquí, comparado con lo que tenías en tus brókeres ese 31 de diciembre: aproximado) o si has dejado de tener algo de lo declarado.", t, gris()));
                v.extend(parrafo("Solo con lo de tus brókeres conectados: si tienes más fuera (otras cuentas, inmuebles), súmalo. Las cuentas cuentan también por su saldo medio del último trimestre, que la app no calcula. Los valores, con lo que valían según el bróker el 31 de diciembre: los fondos van por su valor liquidativo de ese día y las acciones que cotizan en España, por su media del último trimestre.", t, gris()));
                for c in brokers::CONECTORES.iter().filter(|c| ctx.datos.carteras.get(c.id).is_some_and(|f| f.dato.is_some())) {
                    v.extend(parrafo(c.deposito, t, gris()));
                }
            }
        }
        pintar(f, area, v, &mut self.desde);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if k.code == KeyCode::Char('b') {
            ctx.info("Lo de tu bróker se trae en 1 Este año, con b.");
            return true;
        }
        desplazar(&mut self.desde, k)
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "moverse")]
    }

    fn al_entrar(&mut self, _ctx: &mut Ctx) {
        self.desde = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::prueba::{app_con, cargar, caso, ir, pantalla, teclas};
    use crate::dominio::fmt::anio_actual;
    use crate::dominio::fuente::Fuente;
    use crate::dominio::inversiones::{Bien, Cartera, Clase, Cobro, Operacion, Posicion};
    use crate::dominio::Datos;

    /// Una cartera inventada de este año: una venta con ganancia, otra con pérdida, un dividendo de fuera y lo que
    /// había el 31 de diciembre pasado.
    fn con_cartera() -> Datos {
        let hoy = anio_actual();
        let y = |a: i32, md: &str| format!("{}-{md}", hoy + a);
        let op = |fecha: String, isin: &str, compra: bool, cantidad: f64, euros: f64| Operacion { fecha, isin: isin.into(), nombre: format!("Valor Inventado {isin}"), compra, cantidad, precio: euros / cantidad, divisa: "EUR".into(), cambio: 1.0, euros, comisiones: 1.0, ..Default::default() };
        let c = Cartera {
            broker: "DEGIRO".into(),
            hasta: crate::dominio::fmt::hoy(),
            operaciones: vec![
                op(y(-2, "01-10T10:00:00"), "XX0000000001", true, 10.0, 1000.0),
                op(y(0, "01-15T10:00:00"), "XX0000000001", false, 10.0, 1600.0),
                op(y(-2, "01-10T10:00:00"), "XX0000000002", true, 10.0, 1000.0),
                op(y(0, "01-20T10:00:00"), "XX0000000002", false, 10.0, 700.0),
            ],
            cobros: vec![Cobro { fecha: y(0, "01-25"), clase: Clase::Dividendo, isin: "US0000000003".into(), nombre: "Empresa Inventada".into(), divisa: "USD".into(), bruto: 100.0, retenido_origen: 15.0, retenido_espana: 0.0, cambio: Some(1.25) }],
            posiciones: vec![Posicion { fecha: y(-1, "12-31"), bien: Bien::Valores, isin: "XX0000000001".into(), nombre: "Valor".into(), cantidad: 10.0, euros: 60000.0, pais: "NL".into(), depositario: "Stichting DEGIRO (vehículo de DEGIRO en los Países Bajos)".into() }],
            cierres: vec![y(-1, "12-31")],
            avisos: vec!["Un aviso de prueba.".into()],
            ..Default::default()
        };
        let mut d = cargar(&caso("a-de-todo"));
        d.inversiones.conectores.insert("degiro".into(), true);
        d.carteras.insert("degiro".into(), Fuente::nueva("DEGIRO", "2026-11-20T10:05:00", Some(c), None));
        d
    }

    #[test]
    fn este_anio_sin_broker() {
        let dir = caso("a-de-todo");
        let mut a = app_con(cargar(&dir), &dir);
        teclas(&mut a, "i");
        assert_eq!(a.titulo_activa(), "Este año");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("conectar con tu bróker (DEGIRO)") && p.contains("Está desactivado"), "{p}");
        // b pide activarlo, con el aviso del contrato; Esc no activa nada.
        teclas(&mut a, "b");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Conectar con DEGIRO") && p.contains("art. 5.6") && a.pantalla().capturando(), "{p}");
        teclas(&mut a, "<esc>");
        assert!(!a.ctx.datos.inversiones.activado("degiro") && !a.pantalla().capturando());
    }

    /// Activar, escribir el usuario, la contraseña (que no se ve) y el código (que no navega).
    #[test]
    fn conectar_sin_que_se_vea_la_contrasena() {
        let dir = caso("a-de-todo");
        let mut a = app_con(cargar(&dir), &dir);
        teclas(&mut a, "ib<enter>");
        assert!(a.ctx.datos.inversiones.activado("degiro"), "activado");
        teclas(&mut a, "ana<tab>secreta9<tab>123456");
        assert_eq!(a.titulo_activa(), "Este año", "las cifras se escriben, no navegan");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Entrar en DEGIRO") && p.contains("ana") && p.contains("••••••••") && !p.contains("secreta9") && p.contains("123456"), "{p}");
        teclas(&mut a, "<enter>");
        let aviso = a.ctx.aviso.as_ref().map(|x| x.0.clone()).unwrap_or_default();
        assert!(aviso.contains("(prueba) traería"), "en las pruebas no sale a la red: {aviso}");
    }

    #[test]
    fn este_anio_con_cartera() {
        let d = con_cartera();
        let dir = caso("a-de-todo");
        let mut a = app_con(d, &dir);
        ir(&mut a, "Este año");
        let p = pantalla(&mut a, 140, 40);
        assert!(p.contains("DEGIRO · traído el") && p.contains("has ganado con lo que has vendido") && p.contains("+296,00 €") && p.contains("te tocaría pagar en tu Renta de"), "{p}");
        assert!(p.contains("te dieron") && p.contains("Valor Inventado"), "la lista de las ventas:\n{p}");
        teclas(&mut a, "<end>");
        let p = pantalla(&mut a, 140, 40);
        assert!(p.contains("Un aviso de prueba") && p.contains("Empresa Inventada"), "{p}");
        // En pequeño, no se rompe.
        assert!(!pantalla(&mut a, 80, 24).trim().is_empty());
    }

    #[test]
    fn declarado_y_720() {
        let d = con_cartera();
        let dir = caso("a-de-todo");
        let mut a = app_con(d, &dir);
        teclas(&mut a, "i2");
        assert_eq!(a.titulo_activa(), "Declarado");
        assert!(!pantalla(&mut a, 120, 36).trim().is_empty());
        teclas(&mut a, "3-");
        assert_eq!(a.titulo_activa(), "Modelo 720");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("a 31 de diciembre") && p.contains("60.000 €") && p.contains("▕ TE TOCA EL 720 ▏ pasa de 50.000 €") && p.contains("Países Bajos"), "{p}");
    }
}

