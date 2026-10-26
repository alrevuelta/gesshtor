//! Hacienda: lo que tienes presentado según la última consulta (con tu certificado), sus PDF y
//! si la app ha podido leerlos; al lado, tus recibos de autónomo de la Seguridad Social y tus declaraciones
//! censales. Con Enter se ve la ficha de una declaración, con las casillas de su PDF. Aquí solo se mira.
//! Solo desde `PRIMER_ANIO`.

use crate::dominio::Datos;
use crate::dominio::fmt;
use crate::hacienda::{self, consulta, Decl, Modelo};
use crate::dominio::PRIMER_ANIO;
use detalle::{Detalle, Memo};
use crate::tui::texto::{cuando, hora};
use crate::tui::widgets::{bloque, coincide, tema, Entrada};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
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
    match l.fallos.get(&hacienda::clave(d.ejercicio, q, m)) {
        Some(motivo) => Lectura::Fallo(motivo.clone()),
        None if hay => Lectura::Leida,
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

    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let titulo = if self.todos { "Declaraciones presentadas · todos los años".to_string() } else { format!("Declaraciones presentadas en {}", ctx.anio) };
        let b = bloque(&titulo);
        let dentro = b.inner(area);
        f.render_widget(b, area);

        // Arriba: cuándo se consultó y si algo falló.
        let mut arriba: Vec<Line> = Vec::new();
        let n = self.filas.len();
        let vis = self.visibles();
        let cuantas = match n {
            0 => String::new(),
            n if vis.len() != n => format!(" · {} de {n}", vis.len()),
            1 => " · 1 declaración".into(),
            n => format!(" · {n} declaraciones"),
        };
        let c = consulta(&ctx.datos, ctx.anio);
        if self.todos {
            let anios = anios_consultados(&ctx.datos);
            if let (Some(a), Some(z)) = (anios.last(), anios.first()) {
                arriba.push(Line::styled(format!("Años consultados con Hacienda: {a}–{z}{cuantas}"), tema::apagado()));
            }
        } else {
            match &c {
                Consulta::Hecha(fe) => arriba.push(Line::styled(format!("Consultado con Hacienda {}{cuantas}", cuando(fe)), tema::apagado())),
                Consulta::Parcial(fe, ms) => arriba.push(Line::styled(
                    format!("Consulta incompleta: solo {} ({}){cuantas}", ms.join(", "), if fe.is_empty() { "sin fecha".into() } else { cuando(fe) }),
                    Style::new().fg(tema::AVISO),
                )),
                Consulta::Nunca => {}
            }
            for (m, e) in errores(&ctx.datos, ctx.anio) {
                arriba.push(Line::styled(format!("No se pudo consultar el {m}: {e}"), Style::new().fg(tema::AVISO)));
            }
        }
        self.sel = self.sel.min(vis.len().saturating_sub(1));
        let hay_filtro = self.buscando || !self.busca.valor.is_empty();
        // Abajo, la elegida: 3 líneas, y una más con el motivo si no se ha podido leer.
        let fallo = self.elegida().is_some_and(|x| matches!(x.lectura, Lectura::Fallo(..)));
        let alto_detalle = if vis.is_empty() { 0 } else { 3 + u16::from(fallo) };
        let [a_arriba, a_tabla, a_detalle, a_busca] = Layout::vertical([
            Constraint::Length(arriba.len().min(3) as u16),
            Constraint::Min(1),
            Constraint::Length(alto_detalle),
            Constraint::Length(if hay_filtro { 1 } else { 0 }),
        ])
        .areas(dentro);
        f.render_widget(Paragraph::new(arriba), a_arriba);

        if hay_filtro {
            let [etq, txt] = Layout::horizontal([Constraint::Length(8), Constraint::Min(1)]).areas(a_busca);
            f.render_widget(Paragraph::new(Span::styled("Buscar: ", Style::new().fg(tema::ACENTO))), etq);
            self.busca.dibujar(f, txt, self.buscando, Style::new());
        }

        if vis.is_empty() {
            let t = self.vacia(ctx, &c);
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), a_tabla);
            return;
        }

        // Columnas según el ancho: primero se quita el justificante (está abajo), luego se acorta el nombre y luego la hora.
        let w = a_tabla.width as usize;
        let largo = vis.iter().map(|&i| nombre_modelo(&self.filas[i].d.modelo, false).chars().count()).max().unwrap_or(0).max(6);
        let corto = vis.iter().map(|&i| nombre_modelo(&self.filas[i].d.modelo, true).chars().count()).max().unwrap_or(0).max(6);
        let anio_w = if self.todos { 5 } else { 0 };
        // fijo = símbolo de elegida (2) + año + modelo + periodo + fecha + justificante + PDF + espacios; la nota, al menos 12.
        let fijo = |nombre: usize, fecha: usize, just: bool| 2 + anio_w + 5 + nombre + 1 + 8 + fecha + 1 + if just { 14 } else { 0 } + 4 + 12;
        let (nombre_w, fecha_w, con_just) = primera_que_cabe(
            w,
            &[
                (fijo(largo, 17, true), (largo, 17, true)),
                (fijo(largo, 17, false), (largo, 17, false)),
                (fijo(corto, 17, false), (corto, 17, false)),
                (fijo(corto, 11, false), (corto, 11, false)),
                (0, (corto.min(8), 10, false)),
            ],
        );
        let nombre_corto = nombre_w < largo;
        let nota_w = w.saturating_sub(fijo(nombre_w, fecha_w, con_just) - 12);

        let mut cab = Vec::new();
        if self.todos {
            cab.push(Cell::from("Año"));
        }
        cab.extend([Cell::from("Mod."), Cell::from("Nombre"), Cell::from("Periodo"), Cell::from("Presentada")]);
        if con_just {
            cab.push(Cell::from("Justificante"));
        }
        cab.extend([Cell::from("PDF"), Cell::from("")]);

        let filas: Vec<Row> = vis
            .iter()
            .map(|&i| {
                let x = &self.filas[i];
                let d = &x.d;
                let mut c: Vec<Cell> = Vec::new();
                if self.todos {
                    c.push(Cell::from(d.ejercicio.to_string()));
                }
                c.push(Cell::from(Span::styled(codigo(&d.modelo).to_string(), Style::new().add_modifier(Modifier::BOLD))));
                c.push(Cell::from(Span::styled(nombre_modelo(&d.modelo, nombre_corto), tema::apagado())));
                c.push(Cell::from(nombre_periodo(d)));
                let dia = d.fecha.get(..10).unwrap_or(&d.fecha);
                let fecha = match (fecha_w >= 17, hora(&d.fecha)) {
                    (true, Some(h)) => format!("{} {h}", fmt::fecha_corta(dia)),
                    _ if fecha_w >= 11 => fmt::fecha_corta(dia),
                    _ => fmt::dmy(dia),
                };
                c.push(Cell::from(fecha));
                if con_just {
                    c.push(Cell::from(Span::styled(d.justificante.clone(), tema::apagado())));
                }
                // El PDF (● suyo, ○ otro del periodo, — ninguno) y si la app lo ha leído (✓ sí, ✗ no se puede, … aún no).
                let mut pdf = vec![match (&x.pdf, x.exacto) {
                    (Some(_), true) => Span::styled(" ●", Style::new().fg(tema::BIEN)),
                    (Some(_), false) => Span::styled(" ○", Style::new().fg(tema::AVISO)),
                    (None, _) => Span::styled(" —", tema::apagado()),
                }];
                pdf.push(match x.lectura {
                    Lectura::Leida => Span::styled("✓", Style::new().fg(tema::BIEN)),
                    Lectura::Fallo(..) => Span::styled("✗", Style::new().fg(tema::MAL)),
                    Lectura::Pendiente(_) => Span::styled("…", tema::apagado()),
                    Lectura::NoLee | Lectura::SinPdf => Span::raw(""),
                });
                c.push(Cell::from(Line::from(pdf)));
                let nota = if !d.anterior.is_empty() {
                    let t = [format!("corrige la {}", d.anterior), "corrige la anterior".into(), "corrige otra".into()]
                        .into_iter()
                        .find(|t| t.chars().count() <= nota_w)
                        .unwrap_or_else(|| "corrige".into());
                    Span::styled(t, Style::new().fg(tema::ACENTO))
                } else if x.corregida {
                    Span::styled(if nota_w >= 17 { "corregida después" } else { "corregida" }, tema::apagado())
                } else if matches!(x.lectura, Lectura::Fallo(..)) {
                    Span::styled("no se lee", Style::new().fg(tema::MAL))
                } else {
                    Span::raw("")
                };
                c.push(Cell::from(nota));
                Row::new(c)
            })
            .collect();

        let mut anchos = Vec::new();
        if self.todos {
            anchos.push(Constraint::Length(4));
        }
        anchos.extend([Constraint::Length(4), Constraint::Length(nombre_w as u16), Constraint::Length(7), Constraint::Length(fecha_w as u16)]);
        if con_just {
            anchos.push(Constraint::Length(13));
        }
        anchos.extend([Constraint::Length(3), Constraint::Min(0)]);

        let tabla = Table::new(filas, anchos)
            .header(Row::new(cab).style(tema::cabecera_tabla()))
            .column_spacing(1)
            .row_highlight_style(tema::seleccion())
            .highlight_symbol("› ");
        self.estado.select(Some(self.sel));
        f.render_stateful_widget(tabla, a_tabla, &mut self.estado);

        // Abajo: la elegida, entera.
        if let Some(x) = self.elegida() {
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
            let mut l2 = vec![Span::styled(format!("Justificante {}", d.justificante), tema::apagado())];
            if !d.csv.is_empty() {
                l2.push(Span::styled(format!(" · CSV {}", d.csv), tema::apagado()));
            }
            if !d.anterior.is_empty() {
                l2.push(Span::styled(format!(" · complementaria o sustitutiva de la {}", d.anterior), Style::new().fg(tema::ACENTO)));
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
            f.render_widget(Paragraph::new(v), a_detalle);
        }
    }

    /// Qué decir cuando no hay nada que enseñar.
    fn vacia(&self, ctx: &Ctx, c: &Consulta) -> Vec<Line<'static>> {
        let como = "Se consulta con tu certificado: abre la app con --certificado (o pulsa S si ya lo has hecho).";
        if !self.busca.valor.is_empty() && !self.filas.is_empty() {
            return vec![Line::styled(format!("Nada con «{}».", self.busca.valor), tema::apagado())];
        }
        if self.todos {
            return vec![Line::from("No se ha consultado ningún año con Hacienda.".to_string()).bold(), Line::from(""), Line::styled(como, tema::apagado())];
        }
        match c {
            Consulta::Nunca => vec![
                Line::from(format!("No se ha consultado {} con Hacienda.", ctx.anio)).bold(),
                Line::from(""),
                Line::styled(como, tema::apagado()),
                Line::from(""),
                Line::styled("Con «a» ves lo de todos los años consultados.", tema::apagado()),
            ],
            _ => {
                let ms = modelos_consultados(&ctx.datos, ctx.anio);
                vec![
                    Line::from(format!("Hacienda no tiene ninguna declaración tuya de {}.", ctx.anio)).bold(),
                    Line::from(""),
                    Line::styled(format!("Modelos consultados: {}.", if ms.is_empty() { "ninguno".into() } else { ms.join(", ") }), tema::apagado()),
                ]
            }
        }
    }
}

/* ---------------- Seguridad Social y censales ---------------- */


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
            KeyCode::Char('a') => {
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
        v.push(("a", if self.todos { "solo el año" } else { "todos los años" }));
        if !self.busca.valor.is_empty() {
            v.push(("Esc", "quitar filtro"));
        }
        v
    }

    fn capturando(&self) -> bool {
        self.buscando
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
