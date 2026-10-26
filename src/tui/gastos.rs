//! Gastos: la lista del año con sus filtros (trimestre, categoría y búsqueda), lo que te deduces y la ficha de cada
//! gasto con su desglose (`dominio::gasto::gasto_calc`). Se añaden, editan, duplican y borran.

use crate::dominio::gasto::{self, Cat, GastoCalc, AMORT, CATS, DIETAS, ORIGENES, RENTA_LINEAS};
use crate::dominio::fmt::{self, r2};
use crate::dominio::{Datos, Gasto};
use crate::dominio::almacen;
use crate::informe::autonomo;
use crate::tui::widgets::{bloque, coincide, derecha, kpi, tema, Confirmar, Eleccion, Entrada, Respuesta, Selector};
use crate::tui::texto::lineas;
use crate::tui::{Ctx, Nivel, Pantalla};
use formulario::{Edicion, Fin};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use super::gastos_formulario as formulario;

/// Ancho a partir del cual la ficha del gasto elegido va al lado de la lista.
const ANCHO_FICHA: u16 = 110;

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

/// Líneas de una ficha «etiqueta  valor»: lo que no cabe sigue debajo del valor, no de la etiqueta.
pub(super) struct Hoja {
    ancho: usize,
    pub(super) t: Vec<Line<'static>>,
}

impl Hoja {
    const ETIQUETA: usize = 15;

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
    pub(super) fn push(&mut self, l: Line<'static>) {
        self.t.push(l);
    }
}

/// El desglose de `gasto_calc`: base, IVA, lo deducible, lo pagado y la línea de la Renta.
pub(super) fn cifras(g: &Gasto, c: &GastoCalc, h: &mut Hoja) {
    h.kv(if c.origen == "es" { "Base" } else { "Importe" }, fmt::eur(c.base));
    if c.origen != "sin" {
        let nota = if c.origen != "es" { " (la autoliquidas)" } else if g.cuota.vacio() { " (calculada)" } else { "" };
        h.kv("IVA", format!("{} · {}{nota}", fmt::pct(c.tipo), fmt::eur(c.cuota)));
    }
    let no_ded = if c.origen == "es" && g.iva_no_ded && c.cuota > 0.0 { " · no deducible" } else { "" };
    h.kv("IVA deducible", format!("{} ({}){no_ded}", fmt::eur(c.iva_ded), fmt::pct(c.p_iva)));
    let amort = if c.amortizable { " · se amortiza" } else { "" };
    h.kv("Gasto en IRPF", format!("{} ({}){amort}", fmt::eur(c.coste), fmt::pct(c.p_irpf)));
    h.kv("Pagado", fmt::eur(c.pagado));
    h.kv("Renta", renta(c.cat));
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

/// La ficha de un gasto (en `ancho` columnas): datos, desglose, amortización, ayuda de la categoría y notas.
fn lineas_ficha(g: &Gasto, ctx: &Ctx, ancho: u16) -> Vec<Line<'static>> {
    let d = &ctx.datos;
    let c = gasto::gasto_calc(g, d);
    let mut t = Hoja::new(ancho);
    t.push(Line::styled(nombre(g), Style::new().add_modifier(Modifier::BOLD)));
    if !g.proveedor.is_empty() && !g.concepto.is_empty() {
        t.push(Line::from(g.concepto.clone()));
    }
    t.push(Line::from(""));
    t.kv("Fecha", fmt::dmy(&g.fecha));
    if !g.num_factura.is_empty() {
        t.kv("Nº factura", g.num_factura.clone());
    }
    if !g.nif.is_empty() {
        t.kv("NIF", g.nif.clone());
    }
    t.kv("Categoría", c.cat.label);
    let origen = ORIGENES.iter().find(|o| o.0 == c.origen).map(|o| o.1).unwrap_or(c.origen.as_str()).to_string();
    t.kv("Origen", origen);
    cifras(g, &c, &mut t);
    let doc = if g.doc.is_empty() {
        "ninguno".to_string()
    } else if almacen::documento(&ctx.dir, &g.doc).is_some() {
        format!("{} (o: abrir)", g.doc)
    } else {
        format!("{} (no está en la carpeta)", g.doc)
    };
    t.kv("Documento", doc);
    if c.amortizable {
        let coef = gasto::amort_coef(g);
        let clave = if g.amort_tipo.is_empty() { c.cat.amort } else { g.amort_tipo.as_str() };
        let tabla = AMORT.iter().find(|a| a.0 == clave).map(|a| a.1).unwrap_or("Mobiliario y enseres");
        t.kv("Amortización", format!("{tabla}, {} al año", fmt::pct(coef)));
        t.kv("", format!("{} al año de {}", fmt::eur(r2(c.coste * coef / 100.0)), fmt::eur(c.coste)));
        let a = ctx.anio;
        let fin = format!("{a}-12-31");
        let este = gasto::amort_en_rango(g, &c, &format!("{a}-01-01"), &fin);
        let hecha = gasto::amort_en_rango(g, &c, &g.fecha, &fin);
        t.kv(&format!("En {a}"), format!("{} · queda {}", fmt::eur(este), fmt::eur((c.coste - hecha).max(0.0))));
        // Año a año, hasta que se acaba (por días: el primer año y el último van partidos).
        let ini = fmt::anio_de(&g.fecha);
        let anios = (100.0 / coef.max(1.0)).ceil() as i32;
        let partes: Vec<String> = (ini..=ini + anios)
            .map(|y| format!("{y}: {}", fmt::eur(gasto::amort_en_rango(g, &c, &format!("{y}-01-01"), &format!("{y}-12-31")))))
            .collect();
        t.push(Line::styled(partes.join(" · "), tema::apagado()));
    }
    t.push(Line::from(""));
    let mut fr = frases(g, &c, d).into_iter();
    if let Some(p) = fr.next() {
        t.push(Line::styled(p, Style::new().fg(tema::BIEN)));
    }
    for l in fr {
        t.push(Line::from(l));
    }
    let p = c.cat.ayuda;
    if !p.is_empty() {
        t.push(Line::from(""));
        t.push(Line::styled(p, tema::apagado().italic()));
    }
    if !g.notas.is_empty() {
        t.push(Line::from(""));
        t.kv("Notas", g.notas.clone());
    }
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
                KeyCode::Char('n' | 'e' | 'd' | 'D') | KeyCode::Delete => {
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
            KeyCode::Enter | KeyCode::Char('e') => self.editar(ctx),
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

    fn nota(&self, ctx: &Ctx) -> String {
        let mut v = Vec::new();
        let hoy = fmt::hoy();
        let fut = ctx.datos.gastos.iter().filter(|g| g.fecha > hoy).count();
        if fut == 1 {
            v.push("1 gasto con fecha futura: no cuenta todavía.".to_string());
        } else if fut > 1 {
            v.push(format!("{fut} gastos con fecha futura: no cuentan todavía."));
        }
        v.join(" · ")
    }

    fn dibujar_kpis(&self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let t = self.totales(ctx, vis);
        let [a, b, c, d] = Layout::horizontal([Constraint::Ratio(1, 4); 4]).areas(area);
        let cuando = if self.q == 0 { format!("en {}", ctx.anio) } else { format!("en el {}T", self.q) };
        let amort = if t.amort > 0.0 { format!("{} amortiz.", fmt::eur(t.amort)) } else { "coste en la Renta".into() };
        kpi(f, a, &format!("Pagado {cuando}"), &fmt::eur(t.pagado), "base + IVA", Style::new());
        kpi(f, b, "IVA deducible", &fmt::eur(t.iva), "resta en el 303", Style::new().fg(tema::BIEN));
        kpi(f, c, "Deducible en IRPF", &fmt::eur(t.irpf), &amort, Style::new().fg(tema::BIEN));
        kpi(f, d, "Nº de gastos", &t.n.to_string(), &format!("{} con documento", t.con_doc), Style::new());
    }

    fn dibujar_filtros(&self, f: &mut Frame, area: Rect) {
        let mut sp = vec![Span::raw(" ")];
        for (i, t) in ["Año", "1T", "2T", "3T", "4T"].iter().enumerate() {
            let st = if i as u32 == self.q { tema::pestana_activa() } else { tema::apagado() };
            sp.push(Span::styled(format!(" {t} "), st));
        }
        sp.push(Span::styled("  Categoría: ", tema::apagado()));
        sp.push(if self.cat.is_empty() { Span::raw("todas") } else { Span::styled(cat_corta(&self.cat), Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD)) });
        let [izq, der] = Layout::horizontal([Constraint::Min(10), Constraint::Length(30)]).areas(area);
        f.render_widget(Paragraph::new(Line::from(sp)), izq);
        if self.buscando {
            let [et, en] = Layout::horizontal([Constraint::Length(8), Constraint::Min(1)]).areas(der);
            f.render_widget(Paragraph::new(Span::styled("Buscar: ", Style::new().fg(tema::ACENTO))), et);
            self.busca.dibujar(f, en, true, Style::new().add_modifier(Modifier::UNDERLINED));
        } else if !self.busca.valor.trim().is_empty() {
            let t = Line::from(vec![Span::styled("Buscar: ", tema::apagado()), Span::styled(format!("«{}»", self.busca.valor.trim()), Style::new().fg(tema::ACENTO))]);
            f.render_widget(Paragraph::new(t.right_aligned()), der);
        } else {
            f.render_widget(Paragraph::new(Line::styled("/ buscar ", tema::apagado()).right_aligned()), der);
        }
    }

    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let d = &ctx.datos;
        let cuando = if self.q == 0 { ctx.anio.to_string() } else { format!("{}T {}", self.q, ctx.anio) };
        let titulo = format!("{} gasto{} · {cuando}", vis.len(), if vis.len() == 1 { "" } else { "s" });
        let b = bloque(&titulo);
        let dentro = b.inner(area);
        self.filas = dentro.height.saturating_sub(1) as usize;
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
            let t = vec![Line::from(""), Line::from(txt).centered(), Line::styled(pie, tema::apagado()).centered()];
            f.render_widget(Paragraph::new(t).block(b), area);
            return;
        }
        // Columnas según el ancho: el concepto aparte si cabe; si no, el IVA es lo primero que sobra.
        let w = dentro.width;
        let partido = w >= 100;
        let con_iva = w >= 74;
        let con_base = w >= 64;
        let mut cab: Vec<Cell> = vec![Cell::from("Fecha"), Cell::from(""), Cell::from("Proveedor")];
        let mut anchos = vec![Constraint::Length(6), Constraint::Length(1), Constraint::Fill(1)];
        if partido {
            cab.push(Cell::from("Concepto"));
            anchos.push(Constraint::Fill(1));
        }
        cab.push(Cell::from("Categoría"));
        anchos.push(Constraint::Length(11));
        if con_base {
            cab.push(Cell::from(derecha("Base")));
            anchos.push(Constraint::Length(10));
        }
        if con_iva {
            cab.push(Cell::from(derecha("IVA")));
            anchos.push(Constraint::Length(9));
        }
        cab.push(Cell::from(derecha("IVA deduc.")));
        anchos.push(Constraint::Length(10));
        cab.push(Cell::from(derecha("Gasto IRPF")));
        anchos.push(Constraint::Length(10));
        let hoy = fmt::hoy();
        let filas: Vec<Row> = vis
            .iter()
            .map(|&i| {
                let g = &d.gastos[i];
                let c = gasto::gasto_calc(g, d);
                let doc = if g.doc.is_empty() { Span::raw("") } else { Span::styled("•", Style::new().fg(tema::ACENTO)) };
                let quien = if partido { if g.proveedor.is_empty() { "—".to_string() } else { g.proveedor.clone() } } else { nombre(g) };
                let mut celdas = vec![Cell::from(dia_mes(&g.fecha)), Cell::from(doc), Cell::from(quien)];
                if partido {
                    celdas.push(Cell::from(g.concepto.clone()));
                }
                celdas.push(Cell::from(cat_corta(c.cat.id)));
                if con_base {
                    celdas.push(Cell::from(derecha(fmt::eur(c.base))));
                }
                if con_iva {
                    let iva = match c.origen.as_str() {
                        "ue" => "UE".to_string(),
                        "extra" => "fuera UE".to_string(),
                        _ if c.cuota == 0.0 => "—".to_string(),
                        _ => fmt::eur(c.cuota),
                    };
                    celdas.push(Cell::from(derecha(iva)));
                }
                let ded = if c.iva_ded > 0.0 {
                    fmt::eur(c.iva_ded)
                } else if c.origen == "es" && c.cuota > 0.0 {
                    "no ded.".to_string()
                } else {
                    "—".to_string()
                };
                celdas.push(Cell::from(derecha(ded)));
                let irpf = if c.amortizable {
                    "amortiza".to_string()
                } else if c.coste > 0.0 {
                    fmt::eur(c.coste)
                } else {
                    "—".to_string()
                };
                celdas.push(Cell::from(derecha(irpf)));
                let fila = Row::new(celdas);
                if g.fecha > hoy { fila.style(tema::apagado()) } else { fila }
            })
            .collect();
        let tabla = Table::new(filas, anchos).header(Row::new(cab).style(tema::cabecera_tabla())).block(b).column_spacing(1).row_highlight_style(tema::seleccion());
        self.tabla.select(Some(self.sel));
        f.render_stateful_widget(tabla, area, &mut self.tabla);
    }

    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx, vis: &[usize]) {
        let nota = self.nota(ctx);
        let alto_nota = if nota.is_empty() { 0 } else { 1 };
        let [zk, zf, zc, zn] = Layout::vertical([Constraint::Length(5), Constraint::Length(1), Constraint::Min(3), Constraint::Length(alto_nota)]).areas(area);
        self.dibujar_kpis(f, zk, ctx, vis);
        self.dibujar_filtros(f, zf);
        if self.ficha_al_lado() {
            let [zt, zd] = Layout::horizontal([Constraint::Min(40), Constraint::Length(40)]).areas(zc);
            self.dibujar_tabla(f, zt, ctx, vis);
            let t = match vis.get(self.sel) {
                Some(&i) => lineas_ficha(&ctx.datos.gastos[i], ctx, zd.width.saturating_sub(2)),
                None => vec![Line::styled("Elige un gasto para ver su desglose.", tema::apagado())],
            };
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }).block(bloque("Detalle")), zd);
        } else {
            self.dibujar_tabla(f, zc, ctx, vis);
        }
        if !nota.is_empty() {
            f.render_widget(Paragraph::new(Line::styled(format!(" {nota}"), tema::apagado())), zn);
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
                    let g = &ctx.datos.gastos[i];
                    let titulo = format!("{} · {}", fmt::fecha_corta(&g.fecha), nombre(g));
                    let t = lineas_ficha(g, ctx, area.width.saturating_sub(2));
                    self.desplaza = self.desplaza.min(t.len().saturating_sub(1) as u16);
                    let p = Paragraph::new(t).wrap(Wrap { trim: false }).scroll((self.desplaza, 0)).block(bloque(&titulo));
                    f.render_widget(p, area);
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
            Modo::Ficha => vec![("Esc", "volver"), ("↑↓", "otro gasto"), ("PgUp PgDn", "desplazar"), ("e", "editar"), ("D", "duplicar"), ("d", "borrar"), ("o", "documento")],
            Modo::Lista if self.cerrado && !self.buscando => {
                let mut v = vec![("Enter", "ficha"), ("o", "documento"), ("t", "trimestre"), ("c", "categoría"), ("/", "buscar")];
                if self.hay_filtros() {
                    v.push(("Esc", "sin filtros"));
                }
                v
            }
            Modo::Lista if self.buscando => vec![("Enter", "aceptar"), ("Esc", "quitar la búsqueda"), ("↑↓", "mover")],
            Modo::Lista => {
                let mut v = vec![("n", "nuevo")];
                if self.ficha_al_lado() {
                    v.push(("Enter", "editar"));
                } else {
                    v.extend([("Enter", "ficha"), ("e", "editar")]);
                }
                v.extend([("d", "borrar"), ("D", "duplicar"), ("o", "documento"), ("t", "trimestre"), ("c", "categoría"), ("/", "buscar")]);
                if self.hay_filtros() {
                    v.push(("Esc", "sin filtros"));
                }
                v
            }
        }
    }
}
