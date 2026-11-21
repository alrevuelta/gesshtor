//! Gastos: la lista del año con sus filtros (trimestre, categoría y búsqueda), lo que te deduces y la ficha de cada
//! gasto con su desglose (`dominio::gasto::gasto_calc`). Se añaden, editan, duplican y borran.

use crate::dominio::gasto::{self, Cat, GastoCalc, AMORT, CATS, DIETAS, ORIGENES, RENTA_LINEAS};
use crate::dominio::fmt::{self, r2};
use crate::dominio::{Datos, Gasto};
use crate::dominio::almacen;
use crate::informe::autonomo;
use crate::tui::widgets::{coincide, derecha, filas_partidas, guia, linea_atajos, tema, titulo_bloque, Confirmar, Eleccion, Entrada, Respuesta, Selector};
use crate::tui::texto::lineas;
use crate::tui::{Ctx, Nivel, Pantalla};
use formulario::{Edicion, Fin};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use super::gastos_cuotas as cuotas;
use super::gastos_formulario as formulario;
use crate::dominio::tgss::Recibo;

/// Ancho a partir del cual la ficha del gasto elegido va al lado de la lista.
const ANCHO_FICHA: u16 = 110;
/// La ficha a pantalla completa, no más ancha de lo que se lee bien (como el detalle de una factura).
const ANCHO_HOJA: u16 = 60;

#[derive(Default)]
pub struct Gastos {
    /// Trimestre que se ve (0 = todo el año).
    q: u32,
    /// Categoría elegida en el filtro (vacío = todas).
    cat: String,
    busca: Entrada,
    buscando: bool,
    /// Fila elegida de la lista filtrada.
    sel: usize,
    tabla: TableState,
    modo: Modo,
    /// Al cerrar el formulario o el borrado, se vuelve a la ficha a pantalla completa.
    desde_ficha: bool,
    /// Líneas desplazadas de la ficha a pantalla completa (PgUp/PgDn).
    desplaza: u16,
    /// Del último dibujo: ancho del área (¿cabe la ficha al lado?) y filas de la tabla (PgUp/PgDn).
    ancho: Option<u16>,
    filas: usize,
    anio: i32,
    /// El año que se ve está cerrado: solo se miran (del último dibujo, para los atajos).
    cerrado: bool,
}

#[derive(Default)]
enum Modo {
    #[default]
    Lista,
    /// La ficha del gasto elegido a pantalla completa (cuando no cabe al lado).
    Ficha,
    Form(Box<Edicion>),
    Categoria(Selector),
    /// Confirmar el borrado del gasto con ese id.
    Borrar(Confirmar, String),
}

#[derive(Default)]
struct Totales {
    pagado: f64,
    iva: f64,
    /// Coste en el IRPF de lo no amortizable más la amortización del periodo.
    irpf: f64,
    amort: f64,
    n: usize,
    con_doc: usize,
}

/// Columnas de la lista, en su orden (se quitan las menos importantes si no caben).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Col {
    Fecha,
    Proveedor,
    Concepto,
    Categoria,
    Base,
    Iva,
    IvaDed,
    Irpf,
    Doc,
}

impl Col {
    fn titulo(self) -> &'static str {
        match self {
            Col::Fecha => "fecha",
            Col::Proveedor => "proveedor",
            Col::Concepto => "concepto",
            Col::Categoria => "categoría",
            Col::Base => "base",
            Col::Iva => "IVA",
            Col::IvaDed => "IVA deduc.",
            Col::Irpf => "gasto IRPF",
            Col::Doc => "",
        }
    }
    /// Las de cifras, a la derecha.
    fn numero(self) -> bool {
        matches!(self, Col::Base | Col::Iva | Col::IvaDed | Col::Irpf)
    }
}

fn plural(n: usize) -> String {
    if n == 1 { "1 gasto".into() } else { format!("{n} gastos") }
}

/* ================= piezas que usa también el formulario ================= */

/// Nombre corto de la categoría, para la tabla.
fn cat_corta(id: &str) -> &'static str {
    match id {
        "software" => "Software",
        "publicidad" => "Publicidad",
        "material" => "Material",
        "profesionales" => "Gestoría",
        "formacion" => "Formación",
        "suministros" => "Suministros",
        "vivienda" => "Casa",
        "alquiler" => "Alquiler",
        "reparaciones" => "Reparación",
        "equipo" => "Equipos",
        "mobiliario" => "Mobiliario",
        "viajes" => "Viajes",
        "dietas" => "Dietas",
        "vehiculo" => "Vehículo",
        "seguros" => "Seguros",
        "salud" => "Salud",
        "ss" => "Autónomos",
        "banco" => "Banco",
        "intereses" => "Intereses",
        "tributos" => "Tributos",
        "compras" => "Compras",
        _ => "Otros",
    }
}

/// Proveedor, o concepto, o la categoría.
pub(super) fn nombre(g: &Gasto) -> String {
    if !g.proveedor.is_empty() {
        g.proveedor.clone()
    } else if !g.concepto.is_empty() {
        g.concepto.clone()
    } else {
        gasto::cat_de(&g.categoria).label.to_string()
    }
}


/// Línea de la Renta de la categoría: «casilla 0202 · Otros servicios exteriores».
fn renta(cat: &Cat) -> String {
    RENTA_LINEAS.iter().find(|r| r.0 == cat.renta).map(|r| format!("casilla {} · {}", r.1, r.2)).unwrap_or_default()
}

/// Las líneas de una ficha de `ancho` columnas: «etiqueta  valor» con la etiqueta en gris (lo que no cabe sigue
/// debajo del valor, no de la etiqueta) y las cifras con puntos guía, acabando en `ancho`.
pub(super) struct Hoja {
    ancho: usize,
    pub(super) t: Vec<Line<'static>>,
}

impl Hoja {
    /// Ancho de las etiquetas («nº factura» y un hueco), como en el detalle de una factura.
    const ETIQUETA: usize = 14;

    pub(super) fn new(ancho: u16) -> Self {
        Hoja { ancho: ancho as usize, t: Vec::new() }
    }
    fn kv(&mut self, k: &str, v: impl Into<String>) {
        let partes = lineas(&v.into(), self.ancho.saturating_sub(Self::ETIQUETA).max(12));
        for (i, p) in partes.into_iter().enumerate() {
            let k = if i == 0 { format!("{k:<w$}", w = Self::ETIQUETA) } else { " ".repeat(Self::ETIQUETA) };
            self.t.push(Line::from(vec![Span::styled(k, tema::apagado()), Span::raw(p)]));
        }
    }
    /// Una cifra con puntos guía: la etiqueta (un texto o varios trozos) y la cifra en negrita con `estilo`.
    fn cifra(&mut self, etiqueta: impl Into<Line<'static>>, cifra: String, estilo: Style) {
        self.t.push(guia(etiqueta, cifra, estilo, self.ancho as u16));
    }
    pub(super) fn push(&mut self, l: Line<'static>) {
        self.t.push(l);
    }
    fn blanco(&mut self) {
        self.t.push(Line::from(""));
    }
}

/// El desglose de `gasto_calc`, con puntos guía: lo de la factura (la base, el IVA y lo pagado, en negrita) y, debajo,
/// lo que te deduces (en BIEN, con su % en gris) y la línea de la Renta.
pub(super) fn cifras(g: &Gasto, c: &GastoCalc, h: &mut Hoja) {
    let (normal, gris, verde) = (Style::new(), tema::apagado(), Style::new().fg(tema::BIEN));
    h.cifra(if c.origen == "es" { "base" } else { "importe" }, fmt::eur(c.base), normal);
    if c.origen != "sin" {
        let nota = if c.origen != "es" { " (la autoliquidas)" } else if g.cuota.vacio() { " (calculada)" } else { "" };
        h.cifra(Line::from(vec![Span::raw(format!("IVA {}", fmt::pct(c.tipo))), Span::styled(nota, gris)]), fmt::eur(c.cuota), normal);
    }
    h.cifra(Span::styled("pagado", Style::new().add_modifier(Modifier::BOLD)), fmt::eur(c.pagado), normal);
    h.blanco();
    let no_ded = if c.origen == "es" && g.iva_no_ded && c.cuota > 0.0 { " · no deducible" } else { "" };
    h.cifra(Line::from(vec![Span::raw("IVA deducible "), Span::styled(format!("{}{no_ded}", fmt::pct(c.p_iva)), gris)]), fmt::eur(c.iva_ded), verde);
    // Corto, para que quepa en la ficha de al lado.
    let amort = if c.amortizable { " · amortiza" } else { "" };
    h.cifra(Line::from(vec![Span::raw("gasto en IRPF "), Span::styled(format!("{}{amort}", fmt::pct(c.p_irpf)), gris)]), fmt::eur(c.coste), verde);
    h.kv("renta", renta(c.cat));
}

/// Lo que te deduces, en frases (debajo del formulario). La primera es la principal.
pub(super) fn frases(g: &Gasto, c: &GastoCalc, d: &Datos) -> Vec<String> {
    let mut v = Vec::new();
    if c.amortizable {
        let coef = gasto::amort_coef(g);
        let anual = r2(c.coste * coef / 100.0);
        let mut s = format!("Cuesta más de 300 €: se amortiza a {} al año ({}).", fmt::eur(anual), fmt::pct(coef));
        if c.iva_ded > 0.0 {
            s += &format!(" El IVA ({}) lo recuperas entero este trimestre.", fmt::eur(c.iva_ded));
        }
        v.push(s);
    } else {
        let mut s = format!("Te deduces {} en el IRPF", fmt::eur(c.coste));
        if c.iva_ded > 0.0 {
            s += &format!(" y recuperas {} de IVA", fmt::eur(c.iva_ded));
        }
        s.push('.');
        v.push(s);
        if g.iva_no_ded && c.origen == "es" && c.cuota > 0.0 {
            v.push(format!("El IVA ({}) no se deduce: va incluido en el gasto del IRPF.", fmt::eur(c.cuota)));
        }
        match c.cat.especial {
            "vivienda" => {
                let p = gasto::pct_vivienda(d);
                v.push(if p > 0.0 {
                    format!("Es el {} del gasto: 30 % × m² del despacho.", fmt::pct(p))
                } else {
                    "Faltan los m² de la casa y del despacho: sin ellos es 0 €.".into()
                });
            }
            "salud" => v.push(format!("Límite anual: {}.", fmt::eur(500.0 * d.fiscal.personas_salud.v().max(1.0)))),
            "dietas" => {
                if let Some((_, l, lim)) = DIETAS.iter().find(|x| x.0 == g.dieta).or(DIETAS.first()) {
                    v.push(format!("Como mucho {} al día ({}).", fmt::eur(*lim), l.to_lowercase()));
                }
            }
            _ => {}
        }
    }
    if c.inversion_iva {
        v.push("Pasa de 3.005,06 €: es un bien de inversión para el IVA.".into());
    }
    match c.origen.as_str() {
        "ue" => v.push(format!(
            "Sin IVA en la factura: autoliquidas {} en el 303 (casillas 10-11) y te lo deduces a la vez (36-37). Va también en el 349.",
            fmt::eur(c.cuota)
        )),
        "extra" => v.push(format!("Inversión del sujeto pasivo: autoliquidas {} (casillas 12-13) y te lo deduces (28-29). Coste neto 0.", fmt::eur(c.cuota))),
        _ => {}
    }
    v
}

/// «2026-09-14» → «14 sep» (el año ya se ve arriba).
fn dia_mes(s: &str) -> String {
    if fmt::fecha(s).is_none() {
        return s.to_string();
    }
    let f = fmt::fecha_corta(s);
    f.rsplit_once(' ').map(|(a, _)| a.to_string()).unwrap_or(f)
}

/// La ficha de un gasto (en `ancho` columnas), al estilo del detalle de una factura: quién y qué, arriba y en el color
/// de la sección; los datos, con las etiquetas en gris; el desglose y lo que te deduces, con puntos guía; la
/// amortización; y lo que te deduces en frases, la ayuda de la categoría y las notas.
fn lineas_ficha(g: &Gasto, ctx: &Ctx, ancho: u16) -> Vec<Line<'static>> {
    let d = &ctx.datos;
    let c = gasto::gasto_calc(g, d);
    if let Some(r) = gasto::recibo_de(g, d) {
        return ficha_recibo(g, r, &c, ancho);
    }
    let mut t = Hoja::new(ancho);
    if c.repetido {
        t.push(Line::styled("No cuenta: ese mes ya está el recibo de la Seguridad Social.", Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)));
        t.blanco();
    }
    t.push(Line::styled(nombre(g), tema::titulo()));
    if !g.proveedor.is_empty() && !g.concepto.is_empty() {
        t.push(Line::from(g.concepto.clone()));
    }
    t.blanco();
    t.kv("fecha", fmt::fecha_corta(&g.fecha));
    if !g.num_factura.is_empty() {
        t.kv("nº factura", g.num_factura.clone());
    }
    if !g.nif.is_empty() {
        t.kv("NIF", g.nif.clone());
    }
    t.kv("categoría", c.cat.label);
    let origen = ORIGENES.iter().find(|o| o.0 == c.origen).map(|o| o.1).unwrap_or(c.origen.as_str()).to_string();
    t.kv("origen", origen);
    let doc = if g.doc.is_empty() {
        "ninguno".to_string()
    } else if almacen::documento(&ctx.dir, &g.doc).is_some() {
        format!("{} (o abrir)", g.doc)
    } else {
        format!("{} (no está en la carpeta)", g.doc)
    };
    t.kv("documento", doc);
    t.blanco();
    cifras(g, &c, &mut t);
    if c.amortizable {
        let normal = Style::new();
        let coef = gasto::amort_coef(g);
        let clave = if g.amort_tipo.is_empty() { c.cat.amort } else { g.amort_tipo.as_str() };
        let tabla = AMORT.iter().find(|a| a.0 == clave).map(|a| a.1).unwrap_or("Mobiliario y enseres");
        t.blanco();
        t.push(titulo_bloque("amortización"));
        t.kv("tabla", format!("{tabla}, {} al año", fmt::pct(coef)));
        t.cifra(format!("al año, de {}", fmt::eur(c.coste)), fmt::eur(r2(c.coste * coef / 100.0)), normal);
        let a = ctx.anio;
        let fin = format!("{a}-12-31");
        let este = gasto::amort_en_rango(g, &c, &format!("{a}-01-01"), &fin);
        let hecha = gasto::amort_en_rango(g, &c, &g.fecha, &fin);
        t.cifra(format!("en {a}"), fmt::eur(este), normal);
        t.cifra("queda", fmt::eur((c.coste - hecha).max(0.0)), normal);
        // Año a año, hasta que se acaba (por días: el primer año y el último van partidos).
        let ini = fmt::anio_de(&g.fecha);
        let anios = (100.0 / coef.max(1.0)).ceil() as i32;
        let partes: Vec<String> = (ini..=ini + anios)
            .map(|y| format!("{y}: {}", fmt::eur(gasto::amort_en_rango(g, &c, &format!("{y}-01-01"), &format!("{y}-12-31")))))
            .collect();
        t.push(Line::styled(partes.join(" · "), tema::apagado()));
    }
    t.blanco();
    let mut fr = frases(g, &c, d).into_iter();
    if let Some(p) = fr.next() {
        t.push(Line::styled(p, Style::new().fg(tema::BIEN)));
    }
    for l in fr {
        t.push(Line::from(l));
    }
    let p = c.cat.ayuda;
    if !p.is_empty() {
        t.blanco();
        t.push(Line::styled(p, tema::apagado().italic()));
    }
    if !g.notas.is_empty() {
        t.blanco();
        t.kv("notas", g.notas.clone());
    }
    t.t
}

/// La ficha de un recibo de autónomo de la Seguridad Social: el cargo y, con puntos guía, el importe, su desglose
/// (en gris), la base y lo que cuenta en el IRPF.
fn ficha_recibo(g: &Gasto, r: &Recibo, c: &GastoCalc, ancho: u16) -> Vec<Line<'static>> {
    let (normal, gris) = (Style::new(), tema::apagado());
    let mut t = Hoja::new(ancho);
    t.push(Line::styled(g.concepto.clone(), tema::titulo()));
    t.push(Line::styled("Recibo de autónomo de la Seguridad Social", gris));
    t.blanco();
    t.kv("cargo", if fmt::fecha(r.fecha_cargo()).is_some() { fmt::fecha_corta(r.fecha_cargo()) } else { r.fecha_cargo().to_string() });
    if !r.periodo.is_empty() {
        t.kv("de", fmt::nombre_mes(&r.periodo));
    }
    t.blanco();
    t.cifra(Span::styled("importe", Style::new().add_modifier(Modifier::BOLD)), fmt::eur(r.importe), normal);
    for (n, x) in cuotas::desglose(r) {
        t.cifra(Span::styled(format!("  {n}"), gris), fmt::eur(x), gris);
    }
    if let Some(b) = r.base() {
        t.cifra(Line::from(vec![Span::raw("base"), Span::styled(" (contingencias comunes)", gris)]), format!("≈ {}", fmt::eur(b)), normal);
    }
    t.blanco();
    t.cifra("gasto en IRPF", fmt::eur(c.coste), Style::new().fg(tema::BIEN));
    t.kv("renta", renta(c.cat));
    t.blanco();
    t.push(Line::styled("Sale de lo que traes de la Seguridad Social con tu certificado: no se apunta ni se cambia aquí.", gris.italic()));
    t.t
}

/* ================= la pantalla ================= */

impl Gastos {
    fn periodo(&self, anio: i32) -> (String, String) {
        if self.q == 0 { (format!("{anio}-01-01"), format!("{anio}-12-31")) } else { fmt::periodo(anio, self.q) }
    }

    /// ¿Pasa los filtros de categoría y búsqueda? (el de fechas va aparte: se amortiza lo de cualquier año)
    fn pasa(&self, g: &Gasto) -> bool {
        let cat = gasto::cat_de(&g.categoria);
        if !self.cat.is_empty() && cat.id != self.cat {
            return false;
        }
        let b = self.busca.valor.trim();
        b.is_empty() || coincide(&format!("{} {} {} {} {} {}", g.proveedor, g.concepto, g.nif, g.num_factura, g.notas, cat.label), b)
    }

    /// Índices (en `datos.gastos`) de los que se ven, del más reciente al más antiguo.
    fn visibles(&self, ctx: &Ctx) -> Vec<usize> {
        let d = &ctx.datos;
        let mut v: Vec<usize> = (0..d.gastos.len())
            .filter(|&i| {
                let g = &d.gastos[i];
                fmt::anio_de(&g.fecha) == ctx.anio && (self.q == 0 || fmt::trimestre_de(&g.fecha) == self.q) && self.pasa(g)
            })
            .collect();
        v.sort_by(|&a, &b| d.gastos[b].fecha.cmp(&d.gastos[a].fecha));
        v
    }

    fn elegido<'a>(&self, ctx: &'a Ctx) -> Option<&'a Gasto> {
        let vis = self.visibles(ctx);
        vis.get(self.sel.min(vis.len().saturating_sub(1))).map(|&i| &ctx.datos.gastos[i])
    }

    /// Antes del primer dibujo no se sabe el ancho: se da por hecho que cabe.
    fn ficha_al_lado(&self) -> bool {
        self.ancho.is_none_or(|w| w >= ANCHO_FICHA)
    }

    fn seleccionar(&mut self, ctx: &Ctx, id: &str) -> bool {
        match self.visibles(ctx).iter().position(|&i| ctx.datos.gastos[i].id == id) {
            Some(p) => {
                self.sel = p;
                true
            }
            None => false,
        }
    }

    fn hay_filtros(&self) -> bool {
        !self.cat.is_empty() || !self.busca.valor.trim().is_empty()
    }

    fn abrir(&mut self, modo: Modo) {
        self.desde_ficha = matches!(self.modo, Modo::Ficha);
        self.modo = modo;
    }

    fn volver(&mut self) {
        self.modo = if self.desde_ficha { Modo::Ficha } else { Modo::Lista };
        self.desde_ficha = false;
    }

    fn totales(&self, ctx: &Ctx, vis: &[usize]) -> Totales {
        let d = &ctx.datos;
        let mut t = Totales { n: vis.len(), ..Default::default() };
        for &i in vis {
            let g = &d.gastos[i];
            let c = gasto::gasto_calc(g, d);
            t.pagado = r2(t.pagado + c.pagado);
            t.iva = r2(t.iva + c.iva_ded);
            if !c.amortizable {
                t.irpf = r2(t.irpf + c.coste);
            }
            if !g.doc.is_empty() {
                t.con_doc += 1;
            }
        }
        // Lo que se amortiza en el periodo: los bienes de este año o de antes que pasan los filtros.
        let (desde, hasta) = self.periodo(ctx.anio);
        for g in d.gastos.iter().filter(|g| g.fecha <= hasta && self.pasa(g)) {
            let c = gasto::gasto_calc(g, d);
            if c.amortizable {
                t.amort = r2(t.amort + gasto::amort_en_rango(g, &c, &desde, &hasta));
            }
        }
        t.irpf = r2(t.irpf + t.amort);
        t
    }

    fn selector_categorias(&self, ctx: &Ctx) -> Selector {
        let mut els = vec![(String::new(), "Todas las categorías".to_string(), String::new())];
        for c in CATS {
            let n = ctx.datos.gastos.iter().filter(|g| fmt::anio_de(&g.fecha) == ctx.anio && gasto::cat_de(&g.categoria).id == c.id).count();
            let det = if n > 0 { format!("{} · {n} en {}", c.grupo, ctx.anio) } else { c.grupo.to_string() };
            els.push((c.id.to_string(), c.label.to_string(), det));
        }
        Selector::new("Filtrar por categoría", els).con_elegido(&self.cat)
    }

    fn editar(&mut self, ctx: &mut Ctx) {
        let Some(g) = self.elegido(ctx).cloned() else { return };
        self.abrir(Modo::Form(Box::new(Edicion::editar(&g))));
    }

    fn preguntar_borrar(&mut self, ctx: &Ctx) {
        let Some(g) = self.elegido(ctx) else { return };
        let gc = gasto::gasto_calc(g, &ctx.datos);
        let cual = format!("{} · {} · {}", fmt::dmy(&g.fecha), nombre(g), fmt::eur(gc.pagado));
        let c = Confirmar::new("Borrar gasto", "Borrar").linea(cual).linea("Se borrará este gasto. No se puede deshacer.");
        let id = g.id.clone();
        self.abrir(Modo::Borrar(c.peligro(), id));
    }

    fn borrar(&mut self, ctx: &mut Ctx, id: &str) {
        let Some(p) = ctx.datos.gastos.iter().position(|g| g.id == id) else { return };
        ctx.datos.gastos.remove(p);
        if ctx.guardar() {
            ctx.ok("Gasto borrado");
        }
        let n = self.visibles(ctx).len();
        self.sel = self.sel.min(n.saturating_sub(1));
        if n == 0 {
            self.modo = Modo::Lista;
        }
    }

    /// Tras guardar: se elige el gasto en la lista o se avisa de por qué no se ve.
    fn tras_guardar(&mut self, ctx: &mut Ctx, id: &str) {
        if self.seleccionar(ctx, id) || matches!(ctx.aviso, Some((_, Nivel::Mal, _))) {
            return;
        }
        let anio = ctx.datos.gastos.iter().find(|g| g.id == id).map(|g| fmt::anio_de(&g.fecha)).unwrap_or(ctx.anio);
        if anio != ctx.anio {
            ctx.ok(format!("Gasto guardado en {anio}: cambia de año con - y + para verlo."));
        } else {
            ctx.ok("Gasto guardado (con los filtros de ahora no se ve).");
        }
    }

    fn tecla_busca(&mut self, k: KeyEvent, ctx: &Ctx) -> bool {
        match k.code {
            KeyCode::Esc => {
                self.busca = Entrada::default();
                self.buscando = false;
                self.sel = 0;
            }
            KeyCode::Enter => self.buscando = false,
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down => self.sel = (self.sel + 1).min(self.visibles(ctx).len().saturating_sub(1)),
            _ => {
                if self.busca.tecla(k) {
                    self.sel = 0;
                }
            }
        }
        true
    }

    fn tecla_lista(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        let n = self.visibles(ctx).len();
        let ultimo = n.saturating_sub(1);
        let ficha = matches!(self.modo, Modo::Ficha);
        let pagina = self.filas.max(1);
        // Los filtros se cambian en la lista.
        if ficha && matches!(k.code, KeyCode::Char('t' | 'T' | 'c' | '/')) {
            self.modo = Modo::Lista;
        }
        if matches!(k.code, KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End) {
            self.desplaza = 0;
        }
        // Un año cerrado se mira (Enter abre la ficha), pero no se apuntan, cambian, duplican ni borran gastos.
        if autonomo::cerrado(&ctx.datos, ctx.anio) {
            match k.code {
                KeyCode::Char('n' | 'd' | 'D') | KeyCode::Delete => {
                    ctx.error(autonomo::aviso_cerrado(ctx.anio));
                    return true;
                }
                KeyCode::Enter if !ficha => {
                    if n > 0 {
                        self.modo = Modo::Ficha;
                        self.desplaza = 0;
                    }
                    return true;
                }
                KeyCode::Enter => return true,
                _ => {}
            }
        }
        // Un recibo de la Seguridad Social se mira, pero no se cambia, duplica ni borra.
        let recibo = self.elegido(ctx).is_some_and(Gasto::es_recibo);
        let cambiar = matches!(k.code, KeyCode::Char('d' | 'D') | KeyCode::Delete) || (k.code == KeyCode::Enter && (ficha || self.ficha_al_lado()));
        if recibo && cambiar {
            ctx.info("Es un recibo de la Seguridad Social: sale de lo que traes con tu certificado y no se cambia aquí.");
            return true;
        }
        match k.code {
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down => self.sel = (self.sel + 1).min(ultimo),
            // En la ficha, PgUp/PgDn la desplazan (si no cabe).
            KeyCode::PageUp if ficha => self.desplaza = self.desplaza.saturating_sub(5),
            KeyCode::PageDown if ficha => self.desplaza = self.desplaza.saturating_add(5),
            KeyCode::PageUp => self.sel = self.sel.saturating_sub(pagina),
            KeyCode::PageDown => self.sel = (self.sel + pagina).min(ultimo),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = ultimo,
            KeyCode::Esc | KeyCode::Backspace if ficha => self.modo = Modo::Lista,
            KeyCode::Esc if self.hay_filtros() => {
                self.cat.clear();
                self.busca = Entrada::default();
                self.sel = 0;
            }
            KeyCode::Char('t') => {
                self.q = (self.q + 1) % 5;
                self.sel = 0;
            }
            KeyCode::Char('T') => {
                self.q = (self.q + 4) % 5;
                self.sel = 0;
            }
            KeyCode::Char('c') => self.modo = Modo::Categoria(self.selector_categorias(ctx)),
            KeyCode::Char('/') => self.buscando = true,
            KeyCode::Char('n') => self.abrir(Modo::Form(Box::new(Edicion::nuevo()))),
            KeyCode::Enter if !ficha && !self.ficha_al_lado() => {
                if n > 0 {
                    self.modo = Modo::Ficha;
                    self.desplaza = 0;
                }
            }
            KeyCode::Enter => self.editar(ctx),
            KeyCode::Char('d') | KeyCode::Delete => self.preguntar_borrar(ctx),
            KeyCode::Char('D') => {
                if let Some(g) = self.elegido(ctx).cloned() {
                    self.abrir(Modo::Form(Box::new(Edicion::duplicar(&g))));
                }
            }
            KeyCode::Char('o') => {
                if let Some(doc) = self.elegido(ctx).map(|g| g.doc.clone()) {
                    ctx.abrir_doc(&doc);
                }
            }
            _ => return false,
        }
        true
    }

    /* ---------------- dibujo ---------------- */

    /// Filtrando por la cuota de autónomos: lo de tus cuotas del año.
    fn cuotas(&self, ctx: &Ctx) -> Option<autonomo::CuotasAnio> {
        (self.cat == "ss").then(|| autonomo::cuotas_anio(&ctx.datos, ctx.anio, &fmt::hoy()))
    }

    /// Lo de debajo de la lista, cada cosa con su estilo: los gastos con fecha futura, las cuotas repetidas y,
    /// filtrando por la cuota de autónomos, lo de tus cuotas.
    fn nota(&self, ctx: &Ctx) -> Vec<(String, Style)> {
        let mut v: Vec<(String, Style)> = Vec::new();
        let hoy = fmt::hoy();
        if let Some(c) = self.cuotas(ctx) {
            v.extend(cuotas::notas(&c, &ctx.datos.tgss, &hoy).into_iter().map(|n| (n.texto, n.estilo)));
        }
        let repetidos = self.visibles(ctx).iter().filter(|&&i| gasto::repetido(&ctx.datos.gastos[i], &ctx.datos)).count();
        if repetidos > 0 {
            let (s, n) = if repetidos == 1 { ("", "") } else { ("s", "n") };
            v.push((format!("{repetidos} cuota{s} apuntada{s} a mano no cuenta{n}: ese mes ya está el recibo."), Style::new().fg(tema::AVISO)));
        }
        let fut = ctx.datos.gastos.iter().filter(|g| g.fecha > hoy).count();
        if fut == 1 {
            v.push(("1 gasto con fecha futura: no cuenta todavía.".to_string(), tema::apagado()));
        } else if fut > 1 {
            v.push((format!("{fut} gastos con fecha futura: no cuentan todavía."), tema::apagado()));
        }
        v
    }

    /// Lo del periodo con puntos guía, como en Facturas: lo pagado, el IVA y el IRPF que te deduces (en BIEN) y
    /// cuántos llevan documento, cada cifra con su nota en gris. Filtrando por la cuota de autónomos, las cifras de
    /// tus cuotas.
    fn dibujar_resumen(&self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let filas: Vec<(String, String, Style, String)> = match self.cuotas(ctx) {
            Some(c) => cuotas::cifras(&c, &ctx.datos.tgss, &fmt::hoy()).into_iter().map(|c| (c.etiqueta, c.valor, c.estilo, c.nota)).collect(),
            None => {
                let t = self.totales(ctx, vis);
                let verde = Style::new().fg(tema::BIEN);
                let cuando = if self.q == 0 { format!("en {}", ctx.anio) } else { format!("en el {}T", self.q) };
                let amort = if t.amort > 0.0 { format!("incluye {} de amortización", fmt::eur(t.amort)) } else { "coste en la Renta".into() };
                vec![
                    (format!("pagado {cuando}"), fmt::eur(t.pagado), Style::new(), format!("base + IVA de {}", plural(t.n))),
                    ("IVA deducible".into(), fmt::eur(t.iva), verde, "resta en el 303".into()),
                    ("deducible en IRPF".into(), fmt::eur(t.irpf), verde, amort),
                    ("con documento".into(), t.con_doc.to_string(), Style::new(), format!("de {}", plural(t.n))),
                ]
            }
        };
        // Todas las cifras acaban en la misma columna, con al menos unos puntos.
        let fin = filas.iter().map(|(e, c, ..)| e.chars().count() + c.chars().count() + 6).max().unwrap_or(0) as u16;
        let v: Vec<Line> = filas
            .into_iter()
            .map(|(e, c, st, nota)| {
                let mut l = guia(e, c, st, fin);
                if !nota.is_empty() {
                    l.push_span(Span::styled(format!("   {nota}"), tema::apagado()));
                }
                l
            })
            .collect();
        f.render_widget(Paragraph::new(v), area);
    }

    /// Arriba de la lista, como en Facturas: los trimestres (el elegido en FONDO sobre el color de la sección), la
    /// categoría si hay una elegida y cuántos se ven. Las teclas (`t`, `c`, `/`…) y el año, solo abajo y arriba: aquí
    /// no se repiten. En la fila de debajo, si se busca, la búsqueda.
    fn dibujar_filtros(&self, f: &mut Frame, area: Rect, _ctx: &Ctx, n: usize) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let mut trimestres = Vec::new();
        for (i, t) in ["Año", "1T", "2T", "3T", "4T"].iter().enumerate() {
            if i > 0 {
                trimestres.push(Span::raw(" "));
            }
            // Todos con su hueco a cada lado: al cambiar de trimestre no se mueve nada.
            trimestres.push(Span::styled(format!(" {t} "), if i as u32 == self.q { tema::pestana_activa() } else { gris }));
        }
        // La categoría, solo si hay una elegida (como el trimestre); luego, cuántos se ven.
        let linea = |hueco: &'static str| {
            let mut v = trimestres.clone();
            if !self.cat.is_empty() {
                v.extend([Span::raw(hueco), Span::styled(format!(" {} ", cat_corta(&self.cat)), tema::pestana_activa())]);
            }
            v.extend([Span::raw(hueco), Span::styled(plural(n), gris)]);
            Line::from(v)
        };
        let w = area.width as usize;
        let variantes = [linea("     "), linea("   ")];
        let l = variantes.iter().find(|l| l.width() <= w).unwrap_or(&variantes[1]).clone();
        let fila = Rect { height: area.height.min(1), ..area };
        f.render_widget(Paragraph::new(l), fila);
        if (self.buscando || !self.busca.valor.trim().is_empty()) && area.height >= 2 {
            let fila = Rect { y: area.y + 1, height: 1, ..area };
            let [e, t] = Layout::horizontal([Constraint::Length(7), Constraint::Min(1)]).areas(fila);
            f.render_widget(Paragraph::new(Span::styled("buscar", gris)), e);
            self.busca.dibujar(f, t, self.buscando, negrita);
        }
    }

    /// La lista, sin caja, como la de Facturas: la cabecera de las columnas en gris, las cifras a la derecha (sin «€»:
    /// ya lo dice el resumen) y el elegido en el fondo suave de la sección; los de fecha futura y las cuotas repetidas,
    /// en gris. Si no caben todas las columnas, se quitan las menos importantes (la primera, el concepto aparte).
    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let d = &ctx.datos;
        self.filas = area.height.saturating_sub(1) as usize;
        if vis.is_empty() {
            let txt = if self.hay_filtros() {
                "Nada con estos filtros (Esc los quita).".to_string()
            } else if self.q > 0 {
                "No hay gastos en este trimestre.".to_string()
            } else if self.cerrado {
                format!("No hay gastos de {} en la app.", ctx.anio)
            } else {
                format!("Todavía no hay gastos en {}.", ctx.anio)
            };
            let pie = if self.cerrado { "Es un año cerrado: se ve en Impuestos, con tu Renta." } else { "Pulsa n para añadir uno." };
            f.render_widget(Paragraph::new(Line::styled(format!("{txt} {pie}"), tema::apagado())).wrap(Wrap { trim: false }), area);
            return;
        }
        let gastos: Vec<&Gasto> = vis.iter().map(|&i| &d.gastos[i]).collect();
        let calc: Vec<GastoCalc> = gastos.iter().map(|g| gasto::gasto_calc(g, d)).collect();
        // El texto de cada celda (sin estilo: la fila va entera en gris si no cuenta).
        let texto = |c: Col, g: &Gasto, x: &GastoCalc, partido: bool| -> String {
            match c {
                Col::Fecha => dia_mes(&g.fecha),
                Col::Proveedor if !partido => nombre(g),
                Col::Proveedor if g.proveedor.is_empty() => "—".into(),
                Col::Proveedor => g.proveedor.clone(),
                Col::Concepto => g.concepto.clone(),
                Col::Categoria => cat_corta(x.cat.id).into(),
                Col::Base => fmt::num(x.base),
                Col::Iva => match x.origen.as_str() {
                    "ue" => "UE".into(),
                    "extra" => "fuera UE".into(),
                    _ if x.cuota == 0.0 => "—".into(),
                    _ => fmt::num(x.cuota),
                },
                Col::IvaDed if x.iva_ded > 0.0 => fmt::num(x.iva_ded),
                Col::IvaDed if x.origen == "es" && x.cuota > 0.0 => "no ded.".into(),
                Col::IvaDed => "—".into(),
                Col::Irpf if x.repetido => "repetido".into(),
                Col::Irpf if x.amortizable => "amortiza".into(),
                Col::Irpf if x.coste > 0.0 => fmt::num(x.coste),
                Col::Irpf => "—".into(),
                Col::Doc => if g.doc.is_empty() { String::new() } else { "▤".into() },
            }
        };
        // El ancho de cada columna: lo más largo de ella, o su título; el proveedor y el concepto, 16 como poco.
        let ancho = |c: Col| -> u16 {
            match c {
                // El concepto aparte, solo si cabe con holgura (si no, va en el proveedor).
                Col::Concepto => 24,
                Col::Proveedor => 16,
                Col::Doc => 1,
                _ => gastos.iter().zip(&calc).map(|(g, x)| texto(c, g, x, false).chars().count()).max().unwrap_or(0).max(c.titulo().chars().count()) as u16,
            }
        };
        const ESPACIO: u16 = 2;
        let mut cols = vec![Col::Fecha, Col::Proveedor, Col::IvaDed, Col::Irpf];
        if gastos.iter().any(|g| !g.doc.is_empty()) {
            cols.push(Col::Doc);
        }
        for c in [Col::Categoria, Col::Base, Col::Iva, Col::Concepto] {
            let ocupa: u16 = cols.iter().chain([&c]).map(|&c| ancho(c) + ESPACIO).sum();
            if ocupa <= area.width {
                cols.push(c);
            }
        }
        cols.sort();
        let partido = cols.contains(&Col::Concepto);
        let cabecera = Row::new(cols.iter().map(|&c| if c.numero() { Cell::from(derecha(c.titulo())) } else { Cell::from(c.titulo()) })).style(tema::cabecera_tabla());
        let hoy = fmt::hoy();
        let filas: Vec<Row> = gastos
            .iter()
            .zip(&calc)
            .map(|(g, x)| {
                let fila = Row::new(cols.iter().map(|&c| {
                    let s = texto(c, g, x, partido);
                    match c {
                        _ if c.numero() => Cell::from(derecha(s)),
                        Col::Fecha | Col::Doc => Cell::from(Span::styled(s, tema::apagado())),
                        Col::Proveedor if !partido && g.proveedor.is_empty() && g.concepto.is_empty() => Cell::from(Span::styled(s, tema::apagado())),
                        _ => Cell::from(s),
                    }
                }));
                if g.fecha > hoy || x.repetido { fila.style(tema::apagado()) } else { fila }
            })
            .collect();
        let anchos = cols.iter().map(|&c| if matches!(c, Col::Proveedor | Col::Concepto) { Constraint::Fill(1) } else { Constraint::Length(ancho(c)) });
        let tabla = Table::new(filas, anchos).header(cabecera).column_spacing(ESPACIO).row_highlight_style(tema::seleccion());
        self.tabla.select(Some(self.sel));
        f.render_stateful_widget(tabla, area, &mut self.tabla);
    }

    /// La lista, como Facturas: arriba los filtros (y la búsqueda); debajo, lo del periodo con puntos guía, si cabe; la
    /// tabla, con la ficha del elegido a la derecha si hay sitio; y abajo, las notas (cuotas repetidas, fechas futuras
    /// y, filtrando por la cuota de autónomos, lo de tus cuotas).
    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let nota = self.nota(ctx);
        // Las líneas que ocupa lo de debajo (como mucho, tres).
        let largo: usize = nota.iter().map(|(t, _)| t.chars().count() + 3).sum();
        let alto_nota = if nota.is_empty() { 0 } else { (largo as u16).div_ceil(area.width.max(1)).clamp(1, 3) };
        let [a_fil, resto, a_nota] = Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(alto_nota)]).areas(area);
        self.dibujar_filtros(f, a_fil, ctx, vis.len());
        let resto = if area.height >= 16 {
            let [a_res, _, resto] = Layout::vertical([Constraint::Length(4), Constraint::Length(1), Constraint::Min(1)]).areas(resto);
            self.dibujar_resumen(f, a_res, ctx, vis);
            resto
        } else {
            resto
        };
        if self.ficha_al_lado() {
            // Algo más ancha que la de Facturas: las cifras de la ficha van con puntos guía.
            let w_panel = ((u32::from(area.width) * 38 / 100) as u16).clamp(44, 56);
            let [a_l, _, a_p] = Layout::horizontal([Constraint::Min(40), Constraint::Length(4), Constraint::Length(w_panel)]).areas(resto);
            self.dibujar_tabla(f, a_l, ctx, vis);
            let t = match vis.get(self.sel) {
                Some(&i) => lineas_ficha(&ctx.datos.gastos[i], ctx, a_p.width),
                None => vec![Line::styled("Elige un gasto para ver su desglose.", tema::apagado())],
            };
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), a_p);
        } else {
            self.dibujar_tabla(f, resto, ctx, vis);
        }
        if !nota.is_empty() {
            let mut l = Vec::new();
            for (i, (t, st)) in nota.into_iter().enumerate() {
                if i > 0 {
                    l.push(Span::styled(" · ", tema::apagado()));
                }
                l.push(Span::styled(t, st));
            }
            f.render_widget(Paragraph::new(Line::from(l)).wrap(Wrap { trim: true }), a_nota);
        }
    }
}

impl Pantalla for Gastos {
    fn titulo(&self) -> &'static str {
        "Gastos"
    }

    fn capturando(&self) -> bool {
        self.buscando || matches!(self.modo, Modo::Form(_) | Modo::Categoria(_) | Modo::Borrar(..))
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.cerrado = autonomo::cerrado(&ctx.datos, ctx.anio);
        if self.anio != ctx.anio {
            self.anio = ctx.anio;
            self.sel = 0;
            *self.tabla.offset_mut() = 0;
        }
        if matches!(self.modo, Modo::Ficha) {
            self.modo = Modo::Lista;
        }
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.ancho = Some(area.width);
        self.cerrado = autonomo::cerrado(&ctx.datos, ctx.anio);
        if let Modo::Form(e) = &mut self.modo {
            e.dibujar(f, area, &ctx.datos);
            return;
        }
        let vis = self.visibles(ctx);
        self.sel = self.sel.min(vis.len().saturating_sub(1));
        if matches!(self.modo, Modo::Ficha) {
            match vis.get(self.sel) {
                Some(&i) => {
                    // La ficha, sin caja: su primera línea ya dice de quién es.
                    let dentro = Rect { width: area.width.min(ANCHO_HOJA), ..area };
                    let t = lineas_ficha(&ctx.datos.gastos[i], ctx, dentro.width);
                    self.desplaza = self.desplaza.min(t.len().saturating_sub(1) as u16);
                    let filas: u16 = t.iter().map(|l| filas_partidas(&l.spans.iter().map(|s| s.content.as_ref()).collect::<String>(), dentro.width as usize)).sum();
                    f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }).scroll((self.desplaza, 0)), dentro);
                    // Si no cabe entera, cómo seguir (abajo no siempre cabe): a su derecha, arriba.
                    let tecla = if filas > self.desplaza + area.height {
                        Some(("PgDn", "más"))
                    } else {
                        (self.desplaza > 0).then_some(("PgUp", "arriba"))
                    };
                    if let Some(k) = tecla {
                        let l = linea_atajos(&[k]);
                        let w = l.width() as u16;
                        if area.width >= dentro.width + 2 + w {
                            f.render_widget(Paragraph::new(l.right_aligned()), Rect { height: area.height.min(1), ..area });
                        }
                    }
                    return;
                }
                None => self.modo = Modo::Lista,
            }
        }
        self.dibujar_lista(f, area, ctx, &vis);
        match &mut self.modo {
            Modo::Categoria(s) => s.dibujar(f, area),
            Modo::Borrar(c, _) => c.dibujar(f, area),
            _ => {}
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        match &mut self.modo {
            Modo::Form(e) => {
                match e.tecla(k, ctx) {
                    Fin::Sigue => {}
                    Fin::Cancelado => self.volver(),
                    Fin::Guardado(id) => {
                        self.volver();
                        self.tras_guardar(ctx, &id);
                    }
                }
                return true;
            }
            Modo::Categoria(s) => {
                match s.tecla(k) {
                    Eleccion::Sigue => {}
                    Eleccion::Cancelado => self.modo = Modo::Lista,
                    Eleccion::Elegido(c) => {
                        self.cat = c;
                        self.sel = 0;
                        self.modo = Modo::Lista;
                    }
                }
                return true;
            }
            Modo::Borrar(c, id) => {
                match c.tecla(k) {
                    Respuesta::Sigue => {}
                    Respuesta::No => self.volver(),
                    Respuesta::Si => {
                        let id = id.clone();
                        self.volver();
                        self.borrar(ctx, &id);
                    }
                }
                return true;
            }
            Modo::Lista | Modo::Ficha => {}
        }
        if self.buscando {
            return self.tecla_busca(k, ctx);
        }
        self.tecla_lista(k, ctx)
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        match &self.modo {
            Modo::Form(e) => e.atajos(),
            Modo::Categoria(_) => vec![("↑↓", "elegir"), ("Enter", "filtrar"), ("Esc", "cancelar")],
            Modo::Borrar(..) => vec![("Enter/s", "sí"), ("Esc/n", "no")],
            Modo::Ficha if self.cerrado => vec![("Esc", "volver"), ("↑↓", "otro gasto"), ("PgUp PgDn", "desplazar"), ("o", "documento")],
            Modo::Ficha => vec![("Esc", "volver"), ("↑↓", "otro gasto"), ("PgUp PgDn", "desplazar"), ("Enter", "editar"), ("D", "duplicar"), ("d", "borrar"), ("o", "documento")],
            Modo::Lista if self.cerrado && !self.buscando => {
                // Las de filtrar, pronto: abajo se recortan por el final y arriba ya no se repiten.
                let mut v = vec![("Enter", "ficha"), ("t", "trimestre"), ("c", "categoría"), ("/", "buscar")];
                if self.hay_filtros() {
                    v.push(("Esc", "sin filtros"));
                }
                v.push(("o", "documento"));
                v
            }
            Modo::Lista if self.buscando => vec![("Enter", "aceptar"), ("Esc", "quitar la búsqueda"), ("↑↓", "mover")],
            Modo::Lista => {
                let mut v = vec![("n", "nuevo")];
                v.push(("Enter", if self.ficha_al_lado() { "editar" } else { "ficha" }));
                // Las de filtrar, pronto: abajo se recortan por el final y arriba ya no se repiten.
                v.extend([("t", "trimestre"), ("c", "categoría"), ("/", "buscar")]);
                if self.hay_filtros() {
                    v.push(("Esc", "sin filtros"));
                }
                v.extend([("o", "documento"), ("D", "duplicar"), ("d", "borrar")]);
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::prueba::{app_con, cargar, caso, ir, pantalla, teclas};
    use crate::dominio::fmt::{self, anio_actual};
    use crate::dominio::{Gasto, Num};

    /// Al estilo C2, como Facturas y sin cajas: los filtros arriba (con su tecla), lo del año con puntos guía, la
    /// cabecera de las columnas en minúscula y las cifras sin «€»; Enter, la ficha con el desglose en puntos guía; y
    /// n, el formulario con lo que te deduces debajo. También a 80 columnas.
    #[test]
    fn la_lista_la_ficha_y_el_formulario() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        d.gastos.push(Gasto { id: "g-hoy".into(), proveedor: "Zeta Software".into(), concepto: "Licencia".into(), categoria: "software".into(), origen: "es".into(), base: Num::de(100.0), iva_tipo: Num::de(21.0), fecha: fmt::hoy(), ..Default::default() });
        let mut a = app_con(d, &dir);
        ir(&mut a, "Gastos");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(" Año   1T   2T   3T   4T") && p.contains(&format!("pagado en {} ··", anio_actual())), "{p}");
        // Arriba, sin las teclas ni «todas las categorías» ni el año (ya están en el pie y en la cabecera).
        assert!(!p.contains("t  Año") && !p.contains("todas las categorías") && !p.contains(&format!("{} · ", anio_actual())), "{p}");
        assert!(p.contains("IVA deducible ··") && p.contains("con documento ··") && !p.contains('│') && !p.contains('┌'), "{p}");
        let cabecera = p.lines().find(|l| l.contains("proveedor")).unwrap_or_default();
        assert!(cabecera.contains("fecha") && cabecera.contains("IVA deduc.") && cabecera.contains("gasto IRPF"), "{p}");
        let fila = p.lines().find(|l| l.contains("Zeta Software")).unwrap_or_default();
        assert!(fila.contains(" 100,00 ") && fila.contains(" 21,00 ") && !fila.contains('€'), "{p}");
        // A 120 columnas la ficha no cabe al lado: Enter la abre.
        teclas(&mut a, "/Zeta<enter><enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Zeta Software") && p.contains("pagado ··") && p.contains(" 121,00 €") && p.contains("IVA deducible 100 % ··"), "{p}");
        assert!(!p.contains('│') && !p.contains('┌'), "{p}");
        teclas(&mut a, "<esc><esc>n");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Nuevo gasto") && p.contains("› Fecha") && p.contains("lo que te deduces") && !p.contains('│'), "{p}");
        assert!(pantalla(&mut a, 80, 24).contains("lo que te deduces"));
    }
}
