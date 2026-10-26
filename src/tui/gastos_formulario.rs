//! El formulario de un gasto: los campos que se ven dependen de la categoría y del origen, al
//! cambiar de categoría se ponen su IVA y su origen, y al lado se ve lo que te deduces mientras escribes.
use super::gastos::{cifras, frases, Hoja};
use crate::dominio::gasto::{self, AMORT, CATS, DIETAS, ORIGENES};
use crate::dominio::fmt;
use crate::dominio::{Datos, Gasto, Num};
use crate::dominio::almacen;
use crate::tui::widgets::{bloque, tema, Eleccion, Formulario, Resultado, Selector, Tipo};
use crate::tui::Ctx;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::collections::HashMap;

/// Para qué se ha abierto el formulario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Uso {
    Nuevo,
    Editar,
    /// Copia de otro gasto, con la fecha de hoy y sin nº de factura ni documento.
    Duplicar,
}

/// Lo que pasa después de una tecla.
#[derive(Debug, Clone, PartialEq)]
pub enum Fin {
    Sigue,
    Cancelado,
    /// Guardado en `ctx.datos`: el id del gasto.
    Guardado(String),
}

pub struct Edicion {
    pub uso: Uso,
    /// El gasto de partida.
    previo: Gasto,
    pub form: Formulario,
    /// Lo escrito en cada campo, también en los que ahora no se ven (si vuelven a verse, vuelven con su valor).
    valores: HashMap<String, String>,
    /// La lista abierta encima del formulario: (campo, lista).
    lista: Option<(&'static str, Selector)>,
}

/// 1234.5 → «1234,5» (como lo escribe `Formulario::numero`); None → vacío.
fn txt_num(x: Option<f64>) -> String {
    x.map(|x| fmt::num(x).replace('.', "").trim_end_matches(",00").to_string()).unwrap_or_default()
}

fn si(b: bool) -> String {
    if b { "1".into() } else { String::new() }
}

/// Mayúsculas, sin espacios, guiones ni puntos.
fn limpia_nif(s: &str) -> String {
    s.to_uppercase().chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '.').collect()
}

fn pares(v: &[(String, String)]) -> Vec<(&str, &str)> {
    v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
}

impl Edicion {
    /// Gasto nuevo: fecha de hoy, categoría software y el origen y el IVA de la categoría.
    pub fn nuevo() -> Self {
        let cat = gasto::cat_de("software");
        let g = Gasto { fecha: fmt::hoy(), categoria: cat.id.into(), origen: cat.origen.into(), iva_tipo: Num::de(cat.iva), dieta: "es".into(), ..Default::default() };
        Self::new(Uso::Nuevo, g)
    }

    pub fn editar(g: &Gasto) -> Self {
        Self::new(Uso::Editar, g.clone())
    }

    pub fn duplicar(g: &Gasto) -> Self {
        let mut c = g.clone();
        c.fecha = fmt::hoy();
        c.num_factura.clear();
        c.doc.clear();
        Self::new(Uso::Duplicar, c)
    }

    fn new(uso: Uso, g: Gasto) -> Self {
        let cat = gasto::cat_de(&g.categoria);
        let amort = if !g.amort_tipo.is_empty() { g.amort_tipo.as_str() } else if !cat.amort.is_empty() { cat.amort } else { "informatica" };
        let avanzado = !(g.p_iva.vacio() && g.p_irpf.vacio() && g.notas.is_empty() && g.doc.is_empty());
        let pares = [
            ("fecha", if fmt::fecha(&g.fecha).is_some() { fmt::dmy(&g.fecha) } else { g.fecha.clone() }),
            ("proveedor", g.proveedor.clone()),
            ("nif", g.nif.clone()),
            ("num_factura", g.num_factura.clone()),
            ("concepto", g.concepto.clone()),
            ("categoria", cat.id.to_string()),
            ("origen", if g.origen.is_empty() { "es".to_string() } else { g.origen.clone() }),
            ("base", txt_num(g.base.0)),
            ("iva_tipo", txt_num(Some(g.iva_tipo.v()))),
            ("cuota", txt_num(g.cuota.0)),
            ("iva_no_ded", si(g.iva_no_ded)),
            ("dieta", if g.dieta.is_empty() { "es".to_string() } else { g.dieta.clone() }),
            ("amort_tipo", amort.to_string()),
            ("avanzado", si(avanzado)),
            ("p_iva", txt_num(g.p_iva.0)),
            ("p_irpf", txt_num(g.p_irpf.0)),
            ("notas", g.notas.clone()),
            ("doc", g.doc.clone()),
        ];
        let valores = pares.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let mut e = Edicion { uso, previo: g, form: Formulario::default(), valores, lista: None };
        e.form = e.construir();
        e
    }

    fn v(&self, k: &str) -> &str {
        self.valores.get(k).map(String::as_str).unwrap_or("")
    }

    fn pon(&mut self, k: &str, v: &str) {
        self.valores.insert(k.into(), v.into());
    }

    fn titulo(&self) -> &'static str {
        match self.uso {
            Uso::Nuevo => "Nuevo gasto",
            Uso::Editar => "Editar gasto",
            Uso::Duplicar => "Duplicar gasto",
        }
    }

    /// El formulario con los campos que tocan según lo escrito.
    fn construir(&self) -> Formulario {
        let cat = gasto::cat_de(self.v("categoria"));
        let origen = self.v("origen");
        let tipo = fmt::parse_num(self.v("iva_tipo"));
        let cats: Vec<(&str, &str)> = CATS.iter().map(|c| (c.id, c.label)).collect();
        // Las etiquetas más largas («NIF del proveedor», «Tabla de amortización») llevan un espacio al final: si no,
        // el formulario las pega al valor.
        let mut f = Formulario::new(self.titulo())
            .fecha("fecha", "Fecha", "")
            .ayuda("dd/mm/aaaa, o «19/10» si es de este año.")
            .texto("proveedor", "Proveedor", "")
            .ayuda(if self.uso == Uso::Nuevo { "Ej.: Movistar, Adobe… Si ya está en otro gasto, se rellenan su NIF y su categoría." } else { "Ej.: Movistar, Adobe…" })
            .texto("nif", "NIF del proveedor", "")
            .texto("num_factura", "Nº de factura", "")
            .texto("concepto", "Concepto", "")
            .ayuda("Qué es.")
            .opciones("categoria", "Categoría", &cats, "")
            .ayuda(format!("Enter o escribe para buscar en la lista. {}", cat.ayuda).trim_end());
        if cat.especial == "dietas" {
            let d: Vec<(String, String)> = DIETAS.iter().map(|(k, l, lim)| (k.to_string(), format!("{l} (máx. {})", fmt::eur(*lim)))).collect();
            f = f.opciones("dieta", "Tipo de dieta", &pares(&d), "").ayuda("En el IRPF cuenta como mucho el límite diario.");
        }
        if !cat.amort.is_empty() {
            let a: Vec<(String, String)> = AMORT.iter().map(|(k, l, c)| (k.to_string(), format!("{l} ({} anual)", fmt::pct(*c)))).collect();
            f = f.opciones("amort_tipo", "Tabla de amortización", &pares(&a), "").ayuda("Si cuesta más de 300 € (sin IVA) se deduce poco a poco, con este % al año.");
        }
        f = f
            .opciones("origen", "Origen", &ORIGENES, "")
            .ayuda("¿De dónde es la factura? De la UE o de fuera: sin IVA en la factura (lo autoliquidas tú).")
            .numero("base", if origen == "es" { "Base (sin IVA)" } else { "Importe" }, None);
        if origen != "sin" {
            let mut tipos = vec![21.0, 10.0, 4.0, 0.0];
            if !tipos.contains(&tipo) {
                tipos.push(tipo);
                tipos.sort_by(|a, b| b.total_cmp(a));
            }
            let t: Vec<(String, String)> = tipos.iter().map(|&x| (txt_num(Some(x)), fmt::pct(x))).collect();
            f = f.opciones("iva_tipo", "IVA", &pares(&t), "").ayuda("← →: 21, 10, 4 o 0 %.");
        }
        if origen == "es" {
            f = f.numero("cuota", "Cuota IVA", None).ayuda("Vacío: base × tipo. Escríbela si la factura dice otra cosa (redondeos).");
        }
        // La casilla solo tiene sentido con factura española con IVA y en categorías cuyo IVA se deduce (las comidas no).
        let p_iva_cat = fmt::num_opcional(self.v("p_iva")).unwrap_or(cat.p_iva);
        if origen == "es" && tipo > 0.0 && p_iva_cat > 0.0 {
            f = f.si("iva_no_ded", "IVA no deducible", false).ayuda(
                "Márcalo si es un tique sin tu NIF o una empresa extranjera que te cobra IVA (OpenAI, Google…): ese IVA no se resta en el 303 y cuenta como gasto en el IRPF.",
            );
        }
        f = f.si("avanzado", "Avanzado", false).ayuda("Espacio: % deducibles, notas y documento.");
        if self.v("avanzado") == "1" {
            let defecto = if cat.especial == "vivienda" { "el de la vivienda (30 % × m²)".to_string() } else { format!("el de la categoría, {}", fmt::pct(cat.p_iva)) };
            f = f
                .numero("p_iva", "% IVA deducible", None)
                .ayuda(&format!("Vacío: {defecto}. Si es de uso mixto, pon solo la parte profesional."))
                .numero("p_irpf", "% gasto en IRPF", None)
                .ayuda(&if cat.especial == "vivienda" { defecto.clone() } else { format!("Vacío: el de la categoría, {}.", fmt::pct(cat.p_irpf)) })
                .texto("notas", "Notas", "")
                .texto("doc", "Documento", "")
                .ayuda("Nombre del fichero en documentos/ (opcional). Enter: elegir de la lista.");
        }
        let claves: Vec<String> = f.campos.iter().map(|c| c.clave.clone()).collect();
        for k in claves {
            f.set(&k, self.v(&k));
        }
        f
    }

    /// Pasa lo escrito en el formulario a `valores`.
    fn leer(&mut self) {
        for c in &self.form.campos {
            let v = match &c.tipo {
                Tipo::Opciones(o) => o.get(c.elegida).map(|x| x.0.clone()).unwrap_or_default(),
                Tipo::Si => si(c.si),
                _ => c.entrada.valor.clone(),
            };
            self.valores.insert(c.clave.clone(), v);
        }
    }

    /// Vuelve a montar el formulario (pueden aparecer o desaparecer campos) sin perder el foco ni el cursor.
    fn rehacer(&mut self) {
        let foco = self.form.foco_clave().to_string();
        let viejo = std::mem::take(&mut self.form);
        let mut nuevo = self.construir();
        for c in nuevo.campos.iter_mut() {
            if let Some(o) = viejo.campos.iter().find(|o| o.clave == c.clave)
                && matches!(c.tipo, Tipo::Texto | Tipo::Numero | Tipo::Fecha)
                && o.entrada.valor == c.entrada.valor
            {
                c.entrada = o.entrada.clone();
            }
        }
        nuevo.foco = nuevo.campos.iter().position(|c| c.clave == foco).unwrap_or(viejo.foco.min(nuevo.campos.len().saturating_sub(1)));
        self.form = nuevo;
    }

    /// Ha cambiado un campo: valores por defecto de la categoría, el proveedor conocido y los campos que se ven.
    fn cambio(&mut self, d: &Datos) {
        let cat_antes = self.v("categoria").to_string();
        let prov_antes = self.v("proveedor").to_string();
        self.leer();
        if self.v("categoria") != cat_antes {
            self.por_categoria();
        }
        if self.uso == Uso::Nuevo && self.v("proveedor") != prov_antes {
            self.por_proveedor(d);
        }
        self.rehacer();
    }

    /// Al cambiar de categoría: las de «sin IVA» ponen ese origen; las demás quitan el «sin» (y dejan
    /// UE o fuera de la UE), ponen su tipo de IVA y su tabla de amortización.
    fn por_categoria(&mut self) {
        let cat = gasto::cat_de(self.v("categoria"));
        if cat.origen == "sin" {
            self.pon("origen", "sin");
        } else if self.v("origen") == "sin" {
            self.pon("origen", "es");
        }
        if self.v("origen") != "sin" {
            self.pon("iva_tipo", &txt_num(Some(if cat.iva > 0.0 { cat.iva } else { 21.0 })));
        }
        if !cat.amort.is_empty() {
            self.pon("amort_tipo", cat.amort);
        }
    }

    /// Si el proveedor escrito ya está en otro gasto, se copian su NIF, categoría, origen, IVA y concepto (solo en los nuevos).
    fn por_proveedor(&mut self, d: &Datos) {
        let p = self.v("proveedor");
        if p.is_empty() {
            return;
        }
        let Some(g) = d.gastos.iter().rev().find(|g| g.proveedor == p).cloned() else { return };
        self.pon("nif", &g.nif);
        self.pon("categoria", gasto::cat_de(&g.categoria).id);
        self.pon("origen", if g.origen.is_empty() { "es" } else { &g.origen });
        self.pon("iva_tipo", &txt_num(Some(g.iva_tipo.v())));
        if self.v("concepto").trim().is_empty() {
            self.pon("concepto", &g.concepto);
        }
    }

    fn elegir(&mut self, campo: &str, v: &str) {
        let antes = self.v(campo).to_string();
        self.pon(campo, v);
        if campo == "categoria" && v != antes {
            self.por_categoria();
        }
        self.rehacer();
    }

    pub fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Fin {
        if let Some((campo, sel)) = self.lista.as_mut() {
            match sel.tecla(k) {
                Eleccion::Sigue => {}
                Eleccion::Cancelado => self.lista = None,
                Eleccion::Elegido(v) => {
                    let campo = *campo;
                    self.lista = None;
                    self.elegir(campo, &v);
                }
            }
            return Fin::Sigue;
        }
        let foco = self.form.foco_clave().to_string();
        // En la categoría, Enter o espacio abren la lista; si se escribe, se abre ya buscando eso.
        if foco == "categoria" && matches!(k.code, KeyCode::Enter | KeyCode::Char(_)) && !crate::tui::widgets::es_ctrl(&k) {
            let els = CATS.iter().map(|c| (c.id.to_string(), c.label.to_string(), c.grupo.to_string())).collect();
            let mut sel = Selector::new("Categoría", els).con_elegido(self.v("categoria"));
            if matches!(k.code, KeyCode::Char(c) if c != ' ') {
                sel.tecla(k);
            }
            self.lista = Some(("categoria", sel));
            return Fin::Sigue;
        }
        if foco == "doc"
            && k.code == KeyCode::Enter
            && let Some(s) = lista_documentos(ctx, self.v("doc"))
        {
            self.lista = Some(("doc", s));
            return Fin::Sigue;
        }
        match self.form.tecla(k) {
            Resultado::Sigue => Fin::Sigue,
            Resultado::Cambio => {
                self.cambio(&ctx.datos);
                Fin::Sigue
            }
            Resultado::Cancelar => Fin::Cancelado,
            Resultado::Guardar => self.guardar(ctx),
        }
    }

    fn guardar(&mut self, ctx: &mut Ctx) -> Fin {
        self.leer();
        let g = match self.validar(ctx) {
            Ok(g) => g,
            Err(e) => {
                self.form.error = Some(e);
                return Fin::Sigue;
            }
        };
        let id = g.id.clone();
        let pos = if self.uso == Uso::Editar { ctx.datos.gastos.iter().position(|x| x.id == id) } else { None };
        match pos {
            Some(i) => ctx.datos.gastos[i] = g,
            None => ctx.datos.gastos.push(g),
        }
        if ctx.guardar() {
            ctx.ok(if self.uso == Uso::Editar { "Gasto guardado" } else { "Gasto añadido" });
        }
        Fin::Guardado(id)
    }

    /// Comprueba lo escrito y devuelve el gasto listo para guardar.
    fn validar(&self, ctx: &Ctx) -> Result<Gasto, String> {
        let fecha = fmt::leer_fecha(self.v("fecha")).ok_or_else(|| "La fecha no es válida: escríbela como 19/10/2026.".to_string())?;
        let y = fmt::anio_de(&fecha);
        if crate::informe::autonomo::cerrado(&ctx.datos, y) {
            return Err(format!("{} Pon una fecha de un año abierto.", crate::informe::autonomo::aviso_cerrado(y)));
        }
        let base = self.v("base").trim();
        if base.is_empty() {
            return Err("Pon el importe.".into());
        }
        if !fmt::es_num(base) || fmt::parse_num(base) <= 0.0 {
            return Err("El importe tiene que ser un número mayor que 0.".into());
        }
        let cuota = self.v("cuota");
        if self.v("origen") == "es" && (!fmt::es_num(cuota) || fmt::parse_num(cuota) < 0.0) {
            return Err("La cuota de IVA no es un importe válido (vacía se calcula sola).".into());
        }
        for (k, nombre) in [("p_iva", "% de IVA deducible"), ("p_irpf", "% del gasto en IRPF")] {
            let t = self.v(k);
            if !fmt::es_num(t) || !(0.0..=100.0).contains(&fmt::parse_num(t)) {
                return Err(format!("El {nombre} tiene que estar entre 0 y 100 (o vacío)."));
            }
        }
        let doc = self.v("doc").trim();
        if !doc.is_empty() {
            if doc.contains(['/', '\\']) || doc.contains("..") {
                return Err("El documento es solo el nombre del fichero, sin carpetas (está en documentos/).".into());
            }
            if doc != self.previo.doc && almacen::documento(&ctx.dir, doc).is_none() {
                return Err(format!("No encuentro «{doc}» en documentos/."));
            }
        }
        let mut g = self.armar();
        g.fecha = fecha;
        if self.uso != Uso::Editar {
            g.id = crate::dominio::uid();
        }
        Ok(g)
    }

    /// El gasto con lo escrito, sin comprobar nada (para el resumen de al lado).
    fn armar(&self) -> Gasto {
        let mut g = self.previo.clone();
        let cat = gasto::cat_de(self.v("categoria"));
        let origen = self.v("origen").to_string();
        let es = origen == "es";
        let tipo = if origen == "sin" { 0.0 } else { fmt::parse_num(self.v("iva_tipo")) };
        if let Some(f) = fmt::leer_fecha(self.v("fecha")) {
            g.fecha = f;
        }
        g.proveedor = self.v("proveedor").trim().into();
        g.nif = limpia_nif(self.v("nif"));
        g.num_factura = self.v("num_factura").trim().into();
        g.concepto = self.v("concepto").trim().into();
        g.categoria = cat.id.into();
        g.base = Num::de(fmt::r2(fmt::parse_num(self.v("base"))));
        g.iva_tipo = Num::de(tipo);
        g.cuota = Num(if es { fmt::num_opcional(self.v("cuota")).map(fmt::r2) } else { None });
        g.p_iva = Num(fmt::num_opcional(self.v("p_iva")));
        g.p_irpf = Num(fmt::num_opcional(self.v("p_irpf")));
        g.p_manual = !g.p_iva.vacio() || !g.p_irpf.vacio();
        g.dieta = self.v("dieta").into();
        g.amort_tipo = if cat.amort.is_empty() { String::new() } else { self.v("amort_tipo").into() };
        g.iva_no_ded = es && tipo > 0.0 && self.v("iva_no_ded") == "1";
        g.notas = self.v("notas").trim().into();
        g.doc = self.v("doc").trim().into();
        g.origen = origen;
        g
    }

    /// Lo que te deduces con lo escrito hasta ahora, en `ancho` columnas. `completo`: con el desglose (si va al lado).
    fn resumen(&self, d: &Datos, completo: bool, ancho: u16) -> Vec<Line<'static>> {
        let cat = gasto::cat_de(self.v("categoria"));
        let mut t = Hoja::new(ancho);
        if self.v("base").trim().is_empty() {
            t.push(Line::styled("Escribe el importe para ver lo que te deduces.", tema::apagado()));
        } else {
            let g = self.armar();
            let c = gasto::gasto_calc(&g, d);
            if completo {
                cifras(&g, &c, &mut t);
                t.push(Line::from(""));
            } else {
                let irpf = if c.amortizable { format!("{} (se amortiza)", fmt::eur(c.coste)) } else { fmt::eur(c.coste) };
                t.push(Line::from(format!("Pagado {} · IVA deducible {} · IRPF {irpf}", fmt::eur(c.pagado), fmt::eur(c.iva_ded))));
            }
            let mut fr = frases(&g, &c, d).into_iter();
            if let Some(p) = fr.next() {
                t.push(Line::styled(p, Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD)));
            }
            for l in fr {
                t.push(Line::from(l));
            }
        }
        let p = cat.ayuda;
        if completo && !p.is_empty() {
            t.push(Line::from(""));
            t.push(Line::styled(p.to_string(), tema::apagado().italic()));
        }
        t.t
    }

    pub fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.lista.is_some() {
            return vec![("↑↓", "elegir"), ("Enter", "aceptar"), ("Esc", "volver")];
        }
        let mut v = match self.form.foco_clave() {
            "categoria" => vec![("Enter", "lista")],
            "doc" => vec![("Enter", "lista de documentos")],
            _ => vec![],
        };
        v.extend([("Tab ↓", "siguiente"), ("← →", "opción"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")]);
        v
    }

    /// El formulario ocupa toda la pestaña; el resumen va al lado si cabe, si no debajo.
    pub fn dibujar(&mut self, f: &mut Frame, area: Rect, d: &Datos) {
        let ancho = area.width >= 100;
        // Con el resumen al lado, la ayuda de la categoría ya se ve ahí.
        let cat = gasto::cat_de(self.v("categoria"));
        let ayuda = if ancho { "" } else { cat.ayuda };
        if let Some(c) = self.form.campos.iter_mut().find(|c| c.clave == "categoria") {
            c.ayuda = format!("Enter o escribe para buscar en la lista. {ayuda}").trim_end().to_string();
        }
        let [zf, zr] = if ancho {
            Layout::horizontal([Constraint::Min(50), Constraint::Length(44)]).areas(area)
        } else {
            Layout::vertical([Constraint::Min(6), Constraint::Length(7)]).areas(area)
        };
        let b = bloque(&self.form.titulo);
        let dentro = b.inner(zf);
        f.render_widget(b, zf);
        self.form.dibujar(f, dentro);
        let resumen = Paragraph::new(self.resumen(d, ancho, zr.width.saturating_sub(2))).wrap(Wrap { trim: false }).block(bloque("Lo que te deduces"));
        f.render_widget(resumen, zr);
        if let Some((_, s)) = &mut self.lista {
            s.dibujar(f, area);
        }
    }
}

/// Los ficheros de documentos/ para elegir uno (el más nuevo arriba), con el gasto o la factura que ya lo usa.
fn lista_documentos(ctx: &Ctx, actual: &str) -> Option<Selector> {
    let mut nombres: Vec<String> = std::fs::read_dir(ctx.dir.join("documentos"))
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .collect();
    if nombres.is_empty() {
        return None;
    }
    nombres.sort_by(|a, b| b.cmp(a));
    let usado = |n: &str| -> String {
        if let Some(g) = ctx.datos.gastos.iter().find(|g| g.doc == n) {
            return format!("en {}", super::gastos::nombre(g));
        }
        match ctx.datos.facturas.iter().find(|f| f.doc == n) {
            Some(f) => format!("en la factura {}", if f.numero.is_empty() { &f.cliente.nombre } else { &f.numero }),
            None => String::new(),
        }
    };
    let mut els = vec![(String::new(), "(ninguno)".to_string(), "sin documento".to_string())];
    els.extend(nombres.into_iter().map(|n| {
        let u = usado(&n);
        (n.clone(), n, u)
    }));
    Some(Selector::new("Documento (en documentos/)", els).con_elegido(actual))
}
