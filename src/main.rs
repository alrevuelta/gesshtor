//! gesshtor: tus impuestos en la terminal.
//!
//!     gesshtor --certificado RUTA [carpeta] [--sincronizar] [--todo]
//!     gesshtor json FICHERO.pdf [AAAA_PP_MMM] | gesshtor json tgss [carpeta | --certificado RUTA]
//!
//! Sin certificado no se abre: con él (un .p12 / .pfx) se sabe quién eres y se trae lo último de Hacienda y la
//! Seguridad Social (solo consulta). La carpeta es la de una persona (con datos.json) o una con varias personas en
//! subcarpetas: se abre la del NIF del certificado. `json`, para ver lo que entiende la app.
use gesshtor::red::certificado::{Certificado, CLAVE_EN_EL_ENTORNO};
use gesshtor::sincronizar;
use gesshtor::{dominio::almacen, dominio::Datos, hacienda::lectura, tui};
use std::io::Write;
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
  -V, --version    La versión y el commit del que sale este ejecutable.

Uso: gesshtor json FICHERO.pdf [AAAA_PP_MMM] | gesshtor json tgss [carpeta | --certificado RUTA]

  json FICHERO.pdf    Lo que lee la app de un PDF de Hacienda, en JSON: lo mismo que guardaría
                      en fuentes/. No guarda nada. De qué es lo dice su nombre, como en
                      justificantes/ (2025_1T_303_CSV.pdf); si se llama de otra forma, dilo
                      detrás: gesshtor json renta.pdf 2025_0A_100. Si no lo entiende, sale
                      con el código 2 y escribe aparte el informe del fallo, para copiarlo.
  json tgss [carpeta] Lo de la Seguridad Social guardado en la carpeta (fuentes/tgss.json), sin
                      certificado: lo que se trajo la última vez.
  json tgss --certificado RUTA
                      Lo trae ahora de la Seguridad Social con tu certificado (tus datos de
                      autónomo y tus nóminas de los últimos meses) y lo enseña, sin guardarlo.";

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

/// Abre el certificado: la contraseña de $PANEL_CLAVE_CERTIFICADO (que se quita del entorno nada más leerla: así no
/// la hereda ningún programa que lance la app) o, si no, se pregunta (sin que se vea), hasta tres veces.
fn abrir_certificado(ruta: &Path) -> Result<Certificado, String> {
    if let Some(clave) = std::env::var_os(CLAVE_EN_EL_ENTORNO) {
        // SAFETY: se llama al empezar, antes de que la app arranque ningún otro hilo.
        unsafe { std::env::remove_var(CLAVE_EN_EL_ENTORNO) };
        return Certificado::abrir(ruta, &clave.to_string_lossy());
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

fn salir_con(e: &str) -> ! {
    eprintln!("{e}");
    std::process::exit(1);
}

/// `gesshtor json …`: el JSON de lo que lee la app de un PDF (sin guardarlo) o el de la Seguridad Social (el guardado
/// o, con el certificado, el de ahora), para ver qué entiende. Sin certificado no sale a la red. ¿Ha salido bien? (un
/// PDF que no se entiende, no)
fn json(args: &[String]) -> Result<bool, String> {
    let mut salida = std::io::stdout();
    match args {
        [t, c, ruta] if t == "tgss" && c == "--certificado" => {
            let cert = abrir_certificado(Path::new(ruta))?;
            let nombres = [cert.nombre_completo(), cert.saludo()];
            let nombres: Vec<&str> = nombres.iter().map(String::as_str).collect();
            let informe = |e: gesshtor::dominio::fallo::Error| gesshtor::dominio::fallo::informes([&e], &nombres);
            eprintln!("Trayendo lo de la Seguridad Social con el certificado de {}…", cert.saludo());
            let ss = gesshtor::tgss::SeguridadSocial::entrar(&cert).map_err(informe)?;
            let mut t = ss.autonomo().map_err(informe)?;
            let meses = gesshtor::tgss::meses_a_consultar(&Datos::default(), &gesshtor::tgss::ahora());
            let (m, _) = ss.nominas(&meses).map_err(informe)?;
            t.nomina.meses = m;
            writeln!(salida, "{}", serde_json::to_string_pretty(&t).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            Ok(true)
        }
        [t, resto @ ..] if t == "tgss" && resto.len() <= 1 => {
            let dir = resto.first().map(PathBuf::from).or_else(|| std::env::var_os("PANEL_DATOS").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("mock"));
            let personas = almacen::perfiles(&dir);
            let [p] = personas.as_slice() else {
                let cuales: Vec<String> = personas.iter().map(|p| p.display().to_string()).collect();
                return Err(match cuales.is_empty() {
                    true => format!("En {} no hay datos de nadie (datos.json).", dir.display()),
                    false => format!("En {} hay varias personas; di la carpeta de una: {}", dir.display(), cuales.join(", ")),
                });
            };
            let ruta = almacen::ruta_fuentes(p).join("tgss.json");
            let txt = std::fs::read_to_string(&ruta).map_err(|_| format!("No hay {}: se crea al traer lo de la Seguridad Social con tu certificado (o al abrir con la app una carpeta de antes).", ruta.display()))?;
            writeln!(salida, "{}", txt.trim_end()).map_err(|e| e.to_string())?;
            Ok(true)
        }
        [pdf, como @ ..] if pdf.to_lowercase().ends_with(".pdf") && como.len() <= 1 => {
            let f = lectura::leer_fichero(Path::new(pdf), como.first().map(String::as_str))?;
            writeln!(salida, "{}", serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            // Si no se entiende, el informe para copiar, aparte (en la salida de errores), con los nombres de la persona
            // tapados si el PDF está en su carpeta (en justificantes/).
            if let Some(e) = &f.error {
                let carpeta = Path::new(pdf).parent().filter(|p| p.ends_with("justificantes")).and_then(Path::parent);
                let perfil = carpeta.and_then(|c| almacen::cargar(c).ok()).map(|d| d.perfil).unwrap_or_default();
                let nombres = [perfil.nombre.as_str(), perfil.nombre_pila.as_str(), perfil.apellidos.as_str()];
                eprintln!("{}", gesshtor::dominio::fallo::informes([e], &nombres));
            }
            Ok(f.error.is_none())
        }
        _ => Err(format!("¿De qué quieres el JSON?\n\n{AYUDA}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "json") {
        let bien = json(&args[1..]).unwrap_or_else(|e| salir_con(&e));
        std::process::exit(if bien { 0 } else { 2 });
    }
    let o = opciones().unwrap_or_else(|e| salir_con(&e));
    let ruta = o.certificado.as_ref().expect("comprobado en opciones()");
    let cert = abrir_certificado(ruta).unwrap_or_else(|e| salir_con(&e));
    let dir = sincronizar::carpeta_de(&o.dir, &cert).unwrap_or_else(|e| salir_con(&e));
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
    let mut r = sincronizar::sincronizar(&ctx.dir, &ctx.datos, cert, todo, &mut |p| eprint!("\r\x1b[2K  {}", p.chars().take(ancho).collect::<String>()));
    eprintln!();
    sincronizar::aplicar(&ctx.dir, &mut ctx.datos, &mut r);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// La contraseña de `$PANEL_CLAVE_CERTIFICADO` se quita del entorno de la app nada más usarla (aunque el
    /// certificado no se abra): así no la hereda ningún programa que lance la app.
    #[test]
    fn la_clave_no_se_queda_en_el_entorno() {
        const CLAVE: &str = gesshtor::red::certificado::CLAVE_EN_EL_ENTORNO;
        // SAFETY: es la única prueba de este ejecutable que toca el entorno.
        unsafe { std::env::set_var(CLAVE, "inventada") };
        assert!(abrir_certificado(Path::new("no-existe-gesshtor.p12")).is_err());
        assert!(std::env::var_os(CLAVE).is_none());
    }
}
