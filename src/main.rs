//! gesshtor: tus impuestos en la terminal.
//!
//!     gesshtor --certificado RUTA [carpeta] [--sincronizar] [--todo]
//!
//! Sin certificado no se abre: con él (un .p12 / .pfx) se sabe quién eres y se trae lo último de Hacienda y la
//! Seguridad Social (solo consulta). La carpeta es la de una persona (con datos.json) o una con varias personas en
//! subcarpetas: se abre la del NIF del certificado.
use gesshtor::red::certificado::Certificado;
use gesshtor::sincronizar;
use gesshtor::{dominio::almacen, dominio::Datos, hacienda::lectura, tui};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const AYUDA: &str = "Uso: gesshtor --certificado RUTA [carpeta] [--sincronizar] [--todo]

  --certificado    Tu certificado digital (.p12 / .pfx): sin él no se abre. Pide la
                   contraseña (o la toma de $PANEL_CLAVE_CERTIFICADO) y trae lo último de
                   Hacienda y la Seguridad Social: solo consulta, nunca presenta nada.
  carpeta          Dónde están tus datos: la de una persona (con datos.json) o una con
                   varias en subcarpetas; se abre la del NIF del certificado (o se crea).
                   Por defecto: $PANEL_DATOS o ./mock.
  --sincronizar    Solo trae lo último y lo guarda, sin abrir la app (y dice qué ha traído).
  --todo           Vuelve a consultar todos los años, no solo los dos últimos.
  -V, --version    La versión y el commit del que sale este ejecutable.";

struct Opciones {
    dir: PathBuf,
    certificado: Option<PathBuf>,
    sincronizar: bool,
    todo: bool,
}

fn opciones() -> Result<Opciones, String> {
    let mut o = Opciones { dir: PathBuf::new(), certificado: None, sincronizar: false, todo: false };
    let mut dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" | "--ayuda" => {
                println!("{AYUDA}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("gesshtor {}", gesshtor::VERSION);
                std::process::exit(0);
            }
            "--certificado" | "-c" => o.certificado = Some(PathBuf::from(args.next().ok_or("Falta la ruta del certificado tras --certificado.")?)),
            "--sincronizar" => o.sincronizar = true,
            "--todo" => o.todo = true,
            x if x.starts_with('-') => return Err(format!("No conozco la opción {x}.\n\n{AYUDA}")),
            x => dir = Some(PathBuf::from(x)),
        }
    }
    o.dir = dir.or_else(|| std::env::var_os("PANEL_DATOS").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("mock"));
    if o.certificado.is_none() {
        return Err(format!("Hace falta tu certificado digital: gesshtor --certificado <tu .p12>.\n\n{AYUDA}"));
    }
    Ok(o)
}

/// Abre el certificado: la contraseña de $PANEL_CLAVE_CERTIFICADO o, si no, se pregunta (sin que se vea), hasta
/// tres veces.
fn abrir_certificado(ruta: &Path) -> Result<Certificado, String> {
    if let Ok(clave) = std::env::var("PANEL_CLAVE_CERTIFICADO") {
        return Certificado::abrir(ruta, &clave);
    }
    let mut ultimo = String::new();
    for _ in 0..3 {
        let clave = rpassword::prompt_password(format!("Contraseña de {}: ", ruta.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()))
            .map_err(|e| format!("No se puede leer la contraseña: {e}"))?;
        match Certificado::abrir(ruta, &clave) {
            Ok(c) => return Ok(c),
            Err(e) if e.contains("contraseña") => {
                eprintln!("{e}");
                ultimo = e;
            }
            Err(e) => return Err(e),
        }
    }
    Err(ultimo)
}

/// La carpeta de la persona del certificado: la que tenga su NIF; si no hay ninguna, una nueva (en una carpeta
/// de varias personas) o la carpeta dada si aún no es de nadie.
fn carpeta_de(dir: &Path, cert: &Certificado) -> Result<PathBuf, String> {
    let nif_de = |p: &Path| almacen::cargar(p).map(|d| d.perfil.nif.trim().to_uppercase()).unwrap_or_default();
    let perfiles = almacen::perfiles(dir);
    if let Some(p) = perfiles.iter().find(|p| nif_de(p) == cert.nif) {
        return Ok(p.clone());
    }
    if almacen::ruta_datos(dir).is_file() {
        return match nif_de(dir) {
            n if n.is_empty() => Ok(dir.to_path_buf()),
            _ => Err(format!("{} es de otra persona (otro NIF). Usa la carpeta de esta persona, o una carpeta nueva.", dir.display())),
        };
    }
    // Persona nueva: su carpeta, con su nombre.
    let nombre: String = cert.nombre_completo().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let nueva = dir.join(nombre.trim_matches('-'));
    std::fs::create_dir_all(&nueva).map_err(|e| format!("No se puede crear {}: {e}", nueva.display()))?;
    let mut d = Datos::default();
    sincronizar::perfil_desde(&mut d, cert);
    almacen::guardar(&nueva, &d, &mut false).map_err(|e| format!("No se puede crear {}: {e}", nueva.display()))?;
    eprintln!("Carpeta nueva para {}: {}", cert.nombre_completo(), nueva.display());
    Ok(nueva)
}

fn salir_con(e: &str) -> ! {
    eprintln!("{e}");
    std::process::exit(1);
}

fn main() {
    let o = opciones().unwrap_or_else(|e| salir_con(&e));
    let ruta = o.certificado.as_ref().expect("comprobado en opciones()");
    let cert = abrir_certificado(ruta).unwrap_or_else(|e| salir_con(&e));
    let dir = carpeta_de(&o.dir, &cert).unwrap_or_else(|e| salir_con(&e));
    let mut ctx = tui::Ctx::abrir(&dir).unwrap_or_else(|e| salir_con(&format!("No puedo abrir {}: {e}", dir.display())));
    if cert.dias_para_caducar() < 0 {
        eprintln!("Ojo: tu certificado caducó el {}. Hacienda y la Seguridad Social no lo aceptarán.", cert.caduca);
    }
    if sincronizar::perfil_desde(&mut ctx.datos, &cert) {
        ctx.guardar();
    }
    if o.sincronizar {
        sin_ventana(&mut ctx, &cert, o.todo);
        return;
    }
    let mut app = tui::App::new(ctx);
    app.leer_en_segundo_plano();
    app.ctx.certificado = Some(Arc::new(cert));
    app.sincronizar(o.todo);
    let terminal = ratatui::init();
    let r = tui::ejecutar(terminal, app);
    ratatui::restore();
    if let Err(e) = r {
        salir_con(&e.to_string());
    }
}

/// `--sincronizar`: trae lo último, lo guarda y lee los PDF nuevos, diciendo por dónde va (sin datos personales).
fn sin_ventana(ctx: &mut tui::Ctx, cert: &Certificado, todo: bool) {
    eprintln!("Trayendo lo último con el certificado de {} (caduca el {})…", cert.saludo(), cert.caduca);
    // Por dónde va, siempre en la misma línea (cortada al ancho de la terminal).
    let ancho = ratatui::crossterm::terminal::size().map_or(100, |(w, _)| usize::from(w)).saturating_sub(3);
    let r = sincronizar::sincronizar(&ctx.dir, &ctx.datos, cert, todo, &mut |p| eprint!("\r\x1b[2K  {}", p.chars().take(ancho).collect::<String>()));
    eprintln!();
    sincronizar::aplicar(&mut ctx.datos, &r);
    if !ctx.guardar() {
        salir_con("No se ha podido guardar datos.json.");
    }
    let l = lectura::leer_todo(&ctx.dir, &mut |h, t| eprint!("\r  Leyendo los PDF: {h} de {t}   "));
    eprintln!();
    eprintln!("Hecho: {}. PDF leídos: {} Rentas y {} declaraciones.", r.resumen(), l.rentas.len(), l.formularios.len());
    for e in &r.errores {
        eprintln!("  No se ha podido: {e}");
    }
    if !r.errores.is_empty() {
        std::process::exit(2);
    }
}
