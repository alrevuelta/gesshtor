//! Modelos: dos secciones, lo que hay que presentar en los próximos 4 meses (y lo que se haya pasado de plazo), cada
//! modelo con su fecha y los días que quedan, y lo ya presentado frente a lo que sale con tus datos. Con Enter, un modelo
//! se despliega en su sitio con sus casillas y nada más: en gris las que puede traer Hacienda o el formulario
//! (`hacienda::rellenar`); las que son una operación con otras no salen.
//!
//! Lo de presentar va por la fecha de hoy, no por el año elegido: en enero, lo primero es el 4T del año pasado. Lo
//! presentado es del año elegido.
use super::declaraciones::{codigo, nombre_modelo};
use super::impuestos::{a_medida, ajustar};
use super::impuestos_casillas::{self as casillas, Clase, Grupo};
use super::impuestos_trimestre::{self as trimestre, Caso, Estado};
use crate::dominio::fmt::{self, DIF};
use crate::dominio::{Casillas, Datos, Valor, PRIMER_ANIO};
use crate::hacienda::modelo::Memo;
use crate::hacienda::modelo_720::{self, Toca};
use crate::hacienda::rellenar::{uso, Uso};
use crate::hacienda::{self, modelo_130, modelo_303, modelo_390, Modelo, Trimestral};
use crate::informe::{autonomo, inversiones};
use crate::tui::texto::lineas as partir;
use crate::tui::widgets::{tema, titulo_bloque};
use crate::tui::Ctx;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Cuántos meses por delante se ven: lo que abre su plazo de aquí a entonces.
const MESES: u32 = 4;
/// Lo más ancho de la lista: en un terminal enorme no se estira hasta el otro lado (como los formularios).
const ANCHO: u16 = 84;
/// Las columnas de una fila: la fecha, los días y lo que sale con su etiqueta.
const C_FECHA: usize = 12;
const C_DIAS: usize = 16;
const C_IMPORTE: usize = 24;
/// Lo desplegado va bajo el nombre del modelo.
const SANGRIA: usize = 7;

/// De qué modelo es una fila, para abrir sus casillas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cual {
    Trimestre(Trimestral, i32, u32),
    Resumen390(i32),
    /// Sin casillas en la app: el 349, el 347, el 720, el 721 y la Renta.
    Otro,
}

/// Algo por presentar.
#[derive(Debug, Clone)]
pub struct Pendiente {
    pub cual: Cual,
    pub codigo: String,
    /// «4T 2026», «2026».
    pub periodo: String,
    /// Cuándo se abre el plazo (None si Hacienda aún no lo ha publicado: la Renta).
    pub abre: Option<String>,
    pub cierra: String,
    /// Lo que sale con tus datos.
    pub resultado: Option<f64>,
    /// El periodo aún no ha acabado: lo que sale puede cambiar.
    pub en_curso: bool,
    /// El periodo aún no ha empezado: todavía no hay casillas.
    pub futuro: bool,
    /// Por qué toca, en los que no llevan casillas.
    pub nota: String,
}

/// Algo ya presentado.
#[derive(Debug, Clone)]
pub struct Hecha {
    pub cual: Cual,
    pub codigo: String,
    /// «3T», «anual».
    pub periodo: String,
    pub anio: i32,
    /// ISO; vacía si solo se tiene el PDF.
    pub fecha: String,
    pub presentado: Option<f64>,
    /// Lo que sale con tus datos (si hay facturas o gastos de ese periodo).
    pub tuyo: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Lista {
    pub pendientes: Vec<Pendiente>,
    pub hechas: Vec<Hecha>,
    /// El año de lo presentado que se ve.
    pub anio_hechas: i32,
    /// Trimestres en los que no toca el 130, con el porqué.
    pub sin130: Vec<(i32, u32, String)>,
}

impl Lista {
    fn len(&self) -> usize {
        self.pendientes.len() + self.hechas.len()
    }
    /// De qué modelo es la fila `i` (primero las pendientes y luego las presentadas).
    fn cual(&self, i: usize) -> Cual {
        match self.pendientes.get(i) {
            Some(p) => p.cual,
            None => self.hechas.get(i - self.pendientes.len()).map_or(Cual::Otro, |h| h.cual),
        }
    }
}

/* ================= qué hay que presentar ================= */

/// ¿Consta presentado un modelo que la app no lee (349, 347, 720…) en lo consultado a Hacienda?
fn consta_otro(d: &Datos, anio: i32, periodo: &str, modelo: &str) -> bool {
    hacienda::declaraciones(d, anio).iter().any(|x| codigo(&x.modelo) == modelo && x.periodo == periodo)
}

/// El orden dentro de un mismo plazo.
fn orden(c: &str) -> usize {
    ["303", "130", "390", "349", "347", "720", "721", "100"].iter().position(|x| *x == c).unwrap_or(9)
}

/// El 303 y el 130 de un trimestre, si hay que presentarlos, y por qué no el 130 si te libras.
fn del_trimestre(d: &Datos, hoy: &str, p: &hacienda::Plazo, con_importes: bool, memo: &mut Memo, v: &mut Vec<Pendiente>, sin130: &mut Vec<(i32, u32, String)>) {
    let (a, q) = (p.anio, p.q);
    for m in Trimestral::TODOS {
        let e = trimestre::estado_modelo(d, hoy, a, q, m);
        let futuro = e == Estado::Futuro;
        // Lo de un trimestre que aún no ha empezado se mira igual: solo se llega a ver el del año que viene en diciembre,
        // con el año casi entero, y así no sale un 130 que el 1 de enero desaparecería.
        let libre = m == Trimestral::M130 && (e == Estado::NoToca("exento") || (futuro && modelo_130::se_libra(d, a, q)));
        if libre {
            let ex = modelo_130::exento(d, a, q);
            let t = if d.fiscal.exento130 == "si" { "lo has indicado tú".to_string() } else { format!("el {} de tus ingresos lleva retención", fmt::pct(ex.pct.round())) };
            sin130.push((a, q, t));
            continue;
        }
        if !futuro && !matches!(e, Estado::EnCurso | Estado::Proximo | Estado::EnPlazo(_) | Estado::Vencido) {
            continue;
        }
        let r = (!futuro && con_importes).then(|| m.resultado_de(&m.calcular(d, a, q, memo)));
        v.push(Pendiente {
            cual: Cual::Trimestre(m, a, q),
            codigo: m.modelo().codigo().into(),
            periodo: format!("{q}T {a}"),
            abre: Some(p.ini.clone()),
            cierra: p.fin.clone(),
            resultado: r,
            en_curso: e == Estado::EnCurso,
            futuro,
            nota: String::new(),
        });
    }
}

/// El 390 de un año, si hay actividad y no consta presentado.
fn del_390(d: &Datos, hoy: &str, a: i32, v: &mut Vec<Pendiente>) {
    let (ini, fin) = (format!("{a}-01-01"), format!("{a}-12-31"));
    if !trimestre::hay_actividad(d, &ini, &fin) || hacienda::de_baja(d, &ini, &fin) {
        return;
    }
    let p = trimestre::plazo(a, 4);
    let pres = trimestre::presentado(d, a, 0, Modelo::M390);
    let e = Caso { hoy, ini: &ini, fin: &fin, plazo_ini: &p.ini, plazo_fin: &p.fin, presentado: pres.as_deref(), alta: d.fechas_actividad().0, baja: d.fechas_actividad().1, exento: false, actividad: true }.estado();
    if matches!(e, Estado::EnCurso | Estado::Proximo | Estado::EnPlazo(_) | Estado::Vencido) {
        v.push(Pendiente {
            cual: Cual::Resumen390(a),
            codigo: "390".into(),
            periodo: a.to_string(),
            abre: Some(p.ini),
            cierra: p.fin,
            resultado: None,
            en_curso: e == Estado::EnCurso,
            futuro: false,
            nota: String::new(),
        });
    }
}

/// Uno de los que no llevan casillas en la app.
fn otro(codigo_: &str, periodo: String, abre: Option<String>, cierra: String, nota: String) -> Pendiente {
    Pendiente { cual: Cual::Otro, codigo: codigo_.into(), periodo, abre, cierra, resultado: None, en_curso: false, futuro: false, nota }
}

/// ¿Se busca lo que falte de este año? Este y el anterior, siempre; los de antes, solo si se consultó a Hacienda: si
/// no, la app no sabe si se presentó y llenaría la lista de falsos vencidos.
fn se_mira(d: &Datos, a: i32, y: i32) -> bool {
    a >= y - 1 || hacienda::consultado(d, a).is_some()
}

/// El 349, el 347, el 720, el 721 y la Renta, si tocan. El 349 y el 347 pasados de plazo solo salen si se consultó ese
/// año a Hacienda y no están.
fn los_demas(d: &Datos, hoy: &str, y: i32, v: &mut Vec<Pendiente>) {
    for a in (PRIMER_ANIO..=y).filter(|&a| se_mira(d, a, y)) {
        let a_tiempo = |fin: &str| hoy <= fin || hacienda::consultado(d, a).is_some();
        let con_ue = autonomo::trimestres_349(d, a);
        for p in hacienda::plazos_trimestres(a).into_iter().filter(|p| con_ue.contains(&p.q)) {
            let empezado = hoy >= fmt::periodo(a, p.q).0.as_str();
            if empezado && a_tiempo(&p.fin) && !consta_otro(d, a, &format!("{}T", p.q), "349") {
                v.push(otro("349", format!("{}T {a}", p.q), Some(p.ini), p.fin, "Tienes operaciones con empresas de la UE en este trimestre.".into()));
            }
        }
        let (ini, fin) = hacienda::plazo_347(a);
        let (nc, np) = autonomo::terceros_347(d, a);
        if nc + np > 0 && a_tiempo(&fin) && !consta_otro(d, a, "0A", "347") {
            let cuantos = |n: usize, uno: &str, varios: &str| if n == 1 { format!("1 {uno}") } else { format!("{n} {varios}") };
            let quienes: Vec<String> = [(nc, "cliente", "clientes"), (np, "proveedor", "proveedores")].into_iter().filter(|x| x.0 > 0).map(|(n, uno, varios)| cuantos(n, uno, varios)).collect();
            let t = format!("Con {} de España pasas de 3.005,06 € en el año, IVA incluido.", quienes.join(" y "));
            v.push(otro("347", a.to_string(), Some(ini), fin, t));
        }
        // La Renta: su apertura no se da por sabida.
        if let (Estado::EnCurso | Estado::Proximo | Estado::EnPlazo(_) | Estado::Vencido, _, fin) = trimestre::estado_renta(d, hoy, a) {
            v.push(otro("100", a.to_string(), None, fin, "Se prepara en Resumen › Años.".into()));
        }
    }
    let a = y - 1;
    let (ini, fin) = modelo_720::plazo(a);
    if hoy >= ini.as_str() && hoy <= fin.as_str()
        && let Some(f) = inversiones::fuera(d, a)
    {
        for m in ["720", "721"] {
            let toca: Vec<&str> = f.bloques.iter().filter(|b| b.bloque.modelo() == m && b.presentado.is_none() && matches!(b.toca, Some(Toca::Si(_)))).map(|b| b.bloque.nombre()).collect();
            if !toca.is_empty() {
                v.push(otro(m, a.to_string(), Some(ini.clone()), fin.clone(), format!("Te toca por: {}.", toca.join(", "))));
            }
        }
    }
}

/// Lo presentado de un año, de lo último a lo primero.
fn presentado_de(d: &Datos, a: i32, memo: &mut Memo) -> Vec<Hecha> {
    let mut v = Vec::new();
    for q in 1..=4 {
        for m in Trimestral::TODOS {
            let Some(fecha) = trimestre::presentado(d, a, q, m.modelo()) else { continue };
            let presentado = trimestre::presentadas(d, a, q, m.modelo()).map(|c| m.resultado_de(&c.0));
            let hay = autonomo::hay_datos(d, &m.desde(a, q), &fmt::periodo(a, q).1);
            let tuyo = hay.then(|| m.resultado_de(&m.calcular(d, a, q, memo)));
            v.push(Hecha { cual: Cual::Trimestre(m, a, q), codigo: m.modelo().codigo().into(), periodo: format!("{q}T"), anio: a, fecha, presentado, tuyo });
        }
    }
    if let Some(fecha) = trimestre::presentado(d, a, 0, Modelo::M390) {
        v.push(Hecha { cual: Cual::Resumen390(a), codigo: "390".into(), periodo: "anual".into(), anio: a, fecha, presentado: None, tuyo: None });
    }
    // Lo demás que trajo la consulta a Hacienda (el último de cada modelo y periodo); lo censal no es un impuesto.
    let mut otros: Vec<hacienda::Decl> = Vec::new();
    for x in hacienda::declaraciones(d, a) {
        if matches!(codigo(&x.modelo), "303" | "130" | "390" | "036" | "037") {
            continue;
        }
        match otros.iter_mut().find(|o| o.modelo == x.modelo && o.periodo == x.periodo) {
            Some(o) if o.fecha < x.fecha => *o = x,
            Some(_) => {}
            None => otros.push(x),
        }
    }
    for x in otros {
        let periodo = if x.trimestre().is_some() { x.periodo.clone() } else if x.periodo == "0A" { "anual".into() } else { x.periodo.clone() };
        v.push(Hecha { cual: Cual::Otro, codigo: codigo(&x.modelo).to_string(), periodo, anio: a, fecha: x.fecha.chars().take(10).collect(), presentado: None, tuyo: None });
    }
    v.sort_by(|a, b| b.fecha.cmp(&a.fecha));
    v
}

/// El último día que se mira: dentro de `MESES` meses.
fn limite(hoy: &str) -> String {
    fmt::fecha(hoy).and_then(|d| d.checked_add_months(chrono::Months::new(MESES))).map_or_else(|| hoy.to_string(), |d| d.format("%Y-%m-%d").to_string())
}

/// Lo que hay por presentar a fecha de `hoy`: lo pasado de plazo, de cualquier año que se mire (`se_mira`), y lo que
/// viene hasta `MESES` meses por delante (por cuándo abre su plazo; la Renta, cuya apertura no se da por sabida, por
/// cuándo acaba). También los trimestres en los que no toca el 130.
fn por_presentar(d: &Datos, hoy: &str, con_importes: bool, memo: &mut Memo) -> (Vec<Pendiente>, Vec<(i32, u32, String)>) {
    let y = fmt::anio_de(hoy);
    let hasta = limite(hoy);
    let (mut v, mut sin130) = (Vec::new(), Vec::new());
    for a in (PRIMER_ANIO..=y + 1).filter(|&a| se_mira(d, a, y)) {
        for p in hacienda::plazos_trimestres(a).iter().filter(|p| p.ini <= hasta) {
            del_trimestre(d, hoy, p, con_importes, memo, &mut v, &mut sin130);
        }
        if a <= y {
            del_390(d, hoy, a, &mut v);
        }
    }
    los_demas(d, hoy, y, &mut v);
    v.retain(|p| p.abre.as_deref().unwrap_or(&p.cierra) <= hasta.as_str());
    v.sort_by(|a, b| (&a.cierra, &a.abre, orden(&a.codigo)).cmp(&(&b.cierra, &b.abre, orden(&b.codigo))));
    (v, sin130)
}

/// Cuántos modelos hay fuera de plazo y sin presentar: lo que la app avisa en rojo.
pub fn vencidos(d: &Datos, hoy: &str) -> usize {
    por_presentar(d, hoy, false, &mut Memo::default()).0.iter().filter(|p| p.cierra.as_str() < hoy).count()
}

/// Lo que hay por presentar y lo presentado de `anio` (si es el año en curso y aún no hay nada, lo del anterior: en
/// enero, el año nuevo estaría vacío).
pub fn lista(d: &Datos, hoy: &str, anio: i32, memo: &mut Memo) -> Lista {
    let y = fmt::anio_de(hoy);
    let (v, sin130) = por_presentar(d, hoy, true, memo);
    let (mut anio_hechas, mut hechas) = (anio, presentado_de(d, anio, memo));
    if hechas.is_empty() && anio == y {
        let antes = presentado_de(d, y - 1, memo);
        if !antes.is_empty() {
            (anio_hechas, hechas) = (y - 1, antes);
        }
    }
    Lista { pendientes: v, hechas, anio_hechas, sin130 }
}

/* ================= fechas, en palabras ================= */

fn dias(n: i64) -> String {
    if n == 1 { "1 día".into() } else { format!("{n} días") }
}

/// Una fecha, con su año solo si no es el de hoy: «20 oct», «1 feb 2027».
fn dia(f: &str, hoy: &str) -> String {
    if fmt::anio_de(f) == fmt::anio_de(hoy) { trimestre::dia_mes(f) } else { fmt::fecha_corta(f) }
}

/// Cuánto queda hasta el último día de un modelo y con qué estilo: «(en 71 días)», «(mañana)», «(hoy)», «(hace 33 días)».
pub fn en_dias(hoy: &str, cierra: &str) -> (String, Style) {
    let n = fmt::dias_entre(hoy, cierra);
    let aviso = Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD);
    match n {
        ..0 => (format!("(hace {})", dias(-n)), Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD)),
        0 => ("(hoy)".into(), aviso),
        1 => ("(mañana)".into(), aviso),
        2..=7 => (format!("(en {n} días)"), aviso),
        8..=30 => (format!("(en {n} días)"), Style::new().fg(tema::acento())),
        _ => (format!("(en {n} días)"), Style::new()),
    }
}

/// Lo que sale: qué es, la cifra y su estilo. Un 303 negativo se compensa en los siguientes; solo en el 4T se puede
/// pedir que lo devuelvan, y es a elegir: por eso ahí es «a tu favor».
fn que_sale(cual: Cual, r: f64) -> (&'static str, String, Style) {
    if r > DIF {
        return ("a ingresar", fmt::eur(r), Style::new().fg(tema::acento()).add_modifier(Modifier::BOLD));
    }
    if r >= -DIF {
        return ("sin importe", fmt::eur(0.0), tema::apagado());
    }
    match cual {
        Cual::Trimestre(Trimestral::M303, _, q) => (if q == 4 { "a tu favor" } else { "a compensar" }, fmt::eur(-r), Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD)),
        _ => ("negativa", fmt::eur(0.0), tema::apagado()),
    }
}

/// La cifra de lo presentado, en una columna: lo que queda a tu favor en el 303, con su signo; el 130 negativo es 0.
fn cifra_con_signo(cual: Cual, r: f64) -> String {
    match cual {
        Cual::Trimestre(Trimestral::M303, ..) if r < -DIF => format!("-{}", fmt::eur(-r)),
        _ => fmt::eur(r.max(0.0)),
    }
}

/* ================= la lista ================= */

/// Las columnas de una fila, en un ancho: la fecha y lo de la derecha (con su etiqueta, si cabe).
#[derive(Clone, Copy)]
struct Columnas {
    fecha: usize,
    importe: usize,
}

impl Columnas {
    fn de(ancho: usize) -> Columnas {
        Columnas { fecha: if ancho >= 62 { C_FECHA } else { 0 }, importe: if ancho >= 73 { C_IMPORTE } else { 13 } }
    }
    /// Lo que queda para el nombre del modelo, tras la marca y su número. Con ancho fijo, las columnas no se mueven.
    fn nombre(self, ancho: usize, texto: &str) -> String {
        ajustar(texto, ancho.saturating_sub(8 + self.fecha + C_DIAS + self.importe))
    }
    /// Lo que va a la derecha del nombre: fecha, columna de en medio y columna final.
    fn derecha(self, fecha: Span<'static>, medio: Span<'static>, fin: Vec<Span<'static>>) -> Vec<Span<'static>> {
        let mut v = Vec::new();
        if self.fecha > 0 {
            v.push(Span::styled(ajustar(&fecha.content, self.fecha), fecha.style));
        }
        v.push(medio);
        let usado: usize = fin.iter().map(Span::width).sum();
        v.push(Span::raw(" ".repeat(self.importe.saturating_sub(usado))));
        v.extend(fin);
        v
    }
}

/// Una fila: `izq` a la izquierda y `der` pegado a la derecha; la elegida, resaltada entera.
fn fila(izq: Vec<Span<'static>>, der: Vec<Span<'static>>, ancho: usize, elegida: bool) -> Line<'static> {
    let (a, b): (usize, usize) = (izq.iter().map(Span::width).sum(), der.iter().map(Span::width).sum());
    let mut s = izq;
    s.push(Span::raw(" ".repeat(ancho.saturating_sub(a + b).max(1))));
    s.extend(der);
    let mut l = a_medida(Line::from(s), ancho);
    if elegida {
        l.spans.iter_mut().for_each(|x| x.style = x.style.patch(tema::seleccion()));
    }
    l
}

/// «▾ » en la desplegada, «› » en la elegida; si no, dos espacios.
fn marca(elegida: bool, abierta: bool) -> Span<'static> {
    match (abierta, elegida) {
        (true, _) => Span::styled("▾ ", Style::new().fg(tema::acento())),
        (false, true) => Span::styled("› ", Style::new().fg(tema::acento())),
        _ => Span::raw("  "),
    }
}

/// El nombre corto de un modelo, para su fila.
fn corto(codigo_: &str) -> &'static str {
    if codigo_ == "130" { "IRPF" } else { nombre_modelo(codigo_, true) }
}

fn fila_pendiente(x: &Pendiente, hoy: &str, ancho: usize, elegida: bool, abierta: bool) -> Line<'static> {
    let gris = tema::apagado();
    let c = Columnas::de(ancho);
    let izq = vec![marca(elegida, abierta), Span::styled(format!("{:<5}", x.codigo), Style::new().add_modifier(Modifier::BOLD)), Span::raw(c.nombre(ancho, &format!("{} {}", corto(&x.codigo), x.periodo)))];
    // Lo pasado de plazo lo dice con todas las letras, en rojo.
    let vencido = x.cierra.as_str() < hoy;
    let rojo = Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD);
    let (txt, st) = if vencido && c.fecha == 0 { ("vencido".to_string(), rojo) } else { en_dias(hoy, &x.cierra) };
    let fecha = if vencido { Span::styled("vencido", rojo) } else { Span::styled(dia(&x.cierra, hoy), gris) };
    let fin = match x.resultado {
        Some(r) => {
            let (que, cifra, st) = que_sale(x.cual, r);
            let casi = if x.en_curso { "≈ " } else { "" };
            // La cifra nunca se corta: si no cabe con todo, primero se va lo que dice y luego el «≈».
            let largo = |t: &str| t.chars().count();
            if largo(casi) + largo(que) + 1 + largo(&cifra) <= c.importe {
                vec![Span::styled(format!("{casi}{que} "), gris), Span::styled(cifra, st)]
            } else if largo(casi) + largo(&cifra) <= c.importe {
                vec![Span::styled(format!("{casi}{cifra}"), st)]
            } else {
                vec![Span::styled(cifra, st)]
            }
        }
        None => Vec::new(),
    };
    fila(izq, c.derecha(fecha, Span::styled(ajustar(&txt, C_DIAS), st), fin), ancho, elegida)
}

fn fila_hecha(h: &Hecha, ancho: usize, elegida: bool, abierta: bool) -> Line<'static> {
    let gris = tema::apagado();
    let c = Columnas::de(ancho);
    let periodo = if h.periodo == "anual" { h.anio.to_string() } else { format!("{} {}", h.periodo, h.anio) };
    let izq = vec![marca(elegida, abierta), Span::styled(format!("{:<5}", h.codigo), Style::new().add_modifier(Modifier::BOLD)), Span::raw(c.nombre(ancho, &format!("{} {periodo}", corto(&h.codigo))))];
    let fecha = if h.fecha.is_empty() { "—".to_string() } else { trimestre::dia_mes(&h.fecha) };
    let cifra = |x: Option<f64>| x.map_or("—".to_string(), |r| cifra_con_signo(h.cual, r));
    let igual = matches!((h.presentado, h.tuyo), (Some(a), Some(b)) if (a - b).abs() <= DIF);
    let tuyo = if igual { Span::styled("✓", Style::new().fg(tema::BIEN)) } else { Span::styled(cifra(h.tuyo), gris) };
    fila(izq, c.derecha(Span::styled(fecha, gris), Span::raw(format!("{:>w$}", cifra(h.presentado), w = C_DIAS)), vec![tuyo]), ancho, elegida)
}

/// La fila de los títulos de una sección.
fn cabecera(titulo: String, fecha: &str, medio: &str, ancho: usize) -> Line<'static> {
    let gris = tema::apagado();
    let c = Columnas::de(ancho);
    let der = c.derecha(Span::styled(fecha.to_string(), gris), Span::styled(format!("{medio:>w$}", w = C_DIAS), gris), vec![Span::styled("con tus datos", gris)]);
    fila(vec![Span::styled(titulo, gris)], der, ancho, false)
}

/// Las dos secciones, con las casillas de la fila desplegada debajo de ella. Devuelve las líneas, en cuál está la fila
/// elegida y en cuál acaba (con lo desplegado, si es ella).
fn lineas(l: &Lista, sel: usize, abierta: Option<(usize, &Abierta)>, hoy: &str, ancho: usize) -> (Vec<Line<'static>>, usize, usize) {
    let gris = tema::apagado();
    let mut v: Vec<Line<'static>> = Vec::new();
    let (mut inicio, mut fin) = (0, 0);
    let esta = |i: usize| abierta.is_some_and(|x| x.0 == i);
    let despliega = |v: &mut Vec<Line<'static>>, i: usize| {
        if let Some((_, a)) = abierta.filter(|x| x.0 == i) {
            v.extend(a.lineas(ancho));
            v.push(Line::default());
        }
    };

    v.push(cabecera("por presentar".to_string(), "hasta", "", ancho));
    if l.pendientes.is_empty() {
        v.push(Line::styled(format!("Nada en los próximos {MESES} meses."), Style::new().fg(tema::BIEN)));
    }
    for (i, x) in l.pendientes.iter().enumerate() {
        if i == sel {
            inicio = v.len();
        }
        v.push(fila_pendiente(x, hoy, ancho, i == sel, esta(i)));
        despliega(&mut v, i);
        if i == sel {
            fin = v.len() - 1;
        }
    }
    // Lo del 130 se dice una vez, por lo primero que aún está en plazo.
    let sin = l.pendientes.iter().filter(|p| p.cierra.as_str() >= hoy).find_map(|p| match p.cual {
        Cual::Trimestre(_, a, q) => l.sin130.iter().find(|s| (s.0, s.1) == (a, q)),
        _ => None,
    });
    if let Some((_, _, t)) = sin {
        v.push(Line::styled(format!("130: no lo presentas, {t}"), gris));
    }

    v.push(Line::default());
    let titulo = format!("ya presentado · {}", l.anio_hechas);
    if l.hechas.is_empty() {
        v.push(titulo_bloque(titulo));
        v.push(Line::styled("Nada todavía.", gris));
    } else {
        v.push(cabecera(titulo, "fecha", "presentado", ancho));
    }
    for (k, h) in l.hechas.iter().enumerate() {
        let i = l.pendientes.len() + k;
        if i == sel {
            inicio = v.len();
        }
        v.push(fila_hecha(h, ancho, i == sel, esta(i)));
        despliega(&mut v, i);
        if i == sel {
            fin = v.len() - 1;
        }
    }
    (v.into_iter().map(|x| a_medida(x, ancho)).collect(), inicio, fin)
}

/* ================= las casillas de un modelo ================= */

/// Una casilla a rellenar.
#[derive(Debug, Clone)]
struct Casilla {
    n: u32,
    nombre: String,
    clase: Clase,
    valor: f64,
    /// En gris: la puede traer Hacienda o rellenar el formulario.
    gris: bool,
}

/// Lo que se despliega bajo un modelo: sus casillas y, solo si una cifra no es de fiar o no hay casillas, un aviso.
struct Abierta {
    casillas: Vec<Casilla>,
    avisos: Vec<String>,
}

/// Las casillas de un modelo que tienen algo, en el orden del formulario y sin las que son una operación con otras.
fn casillas_de(m: Modelo, grupos: &[Grupo], c: &Casillas, por_rellenar: bool) -> Vec<Casilla> {
    let mut v = Vec::new();
    for x in grupos.iter().flat_map(|g| g.1.iter()).filter(|x| c.v(x.n).abs() > DIF) {
        let u = uso(m, x.n);
        if u != Uso::Formula {
            v.push(Casilla { n: x.n, nombre: casillas::nombre_corto(x, &[c]), clase: x.clase, valor: c.v(x.n), gris: por_rellenar && u != Uso::Escribir });
        }
    }
    v
}

/// Si la 05 o la 15 del 130 cuentan con trimestres sin presentar, Hacienda puede traer otra cifra.
fn notas_130(d: &Datos, a: i32, q: u32, c: &Casillas) -> Vec<String> {
    if c.v(5).abs() <= DIF && c.v(15).abs() <= DIF {
        return Vec::new();
    }
    let sin: Vec<String> = (1..q).filter(|&k| modelo_130::obligado(d, a, k) && !hacienda::consta(d, a, k, Modelo::M130)).map(|k| format!("{k}T")).collect();
    if sin.is_empty() {
        return Vec::new();
    }
    let lista: Vec<&str> = sin.iter().map(String::as_str).collect();
    vec![format!("05 y 15 cuentan con el {}, sin presentar.", trimestre::y_lista(&lista))]
}

impl Abierta {
    fn con(casillas: Vec<Casilla>, mut avisos: Vec<String>) -> Abierta {
        if casillas.is_empty() && avisos.is_empty() {
            avisos.push("Ninguna casilla con importe.".to_string());
        }
        Abierta { casillas, avisos }
    }

    fn de_pendiente(d: &Datos, p: &Pendiente, memo: &mut Memo) -> Abierta {
        let (filas, mut avisos) = match p.cual {
            Cual::Trimestre(..) if p.futuro => (Vec::new(), vec!["El trimestre aún no ha empezado.".to_string()]),
            Cual::Trimestre(m, a, q) => {
                let calc = m.calcular(d, a, q, memo);
                let mut avisos = if m == Trimestral::M130 { notas_130(d, a, q, &calc) } else { Vec::new() };
                if m == Trimestral::M303 {
                    let (desde, hasta) = fmt::periodo(a, q);
                    avisos.extend(modelo_303::fuera(d, &desde, &hasta).into_iter().map(|(f, t)| format!("La factura {} lleva un IVA del {} que no está en estas casillas: súmala tú o corrígela.", f.numero, fmt::pct(t))));
                }
                (casillas_de(m.modelo(), casillas::grupos(m), &calc, true), avisos)
            }
            Cual::Resumen390(a) => {
                let calc = modelo_390::calcular(d, a, memo);
                let avisos = if calc.completo { Vec::new() } else { vec!["Faltan 303 de este año: estas casillas salen a medias.".to_string()] };
                (casillas_de(Modelo::M390, casillas::G390, &calc.casillas, true), avisos)
            }
            Cual::Otro => (Vec::new(), Vec::new()),
        };
        if !p.nota.is_empty() {
            avisos.insert(0, p.nota.clone());
        }
        Abierta::con(filas, avisos)
    }

    /// Solo lo que se presentó: la comparación con tus datos ya está en la fila y en el detalle (`d`).
    fn de_hecha(d: &Datos, h: &Hecha) -> Abierta {
        let (pres, m, grupos): (Option<Casillas>, Option<Modelo>, &[Grupo]) = match h.cual {
            Cual::Trimestre(m, a, q) => (trimestre::presentadas(d, a, q, m.modelo()).map(|c| c.0), Some(m.modelo()), casillas::grupos(m)),
            Cual::Resumen390(a) => (Modelo::M390.leido(d, a, 0).map(|l| l.casillas.clone()).filter(|c| !c.is_empty()), Some(Modelo::M390), casillas::G390),
            Cual::Otro => (None, None, &[]),
        };
        match (m, pres) {
            (Some(m), Some(p)) => Abierta::con(casillas_de(m, grupos, &p, false), Vec::new()),
            _ => Abierta::con(Vec::new(), vec!["Sin casillas: abre su PDF con o.".to_string()]),
        }
    }

    /// Lo desplegado, sangrado bajo el nombre del modelo.
    fn lineas(&self, ancho: usize) -> Vec<Line<'static>> {
        let gris = tema::apagado();
        let w = ancho.saturating_sub(SANGRIA).max(10);
        let txt = |c: &Casilla| if c.clase == Clase::Pct { format!("{} %", fmt::num(c.valor)) } else { fmt::num(c.valor) };
        let vw = self.casillas.iter().map(|c| txt(c).chars().count()).max().unwrap_or(0).max(8) + 2;
        let lw = w.saturating_sub(4 + vw).max(6);
        let mut v: Vec<Line<'static>> = self
            .casillas
            .iter()
            .map(|c| {
                let (nombre, cifra) = if c.gris { (gris, gris) } else { (Style::new(), Style::new().add_modifier(Modifier::BOLD)) };
                Line::from(vec![Span::styled(format!("{:<3} ", casillas::numero(c.n)), gris), Span::styled(ajustar(&c.nombre, lw), nombre), Span::styled(format!("{:>vw$}", txt(c)), cifra)])
            })
            .collect();
        if !self.casillas.is_empty() && !self.avisos.is_empty() {
            v.push(Line::default());
        }
        for n in &self.avisos {
            v.extend(partir(&n.replace(" €", "\u{a0}€"), w).into_iter().map(|l| Line::styled(l, gris)));
        }
        v.into_iter()
            .map(|l| {
                let mut s = vec![Span::raw(" ".repeat(SANGRIA))];
                s.extend(l.spans);
                a_medida(Line::from(s), ancho)
            })
            .collect()
    }
}

/* ================= la pantalla ================= */

/// Lo que hay que hacer con una tecla, fuera de esta vista.
pub enum Accion {
    /// La tecla no es de aquí.
    Nada,
    Usada,
    /// Ver el trimestre o el 390 enteros.
    VerTodo(Cual),
}

#[derive(Default)]
pub struct Ahora {
    clave: Option<(u64, i32, String)>,
    lista: Lista,
    sel: usize,
    /// La fila desplegada y sus casillas.
    abierta: Option<(usize, Abierta)>,
    scroll: u16,
    /// Al moverse o desplegar, que se vea la fila elegida con lo suyo.
    seguir: bool,
    /// Del último dibujo: las filas a la vista y dónde empieza y acaba la elegida.
    alto: usize,
    inicio: usize,
    fin: usize,
}

impl Ahora {
    /// Para que se recalcule al volver a dibujar.
    pub fn olvidar(&mut self) {
        self.clave = None;
    }

    /// Al cambiar de año, desde arriba.
    pub fn al_principio(&mut self) {
        (self.sel, self.scroll) = (0, 0);
        self.abierta = None;
    }

    /// Rehace la lista si han cambiado los datos, el año o el día. Devuelve true si la ha rehecho.
    pub fn preparar(&mut self, ctx: &Ctx, hoy: &str) -> bool {
        let k = (ctx.generacion, ctx.anio, hoy.to_string());
        if self.clave.as_ref() == Some(&k) {
            return false;
        }
        self.lista = lista(&ctx.datos, hoy, ctx.anio, &mut Memo::default());
        // Al abrir, hasta leer los PDF no se sabe qué se presentó ni cuándo estabas de alta: lo vencido, al acabar.
        if ctx.sin_leer {
            self.lista.pendientes.retain(|p| p.cierra.as_str() >= hoy);
        }
        self.sel = self.sel.min(self.lista.len().saturating_sub(1));
        if let Some(i) = self.abierta.as_ref().map(|x| x.0) {
            self.abierta = self.desplegar(i, ctx).map(|a| (i, a));
        }
        self.clave = Some(k);
        self.seguir = true;
        true
    }

    /// Las casillas de la fila `i`.
    fn desplegar(&self, i: usize, ctx: &Ctx) -> Option<Abierta> {
        let mut memo = Memo::default();
        match self.lista.pendientes.get(i) {
            Some(p) => Some(Abierta::de_pendiente(&ctx.datos, p, &mut memo)),
            None => self.lista.hechas.get(i - self.lista.pendientes.len()).map(|h| Abierta::de_hecha(&ctx.datos, h)),
        }
    }

    fn esta_abierta(&self) -> bool {
        self.abierta.as_ref().is_some_and(|x| x.0 == self.sel)
    }

    fn mover(&mut self, d: isize) {
        let n = self.lista.len();
        if n > 0 {
            self.sel = self.sel.saturating_add_signed(d).min(n - 1);
            self.seguir = true;
        }
    }

    pub fn dibujar(&mut self, f: &mut Frame, area: Rect, hoy: &str) {
        // Sin estirarse: en un terminal muy ancho, las cifras no se van a la otra punta.
        let area = Rect { width: area.width.min(ANCHO), ..area };
        let (h, ancho) = (area.height as usize, area.width as usize);
        if h == 0 || ancho == 0 {
            return;
        }
        let (v, inicio, fin) = lineas(&self.lista, self.sel, self.abierta.as_ref().map(|x| (x.0, &x.1)), hoy, ancho);
        let hay_mas = v.len() > h;
        let alto = if hay_mas { h - 1 } else { h };
        let max = v.len().saturating_sub(alto);
        if self.seguir {
            // Arriba del todo, también el título; lo desplegado, entero si cabe, sin perder su fila.
            let mut s = if self.sel == 0 { 0 } else { self.scroll as usize };
            if inicio < s {
                s = inicio;
            }
            if alto > 0 && fin >= s + alto {
                s = (fin + 1 - alto).min(inicio);
            }
            self.scroll = if self.sel == 0 { 0 } else { s as u16 };
            self.seguir = false;
        }
        self.scroll = self.scroll.min(max as u16);
        (self.alto, self.inicio, self.fin) = (alto, inicio, fin);
        f.render_widget(Paragraph::new(v).scroll((self.scroll, 0)), Rect { height: alto as u16, ..area });
        if hay_mas {
            let t = if (self.scroll as usize) < max { "↓ más" } else { "↑" };
            f.render_widget(Paragraph::new(Line::styled(t, tema::apagado()).right_aligned()), Rect { y: area.y + alto as u16, height: 1, ..area });
        }
    }

    fn abrir_pdf(&self, ctx: &mut Ctx) {
        let ruta = match self.lista.cual(self.sel) {
            Cual::Trimestre(m, a, q) => trimestre::pdf(&ctx.dir, &ctx.datos, a, q, m.modelo()),
            Cual::Resumen390(a) => trimestre::pdf(&ctx.dir, &ctx.datos, a, 0, Modelo::M390),
            Cual::Otro => None,
        };
        match ruta {
            Some(p) => ctx.abrir_ruta(&p),
            None => ctx.elegir_pdf(ctx.anio),
        }
    }

    pub fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Accion {
        match k.code {
            KeyCode::Char('o') => self.abrir_pdf(ctx),
            KeyCode::Char('d') => {
                // En lo que no tiene casillas, el trimestre en curso (o el último, en un año pasado).
                let hoy = fmt::hoy();
                return Accion::VerTodo(match self.lista.cual(self.sel) {
                    Cual::Trimestre(_, a, _) if a > fmt::anio_de(&hoy) => Cual::Trimestre(Trimestral::M303, ctx.anio, 4),
                    Cual::Otro => {
                        let q = if ctx.anio == fmt::anio_de(&hoy) { fmt::trimestre_de(&hoy) } else { 4 };
                        Cual::Trimestre(Trimestral::M303, ctx.anio, q)
                    }
                    c => c,
                });
            }
            KeyCode::Enter if self.lista.len() > 0 => {
                self.abierta = if self.esta_abierta() { None } else { self.desplegar(self.sel, ctx).map(|a| (self.sel, a)) };
                self.seguir = true;
            }
            KeyCode::Esc if self.abierta.is_some() => {
                self.abierta = None;
                self.seguir = true;
            }
            // Con la elegida desplegada, primero se recorre lo suyo si no se ve entero.
            KeyCode::Up | KeyCode::Char('k') => {
                if self.esta_abierta() && self.scroll as usize > self.inicio {
                    self.scroll -= 1;
                } else {
                    self.mover(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let al_final = self.sel + 1 >= self.lista.len();
                if al_final || (self.esta_abierta() && self.alto > 0 && self.fin >= self.scroll as usize + self.alto) {
                    // En la última fila, lo que quede debajo (las notas, «Nada todavía»).
                    self.scroll = self.scroll.saturating_add(1);
                } else {
                    self.mover(1);
                }
            }
            KeyCode::PageUp => self.mover(-5),
            KeyCode::PageDown | KeyCode::Char(' ') => self.mover(5),
            KeyCode::Home => {
                self.sel = 0;
                self.seguir = true;
            }
            KeyCode::End => {
                self.sel = self.lista.len().saturating_sub(1);
                self.seguir = true;
            }
            _ => return Accion::Nada,
        }
        Accion::Usada
    }

    pub fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![("↑↓", "elegir"), ("Enter", if self.esta_abierta() { "plegar" } else { "casillas" })];
        if self.abierta.is_some() {
            v.push(("Esc", "plegar"));
        }
        v.extend([("d", "ver todo"), ("o", "PDF")]);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::{Factura, Linea, Num};

    /// Una factura de 1.000 € + IVA en cada fecha.
    fn con_facturas(fechas: &[&str]) -> Datos {
        let mut d = Datos::default();
        for (i, f) in fechas.iter().enumerate() {
            let linea = Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(21.0) };
            d.facturas.push(Factura { estado: "emitida".into(), numero: format!("F-{i}"), fecha: (*f).into(), operacion: "nacional".into(), lineas: vec![linea], ..Default::default() });
        }
        d
    }

    fn texto(v: &[Line]) -> String {
        v.iter().map(|l| l.to_string() + "\n").collect()
    }

    fn lista_de(l: &Lista, hoy: &str, ancho: usize) -> String {
        texto(&lineas(l, 0, None, hoy, ancho).0)
    }

    fn pendientes(l: &Lista) -> Vec<String> {
        l.pendientes.iter().map(|p| format!("{} {}", p.codigo, p.periodo)).collect()
    }

    #[test]
    fn dias_que_quedan() {
        let c = |h, f| en_dias(h, f).0;
        assert_eq!(c("2026-11-22", "2027-02-01"), "(en 71 días)");
        assert_eq!(c("2026-10-19", "2026-10-20"), "(mañana)");
        assert_eq!(c("2026-10-20", "2026-10-20"), "(hoy)");
        assert_eq!(c("2026-10-21", "2026-10-20"), "(hace 1 día)");
        assert_eq!(c("2026-11-22", "2026-10-20"), "(hace 33 días)");
    }

    /// Dos secciones y cada modelo con su fecha: el 347 acaba después que el 303.
    #[test]
    fn cada_modelo_con_su_fecha() {
        let mut d = con_facturas(&["2026-11-05", "2026-11-06", "2026-11-07", "2026-11-08"]);
        d.facturas.iter_mut().for_each(|f| f.cliente.nif = "B00000000".into());
        let l = lista(&d, "2026-11-22", 2026, &mut Memo::default());
        assert_eq!(pendientes(&l), ["303 4T 2026", "130 4T 2026", "390 2026", "347 2026"]);
        let t = lista_de(&l, "2026-11-22", 80);
        let fila = |c: &str| t.lines().find(|x| x.contains(c)).unwrap().to_string();
        assert!(fila("303  IVA 4T").contains("1 feb 2027") && fila("303  IVA 4T").contains("(en 71 días)"), "{t}");
        assert!(fila("347  Terceros").contains("1 mar 2027") && fila("347  Terceros").contains("(en 99 días)"), "{t}");
        assert!(t.contains("por presentar") && t.contains("ya presentado") && t.contains("≈") && !t.contains("de momento"), "{t}");
        assert!(!t.contains("quedan") && !t.contains("abre en") && !t.contains("lo siguiente") && !t.contains("después"), "{t}");
    }

    /// El límite va por cuándo abre el plazo: el 1T de 2027 (abre el 1 de abril) sale desde el 1 de diciembre, sin
    /// importes, y la Renta, por cuándo acaba (30 de junio), desde el 1 de marzo.
    #[test]
    fn se_ven_cuatro_meses() {
        let d = con_facturas(&["2026-11-05"]);
        let tiene = |hoy: &str, que: &str| pendientes(&lista(&d, hoy, 2026, &mut Memo::default())).iter().any(|x| x == que);
        assert!(!tiene("2026-11-30", "303 1T 2027") && tiene("2026-12-01", "303 1T 2027"));
        let l = lista(&d, "2026-12-01", 2026, &mut Memo::default());
        let p = l.pendientes.iter().find(|p| p.periodo == "1T 2027").unwrap();
        assert!(p.futuro && p.resultado.is_none());
        // Se ve sin cifras, y al desplegarlo dice por qué no hay casillas.
        let t = lista_de(&l, "2026-12-01", 80);
        assert!(t.contains("303  IVA 1T 2027") && t.contains("en 140 días"), "{t}");
        let a = Abierta::de_pendiente(&d, p, &mut Memo::default());
        let t = texto(&a.lineas(80));
        assert!(a.casillas.is_empty() && t.contains("aún no ha empezado") && !t.contains("Ninguna casilla"), "{t}");
        assert!(!tiene("2027-02-28", "100 2026") && tiene("2027-03-01", "100 2026"));
    }

    /// A quien no le toca el 130 no le sale en diciembre el del año que viene para quitárselo el 1 de enero.
    #[test]
    fn el_130_exento_no_aparece_y_desaparece() {
        let mut d = con_facturas(&["2026-11-05"]);
        d.fiscal.exento130 = "si".into();
        for hoy in ["2026-12-15", "2027-01-02"] {
            let l = lista(&d, hoy, 2026, &mut Memo::default());
            assert!(!pendientes(&l).iter().any(|x| x.starts_with("130")), "{hoy}: {:?}", pendientes(&l));
        }
    }

    /// En enero del año nuevo, con ese año elegido: lo primero sigue siendo el 4T del anterior.
    #[test]
    fn en_enero_sale_el_4t_del_anterior() {
        let d = con_facturas(&["2026-11-05"]);
        let l = lista(&d, "2027-01-10", 2027, &mut Memo::default());
        assert_eq!(pendientes(&l)[..3], ["303 4T 2026", "130 4T 2026", "390 2026"], "{:?}", pendientes(&l));
        let t = lista_de(&l, "2027-01-10", 80);
        assert!(t.contains("en 22 días") && t.contains("1 feb"), "{t}");
    }

    /// El último día y el día después (el 30 de enero de 2027 es sábado: pasa al lunes 1 de febrero).
    #[test]
    fn el_ultimo_dia_y_el_siguiente() {
        let d = con_facturas(&["2026-11-05"]);
        let t = lista_de(&lista(&d, "2027-02-01", 2027, &mut Memo::default()), "2027-02-01", 80);
        assert!(t.lines().any(|x| x.contains("303  IVA 4T") && x.contains("(hoy)")), "{t}");
        let t = lista_de(&lista(&d, "2027-02-02", 2027, &mut Memo::default()), "2027-02-02", 80);
        assert!(t.contains("vencido") && t.contains("(hace 1 día)"), "{t}");
    }

    /// Se despliega en su sitio: debajo de su fila y antes de la siguiente, solo con las casillas que se rellenan.
    #[test]
    fn se_despliega_bajo_su_fila() {
        let d = con_facturas(&["2026-11-05"]);
        let l = lista(&d, "2027-01-10", 2027, &mut Memo::default());
        let a = Abierta::de_pendiente(&d, &l.pendientes[0], &mut Memo::default());
        let n: Vec<u32> = a.casillas.iter().map(|c| c.n).collect();
        assert!(n.contains(&7) && n.contains(&9), "{n:?}");
        for k in [27, 46, 64, 66, 69, 71] {
            assert!(!n.contains(&k), "{k} es una operación");
        }
        let (v, inicio, fin) = lineas(&l, 0, Some((0, &a)), "2027-01-10", 80);
        let t = texto(&v);
        assert!(v.iter().all(|x| x.width() <= 80), "{t}");
        assert!(v[inicio].to_string().starts_with("▾ 303"), "{t}");
        let (casilla, siguiente) = (t.find("\n       07 ").expect(&t), t.find("130  IRPF").expect(&t));
        assert!(casilla < siguiente && fin > inicio + 1, "{t}");
        // Solo las casillas: ni títulos, ni fechas, ni explicaciones.
        for sobra in ["escribe", "comprueba", "* ", "se presenta", "domiciliar", "eliges"] {
            assert!(!t.contains(sobra), "{sobra}: {t}");
        }
    }

    /// Un 303 negativo es «a compensar»; en el 4T, «a tu favor», porque ahí se puede pedir la devolución.
    #[test]
    fn devolver_solo_en_el_4t() {
        let (m, c) = (Trimestral::M303, -56.7);
        assert_eq!(que_sale(Cual::Trimestre(m, 2026, 3), c).0, "a compensar");
        assert_eq!(que_sale(Cual::Trimestre(m, 2026, 4), c).0, "a tu favor");
    }

    /// Solo con recibos de autónomo y nada presentado: vencido, porque se le puede haber olvidado.
    #[test]
    fn con_recibos_y_nada_presentado_sale_vencido() {
        let mut d = Datos::default();
        d.tgss.recibos.push(crate::dominio::tgss::Recibo { periodo: "2026-02".into(), cargo: "2026-02-28".into(), importe: 300.0, ..Default::default() });
        crate::dominio::gasto::con_recibos(&mut d);
        let l = lista(&d, "2026-11-22", 2026, &mut Memo::default());
        assert!(pendientes(&l).contains(&"303 1T 2026".to_string()), "{:?}", pendientes(&l));
        assert!(pendientes(&l).contains(&"130 1T 2026".to_string()), "{:?}", pendientes(&l));
        assert_eq!(vencidos(&d, "2026-04-20"), 0, "aún en plazo");
        assert_eq!(vencidos(&d, "2026-04-21"), 2);
        let t = lista_de(&l, "2026-11-22", 80);
        assert!(t.lines().any(|x| x.contains("303  IVA 1T 2026") && x.contains("vencido") && x.contains("(hace 216 días)")), "{t}");
        // Sin nada de ese trimestre, no se inventa un vencido.
        assert_eq!(vencidos(&Datos::default(), "2026-11-22"), 0);
        // Una regularización cargada en un trimestre no dice que en ese trimestre hubiera actividad.
        let mut d = Datos::default();
        d.tgss.recibos.push(crate::dominio::tgss::Recibo { periodo: "2025-11".into(), cargo: "2026-01-30".into(), concepto: "Regularización cuotas Noviembre 2025".into(), importe: 100.0, ..Default::default() });
        crate::dominio::gasto::con_recibos(&mut d);
        assert_eq!(vencidos(&d, "2026-11-22"), 0);
    }

    /// Pagar la cuota estando de baja en Hacienda no es un vencido; con una factura en ese periodo, sí.
    #[test]
    fn de_baja_en_hacienda_no_vence() {
        use crate::dominio::CambioCenso;
        let mut d = Datos::default();
        d.tgss.recibos.push(crate::dominio::tgss::Recibo { periodo: "2026-02".into(), cargo: "2026-02-28".into(), concepto: "Cuota autónomos".into(), importe: 300.0, ..Default::default() });
        crate::dominio::gasto::con_recibos(&mut d);
        d.lecturas.censo = vec![CambioCenso { fecha: "2025-02-25".into(), alta: false }, CambioCenso { fecha: "2026-08-01".into(), alta: true }];
        assert_eq!(vencidos(&d, "2026-11-22"), 0);
        assert_eq!(trimestre::estado_modelo(&d, "2026-11-22", 2026, 1, Trimestral::M303), Estado::NoToca("de baja en Hacienda"));
        let linea = Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(21.0) };
        d.facturas.push(Factura { estado: "emitida".into(), numero: "F-1".into(), fecha: "2026-02-05".into(), operacion: "nacional".into(), lineas: vec![linea], ..Default::default() });
        assert_eq!(trimestre::estado_modelo(&d, "2026-11-22", 2026, 1, Trimestral::M303), Estado::Vencido);
    }

    /// De hace más de un año solo se avisa si se consultó a Hacienda: si no, no se sabe si se presentó.
    #[test]
    fn los_anios_viejos_solo_si_se_consultaron() {
        let mut d = con_facturas(&["2024-02-05"]);
        let hay = |d: &Datos| pendientes(&lista(d, "2026-11-22", 2026, &mut Memo::default())).contains(&"303 1T 2024".to_string());
        assert!(!hay(&d));
        d.hacienda.insert(2024, crate::dominio::hacienda::Consulta { fecha: "2026-01-01T10:00:00".into(), ..Default::default() });
        assert!(hay(&d));
        // Y si consta presentado, deja de salir.
        d.lecturas.formularios.insert("2024-1T-303".into(), crate::dominio::FormularioLeido { casillas: [(71, 210.0)].into_iter().collect(), ..Default::default() });
        assert!(!hay(&d));
    }

    /// El aviso en rojo se ve desde cualquier sitio: en el menú, en la pestaña y en Inicio.
    #[test]
    fn el_rojo_se_ve_desde_fuera() {
        use super::super::prueba::{app_con, colores, ir, pantalla};
        let d = con_facturas(&["2020-02-05", &format!("{}-01-05", fmt::anio_actual() - 1)]);
        let mut a = app_con(d, std::path::Path::new("."));
        ir(&mut a, "Inicio");
        for (w, h) in [(120, 36), (80, 24)] {
            let p = pantalla(&mut a, w, h);
            assert!(p.contains("vencido"), "{w}x{h}: {p}");
        }
        let rojo = format!("{:?}", tema::MAL);
        let g: Vec<Vec<serde_json::Value>> = serde_json::from_str(&colores(&mut a, 120, 36)).unwrap();
        let en_rojo = |g: &[Vec<serde_json::Value>], que: &str| {
            g.iter().any(|f| {
                let t: Vec<&str> = f.iter().map(|c| c[0].as_str().unwrap()).collect();
                let w: Vec<char> = que.chars().collect();
                (0..t.len().saturating_sub(w.len())).any(|i| (0..w.len()).all(|k| t[i + k] == w[k].to_string() && f[i + k][1] == rojo.as_str()))
            })
        };
        assert!(en_rojo(&g, "Autónomo"), "el menú");
        ir(&mut a, "Facturas");
        let g: Vec<Vec<serde_json::Value>> = serde_json::from_str(&colores(&mut a, 120, 36)).unwrap();
        assert!(en_rojo(&g, "Modelos"), "la pestaña");
    }

    /// La cifra no se corta nunca, aunque no quepa con su etiqueta; y lo vencido no se cuenta en presente.
    #[test]
    fn la_cifra_entera_y_lo_vencido_en_pasado() {
        let p = Pendiente { cual: Cual::Trimestre(Trimestral::M303, 2026, 3), codigo: "303".into(), periodo: "3T 2026".into(), abre: Some("2026-10-01".into()), cierra: "2026-10-20".into(), resultado: Some(-10000.0), en_curso: true, futuro: false, nota: String::new() };
        for ancho in [66, 74, 76, 80, ANCHO as usize] {
            let t = fila_pendiente(&p, "2026-10-10", ancho, false, false).to_string();
            assert!(t.contains("10.000,00 €") && !t.contains('…'), "{ancho}: {t}");
        }
        let t = fila_pendiente(&p, "2026-11-22", 80, false, false).to_string();
        assert!(t.contains("vencido") && t.contains("(hace 33 días)"), "{t}");
    }

    /// Lo presentado no se da por malo si no coincide: se ven las dos cifras.
    #[test]
    fn lo_presentado_frente_a_tus_datos() {
        use crate::dominio::FormularioLeido;
        let mut d = con_facturas(&["2026-02-05"]);
        let mut c = Trimestral::M303.calcular(&d, 2026, 1, &mut Memo::default());
        c.insert(71, 150.0);
        d.lecturas.formularios.insert("2026-1T-303".into(), FormularioLeido { casillas: c, ..Default::default() });
        let l = lista(&d, "2026-11-22", 2026, &mut Memo::default());
        assert_eq!(l.hechas.len(), 1);
        assert_eq!((l.hechas[0].presentado, l.hechas[0].tuyo), (Some(150.0), Some(210.0)));
        let t = lista_de(&l, "2026-11-22", 80);
        assert!(t.contains("ya presentado · 2026") && t.contains("150,00 €") && t.contains("210,00 €") && t.contains("con tus datos"), "{t}");
        assert!(!t.contains("NO CUADRA"), "{t}");
        // Al desplegarlo, solo lo que se presentó: una columna, con la casilla del resultado fuera por ser una operación.
        let a = Abierta::de_hecha(&d, &l.hechas[0]);
        let t = texto(&a.lineas(80));
        assert!(a.casillas.iter().any(|c| c.n == 7) && a.casillas.iter().all(|c| !c.gris), "{t}");
        assert!(!t.contains("con tus datos") && !t.contains("presentado") && !t.contains('✓'), "{t}");
    }

    /// Lo que se pasó de plazo va primero, en la misma lista.
    #[test]
    fn lo_vencido_va_primero() {
        let d = con_facturas(&["2026-02-05", "2026-05-05", "2026-11-05"]);
        let l = lista(&d, "2026-11-22", 2026, &mut Memo::default());
        assert_eq!(l.pendientes[0].periodo, "1T 2026", "{:?}", pendientes(&l));
        let t = lista_de(&l, "2026-11-22", 100);
        let (vencido, queda) = (t.find("(hace").expect(&t), t.find("(en 71 días)").expect(&t));
        assert!(vencido < queda, "{t}");
    }

    /// Nada se pasa de ancho, tampoco en una pantalla muy estrecha.
    #[test]
    fn cabe_en_cualquier_ancho() {
        let d = con_facturas(&["2026-02-05", "2026-11-05"]);
        for hoy in ["2026-11-22", "2027-01-10", "2027-02-02"] {
            let l = lista(&d, hoy, 2026, &mut Memo::default());
            let a = Abierta::de_pendiente(&d, &l.pendientes[0], &mut Memo::default());
            for ancho in [20, 40, 56, 62, 72, 80, ANCHO as usize] {
                let (v, ..) = lineas(&l, 1, Some((0, &a)), hoy, ancho);
                assert!(v.iter().all(|x| x.width() <= ancho), "{hoy} a {ancho}: {}", texto(&v));
            }
        }
    }

    /// En un terminal enorme, la lista no se va a la otra punta.
    #[test]
    fn no_se_estira_en_un_terminal_ancho() {
        use super::super::prueba::{app, ir, pantalla, sin_menu};
        let mut a = app();
        ir(&mut a, "Modelos");
        let p = sin_menu(&pantalla(&mut a, 220, 40));
        let filas: Vec<&str> = p.lines().filter(|l| l.contains("303  IVA")).collect();
        assert!(!filas.is_empty(), "{p}");
        assert!(filas.iter().all(|l| l.trim_end().chars().count() <= ANCHO as usize), "{p}");
    }

    /// ↑↓ recorre primero lo desplegado si no cabe, y Enter y Esc lo pliegan.
    #[test]
    fn desplegar_y_plegar_con_las_teclas() {
        use super::super::prueba::{app, ir, pantalla, teclas};
        let mut a = app();
        ir(&mut a, "Modelos");
        pantalla(&mut a, 120, 36);
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("▾ 303") && p.contains("por presentar"), "sin ventana nueva: {p}");
        teclas(&mut a, "<esc>");
        assert!(!pantalla(&mut a, 120, 36).contains('▾'));
        teclas(&mut a, "<enter>");
        pantalla(&mut a, 120, 36);
        teclas(&mut a, "<enter>");
        assert!(!pantalla(&mut a, 120, 36).contains('▾'));
        // En una pantalla baja, ↓ baja por lo desplegado hasta verlo entero y solo entonces pasa a la fila siguiente.
        teclas(&mut a, "<home><enter>");
        let mut vistas = String::new();
        for _ in 0..40 {
            vistas.push_str(&pantalla(&mut a, 80, 16));
            teclas(&mut a, "<down>");
        }
        assert!(vistas.contains("▾ 303") && vistas.contains("› ") && vistas.contains("ya presentado"), "{vistas}");
        // Y ↑ vuelve hasta arriba, con la primera fila y el título a la vista.
        for _ in 0..80 {
            pantalla(&mut a, 80, 16);
            teclas(&mut a, "<up>");
        }
        let p = pantalla(&mut a, 80, 16);
        assert!(p.contains("por presentar") && p.contains("▾ 303"), "{p}");
    }
}
