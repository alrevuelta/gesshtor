//! Conectar con un bróker, desde Inversiones: activar su conector (diciendo antes qué hace y qué riesgos tiene),
//! pedir lo de entrar (que no se guarda: solo vive en esta ventana mientras se intenta) y traerlo en otro hilo.
//! Al acabar, lo traído va a `fuentes/<conector>.json` (un fallo no pisa lo de antes) y a los datos.
use crate::brokers::{self, Conector};
use crate::dominio::fuente::Error;
use crate::dominio::inversiones::Cartera;
use crate::dominio::{almacen, fmt};
use crate::tui::widgets::{Confirmar, Eleccion, Formulario, Resultado, Respuesta, Selector, Tipo};
use crate::tui::Ctx;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::sync::mpsc::{self, Receiver};

enum Paso {
    Nada,
    /// Con más de un bróker: cuál (para traer o para desconectar).
    Elegir(Para, Selector),
    Activar(&'static Conector, Confirmar),
    Desactivar(&'static Conector, Confirmar),
    /// Pidiendo lo de entrar.
    Pedir(&'static Conector, Formulario),
    /// Trayendo; la ventana se guarda (sin enseñarla) por si hay que volver a intentarlo.
    Trayendo(&'static Conector, Formulario),
}

#[derive(Clone, Copy)]
pub(super) enum Para {
    Traer,
    Desconectar,
}

enum Mensaje {
    Paso(String),
    /// En una caja: el error lleva su informe entero.
    Hecho(Box<Result<Cartera, Error>>),
}

pub(super) struct Conexion {
    paso: Paso,
    rx: Option<Receiver<Mensaje>>,
    /// Por dónde va, mientras se trae.
    pub(super) progreso: Option<String>,
}

impl Default for Conexion {
    fn default() -> Self {
        Conexion { paso: Paso::Nada, rx: None, progreso: None }
    }
}

impl Conexion {
    /// ¿Hay una ventana abierta? (mientras, las letras y los números se escriben)
    pub(super) fn abierta(&self) -> bool {
        matches!(self.paso, Paso::Elegir(..) | Paso::Activar(..) | Paso::Desactivar(..) | Paso::Pedir(..))
    }

    /// `b` (traer) o `x` (desconectar): si hay un solo bróker posible, ese; si hay más, se elige.
    pub(super) fn elegir(&mut self, para: Para, ctx: &mut Ctx) {
        let v: Vec<&'static Conector> = brokers::CONECTORES.iter().filter(|c| matches!(para, Para::Traer) || ctx.datos.inversiones.activado(c.id)).collect();
        match (v.as_slice(), para) {
            ([], _) => ctx.info("No hay ningún bróker conectado."),
            ([c], Para::Traer) => self.empezar(c, ctx),
            ([c], Para::Desconectar) => self.desactivar(c, ctx),
            (_, _) => {
                let titulo = if matches!(para, Para::Traer) { "¿De qué bróker?" } else { "¿Qué bróker desconectas?" };
                let items = v.iter().map(|c| (c.id.to_string(), c.nombre.to_string(), if ctx.datos.inversiones.activado(c.id) { "conectado".into() } else { "desactivado".into() })).collect();
                self.paso = Paso::Elegir(para, Selector::new(titulo, items));
            }
        }
    }

    pub(super) fn trayendo(&self) -> bool {
        matches!(self.paso, Paso::Trayendo(..))
    }

    /// Traer lo de un bróker: si su conector no está activado, primero se pide activarlo.
    pub(super) fn empezar(&mut self, c: &'static Conector, ctx: &mut Ctx) {
        if self.trayendo() {
            ctx.info(format!("Ya se está trayendo lo de {}…", c.nombre));
            return;
        }
        self.paso = if ctx.datos.inversiones.activado(c.id) {
            Paso::Pedir(c, formulario(c))
        } else {
            let mut v = Confirmar::new(format!("Conectar con {}", c.nombre), "Activar");
            v.lineas = parrafos(c.aviso);
            Paso::Activar(c, v)
        };
    }

    /// Desactivar un conector: la app no vuelve a hablar con ese bróker (lo traído se sigue viendo).
    pub(super) fn desactivar(&mut self, c: &'static Conector, ctx: &mut Ctx) {
        if !ctx.datos.inversiones.activado(c.id) || self.trayendo() {
            return;
        }
        let v = Confirmar::new(format!("Desconectar {}", c.nombre), "Desactivar")
            .linea(format!("La app no volverá a hablar con {} hasta que lo actives otra vez.", c.nombre))
            .linea(format!("Lo ya traído se sigue viendo (está en fuentes/{}.json: puedes borrarlo).", c.id));
        self.paso = Paso::Desactivar(c, v);
    }

    pub(super) fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) {
        match std::mem::replace(&mut self.paso, Paso::Nada) {
            Paso::Elegir(para, mut sel) => match sel.tecla(k) {
                Eleccion::Elegido(id) => match (brokers::conector(&id), para) {
                    (Some(c), Para::Traer) => self.empezar(c, ctx),
                    (Some(c), Para::Desconectar) => self.desactivar(c, ctx),
                    (None, _) => {}
                },
                Eleccion::Cancelado => {}
                Eleccion::Sigue => self.paso = Paso::Elegir(para, sel),
            },
            Paso::Activar(c, v) => match v.tecla(k) {
                Respuesta::Si => {
                    ctx.datos.inversiones.conectores.insert(c.id.into(), true);
                    ctx.guardar();
                    self.paso = Paso::Pedir(c, formulario(c));
                }
                Respuesta::No => {}
                Respuesta::Sigue => self.paso = Paso::Activar(c, v),
            },
            Paso::Desactivar(c, v) => match v.tecla(k) {
                Respuesta::Si => {
                    ctx.datos.inversiones.conectores.insert(c.id.into(), false);
                    ctx.guardar();
                    ctx.ok(format!("Desconectado de {}: la app no hablará con él.", c.nombre));
                }
                Respuesta::No => {}
                Respuesta::Sigue => self.paso = Paso::Desactivar(c, v),
            },
            Paso::Pedir(c, mut f) => match f.tecla(k) {
                Resultado::Cancelar => {}
                Resultado::Guardar => {
                    let datos: Vec<String> = f.campos.iter().map(|x| if matches!(x.tipo, Tipo::Secreto) { x.entrada.valor.clone() } else { x.entrada.valor.trim().to_string() }).collect();
                    if datos.len() >= 2 && (datos[0].is_empty() || datos[1].is_empty()) {
                        f.error = Some(format!("Escribe tu usuario y tu contraseña de {}.", c.nombre));
                        self.paso = Paso::Pedir(c, f);
                        return;
                    }
                    self.traer(c, f, datos, ctx);
                }
                _ => self.paso = Paso::Pedir(c, f),
            },
            p => self.paso = p,
        }
    }

    /// Trae en otro hilo (en las pruebas, no: no se sale a la red).
    fn traer(&mut self, c: &'static Conector, f: Formulario, datos: Vec<String>, ctx: &mut Ctx) {
        if ctx.sin_disco {
            ctx.info(format!("(prueba) traería lo de {}", c.nombre));
            return;
        }
        let (tx, rx) = mpsc::channel();
        let inversiones = ctx.datos.inversiones.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let r = brokers::traer(&inversiones, c, &datos, &mut |p| {
                let _ = tx2.send(Mensaje::Paso(p));
            });
            let _ = tx.send(Mensaje::Hecho(Box::new(r)));
        });
        self.rx = Some(rx);
        self.progreso = Some(format!("{}: conectando…", c.nombre));
        self.paso = Paso::Trayendo(c, f);
    }

    /// Lo que llega del hilo. Al acabar, se guarda (también el fallo, sin pisar lo de antes).
    pub(super) fn atender(&mut self, ctx: &mut Ctx) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut fin = None;
        let mut cambio = false;
        while let Ok(m) = rx.try_recv() {
            match m {
                Mensaje::Paso(p) => self.progreso = Some(p),
                Mensaje::Hecho(r) => fin = Some(*r),
            }
            cambio = true;
        }
        let Some(r) = fin else { return cambio };
        self.rx = None;
        self.progreso = None;
        let Paso::Trayendo(c, mut f) = std::mem::replace(&mut self.paso, Paso::Nada) else { return true };
        let (dato, error) = match &r {
            Ok(cartera) => (Some(cartera), None),
            Err(e) => (None, Some(e)),
        };
        match almacen::guardar_cartera(&ctx.dir, c.id, &brokers::origen(c), &fmt::ahora(), dato, error) {
            Ok(fuente) => {
                ctx.datos.carteras.insert(c.id.into(), fuente);
            }
            Err(e) => ctx.error(format!("No se ha podido guardar lo traído de {}: {e}", c.nombre)),
        }
        ctx.generacion += 1;
        match r {
            Ok(cartera) => ctx.ok(format!("Traído de {}: {} operaciones y {} cobros.", c.nombre, cartera.operaciones.len(), cartera.cobros.len())),
            Err(e) => {
                // Para volver a intentarlo: la misma ventana, con el error y sin el código (sirve una vez).
                for (x, p) in f.campos.iter_mut().zip(c.pide) {
                    if p.una_vez {
                        x.entrada = Default::default();
                    }
                }
                f.error = Some(e.mensaje.clone());
                ctx.error(e.mensaje);
                self.paso = Paso::Pedir(c, f);
            }
        }
        true
    }

    pub(super) fn dibujar(&mut self, f: &mut Frame, area: Rect) {
        match &mut self.paso {
            Paso::Elegir(_, sel) => sel.dibujar(f, area),
            Paso::Activar(_, v) | Paso::Desactivar(_, v) => v.dibujar(f, area),
            Paso::Pedir(_, form) => form.dibujar_ventana(f, area, 72),
            Paso::Nada | Paso::Trayendo(..) => {}
        }
    }
}

/// El aviso de un conector viene en líneas cortadas a mano: aquí se juntan en párrafos (separados por una línea en
/// blanco, como vienen) para que la ventana los parta a su ancho.
fn parrafos(lineas: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for grupo in lineas.split(|l| l.trim().is_empty()) {
        let p = grupo.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
        if p.is_empty() {
            continue;
        }
        if !v.is_empty() {
            v.push(String::new());
        }
        v.push(p);
    }
    v
}

/// La ventana para entrar: lo que pide el conector, sin nada escrito.
fn formulario(c: &Conector) -> Formulario {
    let mut f = Formulario::new(format!("Entrar en {}", c.nombre));
    for (i, p) in c.pide.iter().enumerate() {
        let clave = i.to_string();
        f = if p.secreto { f.secreto(&clave, p.etiqueta) } else { f.texto(&clave, p.etiqueta, "") };
        f = f.ayuda(p.ayuda);
    }
    f
}

#[cfg(test)]
mod tests {
    use super::parrafos;

    /// El aviso, en párrafos enteros (sin los cortes a mano), con una línea en blanco entre uno y otro.
    #[test]
    fn el_aviso_va_en_parrafos() {
        let v = parrafos(&["Una frase cortada", "a mano.", "", "Otra", "más."]);
        assert_eq!(v, ["Una frase cortada a mano.", "", "Otra más."]);
        let v = parrafos(crate::brokers::conector("degiro").unwrap().aviso);
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v[0].contains("por la API no oficial de su web (trader.degiro.nl)") && v[2].contains("(art. 5.6 de sus Condiciones de los Servicios de Inversión)"), "{v:?}");
    }
}
