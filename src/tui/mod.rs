//! La app: secciones (Resumen, Empleo, Autónomo, Patrimonio, Utilidades) con sus pestañas, el año, la barra de abajo
//! y el bucle de teclas. Se navega con la letra de la sección (`r`, `e`, `a`, `i`, `u`) y el número de la pestaña
//! (`1`, `2`…). Cada pantalla es un fichero de esta carpeta (y sus partes, `anios_*.rs`, `empleo_*.rs`…); el menú,
//! en `menu.rs`.
pub mod anios;
mod anios_balance;
mod anios_grafico;
mod anios_lista;
mod anios_renta;
mod anios_reparto;
pub mod clientes;
pub mod declaraciones;
mod declaraciones_detalle;
mod documentos;
pub mod empleo;
mod empleo_mes;
mod empleo_nominas;
mod empleo_sueldo;
mod fallos;
pub mod facturas;
mod facturas_editor;
pub mod gastos;
mod gastos_cuotas;
mod gastos_formulario;
pub mod impuestos;
mod impuestos_anio;
mod impuestos_casillas;
pub mod impuestos_trimestre;
pub mod inicio;
pub mod inversiones;
mod inversiones_conectar;
mod juego;
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
use ratatui::crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
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
        almacen::migrar(dir)?;
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
    /// (ni «q», ni las letras y los números de navegar), salvo Ctrl+C. Si no, esas letras y números no le llegan.
    fn capturando(&self) -> bool {
        false
    }
    /// Al entrar en la pestaña, cambiar de año o terminar de leer los PDF.
    fn al_entrar(&mut self, _ctx: &mut Ctx) {}
    /// Lo que se ve depende del año elegido. Si no (Inicio, la calculadora), no se enseña el año ni se cambia con - +.
    fn usa_anio(&self) -> bool {
        true
    }
    /// Recoge lo que haya terminado en segundo plano (traer lo de tu bróker), esté o no a la vista. Devuelve true
    /// si ha cambiado algo.
    fn atender(&mut self, _ctx: &mut Ctx) -> bool {
        false
    }
}

/// Desde este ancho, el carril de la izquierda (el nombre y las secciones); más estrecho, todo arriba.
const ANCHO_CARRIL: u16 = 100;
/// La columna donde empiezan las pestañas y el contenido cuando hay carril: nada del contenido se mete en él.
pub const CONTENIDO: u16 = 22;
/// La fila donde empieza el contenido cuando hay carril (las pestañas en la 1, su raya en la 2 y una de aire).
const FILA_CONTENIDO: u16 = 4;
/// Columnas libres a la derecha del contenido y, sin carril, también a la izquierda.
const MARGEN: u16 = 2;
/// Espacios entre las pestañas de arriba.
const ENTRE_PESTANAS: usize = 5;

/// Dónde va cada cosa del marco: la pantalla, la fila de los avisos y la de las teclas.
struct Zonas {
    /// ¿Con el carril de la izquierda?
    carril: bool,
    contenido: Rect,
    avisos: Rect,
    pie: Rect,
}

impl Zonas {
    /// Con el carril: el contenido desde la columna `CONTENIDO` y la fila `FILA_CONTENIDO`. Sin él: desde la
    /// columna `MARGEN` y, debajo de las dos filas de arriba, una de aire (si hay sitio). Siempre, dos columnas
    /// libres a la derecha. Abajo, los avisos y, debajo, la barra de las teclas, con `MARGEN` a cada lado y (si hay
    /// sitio) una fila de aire debajo.
    fn de(a: Rect) -> Self {
        let carril = a.width >= ANCHO_CARRIL;
        let (x, y) = if carril { (CONTENIDO, FILA_CONTENIDO) } else { (MARGEN, if a.height >= 20 { 3 } else { 2 }) };
        let x = x.min(a.width);
        let ancho = a.width.saturating_sub(MARGEN).saturating_sub(x);
        let aire = if a.height >= 20 { 1 } else { 0 };
        // La barra de abajo, con `MARGEN` a cada lado; en terminal estrecho, con uno (cada columna cuenta).
        let borde = if carril { MARGEN } else { 1 };
        let fila_pie = a.height.saturating_sub(1 + aire);
        let fila_avisos = fila_pie.saturating_sub(1);
        let y = y.min(fila_avisos);
        let una = a.height.min(1);
        Zonas {
            carril,
            contenido: Rect::new(a.x + x, a.y + y, ancho, fila_avisos - y),
            avisos: Rect::new(a.x + x, a.y + fila_avisos, ancho, una),
            pie: Rect::new(a.x + borde.min(a.width), a.y + fila_pie, a.width.saturating_sub(2 * borde), una),
        }
    }
}

/// Una sección del menú: su letra, su nombre, cuándo sale y sus pestañas (la elegida se recuerda al volver).
pub struct Seccion {
    pub tecla: char,
    pub nombre: &'static str,
    /// ¿Le toca a esta persona? (Empleo, si tiene o tuvo nómina…)
    pub sale: fn(&informe::Situacion) -> bool,
    pub pantallas: Vec<Box<dyn Pantalla>>,
    pub activa: usize,
}

impl Seccion {
    /// Una sección que sale siempre.
    pub fn new(tecla: char, nombre: &'static str, pantallas: Vec<Box<dyn Pantalla>>) -> Self {
        Seccion { tecla, nombre, sale: |_| true, pantallas, activa: 0 }
    }
    /// Que salga solo a quien le toque.
    pub fn si(self, sale: fn(&informe::Situacion) -> bool) -> Self {
        Seccion { sale, ..self }
    }
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
    /// Lo que no se ha podido leer o traer, entero (con «!»).
    fallos: Option<fallos::Fallos>,
    lectura: Option<Receiver<Mensaje>>,
    red: Option<Receiver<MensajeRed>>,
    /// Los documentos arrastrados al terminal, que clasifica Claude (si se ha encendido).
    documentos: documentos::Documentos,
}

impl App {
    pub fn new(ctx: Ctx) -> Self {
        let mut app = App { ctx, secciones: menu::secciones(), activa: 0, salir: false, ayuda: false, elegir_pdf: None, fallos: None, lectura: None, red: None, documentos: Default::default() };
        app.entrar();
        app
    }

    /// Las secciones que tocan a esta persona (Empleo si tiene o tuvo nómina, Autónomo si lo es o lo fue).
    pub fn visibles(&self) -> Vec<usize> {
        let s = informe::situacion(&self.ctx.datos);
        (0..self.secciones.len()).filter(|&i| (self.secciones[i].sale)(&s)).collect()
    }

    pub fn pantalla(&self) -> &dyn Pantalla {
        let s = &self.secciones[self.activa];
        s.pantallas[s.activa].as_ref()
    }

    pub fn titulo_activa(&self) -> &'static str {
        self.pantalla().titulo()
    }

    fn entrar(&mut self) {
        // Lo que haga la pantalla al entrar, ya con el color de su sección.
        tema::poner_seccion(self.secciones[self.activa].tecla);
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

    /// Las teclas de navegar, reservadas (las pantallas no las reciben, salvo escribiendo): la letra de una sección
    /// (`menu::RESERVADAS`) y, del `1` al `9`, una pestaña de la sección en la que estás. Devuelve true si la
    /// tecla es de navegar (aunque no lleve a ningún sitio: una sección que no te toca, una pestaña que no hay).
    fn navegar(&mut self, k: KeyEvent) -> bool {
        let KeyCode::Char(c) = k.code else { return false };
        if k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return false;
        }
        if let Some(n) = c.to_digit(10).filter(|&n| n > 0) {
            let s = &mut self.secciones[self.activa];
            if n as usize <= s.pantallas.len() {
                s.activa = n as usize - 1;
                self.entrar();
            }
            return true;
        }
        if !menu::RESERVADAS.contains(c) {
            return false;
        }
        match self.secciones.iter().position(|s| s.tecla == c) {
            Some(i) if self.visibles().contains(&i) => {
                self.activa = i;
                self.entrar();
            }
            Some(i) => self.ctx.info(format!("{} no sale con tus datos.", self.secciones[i].nombre)),
            None => {}
        }
        true
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
        if let Some(mut r) = fin {
            self.red = None;
            self.ctx.sincronizando = None;
            crate::sincronizar::aplicar(&self.ctx.dir, &mut self.ctx.datos, &mut r);
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
        let mut red = self.atender_red() | self.documentos.atender();
        for p in self.secciones.iter_mut().flat_map(|s| s.pantallas.iter_mut()) {
            red |= p.atender(&mut self.ctx);
        }
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
        // La ventana de los documentos espera mientras escribes en un formulario.
        if self.documentos.abierto() && !self.pantalla().capturando() {
            self.documentos.tecla(k, &mut self.ctx);
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
        if let Some(v) = &mut self.fallos {
            if v.tecla(k, &self.ctx) {
                self.fallos = None;
            }
            return;
        }
        let capt = self.pantalla().capturando();
        if !capt && k.code == KeyCode::Char('!') {
            self.fallos = Some(fallos::Fallos::new(&self.ctx));
            return;
        }
        if !capt && self.navegar(k) {
            return;
        }
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
            KeyCode::Tab => self.mover_pestana(1),
            KeyCode::BackTab => self.mover_pestana(-1),
            KeyCode::Char('-' | '_' | '[') if con_anio => self.cambiar_anio(-1),
            KeyCode::Char('+' | '=' | ']') if con_anio => self.cambiar_anio(1),
            _ => {}
        }
    }

    /// Lo que pega el terminal de una vez (al soltar ficheros en él, sus rutas). Escribiendo, se escribe; revisando
    /// un documento, solo a sus euros; si no (también con la lista de los que se leen a la vista), son documentos
    /// para clasificar.
    pub fn pegar(&mut self, texto: &str) {
        if self.pantalla().capturando() {
            for c in texto.chars().filter(|c| !c.is_control()) {
                self.tecla(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            }
            return;
        }
        if self.documentos.revisando() {
            self.documentos.escribir(texto);
            return;
        }
        self.ayuda = false;
        self.elegir_pdf = None;
        self.documentos.pegar(texto, &mut self.ctx);
    }

    /// ¿Se ve la ventana de los documentos que se están leyendo? (su juego y su ruedecita se mueven)
    fn animando(&self) -> bool {
        self.documentos.animando() && !self.pantalla().capturando()
    }

    /// Cada cuánto se repinta, en milisegundos: unas 25 veces por segundo con la ventana de leer a la vista (para el
    /// juego); 10, mientras algo va en segundo plano (para que se vea avanzar); si no, 2 (para que el aviso de abajo
    /// se borre solo).
    fn espera(&self) -> u64 {
        if self.animando() {
            40
        } else if self.ctx.leyendo.is_some() || self.ctx.sincronizando.is_some() || self.documentos.ocupado() {
            100
        } else {
            500
        }
    }

    pub fn dibujar(&mut self, f: &mut Frame) {
        // El color de la sección, para todo lo que se pinte ahora; y el fondo y la tinta, en toda la pantalla.
        tema::poner_seccion(self.secciones[self.activa].tecla);
        let area = f.area();
        f.buffer_mut().set_style(area, Style::new().fg(tema::TINTA).bg(tema::FONDO));
        let z = Zonas::de(area);
        if z.carril {
            self.carril(f, area);
            self.cabecera(f, area);
        } else {
            self.cabecera_estrecha(f, area);
        }
        let s = &mut self.secciones[self.activa];
        s.pantallas[s.activa].dibujar(f, z.contenido, &self.ctx);
        self.avisos(f, z.avisos);
        self.pie(f, z.pie, z.carril);
        if self.ayuda {
            self.dibujar_ayuda(f);
        }
        if let Some(sel) = &mut self.elegir_pdf {
            sel.dibujar(f, f.area());
        }
        if !self.pantalla().capturando() {
            self.documentos.dibujar(f, f.area(), &self.ctx);
        }
        // Lo que no se ha podido leer, encima de todo.
        if let Some(v) = &self.fallos {
            v.dibujar(f, f.area());
        }
        // Lo que se haya quedado sin color (debajo de un `Clear`), con el fondo y la tinta de la app.
        widgets::rellenar_fondo(f.buffer_mut());
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

    /// Cómo va lo de traer lo último de Hacienda y la Seguridad Social: «● al día · 22:10» (el ● en BIEN),
    /// «● Actualizando… <paso>» (en el color de la sección), «● con errores · 22:10» (en AVISO) o «○ sin
    /// actualizar» (en gris). Cabe en `max` columnas (el paso se corta). La hora: «22:10», «ayer 22:10» o «24 oct».
    fn estado(&self, max: usize) -> Vec<Span<'static>> {
        let hora = |c: &str| {
            let (dia, h) = (c.get(..10).unwrap_or_default(), c.get(11..16).unwrap_or_default());
            match fmt::dias_entre(dia, &fmt::hoy()) {
                0 => h.to_string(),
                1 => format!("ayer {h}"),
                _ => fmt::fecha_corta(dia).rsplit_once(' ').map_or(dia.to_string(), |(dm, _)| dm.to_string()),
            }
        };
        let gris = tema::apagado();
        match (&self.ctx.sincronizando, &self.ctx.sincronizado) {
            (Some(p), _) => {
                let ac = Style::new().fg(tema::acento());
                vec![Span::styled("● Actualizando… ", ac), Span::styled(cortar(p, max.saturating_sub(16)), gris)]
            }
            (None, Some((c, _, errores))) if !errores.is_empty() => {
                let aviso = Style::new().fg(tema::AVISO);
                vec![Span::styled("● con errores", aviso), Span::styled(format!(" · {}", hora(c)), gris)]
            }
            (None, Some((c, _, _))) => vec![Span::styled("● ", Style::new().fg(tema::BIEN)), Span::styled(format!("al día · {}", hora(c)), gris)],
            (None, None) => vec![Span::styled("○ sin actualizar", gris)],
        }
    }

    /// El nombre para saludar arriba a la derecha («Hola, Ana»), o nada.
    fn hola(&self) -> Option<Line<'static>> {
        let n = informe::inicio::saludo(&self.ctx.datos, self.ctx.certificado.as_deref());
        (!n.is_empty()).then(|| Line::styled(format!("Hola, {n}"), Style::new().add_modifier(Modifier::BOLD)))
    }

    /// Las pestañas de la sección («1 Inicio     2 Años     3 Declaraciones»): la activa, en negrita y del color de
    /// la sección; las demás, con el número en gris. Devuelve los trozos y dónde empieza y cuánto mide la activa
    /// (para su raya de debajo).
    fn pestanas(&self) -> (Vec<Span<'static>>, u16, u16) {
        let s = &self.secciones[self.activa];
        let mut v = Vec::new();
        let (mut col, mut ini, mut largo) = (0, 0, 0);
        for (j, p) in s.pantallas.iter().enumerate() {
            if j > 0 {
                v.push(Span::raw(" ".repeat(ENTRE_PESTANAS)));
                col += ENTRE_PESTANAS;
            }
            let txt = format!("{} {}", j + 1, p.titulo());
            let n = txt.chars().count();
            if j == s.activa {
                (ini, largo) = (col, n);
                v.push(Span::styled(txt, Style::new().fg(tema::acento()).add_modifier(Modifier::BOLD)));
            } else {
                v.push(Span::styled((j + 1).to_string(), tema::apagado()));
                v.push(Span::raw(format!(" {}", p.titulo())));
            }
            col += n;
        }
        (v, ini as u16, largo as u16)
    }

    /// «‹ 2026 ›», detrás de las pestañas: el año que ves, si la pantalla lo usa (se cambia con - +). Si no, nada.
    fn anio(&self) -> Vec<Span<'static>> {
        if !self.pantalla().usa_anio() {
            return Vec::new();
        }
        let gris = tema::apagado();
        vec![
            Span::raw(" ".repeat(ENTRE_PESTANAS)),
            Span::styled("‹ ", gris),
            Span::styled(self.ctx.anio.to_string(), Style::new().fg(tema::acento()).add_modifier(Modifier::BOLD)),
            Span::styled(" ›", gris),
        ]
    }

    /// El carril de la izquierda (columnas 0 a `CONTENIDO` - 1): el nombre en las filas 1 y 2 y, desde la fila
    /// `FILA_CONTENIDO`, una fila sí y otra no, las secciones que te tocan. La tuya, con su letra en negrita sobre su
    /// color y el nombre en su color; las demás, con la letra en su color y el nombre en gris.
    fn carril(&self, f: &mut Frame, area: Rect) {
        let (a, b) = menu::NOMBRE_COLOR;
        for (k, fila) in menu::NOMBRE.iter().enumerate() {
            let c: Vec<char> = fila.chars().collect();
            let trozo = |i: usize, j: usize| c[i.min(c.len())..j.min(c.len())].iter().collect::<String>();
            let l = Line::from(vec![Span::raw(trozo(0, a)), Span::styled(trozo(a, b), Style::new().fg(menu::NOMBRE_TONOS[k])), Span::raw(trozo(b, c.len()))]);
            let r = Rect::new(area.x + menu::NOMBRE_X, area.y + 1 + k as u16, CONTENIDO - menu::NOMBRE_X, 1).intersection(area);
            f.render_widget(Paragraph::new(l), r);
        }
        let vis = self.visibles();
        // Hasta la fila de los avisos; si no caben con una fila de aire entre ellas, seguidas.
        let tope = area.height.saturating_sub(2);
        let paso = if FILA_CONTENIDO + 2 * vis.len() as u16 <= tope + 1 { 2 } else { 1 };
        for (k, &i) in vis.iter().enumerate() {
            let y = FILA_CONTENIDO + k as u16 * paso;
            if y >= tope {
                break;
            }
            let s = &self.secciones[i];
            let color = tema::seccion(s.tecla);
            let letra = format!(" {} ", s.tecla);
            let l = if i == self.activa {
                Line::from(vec![Span::styled(letra, Style::new().fg(tema::FONDO).bg(color).add_modifier(Modifier::BOLD)), Span::raw("  "), Span::styled(s.nombre, Style::new().fg(color).add_modifier(Modifier::BOLD))])
            } else {
                Line::from(vec![Span::styled(letra, Style::new().fg(color).add_modifier(Modifier::BOLD)), Span::raw("  "), Span::styled(s.nombre, tema::apagado())])
            };
            f.render_widget(Paragraph::new(l), Rect::new(area.x + 2, area.y + y, CONTENIDO - 3, 1).intersection(area));
        }
    }

    /// La cabecera, con el carril. En la fila 1, desde la columna del contenido, las pestañas y el año; a la derecha,
    /// «Hola, Ana». En la 2, la raya de la pestaña activa y, a la derecha, la fecha y cómo va lo de traer lo último.
    /// Sin rayas de separación: el contenido empieza en la fila 4. Si no cabe todo, se quita antes el saludo, luego
    /// la fecha y luego el año; las pestañas, nunca.
    fn cabecera(&self, f: &mut Frame, area: Rect) {
        let x0 = area.x + CONTENIDO;
        let ancho = area.width.saturating_sub(CONTENIDO + MARGEN);
        let fila = |y: u16| Rect::new(x0, area.y + y, ancho, 1).intersection(area);
        let (mut l1, ini, largo) = self.pestanas();
        let anio = self.anio();
        if Line::from(l1.clone()).width() + Line::from(anio.clone()).width() <= ancho as usize {
            l1.extend(anio);
        }
        let usado = Line::from(l1.clone()).width();
        f.render_widget(Paragraph::new(Line::from(l1)), fila(1));
        if let Some(hola) = self.hola().filter(|h| usado + 5 + h.width() <= ancho as usize) {
            f.render_widget(Paragraph::new(hola.right_aligned()), fila(1));
        }
        // La raya de la activa y, a la derecha, la fecha y el estado (sin pisar la raya).
        let raya = Line::styled("▀".repeat(largo as usize), Style::new().fg(tema::acento()));
        f.render_widget(Paragraph::new(raya), Rect { x: x0 + ini, width: largo, ..fila(2) }.intersection(area));
        let libre = (ancho as usize).saturating_sub((ini + largo) as usize + 5);
        let estado = self.estado(libre);
        let larga = widgets::fecha_larga(&fmt::hoy());
        let corta = larga.split_once(", ").map_or(larga.clone(), |(_, x)| x.to_string());
        let gris = tema::apagado();
        let l2 = [larga, corta]
            .into_iter()
            .map(|fecha| Line::from([vec![Span::styled(format!("{fecha} · "), gris)], estado.clone()].concat()))
            .find(|l| l.width() <= libre)
            .unwrap_or_else(|| Line::from(estado));
        if l2.width() <= libre {
            f.render_widget(Paragraph::new(l2.right_aligned()), fila(2));
        }
    }

    /// La cabecera sin carril (terminal estrecho), en dos filas. En la 0, el nombre en texto («GESSHTOR») y las
    /// letras de las secciones que te tocan (la tuya, con su nombre); a la derecha, «Hola, Ana». En la 1, las
    /// pestañas y el año; a la derecha, cómo va lo de traer lo último. Lo de la derecha se quita si no cabe.
    fn cabecera_estrecha(&self, f: &mut Frame, area: Rect) {
        let x0 = area.x + MARGEN.min(area.width);
        let ancho = area.width.saturating_sub(2 * MARGEN);
        let fila = |y: u16| Rect::new(x0, area.y + y, ancho, 1).intersection(area);
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let mut l0 = vec![Span::styled("GE", negrita), Span::styled("SSH", negrita.fg(tema::acento())), Span::styled("TOR", negrita), Span::raw("   ")];
        for (k, &i) in self.visibles().iter().enumerate() {
            if k > 0 {
                l0.push(Span::raw("  "));
            }
            let s = &self.secciones[i];
            let color = tema::seccion(s.tecla);
            if i == self.activa {
                l0.push(Span::styled(format!(" {} ", s.tecla), Style::new().fg(tema::FONDO).bg(color).add_modifier(Modifier::BOLD)));
                l0.push(Span::styled(format!(" {}", s.nombre), negrita.fg(color)));
            } else {
                l0.push(Span::styled(s.tecla.to_string(), negrita.fg(color)));
            }
        }
        let usado = Line::from(l0.clone()).width();
        f.render_widget(Paragraph::new(Line::from(l0)), fila(0));
        if let Some(hola) = self.hola().filter(|h| usado + 3 + h.width() <= ancho as usize) {
            f.render_widget(Paragraph::new(hola.right_aligned()), fila(0));
        }
        let (mut l1, _, _) = self.pestanas();
        let anio = self.anio();
        if Line::from(l1.clone()).width() + Line::from(anio.clone()).width() <= ancho as usize {
            l1.extend(anio);
        }
        let libre = (ancho as usize).saturating_sub(Line::from(l1.clone()).width() + 3);
        f.render_widget(Paragraph::new(Line::from(l1)), fila(1));
        let estado = Line::from(self.estado(libre));
        if estado.width() <= libre {
            f.render_widget(Paragraph::new(estado.right_aligned()), fila(1));
        }
    }

    /// La fila de los avisos, encima de la de las teclas: por dónde va lo de traer lo último, la lectura de los PDF,
    /// el último aviso (durante 8 segundos), los documentos arrastrados o, mientras los haya, que algo no se ha podido
    /// leer o traer. Solo uno: el primero de esta lista.
    fn avisos(&self, f: &mut Frame, area: Rect) {
        let ac = Style::new().fg(tema::acento());
        let l = if let Some(p) = &self.ctx.sincronizando {
            Line::styled(p.clone(), ac)
        } else if let Some((h, t)) = self.ctx.leyendo {
            Line::styled(format!("Leyendo tus PDF de Hacienda: {h} de {t}…"), ac)
        } else if let Some((txt, nivel, cuando)) = &self.ctx.aviso
            && cuando.elapsed() < Duration::from_secs(8)
        {
            let st = match nivel {
                Nivel::Bien => Style::new().fg(tema::BIEN),
                Nivel::Mal => Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD),
                Nivel::Info => ac,
            };
            Line::styled(txt.clone(), st)
        } else if let Some(p) = self.documentos.progreso() {
            Line::styled(p, ac)
        } else if let Some(t) = fallos::aviso(&self.ctx) {
            // Mientras haya fallos, siempre a la vista (cuando no hay otra cosa que decir).
            Line::styled(format!("⚠ {t}"), Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD))
        } else {
            return;
        };
        f.render_widget(Paragraph::new(l), area);
    }

    /// La barra de abajo. A la izquierda, las teclas de la pantalla (y «- + año» si la pantalla usa el año) y, antes,
    /// cómo se navega («r e a p u sección   1 2 3 pestaña»; solo con el carril: sin él se ve arriba); a la derecha,
    /// siempre, «S traer lo último     ? ayuda   q salir». Las de la pantalla van antes que lo demás: si no cabe, se
    /// quitan primero las palabras «sección» y «pestaña», luego lo de navegar, luego «S traer lo último» se queda en
    /// «S traer» y solo entonces se quitan teclas de la pantalla, de la última a la primera. «? ayuda» y «q salir»,
    /// siempre. Escribiendo, solo las de la pantalla (las demás no llegan a la app), enteras: ver `teclas_apretadas`.
    fn pie(&self, f: &mut Frame, area: Rect, carril: bool) {
        // Una barra con su propio fondo y aire dentro, a cada lado: dos columnas (una en terminal estrecho).
        f.buffer_mut().set_style(area, Style::new().bg(tema::BARRA));
        let dentro = if carril { 2 } else { 1 };
        let area = Rect { x: area.x + dentro.min(area.width), width: area.width.saturating_sub(2 * dentro), ..area };
        let p = self.pantalla();
        let mut propias = p.atajos();
        // La S va siempre a la derecha.
        propias.retain(|(k, _)| *k != "S");
        let ancho = area.width as usize;
        if p.capturando() {
            f.render_widget(Paragraph::new(teclas_apretadas(&propias, ancho)), area);
            return;
        }
        if p.usa_anio() {
            propias.push(("- +", "año"));
        }
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let gris = tema::apagado();
        let letras: Vec<char> = self.visibles().iter().map(|&i| self.secciones[i].tecla).collect();
        let pestanas = self.secciones[self.activa].pantallas.len();
        // Lo de navegar: 0, entero; 1, sin las palabras («r e a p u   1 2 3»); 2, nada.
        let nav = |nivel: u8| {
            let mut v: Vec<Span<'static>> = Vec::new();
            if nivel >= 2 {
                return v;
            }
            for (k, t) in letras.iter().enumerate() {
                if k > 0 {
                    v.push(Span::raw(" "));
                }
                v.push(Span::styled(t.to_string(), negrita.fg(tema::seccion(*t))));
            }
            if nivel == 0 {
                v.push(Span::styled(" sección", gris));
            }
            if pestanas > 1 {
                let numeros: Vec<String> = (1..=pestanas).map(|i| i.to_string()).collect();
                v.extend([Span::raw("   "), Span::styled(numeros.join(" "), negrita)]);
                if nivel == 0 {
                    v.push(Span::styled(" pestaña", gris));
                }
            }
            v
        };
        let fijas = |s_corta: bool| {
            let mut v = widgets::atajo("S", if s_corta { "traer" } else { "traer lo último" }).to_vec();
            v.push(Span::raw("     "));
            v.extend(widgets::linea_atajos(&[("?", "ayuda"), ("q", "salir")]).spans);
            Line::from(v)
        };
        // Lo de la izquierda: lo de navegar, cinco espacios y las teclas de la pantalla.
        let izquierda = |nav: Vec<Span<'static>>, propias: &[(&str, &str)]| {
            let mut v = nav;
            if !propias.is_empty() {
                if !v.is_empty() {
                    v.push(Span::raw("     "));
                }
                v.extend(widgets::linea_atajos(propias).spans);
            }
            Line::from(v)
        };
        let (mut nivel, mut s_corta) = (if carril { 0 } else { 2 }, false);
        // Entre lo de la izquierda y lo de la derecha, al menos cinco espacios.
        while izquierda(nav(nivel), &propias).width() + 5 + fijas(s_corta).width() > ancho {
            if nivel < 2 {
                nivel += 1;
            } else if !s_corta {
                s_corta = true;
            } else if propias.pop().is_none() {
                break;
            }
        }
        f.render_widget(Paragraph::new(izquierda(nav(nivel), &propias)), area);
        f.render_widget(Paragraph::new(fijas(s_corta).right_aligned()), area);
    }

    /// La ayuda (`?`), en una ventana encima: las teclas de la app, en negrita, y lo que hacen, en gris; abajo, la
    /// versión.
    fn dibujar_ayuda(&self, f: &mut Frame) {
        let r = widgets::centrado(f.area(), 92, 23);
        let dentro = widgets::ventana(f, r, "Ayuda");
        let vis = self.visibles();
        let letras: Vec<String> = vis.iter().map(|&i| self.secciones[i].tecla.to_string()).collect();
        let nombres: Vec<&str> = vis.iter().map(|&i| self.secciones[i].nombre).collect();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let gris = tema::apagado();
        let teclas = [
            (letras.join(" "), format!("ir a una sección: {}", nombres.join(", "))),
            ("1 2 3 …".into(), "ir a una pestaña de la sección (su número, arriba)".into()),
            ("Tab, Mayús+Tab".into(), "pestaña siguiente / anterior".into()),
            ("- y +".into(), "año anterior / siguiente".into()),
            ("↑ ↓, PgUp PgDn".into(), "moverse por las listas".into()),
            ("Enter".into(), "ver o editar lo elegido".into()),
            ("n".into(), "nuevo (nómina, factura, gasto…)".into()),
            ("o".into(), "abrir el PDF (el de lo que ves, o elegir uno del año)".into()),
            ("/".into(), "buscar".into()),
            ("S".into(), "traer lo último de Hacienda y la Seguridad Social".into()),
            ("arrastrar".into(), "un PDF o una foto: Claude dice si es nómina, gasto o ingreso".into()),
            ("  Espacio · o".into(), "saltar en el juego · abrir el original al revisarlos".into()),
            ("!".into(), "lo que no se ha podido leer o traer: verlo entero y copiarlo".into()),
            ("q".into(), "salir".into()),
        ];
        let mut t = vec![Line::from("")];
        t.extend(teclas.into_iter().map(|(k, d): (String, String)| Line::from(vec![Span::styled(format!("{k:<18}"), negrita), Span::styled(d, gris)])));
        t.extend([
            Line::from(""),
            Line::styled("Los años pasados se ven con lo que dicen tus Rentas y la Seguridad Social;", gris),
            Line::styled("en el año en curso apuntas facturas y gastos, y nóminas en cualquiera.", gris),
            Line::styled("Escribiendo, las letras y los números se escriben. Todo se guarda solo.", gris),
        ]);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), dentro);
        // La versión y el commit, para saber de qué código sale lo que se ve.
        widgets::pie_ventana(f, r, Line::styled(format!("gesshtor {}", crate::VERSION), gris));
    }
}

/// Las teclas de una pantalla mientras se escribe (todas son de ella): enteras si caben en `ancho`. Si no, cada
/// descripción se queda en su primera palabra, luego van a dos espacios, luego se recorta la descripción más larga
/// (con «…», hasta dos letras) y, solo al final, se quitan teclas del final.
fn teclas_apretadas(atajos: &[(&str, &str)], ancho: usize) -> Line<'static> {
    let linea = |v: &[(String, String)], hueco: usize| {
        let mut l = Vec::new();
        for (i, (k, d)) in v.iter().enumerate() {
            if i > 0 {
                l.push(Span::raw(" ".repeat(hueco)));
            }
            l.extend(widgets::atajo(k, d));
        }
        Line::from(l)
    };
    let mut v: Vec<(String, String)> = atajos.iter().map(|(k, d)| (k.to_string(), d.to_string())).collect();
    let mut hueco = 3;
    while linea(&v, hueco).width() > ancho {
        if v.iter().any(|(_, d)| d.contains(' ')) {
            for (_, d) in &mut v {
                *d = d.split(' ').next().unwrap_or_default().to_string();
            }
        } else if hueco > 2 {
            hueco = 2;
        } else if let Some((_, d)) = v.iter_mut().filter(|(_, d)| d.chars().count() > 3).max_by_key(|(_, d)| d.chars().count()) {
            let n = d.chars().count();
            *d = d.chars().take(n - 2).chain(['…']).collect();
        } else if v.pop().is_none() {
            break;
        }
    }
    linea(&v, hueco)
}

/// `s` en `n` columnas como mucho, con «…» si se corta.
fn cortar(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).chain(['…']).take(n).collect()
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

pub fn ejecutar(mut terminal: DefaultTerminal, mut app: App) -> std::io::Result<()> {
    // Lo pegado llega de una vez (al soltar ficheros, sus rutas): ver `App::pegar`. Donde no se puede, se escribe
    // como siempre, tecla a tecla.
    let _ = ratatui::crossterm::execute!(std::io::stdout(), EnableBracketedPaste);
    while !app.salir {
        app.atender();
        // Lo que se mueve en la ventana de los documentos (el juego, la ruedecita), hasta ahora.
        if app.animando() {
            app.documentos.animar(Instant::now());
        }
        terminal.draw(|f| app.dibujar(f))?;
        // Se repinta cada poco para que el aviso de abajo se borre solo y avance lo que va en segundo plano.
        if event::poll(Duration::from_millis(app.espera()))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.tecla(k),
                Event::Paste(texto) => app.pegar(&texto),
                _ => {}
            }
        }
    }
    let _ = ratatui::crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::prueba::{app, app_con, cargar, caso, ir, pantalla, teclas, titulos};
    use super::{menu, App};
    use crate::dominio::fmt::{self, anio_actual};
    use crate::dominio::{Cliente, Datos, Factura, Linea, Nomina, Num, PRIMER_ANIO};

    /// Una factura emitida hoy y sin cobrar, a «Zeta Pruebas»: la primera de Facturas, sea el año que sea.
    pub(super) fn factura_de_hoy(d: &mut Datos) {
        let cliente = Cliente { id: "c-zeta".into(), nombre: "Zeta Pruebas".into(), nif: "B12345674".into(), pais: "ES".into(), ..Default::default() };
        let linea = Linea { concepto: "Trabajo".into(), cantidad: Num::de(1.0), precio: Num::de(100.0), iva: Num::de(21.0) };
        d.facturas.push(Factura { id: "f-hoy".into(), estado: "emitida".into(), numero: "HOY-1".into(), fecha: fmt::hoy(), operacion: "nacional".into(), cliente, lineas: vec![linea], ..Default::default() });
    }

    /// La autónoma de «a-de-todo» con una nómina (le salen todas las secciones) y una factura de hoy.
    fn con_empleo() -> App {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        d.nominas.push(Nomina { id: "n1".into(), anio: anio_actual(), mes: 1, dinerario: 2000.0, ..Default::default() });
        factura_de_hoy(&mut d);
        app_con(d, &dir)
    }

    fn aviso(a: &App) -> String {
        a.ctx.aviso.as_ref().map(|(t, ..)| t.clone()).unwrap_or_default()
    }

    /// Cada sección va con su letra y cada pestaña con su número (la de cada sección se recuerda); el menú lo enseña.
    #[test]
    fn una_letra_por_seccion_y_un_numero_por_pestana() {
        let mut a = con_empleo();
        let pasos = [
            ("a", "Facturas"),
            ("2", "Gastos"),
            ("3", "Impuestos"),
            ("9", "Impuestos"),
            ("e", "Sueldo"),
            ("2", "Nóminas"),
            ("r", "Inicio"),
            ("2", "Años"),
            ("3", "Declaraciones"),
            ("u", "Calcula tu sueldo"),
            ("1", "Calcula tu sueldo"),
            ("a", "Impuestos"),
            ("1", "Facturas"),
            ("e", "Nóminas"),
        ];
        for (k, t) in pasos {
            teclas(&mut a, k);
            assert_eq!(a.titulo_activa(), t, "tras «{k}»");
        }
        // A la izquierda, cada sección con su letra; arriba, las pestañas de la sección en la que estás, con su número
        // (solo ahí sirven), y la raya bajo la activa. Las de las otras secciones no salen.
        let p = pantalla(&mut a, 120, 36);
        let filas: Vec<&str> = p.lines().collect();
        assert!(p.contains(" r   Resumen") && p.contains(" e   Empleo") && p.contains(" a   Autónomo") && p.contains(" u   Utilidades"), "{p}");
        assert!(filas[1].contains("1 Sueldo     2 Nóminas") && !filas[1].contains("Facturas"), "{p}");
        assert_eq!(filas[2].chars().position(|c| c == '▀'), Some(super::CONTENIDO as usize + "1 Sueldo     ".len()), "{p}");
        // Y abajo, cómo se navega (sin las palabras si no caben con las teclas de la pantalla).
        assert!(filas[34].starts_with("    r e a p u ") && filas[34].contains("1 2"), "{p}");
        // Con la terminal estrecha, arriba: el nombre y las letras de las secciones (la tuya, con su nombre) y,
        // debajo, las pestañas de la tuya.
        let p = pantalla(&mut a, 80, 24);
        let filas: Vec<&str> = p.lines().collect();
        assert!(filas[0].contains("GESSHTOR") && filas[0].contains("r   e  Empleo  a  p  u"), "{p}");
        assert!(filas[1].contains("1 Sueldo") && filas[1].contains("2 Nóminas"), "{p}");
        // Y en la ayuda.
        teclas(&mut a, "?");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.lines().any(|l| l.contains("r e a p u ") && l.contains("ir a una sección: Resumen, Empleo, Autónomo")), "{p}");
    }

    /// El nombre de arriba a la izquierda: sus dos filas miden lo mismo (una celda por carácter), caben antes de la
    /// columna del contenido con una de aire y el tramo en color cae dentro. Y se pinta en las filas 1 y 2.
    #[test]
    fn el_nombre_cabe_en_el_carril() {
        use ratatui::text::Line;
        let [a, b] = menu::NOMBRE.map(|f| Line::from(f).width());
        assert_eq!(a, b, "las dos filas, igual de anchas");
        assert!(menu::NOMBRE.iter().all(|f| f.chars().count() == a), "una celda por carácter: los cortes de color van por caracteres");
        assert!(menu::NOMBRE_X as usize + a < super::CONTENIDO as usize, "antes de la columna {}", super::CONTENIDO);
        let (desde, hasta) = menu::NOMBRE_COLOR;
        assert!(desde < hasta && hasta <= a, "el tramo en color, dentro del nombre");
        let p = pantalla(&mut app(), 120, 36);
        for (k, fila) in menu::NOMBRE.iter().enumerate() {
            assert!(p.lines().nth(1 + k).unwrap().starts_with(&format!("  {}", fila.trim_end())), "{p}");
        }
    }

    /// Toda la pantalla lleva el fondo y la tinta de la app, también con las ventanas encima (la ayuda, los fallos,
    /// el selector de PDF, el editor de una factura, la de cobrarla…): ninguna celda con los colores del terminal.
    #[test]
    fn sin_agujeros_en_el_fondo() {
        use ratatui::style::Color;
        for (t, k) in [("Inicio", ""), ("Inicio", "?"), ("Inicio", "!"), ("Declaraciones", "o"), ("Facturas", "n"), ("Facturas", "n<esc>"), ("Facturas", "c"), ("Gastos", "n")] {
            for (w, h) in [(120, 36), (80, 24)] {
                let mut a = con_empleo();
                ir(&mut a, t);
                teclas(&mut a, k);
                let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                term.draw(|f| a.dibujar(f)).unwrap();
                let b = term.backend().buffer();
                let mal = b.content.iter().position(|c| c.bg == Color::Reset || c.fg == Color::Reset);
                assert!(mal.is_none(), "{t} «{k}» {w}x{h}: celda {mal:?}");
            }
        }
    }

    /// En la barra de abajo, las teclas de la pantalla van antes: si no cabe todo, se quitan primero las palabras de
    /// navegar, luego lo de navegar entero, luego «S traer lo último» se queda en «S traer» y solo entonces se van
    /// las teclas del final. «? ayuda» y «q salir», siempre. Escribiendo, enteras aunque sea recortando lo que dicen.
    #[test]
    fn el_pie_da_prioridad_a_las_teclas_de_la_pantalla() {
        let pie = |a: &mut App, w: u16, h: u16| pantalla(a, w, h).lines().nth(h as usize - 2).unwrap().to_string();
        let mut a = app();
        // Con sitio de sobra (Inicio tiene una sola tecla), todo entero.
        let p = pie(&mut a, 120, 36);
        assert_eq!(p, format!("    r a p u sección   1 2 3 pestaña     Tab más{}S traer lo último     ? ayuda   q salir", " ".repeat(30)));
        // En Facturas, sin lo de navegar, con «S traer» y las teclas que caben, de la primera en adelante.
        ir(&mut a, "Facturas");
        let p = pie(&mut a, 120, 36);
        assert!(p.starts_with("    n nueva   Enter ver   c cobrada   f filtro   / buscar   C clientes   "), "{p}");
        assert!(p.ends_with("S traer     ? ayuda   q salir") && !p.contains("sección") && !p.contains("r a p u"), "{p}");
        let p = pie(&mut a, 80, 24);
        assert!(p.starts_with("  n nueva   Enter ver   c cobrada") && p.ends_with("S traer     ? ayuda   q salir"), "{p}");
        // Si lo de navegar entero no cabe con las teclas de la pantalla (Nóminas), sin las palabras.
        let mut a = con_empleo();
        teclas(&mut a, "e2");
        let p = pie(&mut a, 120, 36);
        assert!(p.starts_with("    r e a p u   1 2     n nueva") && p.ends_with("S traer lo último     ? ayuda   q salir"), "{p}");
        // Escribiendo (el editor de una factura), a 80 columnas: todas las teclas, con lo que dicen recortado.
        ir(&mut a, "Facturas");
        teclas(&mut a, "n<esc>");
        let p = pie(&mut a, 80, 24);
        assert!(p.starts_with("  Enter elegir  ↑↓ campo  Tab líneas  Ctrl+S guardar  Ctrl+E emitir  Esc salir"), "{p}");
        // Primero, la primera palabra; luego, a dos espacios; luego, lo más largo recortado; y al final, fuera teclas.
        let apretadas = |v: &[(&str, &str)], w: usize| super::teclas_apretadas(v, w).to_string();
        assert_eq!(apretadas(&[("a", "uno dos"), ("b", "tres")], 14), "a uno   b tres");
        assert_eq!(apretadas(&[("a", "uno dos"), ("b", "tres")], 13), "a uno  b tres");
        assert_eq!(apretadas(&[("a", "uno dos"), ("b", "tres")], 12), "a uno  b tr…");
        assert_eq!(apretadas(&[("a", "uno dos"), ("b", "tres")], 8), "a uno");
    }

    /// La letra de una sección que no te toca no la abre: se dice. Un número sin pestaña no hace nada.
    #[test]
    fn una_seccion_que_no_sale() {
        let mut a = app();
        teclas(&mut a, "a2e");
        assert_eq!(a.titulo_activa(), "Gastos");
        assert!(aviso(&a).contains("Empleo"), "{}", aviso(&a));
        teclas(&mut a, "u2");
        assert_eq!(a.titulo_activa(), "Calcula tu sueldo");
    }

    /// Desde dentro de una pantalla (la ficha de una factura o un gasto, el detalle de un año, las casillas de una
    /// declaración, tus clientes) y en un año cerrado, las letras y los números llevan a su sitio.
    #[test]
    fn se_navega_desde_cualquier_sitio() {
        let pasos = [
            ("a1<enter>", "r", "Inicio"),
            ("a2<down>", "u", "Calcula tu sueldo"),
            ("r2<enter>", "a", "Facturas"),
            ("r3<enter>", "1", "Inicio"),
            ("a1C", "e", "Sueldo"),
            ("a1C<down>", "2", "Gastos"),
        ];
        for anio in [anio_actual(), PRIMER_ANIO] {
            for (antes, k, t) in pasos {
                let mut a = con_empleo();
                a.ctx.anio = anio;
                teclas(&mut a, antes);
                assert!(!a.pantalla().capturando(), "{antes}");
                teclas(&mut a, k);
                assert_eq!(a.titulo_activa(), t, "{anio}: «{antes}» y «{k}»");
            }
        }
        // Al volver a Facturas, tus clientes siguen abiertos; Esc vuelve a las facturas.
        let mut a = con_empleo();
        teclas(&mut a, "a1Cr" );
        teclas(&mut a, "a");
        assert!(pantalla(&mut a, 120, 36).contains("Tus clientes"));
        teclas(&mut a, "<esc>");
        assert!(!pantalla(&mut a, 120, 36).contains("Tus clientes"));
    }

    /// Escribiendo (una búsqueda, un formulario, el editor de una factura, elegir una categoría), las letras y los
    /// números se escriben.
    #[test]
    fn escribiendo_no_se_navega() {
        const T: &str = "area 12 ui";
        let casos = [
            ("Facturas", "/"),
            ("Facturas", "n"),
            ("Facturas", "C/"),
            ("Facturas", "Cn"),
            ("Facturas", "c"),
            ("Gastos", "/"),
            ("Gastos", "n"),
            ("Gastos", "c"),
            ("Declaraciones", "/"),
            ("Sueldo", "n"),
            ("Nóminas", "n"),
            ("Calcula tu sueldo", "<enter>"),
        ];
        for (titulo, abrir) in casos {
            let mut a = con_empleo();
            ir(&mut a, titulo);
            teclas(&mut a, abrir);
            assert!(a.pantalla().capturando(), "{titulo} «{abrir}»");
            teclas(&mut a, T);
            assert_eq!(a.titulo_activa(), titulo, "«{abrir}»");
            let p = pantalla(&mut a, 120, 36);
            assert!(p.contains(T), "{titulo} «{abrir}»:\n{p}");
        }
    }

    /// Ninguna pantalla anuncia abajo una tecla de navegar: no le llegaría.
    #[test]
    fn ninguna_pantalla_usa_las_teclas_de_navegar() {
        let reservada = |k: &str| k.chars().count() == 1 && k.chars().all(|c| menu::RESERVADAS.contains(c) || c.is_ascii_digit());
        for t in titulos(&con_empleo()) {
            for anio in [anio_actual(), anio_actual() - 1, PRIMER_ANIO] {
                for paso in ["", "<enter>", "<down><enter>", "C", "/<enter>"] {
                    let mut a = con_empleo();
                    a.ctx.anio = anio;
                    ir(&mut a, t);
                    teclas(&mut a, paso);
                    pantalla(&mut a, 120, 36);
                    if a.pantalla().capturando() {
                        continue;
                    }
                    for (k, que) in a.pantalla().atajos() {
                        assert!(!reservada(k), "{t} {anio} «{paso}»: {k} {que}");
                    }
                }
            }
        }
    }

    /// Lo de siempre sigue: `-` `+` el año, Tab la pestaña siguiente (también de una sección a la otra), `o` los PDF,
    /// `S` traer lo último (aquí, sin certificado, se dice) y `q` salir.
    #[test]
    fn las_teclas_de_siempre() {
        let hoy = anio_actual();
        let mut a = app();
        teclas(&mut a, "a1-");
        assert_eq!(a.ctx.anio, hoy - 1);
        teclas(&mut a, "+");
        assert_eq!(a.ctx.anio, hoy);
        teclas(&mut a, "3<tab>");
        assert_eq!(a.titulo_activa(), "Este año", "la sección siguiente: Patrimonio");
        teclas(&mut a, "<s-tab>");
        assert_eq!(a.titulo_activa(), "Impuestos");
        teclas(&mut a, "r2o");
        assert!(aviso(&a).contains("PDF"), "{}", aviso(&a));
        teclas(&mut a, "S");
        assert!(aviso(&a).contains("certificado"), "{}", aviso(&a));
        assert!(!a.salir);
        teclas(&mut a, "q");
        assert!(a.salir);
    }
}
