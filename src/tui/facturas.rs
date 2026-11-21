//! Facturas: las del año y todos los borradores, el detalle, el editor de borradores,
//! emitir, cobrar y duplicar; y tus clientes (`C`, con el de la factura elegida).

use super::clientes::{self, limpia_nif, opciones_operacion, validar_nif, Clientes};
use crate::dominio::factura::{self, mencion_iva, serie_factura, siguiente_numero, totales_factura};
use crate::dominio::fmt::{self, dmy, eur, fecha_corta, pct};
use crate::dominio::{uid, Cliente, Datos, Factura, Linea, Num};
use crate::informe::autonomo;
use crate::tui::widgets::{coincide, derecha, guia, linea_atajos, tema, Confirmar, Entrada, Formulario, Respuesta, Resultado};
use crate::tui::texto::lineas;
use crate::tui::{Ctx, Pantalla};
use editor::{Editor, Salida};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use super::facturas_editor as editor;

/* ================= reglas ================= */

const PAISES_UE: [&str; 28] = ["AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "GR", "ES", "FI", "FR", "HR", "HU", "IE", "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK"];

/// Lo que falta o no cuadra en el cliente de una factura sin IVA a otro país.
pub fn problemas_cliente(f: &Factura) -> Vec<String> {
    let op = f.operacion.as_str();
    let mut v: Vec<String> = Vec::new();
    if op != "ue" && op != "extra" {
        return v;
    }
    let pais = f.cliente.pais.trim().to_uppercase();
    let nif = limpia_nif(&f.cliente.nif);
    let de_ue = PAISES_UE.contains(&pais.as_str());
    if pais.len() != 2 || !pais.chars().all(|c| c.is_ascii_uppercase()) {
        v.push("Pon el país del cliente con su código de 2 letras (FR, DE, US…).".into());
    }
    if op == "ue" {
        if pais == "ES" {
            v.push("Un cliente de España no va sin IVA: elige «Con IVA».".into());
        } else if !pais.is_empty() && !de_ue {
            v.push("Ese país no es de la UE: elige «Sin IVA: fuera de la UE».".into());
        }
        if nif.is_empty() {
            v.push("Pon el número de IVA intracomunitario del cliente (compruébalo en VIES). Sin él, la factura lleva IVA español.".into());
        } else if de_ue {
            let pref = if pais == "GR" { "EL" } else { pais.as_str() };
            if !nif.starts_with(pref) {
                v.push(format!("El número de IVA intracomunitario empieza por el código del país: {pref}…"));
            }
        }
    } else {
        if pais != "ES" && de_ue {
            v.push("Ese país es de la UE: si es una empresa, elige «Sin IVA: empresa de la UE»; si es un particular, «Con IVA».".into());
        }
        if nif.is_empty() {
            v.push(if pais == "ES" { "Pon el NIF del cliente de Canarias, Ceuta o Melilla." } else { "Pon el número fiscal del cliente (Tax ID, EIN, VAT…)." }.into());
        } else if pais == "ES" && !validar_nif(&nif) {
            v.push("Para un cliente de Canarias, Ceuta o Melilla pon su NIF español (no parece válido).".into());
        }
    }
    v
}

/// Avisos del editor (true: grave).
pub(super) fn avisos(d: &Datos, f: &Factura) -> Vec<(bool, String)> {
    let mut v = Vec::new();
    let t = totales_factura(f);
    let sin_cliente = f.cliente.nombre.trim().is_empty();
    if d.perfil.nombre.trim().is_empty() || d.perfil.nif.trim().is_empty() {
        v.push((true, "Faltan tus datos (nombre y NIF): se toman de tu certificado.".to_string()));
    }
    if f.operacion == "nacional" && !f.cliente.nif.is_empty() && !validar_nif(&f.cliente.nif) {
        v.push((false, "El NIF del cliente no parece válido.".into()));
    }
    v.extend(problemas_cliente(f).into_iter().map(|p| (false, p)));
    if sin_cliente && t.importe_total.abs() > 400.0 {
        v.push((false, "Sin datos del cliente (factura simplificada) solo se puede hasta 400 € IVA incluido.".into()));
    }
    if sin_cliente && f.operacion != "nacional" {
        v.push((false, "Para clientes extranjeros pon sus datos.".into()));
    }
    if f.retencion.v() > 0.0 && f.operacion != "nacional" {
        v.push((false, "La retención de IRPF solo se aplica a empresas o autónomos de España.".into()));
    }
    if f.retencion.v() > 0.0 && f.operacion == "nacional" && f.cliente.nif.is_empty() {
        v.push((false, "A un particular no se le aplica retención de IRPF.".into()));
    }
    if f.fecha > fmt::hoy() {
        v.push((false, "La fecha de la factura es futura.".into()));
    }
    v
}

/// Por qué no se puede emitir, o None si se puede.
fn problema_emision(d: &Datos, f: &Factura) -> Option<String> {
    if f.emitida() {
        return Some(format!("La factura {} ya está emitida.", f.numero));
    }
    if f.rectifica.is_some() {
        return Some("Es una rectificativa: aún no se pueden hacer en la app.".into());
    }
    if fmt::fecha(&f.fecha).is_none() {
        return Some("La factura no tiene una fecha válida: edítala antes de emitirla.".into());
    }
    if d.perfil.nombre.trim().is_empty() || d.perfil.nif.trim().is_empty() {
        return Some("Faltan tus datos fiscales (nombre y NIF): se toman de tu certificado.".into());
    }
    if !f.lineas.iter().any(|l| !l.concepto.trim().is_empty() && l.precio.v() != 0.0) {
        return Some("Añade al menos un concepto con precio.".into());
    }
    let sin_cliente = f.cliente.nombre.trim().is_empty();
    if f.operacion != "nacional" && sin_cliente {
        return Some("Pon los datos del cliente extranjero.".into());
    }
    if let Some(p) = problemas_cliente(f).into_iter().next() {
        return Some(p);
    }
    if sin_cliente && totales_factura(f).importe_total.abs() > 3000.0 {
        return Some("Una factura sin datos del cliente no puede superar 3.000 €.".into());
    }
    None
}

/// Una línea en blanco con el IVA por defecto.
pub(super) fn linea_vacia(d: &Datos) -> Linea {
    let iva = d.facturacion.iva.0.filter(|x| *x != 0.0).unwrap_or(21.0);
    Linea { cantidad: Num::de(1.0), precio: Num(None), iva: Num::de(iva), ..Default::default() }
}

/// Un borrador nuevo con lo de `facturacion` (serie, IVA, retención, forma de pago).
fn nueva_factura(d: &Datos) -> Factura {
    let hoy = fmt::hoy();
    let dias = d.facturacion.dias_vencimiento.0.filter(|x| *x != 0.0).unwrap_or(30.0);
    Factura {
        id: uid(),
        estado: "borrador".into(),
        vencimiento: fmt::sumar_dias(&hoy, dias as i64),
        fecha: hoy,
        operacion: "nacional".into(),
        cliente: Cliente { pais: "ES".into(), ..Default::default() },
        retencion: d.facturacion.retencion,
        forma_pago: d.facturacion.forma_pago.clone(),
        creada: fmt::ahora(),
        ..Default::default()
    }
}

/// Un borrador nuevo con el cliente, las líneas y las condiciones de otra factura («Duplicar»).
fn duplicada(d: &Datos, o: &Factura) -> Factura {
    Factura {
        operacion: o.operacion.clone(),
        cliente: o.cliente.clone(),
        lineas: o.lineas.clone(),
        retencion: o.retencion,
        forma_pago: o.forma_pago.clone(),
        notas: o.notas.clone(),
        exencion: o.exencion.clone(),
        ..nueva_factura(d)
    }
}

/// Mete la factura en los datos (o cambia la que tiene el mismo id).
fn poner(d: &mut Datos, f: Factura) {
    match d.facturas.iter_mut().find(|x| x.id == f.id) {
        Some(x) => *x = f,
        None => d.facturas.push(f),
    }
}

/// Al emitir, el cliente queda guardado en tus clientes (si ya estaba, con sus datos de esta factura).
fn guardar_cliente(d: &mut Datos, f: &Factura) {
    let c = &f.cliente;
    if c.nombre.is_empty() {
        return;
    }
    let nif = limpia_nif(&c.nif);
    let ex = d.clientes.iter_mut().find(|x| (!nif.is_empty() && limpia_nif(&x.nif) == nif) || x.nombre == c.nombre);
    let x = match ex {
        Some(x) => x,
        None => {
            d.clientes.push(Cliente { id: uid(), ..Default::default() });
            d.clientes.last_mut().unwrap()
        }
    };
    x.nombre = c.nombre.clone();
    x.nif = c.nif.clone();
    x.pais = c.pais.clone();
    x.direccion = c.direccion.clone();
    x.cp = c.cp.clone();
    x.ciudad = c.ciudad.clone();
    x.email = c.email.clone();
    x.operacion = f.operacion.clone();
}

/// Emite el borrador: número, estado, fecha de emisión y NIF limpio. Devuelve el número.
fn emitir(ctx: &mut Ctx, mut f: Factura) -> String {
    let numero = siguiente_numero(&ctx.datos, false, fmt::anio_de(&f.fecha));
    f.numero = numero.clone();
    f.estado = "emitida".into();
    f.emitida = Some(fmt::ahora());
    f.cliente.nif = limpia_nif(&f.cliente.nif);
    guardar_cliente(&mut ctx.datos, &f);
    poner(&mut ctx.datos, f);
    if ctx.guardar() {
        ctx.ok(format!("Factura {numero} emitida"));
    }
    numero
}

/// Guarda el borrador y devuelve su id.
fn guardar_borrador(ctx: &mut Ctx, mut f: Factura) -> String {
    if f.lineas.is_empty() {
        // Siempre al menos una línea en el editor.
        f.lineas.push(linea_vacia(&ctx.datos));
    }
    let id = f.id.clone();
    poner(&mut ctx.datos, f);
    if ctx.guardar() {
        ctx.ok("Borrador guardado");
    }
    id
}

/* ================= cómo se ve ================= */

/// 2 → «2», 1,5 → «1,5» (hasta 3 decimales, como `fmtCant`).
pub(super) fn cant(x: f64) -> String {
    let s = format!("{:.3}", (x * 1000.0).round() / 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

/// Etiqueta a la izquierda y valor a la derecha en `ancho` caracteres.
pub(super) fn fila(etq: impl Into<String>, val: impl Into<String>, ancho: u16, est_etq: Style, est_val: Style) -> Line<'static> {
    let (e, v) = (etq.into(), val.into());
    let hueco = (ancho as usize).saturating_sub(e.chars().count() + v.chars().count()).max(1);
    Line::from(vec![Span::styled(e, est_etq), Span::raw(" ".repeat(hueco)), Span::styled(v, est_val)])
}

/// Ancho de las etiquetas del detalle («Forma de pago» y un hueco).
const ETQ: usize = 14;

/// «Etiqueta     valor», con el valor partido en varias líneas (sangradas) si no cabe.
fn kv(etq: &str, val: &str, ancho: u16, estilo: Style) -> Vec<Line<'static>> {
    lineas(val, (ancho as usize).saturating_sub(ETQ))
        .into_iter()
        .enumerate()
        .map(|(i, t)| Line::from(vec![Span::styled(format!("{:<ETQ$}", if i == 0 { etq } else { "" }), tema::apagado()), Span::styled(t, estilo)]))
        .collect()
}

/// Estado para la lista y el detalle: Borrador, Pendiente, Vencida, Cobrada 14 oct.
fn estado(f: &Factura, hoy: &str) -> (String, Style) {
    if f.borrador() {
        ("Borrador".into(), tema::apagado())
    } else if let Some(c) = f.cobrada.as_deref().filter(|c| !c.is_empty()) {
        (format!("Cobrada {}", fecha_lista(c, fmt::anio_de(hoy))), Style::new().fg(tema::BIEN))
    } else if !f.vencimiento.is_empty() && f.vencimiento.as_str() < hoy {
        ("Vencida".into(), Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD))
    } else {
        ("Pendiente".into(), Style::new().fg(tema::AVISO))
    }
}

/// «2026-10-14» → «14 oct»; con el año si no es `anio` («1 abr 2023»).
fn fecha_lista(s: &str, anio: i32) -> String {
    let c = fecha_corta(s);
    match c.rsplit_once(' ') {
        Some((dm, _)) if fmt::anio_de(s) == anio => dm.to_string(),
        _ => c,
    }
}

fn plural(n: usize) -> String {
    if n == 1 { "1 factura".into() } else { format!("{n} facturas") }
}

/// Todo lo de una factura, para la vista completa; `corto` (el panel de la derecha) deja fuera la dirección
/// del cliente. Estilo C2: el número y el estado arriba; las etiquetas en gris; y los totales con puntos guía,
/// acabando en `ancho`.
fn texto_detalle(f: &Factura, ancho: u16, corto: bool, cerrado: bool) -> Vec<Line<'static>> {
    let hoy = fmt::hoy();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let nacional = f.operacion == "nacional";
    let c = &f.cliente;
    let mut v: Vec<Line> = Vec::new();
    let (est, st) = estado(f, &hoy);
    let titulo = if f.numero.is_empty() { "Borrador".to_string() } else { format!("Factura {}", f.numero) };
    let est = if f.emitida() && !f.cobrada() { format!("Emitida · {}", est.to_lowercase()) } else { est };
    v.push(Line::from(vec![Span::styled(titulo, tema::titulo()), Span::raw("   "), Span::styled(est, st)]));
    if let Some(r) = &f.rectifica {
        v.push(Line::styled(format!("Rectifica la factura {}", if r.numero.is_empty() { "?" } else { &r.numero }), tema::apagado()));
    }
    v.push(Line::from(""));
    let normal = Style::new();
    if c.nombre.trim().is_empty() {
        v.extend(kv("cliente", "Consumidor final (factura simplificada)", ancho, normal));
    } else {
        v.extend(kv("cliente", &c.nombre, ancho, negrita));
        if !c.nif.is_empty() {
            v.extend(kv("", &format!("{} {}", if f.operacion == "ue" { "VAT" } else { "NIF" }, c.nif), ancho, normal));
        }
        let cp_ciudad = [c.cp.as_str(), c.ciudad.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        let pais = if c.pais.is_empty() || c.pais == "ES" { "" } else { c.pais.as_str() };
        for x in [c.direccion.as_str(), cp_ciudad.as_str(), pais, c.email.as_str()] {
            if !x.is_empty() && !corto {
                v.extend(kv("", x, ancho, normal));
            }
        }
    }
    v.push(Line::from(""));
    v.extend(kv("fecha", &fecha_corta(&f.fecha), ancho, normal));
    if !f.fecha_operacion.is_empty() && f.fecha_operacion != f.fecha {
        v.extend(kv("f. operación", &fecha_corta(&f.fecha_operacion), ancho, normal));
    }
    if !f.vencimiento.is_empty() {
        v.extend(kv("vence", &fecha_corta(&f.vencimiento), ancho, normal));
    }
    let op = opciones_operacion().into_iter().find(|o| o.0 == f.operacion).map(|o| o.1).unwrap_or("");
    v.extend(kv("operación", op, ancho, normal));
    v.push(Line::from(""));
    if f.lineas.is_empty() {
        v.push(Line::styled("Sin líneas", tema::apagado()));
    }
    for l in &f.lineas {
        let concepto = if l.concepto.trim().is_empty() { "(sin concepto)" } else { l.concepto.as_str() };
        v.extend(lineas(concepto, ancho as usize).into_iter().map(Line::from));
        let iva = if nacional { format!(" · IVA {}", pct(l.iva.v())) } else { String::new() };
        let izq = format!("   {} × {}{iva}", cant(l.cantidad.v()), eur(l.precio.v()));
        v.push(fila(izq, eur(fmt::r2(l.cantidad.v() * l.precio.v())), ancho, tema::apagado(), normal));
    }
    v.push(Line::from(""));
    v.extend(totales(f, ancho));
    if let Some(m) = mencion_iva(f) {
        v.push(Line::from(""));
        v.extend(lineas(&m, ancho as usize).into_iter().map(|t| Line::styled(t, tema::apagado())));
    }
    v.push(Line::from(""));
    if !f.forma_pago.is_empty() {
        v.extend(kv("forma de pago", &f.forma_pago, ancho, normal));
    }
    if !f.notas.is_empty() {
        v.extend(kv("notas", &f.notas, ancho, normal));
    }
    if let Some(cb) = f.cobrada.as_deref().filter(|s| !s.is_empty()) {
        v.extend(kv("cobrada", &fecha_corta(cb), ancho, Style::new().fg(tema::BIEN)));
    } else if f.emitida() {
        let t = if cerrado { "todavía no" } else { "todavía no (c para marcarla)" };
        v.extend(kv("cobrada", t, ancho, Style::new().fg(tema::AVISO)));
    }
    if !f.doc.is_empty() {
        v.extend(kv("documento", &format!("{} (o abrir)", f.doc), ancho, normal));
    }
    if f.borrador() {
        v.push(Line::styled("Borrador: Enter para editarlo, E para emitirlo.", tema::apagado()));
    }
    v
}

/// Los totales de una factura con puntos guía, con las cifras acabando en `ancho`: la base, el IVA (por tipos), la
/// retención y el total a cobrar, en negrita. Los usan el detalle y el editor.
pub(super) fn totales(f: &Factura, ancho: u16) -> Vec<Line<'static>> {
    let t = totales_factura(f);
    let normal = Style::new();
    let negrita = normal.add_modifier(Modifier::BOLD);
    let linea = |etq: String, cifra: String, st: Style| guia(Span::styled(etq, st), cifra, st, ancho);
    let mut v = vec![linea("base imponible".into(), eur(t.base), normal)];
    if f.operacion == "nacional" {
        for g in &t.desglose {
            let sobre = if t.desglose.len() > 1 { format!(" s/ {}", eur(g.base)) } else { String::new() };
            v.push(linea(format!("IVA {}{sobre}", pct(g.tipo)), eur(g.cuota), normal));
        }
    } else {
        v.push(linea("IVA".into(), eur(0.0), normal));
    }
    if t.retencion != 0.0 {
        v.push(linea("importe total".into(), eur(t.importe_total), tema::apagado()));
        v.push(linea(format!("retención IRPF {}", pct(f.retencion.v())), format!("−{}", eur(t.retencion)), normal));
    }
    v.push(linea("total a cobrar".into(), eur(t.total), negrita));
    v
}

/* ================= la pantalla ================= */

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Filtro {
    #[default]
    Todas,
    Pendientes,
    Borradores,
}

impl Filtro {
    fn siguiente(self) -> Self {
        match self {
            Filtro::Todas => Filtro::Pendientes,
            Filtro::Pendientes => Filtro::Borradores,
            Filtro::Borradores => Filtro::Todas,
        }
    }
    fn nombre(self) -> &'static str {
        match self {
            Filtro::Todas => "Todas",
            Filtro::Pendientes => "Pendientes de cobro",
            Filtro::Borradores => "Borradores",
        }
    }
    fn deja(self, f: &Factura) -> bool {
        match self {
            Filtro::Todas => true,
            Filtro::Pendientes => f.emitida() && !f.cobrada(),
            Filtro::Borradores => f.borrador(),
        }
    }
}

/// Lo que se hace si se responde que sí en la ventana de confirmar.
enum Accion {
    Emitir(Box<Factura>),
    Borrar(String),
    /// Salir del editor sin guardar.
    Descartar,
}

/// Columnas de la lista, en su orden (se quitan las menos importantes si no caben).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Col {
    Numero,
    Fecha,
    Cliente,
    Base,
    Iva,
    Retencion,
    Total,
    Estado,
    Doc,
}

#[derive(Default)]
pub struct Facturas {
    sel: usize,
    tabla: TableState,
    filtro: Filtro,
    busca: Entrada,
    buscando: bool,
    /// El detalle a pantalla completa (Enter en una emitida).
    ver: bool,
    scroll: u16,
    /// Hasta dónde se puede bajar en el detalle.
    max_scroll: u16,
    editor: Option<Box<Editor>>,
    /// Fecha de cobro que se está pidiendo (id de la factura).
    cobro: Option<(Formulario, String)>,
    confirmar: Option<(Confirmar, Accion)>,
    /// La lista de tus clientes, abierta encima.
    clientes: Option<Clientes>,
    /// Filas que caben en la tabla (para PgUp / PgDn).
    alto: usize,
    /// La elegida es un borrador (Some(true)), una emitida (Some(false)) o no hay (None): para los atajos.
    elegida: Option<bool>,
    /// El año que se ve está cerrado: solo se miran (del último dibujo, para los atajos).
    cerrado: bool,
}

impl Facturas {
    /// Índices (en `d.facturas`) de las que se ven: emitidas primero, de la más nueva a la
    /// más antigua, y después los borradores (de cualquier año; en un año cerrado no salen: no se pueden tocar).
    fn lista(&self, d: &Datos, anio: i32) -> Vec<usize> {
        let abierto = !autonomo::cerrado(d, anio);
        let mut v: Vec<usize> = (0..d.facturas.len())
            .filter(|&i| {
                let f = &d.facturas[i];
                (if f.borrador() { abierto } else { fmt::anio_de(&f.fecha) == anio }) && self.filtro.deja(f) && {
                    let conceptos: Vec<&str> = f.lineas.iter().map(|l| l.concepto.as_str()).collect();
                    coincide(&format!("{} {} {} {}", f.numero, f.cliente.nombre, f.cliente.nif, conceptos.join(" ")), &self.busca.valor)
                }
            })
            .collect();
        v.sort_by(|&a, &b| {
            let (x, y) = (&d.facturas[a], &d.facturas[b]);
            x.borrador().cmp(&y.borrador()).then_with(|| y.fecha.cmp(&x.fecha)).then_with(|| y.numero.cmp(&x.numero))
        });
        v
    }

    /// La factura elegida (índice en `d.facturas`).
    fn elegida(&self, ctx: &Ctx) -> Option<usize> {
        self.lista(&ctx.datos, ctx.anio).get(self.sel).copied()
    }

    fn ir_a(&mut self, ctx: &Ctx, id: &str) {
        if let Some(p) = self.lista(&ctx.datos, ctx.anio).iter().position(|&i| ctx.datos.facturas[i].id == id) {
            self.sel = p;
        }
    }

    fn mover(&mut self, d: isize, n: usize) {
        self.sel = if n == 0 { 0 } else { (self.sel as isize + d).clamp(0, n as isize - 1) as usize };
        self.scroll = 0;
    }

    /// Abre tus clientes, con el de la factura elegida (si está guardado; si no, se dice).
    fn abrir_clientes(&mut self, ctx: &mut Ctx) {
        let f = self.elegida(ctx).map(|i| &ctx.datos.facturas[i]);
        let c = Clientes::de(&ctx.datos, f);
        if let Some(f) = f.filter(|f| !f.cliente.nombre.trim().is_empty() && !ctx.datos.clientes.iter().any(|c| clientes::es_del_cliente(f, c))) {
            let nombre = f.cliente.nombre.clone();
            ctx.info(format!("«{nombre}» no está en tus clientes: se guarda al emitir la factura (o con n)."));
        }
        self.clientes = Some(c);
    }

    fn editar(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.elegida(ctx) else { return ctx.info("No hay ninguna factura elegida.") };
        let f = &ctx.datos.facturas[i];
        if f.emitida() {
            ctx.error(format!("La factura {} está emitida y no se puede modificar: si hay un error, hace falta una rectificativa.", f.numero));
        } else if f.rectifica.is_some() {
            ctx.error("Las rectificativas aún no se pueden cambiar en la app.");
        } else {
            self.editor = Some(Box::new(Editor::abrir(f, &ctx.datos)));
        }
    }

    /// Comprueba que se puede emitir y pide confirmación.
    fn pedir_emitir(&mut self, ctx: &mut Ctx, f: Factura) {
        if let Some(e) = problema_emision(&ctx.datos, &f) {
            return ctx.error(e);
        }
        let y = fmt::anio_de(&f.fecha);
        if autonomo::cerrado(&ctx.datos, y) {
            return ctx.error(format!("{} La factura tiene que llevar una fecha de un año abierto.", autonomo::aviso_cerrado(y)));
        }
        let d = &ctx.datos;
        let anio = fmt::anio_de(&f.fecha);
        let numero = siguiente_numero(d, false, anio);
        let serie = serie_factura(d, false, anio);
        let ultima = d.facturas.iter().filter(|x| x.emitida() && !x.numero.is_empty() && x.numero.starts_with(&serie)).map(|x| x.fecha.as_str()).max().unwrap_or("");
        let t = totales_factura(&f);
        let cliente = if f.cliente.nombre.trim().is_empty() { "Consumidor final" } else { f.cliente.nombre.as_str() };
        let mut c = Confirmar::new(format!("Emitir factura {numero}"), "Emitir")
            .linea(format!("Número:          {numero}"))
            .linea(format!("Cliente:         {cliente}"))
            .linea(format!("Fecha:           {}", fecha_corta(&f.fecha)))
            .linea(format!("Total a cobrar:  {}", eur(t.total)));
        if !ultima.is_empty() && f.fecha.as_str() < ultima {
            c = c.linea("").linea("¡Ojo! La fecha es anterior a la de tu última factura de la serie").linea(format!("({}). Las facturas tienen que ir en orden.", fecha_corta(ultima)));
        }
        c = c.linea("").linea("Una vez emitida no se puede modificar: si te equivocas,").linea("se corrige con una factura rectificativa.");
        self.confirmar = Some((c, Accion::Emitir(Box::new(f))));
    }

    fn cobrar(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.elegida(ctx) else { return };
        let f = &mut ctx.datos.facturas[i];
        if f.borrador() {
            return ctx.error("Es un borrador: emítela antes de marcarla como cobrada.");
        }
        if f.cobrada() {
            f.cobrada = None;
            let n = f.numero.clone();
            if ctx.guardar() {
                ctx.ok(format!("Factura {n} marcada como no cobrada"));
            }
            return;
        }
        let fm = Formulario::new(format!("Cobrada · {}", f.numero)).fecha("fecha", "Fecha de cobro", &dmy(&fmt::hoy())).ayuda("Enter: cobrada en esa fecha.");
        self.cobro = Some((fm, f.id.clone()));
    }

    fn pedir_borrar(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.elegida(ctx) else { return };
        let f = &ctx.datos.facturas[i];
        if !f.borrador() {
            return ctx.error("Las facturas emitidas no se borran: si hay un error, hace falta una rectificativa.");
        }
        let quien = if f.cliente.nombre.is_empty() { String::new() } else { format!(" de {}", f.cliente.nombre) };
        let c = Confirmar::new("Borrar borrador", "Borrar").peligro().linea(format!("Se eliminará este borrador{quien}.")).linea("No afecta a ninguna factura emitida.");
        self.confirmar = Some((c, Accion::Borrar(f.id.clone())));
    }

    fn hacer(&mut self, acc: Accion, ctx: &mut Ctx) {
        match acc {
            Accion::Emitir(f) => {
                let id = f.id.clone();
                emitir(ctx, *f);
                if self.editor.as_ref().is_some_and(|e| e.f.id == id) {
                    self.editor = None;
                }
                self.ir_a(ctx, &id);
            }
            Accion::Borrar(id) => {
                ctx.datos.facturas.retain(|f| f.id != id);
                if ctx.guardar() {
                    ctx.ok("Borrador borrado");
                }
                self.ver = false;
            }
            Accion::Descartar => {
                self.editor = None;
                ctx.info("Sin guardar");
            }
        }
    }

    fn tecla_editor(&mut self, mut ed: Box<Editor>, k: KeyEvent, ctx: &mut Ctx) {
        match ed.tecla(k, ctx) {
            Salida::Nada => self.editor = Some(ed),
            Salida::Guardar => match ed.leer() {
                Err(e) => {
                    ed.poner_error(e.clone());
                    ctx.error(e);
                    self.editor = Some(ed);
                }
                Ok(()) => {
                    let id = guardar_borrador(ctx, ed.f.clone());
                    self.ver = false;
                    self.ir_a(ctx, &id);
                }
            },
            Salida::Emitir => {
                let r = ed.leer();
                let f = ed.f.clone();
                self.editor = Some(ed);
                match r {
                    Err(e) => {
                        if let Some(ed) = &mut self.editor {
                            ed.poner_error(e.clone());
                        }
                        ctx.error(e);
                    }
                    Ok(()) => self.pedir_emitir(ctx, f),
                }
            }
            Salida::Salir => {
                if ed.cambiado() {
                    let c = Confirmar::new("Salir sin guardar", "Salir sin guardar").peligro().linea("Los cambios de este borrador se perderán.").linea("Para guardarlos: Esc/n y luego Ctrl+S.");
                    self.confirmar = Some((c, Accion::Descartar));
                    self.editor = Some(ed);
                }
            }
        }
    }

    /// Arriba de la lista: los filtros (con su tecla delante y el elegido en FONDO sobre el color de la sección), el
    /// año y cuántas se ven y, a la derecha si caben, las teclas de buscar y de los clientes (abajo no caben todas);
    /// en la fila de debajo, si se busca, la búsqueda.
    fn dibujar_filtros(&self, f: &mut Frame, area: Rect, ctx: &Ctx, n: usize) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        // La «f» delante: es la que pasa de un filtro a otro.
        let mut v = vec![Span::styled("f ", negrita)];
        for (i, x) in [Filtro::Todas, Filtro::Pendientes, Filtro::Borradores].into_iter().enumerate() {
            if i > 0 {
                v.push(Span::raw("  "));
            }
            // Todas con su hueco a cada lado: al cambiar de filtro no se mueve nada.
            v.push(Span::styled(format!(" {} ", x.nombre()), if x == self.filtro { tema::pestana_activa() } else { gris }));
        }
        v.extend([Span::raw("     "), Span::styled(ctx.anio.to_string(), negrita), Span::styled(format!(" · {}", plural(n)), gris)]);
        let usado = Line::from(v.clone()).width();
        let fila = Rect { height: area.height.min(1), ..area };
        f.render_widget(Paragraph::new(Line::from(v)), fila);
        let teclas = linea_atajos(&[("/", "buscar"), ("C", "clientes")]);
        if !self.buscando && usado + 5 + teclas.width() <= area.width as usize {
            f.render_widget(Paragraph::new(teclas.right_aligned()), fila);
        }
        if (self.buscando || !self.busca.valor.is_empty()) && area.height >= 2 {
            let fila = Rect { y: area.y + 1, height: 1, ..area };
            let [e, t] = Layout::horizontal([Constraint::Length(7), Constraint::Min(1)]).areas(fila);
            f.render_widget(Paragraph::new(Span::styled("buscar", gris)), e);
            self.busca.dibujar(f, t, self.buscando, negrita);
        }
    }

    /// Lo facturado en el año (por la fecha que cuenta para los impuestos, como el 303 y el 130), con puntos guía:
    /// la base, el IVA, las retenciones y lo pendiente de cobro (en AVISO si hay), cada cifra con su nota en gris.
    fn dibujar_resumen(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let (mut base, mut cuota, mut ret, mut pend, mut n, mut np) = (0.0, 0.0, 0.0, 0.0, 0, 0);
        for x in factura::emitidas(&ctx.datos).filter(|x| fmt::anio_de(x.devengo()) == ctx.anio) {
            let t = totales_factura(x);
            base += t.base;
            cuota += t.cuota;
            ret += t.retencion;
            n += 1;
            if !x.cobrada() {
                pend += t.total;
                np += 1;
            }
        }
        let st_pend = if pend > 0.005 { Style::new().fg(tema::AVISO) } else { Style::new() };
        let filas = [
            (format!("facturado en {}", ctx.anio), eur(base), Style::new(), format!("base de {}", plural(n))),
            ("IVA repercutido".to_string(), eur(cuota), Style::new(), "va al modelo 303".to_string()),
            ("retenciones".to_string(), eur(ret), Style::new(), "a cuenta del IRPF".to_string()),
            ("pendiente de cobro".to_string(), eur(pend), st_pend, format!("de {}", plural(np))),
        ];
        // Todas las cifras acaban en la misma columna, con al menos unos puntos.
        let fin = filas.iter().map(|(e, c, ..)| e.chars().count() + c.chars().count() + 6).max().unwrap_or(0) as u16;
        let v: Vec<Line> = filas
            .into_iter()
            .map(|(e, c, st, nota)| {
                let mut l = guia(e, c, st, fin);
                l.push_span(Span::styled(format!("   {nota}"), tema::apagado()));
                l
            })
            .collect();
        f.render_widget(Paragraph::new(v), area);
    }

    /// La lista, sin caja: la cabecera de las columnas en gris, las cifras a la derecha (sin «€»: ya lo dice el
    /// resumen) y la elegida en el fondo suave de la sección. Si no caben todas las columnas, se quitan las menos
    /// importantes.
    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, lista: &[usize]) {
        let d = &ctx.datos;
        if lista.is_empty() {
            let txt = if self.filtro == Filtro::Todas && self.busca.valor.is_empty() {
                if self.cerrado {
                    format!("No hay facturas de {} en la app. Es un año cerrado: se ve en Impuestos, con tu Renta.", ctx.anio)
                } else {
                    format!("Todavía no hay facturas en {}. Pulsa n para crear la primera.", ctx.anio)
                }
            } else {
                "No hay facturas en esta vista.".to_string()
            };
            f.render_widget(Paragraph::new(Line::styled(txt, tema::apagado())).wrap(Wrap { trim: false }), area);
            return;
        }
        let hoy = fmt::hoy();
        let facturas: Vec<&Factura> = lista.iter().map(|&i| &d.facturas[i]).collect();
        let tot: Vec<_> = facturas.iter().map(|x| totales_factura(x)).collect();
        let estados: Vec<(String, Style)> = facturas.iter().map(|x| estado(x, &hoy)).map(|(e, st)| (e.to_lowercase(), st)).collect();
        // El ancho de cada columna: lo más largo de ella, o su título.
        fn largo(v: impl Iterator<Item = String>, titulo: &str) -> u16 {
            v.map(|s| s.chars().count()).max().unwrap_or(0).max(titulo.chars().count()) as u16
        }
        let ancho = |c: Col| match c {
            Col::Numero => largo(facturas.iter().map(|x| x.numero.clone()), "borrador").min(14),
            Col::Fecha => largo(facturas.iter().map(|x| fecha_lista(&x.fecha, ctx.anio)), "fecha"),
            Col::Cliente => 16,
            Col::Base => largo(tot.iter().map(|t| fmt::num(t.base)), "base"),
            Col::Iva => largo(tot.iter().map(|t| fmt::num(t.cuota)), "IVA"),
            Col::Retencion => largo(tot.iter().map(|t| fmt::num(t.retencion)), "retención"),
            Col::Total => largo(tot.iter().map(|t| fmt::num(t.total)), "total"),
            Col::Estado => largo(estados.iter().map(|e| e.0.clone()), "estado"),
            Col::Doc => 1,
        };
        // Las que siempre van y, mientras quepan (con el cliente a 16 como poco), las demás por orden de importancia.
        const ESPACIO: u16 = 2;
        let mut cols = vec![Col::Numero, Col::Cliente, Col::Total, Col::Estado];
        if facturas.iter().any(|x| !x.doc.is_empty()) {
            cols.push(Col::Doc);
        }
        for c in [Col::Fecha, Col::Base, Col::Iva, Col::Retencion] {
            let ocupa: u16 = cols.iter().chain([&c]).map(|&c| ancho(c) + ESPACIO).sum();
            if ocupa <= area.width {
                cols.push(c);
            }
        }
        cols.sort();
        let titulo = |c: Col| match c {
            Col::Numero => Cell::from("número"),
            Col::Fecha => Cell::from("fecha"),
            Col::Cliente => Cell::from("cliente"),
            Col::Base => Cell::from(derecha("base")),
            Col::Iva => Cell::from(derecha("IVA")),
            Col::Retencion => Cell::from(derecha("retención")),
            Col::Total => Cell::from(derecha("total")),
            Col::Estado => Cell::from("estado"),
            Col::Doc => Cell::from(""),
        };
        let filas: Vec<Row> = facturas
            .iter()
            .zip(&tot)
            .zip(&estados)
            .map(|((x, t), (e, st))| {
                Row::new(cols.iter().map(|&c| match c {
                    Col::Numero if x.numero.is_empty() => Cell::from(Span::styled("borrador", tema::apagado())),
                    Col::Numero => Cell::from(x.numero.clone()),
                    Col::Fecha => Cell::from(Span::styled(fecha_lista(&x.fecha, ctx.anio), tema::apagado())),
                    Col::Cliente if x.cliente.nombre.trim().is_empty() => Cell::from(Span::styled("Consumidor final", tema::apagado())),
                    Col::Cliente => Cell::from(x.cliente.nombre.clone()),
                    Col::Base => Cell::from(derecha(fmt::num(t.base))),
                    Col::Iva => Cell::from(derecha(fmt::num(t.cuota))),
                    Col::Retencion => Cell::from(derecha(fmt::num(t.retencion))),
                    Col::Total => Cell::from(derecha(fmt::num(t.total))),
                    Col::Estado => Cell::from(Span::styled(e.clone(), *st)),
                    Col::Doc => Cell::from(Span::styled(if x.doc.is_empty() { "" } else { "▤" }, tema::apagado())),
                }))
            })
            .collect();
        let anchos = cols.iter().map(|&c| if c == Col::Cliente { Constraint::Fill(1) } else { Constraint::Length(ancho(c)) });
        let tabla = Table::new(filas, anchos).header(Row::new(cols.iter().map(|&c| titulo(c))).style(tema::cabecera_tabla())).column_spacing(ESPACIO).row_highlight_style(tema::seleccion());
        self.tabla.select(Some(self.sel));
        self.alto = area.height.saturating_sub(1).max(1) as usize;
        f.render_stateful_widget(tabla, area, &mut self.tabla);
    }

    /// El detalle de una factura, sin caja: a pantalla completa (Enter) o en el panel de la derecha (`corto`). Se
    /// desplaza con ↑ ↓.
    fn dibujar_detalle(&mut self, f: &mut Frame, area: Rect, x: &Factura, corto: bool) {
        // A pantalla completa, no más ancho de lo que se lee bien.
        let dentro = Rect { width: area.width.min(60), ..area };
        let t = texto_detalle(x, dentro.width, corto, self.cerrado);
        // Filas que ocupa con las líneas largas partidas (aproximado: basta para no bajar de más).
        let w = dentro.width.max(1) as usize;
        let filas: usize = t.iter().map(|l| l.width().div_ceil(w).max(1)).sum();
        self.max_scroll = filas.saturating_sub(dentro.height as usize) as u16;
        self.scroll = self.scroll.min(self.max_scroll);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }).scroll((self.scroll, 0)), dentro);
    }
}

impl Pantalla for Facturas {
    fn titulo(&self) -> &'static str {
        "Facturas"
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.cerrado = autonomo::cerrado(&ctx.datos, ctx.anio);
        let n = self.lista(&ctx.datos, ctx.anio).len();
        self.sel = self.sel.min(n.saturating_sub(1));
        self.scroll = 0;
    }

    fn capturando(&self) -> bool {
        self.editor.is_some() || self.confirmar.is_some() || self.cobro.is_some() || self.buscando || self.clientes.as_ref().is_some_and(Clientes::capturando)
    }

    /// Los clientes no son de un año.
    fn usa_anio(&self) -> bool {
        self.clientes.is_none()
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.cerrado = autonomo::cerrado(&ctx.datos, ctx.anio);
        if let Some(c) = &mut self.clientes {
            return c.dibujar(f, area, ctx);
        }
        if let Some(ed) = &mut self.editor {
            ed.dibujar(f, area, ctx);
        } else {
            let d = &ctx.datos;
            let lista = self.lista(d, ctx.anio);
            self.sel = self.sel.min(lista.len().saturating_sub(1));
            let actual = lista.get(self.sel).map(|&i| &d.facturas[i]);
            self.elegida = actual.map(|x| x.borrador());
            if self.ver && actual.is_none() {
                self.ver = false;
            }
            match actual {
                Some(x) if self.ver => self.dibujar_detalle(f, area, x, false),
                _ => {
                    // Arriba, los filtros (y la búsqueda); debajo, el resumen del año, si cabe; y la lista, con el
                    // detalle de la elegida a la derecha si hay sitio.
                    let [a_fil, resto] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
                    self.dibujar_filtros(f, a_fil, ctx, lista.len());
                    let resto = if area.height >= 16 {
                        let [a_res, _, resto] = Layout::vertical([Constraint::Length(4), Constraint::Length(1), Constraint::Min(1)]).areas(resto);
                        self.dibujar_resumen(f, a_res, ctx);
                        resto
                    } else {
                        resto
                    };
                    if area.width >= 110 {
                        let w_panel = (area.width * 35 / 100).clamp(40, 56);
                        let [a_l, _, a_p] = Layout::horizontal([Constraint::Min(40), Constraint::Length(4), Constraint::Length(w_panel)]).areas(resto);
                        self.dibujar_lista(f, a_l, ctx, &lista);
                        if let Some(x) = actual {
                            self.dibujar_detalle(f, a_p, x, true);
                        }
                    } else {
                        self.dibujar_lista(f, resto, ctx, &lista);
                    }
                }
            }
        }
        if let Some((fm, _)) = &self.cobro {
            fm.dibujar_ventana(f, area, 56);
        }
        if let Some((c, _)) = &self.confirmar {
            c.dibujar(f, area);
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if let Some((c, acc)) = self.confirmar.take() {
            match c.tecla(k) {
                Respuesta::Si => self.hacer(acc, ctx),
                Respuesta::No => {}
                Respuesta::Sigue => self.confirmar = Some((c, acc)),
            }
            return true;
        }
        if let Some((mut fm, id)) = self.cobro.take() {
            match fm.tecla(k) {
                Resultado::Guardar => match fm.fecha_iso("fecha") {
                    None => {
                        fm.error = Some("Pon una fecha válida (19/10/2026).".into());
                        self.cobro = Some((fm, id));
                    }
                    Some(fecha) => {
                        if let Some(x) = ctx.datos.facturas.iter_mut().find(|x| x.id == id) {
                            x.cobrada = Some(fecha.clone());
                            let n = x.numero.clone();
                            if ctx.guardar() {
                                ctx.ok(format!("Factura {n} cobrada el {}", fecha_corta(&fecha)));
                            }
                        }
                    }
                },
                Resultado::Cancelar => {}
                Resultado::Sigue | Resultado::Cambio => self.cobro = Some((fm, id)),
            }
            return true;
        }
        if let Some(ed) = self.editor.take() {
            self.tecla_editor(ed, k, ctx);
            return true;
        }
        if let Some(c) = &mut self.clientes {
            return match c.tecla(k, ctx) {
                clientes::Tecla::Usada => true,
                clientes::Tecla::NoUsada => false,
                clientes::Tecla::Cerrar => {
                    self.clientes = None;
                    true
                }
            };
        }
        let n = self.lista(&ctx.datos, ctx.anio).len();
        if self.buscando {
            match k.code {
                KeyCode::Esc => {
                    self.busca = Entrada::default();
                    self.buscando = false;
                }
                KeyCode::Enter => self.buscando = false,
                KeyCode::Up => self.mover(-1, n),
                KeyCode::Down => self.mover(1, n),
                _ => {
                    if self.busca.tecla(k) {
                        self.sel = 0;
                    }
                }
            }
            return true;
        }
        let pagina = self.alto.max(1) as isize;
        if self.ver {
            // En el detalle: ↑↓ bajan y suben, ← → pasan a otra factura.
            let d = match k.code {
                KeyCode::Up => Some(-1),
                KeyCode::Down => Some(1),
                KeyCode::PageUp => Some(-10),
                KeyCode::PageDown => Some(10),
                _ => None,
            };
            if let Some(d) = d {
                // El tope de abajo lo pone `dibujar_detalle` (depende del ancho).
                self.scroll = (self.scroll.min(self.max_scroll) as i32 + d).max(0) as u16;
                return true;
            }
            match k.code {
                KeyCode::Left | KeyCode::Right => {
                    self.mover(if k.code == KeyCode::Left { -1 } else { 1 }, n);
                    return true;
                }
                _ => {}
            }
        }
        // Un año cerrado se mira, pero no se crean, emiten, cobran, duplican ni borran facturas.
        if autonomo::cerrado(&ctx.datos, ctx.anio) && matches!(k.code, KeyCode::Char('n' | 'E' | 'c' | 'D' | 'd') | KeyCode::Delete) {
            ctx.error(autonomo::aviso_cerrado(ctx.anio));
            return true;
        }
        match k.code {
            KeyCode::Up => self.mover(-1, n),
            KeyCode::Down => self.mover(1, n),
            KeyCode::PageUp => self.mover(-pagina, n),
            KeyCode::PageDown => self.mover(pagina, n),
            KeyCode::Home => self.mover(-(n as isize), n),
            KeyCode::End => self.mover(n as isize, n),
            KeyCode::Char('/') => {
                self.buscando = true;
                self.ver = false;
            }
            KeyCode::Char('f') => {
                self.filtro = self.filtro.siguiente();
                self.sel = 0;
                self.ver = false;
            }
            KeyCode::Char('n') => {
                self.ver = false;
                self.editor = Some(Box::new(Editor::nueva(nueva_factura(&ctx.datos), &ctx.datos)));
            }
            KeyCode::Enter => match self.elegida(ctx) {
                Some(i) if ctx.datos.facturas[i].borrador() => self.editar(ctx),
                Some(_) => {
                    self.ver = !self.ver;
                    self.scroll = 0;
                }
                None if self.cerrado => ctx.info(format!("No hay facturas de {} en la app.", ctx.anio)),
                None => ctx.info("No hay facturas aquí: pulsa n para crear una."),
            },
            KeyCode::Esc if self.ver => self.ver = false,
            KeyCode::Esc if !self.busca.valor.is_empty() => {
                self.busca = Entrada::default();
                self.sel = 0;
            }
            KeyCode::Char('E') => {
                if let Some(i) = self.elegida(ctx) {
                    let x = ctx.datos.facturas[i].clone();
                    self.pedir_emitir(ctx, x);
                }
            }
            KeyCode::Char('c') => self.cobrar(ctx),
            KeyCode::Char('D') => {
                if let Some(i) = self.elegida(ctx) {
                    let o = &ctx.datos.facturas[i];
                    let de = if o.numero.is_empty() { "un borrador".to_string() } else { o.numero.clone() };
                    let copia = duplicada(&ctx.datos, o);
                    self.editor = Some(Box::new(Editor::copia(copia, &de, &ctx.datos)));
                    self.ver = false;
                    ctx.info("Copia con la fecha de hoy: revísala y guárdala con Ctrl+S.");
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => self.pedir_borrar(ctx),
            KeyCode::Char('o') => {
                if let Some(i) = self.elegida(ctx) {
                    let doc = ctx.datos.facturas[i].doc.clone();
                    ctx.abrir_doc(&doc);
                }
            }
            KeyCode::Char('R') => ctx.info("Las rectificativas aún no se pueden hacer en la app."),
            KeyCode::Char('C') => self.abrir_clientes(ctx),
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if let Some(c) = &self.clientes {
            return c.atajos();
        }
        if self.confirmar.is_some() {
            return vec![("Enter/s", "sí"), ("Esc/n", "cancelar")];
        }
        if self.cobro.is_some() {
            return vec![("Enter", "marcar cobrada"), ("Esc", "cancelar")];
        }
        if let Some(ed) = &self.editor {
            return ed.atajos();
        }
        if self.buscando {
            return vec![("Enter", "aceptar"), ("Esc", "borrar la búsqueda"), ("↑↓", "mover")];
        }
        if self.cerrado {
            let mut v = match self.elegida {
                Some(_) if self.ver => vec![("← →", "otra"), ("↑↓", "bajar"), ("Esc", "volver"), ("o", "doc"), ("C", "su cliente")],
                Some(_) => vec![("Enter", "ver"), ("o", "doc")],
                None => vec![],
            };
            if !self.ver {
                v.extend([("f", "filtro"), ("/", "buscar"), ("C", "clientes")]);
            }
            return v;
        }
        let mut v = match self.elegida {
            Some(true) => vec![("Enter", "editar"), ("E", "emitir"), ("d", "borrar"), ("D", "duplicar")],
            Some(false) if self.ver => vec![("← →", "otra"), ("Esc", "volver"), ("c", "cobrada"), ("D", "duplicar"), ("o", "doc"), ("C", "su cliente")],
            Some(false) => vec![("Enter", "ver"), ("c", "cobrada"), ("D", "duplicar"), ("o", "doc")],
            None => vec![],
        };
        v.insert(0, ("n", "nueva"));
        if !self.ver {
            v.extend([("f", "filtro"), ("/", "buscar"), ("C", "clientes")]);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::super::prueba::{app_con, cargar, caso, ir, pantalla, teclas};
    use crate::dominio::fmt::anio_actual;

    /// Al estilo C2, sin cajas: los filtros arriba (con su tecla), el resumen del año con puntos guía, la cabecera
    /// de las columnas en minúscula y las cifras sin «€»; Enter, el detalle con los totales; y n, el editor.
    #[test]
    fn la_lista_el_detalle_y_el_editor() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        super::super::tests::factura_de_hoy(&mut d);
        let mut a = app_con(d, &dir);
        ir(&mut a, "Facturas");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("f  Todas    Pendientes de cobro    Borradores ") && p.contains(&format!("facturado en {} ··", anio_actual())), "{p}");
        assert!(p.contains("pendiente de cobro ··") && !p.contains('│') && !p.contains('╭'), "{p}");
        let cabecera = p.lines().find(|l| l.contains("número")).unwrap_or_default();
        assert!(cabecera.contains("cliente") && cabecera.contains("total") && cabecera.contains("estado"), "{p}");
        let fila = p.lines().find(|l| l.contains("HOY-1")).unwrap_or_default();
        assert!(fila.contains("Zeta Pruebas") && fila.contains(" 121,00  ") && fila.ends_with("pendiente"), "{p}");
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Factura HOY-1   Emitida · pendiente") && p.contains("total a cobrar ··") && p.contains(" 121,00 €"), "{p}");
        teclas(&mut a, "<esc>n<esc>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Nueva factura (borrador)") && p.contains("› Cliente") && p.contains("líneas (0)"), "{p}");
        // Y en un terminal estrecho, también.
        assert!(pantalla(&mut a, 80, 24).contains("líneas (0)"));
    }
}
