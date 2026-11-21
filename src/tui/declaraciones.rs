//! Hacienda: lo que tienes presentado según la última consulta (con tu certificado), sus PDF y si la app ha podido
//! leerlos, en una lista sin caja (la cabecera en gris, las fechas cortas y el estado con ✓ y su color) y, debajo,
//! la elegida entera. Con Enter se ve la ficha de una declaración, con las casillas de su PDF. Aquí solo se mira.
//! Solo desde `PRIMER_ANIO`.

use crate::dominio::Datos;
use crate::dominio::fmt;
use crate::hacienda::{self, consulta, Decl, Modelo};
use crate::dominio::PRIMER_ANIO;
use detalle::{Detalle, Memo};
use crate::tui::texto::cuando;
use crate::tui::widgets::{coincide, filas_partidas, linea_atajos, tema, Entrada};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use std::path::PathBuf;
use super::declaraciones_detalle as detalle;

/// Los modelos, en el orden en que se enseñan: (modelo, nombre, nombre corto). Los que no estén aquí salen al
/// final, con su número.
const MODELOS: [(&str, &str, &str); 19] = [
    ("100", "Renta", "Renta"),
    ("714", "Patrimonio", "Patrimonio"),
    ("720", "Bienes en el extranjero", "Bienes fuera"),
    ("721", "Criptomonedas en el extranjero", "Cripto fuera"),
    ("303", "IVA", "IVA"),
    ("130", "IRPF, pago fraccionado", "IRPF fracc."),
    ("131", "IRPF, pago fraccionado (módulos)", "IRPF módulos"),
    ("390", "Resumen anual de IVA", "IVA anual"),
    ("349", "Operaciones con la UE", "Op. UE"),
    ("347", "Operaciones con terceros", "Terceros"),
    ("111", "Retenciones de trabajo y profesionales", "Retenciones"),
    ("115", "Retenciones de alquileres", "Ret. alquiler"),
    ("123", "Retenciones del capital", "Ret. capital"),
    ("180", "Resumen anual de alquileres", "Alquileres año"),
    ("190", "Resumen anual de retenciones", "Retenc. año"),
    ("193", "Resumen anual del capital", "Capital año"),
    ("036", "Declaración censal", "Censal"),
    ("037", "Declaración censal simplificada", "Censal simpl."),
    ("840", "Impuesto de actividades económicas", "IAE"),
];

pub(super) use crate::hacienda::consulta::codigo;

pub(super) fn nombre_modelo(m: &str, corto: bool) -> &'static str {
    MODELOS.iter().find(|x| x.0 == codigo(m)).map(|x| if corto { x.2 } else { x.1 }).unwrap_or("")
}

fn orden_modelo(m: &str) -> usize {
    MODELOS.iter().position(|x| x.0 == codigo(m)).unwrap_or(MODELOS.len())
}

fn es_censal(m: &str) -> bool {
    matches!(codigo(m), "036" | "037")
}

/// «3T», «octubre», «Anual» (y «—» en las censales, que no tienen periodo).
fn nombre_periodo(d: &Decl) -> String {
    if es_censal(&d.modelo) {
        return "—".into();
    }
    if d.trimestre().is_some() {
        return d.periodo.clone();
    }
    match d.periodo.parse::<usize>() {
        Ok(m) if d.periodo.len() == 2 && (1..=12).contains(&m) => fmt::MESES[m - 1].into(),
        _ => "Anual".into(),
    }
}

/// El trimestre para buscar su PDF por el nombre (0: anual); None en los mensuales, que no se guardan así.
fn q_pdf(d: &Decl) -> Option<u32> {
    match d.trimestre() {
        Some(q) => Some(q),
        None if d.periodo == "0A" || d.periodo.is_empty() => Some(0),
        None => None,
    }
}

/// «303 3T 2026», «347 de 2024», «036 del 15 oct 2026».
pub(super) fn que_es(d: &Decl) -> String {
    let m = codigo(&d.modelo);
    if es_censal(m) {
        format!("{m} del {}", fmt::fecha_corta(&d.fecha))
    } else if let Some(q) = d.trimestre() {
        format!("{m} {q}T {}", d.ejercicio)
    } else {
        format!("{m} de {}", d.ejercicio)
    }
}

/// Cuántas columnas caben: recorre las opciones (de la más completa a la más corta) y da la primera que cabe.
pub(super) fn primera_que_cabe<T: Copy>(ancho: usize, opciones: &[(usize, T)]) -> T {
    opciones.iter().find(|(w, _)| *w <= ancho).or(opciones.last()).map(|x| x.1).expect("al menos una opción")
}

/// Cómo ha leído la app el PDF de una declaración.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Lectura {
    /// La app no lee este modelo: solo se ven sus casillas tal cual.
    NoLee,
    /// No hay PDF que leer.
    SinPdf,
    /// La app tiene sus casillas (las que leyó del PDF).
    Leida,
    /// No se ha podido leer: el motivo (de `Lecturas.fallos`).
    Fallo(String),
    /// Aún sin leer (true: se están leyendo los PDF ahora).
    Pendiente(bool),
}

fn lectura(ctx: &Ctx, d: &Decl, pdf: Option<&PathBuf>) -> Lectura {
    let m = codigo(&d.modelo);
    let Some(modelo) = Modelo::de_codigo(m).filter(|_| d.ejercicio >= PRIMER_ANIO) else {
        return Lectura::NoLee;
    };
    if pdf.is_none() {
        return Lectura::SinPdf;
    }
    let l = &ctx.datos.lecturas;
    let q = d.trimestre().unwrap_or(0);
    let hay = hacienda::leida(&ctx.datos, d.ejercicio, q, modelo);
    // Si se leyó bien antes, vale lo de antes (el fallo de ahora sale en el aviso de fallos, abajo).
    match l.fallos.get(&hacienda::clave(d.ejercicio, q, m)) {
        _ if hay => Lectura::Leida,
        Some(e) => Lectura::Fallo(e.mensaje.clone()),
        None => Lectura::Pendiente(ctx.leyendo.is_some()),
    }
}

/// Una declaración de la lista, con lo que se enseña de ella ya mirado.
#[derive(Debug, Clone)]
struct Fila {
    d: Decl,
    /// El PDF de esta declaración o, si no está, otro guardado del mismo modelo y periodo.
    pdf: Option<PathBuf>,
    /// El PDF es justo el de esta (no otro del mismo periodo).
    exacto: bool,
    /// Hay otra posterior del mismo modelo y periodo (una complementaria o sustitutiva).
    corregida: bool,
    lectura: Lectura,
    texto: String,
}

/// Lo que dice la última consulta de ese año.
#[derive(Debug, Clone, PartialEq)]
enum Consulta {
    Nunca,
    /// Se miraron solo algunos modelos (cuándo, cuáles).
    Parcial(String, Vec<String>),
    Hecha(String),
}

fn consulta(d: &Datos, anio: i32) -> Consulta {
    if let Some(f) = hacienda::consultado(d, anio) {
        return Consulta::Hecha(f);
    }
    let Some(h) = d.hacienda.get(&anio).filter(|h| !h.modelos.is_empty()) else { return Consulta::Nunca };
    let fecha = h.modelos.values().map(|m| m.fecha.clone()).max().unwrap_or_default();
    Consulta::Parcial(fecha, modelos_consultados(d, anio))
}

/// Los modelos que no se pudieron consultar: (modelo, error). «No ha presentado» no es un fallo.
fn errores(d: &Datos, anio: i32) -> Vec<(String, String)> {
    let Some(h) = d.hacienda.get(&anio) else { return Vec::new() };
    h.errores
        .iter()
        .map(|(m, x)| (m.clone(), if x.error.is_empty() { "error desconocido".to_string() } else { x.error.clone() }))
        .filter(|(_, err)| !consulta::sin_resultados(err) && !consulta::no_disponible(err))
        .collect()
}

/// Los modelos consultados de un año, en el orden de la app.
fn modelos_consultados(d: &Datos, anio: i32) -> Vec<String> {
    let mut v: Vec<String> = d.hacienda.get(&anio).map(|h| h.modelos.keys().map(|k| codigo(k).to_string()).collect()).unwrap_or_default();
    v.sort_by_key(|m| (orden_modelo(m), m.clone()));
    v
}

/// Los años consultados que enseña la app (desde `PRIMER_ANIO`).
fn anios_consultados(d: &Datos) -> Vec<i32> {
    hacienda::anios_consultados(d).into_iter().filter(|&a| a >= PRIMER_ANIO).collect()
}

fn cargar(ctx: &Ctx, anio: i32, todos: bool) -> Vec<Fila> {
    let anios = if todos { anios_consultados(&ctx.datos) } else { vec![anio] };
    // La última de cada modelo y periodo es la que vale (en las censales, cada una va aparte).
    let clave = |x: &Decl| if es_censal(&x.modelo) { x.justificante.clone() } else { format!("{}-{}", codigo(&x.modelo), x.periodo) };
    let mut v = Vec::new();
    for a in anios {
        let ds = hacienda::declaraciones(&ctx.datos, a);
        for d in ds.iter().filter(|d| d.ejercicio >= PRIMER_ANIO) {
            let corregida = !es_censal(&d.modelo) && ds.iter().any(|o| clave(o) == clave(d) && o.ejercicio == d.ejercicio && o.fecha > d.fecha);
            let exacto = hacienda::pdf_de(&ctx.dir, &ctx.datos, d);
            let es_exacto = exacto.is_some();
            let pdf = exacto.or_else(|| q_pdf(d).and_then(|q| hacienda::pdf_periodo(&ctx.dir, d.ejercicio, q, codigo(&d.modelo))));
            let lectura = lectura(ctx, d, pdf.as_ref());
            let texto = format!(
                "{} {} {} {} {} {} {} {} {} {} {}",
                d.ejercicio,
                codigo(&d.modelo),
                nombre_modelo(&d.modelo, false),
                nombre_periodo(d),
                d.periodo,
                fmt::dmy(&d.fecha),
                fmt::fecha_corta(&d.fecha),
                d.justificante,
                d.csv,
                if d.anterior.is_empty() { String::new() } else { format!("complementaria sustitutiva corrige {}", d.anterior) },
                if corregida { "corregida" } else { "" },
            );
            let texto = if matches!(lectura, Lectura::Fallo(..)) { format!("{texto} no se lee") } else { texto };
            v.push(Fila { d: d.clone(), pdf, exacto: es_exacto, corregida, lectura, texto });
        }
    }
    v.sort_by(|a, b| {
        (std::cmp::Reverse(a.d.ejercicio), orden_modelo(&a.d.modelo), a.d.periodo.as_str(), a.d.fecha.as_str()).cmp(&(
            std::cmp::Reverse(b.d.ejercicio),
            orden_modelo(&b.d.modelo),
            b.d.periodo.as_str(),
            b.d.fecha.as_str(),
        ))
    });
    v
}

#[derive(Default)]
pub struct Declaraciones {
    /// Todos los años consultados, no solo el elegido.
    todos: bool,
    filas: Vec<Fila>,
    /// (año, todos) con que se cargaron las filas.
    cargadas: Option<(i32, bool)>,
    /// La elegida, entre las que se ven (con el filtro).
    sel: usize,
    estado: TableState,
    buscando: bool,
    busca: Entrada,
    /// El ancho de la última vez que se pintó (para saber si caben los dos paneles).
    ancho: u16,
    /// La ficha abierta (Enter), si hay una.
    detalle: Option<Detalle>,
    /// Las casillas impresas de los PDF ya mirados.
    memo: Memo,
}

impl Declaraciones {
    fn refrescar(&mut self, ctx: &Ctx) {
        let antes = self.elegida().map(|f| (f.d.modelo.clone(), f.d.justificante.clone()));
        self.filas = cargar(ctx, ctx.anio, self.todos);
        self.cargadas = Some((ctx.anio, self.todos));
        // Si la elegida sigue en la lista, se queda elegida.
        if let Some((m, j)) = antes
            && let Some(i) = self.visibles().iter().position(|&i| self.filas[i].d.modelo == m && self.filas[i].d.justificante == j)
        {
            self.sel = i;
        }
        self.sel = self.sel.min(self.visibles().len().saturating_sub(1));
    }

    fn visibles(&self) -> Vec<usize> {
        (0..self.filas.len()).filter(|&i| coincide(&self.filas[i].texto, &self.busca.valor)).collect()
    }

    fn elegida(&self) -> Option<&Fila> {
        self.visibles().get(self.sel).map(|&i| &self.filas[i])
    }

    fn mover(&mut self, d: isize) {
        let n = self.visibles().len();
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + d).clamp(0, n as isize - 1) as usize;
    }

    /// Abre la ficha de la elegida, con las casillas de su PDF.
    fn ver(&mut self, ctx: &mut Ctx) {
        let Some(f) = self.elegida().cloned() else {
            ctx.error("No hay ninguna declaración elegida.");
            return;
        };
        // Se sacan ya sus casillas (la primera vez lee el PDF; luego, de memoria).
        if let Some(p) = &f.pdf {
            let _ = self.memo.casillas(p);
        }
        self.detalle = Some(Detalle { d: f.d, pdf: f.pdf, exacto: f.exacto, lectura: f.lectura, desplaz: 0, alto: 1 });
    }

    fn abrir(&self, ctx: &mut Ctx) {
        let d = match (&self.detalle, self.elegida()) {
            (Some(x), _) => &x.d,
            (None, Some(f)) => &f.d,
            (None, None) => {
                ctx.error("No hay ninguna declaración elegida.");
                return;
            }
        };
        // Se vuelve a mirar en disco, por si se ha descargado mientras tanto.
        let p = hacienda::pdf_de(&ctx.dir, &ctx.datos, d).or_else(|| q_pdf(d).and_then(|q| hacienda::pdf_periodo(&ctx.dir, d.ejercicio, q, codigo(&d.modelo))));
        match p {
            Some(p) => ctx.abrir_ruta(&p),
            None => ctx.error(format!("No está el PDF del {}: se descarga al traer lo último con tu certificado.", que_es(d))),
        }
    }

    /* ---------------- lista de declaraciones ---------------- */

    /// La lista, sin caja: arriba, de qué año (o de todos), cuántas y cuándo se consultó (y en AVISO lo que no se
    /// pudo consultar) y, si se busca, la búsqueda; la tabla, con la cabecera en gris, las fechas cortas, dónde está
    /// el PDF y cómo lo ha leído la app (✓ en BIEN, ✗ en MAL), y la elegida en el fondo suave de la sección; y abajo,
    /// la elegida entera.
    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let aviso = Style::new().fg(tema::AVISO);

        // Arriba: el año (o todos), cuántas y cuándo se consultó; debajo, si algo falló.
        let n = self.filas.len();
        let vis = self.visibles();
        let cuantas = match n {
            0 => String::new(),
            n if vis.len() != n => format!(" · {} de {n}", vis.len()),
            1 => " · 1 declaración".into(),
            n => format!(" · {n} declaraciones"),
        };
        let c = consulta(&ctx.datos, ctx.anio);
        let mut l0 = Vec::new();
        let mut avisos: Vec<Line> = Vec::new();
        if self.todos {
            l0.push(Span::styled("todos los años", negrita));
            let anios = anios_consultados(&ctx.datos);
            if let (Some(a), Some(z)) = (anios.last(), anios.first()) {
                l0.push(Span::styled(format!(" · consultados con Hacienda: {a}–{z}{cuantas}"), gris));
            }
        } else {
            l0.push(Span::styled(ctx.anio.to_string(), negrita));
            l0.push(Span::styled(cuantas, gris));
            match &c {
                Consulta::Hecha(fe) => l0.push(Span::styled(format!(" · consultado con Hacienda {}", cuando(fe)), gris)),
                Consulta::Parcial(fe, ms) => avisos.push(Line::styled(
                    format!("Consulta incompleta: solo {} ({})", ms.join(", "), if fe.is_empty() { "sin fecha".into() } else { cuando(fe) }),
                    aviso,
                )),
                Consulta::Nunca => {}
            }
            for (m, e) in errores(&ctx.datos, ctx.anio) {
                avisos.push(Line::styled(format!("No se pudo consultar el {m}: {e}"), aviso));
            }
        }
        avisos.truncate(2);
        self.sel = self.sel.min(vis.len().saturating_sub(1));
        let hay_filtro = self.buscando || !self.busca.valor.is_empty();
        // Abajo, la elegida (con una fila de aire encima): sus líneas, partidas si no caben.
        let elegida = self.elegida().map(lineas_elegida).unwrap_or_default();
        let alto_detalle = if elegida.is_empty() { 0 } else { 1 + elegida.iter().map(|l| filas_partidas(&l.to_string(), area.width as usize)).sum::<u16>() };
        let [a_arriba, a_busca, _, a_tabla, a_detalle] = Layout::vertical([
            Constraint::Length(1 + avisos.len() as u16),
            Constraint::Length(u16::from(hay_filtro)),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(alto_detalle),
        ])
        .areas(area);
        let fila0 = Rect { height: a_arriba.height.min(1), ..a_arriba };
        let usado = Line::from(l0.clone()).width();
        f.render_widget(Paragraph::new(Line::from(l0)), fila0);
        // A la derecha, si caben, las teclas de buscar y de los años (abajo no siempre caben).
        if !self.buscando {
            let t = if self.todos { "solo el año" } else { "todos los años" };
            let teclas = [linea_atajos(&[("/", "buscar"), ("t", t)]), linea_atajos(&[("t", t)])].into_iter().find(|l| usado + 5 + l.width() <= area.width as usize);
            if let Some(l) = teclas {
                f.render_widget(Paragraph::new(l.right_aligned()), fila0);
            }
        }
        f.render_widget(Paragraph::new(avisos), Rect { y: a_arriba.y + 1, height: a_arriba.height.saturating_sub(1), ..a_arriba });

        if hay_filtro {
            let [etq, txt] = Layout::horizontal([Constraint::Length(7), Constraint::Min(1)]).areas(a_busca);
            f.render_widget(Paragraph::new(Span::styled("buscar", gris)), etq);
            self.busca.dibujar(f, txt, self.buscando, negrita);
        }

        if vis.is_empty() {
            let t = self.vacia(ctx, &c);
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), a_tabla);
            return;
        }

        // Columnas según el ancho: primero se quita el justificante (está abajo) y luego se acorta el nombre.
        const ESPACIO: u16 = 2;
        let w = a_tabla.width as usize;
        let largo = vis.iter().map(|&i| nombre_modelo(&self.filas[i].d.modelo, false).chars().count()).max().unwrap_or(0).max(6);
        let corto = vis.iter().map(|&i| nombre_modelo(&self.filas[i].d.modelo, true).chars().count()).max().unwrap_or(0).max(6);
        let fechas: Vec<String> = vis.iter().map(|&i| fecha_lista(&self.filas[i].d)).collect();
        let fecha_w = fechas.iter().map(|s| s.chars().count()).max().unwrap_or(0).max("presentada".len());
        let anio_w = if self.todos { 4 + ESPACIO as usize } else { 0 };
        // fijo = año + modelo + nombre + periodo + fecha + justificante + PDF, con sus espacios; el estado, al menos 12.
        let e = ESPACIO as usize;
        let fijo = |nombre: usize, just: bool| anio_w + 6 + e + nombre + e + 7 + e + fecha_w + e + if just { 13 + e } else { 0 } + 3 + e + 12;
        let (nombre_w, con_just) = primera_que_cabe(
            w,
            &[(fijo(largo, true), (largo, true)), (fijo(largo, false), (largo, false)), (fijo(corto, false), (corto, false)), (0, (corto.min(8), false))],
        );
        let nombre_corto = nombre_w < largo;
        let estado_w = w.saturating_sub(fijo(nombre_w, con_just) - 12);

        let mut cab = Vec::new();
        if self.todos {
            cab.push(Cell::from("año"));
        }
        cab.extend([Cell::from("modelo"), Cell::from("nombre"), Cell::from("periodo"), Cell::from("presentada")]);
        if con_just {
            cab.push(Cell::from("justificante"));
        }
        cab.extend([Cell::from("PDF"), Cell::from("estado")]);

        let filas: Vec<Row> = vis
            .iter()
            .zip(fechas)
            .map(|(&i, fecha)| {
                let x = &self.filas[i];
                let d = &x.d;
                let mut c: Vec<Cell> = Vec::new();
                if self.todos {
                    c.push(Cell::from(d.ejercicio.to_string()));
                }
                c.push(Cell::from(Span::styled(codigo(&d.modelo).to_string(), negrita)));
                c.push(Cell::from(nombre_modelo(&d.modelo, nombre_corto)));
                c.push(Cell::from(nombre_periodo(d)));
                c.push(Cell::from(Span::styled(fecha, gris)));
                if con_just {
                    c.push(Cell::from(Span::styled(d.justificante.clone(), gris)));
                }
                c.push(Cell::from(simbolo_pdf(x)));
                c.push(Cell::from(estado(x, estado_w)));
                Row::new(c)
            })
            .collect();

        let mut anchos = Vec::new();
        if self.todos {
            anchos.push(Constraint::Length(4));
        }
        anchos.extend([Constraint::Length(6), Constraint::Length(nombre_w as u16), Constraint::Length(7), Constraint::Length(fecha_w as u16)]);
        if con_just {
            anchos.push(Constraint::Length(13));
        }
        anchos.extend([Constraint::Length(3), Constraint::Min(0)]);

        let tabla = Table::new(filas, anchos).header(Row::new(cab).style(tema::cabecera_tabla())).column_spacing(ESPACIO).row_highlight_style(tema::seleccion());
        self.estado.select(Some(self.sel));
        f.render_stateful_widget(tabla, a_tabla, &mut self.estado);

        // Abajo, tras una fila de aire: la elegida, entera (partida si no cabe).
        let a_elegida = Rect { y: a_detalle.y + 1, height: a_detalle.height.saturating_sub(1), ..a_detalle };
        f.render_widget(Paragraph::new(elegida).wrap(Wrap { trim: false }), a_elegida);
    }

    /// Qué decir cuando no hay nada que enseñar.
    fn vacia(&self, ctx: &Ctx, c: &Consulta) -> Vec<Line<'static>> {
        let como = "Se consulta con tu certificado: pulsa S para traerlo.";
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        if !self.busca.valor.is_empty() && !self.filas.is_empty() {
            return vec![Line::styled(format!("Nada con «{}».", self.busca.valor), tema::apagado())];
        }
        if self.todos {
            return vec![Line::styled("No se ha consultado ningún año con Hacienda.", negrita), Line::from(""), Line::styled(como, tema::apagado())];
        }
        match c {
            Consulta::Nunca => vec![
                Line::styled(format!("No se ha consultado {} con Hacienda.", ctx.anio), negrita),
                Line::from(""),
                Line::styled(como, tema::apagado()),
                Line::from(""),
                Line::styled("Con «t» ves lo de todos los años consultados.", tema::apagado()),
            ],
            _ => {
                let ms = modelos_consultados(&ctx.datos, ctx.anio);
                vec![
                    Line::styled(format!("Hacienda no tiene ninguna declaración tuya de {}.", ctx.anio), negrita),
                    Line::from(""),
                    Line::styled(format!("Modelos consultados: {}.", if ms.is_empty() { "ninguno".into() } else { ms.join(", ") }), tema::apagado()),
                ]
            }
        }
    }
}

/// La elegida, entera: qué es, cuándo se presentó (con la hora), su justificante y su CSV (y si corrige otra), su
/// PDF y cómo lo ha leído la app (y, si no se puede, por qué).
fn lineas_elegida(x: &Fila) -> Vec<Line<'static>> {
    let d = &x.d;
    let mut l1 = vec![
        Span::styled(format!("{} {}", codigo(&d.modelo), nombre_modelo(&d.modelo, false)), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                " · {} · presentada {}",
                if es_censal(&d.modelo) { d.ejercicio.to_string() } else { format!("{} {}", nombre_periodo(d), d.ejercicio) },
                cuando(&d.fecha)
            ),
            tema::apagado(),
        ),
    ];
    if x.corregida {
        l1.push(Span::styled(" · hay otra posterior", tema::apagado()));
    }
    let mut l2 = vec![Span::styled(format!("justificante {}", d.justificante), tema::apagado())];
    if !d.csv.is_empty() {
        l2.push(Span::styled(format!(" · CSV {}", d.csv), tema::apagado()));
    }
    if !d.anterior.is_empty() {
        l2.push(Span::styled(format!(" · complementaria o sustitutiva de la {}", d.anterior), Style::new().fg(tema::acento())));
    }
    let nombre = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut l3 = match (&x.pdf, x.exacto) {
        (Some(p), true) => vec![Span::styled("● ", Style::new().fg(tema::BIEN)), Span::styled(format!("PDF guardado: {}", nombre(p)), tema::apagado())],
        (Some(p), false) => vec![
            Span::styled("○ ", Style::new().fg(tema::AVISO)),
            Span::styled(format!("Hay un PDF de ese periodo ({}), puede que de otra versión", nombre(p)), tema::apagado()),
        ],
        (None, _) => vec![Span::styled("— Sin PDF: se descarga al traer lo último con tu certificado.", tema::apagado())],
    };
    let mut l4 = None;
    match &x.lectura {
        Lectura::Leida => l3.push(Span::styled(" · leída ✓", Style::new().fg(tema::BIEN))),
        Lectura::Fallo(m) => {
            l3.push(Span::styled(" · no se lee ✗", Style::new().fg(tema::MAL)));
            l4 = Some(Line::styled(format!("No se puede leer: {m}"), Style::new().fg(tema::MAL)));
        }
        Lectura::Pendiente(true) => l3.push(Span::styled(" · leyéndola…", tema::apagado())),
        Lectura::Pendiente(false) => l3.push(Span::styled(" · sin leer", tema::apagado())),
        Lectura::NoLee | Lectura::SinPdf => {}
    }
    let mut v = vec![Line::from(l1), Line::from(l2), Line::from(l3)];
    v.extend(l4);
    v
}

/// Cuándo se presentó, en corto: «14 oct» si es del año de la declaración; si no, con el año: «28 ene 2027». (La
/// hora, en la elegida.)
fn fecha_lista(d: &Decl) -> String {
    let dia = d.fecha.get(..10).unwrap_or(&d.fecha);
    let c = fmt::fecha_corta(dia);
    match c.rsplit_once(' ') {
        Some((dm, _)) if fmt::anio_de(dia) == d.ejercicio => dm.to_string(),
        _ => c,
    }
}

/// Dónde está el PDF: ● el suyo (en BIEN), ○ otro del mismo periodo (en AVISO), — ninguno.
fn simbolo_pdf(x: &Fila) -> Span<'static> {
    match (&x.pdf, x.exacto) {
        (Some(_), true) => Span::styled(" ●", Style::new().fg(tema::BIEN)),
        (Some(_), false) => Span::styled(" ○", Style::new().fg(tema::AVISO)),
        (None, _) => Span::styled(" —", tema::apagado()),
    }
}

/// El estado de una declaración en `w` columnas: cómo ha leído la app su PDF («✓ leída» en BIEN, «✗ no se lee» en
/// MAL, «sin leer» en gris) y, si la hay, su nota: si corrige otra (en el color de la sección) o si la corrigió una
/// posterior; la nota, tan larga como quepa.
fn estado(x: &Fila, w: usize) -> Line<'static> {
    let gris = tema::apagado();
    let mut v = match x.lectura {
        Lectura::Leida => vec![Span::styled("✓ leída", Style::new().fg(tema::BIEN))],
        Lectura::Fallo(..) => vec![Span::styled("✗ no se lee", Style::new().fg(tema::MAL))],
        Lectura::Pendiente(true) => vec![Span::styled("leyéndola…", gris)],
        Lectura::Pendiente(false) => vec![Span::styled("sin leer", gris)],
        Lectura::NoLee | Lectura::SinPdf => vec![],
    };
    let d = &x.d;
    let (notas, st) = if !d.anterior.is_empty() {
        (vec![format!("corrige la {}", d.anterior), "corrige la anterior".into(), "corrige otra".into(), "corrige".into()], Style::new().fg(tema::acento()))
    } else if x.corregida {
        (vec!["corregida después".to_string(), "corregida".into()], gris)
    } else {
        (vec![], gris)
    };
    let sep = if v.is_empty() { "" } else { " · " };
    let libre = w.saturating_sub(Line::from(v.clone()).width() + sep.chars().count());
    if let Some(nota) = notas.iter().find(|t| t.chars().count() <= libre).or(notas.last()) {
        v.push(Span::styled(format!("{sep}{nota}"), st));
    }
    Line::from(v)
}

impl Pantalla for Declaraciones {
    fn titulo(&self) -> &'static str {
        "Declaraciones"
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.ancho = area.width;
        if self.cargadas != Some((ctx.anio, self.todos)) {
            self.refrescar(ctx);
        }
        if let Some(x) = &mut self.detalle {
            let casillas = x.pdf.as_ref().map(|p| self.memo.casillas(p));
            let q = x.d.trimestre().unwrap_or(0);
            let tipo = Modelo::de_codigo(codigo(&x.d.modelo)).and_then(|m| m.leido(&ctx.datos, x.d.ejercicio, q)).and_then(|l| l.tipo);
            x.dibujar(f, area, casillas.as_ref(), tipo);
            return;
        }
        self.dibujar_lista(f, area, ctx);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        if self.buscando {
            match k.code {
                KeyCode::Esc => {
                    self.buscando = false;
                    self.busca = Entrada::default();
                    self.sel = 0;
                }
                KeyCode::Enter => self.buscando = false,
                KeyCode::Up => self.mover(-1),
                KeyCode::Down => self.mover(1),
                KeyCode::PageUp => self.mover(-10),
                KeyCode::PageDown => self.mover(10),
                _ => {
                    if self.busca.tecla(k) {
                        self.sel = 0;
                    }
                }
            }
            return true;
        }
        if let Some(x) = &mut self.detalle {
            let n = x.pdf.as_ref().and_then(|p| self.memo.casillas(p).ok()).map_or(0, |c| c.len());
            let pagina = x.alto.max(1) as isize;
            match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace | KeyCode::Left => self.detalle = None,
                KeyCode::Up => x.mover(-1, n),
                KeyCode::Down => x.mover(1, n),
                KeyCode::PageUp => x.mover(-pagina, n),
                KeyCode::PageDown | KeyCode::Char(' ') => x.mover(pagina, n),
                KeyCode::Home => x.desplaz = 0,
                KeyCode::End => x.mover(isize::MAX / 2, n),
                KeyCode::Char('o') => self.abrir(ctx),
                _ => return false,
            }
            return true;
        }
        let lista = true;
        match k.code {
            KeyCode::Char('/') => self.buscando = true,
            KeyCode::Char('t') => {
                self.todos = !self.todos;
                self.refrescar(ctx);
            }
            KeyCode::Esc if !self.busca.valor.is_empty() => {
                self.busca = Entrada::default();
                self.sel = 0;
            }
            KeyCode::Up if lista => self.mover(-1),
            KeyCode::Down if lista => self.mover(1),
            KeyCode::PageUp if lista => self.mover(-10),
            KeyCode::PageDown if lista => self.mover(10),
            KeyCode::Home if lista => self.sel = 0,
            KeyCode::End if lista => self.mover(isize::MAX / 2),
            KeyCode::Enter if lista => self.ver(ctx),
            KeyCode::Char('o') if lista => self.abrir(ctx),
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.buscando {
            return vec![("Enter", "aceptar"), ("Esc", "borrar"), ("↑↓", "elegir")];
        }
        if self.detalle.is_some() {
            return vec![("Esc", "volver"), ("↑↓", "moverse"), ("o", "abrir PDF")];
        }
        let mut v = vec![("Enter", "casillas"), ("o", "abrir PDF"), ("/", "buscar")];
        v.push(("t", if self.todos { "solo el año" } else { "todos los años" }));
        if !self.busca.valor.is_empty() {
            v.push(("Esc", "quitar filtro"));
        }
        v
    }

    fn capturando(&self) -> bool {
        self.buscando
    }

    /// Con «todos los años», el de arriba no cuenta.
    fn usa_anio(&self) -> bool {
        !self.todos
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        if !self.todos && self.cargadas.is_some_and(|(a, _)| a != ctx.anio) {
            self.sel = 0;
            self.estado = TableState::default();
        }
        self.refrescar(ctx);
        // La ficha abierta: fuera si es de otro año; si no, con la lectura al día (p. ej. al terminar de leer).
        if let Some(x) = &mut self.detalle {
            if !self.todos && x.d.ejercicio != ctx.anio {
                self.detalle = None;
            } else {
                x.lectura = lectura(ctx, &x.d, x.pdf.as_ref());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dominio::hacienda::{Consulta, Decl, Modelo};
    use crate::tui::prueba::{app, ir, pantalla, teclas};

    fn decl(m: &str, y: i32, p: &str, fecha: &str, n: u32, anterior: &str) -> Decl {
        Decl { modelo: m.into(), ejercicio: y, periodo: p.into(), csv: format!("CSV{n}"), justificante: format!("{m}{n:010}"), fecha: fecha.into(), anterior: anterior.into() }
    }

    /// La lista, sin caja: el año y cuántas arriba, la cabecera en minúscula, las fechas cortas (con el año solo si
    /// no es el de la declaración), si corrige otra o la corrigieron y, abajo, la elegida entera (con la hora).
    #[test]
    fn la_lista_sin_caja() {
        let mut a = app();
        let renta = |fecha: &str, n: u32, anterior: &str| decl("100", 2025, "0A", fecha, n, anterior);
        let modelos = [
            ("100".to_string(), Modelo { fecha: "2026-06-01T10:00:00".into(), decl: vec![renta("2026-05-02T20:30:00", 1, ""), renta("2026-05-20T08:30:00", 2, "1000000000001")] }),
            ("303".to_string(), Modelo { fecha: "2026-06-01T10:00:00".into(), decl: vec![decl("303", 2025, "3T", "2025-10-15T10:00:00", 3, "")] }),
        ];
        a.ctx.datos.hacienda.insert(2025, Consulta { fecha: "2026-06-01T10:00:00".into(), modelos: modelos.into_iter().collect(), ..Default::default() });
        a.ctx.anio = 2025;
        ir(&mut a, "Declaraciones");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("2025 · 3 declaraciones · consultado con Hacienda el 1 jun 2026 a las 10:00"), "{p}");
        assert!(p.contains("modelo  nombre") && p.contains("presentada") && p.contains("estado"), "{p}");
        assert!(p.contains("2 may 2026") && p.contains("15 oct ") && !p.contains("15 oct 2025"), "fechas cortas:\n{p}");
        assert!(p.contains("corregida después") && p.contains("corrige la 1000000000001"), "{p}");
        assert!(p.contains("100 Renta · Anual 2025 · presentada el 2 may 2026 a las 20:30 · hay otra posterior"), "la elegida, entera:\n{p}");
        assert!(!p.contains('┌') && !p.contains('│'), "sin caja:\n{p}");
        // A 80×24 también caben las columnas que importan.
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("Renta") && p.contains("estado") && p.contains("corrige"), "{p}");
        // Y con «t», todos los años.
        teclas(&mut a, "t");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("todos los años · consultados con Hacienda: 2025–2025 · 3 declaraciones") && p.contains("año   modelo"), "{p}");
    }
}
