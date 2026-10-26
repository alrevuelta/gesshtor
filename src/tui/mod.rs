//! La app: secciones (Resumen, Empleo, Autónomo) con sus pestañas, el año, la barra de abajo y el bucle de
//! teclas. Cada pantalla es un fichero de esta carpeta (y sus partes, `anios_*.rs`, `empleo_*.rs`…); el menú, en
//! `menu.rs`.
pub mod anios;
mod anios_balance;
mod anios_grafico;
mod anios_lista;
mod anios_renta;
mod anios_reparto;
pub mod clientes;
pub mod cuotas;
mod cuotas_textos;
pub mod declaraciones;
mod declaraciones_detalle;
pub mod empleo;
mod empleo_mes;
mod empleo_nominas;
mod empleo_sueldo;
pub mod facturas;
mod facturas_editor;
pub mod gastos;
mod gastos_formulario;
pub mod impuestos;
mod impuestos_anio;
mod impuestos_casillas;
pub mod impuestos_trimestre;
pub mod inicio;
pub mod menu;
#[cfg(test)]
pub mod prueba;
pub mod texto;
pub mod widgets;

use crate::dominio::Datos;
use crate::dominio::{almacen, fmt};
use crate::hacienda::lectura;
use crate::informe;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use widgets::{tema, Eleccion, Selector};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nivel {
    Bien,
    Mal,
    Info,
}

/// Lo que comparten las pantallas: los datos, la carpeta, el año elegido y el aviso de la barra de abajo.
pub struct Ctx {
    pub datos: Datos,
    pub dir: PathBuf,
    pub anio: i32,
    pub aviso: Option<(String, Nivel, Instant)>,
    /// Sube con cada cambio de los datos (al guardar o al terminar de leer los PDF): para que las pantallas
    /// sepan cuándo recalcular lo que tengan guardado.
    pub generacion: u64,
    /// Mientras se leen los PDF de Hacienda en segundo plano: (hechos, total).
    pub leyendo: Option<(usize, usize)>,
    /// Una pantalla pide ir a otra (por su título); la app va al terminar de atender la tecla.
    pub ir_a: Option<&'static str>,
    /// Una pantalla pide elegir uno de los PDF de un año (no tiene uno concreto que abrir).
    pub pdfs_de: Option<i32>,
    /// El certificado digital, si se arrancó con `--certificado` (para traer lo último de Hacienda y la
    /// Seguridad Social).
    pub certificado: Option<Arc<crate::red::certificado::Certificado>>,
    /// Mientras se trae lo último: por dónde va.
    pub sincronizando: Option<String>,
    /// La última vez que se trajo en esta sesión: (cuándo, resumen, errores).
    pub sincronizado: Option<(String, String, Vec<String>)>,
    copia_hecha: bool,
    /// En las pruebas no se escribe en disco.
    pub sin_disco: bool,
    _candado: Option<almacen::Candado>,
}

impl Ctx {
    pub fn new(datos: Datos, dir: PathBuf) -> Self {
        Ctx { datos, dir, anio: fmt::anio_actual(), aviso: None, generacion: 0, leyendo: None, ir_a: None, pdfs_de: None, certificado: None, sincronizando: None, sincronizado: None, copia_hecha: false, sin_disco: false, _candado: None }
    }

    /// Abre la carpeta de una persona: sus datos y el candado (una sola ventana por carpeta).
    pub fn abrir(dir: &Path) -> std::io::Result<Self> {
        let candado = almacen::bloquear(dir)?;
        let datos = almacen::cargar(dir)?;
        crate::hacienda::lectura::renombrar_antiguos(dir);
        let mut c = Ctx::new(datos, dir.to_path_buf());
        c._candado = Some(candado);
        Ok(c)
    }

    /// ¿Se pueden apuntar cosas en el año elegido? (el actual, y el anterior mientras no tenga Renta)
    pub fn editable(&self) -> bool {
        informe::editable(&self.datos, self.anio)
    }

    /// Guarda `datos.json`. Llámalo después de cada cambio.
    pub fn guardar(&mut self) -> bool {
        self.generacion += 1;
        if self.sin_disco {
            return true;
        }
        let mut primera = !self.copia_hecha;
        let r = almacen::guardar(&self.dir, &self.datos, &mut primera);
        self.copia_hecha = !primera;
        match r {
            Ok(()) => true,
            Err(e) => {
                self.error(format!("No se ha podido guardar: {e}"));
                false
            }
        }
    }

    pub fn ok(&mut self, s: impl Into<String>) {
        self.aviso = Some((s.into(), Nivel::Bien, Instant::now()));
    }
    pub fn error(&mut self, s: impl Into<String>) {
        self.aviso = Some((s.into(), Nivel::Mal, Instant::now()));
    }
    pub fn info(&mut self, s: impl Into<String>) {
        self.aviso = Some((s.into(), Nivel::Info, Instant::now()));
    }

    /// Abre un documento adjunto (documentos/, justificantes/) con la app del sistema.
    pub fn abrir_doc(&mut self, nombre: &str) {
        match almacen::documento(&self.dir, nombre) {
            Some(p) => self.abrir_ruta(&p),
            None if nombre.is_empty() => self.error("No tiene documento adjunto."),
            None => self.error(format!("No encuentro {nombre} en la carpeta de datos.")),
        }
    }

    /// Deja elegir uno de los PDF de Hacienda de ese año (Renta, 303, 130, 036…) y lo abre. Si solo hay uno, lo
    /// abre directamente.
    pub fn elegir_pdf(&mut self, anio: i32) {
        self.pdfs_de = Some(anio);
    }

    pub fn abrir_ruta(&mut self, p: &Path) {
        if self.sin_disco {
            self.ok(format!("(prueba) abriría {}", p.display()));
            return;
        }
        match almacen::abrir(p) {
            Ok(()) => self.ok(format!("Abierto {}", p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
            Err(e) => self.error(format!("No se ha podido abrir: {e}")),
        }
    }
}

/// Una pantalla (pestaña de una sección).
pub trait Pantalla {
    fn titulo(&self) -> &'static str;
    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);
    /// Devuelve true si ha usado la tecla; si no, la usa la app (cambiar de sección o pestaña, de año, salir…).
    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool;
    /// Atajos para la barra de abajo: (tecla, qué hace).
    fn atajos(&self) -> Vec<(&'static str, &'static str)>;
    /// true mientras se escribe en un formulario o hay una ventana abierta: la app no usa ninguna tecla
    /// (ni «q», ni los números de las secciones), salvo Ctrl+C.
    fn capturando(&self) -> bool {
        false
    }
    /// Al entrar en la pestaña, cambiar de año o terminar de leer los PDF.
    fn al_entrar(&mut self, _ctx: &mut Ctx) {}
    /// Lo que se ve depende del año elegido. Si no (Inicio, Clientes), no se enseña el año ni se cambia con - +.
    fn usa_anio(&self) -> bool {
        true
    }
}

/// Desde este ancho, el menú va a la izquierda; más estrecho, arriba.
const ANCHO_MENU: u16 = 100;
/// Ancho del menú de la izquierda.
const MENU: u16 = 20;
/// Desde esta altura, el año de arriba va en números grandes (tres líneas, y una de margen); más bajo, en una.
const ALTO_ANIO_GRANDE: u16 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Resumen,
    Empleo,
    Autonomo,
}

/// Una sección del menú de arriba, con sus pestañas.
pub struct Seccion {
    pub area: Area,
    pub nombre: &'static str,
    pub pantallas: Vec<Box<dyn Pantalla>>,
    pub activa: usize,
}

enum Mensaje {
    Progreso(usize, usize),
    Hecho(crate::dominio::Lecturas),
}

enum MensajeRed {
    Paso(String),
    Hecho(Box<crate::sincronizar::Resultado>),
}

pub struct App {
    pub ctx: Ctx,
    pub secciones: Vec<Seccion>,
    /// Índice en `secciones`.
    pub activa: usize,
    pub salir: bool,
    pub ayuda: bool,
    /// Las personas de la carpeta (si hay más de una se puede cambiar con «p»).
    pub perfiles: Vec<PathBuf>,
    elegir: Option<Selector>,
    /// La lista de PDF de un año para abrir uno (con «o»).
    elegir_pdf: Option<Selector>,
    lectura: Option<Receiver<Mensaje>>,
    red: Option<Receiver<MensajeRed>>,
}

impl App {
    pub fn new(ctx: Ctx) -> Self {
        let mut app = App { ctx, secciones: menu::secciones(), activa: 0, salir: false, ayuda: false, perfiles: Vec::new(), elegir: None, elegir_pdf: None, lectura: None, red: None };
        app.entrar();
        app
    }

    /// Las secciones que tocan a esta persona (Empleo si tiene o tuvo nómina, Autónomo si lo es o lo fue).
    pub fn visibles(&self) -> Vec<usize> {
        let s = informe::situacion(&self.ctx.datos);
        (0..self.secciones.len())
            .filter(|&i| match self.secciones[i].area {
                Area::Resumen => true,
                Area::Empleo => s.empleo,
                Area::Autonomo => s.autonomo,
            })
            .collect()
    }

    pub fn pantalla(&self) -> &dyn Pantalla {
        let s = &self.secciones[self.activa];
        s.pantallas[s.activa].as_ref()
    }

    pub fn titulo_activa(&self) -> &'static str {
        self.pantalla().titulo()
    }

    fn entrar(&mut self) {
        let s = &mut self.secciones[self.activa];
        s.pantallas[s.activa].al_entrar(&mut self.ctx);
    }

    /// Va a la pantalla con ese título (en cualquier sección). Devuelve false si no existe.
    pub fn ir_a(&mut self, titulo: &str) -> bool {
        for (i, s) in self.secciones.iter_mut().enumerate() {
            if let Some(j) = s.pantallas.iter_mut().position(|p| p.titulo() == titulo) {
                s.activa = j;
                self.activa = i;
                self.entrar();
                return true;
            }
        }
        false
    }

    /// Las entradas del menú que se ven, en orden: (sección, pestaña).
    fn entradas(&self) -> Vec<(usize, usize)> {
        self.visibles().into_iter().flat_map(|i| (0..self.secciones[i].pantallas.len()).map(move |j| (i, j))).collect()
    }

    /// Va a la entrada `n` del menú (0, la primera).
    fn ir_entrada(&mut self, n: usize) {
        if let Some(&(i, j)) = self.entradas().get(n) {
            self.activa = i;
            self.secciones[i].activa = j;
            self.entrar();
        }
    }

    /// La entrada siguiente o anterior del menú (pasando de una sección a otra).
    fn mover_pestana(&mut self, d: isize) {
        let todas = self.entradas();
        let ahora = todas.iter().position(|&(i, j)| i == self.activa && j == self.secciones[i].activa).unwrap_or(0);
        let (i, j) = todas[((ahora as isize + d).rem_euclid(todas.len() as isize)) as usize];
        self.activa = i;
        self.secciones[i].activa = j;
        self.entrar();
    }

    fn cambiar_anio(&mut self, d: i32) {
        self.ctx.anio = (self.ctx.anio + d).clamp(crate::dominio::PRIMER_ANIO, fmt::anio_actual());
        self.entrar();
    }

    /// Lee en otro hilo los PDF de Hacienda que no estén ya en la caché; la app sigue respondiendo mientras.
    pub fn leer_en_segundo_plano(&mut self) {
        let (tx, rx) = mpsc::channel();
        let dir = self.ctx.dir.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let l = lectura::leer_todo(&dir, &mut |h, t| {
                let _ = tx2.send(Mensaje::Progreso(h, t));
            });
            let _ = tx.send(Mensaje::Hecho(l));
        });
        self.lectura = Some(rx);
    }

    /// Trae en otro hilo lo último de Hacienda y la Seguridad Social con el certificado (solo consulta). Con
    /// `todo`, también los años ya consultados.
    pub fn sincronizar(&mut self, todo: bool) {
        let Some(cert) = self.ctx.certificado.clone() else {
            self.ctx.error("Para traer lo último hace falta tu certificado: arranca con --certificado <ruta del .p12>.");
            return;
        };
        if self.red.is_some() {
            self.ctx.info("Ya se está trayendo lo último…");
            return;
        }
        let (tx, rx) = mpsc::channel();
        let dir = self.ctx.dir.clone();
        let datos = self.ctx.datos.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let r = crate::sincronizar::sincronizar(&dir, &datos, &cert, todo, &mut |p| {
                let _ = tx2.send(MensajeRed::Paso(p));
            });
            let _ = tx.send(MensajeRed::Hecho(Box::new(r)));
        });
        self.ctx.sincronizando = Some("Conectando…".into());
        self.red = Some(rx);
    }

    /// Lo traído de Hacienda y la Seguridad Social, a los datos (y se guarda).
    fn atender_red(&mut self) -> bool {
        let Some(rx) = &self.red else { return false };
        let mut fin = None;
        let mut cambio = false;
        while let Ok(m) = rx.try_recv() {
            match m {
                MensajeRed::Paso(p) => self.ctx.sincronizando = Some(p),
                MensajeRed::Hecho(r) => fin = Some(r),
            }
            cambio = true;
        }
        if let Some(r) = fin {
            self.red = None;
            self.ctx.sincronizando = None;
            crate::sincronizar::aplicar(&mut self.ctx.datos, &r);
            self.ctx.guardar();
            let resumen = r.resumen();
            if r.errores.is_empty() {
                self.ctx.ok(format!("Traído de Hacienda y la Seguridad Social: {resumen}."));
            } else {
                self.ctx.error(format!("{} (y {} cosa(s) no se han podido traer: mira Inicio).", r.errores[0], r.errores.len()));
            }
            self.ctx.sincronizado = Some((r.cuando.clone(), resumen, r.errores.clone()));
            // Los PDF nuevos se leen; y lo demás ya está en los datos.
            self.leer_en_segundo_plano();
            self.entrar();
        }
        cambio
    }

    /// Recoge lo que haya terminado en segundo plano. Devuelve true si ha cambiado algo.
    pub fn atender(&mut self) -> bool {
        let red = self.atender_red();
        let Some(rx) = &self.lectura else { return red };
        let mut cambio = false;
        let mut fin = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Mensaje::Progreso(h, t) => self.ctx.leyendo = Some((h, t)),
                Mensaje::Hecho(l) => fin = Some(l),
            }
            cambio = true;
        }
        if let Some(l) = fin {
            let leidos = l.rentas.len() + l.formularios.len();
            self.ctx.datos.lecturas = l;
            self.ctx.leyendo = None;
            self.ctx.generacion += 1;
            self.lectura = None;
            if !self.visibles().contains(&self.activa) {
                self.activa = 0;
            }
            if leidos > 0 && !self.ctx.datos.lecturas.fallos.is_empty() {
                let n = self.ctx.datos.lecturas.fallos.len();
                self.ctx.info(format!("{n} PDF no se han podido leer (se ven en Declaraciones)."));
            }
            self.entrar();
        }
        cambio
    }

    /// Cambia a la carpeta de otra persona.
    fn cambiar_perfil(&mut self, dir: &Path) {
        if dir == self.ctx.dir {
            return;
        }
        match Ctx::abrir(dir) {
            Ok(mut ctx) => {
                ctx.anio = self.ctx.anio;
                let perfiles = std::mem::take(&mut self.perfiles);
                *self = App::new(ctx);
                self.perfiles = perfiles;
                self.leer_en_segundo_plano();
                let quien = nombre_de(&self.ctx.datos);
                self.ctx.ok(format!("Ahora ves los datos de {quien}."));
            }
            Err(e) => self.ctx.error(format!("No se puede abrir {}: {e}", dir.display())),
        }
    }

    pub fn tecla(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.salir = true;
            return;
        }
        if self.ayuda {
            self.ayuda = false;
            return;
        }
        if let Some(sel) = &mut self.elegir_pdf {
            match sel.tecla(k) {
                Eleccion::Elegido(r) => {
                    self.elegir_pdf = None;
                    self.ctx.abrir_ruta(&PathBuf::from(r));
                }
                Eleccion::Cancelado => self.elegir_pdf = None,
                Eleccion::Sigue => {}
            }
            return;
        }
        if let Some(sel) = &mut self.elegir {
            match sel.tecla(k) {
                Eleccion::Elegido(r) => {
                    self.elegir = None;
                    self.cambiar_perfil(&PathBuf::from(r));
                }
                Eleccion::Cancelado => self.elegir = None,
                Eleccion::Sigue => {}
            }
            return;
        }
        let capt = self.pantalla().capturando();
        let con_anio = self.pantalla().usa_anio();
        let s = &mut self.secciones[self.activa];
        let usada = s.pantallas[s.activa].tecla(k, &mut self.ctx);
        if let Some(t) = self.ctx.ir_a.take() {
            self.ir_a(t);
            return;
        }
        if let Some(y) = self.ctx.pdfs_de.take() {
            self.lista_pdfs(y);
            return;
        }
        if usada || capt {
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.salir = true,
            KeyCode::Char('?') => self.ayuda = true,
            KeyCode::Char('S') => self.sincronizar(false),
            // Las pantallas sin un PDF propio: los del año.
            KeyCode::Char('o') if con_anio => self.lista_pdfs(self.ctx.anio),
            // Cada entrada del menú, con el número que lleva al lado (1–9 y 0, la décima).
            KeyCode::Char(c @ '1'..='9') => self.ir_entrada(c as usize - '1' as usize),
            KeyCode::Char('0') => self.ir_entrada(9),
            KeyCode::Tab => self.mover_pestana(1),
            KeyCode::BackTab => self.mover_pestana(-1),
            KeyCode::Char('-' | '_' | '[') if con_anio => self.cambiar_anio(-1),
            KeyCode::Char('+' | '=' | ']') if con_anio => self.cambiar_anio(1),
            KeyCode::Char('p') if self.perfiles.len() > 1 => {
                let v = self.perfiles.iter().map(|p| (p.to_string_lossy().to_string(), p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), String::new())).collect();
                self.elegir = Some(Selector::new("Ver los datos de…", v).con_elegido(&self.ctx.dir.to_string_lossy()));
            }
            _ => {}
        }
    }

    pub fn dibujar(&mut self, f: &mut Frame) {
        let [arriba, pie] = Layout::vertical([Constraint::Min(5), Constraint::Length(2)]).areas(f.area());
        let cuerpo = if f.area().width >= ANCHO_MENU {
            let [menu, cuerpo] = Layout::horizontal([Constraint::Length(MENU), Constraint::Min(10)]).areas(arriba);
            self.menu(f, menu);
            cuerpo
        } else {
            let [cab, cuerpo] = Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(arriba);
            self.cabecera(f, cab);
            cuerpo
        };
        // Arriba, en el centro, el año que se ve (si la pantalla depende de él).
        let cuerpo = if self.pantalla().usa_anio() {
            let alto = if f.area().height >= ALTO_ANIO_GRANDE { 4 } else { 1 };
            let [barra, resto] = Layout::vertical([Constraint::Length(alto), Constraint::Min(0)]).areas(cuerpo);
            self.barra_anio(f, barra);
            resto
        } else {
            cuerpo
        };
        let s = &mut self.secciones[self.activa];
        s.pantallas[s.activa].dibujar(f, cuerpo, &self.ctx);
        self.pie(f, pie);
        if self.ayuda {
            self.dibujar_ayuda(f);
        }
        if let Some(sel) = &mut self.elegir {
            sel.dibujar(f, f.area());
        }
        if let Some(sel) = &mut self.elegir_pdf {
            sel.dibujar(f, f.area());
        }
    }

    /// Los PDF de Hacienda de un año: si hay uno, lo abre; si hay varios, deja elegir.
    fn lista_pdfs(&mut self, anio: i32) {
        let v: Vec<lectura::Justificante> = lectura::justificantes(&self.ctx.dir).into_iter().filter(|j| j.ejercicio == anio).collect();
        match v.as_slice() {
            [] => self.ctx.error(format!("No hay ningún PDF de {anio} en la carpeta (se descargan al traer lo último con tu certificado).")),
            [j] => self.ctx.abrir_ruta(&j.ruta),
            _ => {
                // Primero la Renta y los trimestrales (el último trimestre arriba); lo censal, al final.
                let orden = |m: &str| ["100", "714", "303", "130", "131", "390", "347", "349", "720", "721", "111", "115", "123", "180", "190", "193"].iter().position(|x| *x == m).unwrap_or(99);
                let mut v = v;
                v.sort_by(|a, b| (orden(&a.modelo), std::cmp::Reverse(&b.periodo)).cmp(&(orden(&b.modelo), std::cmp::Reverse(&a.periodo))));
                let items = v.iter().map(|j| (j.ruta.to_string_lossy().to_string(), nombre_pdf(j), j.nombre())).collect();
                self.elegir_pdf = Some(Selector::new(format!("Abrir un PDF de {anio}"), items));
            }
        }
    }

    /// El menú de la izquierda: quién y las secciones con sus entradas.
    fn menu(&self, f: &mut Frame, area: Rect) {
        let w = area.width.saturating_sub(1) as usize;
        let corta = |s: &str, n: usize| if s.chars().count() > n { s.chars().take(n.saturating_sub(1)).collect::<String>() + "…" } else { s.to_string() };
        let mut v = vec![
            Line::styled(format!(" {}", corta(&nombre_de(&self.ctx.datos), w - 1)), Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD)),
        ];
        let mut n = 0;
        for &i in &self.visibles() {
            let s = &self.secciones[i];
            v.push(Line::from(""));
            v.push(Line::styled(format!(" {}", s.nombre), Style::new().add_modifier(Modifier::BOLD)));
            for (j, p) in s.pantallas.iter().enumerate() {
                // Su número (la tecla que lleva allí) y el título.
                let t = corta(p.titulo(), w - 5);
                let activa = i == self.activa && j == s.activa;
                let st = if activa { tema::pestana_activa() } else { tema::apagado() };
                let marca = if activa { " ›" } else { "  " };
                v.push(Line::from(vec![Span::styled(marca, st), Span::styled(format!("{:>2} ", tecla_entrada(n)), if activa { st } else { Style::new().fg(tema::ACENTO) }), Span::styled(format!("{t:<ancho$}", ancho = w - 5), st)]));
                n += 1;
            }
        }
        let borde = ratatui::widgets::Block::new().borders(ratatui::widgets::Borders::RIGHT).border_style(tema::apagado());
        f.render_widget(Paragraph::new(v).block(borde), area);
    }

    /// El año, centrado; con sitio, en grande (tres líneas) y con una de margen encima.
    fn barra_anio(&self, f: &mut Frame, area: Rect) {
        let y = self.ctx.anio;
        let st = Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD);
        let lineas: Vec<Line> = if area.height < 4 {
            vec![Line::styled(y.to_string(), st)]
        } else {
            std::iter::once(Line::from("")).chain(en_grande(y).into_iter().map(|l| Line::styled(l, st))).collect()
        };
        f.render_widget(Paragraph::new(lineas).centered(), area);
    }

    fn cabecera(&self, f: &mut Frame, area: Rect) {
        let [l1, l2] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        // Línea 1: quién y las secciones, cada una con el número de su primera entrada.
        let entradas = self.entradas();
        let mut v = vec![Span::styled(format!(" {} ", nombre_de(&self.ctx.datos)), Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD)), Span::raw("  ")];
        for &i in &self.visibles() {
            let n = entradas.iter().position(|e| e.0 == i).unwrap_or(0);
            let t = format!(" {} {} ", tecla_entrada(n), self.secciones[i].nombre);
            v.push(if i == self.activa { Span::styled(t, tema::pestana_activa()) } else { Span::styled(t, tema::apagado()) });
            v.push(Span::raw(" "));
        }
        f.render_widget(Paragraph::new(Line::from(v)), l1);
        // Línea 2: las pestañas de la sección, con su número.
        let s = &self.secciones[self.activa];
        let primera = entradas.iter().position(|e| e.0 == self.activa).unwrap_or(0);
        let mut v = vec![Span::raw("  ")];
        for (i, p) in s.pantallas.iter().enumerate() {
            let t = p.titulo();
            v.push(Span::styled(format!("{} ", tecla_entrada(primera + i)), Style::new().fg(tema::ACENTO)));
            v.push(if i == s.activa { Span::styled(t, Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)) } else { Span::styled(t, tema::apagado()) });
            v.push(Span::styled("   ", tema::apagado()));
        }
        f.render_widget(Paragraph::new(Line::from(v)), l2);
    }

    fn pie(&self, f: &mut Frame, area: Rect) {
        let [l1, l2] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        if let Some(p) = &self.ctx.sincronizando {
            f.render_widget(Paragraph::new(Line::styled(format!(" {p}"), Style::new().fg(tema::ACENTO))), l1);
        } else if let Some((h, t)) = self.ctx.leyendo {
            f.render_widget(Paragraph::new(Line::styled(format!(" Leyendo tus PDF de Hacienda: {h} de {t}…"), Style::new().fg(tema::ACENTO))), l1);
        } else if let Some((txt, nivel, cuando)) = &self.ctx.aviso
            && cuando.elapsed() < Duration::from_secs(8) {
                let st = match nivel {
                    Nivel::Bien => Style::new().fg(tema::BIEN),
                    Nivel::Mal => Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD),
                    Nivel::Info => Style::new().fg(tema::ACENTO),
                };
                f.render_widget(Paragraph::new(Line::styled(format!(" {txt}"), st)), l1);
            }
        let p = self.pantalla();
        let mut atajos = p.atajos();
        if !p.capturando() {
            // Las de la app, si caben; si no, solo la ayuda y salir.
            let largo: usize = atajos.iter().map(|(k, d)| k.chars().count() + d.chars().count() + 4).sum();
            if largo + 52 <= area.width as usize {
                atajos.push(("Tab", "menú"));
                if p.usa_anio() {
                    atajos.push(("- +", "año"));
                }
            }
            atajos.extend([("?", "ayuda"), ("q", "salir")]);
        }
        f.render_widget(Paragraph::new(widgets::linea_atajos(&atajos)), l2);
    }

    fn dibujar_ayuda(&self, f: &mut Frame) {
        let area = widgets::centrado(f.area(), 78, 20);
        f.render_widget(Clear, area);
        let mut t = vec![
            Line::from("Teclas de la app".bold()),
            Line::from(""),
            Line::from("1 … 9, 0          ir a la entrada del menú con ese número"),
            Line::from("Tab, Mayús+Tab    entrada siguiente / anterior del menú"),
            Line::from("- y +             año anterior / siguiente"),
            Line::from("↑ ↓, PgUp PgDn    moverse por las listas"),
            Line::from("Enter             ver o editar lo elegido"),
            Line::from("n                 nuevo (nómina, factura, gasto, cliente…)"),
            Line::from("o                 abrir el PDF (el de lo que ves, o elegir uno del año)"),
            Line::from("/                 buscar"),
        ];
        if self.perfiles.len() > 1 {
            t.push(Line::from("p                 ver los datos de otra persona"));
        }
        if self.ctx.certificado.is_some() {
            t.push(Line::from("S                 traer lo último de Hacienda y la Seguridad Social"));
        }
        t.extend([
            Line::from(""),
            Line::from("Los años pasados se ven con lo que dicen tus Rentas y la Seguridad Social;"),
            Line::from("en el año en curso apuntas facturas y gastos, y nóminas en cualquiera."),
            Line::from("Los cambios se guardan solos. Ctrl+C sale en cualquier momento.".italic()),
        ]);
        // La versión y el commit, para saber de qué código sale lo que se ve.
        let version = Line::styled(format!(" gesshtor {} ", crate::VERSION), widgets::tema::apagado()).right_aligned();
        f.render_widget(Paragraph::new(t).block(widgets::bloque("Ayuda").title_bottom(version)).wrap(Wrap { trim: false }), area);
    }
}

/// Un año en números grandes, en tres líneas: cada cifra, de 3 × 5 puntos, con medios bloques (dos filas de puntos
/// por línea).
fn en_grande(y: i32) -> [String; 3] {
    const CIFRAS: [[&str; 5]; 10] = [
        ["###", "#.#", "#.#", "#.#", "###"],
        [".#.", "##.", ".#.", ".#.", "###"],
        ["###", "..#", "###", "#..", "###"],
        ["###", "..#", "###", "..#", "###"],
        ["#.#", "#.#", "###", "..#", "..#"],
        ["###", "#..", "###", "..#", "###"],
        ["###", "#..", "###", "#.#", "###"],
        ["###", "..#", "..#", "..#", "..#"],
        ["###", "#.#", "###", "#.#", "###"],
        ["###", "#.#", "###", "..#", "###"],
    ];
    let mut out: [String; 3] = Default::default();
    for (k, c) in y.to_string().chars().enumerate() {
        let p = CIFRAS[c.to_digit(10).unwrap_or(0) as usize];
        for (l, linea) in out.iter_mut().enumerate() {
            if k > 0 {
                linea.push(' ');
            }
            let (arriba, abajo) = (p[2 * l].as_bytes(), p.get(2 * l + 1).map(|s| s.as_bytes()));
            for i in 0..3 {
                linea.push(match (arriba[i] == b'#', abajo.is_some_and(|s| s[i] == b'#')) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                });
            }
        }
    }
    out
}

/// La tecla de la entrada `n` del menú (0, la primera): «1»…«9» y «0»; más allá, ninguna.
fn tecla_entrada(n: usize) -> String {
    match n {
        0..=8 => (n + 1).to_string(),
        9 => "0".into(),
        _ => String::new(),
    }
}

/// «Renta 2025», «IVA (303) · 3.er trimestre», «Alta y cambios (036)»…
fn nombre_pdf(j: &lectura::Justificante) -> String {
    let modelo = match j.modelo.as_str() {
        "100" => return format!("Renta {}", j.ejercicio),
        "714" => return format!("Patrimonio {}", j.ejercicio),
        "303" => "IVA (303)",
        "130" => "IRPF (130)",
        "131" => "IRPF por módulos (131)",
        "390" => "Resumen anual de IVA (390)",
        "347" => "Operaciones con terceros (347)",
        "349" => "Operaciones con la UE (349)",
        "720" => "Bienes en el extranjero (720)",
        "721" => "Criptomonedas en el extranjero (721)",
        "111" => "Retenciones (111)",
        "115" => "Retenciones de alquileres (115)",
        "123" => "Retenciones del capital (123)",
        "180" => "Resumen de alquileres (180)",
        "190" => "Resumen de retenciones (190)",
        "193" => "Resumen del capital (193)",
        "036" | "037" => "Alta y cambios en Hacienda (036)",
        "840" => "IAE (840)",
        m => return format!("Modelo {m} · {}", j.periodo),
    };
    match j.trimestre() {
        0 => modelo.to_string(),
        q => format!("{modelo} · {q}T"),
    }
}

/// «Ana García» (de su perfil), o el nombre de la carpeta.
pub fn nombre_de(d: &Datos) -> String {
    let p = &d.perfil;
    if !p.nombre_pila.is_empty() && !p.apellidos.is_empty() {
        return format!("{} {}", p.nombre_pila, p.apellidos);
    }
    if p.nombre.chars().count() > 2 {
        return p.nombre.clone();
    }
    "gesshtor".into()
}

pub fn ejecutar(mut terminal: DefaultTerminal, mut app: App) -> std::io::Result<()> {
    while !app.salir {
        app.atender();
        terminal.draw(|f| app.dibujar(f))?;
        // Se repinta cada poco para que el aviso de abajo se borre solo y avance la lectura en segundo plano.
        let espera = if app.ctx.leyendo.is_some() || app.ctx.sincronizando.is_some() { 100 } else { 500 };
        if event::poll(Duration::from_millis(espera))?
            && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press {
                    app.tecla(k);
                }
    }
    Ok(())
}
