//! La app: secciones (Resumen, Empleo, Autónomo, Patrimonio, Utilidades) con sus pestañas, el año, la barra de abajo
//! y el bucle de teclas. Se navega con la letra de la sección (`r`, `e`, `a`, `i`, `u`) y el número de la pestaña
//! (`1`, `2`…). Cada pantalla es un fichero de esta carpeta (y sus partes, `anios_*.rs`, `empleo_*.rs`…); el menú,
//! en `menu.rs`; tu gestor, debajo del menú, en `gestor.rs`.
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
mod gestor;
pub mod impuestos;
mod impuestos_ahora;
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
    /// Cuántos modelos hay fuera de plazo y sin presentar: se avisa en rojo en el menú, en su pestaña y en Inicio.
    pub vencidos: usize,
    /// Al abrir la app, hasta que acaba la primera lectura de los PDF: aún no se sabe qué se presentó ni cuándo estabas
    /// de alta, así que no se avisa de nada vencido.
    pub sin_leer: bool,
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
        Ctx { datos, dir, anio: fmt::anio_actual(), aviso: None, generacion: 0, leyendo: None, ir_a: None, pdfs_de: None, vencidos: 0, sin_leer: false, certificado: None, sincronizando: None, sincronizado: None, copia_hecha: false, sin_disco: false, _candado: None }
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
/// La columna donde empiezan las pestañas y el contenido cuando hay carril: nada del contenido se mete en él. El
/// carril acaba en la 21 (el dibujo del gestor, de la 2 a la 21) y quedan dos columnas de aire.
pub const CONTENIDO: u16 = 24;
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

/// Las columnas que deja la barra de abajo a cada lado (`MARGEN`; en terminal estrecho, una: cada columna cuenta) y,
/// dentro de su fondo, antes de las teclas (dos; en terminal estrecho, una), en un terminal de `ancho` columnas.
fn borde_pie(ancho: u16) -> u16 {
    if ancho >= ANCHO_CARRIL { MARGEN } else { 1 }
}

fn dentro_pie(ancho: u16) -> u16 {
    if ancho >= ANCHO_CARRIL { 2 } else { 1 }
}

/// Las columnas para las teclas de la barra de abajo, en un terminal de `ancho` columnas.
fn ancho_pie(ancho: u16) -> usize {
    ancho.saturating_sub(2 * borde_pie(ancho) + 2 * dentro_pie(ancho)) as usize
}

impl Zonas {
    /// Con el carril: el contenido desde la columna `CONTENIDO` y la fila `FILA_CONTENIDO`. Sin él: desde la
    /// columna `MARGEN` y, debajo de las dos filas de arriba, una de aire (si hay sitio). Siempre, dos columnas
    /// libres a la derecha. Abajo, los avisos y, debajo, la barra de las teclas (de `filas_pie` filas), con `MARGEN`
    /// a cada lado y (si hay sitio) una fila de aire debajo.
    fn de(a: Rect, filas_pie: u16) -> Self {
        let carril = a.width >= ANCHO_CARRIL;
        let (x, y) = if carril { (CONTENIDO, FILA_CONTENIDO) } else { (MARGEN, if a.height >= 20 { 3 } else { 2 }) };
        let x = x.min(a.width);
        let ancho = a.width.saturating_sub(MARGEN).saturating_sub(x);
        let aire = if a.height >= 20 { 1 } else { 0 };
        let borde = borde_pie(a.width);
        let fila_pie = a.height.saturating_sub(filas_pie + aire);
        let fila_avisos = fila_pie.saturating_sub(1);
        let y = y.min(fila_avisos);
        let una = a.height.min(1);
        Zonas {
            carril,
            contenido: Rect::new(a.x + x, a.y + y, ancho, fila_avisos - y),
            avisos: Rect::new(a.x + x, a.y + fila_avisos, ancho, una),
            pie: Rect::new(a.x + borde.min(a.width), a.y + fila_pie, a.width.saturating_sub(2 * borde), filas_pie.min(a.height - fila_pie)),
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
    /// La ventana para elegir gestor (con `G`).
    elegir_gestor: Option<gestor::Elegir>,
    /// Con qué datos y qué día se contó lo vencido (`ctx.vencidos`).
    vencidos_de: Option<(u64, String)>,
}

impl App {
    pub fn new(ctx: Ctx) -> Self {
        let mut app = App { ctx, secciones: menu::secciones(), activa: 0, salir: false, ayuda: false, elegir_pdf: None, fallos: None, lectura: None, red: None, documentos: Default::default(), elegir_gestor: None, vencidos_de: None };
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

    /// Cuenta lo vencido si han cambiado los datos o el día.
    fn contar_vencidos(&mut self) {
        if self.ctx.sin_leer {
            return;
        }
        let k = (self.ctx.generacion, fmt::hoy());
        if self.vencidos_de.as_ref() != Some(&k) {
            let autonomo = informe::situacion(&self.ctx.datos).autonomo;
            self.ctx.vencidos = if autonomo { impuestos_ahora::vencidos(&self.ctx.datos, &k.1) } else { 0 };
            self.vencidos_de = Some(k);
        }
    }

    /// ¿Va en rojo esta sección? La que tiene los modelos, si hay alguno vencido.
    fn en_rojo(&self, s: &Seccion) -> bool {
        self.ctx.vencidos > 0 && s.pantallas.iter().any(|p| p.titulo() == impuestos::TITULO)
    }

    fn entrar(&mut self) {
        // Por si los datos cambiaron sin pasar por `guardar`.
        self.vencidos_de = None;
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
        self.ctx.sin_leer = self.ctx.generacion == 0;
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
            self.ctx.sin_leer = false;
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
        if let Some(v) = &mut self.elegir_gestor {
            match v.tecla(k) {
                gestor::Respuesta::Contratado(i) => {
                    self.elegir_gestor = None;
                    self.contratar(i);
                }
                gestor::Respuesta::Cancelado => self.elegir_gestor = None,
                gestor::Respuesta::Sigue => {}
            }
            return;
        }
        let capt = self.pantalla().capturando();
        if !capt && k.code == KeyCode::Char('!') {
            self.fallos = Some(fallos::Fallos::new(&self.ctx));
            return;
        }
        // Elegir gestor: reservada, como las de navegar (ninguna pantalla la recibe, salvo escribiendo).
        if !capt && k.code == KeyCode::Char(gestor::TECLA) && !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            self.elegir_gestor = Some(gestor::Elegir { i: gestor::elegido(&self.ctx.datos) });
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

    /// Contrata al gestor `i` de `gestor::GESTORES`: se guarda en tus datos y se dice abajo.
    fn contratar(&mut self, i: usize) {
        let g = &gestor::GESTORES[i];
        self.ctx.datos.gestor = g.clave.to_string();
        if self.ctx.guardar() {
            self.ctx.ok(g.contratado);
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
        self.elegir_gestor = None;
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
        self.contar_vencidos();
        let area = f.area();
        let filas = self.lineas_pie(area.width >= ANCHO_CARRIL, ancho_pie(area.width)).len();
        // Las teclas de la pantalla pueden depender de lo que calcula al dibujarse (lo elegido de una lista…): si
        // luego no caben en las filas de la barra que se le dejaron (o sobra una), otra vez, desde cero.
        if let Some(otras) = self.dibujar_con(f, filas) {
            f.render_widget(ratatui::widgets::Clear, area);
            self.dibujar_con(f, otras);
        }
    }

    /// La app, con `filas` filas para la barra de abajo; si al final sus teclas necesitan otras, cuántas.
    fn dibujar_con(&mut self, f: &mut Frame, filas: usize) -> Option<usize> {
        // El color de la sección, para todo lo que se pinte ahora; y el fondo y la tinta, en toda la pantalla.
        tema::poner_seccion(self.secciones[self.activa].tecla);
        let area = f.area();
        f.buffer_mut().set_style(area, Style::new().fg(tema::TINTA).bg(tema::FONDO));
        let z = Zonas::de(area, filas as u16);
        if z.carril {
            self.carril(f, area, z.pie.y);
            self.cabecera(f, area);
        } else {
            self.cabecera_estrecha(f, area);
        }
        let s = &mut self.secciones[self.activa];
        s.pantallas[s.activa].dibujar(f, z.contenido, &self.ctx);
        let lineas = self.lineas_pie(z.carril, ancho_pie(area.width));
        self.avisos(f, z.avisos);
        self.pie(f, z.pie, &lineas);
        if self.ayuda {
            self.dibujar_ayuda(f);
        }
        if let Some(sel) = &mut self.elegir_pdf {
            sel.dibujar(f, f.area());
        }
        if let Some(v) = &self.elegir_gestor {
            v.dibujar(f, f.area());
        }
        if !self.pantalla().capturando() {
            // Con el carril, a su derecha (sobre el contenido): así se ve la mesa del gestor mientras se leen.
            let zona = if z.carril { Rect { y: area.y, height: area.height, ..z.contenido } } else { area };
            self.documentos.dibujar(f, zona, &self.ctx);
        }
        // Lo que no se ha podido leer, encima de todo.
        if let Some(v) = &self.fallos {
            v.dibujar(f, f.area());
        }
        // Lo que se haya quedado sin color (debajo de un `Clear`), con el fondo y la tinta de la app.
        widgets::rellenar_fondo(f.buffer_mut());
        (lineas.len() != filas).then_some(lineas.len())
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
            let rojo = self.ctx.vencidos > 0 && p.titulo() == impuestos::TITULO;
            if j == s.activa {
                (ini, largo) = (col, n);
                v.push(Span::styled(txt, Style::new().fg(if rojo { tema::MAL } else { tema::acento() }).add_modifier(Modifier::BOLD)));
            } else {
                v.push(Span::styled((j + 1).to_string(), tema::apagado()));
                let st = if rojo { Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD) } else { Style::new() };
                v.push(Span::styled(format!(" {}", p.titulo()), st));
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

    /// El carril de la izquierda (columnas 0 a `CONTENIDO` - 3; las dos últimas, de aire): el nombre en las filas 1
    /// y 2 y, desde la fila `FILA_CONTENIDO`, una fila sí y otra no, las secciones que te tocan. La tuya, con su
    /// letra en negrita sobre su color y el nombre en su color; las demás, con la letra en su color y el nombre en
    /// gris. Debajo, tras una fila de aire, tu gestor con su mesa, hasta la fila de los avisos (incluida; la de la
    /// barra de abajo es `pie`; ver `gestor::dibujar_bloque`: si no cabe, sin el dibujo, o nada).
    fn carril(&self, f: &mut Frame, area: Rect, pie: u16) {
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
        let tope = pie.saturating_sub(area.y);
        let paso = if FILA_CONTENIDO + 2 * vis.len() as u16 <= tope + 1 { 2 } else { 1 };
        // La fila siguiente a la última sección.
        let mut debajo = FILA_CONTENIDO;
        for (k, &i) in vis.iter().enumerate() {
            let y = FILA_CONTENIDO + k as u16 * paso;
            if y >= tope {
                break;
            }
            debajo = y + 1;
            let s = &self.secciones[i];
            let color = tema::seccion(s.tecla);
            let letra = format!(" {} ", s.tecla);
            let rojo = Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD);
            let l = if i == self.activa {
                let nombre = if self.en_rojo(s) { rojo } else { Style::new().fg(color).add_modifier(Modifier::BOLD) };
                Line::from(vec![Span::styled(letra, Style::new().fg(tema::FONDO).bg(color).add_modifier(Modifier::BOLD)), Span::raw("  "), Span::styled(s.nombre, nombre)])
            } else {
                let nombre = if self.en_rojo(s) { rojo } else { tema::apagado() };
                Line::from(vec![Span::styled(letra, Style::new().fg(color).add_modifier(Modifier::BOLD)), Span::raw("  "), Span::styled(s.nombre, nombre)])
            };
            f.render_widget(Paragraph::new(l), Rect::new(area.x + 2, area.y + y, CONTENIDO - 3, 1).intersection(area));
        }
        let desde = debajo + 1;
        let zona = Rect::new(area.x + menu::NOMBRE_X, area.y + desde, gestor::ANCHO, tope.saturating_sub(desde)).intersection(area);
        let g = &gestor::GESTORES[gestor::elegido(&self.ctx.datos)];
        gestor::dibujar_bloque(f, zona, g, &self.documentos.mesa(Instant::now()));
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
            let aviso = if self.en_rojo(s) { tema::MAL } else { color };
            if i == self.activa {
                l0.push(Span::styled(format!(" {} ", s.tecla), Style::new().fg(tema::FONDO).bg(color).add_modifier(Modifier::BOLD)));
                l0.push(Span::styled(format!(" {}", s.nombre), negrita.fg(aviso)));
            } else {
                l0.push(Span::styled(s.tecla.to_string(), negrita.fg(aviso)));
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

    /// La barra de abajo, ya dibujada su fila (o sus dos filas): ver `lineas_pie`.
    fn pie(&self, f: &mut Frame, area: Rect, lineas: &[(Line<'static>, Line<'static>)]) {
        // Una barra con su propio fondo y aire dentro, a cada lado: dos columnas (una en terminal estrecho).
        f.buffer_mut().set_style(area, Style::new().bg(tema::BARRA));
        let dentro = dentro_pie(f.area().width);
        let area = Rect { x: area.x + dentro.min(area.width), width: area.width.saturating_sub(2 * dentro), ..area };
        for (k, (izquierda, derecha)) in lineas.iter().enumerate() {
            let r = Rect { y: area.y + k as u16, height: 1, ..area }.intersection(area);
            f.render_widget(Paragraph::new(izquierda.clone()), r);
            f.render_widget(Paragraph::new(derecha.clone().right_aligned()), r);
        }
    }

    /// Lo que dice la barra de abajo, en `ancho` columnas: una fila (o dos), cada una con lo de la izquierda y lo de
    /// la derecha. A la izquierda, cómo se navega («r e a p u sección   1 2 3 pestaña»; solo con el carril: sin él
    /// se ve arriba) y las teclas de la pantalla (y «- + año» si la pantalla usa el año); a la derecha, «G cambiar
    /// gestor   S traer lo último     ? ayuda   q salir» (la G, solo con el carril, donde está el gestor).
    ///
    /// Si no cabe, se quitan primero las palabras «sección» y «pestaña», luego lo de navegar (ya está en el menú), la
    /// G se queda en «G gestor» y «S traer lo último» en «S traer». Si aun así no cabe, en dos filas, en columnas
    /// alineadas (ver `rejilla`), otra vez entero y recortado igual si no cabe; si tampoco, en tres; y si tampoco, de
    /// corrido en dos (la única manera en que se puede quitar la G). Las teclas de la pantalla no se quitan nunca (salvo que tampoco
    /// quepan en dos filas); «? ayuda» y «q salir», tampoco. Escribiendo, solo las de la pantalla (las demás no llegan
    /// a la app), en una fila: ver `teclas_apretadas`.
    fn lineas_pie(&self, carril: bool, ancho: usize) -> Vec<(Line<'static>, Line<'static>)> {
        let p = self.pantalla();
        let mut propias = p.atajos();
        // La S va siempre a la derecha.
        propias.retain(|(k, _)| *k != "S");
        if p.capturando() {
            return vec![(teclas_apretadas(&propias, ancho), Line::default())];
        }
        if p.usa_anio() {
            propias.push(("- +", "año"));
        }
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let gris = tema::apagado();
        let letras: Vec<char> = self.visibles().iter().map(|&i| self.secciones[i].tecla).collect();
        let pestanas = self.secciones[self.activa].pantallas.len();
        // Lo de navegar, en dos trozos (las secciones y las pestañas): 0, entero; 1, sin las palabras («r e a p u» y
        // «1 2 3»); 2, nada.
        let nav_trozos = |nivel: u8| {
            let (mut secciones, mut numeros): (Vec<Span<'static>>, Vec<Span<'static>>) = (Vec::new(), Vec::new());
            if nivel >= 2 {
                return [secciones, numeros];
            }
            for (k, t) in letras.iter().enumerate() {
                if k > 0 {
                    secciones.push(Span::raw(" "));
                }
                secciones.push(Span::styled(t.to_string(), negrita.fg(tema::seccion(*t))));
            }
            if nivel == 0 {
                secciones.push(Span::styled(" sección", gris));
            }
            if pestanas > 1 {
                let n: Vec<String> = (1..=pestanas).map(|i| i.to_string()).collect();
                numeros.push(Span::styled(n.join(" "), negrita));
                if nivel == 0 {
                    numeros.push(Span::styled(" pestaña", gris));
                }
            }
            [secciones, numeros]
        };
        // En una fila: «r e a p u sección   1 2 3 pestaña».
        let nav = |nivel: u8| {
            let [mut v, numeros] = nav_trozos(nivel);
            if !numeros.is_empty() {
                v.push(Span::raw("   "));
                v.extend(numeros);
            }
            v
        };
        // Lo de la derecha; `g`: 0, «G cambiar gestor»; 1, «G gestor»; 2, nada (sin carril no hay gestor a la vista).
        let fijas = |g: u8, s_corta: bool| {
            let mut v = Vec::new();
            if g < 2 {
                v.extend(widgets::atajo(&gestor::TECLA.to_string(), if g == 0 { "cambiar gestor" } else { "gestor" }));
                v.push(Span::raw("   "));
            }
            v.extend(widgets::atajo("S", if s_corta { "traer" } else { "traer lo último" }));
            v.push(Span::raw("     "));
            v.extend(widgets::linea_atajos(&[("?", "ayuda"), ("q", "salir")]).spans);
            Line::from(v)
        };
        // Lo de la izquierda: unas cosas detrás de otras, a cinco espacios.
        let juntas = |trozos: Vec<Vec<Span<'static>>>| {
            let mut v: Vec<Span<'static>> = Vec::new();
            for t in trozos.into_iter().filter(|t| !t.is_empty()) {
                if !v.is_empty() {
                    v.push(Span::raw("     "));
                }
                v.extend(t);
            }
            Line::from(v)
        };
        let teclas = |v: &[(&str, &str)]| widgets::linea_atajos(v).spans;
        // Lo que se recorta, en orden; `None`, si ya no hay nada más que recortar.
        // (La G, solo si no queda otra: en la última manera de ponerlo, la de corrido.)
        let recortar = |(nivel, g, s_corta): (u8, u8, bool), quitar_g: bool| match () {
            _ if nivel < 2 => Some((nivel + 1, g, s_corta)),
            _ if g == 0 => Some((nivel, 1, s_corta)),
            _ if !s_corta => Some((nivel, g, true)),
            _ if g == 1 && quitar_g => Some((nivel, 2, s_corta)),
            _ => None,
        };
        let entero = if carril { (0, 0, false) } else { (2, 2, false) };
        // Entre lo de la izquierda y lo de la derecha, al menos cinco espacios.
        let cabe = |izquierda: &Line, (_, g, s_corta): (u8, u8, bool)| izquierda.width() + 5 + fijas(g, s_corta).width() <= ancho;
        // En una fila.
        let mut r = Some(entero);
        while let Some(c) = r {
            let izquierda = juntas(vec![nav(c.0), teclas(&propias)]);
            if cabe(&izquierda, c) {
                return vec![(izquierda, fijas(c.1, c.2))];
            }
            r = recortar(c, false);
        }
        // En dos filas (y, si no, en tres), en columnas: lo de navegar (las secciones encima de las pestañas) y las
        // teclas de la pantalla, en orden, de arriba abajo; a la derecha, lo de siempre («G» encima de «S», «?» encima
        // de «q»). Si no cabe, se recorta lo mismo y, al final, las columnas van a dos espacios.
        let celda = |(k, d): &(&str, &str)| widgets::atajo(k, d).to_vec();
        let derecha = |g: u8, s_corta: bool| {
            let s = widgets::atajo("S", if s_corta { "traer" } else { "traer lo último" }).to_vec();
            let primera = match g {
                2 => vec![s],
                _ => vec![widgets::atajo(&gestor::TECLA.to_string(), if g == 0 { "cambiar gestor" } else { "gestor" }).to_vec(), s],
            };
            vec![primera, vec![widgets::atajo("?", "ayuda").to_vec(), widgets::atajo("q", "salir").to_vec()]]
        };
        for filas in [2, 3] {
            let (mut c, mut hueco) = (entero, 3);
            loop {
                let [secciones, numeros] = nav_trozos(c.0);
                let mut columnas = if secciones.is_empty() { Vec::new() } else { vec![vec![secciones, numeros]] };
                columnas.extend(propias.chunks(filas).map(|c| c.iter().map(celda).collect()));
                let izquierda = rejilla(&columnas, filas, hueco);
                let la_derecha = rejilla(&derecha(c.1, c.2), filas, hueco);
                if izquierda[0].width() + 5 + la_derecha[0].width() <= ancho {
                    return izquierda.into_iter().zip(la_derecha).collect();
                }
                match recortar(c, false) {
                    Some(otro) => c = otro,
                    None if hueco > 2 => hueco = 2,
                    None => break,
                }
            }
        }
        // Y si tampoco, de corrido: arriba, las de la pantalla que quepan; abajo, las que no, lo de navegar y lo de la
        // derecha.
        let mut arriba = propias;
        let mut abajo = Vec::new();
        while arriba.len() > 1 && widgets::linea_atajos(&arriba).width() > ancho {
            abajo.insert(0, arriba.pop().unwrap_or_default());
        }
        // Con teclas de la pantalla abajo, sin lo de navegar (ya está en el menú).
        let mut c = if abajo.is_empty() { entero } else { (2, entero.1, entero.2) };
        loop {
            let izquierda = juntas(vec![teclas(&abajo), nav(c.0)]);
            if cabe(&izquierda, c) {
                break;
            }
            match recortar(c, true) {
                Some(otro) => c = otro,
                None if abajo.pop().is_some() => {}
                None => break,
            }
        }
        vec![(widgets::linea_atajos(&arriba), Line::default()), (juntas(vec![teclas(&abajo), nav(c.0)]), fijas(c.1, c.2))]
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
            (gestor::TECLA.to_string(), "cambiar de gestor (el de debajo del menú, con su mesa)".into()),
            ("!".into(), "lo que no se ha podido leer o traer: verlo entero y copiarlo".into()),
            ("q".into(), "salir".into()),
        ];
        // Una fila de aire arriba, salvo en un terminal bajo (ahí no cabe todo).
        let mut t = if f.area().height < 25 { Vec::new() } else { vec![Line::from("")] };
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

/// Columnas de celdas (de arriba abajo; las que falten, vacías) en `n` filas, cada columna tan ancha como su celda
/// más ancha y a `hueco` espacios de la siguiente: las filas, igual de anchas (para que, alineadas a la derecha,
/// también cuadren).
fn rejilla(columnas: &[Vec<Vec<Span<'static>>>], n: usize, hueco: usize) -> Vec<Line<'static>> {
    let ancho = |v: &[Span]| v.iter().map(|s| s.width()).sum::<usize>();
    let mut filas: Vec<Vec<Span<'static>>> = vec![Vec::new(); n];
    for (j, col) in columnas.iter().enumerate() {
        let w = col.iter().map(|c| ancho(c)).max().unwrap_or(0);
        for (k, fila) in filas.iter_mut().enumerate() {
            let celda = col.get(k).map(Vec::as_slice).unwrap_or_default();
            if j > 0 {
                fila.push(Span::raw(" ".repeat(hueco)));
            }
            fila.extend(celda.iter().cloned());
            fila.push(Span::raw(" ".repeat(w - ancho(celda))));
        }
    }
    filas.into_iter().map(Line::from).collect()
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
    use super::{gestor, menu, App};
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
            ("3", "Modelos"),
            ("9", "Modelos"),
            ("e", "Sueldo"),
            ("2", "Nóminas"),
            ("r", "Inicio"),
            ("2", "Años"),
            ("3", "Declaraciones"),
            ("u", "Calcula tu sueldo"),
            ("2", "Calcula como autónomo"),
            ("1", "Calcula tu sueldo"),
            ("a", "Modelos"),
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
        // Y abajo, cómo se navega, solo si cabe con las teclas de la pantalla (en Nóminas no: las letras ya están en
        // el menú; ver `el_pie_da_prioridad_a_las_teclas_de_la_pantalla`).
        assert!(filas[34].starts_with("    n nueva") && !filas[34].contains("r e a p u"), "{p}");
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
        for (t, k) in [("Inicio", ""), ("Inicio", "?"), ("Inicio", "!"), ("Inicio", "G"), ("Inicio", "G<right>"), ("Declaraciones", "o"), ("Facturas", "n"), ("Facturas", "n<esc>"), ("Facturas", "c"), ("Gastos", "n")] {
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

    /// En la barra de abajo están siempre todas las teclas de la pantalla (si no caben en una fila, en dos), en todas
    /// las pantallas, en lo que se ve al entrar y al abrir lo primero, de 80 a 140 columnas.
    #[test]
    fn el_pie_no_se_deja_teclas() {
        let mut a = con_empleo();
        for t in titulos(&a) {
            for paso in ["", "<down><enter>"] {
                for (w, h) in [(80, 24), (100, 30), (120, 36), (140, 40)] {
                    ir(&mut a, t);
                    teclas(&mut a, paso);
                    let p = pantalla(&mut a, w, h);
                    let pie: String = p.lines().skip(h as usize - 4).collect::<Vec<_>>().join(" ");
                    if a.pantalla().capturando() {
                        teclas(&mut a, "<esc><esc>");
                        continue;
                    }
                    let mut propias = a.pantalla().atajos();
                    if a.pantalla().usa_anio() {
                        propias.push(("- +", "año"));
                    }
                    for (k, d) in propias.into_iter().filter(|(k, _)| *k != "S") {
                        assert!(pie.contains(&format!("{k} {d}")), "{t} «{paso}» {w}x{h}: falta «{k} {d}»\n{p}");
                    }
                    assert!(pie.contains("? ayuda") && pie.contains("q salir"), "{t} {w}x{h}");
                    teclas(&mut a, "<esc><esc>");
                }
            }
        }
    }

    /// En la barra de abajo, las teclas de la pantalla van antes: si no cabe todo, se quitan primero las palabras de
    /// navegar, luego lo de navegar entero, «G cambiar gestor» se queda en «G gestor» y «S traer lo último» en «S
    /// traer»; si aun así no caben, dos filas (o tres), en columnas alineadas. «? ayuda» y «q salir», siempre; la G,
    /// con carril (sin él no hay gestor a la vista). Escribiendo, en una fila, enteras aunque sea recortando lo que
    /// dicen.
    #[test]
    fn el_pie_da_prioridad_a_las_teclas_de_la_pantalla() {
        let fila = |a: &mut App, w: u16, h: u16, k: usize| pantalla(a, w, h).lines().nth(h as usize - k).unwrap().trim_end().to_string();
        let pie = |a: &mut App, w: u16, h: u16| fila(a, w, h, 2);
        let mut a = app();
        // Con sitio de sobra (Inicio tiene una sola tecla), todo entero, en una fila.
        let p = pie(&mut a, 120, 36);
        assert_eq!(p, format!("    r a p u sección   1 2 3 pestaña     Tab más{}G cambiar gestor   S traer lo último     ? ayuda   q salir", " ".repeat(11)));
        assert!(fila(&mut a, 120, 36, 3).trim().is_empty());
        // Más estrecho, sin las palabras de navegar.
        let p = pie(&mut a, 100, 30);
        assert!(p.starts_with("    r a p u   1 2 3     Tab más") && p.ends_with("G cambiar gestor   S traer lo último     ? ayuda   q salir"), "{p}");
        // Sin carril, sin la G.
        assert!(pie(&mut a, 80, 24).ends_with("    S traer lo último     ? ayuda   q salir"));
        // En Facturas no caben en una: en dos, en columnas, con lo de navegar delante y lo de siempre a la derecha.
        ir(&mut a, "Facturas");
        assert_eq!(
            [fila(&mut a, 120, 36, 3), pie(&mut a, 120, 36)],
            [
                "    r a p u sección   n nueva     c cobrada       f filtro   C clientes   o doc          G cambiar gestor    ? ayuda",
                "    1 2 3 pestaña     Enter ver   ← → trimestre   / buscar   D duplicar   - + año        S traer lo último   q salir",
            ]
        );
        // A 80 columnas, sin lo de navegar, con «S traer» y a dos espacios.
        assert_eq!(
            [fila(&mut a, 80, 24, 3), pie(&mut a, 80, 24)],
            ["  n nueva    c cobrada      f filtro  C clientes  o doc       S traer  ? ayuda", "  Enter ver  ← → trimestre  / buscar  D duplicar  - + año              q salir"]
        );
        // Gastos, a 80 columnas, no cabe en dos: en tres.
        ir(&mut a, "Gastos");
        let p = pantalla(&mut a, 80, 24);
        let filas: Vec<&str> = p.lines().skip(20).take(3).map(str::trim_end).collect();
        assert_eq!(filas[0], "  n nuevo         c categoría   D duplicar         S traer lo último   ? ayuda", "{p}");
        assert_eq!(filas[1], "  Enter ficha     / buscar      d borrar                               q salir", "{p}");
        assert_eq!(filas[2], "  ← → trimestre   o documento   - + año", "{p}");
        // Lo de navegar se va antes que lo de la G (Modelos); y la G se queda en «G gestor» antes que la S (Nóminas,
        // a 100 columnas).
        let mut a = con_empleo();
        teclas(&mut a, "a3");
        let p = pie(&mut a, 120, 36);
        assert!(p.starts_with("    ↑↓ elegir") && p.ends_with("    G gestor   S traer lo último     ? ayuda   q salir"), "{p}");
        // A 100 columnas no cabe en una fila: en dos, sin perder ninguna.
        let p = pantalla(&mut a, 100, 30);
        assert!(["↑↓ elegir", "Enter casillas", "d ver todo", "o PDF", "G cambiar gestor", "S traer lo último", "q salir"].iter().all(|t| p.contains(t)), "{p}");
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

    /// Debajo del menú, tu gestor: su nombre, su dibujo y su mesa («⇣ arrastra aquí» y qué se puede soltar), con
    /// todas las secciones a 120 × 36 y una fila de aire encima de la barra de abajo; la tecla para cambiarlo, en la
    /// barra, con las demás.
    /// Entre el carril y el contenido, dos columnas de aire. Si no cabe, sin el dibujo; si tampoco, nada; sin carril,
    /// nada.
    #[test]
    fn el_gestor_cabe_en_el_carril() {
        use ratatui::style::Color;
        let mut a = con_empleo();
        let (w, h) = (120, 36);
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        term.draw(|f| a.dibujar(f)).unwrap();
        let b = term.backend().buffer().clone();
        let p = pantalla(&mut a, w, h);
        let filas: Vec<&str> = p.lines().collect();
        // Las cinco secciones (de la 4 a la 12), aire, el nombre (14), el dibujo (15 a 27), el tablero (28), el
        // faldón (29 a 31) y las patas (32); la 33 (la de los avisos, libre en el carril), de aire.
        let carril = |k: usize| filas[k].chars().take(super::CONTENIDO as usize).collect::<String>();
        assert!(carril(12).contains("u   Utilidades") && carril(13).trim().is_empty(), "{p}");
        assert!(filas[14].starts_with("  Mari Carmen"), "{p}");
        let fin = 14 + gestor::ALTO_BLOQUE as usize - 1;
        assert_eq!(fin, h as usize - 4, "con una fila de aire encima de la barra de abajo");
        for (k, t) in [(29, "⇣ arrastra aquí"), (30, "facturas, gastos y"), (31, "nóminas (PDF, foto)")] {
            assert!(carril(k).contains(t), "fila {k}: {p}");
        }
        assert!(carril(fin).trim().is_empty(), "las patas, solas: {p}");
        assert!(filas[h as usize - 2].contains("G cambiar gestor"), "{p}");
        // El dibujo: de la columna 2 a la 21, en color; nada del carril en las columnas 22 y 23, ni del contenido.
        let pintadas = (15..28).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| b[(x, y)].symbol() == "▀" && x < super::CONTENIDO);
        assert!(pintadas.clone().all(|(x, _)| (2..22).contains(&x)), "el dibujo, de la 2 a la 21");
        assert!(pintadas.count() > 100, "el dibujo se ve");
        for y in 0..h - 2 {
            for x in super::CONTENIDO - 2..super::CONTENIDO {
                let c = &b[(x, y)];
                assert!(c.symbol() == " " && c.bg == super::tema::FONDO && c.bg != Color::Reset, "aire en ({x}, {y}): {p}");
            }
        }
        // Más bajo: sin el dibujo, con la mesa; más aún, nada. Sin carril, tampoco.
        let p = pantalla(&mut a, 120, 28);
        assert!(p.contains("Mari Carmen") && p.contains("⇣ arrastra aquí") && p.contains("G cambiar gestor"), "{p}");
        let con_medios = p.lines().skip(3).filter(|l| l.chars().take(super::CONTENIDO as usize).any(|c| c == '▀')).count();
        assert_eq!(con_medios, 1, "sin el dibujo, solo el tablero:\n{p}");
        let filas: Vec<String> = p.lines().map(|l| l.chars().take(super::CONTENIDO as usize).collect()).collect();
        let nombre = filas.iter().position(|l| l.contains("Mari Carmen")).unwrap();
        assert!(filas[nombre + 2].contains("arrastra aquí"), "el tablero justo debajo del nombre: {p}");
        assert!(!pantalla(&mut a, 120, 20).contains("arrastra aquí"));
        assert!(!pantalla(&mut a, 80, 24).contains("arrastra aquí"));
    }

    /// `G` abre la ventana para elegir gestor (con los cuatro, el tuyo resaltado y lo suyo debajo): ← → cambia de
    /// uno a otro, Enter lo contrata (se guarda en tus datos y sale en el carril) y Esc lo deja como estaba. Sin
    /// carril, sin dibujos. Escribiendo, la G se escribe.
    #[test]
    fn la_g_elige_gestor() {
        let mut a = app();
        assert_eq!(a.ctx.datos.gestor, "", "de entrada, el de siempre");
        teclas(&mut a, "G");
        let p = pantalla(&mut a, 120, 36);
        for t in ["Elige a tu gestor", "Don Antonio", "Mari Carmen", "Hermanos Martínez", "Doña Remedios", "la de las dos", "★ Sprint de las 13:30", "✗ Persiana a las dos", "Puntualidad", "para irse", "← → elegir", "Enter contratar", "Esc cancelar"] {
            assert!(p.contains(t), "«{t}»: {p}");
        }
        teclas(&mut a, "<right>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("eso lo lleva mi hermano") && p.contains("—¡Que no, que lo llevas tú!") && !p.contains("la de las dos"), "{p}");
        // Sin carril, solo los nombres; y lo de Doña Remedios, que se pasa de la barra.
        teclas(&mut a, "<right>");
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("Doña Remedios") && p.contains("no cabe en la barra") && p.contains("12"), "{p}");
        teclas(&mut a, "<left><enter>");
        assert_eq!(a.ctx.datos.gestor, "hermanos-martinez");
        assert!(aviso(&a).contains("Hermanos Martínez"), "{}", aviso(&a));
        let p = pantalla(&mut a, 120, 36);
        assert!(!p.contains("Elige a tu gestor") && p.lines().nth(12).unwrap().starts_with("  Hermanos Martínez"), "{p}");
        // Esc: se queda el que había.
        teclas(&mut a, "G<right><right><esc>");
        assert_eq!(a.ctx.datos.gestor, "hermanos-martinez");
        assert!(!pantalla(&mut a, 120, 36).contains("Elige a tu gestor"));
        // Escribiendo (buscar en Facturas), la G es una letra más.
        ir(&mut a, "Facturas");
        teclas(&mut a, "/G");
        assert!(a.pantalla().capturando() && !pantalla(&mut a, 120, 36).contains("Elige a tu gestor"));
        // En cualquier tamaño, sin romperse (la ventana y el carril).
        teclas(&mut a, "<esc><esc>G");
        for (w, h) in [(20, 5), (30, 8), (40, 13), (60, 12), (100, 30), (100, 20), (200, 60)] {
            pantalla(&mut a, w, h);
        }
        // Y está en la ayuda.
        teclas(&mut a, "<esc>?");
        assert!(pantalla(&mut a, 120, 36).lines().any(|l| l.contains("G ") && l.contains("cambiar de gestor")));
        assert!(pantalla(&mut a, 80, 24).contains("cambiar de gestor"));
    }

    /// La letra de una sección que no te toca no la abre: se dice. Un número sin pestaña no hace nada.
    #[test]
    fn una_seccion_que_no_sale() {
        let mut a = app();
        teclas(&mut a, "a2e");
        assert_eq!(a.titulo_activa(), "Gastos");
        assert!(aviso(&a).contains("Empleo"), "{}", aviso(&a));
        teclas(&mut a, "u3");
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
            ("Calcula como autónomo", "<enter>"),
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

    /// Ninguna pantalla anuncia abajo una tecla de navegar ni la de elegir gestor: no le llegarían.
    #[test]
    fn ninguna_pantalla_usa_las_teclas_de_navegar() {
        let reservada = |k: &str| k.chars().count() == 1 && k.chars().all(|c| menu::RESERVADAS.contains(c) || c.is_ascii_digit() || c == gestor::TECLA);
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
        assert_eq!(a.titulo_activa(), "Modelos");
        teclas(&mut a, "r2o");
        assert!(aviso(&a).contains("PDF"), "{}", aviso(&a));
        teclas(&mut a, "S");
        assert!(aviso(&a).contains("certificado"), "{}", aviso(&a));
        assert!(!a.salir);
        teclas(&mut a, "q");
        assert!(a.salir);
    }
}
