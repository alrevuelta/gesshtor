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
pub mod utilidades;
pub mod widgets;

use crate::dominio::Datos;
use crate::dominio::{almacen, fmt};
use crate::hacienda::lectura;
use crate::informe;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
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
    /// Tu certificado digital (para traer lo último de Hacienda y la Seguridad Social). La app no se abre sin él;
    /// solo en las pruebas falta.
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
    /// Su tecla en el menú, si es una letra (las utilidades: «C»). Las demás llevan un número, por orden.
    fn tecla_menu(&self) -> Option<char> {
        None
    }
}

/// Desde este ancho, el menú va a la izquierda; más estrecho, arriba.
const ANCHO_MENU: u16 = 100;
/// Ancho del menú de la izquierda.
const MENU: u16 = 26;
/// «GESSHTOR» en letras de tres filas: cada una de 4 × 5 puntos y un punto de separación, con cuartos de bloque (dos
/// puntos por celda a lo ancho y a lo alto). Las columnas 0–4 son «GE»; la 5–11, «SSH»; y de la 12, «TOR».
const MARCA: [&str; 3] = ["▞▀▐▀▘▞▀▗▀▘▌▐▝█▘▞▚▐▀▖", "▌▜▐▀ ▝▚ ▀▖▛▜ █ ▌▐▐▜ ", "▝▀▝▀▘▀▘▝▀ ▘▝ ▀ ▝▘▝ ▘"];

/// Desde esta altura, la franja de arriba (con la marca y el año en grande) ocupa cinco líneas: una de margen, tres
/// y la línea de debajo; más bajo, una y la línea.
const ALTO_ANIO_GRANDE: u16 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Resumen,
    Empleo,
    Autonomo,
    Utilidades,
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
    /// La lista de PDF de un año para abrir uno (con «o»).
    elegir_pdf: Option<Selector>,
    lectura: Option<Receiver<Mensaje>>,
    red: Option<Receiver<MensajeRed>>,
}

impl App {
    pub fn new(ctx: Ctx) -> Self {
        let mut app = App { ctx, secciones: menu::secciones(), activa: 0, salir: false, ayuda: false, elegir_pdf: None, lectura: None, red: None };
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
                Area::Utilidades => true,
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

    /// La tecla de cada entrada del menú, en el orden de `entradas`: las de datos, 1–9 y 0 (más allá, ninguna; se
    /// llega con Tab); las utilidades, su letra (no cambia según las secciones que tengas).
    fn teclas(&self) -> Vec<String> {
        let mut n = 0;
        self.entradas()
            .into_iter()
            .map(|(i, j)| match self.secciones[i].pantallas[j].tecla_menu() {
                Some(c) => c.to_string(),
                None => {
                    n += 1;
                    match n {
                        1..=9 => n.to_string(),
                        10 => "0".into(),
                        _ => String::new(),
                    }
                }
            })
            .collect()
    }

    /// Va a la entrada del menú con esa tecla. Devuelve false si no hay ninguna.
    fn ir_tecla(&mut self, c: char) -> bool {
        let t = c.to_string();
        match self.teclas().iter().position(|x| *x == t) {
            Some(n) => {
                let (i, j) = self.entradas()[n];
                self.activa = i;
                self.secciones[i].activa = j;
                self.entrar();
                true
            }
            None => false,
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
            self.ctx.error("No hay ningún certificado abierto.");
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
            // Cada entrada del menú, con la tecla que lleva al lado (1–9 y 0; las utilidades, su letra).
            KeyCode::Char(c) if self.ir_tecla(c) => {}
            KeyCode::Tab => self.mover_pestana(1),
            KeyCode::BackTab => self.mover_pestana(-1),
            KeyCode::Char('-' | '_' | '[') if con_anio => self.cambiar_anio(-1),
            KeyCode::Char('+' | '=' | ']') if con_anio => self.cambiar_anio(1),
            _ => {}
        }
    }

    pub fn dibujar(&mut self, f: &mut Frame) {
        let [arriba, pie] = Layout::vertical([Constraint::Min(5), Constraint::Length(2)]).areas(f.area());
        // Arriba, de lado a lado, la franja: la marca, dónde estás, el año y quién eres, con una línea debajo (y una
        // fila de aire). Con la terminal estrecha, encima va el menú, en dos líneas.
        let (franja, aire) = if f.area().height >= ALTO_ANIO_GRANDE { (5, 1) } else { (2, 0) };
        let cuerpo = if f.area().width >= ANCHO_MENU {
            let [a_franja, _, abajo] = Layout::vertical([Constraint::Length(franja), Constraint::Length(aire), Constraint::Min(0)]).areas(arriba);
            self.franja(f, a_franja, true);
            let [menu, cuerpo] = Layout::horizontal([Constraint::Length(MENU), Constraint::Min(10)]).areas(abajo);
            self.menu(f, menu);
            cuerpo
        } else {
            // Con poca altura, sin la línea: cada fila cuenta.
            let franja = if franja < 5 { 1 } else { franja };
            let [cab, a_franja, _, cuerpo] = Layout::vertical([Constraint::Length(2), Constraint::Length(franja), Constraint::Length(aire), Constraint::Min(3)]).areas(arriba);
            self.cabecera(f, cab);
            self.franja(f, a_franja, false);
            cuerpo
        };
        let s = &mut self.secciones[self.activa];
        s.pantallas[s.activa].dibujar(f, cuerpo, &self.ctx);
        self.pie(f, pie);
        if self.ayuda {
            self.dibujar_ayuda(f);
        }
        if let Some(sel) = &mut self.elegir_pdf {
            sel.dibujar(f, f.area());
        }
        widgets::uniformar_seleccion(f.buffer_mut());
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

    /// Cómo va lo de traer lo último de Hacienda y la Seguridad Social: (glifo y su estilo, texto, hora, tecla S
    /// resaltada, qué hace la S). La hora: «22:10», «ayer 22:10» o «24 oct».
    fn estado(&self) -> ((&'static str, Style), (String, Style), String, bool, &'static str) {
        let hora = |c: &str| {
            let (dia, h) = (c.get(..10).unwrap_or_default(), c.get(11..16).unwrap_or_default());
            match fmt::dias_entre(dia, &fmt::hoy()) {
                0 => h.to_string(),
                1 => format!("ayer {h}"),
                _ => fmt::fecha_corta(dia).rsplit_once(' ').map_or(dia.to_string(), |(dm, _)| dm.to_string()),
            }
        };
        let normal = Style::new();
        match (&self.ctx.sincronizando, &self.ctx.sincronizado) {
            (Some(p), _) => (("●", Style::new().fg(tema::ACENTO)), ("Actualizando…".into(), Style::new().fg(tema::ACENTO)), p.clone(), false, ""),
            (None, Some((c, _, errores))) if !errores.is_empty() => (("●", Style::new().fg(tema::AVISO)), ("Con errores".into(), Style::new().fg(tema::AVISO)), hora(c), true, "Reintentar"),
            (None, Some((c, _, _))) => (("●", Style::new().fg(tema::BIEN)), ("Al día".into(), normal), hora(c), false, "Actualizar"),
            (None, None) => (("○", tema::apagado()), ("Sin actualizar".into(), normal), String::new(), true, "Actualizar"),
        }
    }

    /// El menú de la izquierda: cada sección con sus entradas, cada una con su tecla; y abajo, cómo va lo de traer lo
    /// último y la versión. La marca va encima, en la franja. Todo sobre una rejilla: la marca de la elegida en la
    /// columna 0, las teclas y la marca de arriba en la 2, los textos en la 5, y lo de la derecha acaba en la
    /// penúltima; la última queda en blanco, de calle hasta el contenido. Sin fondos: solo el color de acento.
    fn menu(&self, f: &mut Frame, area: Rect) {
        let gris = tema::apagado();
        let acento = Style::new().fg(tema::ACENTO);
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let w = area.width.saturating_sub(1) as usize;
        let corta = |s: &str, n: usize| if s.chars().count() > n { s.chars().take(n.saturating_sub(1)).collect::<String>() + "…" } else { s.to_string() };
        let mut v: Vec<Line> = Vec::new();
        let teclas = self.teclas();
        let mut n = 0;
        for (k, &i) in self.visibles().iter().enumerate() {
            let s = &self.secciones[i];
            // La primera, a la altura del contenido.
            if k > 0 {
                v.push(Line::from(""));
            }
            v.push(Line::styled(format!("  {}", s.nombre.to_uppercase()), gris));
            for (j, p) in s.pantallas.iter().enumerate() {
                let t = corta(p.titulo(), w - 5);
                let tecla = teclas.get(n).cloned().unwrap_or_default();
                n += 1;
                v.push(if i == self.activa && j == s.activa {
                    Line::from(vec![Span::styled("▌", acento), Span::styled(format!(" {tecla:<2} {t}"), negrita.fg(tema::ACENTO))])
                } else {
                    Line::from(vec![Span::styled(format!("  {tecla:<2} "), gris), Span::raw(t)])
                });
            }
        }
        let alto = v.len() as u16;
        f.render_widget(Paragraph::new(v), Rect { width: w as u16, ..area });
        // Abajo, si cabe: cómo va lo de traer lo último y la versión (y una fila de aire hasta la barra de abajo).
        if area.height < alto + 4 {
            return;
        }
        let ((glifo, st_glifo), (que, st_que), hora, resaltar, accion) = self.estado();
        let r1 = Rect { y: area.bottom() - 3, height: 1, width: w as u16, ..area };
        let r2 = Rect { y: r1.y + 1, ..r1 };
        let mut l1 = vec![Span::raw("  "), Span::styled(glifo, st_glifo), Span::raw("  "), Span::styled(que, st_que)];
        let version = format!("v{}", crate::VERSION.split(' ').next().unwrap_or_default());
        if self.ctx.sincronizando.is_some() {
            // Por dónde va, en la segunda línea.
            f.render_widget(Paragraph::new(Line::from(l1)), r1);
            f.render_widget(Paragraph::new(Line::styled(format!("     {}", corta(&hora, w.saturating_sub(5))), gris)), r2);
            return;
        }
        if !hora.is_empty() {
            l1.push(Span::styled(format!(" · {hora}"), gris));
        }
        f.render_widget(Paragraph::new(Line::from(l1)), r1);
        let (st_s, st_t) = if resaltar { (negrita.fg(tema::ACENTO), Style::new()) } else { (gris, gris) };
        f.render_widget(Paragraph::new(Line::from(vec![Span::raw("  "), Span::styled("S", st_s), Span::styled(format!("  {accion}"), st_t)])), r2);
        f.render_widget(Paragraph::new(Line::styled(version, gris).right_aligned()), r2);
    }

    /// La franja de arriba, de lado a lado, con todo en la misma fila (la del medio): encima del menú, la marca
    /// («GESSHTOR» en grande); encima del contenido y alineado con él, dónde estás («Autónomo ›
    /// Facturas»); en el centro del contenido, el año que ves, en grande (si la pantalla depende de él); y a la
    /// derecha, quién eres (tus iniciales y tu nombre). Debajo, una línea. Con poca altura, todo en una fila.
    fn franja(&self, f: &mut Frame, area: Rect, con_marca: bool) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let grande = area.height >= 5;
        let filas = if grande { Rect { y: area.y + 1, height: 3, ..area } } else { Rect { height: 1, ..area } };
        let fila = Rect { y: if grande { filas.y + 1 } else { filas.y }, height: 1, ..area };
        // En degradado, como el año: más claro arriba.
        let tonos = [Color::LightCyan, tema::ACENTO, Color::Blue];
        let izq = if con_marca { MENU.min(area.width) } else { 0 };
        if con_marca {
            // El logo es el nombre: «GESSHTOR» en letras de tres filas (de 4 × 5 puntos, con cuartos de bloque), con
            // «SSH» (gestor con ssh dentro) en el degradado del año.
            if grande {
                let logo: Vec<Line> = MARCA
                    .into_iter()
                    .zip(tonos)
                    .map(|(l, c)| {
                        let t: Vec<char> = l.chars().collect();
                        let trozo = |a: usize, b: usize| t[a..b].iter().collect::<String>();
                        Line::from(vec![Span::styled(trozo(0, 5), negrita), Span::styled(trozo(5, 12), negrita.fg(c)), Span::styled(trozo(12, t.len()), negrita)])
                    })
                    .collect();
                f.render_widget(Paragraph::new(logo), Rect { x: area.x + 2, width: izq.saturating_sub(2), ..filas });
            } else {
                let l = vec![Span::styled("  GE", negrita), Span::styled("SSH", negrita.fg(tema::ACENTO)), Span::styled("TOR", negrita)];
                f.render_widget(Paragraph::new(Line::from(l)), Rect { width: izq, ..fila });
            }
        }
        let zona = Rect { x: area.x + izq, width: area.width.saturating_sub(izq), ..area };
        // Dónde estás.
        let seccion = self.secciones[self.activa].nombre;
        let titulo = self.pantalla().titulo();
        let antes = if con_marca { "" } else { " " };
        let donde = Line::from(vec![Span::styled(format!("{antes}{seccion} › "), gris), Span::styled(titulo, negrita)]);
        f.render_widget(Paragraph::new(donde), Rect { y: fila.y, height: 1, ..zona });
        // Quién: las iniciales en un círculo del color de acento y el nombre, recortado para no pisar el año.
        let nombre = nombre_de(&self.ctx.datos);
        let ini = iniciales(&nombre);
        let sitio = (zona.width as usize).saturating_sub(17) / 2;
        let libre = sitio.saturating_sub(ini.chars().count() + 5);
        let mut quien = vec![Span::styled("▐", Style::new().fg(tema::ACENTO)), Span::styled(format!(" {ini} "), tema::pestana_activa()), Span::styled("▌", Style::new().fg(tema::ACENTO))];
        if libre >= 4 {
            let n: String = if nombre.chars().count() > libre { nombre.chars().take(libre - 1).chain(['…']).collect() } else { nombre.clone() };
            quien.push(Span::styled(format!(" {n}"), negrita));
        }
        quien.push(Span::raw(" "));
        f.render_widget(Paragraph::new(Line::from(quien).right_aligned()), Rect { y: fila.y, height: 1, ..zona });
        // El año, en el centro del contenido.
        if self.pantalla().usa_anio() {
            let st = negrita.fg(tema::ACENTO);
            if grande {
                let g: Vec<Line> = en_grande(self.ctx.anio).into_iter().zip(tonos).map(|(l, c)| Line::styled(l, st.fg(c))).collect();
                f.render_widget(Paragraph::new(g).centered(), Rect { y: filas.y, height: 3, ..zona });
            } else {
                f.render_widget(Paragraph::new(Line::styled(format!(" {} ", self.ctx.anio), st).centered()), Rect { y: fila.y, height: 1, ..zona });
            }
        }
        // Y la línea de debajo.
        if area.height >= 2 {
            let y = area.bottom() - 1;
            f.render_widget(Paragraph::new(Line::styled("─".repeat(area.width as usize), gris)), Rect { y, height: 1, ..area });
        }
    }

    /// Con la terminal estrecha, el menú va arriba, en dos líneas: la marca, las secciones y cómo va lo de traer lo
    /// último; y debajo, las entradas de la sección elegida, cada una con su tecla.
    fn cabecera(&self, f: &mut Frame, area: Rect) {
        let [l1, l2] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let mut v = vec![Span::raw(" "), Span::styled(" gesshtor ", tema::pestana_activa()), Span::raw("   ")];
        for &i in &self.visibles() {
            let st = if i == self.activa { negrita } else { gris };
            v.push(Span::styled(self.secciones[i].nombre.to_uppercase(), st));
            v.push(Span::raw("   "));
        }
        f.render_widget(Paragraph::new(Line::from(v)), l1);
        let ((glifo, st_glifo), (que, st_que), hora, _, _) = self.estado();
        let mut der = vec![Span::styled(glifo, st_glifo), Span::raw(" "), Span::styled(que, st_que)];
        if !hora.is_empty() && self.ctx.sincronizando.is_none() {
            der.push(Span::styled(format!(" · {hora}"), gris));
        }
        der.push(Span::raw(" "));
        f.render_widget(Paragraph::new(Line::from(der).right_aligned()), l1);
        // Las entradas de la sección elegida, a la altura de las secciones.
        let teclas = self.teclas();
        let primera = self.entradas().iter().position(|e| e.0 == self.activa).unwrap_or(0);
        let s = &self.secciones[self.activa];
        let mut v = vec![Span::raw(" ".repeat(14))];
        for (j, p) in s.pantallas.iter().enumerate() {
            let tecla = teclas.get(primera + j).cloned().unwrap_or_default();
            if j == s.activa {
                v.push(Span::styled(format!("{tecla} {}", p.titulo()), negrita.fg(tema::ACENTO)));
            } else {
                v.push(Span::styled(format!("{tecla} "), gris));
                v.push(Span::raw(p.titulo()));
            }
            v.push(Span::raw("   "));
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
        // Con el menú a la izquierda, lo de traer lo último ya está abajo en él.
        if area.width >= ANCHO_MENU {
            atajos.retain(|(k, _)| *k != "S");
        }
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
            Line::from("1 … 9, 0, C       ir a la entrada del menú con esa tecla (la de al lado)"),
            Line::from("Tab, Mayús+Tab    entrada siguiente / anterior del menú"),
            Line::from("- y +             año anterior / siguiente"),
            Line::from("↑ ↓, PgUp PgDn    moverse por las listas"),
            Line::from("Enter             ver o editar lo elegido"),
            Line::from("n                 nuevo (nómina, factura, gasto, cliente…)"),
            Line::from("o                 abrir el PDF (el de lo que ves, o elegir uno del año)"),
            Line::from("/                 buscar"),
        ];
        t.push(Line::from("S                 traer lo último de Hacienda y la Seguridad Social"));
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

/// «AP» de «Ana Pérez» (sin «de», «la»…); con una sola palabra, su primera letra.
fn iniciales(nombre: &str) -> String {
    let palabras: Vec<&str> = nombre.split_whitespace().filter(|p| !["de", "del", "la", "las", "los", "y"].contains(&p.to_lowercase().as_str())).collect();
    let letra = |p: &&str| p.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    match palabras.as_slice() {
        [] => String::new(),
        [p] => letra(p),
        [p, .., q] => letra(p) + &letra(q),
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
