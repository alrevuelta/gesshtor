//! Facturas: las del año y todos los borradores, el detalle, el editor de borradores,
//! emitir, cobrar y duplicar.

use super::clientes::{limpia_nif, opciones_operacion, validar_nif};
use crate::dominio::factura::{self, mencion_iva, serie_factura, siguiente_numero, totales_factura};
use crate::dominio::fmt::{self, dmy, eur, fecha_corta, pct};
use crate::dominio::{uid, Cliente, Datos, Factura, Linea, Num};
use crate::informe::autonomo;
use crate::tui::widgets::{bloque, coincide, derecha, kpi, tema, Confirmar, Entrada, Formulario, Respuesta, Resultado};
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

/* ================= reglas (las de la web) ================= */

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

/// Al emitir, el cliente queda guardado en Clientes.
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

/// Estado para la lista: Borrador, Pendiente, Vencida, Cobrada dd/mm.
fn estado(f: &Factura, hoy: &str) -> (String, Style) {
    if f.borrador() {
        ("Borrador".into(), tema::apagado())
    } else if let Some(c) = f.cobrada.as_deref().filter(|c| !c.is_empty()) {
        let dm = fmt::fecha(c).map(|d| d.format("%d/%m").to_string()).unwrap_or_default();
        (format!("Cobrada {dm}"), Style::new().fg(tema::BIEN))
    } else if !f.vencimiento.is_empty() && f.vencimiento.as_str() < hoy {
        ("Vencida".into(), Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD))
    } else {
        ("Pendiente".into(), Style::new().fg(tema::AVISO))
    }
}

fn plural(n: usize) -> String {
    if n == 1 { "1 factura".into() } else { format!("{n} facturas") }
}

/// Todo lo de una factura, para la vista completa; `corto` (el panel de la derecha) deja fuera la dirección
/// del cliente.
fn texto_detalle(f: &Factura, ancho: u16, corto: bool, cerrado: bool) -> Vec<Line<'static>> {
    let t = totales_factura(f);
    let hoy = fmt::hoy();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let nacional = f.operacion == "nacional";
    let c = &f.cliente;
    let mut v: Vec<Line> = Vec::new();
    let (est, st) = estado(f, &hoy);
    let titulo = if f.numero.is_empty() { "Borrador".to_string() } else { format!("Factura {}", f.numero) };
    let est = if f.emitida() && !f.cobrada() { format!("Emitida · {}", est.to_lowercase()) } else { est };
    v.push(Line::from(vec![Span::styled(titulo, tema::titulo()), Span::raw("  "), Span::styled(est, st)]));
    if let Some(r) = &f.rectifica {
        v.push(Line::styled(format!("Rectifica la factura {}", if r.numero.is_empty() { "?" } else { &r.numero }), tema::apagado()));
    }
    v.push(Line::from(""));
    let normal = Style::new();
    if c.nombre.trim().is_empty() {
        v.extend(kv("Cliente", "Consumidor final (factura simplificada)", ancho, normal));
    } else {
        v.extend(kv("Cliente", &c.nombre, ancho, negrita));
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
    v.extend(kv("Fecha", &fecha_corta(&f.fecha), ancho, normal));
    if !f.fecha_operacion.is_empty() && f.fecha_operacion != f.fecha {
        v.extend(kv("F. operación", &fecha_corta(&f.fecha_operacion), ancho, normal));
    }
    if !f.vencimiento.is_empty() {
        v.extend(kv("Vence", &fecha_corta(&f.vencimiento), ancho, normal));
    }
    let op = opciones_operacion().into_iter().find(|o| o.0 == f.operacion).map(|o| o.1).unwrap_or("");
    v.extend(kv("Operación", op, ancho, normal));
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
    v.push(fila("Base imponible", eur(t.base), ancho, normal, normal));
    if nacional {
        for g in &t.desglose {
            let sobre = if t.desglose.len() > 1 { format!(" s/ {}", eur(g.base)) } else { String::new() };
            v.push(fila(format!("IVA {}{sobre}", pct(g.tipo)), eur(g.cuota), ancho, normal, normal));
        }
    } else {
        v.push(fila("IVA", eur(0.0), ancho, normal, normal));
    }
    if t.retencion != 0.0 {
        v.push(fila("Importe total", eur(t.importe_total), ancho, tema::apagado(), tema::apagado()));
        v.push(fila(format!("Retención IRPF {}", pct(f.retencion.v())), format!("−{}", eur(t.retencion)), ancho, normal, normal));
    }
    v.push(fila("Total a cobrar", eur(t.total), ancho, negrita, negrita));
    if let Some(m) = mencion_iva(f) {
        v.push(Line::from(""));
        let cursiva = tema::apagado().add_modifier(Modifier::ITALIC);
        v.extend(lineas(&m, ancho as usize).into_iter().map(|t| Line::styled(t, cursiva)));
    }
    v.push(Line::from(""));
    if !f.forma_pago.is_empty() {
        v.extend(kv("Forma de pago", &f.forma_pago, ancho, normal));
    }
    if !f.notas.is_empty() {
        v.extend(kv("Notas", &f.notas, ancho, normal));
    }
    if let Some(cb) = f.cobrada.as_deref().filter(|s| !s.is_empty()) {
        v.extend(kv("Cobrada", &fecha_corta(cb), ancho, Style::new().fg(tema::BIEN)));
    } else if f.emitida() {
        let t = if cerrado { "todavía no" } else { "todavía no (c para marcarla)" };
        v.extend(kv("Cobrada", t, ancho, Style::new().fg(tema::AVISO)));
    }
    if !f.doc.is_empty() {
        v.extend(kv("Documento", &format!("{} (o abrir)", f.doc), ancho, normal));
    }
    if f.borrador() {
        v.push(Line::styled("Borrador: Enter para editarlo, E para emitirlo.", tema::apagado()));
    }
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

/// Columnas de la lista (se quitan las menos importantes si no caben).
#[derive(Debug, Clone, Copy, PartialEq)]
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

    fn dibujar_kpis(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let (mut base, mut cuota, mut ret, mut pend, mut n, mut np) = (0.0, 0.0, 0.0, 0.0, 0, 0);
        // Por la fecha que cuenta para los impuestos (la de la operación, si la hay), como el 303 y el 130.
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
        let [a, b, c, d] = Layout::horizontal([Constraint::Ratio(1, 4); 4]).areas(area);
        kpi(f, a, &format!("Facturado en {}", ctx.anio), &eur(base), &format!("{} (base)", plural(n)), Style::new());
        kpi(f, b, "IVA repercutido", &eur(cuota), "va al modelo 303", Style::new());
        kpi(f, c, "Retenciones", &eur(ret), "a cuenta del IRPF", Style::new());
        kpi(f, d, "Pendiente de cobro", &eur(pend), &plural(np), if pend > 0.005 { Style::new().fg(tema::AVISO) } else { Style::new() });
    }

    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, lista: &[usize]) {
        let d = &ctx.datos;
        let b = bloque("Facturas");
        let b = b.title(Span::styled(format!(" {} · {} ", ctx.anio, plural(lista.len())), tema::apagado()));
        let dentro = b.inner(area);
        f.render_widget(b, area);
        let [barra, cuerpo] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(dentro);
        // Filtros y búsqueda
        let mut spans = vec![Span::raw(" ")];
        for x in [Filtro::Todas, Filtro::Pendientes, Filtro::Borradores] {
            spans.push(Span::styled(format!(" {} ", x.nombre()), if x == self.filtro { tema::pestana_activa() } else { tema::apagado() }));
            spans.push(Span::raw(" "));
        }
        let w_filtros = spans.iter().map(|s| s.width()).sum::<usize>() as u16;
        let [a_fil, a_bus] = Layout::horizontal([Constraint::Length(w_filtros), Constraint::Min(0)]).areas(barra);
        f.render_widget(Paragraph::new(Line::from(spans)), a_fil);
        if self.buscando || !self.busca.valor.is_empty() {
            let [e, t] = Layout::horizontal([Constraint::Length(9), Constraint::Min(1)]).areas(a_bus);
            f.render_widget(Paragraph::new(Span::styled(" Buscar: ", tema::apagado())), e);
            self.busca.dibujar(f, t, self.buscando, Style::new().add_modifier(Modifier::BOLD));
        } else {
            f.render_widget(Paragraph::new(Span::styled(" f filtro · / buscar", tema::apagado())), a_bus);
        }
        if lista.is_empty() {
            let txt = if self.filtro == Filtro::Todas && self.busca.valor.is_empty() {
                if self.cerrado {
                    format!(" No hay facturas de {} en la app. Es un año cerrado: se ve en Actividad, con tu Renta.", ctx.anio)
                } else {
                    format!(" Todavía no hay facturas en {}. Pulsa n para crear la primera.", ctx.anio)
                }
            } else {
                " No hay facturas en esta vista.".to_string()
            };
            f.render_widget(Paragraph::new(Line::styled(txt, tema::apagado())).wrap(Wrap { trim: false }), cuerpo);
            return;
        }
        let w = cuerpo.width;
        let w_num = lista.iter().map(|&i| d.facturas[i].numero.chars().count()).max().unwrap_or(0).clamp(8, 14) as u16;
        let mut cols = vec![Col::Numero];
        if w >= 62 {
            cols.push(Col::Fecha);
        }
        cols.push(Col::Cliente);
        if w >= 75 {
            cols.push(Col::Base);
        }
        if w >= 88 {
            cols.push(Col::Iva);
        }
        if w >= 100 {
            cols.push(Col::Retencion);
        }
        cols.extend([Col::Total, Col::Estado, Col::Doc]);
        let ancho = |c: Col| match c {
            Col::Numero => Constraint::Length(w_num),
            Col::Fecha => Constraint::Length(11),
            Col::Cliente => Constraint::Fill(1),
            Col::Base | Col::Total => Constraint::Length(12),
            Col::Iva | Col::Retencion => Constraint::Length(11),
            Col::Estado => Constraint::Length(13),
            Col::Doc => Constraint::Length(1),
        };
        let titulo = |c: Col| match c {
            Col::Numero => Cell::from("Número"),
            Col::Fecha => Cell::from("Fecha"),
            Col::Cliente => Cell::from("Cliente"),
            Col::Base => Cell::from(derecha("Base")),
            Col::Iva => Cell::from(derecha("IVA")),
            Col::Retencion => Cell::from(derecha("Retención")),
            Col::Total => Cell::from(derecha("Total")),
            Col::Estado => Cell::from("Estado"),
            Col::Doc => Cell::from(""),
        };
        let hoy = fmt::hoy();
        let filas: Vec<Row> = lista
            .iter()
            .map(|&i| {
                let x = &d.facturas[i];
                let t = totales_factura(x);
                Row::new(cols.iter().map(|&c| match c {
                    Col::Numero if x.numero.is_empty() => Cell::from(Span::styled("borrador", tema::apagado().add_modifier(Modifier::ITALIC))),
                    Col::Numero => Cell::from(x.numero.clone()),
                    Col::Fecha => Cell::from(fecha_corta(&x.fecha)),
                    Col::Cliente if x.cliente.nombre.trim().is_empty() => Cell::from(Span::styled("Consumidor final", tema::apagado())),
                    Col::Cliente => Cell::from(x.cliente.nombre.clone()),
                    Col::Base => Cell::from(derecha(eur(t.base))),
                    Col::Iva => Cell::from(derecha(eur(t.cuota))),
                    Col::Retencion => Cell::from(derecha(eur(t.retencion))),
                    Col::Total => Cell::from(derecha(eur(t.total))),
                    Col::Estado => {
                        let (e, st) = estado(x, &hoy);
                        Cell::from(Span::styled(e, st))
                    }
                    Col::Doc => Cell::from(if x.doc.is_empty() { "" } else { "▤" }),
                }))
            })
            .collect();
        let tabla = Table::new(filas, cols.iter().map(|&c| ancho(c)))
            .header(Row::new(cols.iter().map(|&c| titulo(c))).style(tema::cabecera_tabla()))
            .column_spacing(1)
            .row_highlight_style(tema::seleccion());
        self.tabla.select(Some(self.sel));
        self.alto = cuerpo.height.saturating_sub(1).max(1) as usize;
        f.render_stateful_widget(tabla, cuerpo, &mut self.tabla);
    }

    fn dibujar_detalle(&mut self, f: &mut Frame, area: Rect, x: &Factura, titulo: &str, corto: bool) {
        let b = bloque(titulo);
        let dentro = b.inner(area);
        f.render_widget(b, area);
        let dentro = dentro.inner(ratatui::layout::Margin::new(1, 0));
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
        self.editor.is_some() || self.confirmar.is_some() || self.cobro.is_some() || self.buscando
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.cerrado = autonomo::cerrado(&ctx.datos, ctx.anio);
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
                Some(x) if self.ver => {
                    let t = if x.numero.is_empty() { "Borrador".to_string() } else { format!("Factura {}", x.numero) };
                    self.dibujar_detalle(f, area, x, &t, false);
                }
                _ => {
                    let resto = if area.height >= 12 {
                        let [a_kpi, resto] = Layout::vertical([Constraint::Length(5), Constraint::Min(3)]).areas(area);
                        self.dibujar_kpis(f, a_kpi, ctx);
                        resto
                    } else {
                        area
                    };
                    if area.width >= 110 {
                        let w_panel = (area.width * 35 / 100).clamp(40, 56);
                        let [a_l, a_p] = Layout::horizontal([Constraint::Min(40), Constraint::Length(w_panel)]).areas(resto);
                        self.dibujar_lista(f, a_l, ctx, &lista);
                        match actual {
                            Some(x) => self.dibujar_detalle(f, a_p, x, "Detalle", true),
                            None => f.render_widget(Paragraph::new("").block(bloque("Detalle")), a_p),
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
        if autonomo::cerrado(&ctx.datos, ctx.anio) && matches!(k.code, KeyCode::Char('n' | 'e' | 'E' | 'c' | 'D' | 'd') | KeyCode::Delete) {
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
            KeyCode::Char('e') => self.editar(ctx),
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
            KeyCode::Char('r') | KeyCode::Char('R') => ctx.info("Las rectificativas aún no se pueden hacer en la app."),
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
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
                Some(_) if self.ver => vec![("← →", "otra"), ("↑↓", "bajar"), ("Esc", "volver"), ("o", "doc")],
                Some(_) => vec![("Enter", "ver"), ("o", "doc")],
                None => vec![],
            };
            if !self.ver {
                v.extend([("f", "filtro"), ("/", "buscar")]);
            }
            return v;
        }
        let mut v = match self.elegida {
            Some(true) => vec![("Enter", "editar"), ("E", "emitir"), ("d", "borrar"), ("D", "duplicar")],
            Some(false) if self.ver => vec![("← →", "otra"), ("↑↓", "bajar"), ("Esc", "volver"), ("c", "cobrada"), ("D", "duplicar"), ("o", "doc")],
            Some(false) => vec![("Enter", "ver"), ("c", "cobrada"), ("D", "duplicar"), ("o", "doc")],
            None => vec![],
        };
        v.insert(0, ("n", "nueva"));
        if !self.ver {
            v.extend([("f", "filtro"), ("/", "buscar")]);
        }
        v
    }
}
