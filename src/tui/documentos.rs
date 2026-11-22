//! Los documentos que se arrastran al terminal (el terminal pega sus rutas): Claude Haiku los clasifica en segundo
//! plano (ver `clasificar`). Mientras, la ventana «Leyendo tus documentos» enseña la tanda, cada uno con cómo va,
//! por dónde va y un minijuego para la espera (`juego`). Con Esc se oculta: se sigue leyendo (abajo se ve por dónde
//! va) y, al acabar, vuelve a salir. Con Enter, uno a uno, la app enseña lo que ha entendido para guardarlo donde va
//! (Enter) o descartarlo (Esc); con «o» se abre el original. Lo que no se ha podido clasificar, lo que ya estaba y lo
//! que es de un año cerrado también se enseña, con el motivo. Si la función está apagada, se dice cómo encenderla.
use super::fallos::partir;
use super::gestor::Mesa;
use super::juego::{self, Juego};
use super::widgets::{atajo, barra, centrado, tema, ventana, Entrada};
use super::{cortar, Ctx};
use crate::clasificar::{self, Apunte, Entendido, Fichero, Leido, Modo};
use crate::dominio::fmt::{self, eur, fecha_corta, nombre_mes, pct};
use crate::dominio::gasto::gasto_calc;
use crate::dominio::factura::{nombre_operacion, totales_factura};
use crate::informe;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Un documento esperando a que digas qué hacer: lo entendido o por qué no. `fila`: la suya en la lista.
struct Pendiente {
    fila: usize,
    fichero: Option<Fichero>,
    resultado: Result<Entendido, String>,
}

/// Un documento de la tanda: su nombre, de dónde salió (para abrirlo con «o»), su huella (para reconocerlo cuando
/// Claude acaba) y cómo va.
struct Fila {
    nombre: String,
    ruta: Option<PathBuf>,
    huella: Option<String>,
    estado: Estado,
}

enum Estado {
    /// Claude lo está leyendo.
    Leyendo,
    /// Leído y en la cola: cómo ha ido lo dice su `Pendiente`. Se mira al pintar (guardar otro igual lo hace
    /// repetido).
    EnCola,
    /// Ya se sabe: no ha ido a Claude (no se puede leer, ya estaba, va repetido) o ya se ha revisado.
    Hecho(Signo, String),
}

/// El signo de una fila: ✓ en BIEN (entendido), ✗ en MAL (no se ha podido), ✓ en gris (ya está en tus datos) o «–»
/// en gris (repetido, descartado).
#[derive(Clone, Copy)]
enum Signo {
    Bien,
    Mal,
    Ya,
    Fuera,
}

/// Qué ventana se ve.
#[derive(Clone, Copy, PartialEq)]
enum Vista {
    /// «Leyendo tus documentos»: la lista, por dónde va y el juego.
    Leyendo,
    /// Uno a uno, el primero de la cola.
    Revisando,
    /// Ninguna: sin tanda, o con la lista oculta (con Esc) mientras se sigue leyendo.
    Oculta,
}

/// Cómo acaba un documento de la cola.
enum Fin {
    Guardado((&'static str, i32)),
    Descartado,
    SinMeter,
}

/// La ruedecita de lo que se está leyendo: un paso cada `PASO_RUEDA`.
const RUEDA: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
const PASO_RUEDA: Duration = Duration::from_millis(80);

pub struct Documentos {
    /// Cómo se pregunta a Claude (de `PANEL_CLASIFICAR`), o cómo encenderlo.
    pub modo: Result<Modo, String>,
    tx: Sender<Leido>,
    rx: Receiver<Leido>,
    /// Las huellas de los que se están clasificando (para no mandar dos veces el mismo).
    en_marcha: Vec<String>,
    cola: VecDeque<Pendiente>,
    /// Todos los de la tanda, en el orden en que se pegaron.
    lista: Vec<Fila>,
    vista: Vista,
    /// Los euros de un documento en otra moneda, y lo que está mal de lo escrito.
    euros: Entrada,
    error: Option<String>,
    /// Desde que empezó la tanda: cuántos van y dónde ha ido cada uno guardado; descartados y sin clasificar.
    vistos: usize,
    /// Dónde ha ido cada uno guardado, y de qué año es.
    guardados: Vec<(&'static str, i32)>,
    descartados: usize,
    fallidos: usize,
    /// Cuándo salió la ventana: un Enter justo después era para otra cosa (ver `PAUSA`).
    aparecio: Option<Instant>,
    juego: Juego,
    /// El paso de la ruedecita, contado desde `creado` (lo pone `animar`).
    creado: Instant,
    fotograma: usize,
    /// Cuándo se soltaron los últimos que han ido a Claude, y cuántos eran: para la mesa del gestor (`mesa`).
    soltado: Option<(Instant, usize)>,
}

/// Lo que la mesa del gestor dice «¡recibido!» al soltar, antes de ponerse a leer.
const RECIBIDO: Duration = Duration::from_millis(1500);

/// Lo que se ignora Enter cuando sale la ventana sin que la esperes: estabas escribiendo o eligiendo algo y el Enter
/// no era para guardar un documento que aún no has visto.
#[cfg(not(test))]
const PAUSA: Duration = Duration::from_millis(600);
#[cfg(test)]
const PAUSA: Duration = Duration::ZERO;

impl Default for Documentos {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        // La semilla del juego, de la hora: cada vez, otros obstáculos.
        let semilla = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Documentos {
            modo: clasificar::modo(),
            tx,
            rx,
            en_marcha: Vec::new(),
            cola: VecDeque::new(),
            lista: Vec::new(),
            vista: Vista::Oculta,
            euros: Entrada::default(),
            error: None,
            vistos: 0,
            guardados: Vec::new(),
            descartados: 0,
            fallidos: 0,
            aparecio: None,
            juego: Juego::new(semilla),
            creado: Instant::now(),
            fotograma: 0,
            soltado: None,
        }
    }
}

impl Documentos {
    /// ¿Se ve la ventana? (coge todas las teclas)
    pub fn abierto(&self) -> bool {
        self.animando() || self.revisando()
    }

    /// ¿Se ve «Leyendo tus documentos»? (el juego y la ruedecita se mueven: hay que repintar a menudo)
    pub fn animando(&self) -> bool {
        self.vista == Vista::Leyendo && !self.lista.is_empty()
    }

    /// ¿Se está revisando uno?
    pub fn revisando(&self) -> bool {
        self.vista == Vista::Revisando && !self.cola.is_empty()
    }

    /// ¿Se está clasificando alguno?
    pub fn ocupado(&self) -> bool {
        !self.en_marcha.is_empty()
    }

    /// Por dónde va, para la barra de abajo.
    pub fn progreso(&self) -> Option<String> {
        let faltan = self.en_marcha.len();
        let total = self.lista.len().max(faltan);
        match (faltan, total) {
            (0, _) => None,
            (1, 1) => Some("Claude está leyendo un documento…".into()),
            _ => Some(format!("Claude está leyendo tus documentos: {} de {total} leídos…", total - faltan)),
        }
    }

    /// Cómo está la mesa del gestor `ahora`: recién soltados (un momento, `RECIBIDO`), leyéndose o, leídos todos,
    /// ordenados en carpetas mientras se revisan. Sin tanda (o cuando no ha ido nada a Claude: apagada, nada que
    /// leer), esperando; al revisar el último, vuelve a esperar. Si Claude no ha entendido ninguno, no dice «ordenado».
    pub fn mesa(&self, ahora: Instant) -> Mesa {
        let Some((cuando, n)) = self.soltado else { return Mesa::Reposo };
        if self.lista.is_empty() {
            return Mesa::Reposo;
        }
        if ahora.saturating_duration_since(cuando) < RECIBIDO {
            return Mesa::Recibido(n);
        }
        if self.ocupado() {
            let leidos = self.lista.iter().filter(|x| !matches!(x.estado, Estado::Leyendo)).count();
            return Mesa::Leyendo { leidos, total: self.lista.len() };
        }
        // Una carpeta por cada uno entendido, del color de donde va.
        let carpetas: Vec<Color> = self
            .cola
            .iter()
            .filter_map(|p| p.resultado.as_ref().ok())
            .map(|e| if matches!(e.apunte, Apunte::Nomina(_)) { tema::EMPLEO } else { tema::AUTONOMO })
            .collect();
        if carpetas.is_empty() {
            return Mesa::Reposo;
        }
        Mesa::Ordenado { carpetas, por_revisar: self.cola.len() }
    }

    /// Lo que se mueve (el juego, la ruedecita), hasta `ahora`. Lo llama el bucle de la app mientras se ve la lista.
    pub fn animar(&mut self, ahora: Instant) {
        self.juego.avanzar(ahora);
        self.fotograma = (ahora.saturating_duration_since(self.creado).as_millis() / PASO_RUEDA.as_millis()) as usize;
    }

    /// Lo que ha pegado el terminal (las rutas de los ficheros que se han soltado): a la lista de la tanda; los que
    /// valen y no están ya, a clasificar en otro hilo; los demás, a la cola con el motivo. Sale la lista.
    pub fn pegar(&mut self, texto: &str, ctx: &mut Ctx) {
        let (rutas, no) = clasificar::rutas_pegadas(texto);
        if rutas.is_empty() {
            match no.first() {
                Some(n) if n.contains(['/', '\\']) => ctx.error(format!("No encuentro {n}.")),
                _ => ctx.info("Lo pegado no es ningún fichero: arrastra aquí un PDF o una foto."),
            }
            return;
        }
        let nombre_de = |r: &Path| r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.aparecio = Some(Instant::now());
        let modo = match self.modo.clone() {
            Ok(m) => m,
            Err(e) => {
                // Apagada (o mal encendida): se dice en la ventana, que se lee mejor que abajo.
                let nombre = match rutas.as_slice() {
                    [r] => nombre_de(r),
                    v => format!("{} documentos", v.len()),
                };
                self.cola.push_back(Pendiente { fila: self.lista.len(), fichero: None, resultado: Err(e.clone()) });
                self.lista.push(Fila { nombre, ruta: None, huella: None, estado: Estado::Hecho(Signo::Mal, e) });
                self.vista = Vista::Revisando;
                return;
            }
        };
        self.vista = Vista::Leyendo;
        let mut ficheros = Vec::new();
        for r in &rutas {
            let nombre = nombre_de(r);
            let f = match clasificar::leer(r) {
                Ok(f) => f,
                Err(e) => {
                    let corto = sin_punto(e.strip_prefix(&format!("{nombre}: ")).unwrap_or(&e));
                    self.sin_leer(nombre, r, (Signo::Mal, corto), e);
                    continue;
                }
            };
            if let Some(d) = clasificar::ya_metido(&ctx.datos, &f.huella) {
                self.sin_leer(nombre, r, (Signo::Ya, d.clone()), format!("No se mete otra vez: {d}."));
                continue;
            }
            let en_curso = self.en_marcha.contains(&f.huella) || ficheros.iter().any(|x: &Fichero| x.huella == f.huella);
            let en_cola = self.cola.iter().any(|p| p.fichero.as_ref().is_some_and(|x| x.huella == f.huella));
            if en_curso || en_cola {
                self.sin_leer(nombre, r, (Signo::Fuera, "repetido: ya lo estás metiendo".into()), "Ya lo estás metiendo.".into());
                continue;
            }
            self.lista.push(Fila { nombre, ruta: Some(r.clone()), huella: Some(f.huella.clone()), estado: Estado::Leyendo });
            self.en_marcha.push(f.huella.clone());
            ficheros.push(f);
        }
        if ficheros.is_empty() {
            return;
        }
        let n = ficheros.len();
        self.soltado = Some((Instant::now(), rutas.len()));
        ctx.info(if n == 1 { "Claude está leyendo el documento…".to_string() } else { format!("Claude está leyendo {n} documentos…") });
        // En las pruebas no se pregunta a Claude: lo que contesta lo manda la prueba (ver `llega`).
        if cfg!(test) {
            return;
        }
        let (tx, nif, hoy) = (self.tx.clone(), ctx.datos.perfil.nif.clone(), fmt::hoy());
        std::thread::spawn(move || {
            for f in ficheros {
                if tx.send(clasificar::clasificar(&modo, f, &nif, &hoy)).is_err() {
                    break;
                }
            }
        });
    }

    /// Uno que no va a Claude (no se puede leer, ya está o va repetido): a la lista, ya hecho, y a la cola con su
    /// motivo.
    fn sin_leer(&mut self, nombre: String, ruta: &Path, (signo, corto): (Signo, String), motivo: String) {
        self.cola.push_back(Pendiente { fila: self.lista.len(), fichero: None, resultado: Err(motivo) });
        self.lista.push(Fila { nombre, ruta: Some(ruta.to_path_buf()), huella: None, estado: Estado::Hecho(signo, corto) });
    }

    /// Lo pegado mientras se revisa uno: solo al campo de los euros (si lo hay), nunca como teclas de la ventana
    /// (una «o» de una ruta abriría el original).
    pub fn escribir(&mut self, texto: &str) {
        if matches!(self.cola.front(), Some(Pendiente { resultado: Ok(e), .. }) if e.moneda.is_some()) {
            for c in texto.chars().filter(|c| !c.is_control()) {
                self.euros.tecla(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            }
            self.error = None;
        }
    }

    /// Recoge lo que haya terminado. Devuelve true si ha cambiado algo.
    pub fn atender(&mut self) -> bool {
        let mut cambio = false;
        while let Ok(l) = self.rx.try_recv() {
            self.en_marcha.retain(|h| *h != l.fichero.huella);
            let es_este = |f: &Fila| matches!(f.estado, Estado::Leyendo) && f.huella.as_deref() == Some(l.fichero.huella.as_str());
            let fila = match self.lista.iter().position(es_este) {
                Some(i) => i,
                // Sin su fila (en las pruebas, que no pegan nada): al final.
                None => {
                    self.lista.push(Fila { nombre: l.fichero.nombre.clone(), ruta: None, huella: Some(l.fichero.huella.clone()), estado: Estado::Leyendo });
                    self.lista.len() - 1
                }
            };
            self.lista[fila].estado = Estado::EnCola;
            self.cola.push_back(Pendiente { fila, fichero: Some(l.fichero), resultado: l.resultado });
            cambio = true;
        }
        // Con la lista oculta, al acabar de leerlos todos vuelve a salir (sin que la esperes: con la pausa).
        if cambio && self.vista == Vista::Oculta && !self.ocupado() {
            self.vista = Vista::Leyendo;
            self.aparecio = Some(Instant::now());
        }
        cambio
    }

    /// El año de lo entendido, si es uno en que ya no se apuntan facturas ni gastos (las nóminas, en cualquiera).
    fn anio_cerrado(e: &Entendido, ctx: &Ctx) -> Option<i32> {
        let fecha = match &e.apunte {
            Apunte::Gasto(g) => &g.fecha,
            Apunte::Ingreso(f) => &f.fecha,
            Apunte::Nomina(_) => return None,
        };
        let anio = fmt::anio_de(fecha);
        (!informe::editable(&ctx.datos, anio)).then_some(anio)
    }

    /// Lo que impide guardar el primero de la cola, aunque se haya entendido: que ya lo tengas o que sea de un año
    /// en que ya no se apuntan facturas ni gastos.
    fn impide(e: &Entendido, ctx: &Ctx) -> Option<String> {
        if let Some(r) = clasificar::repetido(&ctx.datos, e) {
            return Some(r);
        }
        Self::anio_cerrado(e, ctx).map(|anio| format!("Es de {anio}: las facturas y los gastos solo se apuntan en el año en curso (y en el anterior mientras no tenga Renta)."))
    }

    /// Lo que ha contestado Claude, para la lista: entendido (en corto), repetido, de un año cerrado o por qué no.
    fn en_corto(r: &Result<Entendido, String>, ctx: &Ctx) -> (Signo, String) {
        match r {
            Err(m) => (Signo::Mal, sin_punto(m)),
            Ok(e) => match (clasificar::repetido(&ctx.datos, e), Self::anio_cerrado(e, ctx)) {
                (Some(r), _) => (Signo::Fuera, sin_punto(&r)),
                (None, Some(anio)) => (Signo::Mal, format!("es de {anio}, un año cerrado")),
                (None, None) => (Signo::Bien, resumen(e, ctx)),
            },
        }
    }

    /// Cómo va la fila `i`: su signo y lo entendido en corto o el motivo (nada si se está leyendo).
    fn como_va(&self, i: usize, ctx: &Ctx) -> Option<(Signo, String)> {
        match &self.lista[i].estado {
            Estado::Leyendo => None,
            Estado::Hecho(s, t) => Some((*s, t.clone())),
            Estado::EnCola => Some(self.cola.iter().find(|p| p.fila == i).map_or((Signo::Fuera, String::new()), |p| Self::en_corto(&p.resultado, ctx))),
        }
    }

    pub fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) {
        if k.code == KeyCode::Enter && self.aparecio.is_some_and(|t| t.elapsed() < PAUSA) {
            return;
        }
        match self.vista {
            Vista::Leyendo => match k.code {
                KeyCode::Char(' ') => self.juego.saltar(Instant::now()),
                // Los ya leídos, uno a uno; los que faltan se suman a la cola según llegan.
                KeyCode::Enter if !self.cola.is_empty() => {
                    self.vista = Vista::Revisando;
                    self.aparecio = Some(Instant::now());
                }
                // Se sigue leyendo en segundo plano y, al acabar, vuelve a salir. Leídos todos, espera al Enter.
                KeyCode::Esc if self.ocupado() => self.vista = Vista::Oculta,
                _ => {}
            },
            Vista::Revisando => self.revisar(k, ctx),
            Vista::Oculta => {}
        }
    }

    /// Las teclas del que se está revisando.
    fn revisar(&mut self, k: KeyEvent, ctx: &mut Ctx) {
        let Some(p) = self.cola.front() else { return };
        let fila = p.fila;
        if k.code == KeyCode::Char('o') {
            self.abrir(fila, ctx);
            return;
        }
        let (enter, esc) = (k.code == KeyCode::Enter, k.code == KeyCode::Esc);
        let se_puede = matches!(&p.resultado, Ok(e) if Self::impide(e, ctx).is_none());
        let en_otra_moneda = matches!(&p.resultado, Ok(e) if e.moneda.is_some());
        if !se_puede {
            // Lo que no se puede guardar: Enter o Esc, y al siguiente.
            if enter || esc {
                self.fin(ctx, Fin::SinMeter, fila);
            }
        } else if esc {
            self.fin(ctx, Fin::Descartado, fila);
        } else if enter {
            match self.guardar_primero(ctx) {
                Ok(sitio) => self.fin(ctx, Fin::Guardado(sitio), fila),
                Err(m) => self.error = Some(m),
            }
        } else if en_otra_moneda && self.euros.tecla(k) {
            self.error = None;
        }
    }

    /// «o»: el original, con el visor del sistema (también si no se ha podido meter: para ver qué era).
    fn abrir(&self, fila: usize, ctx: &mut Ctx) {
        match self.lista.get(fila).and_then(|f| f.ruta.as_ref()) {
            Some(r) if r.is_file() => ctx.abrir_ruta(r),
            Some(r) => ctx.error(format!("No encuentro {} (¿lo has movido o borrado?).", r.display())),
            None => {}
        }
    }

    /// Guarda el primero de la cola (en euros, si estaba en otra moneda), con la copia del documento en tu carpeta.
    /// Devuelve dónde ha ido.
    fn guardar_primero(&mut self, ctx: &mut Ctx) -> Result<(&'static str, i32), String> {
        let Some(Pendiente { fichero, resultado: Ok(e), .. }) = self.cola.front_mut() else { return Err("No hay nada que guardar.".into()) };
        if e.moneda.is_some() {
            let t = self.euros.valor.trim();
            if t.is_empty() || !fmt::es_num(t) {
                return Err("Escribe cuántos euros fueron.".into());
            }
            e.en_euros(fmt::parse_num(t))?;
        }
        // En las pruebas no se escribe nada en disco.
        let doc = match (fichero.as_ref(), ctx.sin_disco) {
            (Some(f), true) => clasificar::nombre_copia(f),
            (Some(f), false) => clasificar::guardar_copia(&ctx.dir, f).map_err(|e| format!("No se ha podido copiar el documento a documentos/: {e}"))?,
            (None, _) => String::new(),
        };
        let Some(Pendiente { resultado: Ok(e), .. }) = self.cola.pop_front() else { return Err("No hay nada que guardar.".into()) };
        let anio = match &e.apunte {
            Apunte::Nomina(n) => n.anio,
            Apunte::Gasto(g) => fmt::anio_de(&g.fecha),
            Apunte::Ingreso(f) => fmt::anio_de(&f.fecha),
        };
        let sitio = clasificar::guardar(&mut ctx.datos, e, doc);
        ctx.guardar();
        Ok((sitio, anio))
    }

    /// Quita el primero de la cola (si sigue ahí) y apunta en su fila cómo ha acabado. Si faltan por leer, otra vez
    /// la lista; si era el último, dice cómo ha ido la tanda.
    fn fin(&mut self, ctx: &mut Ctx, fin: Fin, fila: usize) {
        let hecho = match fin {
            Fin::Guardado(s) => {
                self.guardados.push(s);
                Some((Signo::Ya, format!("guardado en {}", s.0)))
            }
            Fin::Descartado => {
                self.cola.pop_front();
                self.descartados += 1;
                Some((Signo::Fuera, "descartado".to_string()))
            }
            Fin::SinMeter => {
                // Lo leído se queda con lo que lo ha impedido (si era repetido, ahora se ve).
                let ahora = match (self.lista.get(fila).map(|f| &f.estado), self.cola.front()) {
                    (Some(Estado::EnCola), Some(p)) => Some(Self::en_corto(&p.resultado, ctx)),
                    _ => None,
                };
                self.cola.pop_front();
                self.fallidos += 1;
                ahora
            }
        };
        if let (Some((s, t)), Some(f)) = (hecho, self.lista.get_mut(fila)) {
            f.estado = Estado::Hecho(s, t);
        }
        self.vistos += 1;
        self.euros = Entrada::default();
        self.error = None;
        if !self.cola.is_empty() {
            return;
        }
        if self.ocupado() {
            // Faltan por leer: otra vez la lista (y el juego) hasta que lleguen.
            self.vista = Vista::Leyendo;
            return;
        }
        let plural = |n: usize, una: &str, varias: &str| if n == 1 { format!("1 {una}") } else { format!("{n} {varias}") };
        let mut v = Vec::new();
        if !self.guardados.is_empty() {
            let mut sitios: Vec<&str> = self.guardados.iter().map(|g| g.0).collect();
            sitios.sort();
            sitios.dedup();
            // Los de otro año no se ven en el que tienes elegido: se dice cuál.
            let mut anios: Vec<String> = self.guardados.iter().map(|g| g.1).filter(|a| *a != ctx.anio).map(|a| a.to_string()).collect();
            anios.sort();
            anios.dedup();
            let otro = if anios.is_empty() { String::new() } else { format!("; de {}: cambia el año con - +", anios.join(" y ")) };
            v.push(format!("{} (en {}{otro})", plural(self.guardados.len(), "guardado", "guardados"), sitios.join(" y ")));
        }
        if self.descartados > 0 {
            v.push(plural(self.descartados, "descartado", "descartados"));
        }
        if self.fallidos > 0 {
            v.push(format!("{} sin meter", self.fallidos));
        }
        let txt = format!("Documentos: {}.", v.join(", "));
        if self.guardados.is_empty() { ctx.info(txt) } else { ctx.ok(txt) }
        // La tanda siguiente empieza de cero (el récord del juego se queda).
        (self.vistos, self.guardados, self.descartados, self.fallidos) = (0, Vec::new(), 0, 0);
        self.lista.clear();
        self.soltado = None;
        self.vista = Vista::Oculta;
        self.juego.parar();
    }

    /// La ventana que toque: la lista mientras se leen o uno a uno.
    pub fn dibujar(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if self.animando() {
            self.dibujar_lista(f, area, ctx);
        } else if self.revisando() {
            self.dibujar_uno(f, area, ctx);
        }
    }

    /// «Leyendo tus documentos»: la tanda, uno por fila con cómo va; por dónde va, con una barra fina; el juego y
    /// las teclas. Primero se guarda sitio para lo fijo y la lista va en lo que quede (si no cabe, «… y N más»); si
    /// no queda ni para una fila con el juego, va sin juego. Estilo C2: sin cajas, el signo en su color, el nombre
    /// en TINTA y lo demás en gris.
    fn dibujar_lista(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let ancho = 96.min(area.width);
        let util = ancho.saturating_sub(4) as usize;
        // Lo fijo: aire, aire, el progreso, aire y las teclas; con el juego, él y otra fila de aire (si con los bordes
        // queda sitio para una fila de la lista).
        let con_juego = area.height > 6 + juego::ALTO + 2;
        let fijo = if con_juego { 6 + juego::ALTO } else { 5 };
        let total = self.lista.len();
        let n = total.min((area.height.saturating_sub(fijo + 2) as usize).max(1));
        let r = centrado(area, ancho, n as u16 + fijo + 2);
        let dentro = ventana(f, r, "Leyendo tus documentos");
        // Si no caben todos, los que se están leyendo a la vista.
        let foco = self.lista.iter().position(|x| matches!(x.estado, Estado::Leyendo)).unwrap_or(0);
        let desde = foco.saturating_sub(n / 3).min(total - n);
        let col = self.lista[desde..desde + n].iter().map(|x| x.nombre.chars().count()).max().unwrap_or(0).clamp(8, (util * 2 / 5).max(8));
        let mut t = vec![Line::from("")];
        for i in desde..desde + n {
            if total > n && i == desde + n - 1 {
                t.push(Line::styled(format!("… y {} más", total - n + 1), gris));
                break;
            }
            // El signo en su color; lo que ya está o se queda fuera, entero en gris.
            let (signo, color, texto) = match self.como_va(i, ctx) {
                None => (RUEDA.chars().nth(self.fotograma % RUEDA.chars().count()).unwrap_or('·'), tema::acento(), String::new()),
                Some((Signo::Bien, texto)) => ('✓', tema::BIEN, texto),
                Some((Signo::Mal, texto)) => ('✗', tema::MAL, texto),
                Some((Signo::Ya, texto)) => ('✓', tema::GRIS, texto),
                Some((Signo::Fuera, texto)) => ('–', tema::GRIS, texto),
            };
            let nombre = if color == tema::GRIS { gris } else { Style::new() };
            t.push(Line::from(vec![
                Span::styled(signo.to_string(), negrita.fg(color)),
                Span::raw("  "),
                Span::styled(format!("{:<col$}", cortar(&self.lista[i].nombre, col)), nombre),
                Span::raw("   "),
                Span::styled(cortar(&texto, util.saturating_sub(col + 6)), gris),
            ]));
        }
        t.push(Line::from(""));
        let leidos = self.lista.iter().filter(|x| !matches!(x.estado, Estado::Leyendo)).count();
        t.push(if self.ocupado() {
            let txt = format!("{leidos} de {total} {}", if total == 1 { "leído" } else { "leídos" });
            let largo = util.saturating_sub(txt.chars().count() + 3) as u16;
            let mut v = vec![Span::raw(txt), Span::raw("   ")];
            v.extend(barra(&[(leidos as f64, tema::acento()), ((total - leidos) as f64, tema::MARCO)], largo).spans);
            Line::from(v)
        } else {
            let (que, cuales) = if total == 1 { ("✓ leído", "revisarlo") } else { ("✓ todos leídos", "revisarlos") };
            Line::from(vec![Span::styled(que, negrita.fg(tema::BIEN)), Span::styled(" · ", gris), Span::styled("Enter", negrita), Span::styled(format!(" {cuales}"), negrita.fg(tema::acento()))])
        });
        f.render_widget(Paragraph::new(t), dentro);
        if con_juego {
            let zona = Rect { y: dentro.y + n as u16 + 4, height: juego::ALTO, ..dentro };
            if zona.bottom() < dentro.bottom() {
                self.juego.dibujar(f.buffer_mut(), zona);
            }
        }
        // Las teclas, sin botoncitos: la que sigue, con lo que hace en el color de la sección.
        let mut teclas = Vec::new();
        let mut poner = |v: Vec<Span<'static>>| {
            if !teclas.is_empty() {
                teclas.push(Span::raw("   "));
            }
            teclas.extend(v);
        };
        if con_juego {
            poner(atajo("Espacio", "saltar").to_vec());
        }
        if !self.cola.is_empty() {
            poner(vec![Span::styled("Enter", negrita), Span::styled(" revisar", negrita.fg(tema::acento()))]);
        }
        if self.ocupado() {
            poner(atajo("Esc", "ocultar").to_vec());
        }
        if dentro.height > 0 {
            f.render_widget(Paragraph::new(Line::from(teclas)), Rect { y: dentro.bottom() - 1, height: 1, ..dentro });
        }
    }

    /// La ventana con el primero de la cola: arriba, lo entendido (si no cabe, se corta por abajo); abajo, siempre a
    /// la vista, los euros (si hace falta) y las teclas. Estilo C2: los nombres de los campos en gris, la cifra que
    /// cuenta en negrita, los avisos en AVISO (lo partido, con sangría) y lo que impide guardarlo en MAL.
    fn dibujar_uno(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let Some(p) = self.cola.front() else { return };
        let fila = self.lista.get(p.fila);
        let titulo = format!("Documento {} de {} · {}", self.vistos + 1, self.lista.len(), fila.map_or("", |x| x.nombre.as_str()));
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let mal = Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD);
        let ancho = 84.min(area.width);
        let util = ancho.saturating_sub(4);
        let mut t: Vec<Line> = Vec::new();
        let mut pie: Vec<Line> = Vec::new();
        // La fila del campo de los euros dentro de `pie`, si hace falta.
        let mut fila_euros = None;
        let impide = p.resultado.as_ref().ok().and_then(|e| Self::impide(e, ctx));
        match &p.resultado {
            Err(motivo) => {
                t.push(Line::styled("No se ha podido meter", mal));
                t.push(Line::from(""));
                t.push(Line::from(motivo.clone()));
            }
            Ok(e) => {
                let (que, donde) = match &e.apunte {
                    Apunte::Nomina(_) => ("una nómina", "Empleo › Nóminas"),
                    Apunte::Gasto(_) => ("un gasto", "Autónomo › Gastos"),
                    Apunte::Ingreso(_) => ("un ingreso", "Autónomo › Facturas"),
                };
                t.push(Line::from(vec![Span::raw("Es "), Span::styled(que, negrita), Span::styled(format!(" ({})", e.que), gris)]));
                t.push(Line::from(vec![Span::raw("Va a "), Span::styled(donde, negrita.fg(tema::acento())), Span::raw(", así:")]));
                t.push(Line::from(""));
                let m = e.moneda.as_ref().map(|(m, _)| m.as_str());
                // La cifra que cuenta (lo pagado, el total, el bruto), en negrita.
                let principal = match &e.apunte {
                    Apunte::Nomina(_) => "Bruto",
                    Apunte::Gasto(_) => "Pagado",
                    Apunte::Ingreso(_) => "Total",
                };
                for (k, v) in campos(e, m, ctx) {
                    let st = if k == principal { negrita } else { Style::new() };
                    t.push(Line::from(vec![Span::styled(format!("  {k:<14}"), gris), Span::styled(v, st)]));
                }
                let mut avisos = e.avisos.clone();
                // Lo que pedirá Facturas para emitirla (el número fiscal de un cliente extranjero…).
                if let Apunte::Ingreso(fa) = &e.apunte
                    && !fa.emitida()
                {
                    avisos.extend(super::facturas::problemas_cliente(fa).into_iter().map(|x| format!("Para emitirla: {x}")));
                }
                if !avisos.is_empty() {
                    t.push(Line::from(""));
                }
                for a in avisos {
                    t.extend(partir(&[(format!("· {a}"), Style::new().fg(tema::AVISO))], util as usize, 2));
                }
                if let Some(x) = &impide {
                    t.push(Line::from(""));
                    t.push(Line::styled(format!("No se guarda: {x}"), mal));
                } else if let Some((m, total)) = &e.moneda {
                    pie.push(Line::from(""));
                    pie.push(Line::from(format!("Está en {m} ({} {m}). ¿Cuántos euros fueron? (lo que te cobró o pagó el banco)", fmt::num(*total))));
                    fila_euros = Some(pie.len());
                    pie.push(Line::from(vec![Span::styled("Euros: ", negrita)]));
                }
                if let Some(err) = &self.error {
                    pie.push(Line::styled(err.clone(), mal));
                }
            }
        }
        // Las teclas, sin botoncitos: la que guarda, con lo que hace en el color de la sección (como al confirmar);
        // y «o» para ver el original, si se sabe de dónde salió.
        let mut teclas = match (&p.resultado, &impide) {
            (Ok(_), None) => {
                let mut v = vec![Span::styled("Enter", negrita), Span::styled(" guardar", negrita.fg(tema::acento())), Span::raw("   ")];
                v.extend(atajo("Esc", "descartar"));
                v
            }
            _ => atajo("Enter", "seguir").to_vec(),
        };
        if fila.is_some_and(|x| x.ruta.is_some()) {
            teclas.push(Span::raw("   "));
            teclas.extend(atajo("o", "abrir"));
        }
        pie.push(Line::from(""));
        pie.push(Line::from(teclas));
        let (alto_t, alto_pie) = (filas(&t, util), filas(&pie, util));
        let r = centrado(area, ancho, (alto_t + alto_pie + 2).min(area.height));
        let dentro = ventana(f, r, &titulo);
        let alto_pie = alto_pie.min(dentro.height);
        let abajo = Rect { y: dentro.bottom() - alto_pie, height: alto_pie, ..dentro };
        let arriba = Rect { height: dentro.height - alto_pie, ..dentro };
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), arriba);
        let fila_euros = fila_euros.map(|i| filas(&pie[..i], util));
        f.render_widget(Paragraph::new(pie).wrap(Wrap { trim: false }), abajo);
        if let Some(y) = fila_euros.filter(|y| *y < abajo.height) {
            // El campo, como en los formularios: sobre el fondo suave de la sección y en negrita.
            let campo = Rect { x: abajo.x + 7, y: abajo.y + y, width: 18.min(abajo.width.saturating_sub(7)), height: 1 };
            f.buffer_mut().set_style(campo, Style::new().bg(tema::suave(tema::acento())));
            let dentro = Rect { x: campo.x + 1, width: campo.width.saturating_sub(1), ..campo };
            self.euros.dibujar(f, dentro, true, negrita);
        }
    }
}

/// Un motivo sin el punto final, para la lista.
fn sin_punto(s: &str) -> String {
    s.trim().trim_end_matches('.').to_string()
}

/// Lo entendido en corto, para la lista: «gasto · Repsol · 42,10 €», «nómina de octubre · Empresa», «factura
/// emitida · Cliente · 1.210,00 €». El importe, en su moneda si no es el euro.
fn resumen(e: &Entendido, ctx: &Ctx) -> String {
    let importe = |x: f64| match &e.moneda {
        Some((m, total)) => format!("{} {m}", fmt::num(*total)),
        None => eur(x),
    };
    let partes: Vec<String> = match &e.apunte {
        Apunte::Gasto(g) => vec!["gasto".into(), g.proveedor.clone(), importe(gasto_calc(g, &ctx.datos).pagado)],
        Apunte::Nomina(n) => {
            let mes = fmt::MESES.get((n.mes as usize).wrapping_sub(1)).unwrap_or(&"?");
            let anio = if n.anio == fmt::anio_actual() { String::new() } else { format!(" de {}", n.anio) };
            vec![format!("nómina de {mes}{anio}"), n.empresa.clone()]
        }
        Apunte::Ingreso(f) => vec![if f.emitida() { "factura emitida" } else { "factura en borrador" }.into(), f.cliente.nombre.clone(), importe(totales_factura(f).total)],
    };
    partes.into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join(" · ")
}

/// Las filas que ocupan unas líneas en `ancho` columnas, partiéndolas por las palabras (como `Wrap`).
fn filas(lineas: &[Line], ancho: u16) -> u16 {
    let ancho = ancho.max(1) as usize;
    let mut n = 0;
    for l in lineas {
        let texto: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        let (mut fila, mut largo) = (1, 0);
        for p in texto.split(' ') {
            let w = p.chars().count();
            let con = if largo == 0 { w } else { largo + 1 + w };
            if con <= ancho {
                largo = con;
            } else {
                fila += 1;
                largo = w % ancho;
                fila += w / ancho;
            }
        }
        n += fila;
    }
    n as u16
}

/// Lo entendido, campo a campo (los importes, en su moneda si no es el euro).
fn campos(e: &Entendido, moneda: Option<&str>, ctx: &Ctx) -> Vec<(&'static str, String)> {
    let dinero = |x: f64| match moneda {
        Some(m) => format!("{} {m}", fmt::num(x)),
        None => eur(x),
    };
    let con_nif = |nombre: &str, nif: &str| if nif.is_empty() { nombre.to_string() } else { format!("{nombre} ({nif})") };
    match &e.apunte {
        Apunte::Nomina(n) => vec![
            ("Empresa", n.empresa.clone()),
            ("Mes", nombre_mes(&format!("{}-{:02}", n.anio, n.mes))),
            ("Bruto", eur(n.dinerario)),
            ("En especie", eur(n.especie)),
            ("IRPF", eur(n.retenciones)),
            ("Seg. Social", eur(n.ss)),
        ],
        Apunte::Gasto(g) => {
            let c = gasto_calc(g, &ctx.datos);
            let iva = match c.origen.as_str() {
                "es" => format!("{} al {} · te deduces {}", dinero(c.cuota), pct(c.tipo), dinero(c.iva_ded)),
                "ue" | "extra" => format!("lo autoliquidas tú al {} ({}), y te lo deduces", pct(c.tipo), dinero(c.cuota)),
                _ => "sin IVA".into(),
            };
            vec![
                ("Proveedor", con_nif(&g.proveedor, &g.nif)),
                ("Fecha", fecha_corta(&g.fecha)),
                ("Concepto", g.concepto.clone()),
                ("Categoría", clasificar::categoria(g).to_string()),
                ("Base", dinero(c.base)),
                ("IVA", iva),
                ("Pagado", dinero(c.pagado)),
            ]
        }
        Apunte::Ingreso(f) => {
            let t = totales_factura(f);
            let estado = if f.emitida() { format!("emitida, n.º {}", f.numero) } else { "borrador".into() };
            let cobrada = f.cobrada.as_deref().map(|c| format!(" · cobrada el {}", fecha_corta(c))).unwrap_or_default();
            vec![
                ("Cliente", con_nif(&f.cliente.nombre, &f.cliente.nif) + &if f.cliente.pais.is_empty() { String::new() } else { format!(" · {}", f.cliente.pais) }),
                ("Fecha", format!("{}{cobrada}", fecha_corta(&f.fecha))),
                ("Concepto", f.lineas.first().map(|l| l.concepto.clone()).unwrap_or_default()),
                ("Operación", nombre_operacion(&f.operacion).to_string()),
                ("Base", dinero(t.base)),
                ("IVA", dinero(t.cuota)),
                ("Retención", format!("{} ({})", dinero(t.retencion), pct(f.retencion.v()))),
                ("Total", dinero(t.total)),
                ("Factura", estado),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::{Cliente, Factura, Gasto, Linea, Nomina, Num, PRIMER_ANIO};
    use crate::tui::prueba::{app, ir, pantalla, teclas};
    use crate::tui::{App, Nivel};

    fn aviso(a: &App) -> String {
        a.ctx.aviso.as_ref().map(|(t, ..)| t.clone()).unwrap_or_default()
    }

    /// Un documento inventado (no se lee de ningún sitio).
    fn fichero(nombre: &str, huella: &str) -> Fichero {
        Fichero { nombre: nombre.into(), tipo: "application/pdf", bytes: b"%PDF inventado".to_vec(), huella: huella.into() }
    }

    /// Como si Claude hubiera terminado con este documento (sin haberlo pegado: sale la lista, ya leída).
    fn llega(a: &mut App, nombre: &str, huella: &str, r: Result<Entendido, String>) {
        a.documentos.tx.send(Leido { fichero: fichero(nombre, huella), resultado: r, coste: 0.001 }).unwrap();
        a.atender();
    }

    /// Unos ficheros inventados en una carpeta temporal nueva, para pegarlos como si se arrastraran.
    fn ficheros(v: &[(&str, &[u8])]) -> (PathBuf, Vec<PathBuf>) {
        let dir = std::env::temp_dir().join(format!("gesshtor-doc-{}", crate::dominio::uid()));
        std::fs::create_dir_all(&dir).unwrap();
        let rutas = v.iter().map(|(n, b)| {
            std::fs::write(dir.join(n), b).unwrap();
            dir.join(n)
        });
        let rutas = rutas.collect();
        (dir, rutas)
    }

    /// Los pega con la función encendida. En las pruebas no se pregunta a Claude: lo que contesta, con `llega_el`.
    fn pegar(a: &mut App, rutas: &[PathBuf]) {
        a.documentos.modo = Ok(Modo::Claude);
        a.pegar(&rutas.iter().map(|r| format!("'{}'", r.display())).collect::<Vec<_>>().join(" "));
    }

    /// Como si Claude hubiera terminado con este fichero, ya pegado.
    fn llega_el(a: &mut App, ruta: &Path, r: Result<Entendido, String>) {
        a.documentos.tx.send(Leido { fichero: clasificar::leer(ruta).unwrap(), resultado: r, coste: 0.001 }).unwrap();
        a.atender();
    }

    /// La línea de la pantalla con ese texto.
    fn fila(p: &str, texto: &str) -> String {
        p.lines().find(|l| l.contains(texto)).unwrap_or_default().to_string()
    }

    fn gasto_de(proveedor: &str, base: f64, fecha: &str) -> Entendido {
        let g = Gasto { id: "g-nuevo".into(), proveedor: proveedor.into(), concepto: "Folios".into(), categoria: "material".into(), origen: "es".into(), base: Num::de(base), iva_tipo: Num::de(21.0), fecha: fecha.into(), ..Default::default() };
        Entendido { apunte: Apunte::Gasto(g), que: "factura de papelería".into(), moneda: None, avisos: vec!["Un aviso inventado.".into()] }
    }

    fn gasto(fecha: &str) -> Entendido {
        gasto_de("Papelería Inventada", 100.0, fecha)
    }

    /// Apagada: se dice cómo encenderla; lo que no es un fichero, también se dice.
    #[test]
    fn apagada_se_dice_como_encenderla() {
        let mut a = app();
        a.documentos.modo = Err(clasificar::COMO_ENCENDER.into());
        let p = std::env::temp_dir().join(format!("gesshtor-doc-{}.pdf", crate::dominio::uid()));
        std::fs::write(&p, b"%PDF").unwrap();
        a.pegar(&p.to_string_lossy());
        assert!(!a.documentos.ocupado());
        let pant = pantalla(&mut a, 80, 24);
        assert!(pant.contains("PANEL_CLASIFICAR=claude") && pant.contains("ANTHROPIC_API_KEY") && pant.contains("seguir"), "{pant}");
        assert!(!pant.contains("Leyendo tus documentos") && !pant.contains("o abrir"), "{pant}");
        teclas(&mut a, "<enter>");
        assert!(!a.documentos.abierto());
        a.pegar("hola, esto es texto");
        assert!(aviso(&a).contains("no es ningún fichero"), "{}", aviso(&a));
        a.pegar("/no/existe/nada.pdf");
        assert!(aviso(&a).contains("No encuentro"), "{}", aviso(&a));
        std::fs::remove_file(p).unwrap();
    }

    /// Escribiendo en un formulario, lo pegado se escribe (sin saltos de línea), como siempre.
    #[test]
    fn escribiendo_se_escribe() {
        let mut a = app();
        ir(&mut a, "Gastos");
        teclas(&mut a, "n");
        assert!(a.pantalla().capturando());
        a.pegar("Taxi al\naeropuerto");
        assert!(pantalla(&mut a, 120, 36).contains("Taxi alaeropuerto"));
        assert!(!a.documentos.abierto());
    }

    /// Al pegar sale la lista: cada uno con su ruedecita mientras se lee y, según acaba Claude, con lo entendido en
    /// corto o por qué no; lo que no se puede leer, ya con el motivo. Debajo, por dónde va y el juego. Lo que se pega
    /// con la lista a la vista se suma a la tanda. Leídos todos, lo dice y espera al Enter.
    #[test]
    fn al_pegar_sale_la_lista() {
        let mut a = app();
        ir(&mut a, "Facturas");
        assert_eq!(a.espera(), 500);
        let (dir, v) = ficheros(&[("tique.pdf", b"%PDF tique"), ("nomina.pdf", b"%PDF nomina"), ("nota.txt", b"hola")]);
        pegar(&mut a, &v[..2]);
        assert!(a.documentos.abierto() && a.documentos.ocupado());
        assert_eq!(a.espera(), 40, "con la lista a la vista, unas 25 veces por segundo");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Leyendo tus documentos") && p.contains("0 de 2 leídos") && !p.contains("Enter revisar"), "{p}");
        teclas(&mut a, "<enter>");
        assert!(a.documentos.animando(), "sin nada leído, Enter no hace nada");
        // Lo pegado con la lista a la vista, a la tanda (no como teclas: los espacios serían saltos).
        a.pegar(&format!("'{}'", v[2].display()));
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("1 de 3 leídos") && p.contains("▄▄▄"), "{p}");
        let rueda = |l: String| l.chars().any(|c| RUEDA.contains(c));
        assert!(rueda(fila(&p, "tique.pdf")) && rueda(fila(&p, "nomina.pdf")), "{p}");
        assert!(fila(&p, "nota.txt").contains("✗") && fila(&p, "nota.txt").contains("solo se pueden leer PDF y fotos"), "{p}");
        assert!(p.contains("0 saltados · récord 0") && p.contains("▁▁▁▁▁▁") && p.contains("Espacio saltar") && p.contains("Esc ocultar"), "{p}");
        // Lo que no se puede leer ya se puede revisar.
        assert!(p.contains("Enter revisar"), "{p}");
        llega_el(&mut a, &v[0], Ok(gasto(&fmt::hoy())));
        let p = pantalla(&mut a, 120, 36);
        assert!(fila(&p, "tique.pdf").contains("✓") && fila(&p, "tique.pdf").contains("gasto · Papelería Inventada · 121,00 €") && p.contains("2 de 3 leídos"), "{p}");
        assert!(p.contains("Enter revisar") && p.contains("Esc ocultar"), "{p}");
        llega_el(&mut a, &v[1], Err("No cuadra: base y total.".into()));
        let p = pantalla(&mut a, 120, 36);
        assert!(fila(&p, "nomina.pdf").contains("✗") && fila(&p, "nomina.pdf").contains("No cuadra: base y total"), "{p}");
        assert!(p.contains("✓ todos leídos · Enter revisarlos") && !p.contains("Esc ocultar"), "{p}");
        // No pasa sola a la revisión, ni Esc la oculta ya: espera al Enter.
        teclas(&mut a, "<esc>");
        assert!(a.documentos.animando() && pantalla(&mut a, 120, 36).contains("Leyendo tus documentos"));
        // A 80×24 también se ve todo.
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("Leyendo tus documentos") && p.contains("tique.pdf") && p.contains("todos leídos") && p.contains("saltados") && p.contains("Espacio saltar"), "{p}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Espacio salta en el juego y no llega a la pantalla de debajo (en un trimestre de Impuestos baja la página).
    #[test]
    fn espacio_salta_y_no_llega_a_la_pantalla() {
        let mut b = app();
        ir(&mut b, "Impuestos");
        teclas(&mut b, "<left>");
        let antes = pantalla(&mut b, 120, 36);
        teclas(&mut b, "<space>");
        assert_ne!(antes, pantalla(&mut b, 120, 36), "sin la ventana, Espacio hace algo en un trimestre");
        let mut a = app();
        ir(&mut a, "Impuestos");
        teclas(&mut a, "<left>");
        let antes = pantalla(&mut a, 120, 36);
        let (dir, v) = ficheros(&[("tique.pdf", b"%PDF tique")]);
        pegar(&mut a, &v);
        teclas(&mut a, "<space>");
        assert!(a.documentos.juego.en_el_aire());
        // Oculta (con Esc), la pantalla de debajo está como estaba (salvo la fila de los avisos y el carril, donde la
        // mesa del gestor dice que los ha recibido).
        teclas(&mut a, "<esc>");
        let ahora = pantalla(&mut a, 120, 36);
        let sin_avisos = |p: &str| crate::tui::prueba::sin_menu(p).lines().enumerate().filter(|(i, _)| *i != 33).map(|(_, l)| l.to_string()).collect::<Vec<_>>();
        assert_eq!(sin_avisos(&antes), sin_avisos(&ahora));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Enter pasa a revisar los ya leídos (con «o abrir»); acabados, si faltan por leer, otra vez la lista. Esc la
    /// oculta: se sigue leyendo, abajo se dice por dónde va y las teclas vuelven a la pantalla; al acabar, vuelve a
    /// salir y espera al Enter.
    #[test]
    fn enter_revisa_y_esc_oculta() {
        let mut a = app();
        ir(&mut a, "Facturas");
        let (dir, v) = ficheros(&[("uno.pdf", b"%PDF uno"), ("dos.pdf", b"%PDF dos")]);
        pegar(&mut a, &v);
        llega_el(&mut a, &v[0], Ok(gasto(&fmt::hoy())));
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Documento 1 de 2 · uno.pdf") && p.contains("guardar") && p.contains("o abrir"), "{p}");
        assert!(!a.documentos.animando() && a.espera() == 100);
        teclas(&mut a, "<esc>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Leyendo tus documentos") && fila(&p, "uno.pdf").contains("descartado") && p.contains("1 de 2 leídos"), "{p}");
        teclas(&mut a, "<esc>");
        assert!(!a.documentos.abierto() && a.documentos.ocupado() && a.espera() == 100);
        let p = pantalla(&mut a, 120, 36);
        assert!(!p.contains("Leyendo tus documentos") && p.contains("Claude está leyendo"), "{p}");
        assert_eq!(a.documentos.progreso().as_deref(), Some("Claude está leyendo tus documentos: 1 de 2 leídos…"));
        teclas(&mut a, "2");
        assert_eq!(a.titulo_activa(), "Gastos");
        // (El mismo gasto que el descartado: como no se guardó, no es repetido.)
        llega_el(&mut a, &v[1], Ok(gasto(&fmt::hoy())));
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Leyendo tus documentos") && p.contains("✓ todos leídos · Enter revisarlos"), "{p}");
        teclas(&mut a, "<enter>");
        assert!(pantalla(&mut a, 120, 36).contains("Documento 2 de 2 · dos.pdf"));
        teclas(&mut a, "<enter>");
        assert!(!a.documentos.abierto() && a.espera() == 500);
        assert_eq!(aviso(&a), "Documentos: 1 guardado (en Autónomo › Gastos), 1 descartado.");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// «o» abre el original, también si no se ha podido meter; si ya no está, se dice. Lo pegado mientras se revisa
    /// no son teclas (una «o» de la ruta no lo abre).
    #[test]
    fn o_abre_el_original() {
        let mut a = app();
        let (dir, v) = ficheros(&[("carta.pdf", b"%PDF carta")]);
        pegar(&mut a, &v);
        llega_el(&mut a, &v[0], Err("No parece una nómina, un gasto ni un ingreso (una carta).".into()));
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("No se ha podido meter") && p.contains("o abrir"), "{p}");
        a.ctx.aviso = None;
        a.pegar("o o o");
        assert!(a.ctx.aviso.is_none() && a.documentos.abierto());
        teclas(&mut a, "o");
        assert!(aviso(&a).contains("abriría") && aviso(&a).contains("carta.pdf"), "{}", aviso(&a));
        std::fs::remove_dir_all(&dir).unwrap();
        teclas(&mut a, "o");
        assert!(aviso(&a).contains("No encuentro") && aviso(&a).contains("carta.pdf"), "{}", aviso(&a));
        assert_eq!(a.ctx.aviso.as_ref().map(|x| x.1), Some(Nivel::Mal));
        teclas(&mut a, "<enter>");
        assert!(!a.documentos.abierto());
        assert_eq!(aviso(&a), "Documentos: 1 sin meter.");
    }

    /// Lo entendido se enseña y, con Enter, va a su sitio con su documento; mientras, las teclas de navegar no
    /// navegan. Lo que no se ha podido clasificar se enseña con el motivo; con Esc, se descarta.
    #[test]
    fn se_ve_se_guarda_o_se_descarta() {
        let mut a = app();
        ir(&mut a, "Facturas");
        let antes = a.ctx.datos.gastos.len();
        llega(&mut a, "tique.pdf", "aaaaaaaaaaaa", Ok(gasto(&fmt::hoy())));
        llega(&mut a, "foto.jpg", "bbbbbbbbbbbb", Err("No cuadra: base y total.".into()));
        llega(&mut a, "otro.pdf", "cccccccccccc", Ok(gasto(&fmt::hoy())));
        // Primero, la lista: cada uno con lo entendido en corto o por qué no.
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("Leyendo tus documentos") && p.contains("✓ todos leídos · Enter revisarlos"), "{p}");
        assert!(fila(&p, "tique.pdf").contains("gasto · Papelería Inventada · 121,00 €") && fila(&p, "foto.jpg").contains("No cuadra: base y total"), "{p}");
        teclas(&mut a, "e2");
        assert_eq!(a.titulo_activa(), "Facturas");
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("Documento 1 de 3 · tique.pdf") && p.contains("Es un gasto") && p.contains("Autónomo › Gastos"), "{p}");
        assert!(p.contains("Papelería Inventada") && p.contains("121,00 €") && p.contains("Un aviso inventado") && p.contains("guardar"), "{p}");
        teclas(&mut a, "e2");
        assert_eq!(a.titulo_activa(), "Facturas");
        teclas(&mut a, "<enter>");
        assert_eq!(a.ctx.datos.gastos.len(), antes + 1);
        assert_eq!(a.ctx.datos.gastos.last().unwrap().doc, "tique-aaaaaaaaaaaa.pdf");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("Documento 2 de 3 · foto.jpg") && p.contains("No cuadra") && p.contains("seguir"), "{p}");
        teclas(&mut a, "<enter>");
        // El tercero es igual que el primero (ya guardado): no se guarda.
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("No se guarda: Ya tienes este gasto"), "{p}");
        teclas(&mut a, "<esc>");
        assert!(!a.documentos.abierto());
        assert_eq!(a.ctx.datos.gastos.len(), antes + 1);
        assert_eq!(aviso(&a), "Documentos: 1 guardado (en Autónomo › Gastos), 2 sin meter.");
        // Uno nuevo, descartado con Esc.
        llega(&mut a, "más.pdf", "dddddddddddd", Ok(gasto(&fmt::sumar_dias(&fmt::hoy(), -1))));
        assert!(pantalla(&mut a, 120, 40).contains("✓ leído · Enter revisarlo"));
        teclas(&mut a, "<enter><esc>");
        assert_eq!(a.ctx.datos.gastos.len(), antes + 1);
        assert_eq!(aviso(&a), "Documentos: 1 descartado.");
    }

    /// En dólares: hay que poner los euros antes de guardar (también pegándolos).
    #[test]
    fn en_otra_moneda_pide_los_euros() {
        let mut a = app();
        let linea = Linea { concepto: "Trabajo".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(0.0) };
        let f = Factura { id: "f-usd".into(), estado: "borrador".into(), fecha: fmt::hoy(), operacion: "extra".into(), cliente: Cliente { nombre: "Inventada Inc".into(), pais: "US".into(), ..Default::default() }, lineas: vec![linea], retencion: Num::de(0.0), ..Default::default() };
        let e = Entendido { apunte: Apunte::Ingreso(f), que: "aviso de pago".into(), moneda: Some(("USD".into(), 1000.0)), avisos: vec![] };
        llega(&mut a, "pago.pdf", "eeeeeeeeeeee", Ok(e));
        // En la lista, en su moneda.
        assert!(fila(&pantalla(&mut a, 120, 40), "pago.pdf").contains("factura en borrador · Inventada Inc · 1.000,00 USD"));
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("1.000,00 USD") && p.contains("¿Cuántos euros fueron?") && p.contains("Autónomo › Facturas"), "{p}");
        teclas(&mut a, "<enter>");
        assert!(pantalla(&mut a, 120, 40).contains("Escribe cuántos euros fueron"));
        // Lo pegado va a los euros, no como teclas (la «o» no abre nada).
        a.pegar("1o");
        let p = pantalla(&mut a, 120, 40);
        assert!(fila(&p, "Euros:").contains("1o") && !p.contains("Escribe cuántos euros fueron") && !aviso(&a).contains("abriría"), "{p}");
        teclas(&mut a, "<bs><bs>912,5<enter>");
        let f = a.ctx.datos.facturas.iter().find(|f| f.id == "f-usd").expect("guardada");
        assert_eq!(crate::dominio::factura::totales_factura(f).total, 912.5);
        assert!(f.notas.contains("USD") && !f.cliente.id.is_empty());
    }

    /// Un ingreso de un cliente de fuera, en dólares, en una terminal pequeña: se ve qué falta para emitirlo, los
    /// euros y las teclas. Guardado, queda en borrador en Facturas y `E` dice qué le falta.
    #[test]
    fn ingreso_en_terminal_pequena_y_luego_en_facturas() {
        let mut a = app();
        let linea = Linea { concepto: "Pagos de un cliente inventado".into(), cantidad: Num::de(1.0), precio: Num::de(15000.0), iva: Num::de(0.0) };
        let f = Factura { id: "f-imp".into(), estado: "borrador".into(), fecha: fmt::hoy(), cobrada: Some(fmt::hoy()), operacion: "extra".into(), cliente: Cliente { nombre: "Cliente Lejano Inc".into(), pais: "US".into(), operacion: "extra".into(), ..Default::default() }, lineas: vec![linea], retencion: Num::de(0.0), creada: fmt::ahora(), ..Default::default() };
        let avisos = vec!["Queda en borrador: revísala en Facturas y emítela con E (le da número de tu serie).".into()];
        llega(&mut a, "pago.pdf", "abababababab", Ok(Entendido { apunte: Apunte::Ingreso(f), que: "aviso de pago de un cliente".into(), moneda: Some(("USD".into(), 15000.0)), avisos }));
        assert!(pantalla(&mut a, 80, 24).contains("Leyendo tus documentos"));
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("Para emitirla: Pon el número fiscal") && p.contains("Euros:") && p.contains("guardar") && p.contains("descartar"), "{p}");
        teclas(&mut a, "13800<enter>");
        assert!(!a.documentos.abierto());
        ir(&mut a, "Facturas");
        teclas(&mut a, "/Cliente Lejano<enter>");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("Cliente Lejano") && p.contains("13.800"), "{p}");
        (a.ctx.datos.perfil.nombre, a.ctx.datos.perfil.nif) = ("Ana Inventada".into(), "12345678Z".into());
        teclas(&mut a, "E");
        assert!(aviso(&a).contains("número fiscal") || pantalla(&mut a, 120, 40).contains("número fiscal"), "{}", aviso(&a));
    }

    /// Un gasto metido así se abre en su formulario como cualquier otro. Mientras escribes en un formulario, la
    /// ventana espera (ni se ve ni coge las teclas).
    #[test]
    fn el_gasto_se_edita_y_la_ventana_espera() {
        let mut a = app();
        ir(&mut a, "Gastos");
        teclas(&mut a, "n");
        llega(&mut a, "tique.pdf", "acacacacacac", Ok(gasto(&fmt::hoy())));
        let p = pantalla(&mut a, 120, 40);
        assert!(!p.contains("Leyendo tus documentos") && !p.contains("Documento 1 de 1"), "{p}");
        assert_eq!(a.espera(), 500, "sin la lista a la vista, no se repinta tanto");
        teclas(&mut a, "<enter>");
        assert!(a.documentos.abierto(), "el Enter era del formulario");
        teclas(&mut a, "<esc>");
        if a.pantalla().capturando() {
            teclas(&mut a, "<esc>");
        }
        assert!(pantalla(&mut a, 120, 40).contains("Leyendo tus documentos"));
        teclas(&mut a, "<enter>");
        assert!(pantalla(&mut a, 120, 40).contains("Documento 1 de 1"));
        teclas(&mut a, "<enter>");
        teclas(&mut a, "/Papelería Inventada<enter>");
        for _ in 0..2 {
            if !a.pantalla().capturando() {
                teclas(&mut a, "<enter>");
            }
        }
        assert!(a.pantalla().capturando());
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("Papelería Inventada") && p.contains("Folios"), "{p}");
    }

    /// Una nómina va a Empleo › Nóminas; un gasto de un año cerrado no se guarda; un documento ya metido no se
    /// vuelve a mandar a Claude.
    #[test]
    fn nomina_anio_cerrado_y_ya_metido() {
        let mut a = app();
        let n = Nomina { id: "n-nueva".into(), anio: PRIMER_ANIO, mes: 2, empresa: "Empresa Inventada".into(), dinerario: 2000.0, retenciones: 300.0, ss: 130.0, ..Default::default() };
        llega(&mut a, "nomina.pdf", "ffffffffffff", Ok(Entendido { apunte: Apunte::Nomina(n), que: "nómina".into(), moneda: None, avisos: vec![] }));
        assert!(fila(&pantalla(&mut a, 120, 40), "nomina.pdf").contains(&format!("nómina de febrero de {PRIMER_ANIO} · Empresa Inventada")));
        teclas(&mut a, "<enter>");
        assert!(pantalla(&mut a, 120, 40).contains("Empleo › Nóminas"));
        teclas(&mut a, "<enter>");
        assert_eq!(a.ctx.datos.nominas.last().map(|n| n.doc.as_str()), Some("nomina-ffffffffffff.pdf"));
        assert!(aviso(&a).contains(&format!("de {PRIMER_ANIO}: cambia el año con - +")), "{}", aviso(&a));
        llega(&mut a, "viejo.pdf", "999999999999", Ok(gasto(&format!("{PRIMER_ANIO}-03-01"))));
        let p = pantalla(&mut a, 120, 40);
        assert!(fila(&p, "viejo.pdf").contains("✗") && fila(&p, "viejo.pdf").contains(&format!("es de {PRIMER_ANIO}, un año cerrado")), "{p}");
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains(&format!("No se guarda: Es de {PRIMER_ANIO}")), "{p}");
        teclas(&mut a, "<enter>");
        assert!(!a.documentos.abierto());
        // Con la función encendida, el mismo documento (por su contenido) ya no se manda.
        let (dir, v) = ficheros(&[("la misma nómina.pdf", b"%PDF nomina inventada")]);
        let f = clasificar::leer(&v[0]).unwrap();
        a.ctx.datos.nominas.last_mut().unwrap().doc = clasificar::nombre_copia(&f);
        pegar(&mut a, &v);
        assert!(!a.documentos.ocupado());
        let p = pantalla(&mut a, 120, 40);
        assert!(fila(&p, "la misma nómina.pdf").contains("ya está en tus nóminas") && p.contains("✓ leído · Enter revisarlo"), "{p}");
        teclas(&mut a, "<enter>");
        let p = pantalla(&mut a, 120, 40);
        assert!(p.contains("No se mete otra vez: ya está en tus nóminas") && p.contains("o abrir"), "{p}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Muchos a la vez en una terminal pequeña: los que se están leyendo, a la vista, y «… y N más».
    #[test]
    fn muchos_en_una_terminal_pequena() {
        let mut a = app();
        let nombres: Vec<String> = (1..=30).map(|i| format!("tique-{i:02}.pdf")).collect();
        let contenidos: Vec<Vec<u8>> = (1..=30).map(|i| format!("%PDF tique {i}").into_bytes()).collect();
        let v: Vec<(&str, &[u8])> = nombres.iter().zip(&contenidos).map(|(n, c)| (n.as_str(), c.as_slice())).collect();
        let (dir, rutas) = ficheros(&v);
        pegar(&mut a, &rutas);
        for r in &rutas[..20] {
            llega_el(&mut a, r, Err("No cuadra.".into()));
        }
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("tique-21.pdf") && p.contains("más") && p.contains("20 de 30 leídos") && p.contains("saltados") && p.contains("Esc ocultar"), "{p}");
        // Más pequeña aún, sin juego; y en cualquier tamaño, sin romperse (la lista y la revisión).
        let p = pantalla(&mut a, 60, 12);
        assert!(p.contains("Leyendo tus documentos") && !p.contains("saltados") && !p.contains("Espacio"), "{p}");
        for (w, h) in [(20, 5), (30, 8), (40, 13), (50, 14), (120, 15)] {
            pantalla(&mut a, w, h);
        }
        teclas(&mut a, "<enter>");
        for (w, h) in [(20, 5), (30, 8), (40, 13)] {
            pantalla(&mut a, w, h);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn nomina_de_octubre() -> Entendido {
        let n = Nomina { id: "n-oct".into(), anio: fmt::anio_actual(), mes: 10, empresa: "Empresa Inventada".into(), dinerario: 2100.0, ..Default::default() };
        Entendido { apunte: Apunte::Nomina(n), que: "nómina".into(), moneda: None, avisos: vec![] }
    }

    /// La mesa del gestor sigue a los documentos: al soltarlos, un momento, «¡recibido!» (con lo que dice el
    /// gestor); luego, «leyendo»; leídos todos, «ordenado», con una carpeta por cada uno entendido (del color de
    /// donde va), mientras se revisan; revisados, otra vez esperando.
    #[test]
    fn la_mesa_del_gestor_sigue_a_los_documentos() {
        let mut a = app();
        let (dir, v) = ficheros(&[("gasto.pdf", b"%PDF gasto"), ("nomina.pdf", b"%PDF nomina"), ("borrosa.jpg", b"foto")]);
        assert_eq!(a.documentos.mesa(Instant::now()), Mesa::Reposo);
        assert!(pantalla(&mut a, 120, 36).contains("⇣ arrastra aquí"));
        pegar(&mut a, &v);
        assert_eq!(a.documentos.mesa(Instant::now()), Mesa::Recibido(3));
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("✓ ¡recibido! 3") && p.contains("antes de las dos") && p.contains("Leyendo tus documentos"), "{p}");
        // Pasado el momento, leyendo (el primero que llega es el segundo leído: «leyendo 2 de 3»).
        let luego = Instant::now() + RECIBIDO;
        assert_eq!(a.documentos.mesa(luego), Mesa::Leyendo { leidos: 0, total: 3 });
        llega_el(&mut a, &v[0], Ok(gasto(&fmt::hoy())));
        assert_eq!(a.documentos.mesa(luego), Mesa::Leyendo { leidos: 1, total: 3 });
        a.documentos.soltado = Some((Instant::now() - RECIBIDO, 3));
        assert!(pantalla(&mut a, 120, 36).contains("leyendo 2 de 3"));
        llega_el(&mut a, &v[1], Ok(nomina_de_octubre()));
        llega_el(&mut a, &v[2], Err("No se lee bien.".into()));
        // Leídos: dos carpetas (lo que no se ha entendido, no) y los tres por revisar.
        assert_eq!(a.documentos.mesa(luego), Mesa::Ordenado { carpetas: vec![tema::AUTONOMO, tema::EMPLEO], por_revisar: 3 });
        assert!(pantalla(&mut a, 120, 36).contains("3 por revisar"));
        teclas(&mut a, "<enter><esc>");
        assert_eq!(a.documentos.mesa(luego), Mesa::Ordenado { carpetas: vec![tema::EMPLEO], por_revisar: 2 });
        teclas(&mut a, "<esc><enter>");
        assert!(!a.documentos.abierto());
        assert_eq!(a.documentos.mesa(luego), Mesa::Reposo);
        assert!(pantalla(&mut a, 120, 36).contains("⇣ arrastra aquí"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Si no va nada a Claude (apagada, lo pegado no es un fichero, ya lo tienes) o no entiende ninguno, la mesa
    /// no dice «recibido» ni «ordenado»: sigue esperando.
    #[test]
    fn la_mesa_no_dice_lo_que_no_ha_pasado() {
        let mut a = app();
        a.documentos.modo = Err(clasificar::COMO_ENCENDER.into());
        let (dir, v) = ficheros(&[("uno.pdf", b"%PDF uno")]);
        a.pegar(&v[0].to_string_lossy());
        assert!(a.documentos.abierto());
        assert_eq!(a.documentos.mesa(Instant::now()), Mesa::Reposo);
        let mut a = app();
        a.documentos.modo = Ok(Modo::Claude);
        a.pegar("hola, esto no es un fichero");
        assert_eq!(a.documentos.mesa(Instant::now()), Mesa::Reposo);
        // Ya metido: no va a Claude.
        let ya = clasificar::nombre_copia(&clasificar::leer(&v[0]).unwrap());
        a.ctx.datos.gastos.first_mut().unwrap().doc = ya;
        pegar(&mut a, &v);
        assert_eq!(a.documentos.mesa(Instant::now()), Mesa::Reposo);
        // Ninguno entendido.
        let mut a = app();
        let (dir2, v2) = ficheros(&[("otro.pdf", b"%PDF otro")]);
        pegar(&mut a, &v2);
        llega_el(&mut a, &v2[0], Err("No se lee.".into()));
        assert_eq!(a.documentos.mesa(Instant::now() + RECIBIDO), Mesa::Reposo);
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(dir2).unwrap();
    }

    /// Para ver la mesa del gestor en cada momento, con la ventana de los documentos a su lado: `ESTADO=leyendo W=120
    /// H=36 cargo test --lib ver_mesa -- --ignored --nocapture` (`recibido`, `leyendo`, `ordenado` u oculta, con
    /// la lista oculta mientras lee). Con `SALIDA`, también sus colores en JSON (ver `prueba::colores`).
    #[test]
    #[ignore]
    fn ver_mesa() {
        let var = |k: &str, defecto: &str| std::env::var(k).unwrap_or_else(|_| defecto.into());
        let mut a = app();
        let (dir, v) = ficheros(&[("gasto.pdf", b"%PDF gasto"), ("nomina.pdf", b"%PDF nomina"), ("factura.pdf", b"%PDF factura")]);
        pegar(&mut a, &v);
        let estado = var("ESTADO", "recibido");
        if estado != "recibido" {
            a.documentos.soltado = Some((Instant::now() - RECIBIDO, 3));
            llega_el(&mut a, &v[0], Ok(gasto(&fmt::hoy())));
        }
        if estado == "oculta" {
            teclas(&mut a, "<esc>");
        }
        if estado == "ordenado" {
            llega_el(&mut a, &v[1], Ok(nomina_de_octubre()));
            llega_el(&mut a, &v[2], Ok(gasto_de("Imprenta Inventada", 40.0, &fmt::hoy())));
        }
        let (w, h) = (var("W", "120").parse().unwrap(), var("H", "36").parse().unwrap());
        println!("{}", pantalla(&mut a, w, h));
        if let Ok(salida) = std::env::var("SALIDA") {
            std::fs::write(salida, crate::tui::prueba::colores(&mut a, w, h)).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Para ver la ventana a medias (uno entendido, uno que no, uno ya metido, uno repetido, dos leyendo) con el
    /// juego en pleno salto: `W=120 H=36 TECLAS="<enter><enter>" cargo test --lib ver_documentos -- --ignored
    /// --nocapture` (con `TECLAS`, lo que se ve después: con dos Enter, la revisión del gasto).
    #[test]
    #[ignore]
    fn ver_documentos() {
        let var = |k: &str, defecto: &str| std::env::var(k).unwrap_or_else(|_| defecto.into());
        let mut a = app();
        ir(&mut a, "Gastos");
        let (dir, v) = ficheros(&[
            ("tique-gasolinera.pdf", b"%PDF gasolinera"),
            ("nomina-octubre.pdf", b"%PDF nomina"),
            ("foto-recibo.jpg", b"foto movida"),
            ("factura-enero.pdf", b"%PDF enero"),
            ("copia-del-tique.pdf", b"%PDF gasolinera"),
            ("notas.txt", b"notas"),
            ("factura-cliente.pdf", b"%PDF cliente"),
            ("recibo-luz.pdf", b"%PDF luz"),
        ]);
        let ya = clasificar::nombre_copia(&clasificar::leer(&v[3]).unwrap());
        a.ctx.datos.gastos.first_mut().unwrap().doc = ya;
        pegar(&mut a, &v);
        llega_el(&mut a, &v[0], Ok(gasto_de("Gasolinera Inventada", 34.79, &fmt::hoy())));
        let n = Nomina { id: "n-oct".into(), anio: fmt::anio_actual(), mes: 10, empresa: "Empresa Inventada".into(), dinerario: 2100.0, ..Default::default() };
        llega_el(&mut a, &v[1], Ok(Entendido { apunte: Apunte::Nomina(n), que: "nómina".into(), moneda: None, avisos: vec![] }));
        llega_el(&mut a, &v[2], Err("No se lee bien: la foto está movida.".into()));
        // El juego: unos segundos de carrera y, al final, en el aire.
        let t0 = Instant::now();
        let mut ms = 0;
        a.documentos.animar(t0);
        a.documentos.juego.saltar(t0);
        while ms < 1850 {
            ms += 40;
            if ms == 1640 {
                a.documentos.juego.saltar(t0 + Duration::from_millis(ms));
            }
            a.documentos.animar(t0 + Duration::from_millis(ms));
        }
        teclas(&mut a, &var("TECLAS", ""));
        println!("{}", pantalla(&mut a, var("W", "120").parse().unwrap(), var("H", "36").parse().unwrap()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
