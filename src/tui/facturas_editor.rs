//! El editor de un borrador: los datos (cliente, operación, fechas, retención…),
//! las líneas y los totales, que se recalculan al momento. No guarda nada: se lo pide a la pantalla de Facturas.
use super::clientes::{formulario_cliente, leer_cliente, opciones_operacion, validar_nif};
use super::facturas::{avisos, cant, fila, linea_vacia};
use crate::dominio::factura::{self, totales_factura, OPERACIONES};
use crate::dominio::fmt::{self, dmy, eur, pct};
use crate::dominio::{uid, Cliente, Datos, Factura, Linea, Num};
use crate::tui::widgets::{bloque, derecha, normalizar, tema, Eleccion, Entrada, Formulario, Resultado, Selector};
use crate::tui::Ctx;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

/// Dónde están las teclas: en los datos de arriba o en la lista de líneas.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Zona {
    Datos,
    Lineas,
}

/// Ventanas que se abren encima del editor.
enum Sub {
    /// Elegir el cliente.
    Clientes(Selector),
    /// Los datos de un cliente; `true` si además se guarda en Clientes.
    Cliente(Formulario, bool),
    /// Una línea y su posición (None: nueva).
    Linea(Formulario, Option<usize>),
}

/// Lo que el editor le pide a la pantalla.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Salida {
    Nada,
    Guardar,
    Emitir,
    /// Esc: cerrar (la pantalla pregunta si hay cambios).
    Salir,
}

pub struct Editor {
    /// La factura que se edita (una copia: hasta guardar no cambia nada en los datos).
    pub f: Factura,
    titulo: String,
    form: Formulario,
    zona: Zona,
    linea: usize,
    tabla: TableState,
    sub: Option<Sub>,
    /// Cómo estaba al abrirla, para saber si hay cambios sin guardar.
    orig: String,
}

/* Claves especiales del selector de clientes. */
const NUEVO: &str = "__nuevo";
const MANO: &str = "__mano";
const NADIE: &str = "__nadie";

fn instantanea(f: &Factura) -> String {
    serde_json::to_string(f).unwrap_or_default()
}

/// «21» para 21 %, «5.5» para 5,5 % (la clave de las opciones de %).
fn clave_pct(x: f64) -> String {
    format!("{}", fmt::r2(x))
}

fn resumen_cliente(c: &Cliente) -> String {
    if c.nombre.trim().is_empty() {
        return "Sin datos (particular: factura simplificada)".into();
    }
    let mut v = vec![c.nombre.clone()];
    if !c.nif.is_empty() {
        v.push(c.nif.clone());
    }
    if !c.pais.is_empty() && c.pais != "ES" {
        v.push(c.pais.clone());
    }
    v.join(" · ")
}

fn form_linea(l: &Linea, titulo: &str, nacional: bool) -> Formulario {
    let mut ivas: Vec<(String, String)> = [21.0, 10.0, 4.0, 0.0].iter().map(|&x| (clave_pct(x), pct(x))).collect();
    let k = clave_pct(l.iva.v());
    if !ivas.iter().any(|(a, _)| *a == k) {
        ivas.push((k.clone(), pct(l.iva.v())));
    }
    let ivas: Vec<(&str, &str)> = ivas.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    Formulario::new(titulo)
        .texto("concepto", "Concepto", &l.concepto)
        .ayuda("Qué has hecho o vendido.")
        .numero("cantidad", "Cantidad", l.cantidad.0.or(Some(1.0)))
        .numero("precio", "Precio (€, sin IVA)", l.precio.0)
        .ayuda("En negativo para un descuento.")
        .opciones("iva", "IVA", &ivas, &k)
        .ayuda(if nacional { "← → para cambiarlo. Enter guarda la línea." } else { "Esta factura va sin IVA: aquí no cuenta." })
}

fn leer_linea(fm: &Formulario, l: &mut Linea) -> Result<(), String> {
    let concepto = fm.get("concepto");
    if concepto.is_empty() {
        return Err("Pon qué has hecho o vendido.".into());
    }
    if !fmt::es_num(&fm.get("cantidad")) {
        return Err("La cantidad no es un número.".into());
    }
    if !fmt::es_num(&fm.get("precio")) {
        return Err("El precio no es un número.".into());
    }
    l.concepto = concepto;
    l.cantidad = Num(Some(fm.num("cantidad").unwrap_or(1.0)));
    l.precio = Num(fm.num("precio"));
    l.iva = Num::de(fmt::parse_num(&fm.get("iva")));
    Ok(())
}

impl Editor {
    fn con(mut f: Factura, titulo: &str, d: &Datos) -> Self {
        // Las líneas vacías (siempre queda una) no pintan nada aquí.
        f.lineas.retain(|l| !(l.concepto.trim().is_empty() && l.precio.v() == 0.0));
        let mut e = Editor { f, titulo: titulo.into(), form: Formulario::default(), zona: Zona::Datos, linea: 0, tabla: TableState::default(), sub: None, orig: String::new() };
        e.form = e.formulario(d);
        // Como queda al leer el formulario sin tocar nada (p. ej. una retención vacía pasa a 0).
        let _ = e.leer();
        e.orig = instantanea(&e.f);
        e
    }

    /// Factura nueva: empieza eligiendo el cliente.
    pub fn nueva(f: Factura, d: &Datos) -> Self {
        let mut e = Editor::con(f, "Nueva factura (borrador)", d);
        e.abrir_selector(d);
        e
    }

    /// Un borrador guardado.
    pub fn abrir(f: &Factura, d: &Datos) -> Self {
        Editor::con(f.clone(), "Editar borrador", d)
    }

    /// Copia de otra factura (todavía sin guardar).
    pub fn copia(f: Factura, de: &str, d: &Datos) -> Self {
        Editor::con(f, &format!("Copia de {de} (borrador)"), d)
    }

    /// ¿Hay algo que se perdería al salir?
    pub fn cambiado(&mut self) -> bool {
        let _ = self.leer();
        instantanea(&self.f) != self.orig
    }

    pub fn poner_error(&mut self, e: impl Into<String>) {
        self.form.error = Some(e.into());
        self.zona = Zona::Datos;
    }

    fn formulario(&self, d: &Datos) -> Formulario {
        let f = &self.f;
        let op = if f.operacion.is_empty() { "nacional" } else { f.operacion.as_str() };
        let ayuda_op = OPERACIONES.iter().find(|o| o.0 == op).map(|o| o.1).unwrap_or("");
        let mut ret: Vec<(String, String)> = vec![("15".into(), "15 %".into()), ("7".into(), "7 % (nuevo autónomo)".into()), ("0".into(), "Sin retención".into())];
        for x in [f.retencion.v(), d.facturacion.retencion.v()] {
            if !ret.iter().any(|(k, _)| *k == clave_pct(x)) {
                ret.push((clave_pct(x), pct(x)));
            }
        }
        let ret: Vec<(&str, &str)> = ret.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let mut fm = Formulario::new(self.titulo.clone())
            .texto("cliente", "Cliente", &resumen_cliente(&f.cliente))
            .ayuda("Enter: elegir uno de tus clientes, crear uno nuevo o escribir sus datos.")
            .opciones("operacion", "Operación", &opciones_operacion(), op)
            .ayuda(ayuda_op);
        if op == "exenta" {
            fm = fm.texto("exencion", "Motivo de la exención", &f.exencion).ayuda("Ej.: art. 20.Uno.9º Ley 37/1992 (enseñanza). Vacío: art. 20 Ley 37/1992.");
        }
        fm.fecha("fecha", "Fecha", &dmy(&f.fecha))
            .ayuda("19/10/2026 o 2026-10-19.")
            .fecha("vencimiento", "Vencimiento", &dmy(&f.vencimiento))
            .ayuda("Vacío si no tiene.")
            .fecha("fecha_op", "Fecha del servicio", &dmy(&f.fecha_operacion))
            .ayuda("Solo si el servicio es de otra fecha; si no, déjala vacía.")
            .opciones("retencion", "Retención IRPF", &ret, &clave_pct(f.retencion.v()))
            .ayuda("A empresas y autónomos de España. A particulares y a clientes de fuera, sin retención.")
            .texto("forma_pago", "Forma de pago", &f.forma_pago)
            .ayuda("Transferencia bancaria, Bizum, Tarjeta, Efectivo, Domiciliación…")
            .texto("notas", "Notas", &f.notas)
            .ayuda("Opcional: salen en la factura.")
    }

    /// Vuelve a hacer el formulario (cambia el cliente o la operación) sin perder lo escrito ni el foco.
    fn rehacer(&mut self, d: &Datos) {
        let viejo = std::mem::take(&mut self.form);
        self.form = self.formulario(d);
        for c in &mut self.form.campos {
            if !matches!(c.clave.as_str(), "cliente" | "operacion" | "retencion")
                && let Some(v) = viejo.campos.iter().find(|x| x.clave == c.clave)
            {
                c.entrada = v.entrada.clone();
            }
        }
        if let Some(i) = self.form.campos.iter().position(|c| c.clave == viejo.foco_clave()) {
            self.form.foco = i;
        }
        self.form.error = viejo.error;
    }

    /// Pasa el formulario a la factura. Lo que no es válido (una fecha mal escrita) se queda como estaba y se
    /// devuelve el primer error.
    pub fn leer(&mut self) -> Result<(), String> {
        let fm = &self.form;
        let mut err: Option<String> = None;
        let op = fm.get("operacion");
        if !op.is_empty() {
            self.f.operacion = op;
        }
        if fm.campos.iter().any(|c| c.clave == "exencion") {
            self.f.exencion = fm.get("exencion");
        }
        match fm.fecha_iso("fecha") {
            Some(x) => self.f.fecha = x,
            None => err = Some("La fecha de la factura no es válida (19/10/2026).".into()),
        }
        for (k, nombre) in [("vencimiento", "El vencimiento"), ("fecha_op", "La fecha del servicio")] {
            let v = if fm.get(k).is_empty() { Some(String::new()) } else { fm.fecha_iso(k) };
            match v {
                Some(x) if k == "vencimiento" => self.f.vencimiento = x,
                Some(x) => self.f.fecha_operacion = x,
                None => {
                    err.get_or_insert_with(|| format!("{nombre} no es una fecha válida (19/10/2026)."));
                }
            }
        }
        self.f.retencion = Num::de(fmt::parse_num(&fm.get("retencion")));
        self.f.forma_pago = fm.get("forma_pago");
        self.f.notas = fm.get("notas");
        err.map_or(Ok(()), Err)
    }

    /// Tras cambiar un campo: al cambiar de operación, la retención por defecto y el campo de la
    /// exención.
    fn sincronizar(&mut self, d: &Datos) {
        let antes = self.f.operacion.clone();
        let _ = self.leer();
        if self.f.operacion != antes {
            self.f.retencion = if self.f.operacion == "nacional" { d.facturacion.retencion } else { Num::de(0.0) };
            self.rehacer(d);
        }
    }

    /// Copia el cliente en la factura; su operación habitual pasa a la factura.
    fn poner_cliente(&mut self, c: &Cliente, d: &Datos) {
        self.f.cliente = c.clone();
        if !c.operacion.is_empty() {
            self.f.operacion = c.operacion.clone();
        }
        // retención solo a quien tiene NIF español (empresa o autónomo) en una factura con IVA.
        self.f.retencion = if self.f.operacion == "nacional" && validar_nif(&c.nif) { d.facturacion.retencion } else { Num::de(0.0) };
        self.rehacer(d);
        self.form.foco = 1.min(self.form.campos.len() - 1);
    }

    fn abrir_selector(&mut self, d: &Datos) {
        let mut idx: Vec<usize> = (0..d.clientes.len()).collect();
        idx.sort_by_cached_key(|&i| normalizar(&d.clientes[i].nombre));
        let mut actual = String::new();
        let mut v: Vec<(String, String, String)> = Vec::new();
        for i in idx {
            let c = &d.clientes[i];
            let clave = if c.id.is_empty() { format!("#{i}") } else { c.id.clone() };
            if !c.id.is_empty() && c.id == self.f.cliente.id {
                actual = clave.clone();
            }
            let det: Vec<&str> = [c.nif.as_str(), c.pais.as_str(), factura::nombre_operacion(&c.operacion)].into_iter().filter(|s| !s.is_empty()).collect();
            v.push((clave, c.nombre.clone(), det.join(" · ")));
        }
        let tiene = !self.f.cliente.nombre.trim().is_empty();
        v.push((NUEVO.into(), "+ Crear un cliente nuevo…".into(), "se guarda en Clientes".into()));
        v.push((MANO.into(), if tiene { "Cambiar los datos del cliente en esta factura…" } else { "Escribir los datos a mano…" }.into(), "solo en esta factura".into()));
        v.push((NADIE.into(), "Sin datos del cliente".into(), "particular: factura simplificada".into()));
        self.sub = Some(Sub::Clientes(Selector::new("¿Para quién es la factura?", v).con_elegido(&actual)));
    }

    fn elegir_cliente(&mut self, clave: &str, ctx: &mut Ctx) {
        match clave {
            NUEVO => {
                let c = Cliente { pais: "ES".into(), operacion: self.f.operacion.clone(), ..Default::default() };
                self.sub = Some(Sub::Cliente(formulario_cliente(&c, "Nuevo cliente"), true));
            }
            MANO => {
                let mut c = self.f.cliente.clone();
                c.operacion = self.f.operacion.clone();
                self.sub = Some(Sub::Cliente(formulario_cliente(&c, "Cliente (solo en esta factura)"), false));
            }
            NADIE => self.poner_cliente(&Cliente { pais: "ES".into(), ..Default::default() }, &ctx.datos),
            _ => {
                let c = ctx.datos.clientes.iter().enumerate().find(|(i, c)| c.id == clave || format!("#{i}") == clave).map(|(_, c)| c.clone());
                if let Some(c) = c {
                    self.poner_cliente(&c, &ctx.datos);
                }
            }
        }
    }

    pub fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Salida {
        if let Some(mut sub) = self.sub.take() {
            if self.tecla_sub(&mut sub, k, ctx) && self.sub.is_none() {
                self.sub = Some(sub);
            }
            return Salida::Nada;
        }
        let ctrl = crate::tui::widgets::es_ctrl(&k);
        match k.code {
            KeyCode::Char('s') if ctrl => return Salida::Guardar,
            KeyCode::Char('e') if ctrl => return Salida::Emitir,
            KeyCode::Esc => return Salida::Salir,
            KeyCode::Tab | KeyCode::BackTab => {
                self.zona = if self.zona == Zona::Datos { Zona::Lineas } else { Zona::Datos };
                return Salida::Nada;
            }
            _ => {}
        }
        match self.zona {
            Zona::Datos => self.tecla_datos(k, ctx),
            Zona::Lineas => self.tecla_lineas(k, ctx),
        }
        Salida::Nada
    }

    /// Devuelve true si la ventana sigue abierta.
    fn tecla_sub(&mut self, sub: &mut Sub, k: KeyEvent, ctx: &mut Ctx) -> bool {
        match sub {
            Sub::Clientes(s) => match s.tecla(k) {
                Eleccion::Sigue => true,
                Eleccion::Cancelado => false,
                Eleccion::Elegido(clave) => {
                    self.elegir_cliente(&clave, ctx);
                    false
                }
            },
            Sub::Cliente(fm, guardar) => match fm.tecla(k) {
                Resultado::Cancelar => false,
                Resultado::Guardar => {
                    let mut c = if *guardar { Cliente { id: uid(), ..Default::default() } } else { self.f.cliente.clone() };
                    if let Err(e) = leer_cliente(fm, &mut c) {
                        fm.error = Some(e);
                        return true;
                    }
                    if *guardar {
                        ctx.datos.clientes.push(c.clone());
                        if ctx.guardar() {
                            ctx.ok(format!("Cliente «{}» guardado en Clientes", c.nombre));
                        }
                    }
                    self.poner_cliente(&c, &ctx.datos);
                    false
                }
                Resultado::Sigue | Resultado::Cambio => true,
            },
            Sub::Linea(fm, i) => match fm.tecla(k) {
                Resultado::Cancelar => false,
                Resultado::Guardar => {
                    let mut l = match *i {
                        Some(i) => self.f.lineas.get(i).cloned().unwrap_or_default(),
                        None => linea_vacia(&ctx.datos),
                    };
                    if let Err(e) = leer_linea(fm, &mut l) {
                        fm.error = Some(e);
                        return true;
                    }
                    match *i {
                        Some(i) if i < self.f.lineas.len() => self.f.lineas[i] = l,
                        _ => {
                            self.f.lineas.push(l);
                            self.linea = self.f.lineas.len() - 1;
                        }
                    }
                    false
                }
                Resultado::Sigue | Resultado::Cambio => true,
            },
        }
    }

    fn tecla_datos(&mut self, k: KeyEvent, ctx: &mut Ctx) {
        let ctrl = crate::tui::widgets::es_ctrl(&k);
        // El cliente no se escribe ahí: se elige (lo que se teclea empieza a buscar).
        if self.form.foco_clave() == "cliente" {
            match k.code {
                KeyCode::Enter | KeyCode::Char(' ') => return self.abrir_selector(&ctx.datos),
                KeyCode::Char(c) if !ctrl => {
                    self.abrir_selector(&ctx.datos);
                    if let Some(Sub::Clientes(s)) = &mut self.sub {
                        s.filtro = Entrada::new(c.to_string());
                        s.estado.select(Some(0));
                    }
                    return;
                }
                KeyCode::Up | KeyCode::Down => {}
                _ => return,
            }
        }
        let ultimo = self.form.foco + 1 >= self.form.campos.len();
        if ultimo && matches!(k.code, KeyCode::Enter | KeyCode::Down) {
            self.zona = Zona::Lineas;
            return;
        }
        if self.form.tecla(k) == Resultado::Cambio {
            self.sincronizar(&ctx.datos);
        }
    }

    fn mover_linea(&mut self, d: isize) {
        let n = self.f.lineas.len() as isize;
        let j = self.linea as isize + d;
        if n > 1 && (0..n).contains(&j) {
            self.f.lineas.swap(self.linea, j as usize);
            self.linea = j as usize;
        }
    }

    fn tecla_lineas(&mut self, k: KeyEvent, ctx: &mut Ctx) {
        let n = self.f.lineas.len();
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let nacional = self.f.operacion == "nacional";
        match k.code {
            KeyCode::Up if shift => self.mover_linea(-1),
            KeyCode::Down if shift => self.mover_linea(1),
            KeyCode::Char('<') => self.mover_linea(-1),
            KeyCode::Char('>') => self.mover_linea(1),
            KeyCode::Up if self.linea == 0 => {
                // Arriba del todo: vuelve al último campo de los datos.
                self.zona = Zona::Datos;
                self.form.foco = self.form.campos.len() - 1;
            }
            KeyCode::Up => self.linea -= 1,
            KeyCode::Down => self.linea = (self.linea + 1).min(n.saturating_sub(1)),
            KeyCode::Home | KeyCode::PageUp => self.linea = 0,
            KeyCode::End | KeyCode::PageDown => self.linea = n.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('e') if n > 0 => {
                let i = self.linea.min(n - 1);
                self.sub = Some(Sub::Linea(form_linea(&self.f.lineas[i], &format!("Línea {}", i + 1), nacional), Some(i)));
            }
            KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('a') | KeyCode::Char('+') | KeyCode::Insert => {
                self.sub = Some(Sub::Linea(form_linea(&linea_vacia(&ctx.datos), "Nueva línea", nacional), None));
            }
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace if n > 0 => {
                let l = self.f.lineas.remove(self.linea.min(n - 1));
                self.linea = self.linea.min(self.f.lineas.len().saturating_sub(1));
                ctx.info(format!("Línea «{}» quitada", l.concepto));
            }
            _ => {}
        }
    }

    pub fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        match &self.sub {
            Some(Sub::Clientes(_)) => vec![("↑↓", "elegir"), ("Enter", "aceptar"), ("Esc", "cancelar")],
            Some(_) => vec![("↑↓", "campo"), ("← →", "opción"), ("Ctrl+S", "aceptar"), ("Esc", "cancelar")],
            None if self.zona == Zona::Lineas => vec![("n", "añadir"), ("Enter", "editar"), ("d", "quitar"), ("< >", "mover"), ("Tab", "datos"), ("Ctrl+S", "guardar"), ("Ctrl+E", "emitir"), ("Esc", "salir")],
            None if self.form.foco_clave() == "cliente" => vec![("Enter", "elegir cliente"), ("↑↓", "campo"), ("Tab", "líneas"), ("Ctrl+S", "guardar"), ("Ctrl+E", "emitir"), ("Esc", "salir")],
            None => vec![("↑↓", "campo"), ("← →", "opción"), ("Tab", "líneas"), ("Ctrl+S", "guardar"), ("Ctrl+E", "emitir"), ("Esc", "salir")],
        }
    }

    pub fn dibujar(&mut self, fr: &mut Frame, area: Rect, ctx: &Ctx) {
        let d = &ctx.datos;
        let t = totales_factura(&self.f);
        let av = avisos(d, &self.f);
        let nacional = self.f.operacion == "nacional";
        let est_aviso = |grave: bool| Style::new().fg(if grave { tema::MAL } else { tema::AVISO });
        if area.width >= 100 {
            let [izq, der] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
            let ancho = izq.width.saturating_sub(4);
            let mut tot = vec![fila("Base imponible", eur(t.base), ancho, Style::new(), Style::new())];
            if nacional {
                for g in &t.desglose {
                    tot.push(fila(format!("IVA {}", pct(g.tipo)), eur(g.cuota), ancho, Style::new(), Style::new()));
                }
            } else {
                tot.push(fila("IVA", eur(0.0), ancho, Style::new(), Style::new()));
            }
            if t.retencion != 0.0 {
                tot.push(fila(format!("Retención IRPF {}", pct(self.f.retencion.v())), format!("−{}", eur(t.retencion)), ancho, Style::new(), Style::new()));
            }
            let negrita = Style::new().add_modifier(Modifier::BOLD);
            tot.push(fila("Total a cobrar", eur(t.total), ancho, negrita, negrita));
            // A la izquierda los datos, los totales y los avisos; a la derecha, las líneas.
            let alto_form = self.form.alto() + 2;
            let [a_f, a_t, a_a] = Layout::vertical([Constraint::Length(alto_form), Constraint::Length(tot.len() as u16 + 2), Constraint::Min(0)]).areas(izq);
            self.dibujar_form(fr, a_f);
            self.dibujar_lineas(fr, der, nacional);
            let b = bloque("Totales");
            let dentro = b.inner(a_t);
            fr.render_widget(b, a_t);
            fr.render_widget(Paragraph::new(tot), dentro.inner(ratatui::layout::Margin::new(1, 0)));
            let lineas: Vec<Line> = av.iter().map(|(g, m)| Line::styled(format!(" ! {m}"), est_aviso(*g))).collect();
            fr.render_widget(Paragraph::new(lineas).wrap(Wrap { trim: true }), a_a);
        } else {
            let alto_l = (self.f.lineas.len().clamp(1, 4) + 3) as u16;
            let [a_f, a_l, a_t, a_a] =
                Layout::vertical([Constraint::Min(5), Constraint::Length(alto_l), Constraint::Length(1), Constraint::Length(if av.is_empty() { 0 } else { 1 })]).areas(area);
            self.dibujar_form(fr, a_f);
            self.dibujar_lineas(fr, a_l, nacional);
            let mut s = vec![Span::styled(format!(" Base {}", eur(t.base)), tema::apagado()), Span::styled(format!(" · IVA {}", eur(t.cuota)), tema::apagado())];
            if t.retencion != 0.0 {
                s.push(Span::styled(format!(" · Ret. −{}", eur(t.retencion)), tema::apagado()));
            }
            s.push(Span::styled(format!(" · Total {}", eur(t.total)), Style::new().add_modifier(Modifier::BOLD)));
            fr.render_widget(Paragraph::new(Line::from(s)), a_t);
            if let Some((g, m)) = av.first() {
                let mas = if av.len() > 1 { format!(" (+{} avisos)", av.len() - 1) } else { String::new() };
                fr.render_widget(Paragraph::new(Line::styled(format!(" ! {m}{mas}"), est_aviso(*g))), a_a);
            }
        }
        match &mut self.sub {
            Some(Sub::Clientes(s)) => s.dibujar(fr, area),
            Some(Sub::Cliente(fm, _)) => fm.dibujar_ventana(fr, area, 84),
            Some(Sub::Linea(fm, _)) => fm.dibujar_ventana(fr, area, 64),
            None => {}
        }
    }

    fn dibujar_form(&self, fr: &mut Frame, area: Rect) {
        let activo = self.zona == Zona::Datos && self.sub.is_none();
        let mut b = bloque(&self.titulo);
        if activo {
            b = b.border_style(Style::new().fg(tema::ACENTO));
        }
        let dentro = b.inner(area);
        fr.render_widget(b, area);
        if activo {
            self.form.dibujar(fr, dentro);
        } else {
            // Sin foco: que no se vea el cursor ni la ayuda.
            let mut c = self.form.clone();
            c.foco = c.campos.len();
            c.dibujar(fr, dentro);
        }
    }

    fn dibujar_lineas(&mut self, fr: &mut Frame, area: Rect, nacional: bool) {
        let activo = self.zona == Zona::Lineas && self.sub.is_none();
        let titulo = format!("Líneas ({})", self.f.lineas.len());
        let mut b = bloque(&titulo);
        if activo {
            b = b.border_style(Style::new().fg(tema::ACENTO));
        }
        let dentro = b.inner(area);
        fr.render_widget(b, area);
        if self.f.lineas.is_empty() {
            let txt = if activo { " Sin líneas: pulsa n para añadir una." } else { " Sin líneas: Tab y n para añadir una." };
            fr.render_widget(Paragraph::new(Line::styled(txt, tema::apagado())), dentro);
            return;
        }
        let mut anchos = vec![Constraint::Fill(1), Constraint::Length(7), Constraint::Length(12)];
        let mut cab = vec![Cell::from("Concepto"), Cell::from(derecha("Cant.")), Cell::from(derecha("Precio"))];
        if nacional {
            anchos.push(Constraint::Length(5));
            cab.push(Cell::from(derecha("IVA")));
        }
        anchos.push(Constraint::Length(12));
        cab.push(Cell::from(derecha("Importe")));
        let filas: Vec<Row> = self
            .f
            .lineas
            .iter()
            .map(|l| {
                let mut v = vec![Cell::from(l.concepto.clone()), Cell::from(derecha(cant(l.cantidad.v()))), Cell::from(derecha(if l.precio.vacio() { "—".into() } else { eur(l.precio.v()) }))];
                if nacional {
                    v.push(Cell::from(derecha(pct(l.iva.v()))));
                }
                v.push(Cell::from(derecha(eur(fmt::r2(l.cantidad.v() * l.precio.v())))));
                Row::new(v)
            })
            .collect();
        let tabla = Table::new(filas, anchos).header(Row::new(cab).style(tema::cabecera_tabla())).column_spacing(1).row_highlight_style(tema::seleccion());
        self.linea = self.linea.min(self.f.lineas.len() - 1);
        self.tabla.select(if activo { Some(self.linea) } else { None });
        fr.render_stateful_widget(tabla, dentro, &mut self.tabla);
    }
}
