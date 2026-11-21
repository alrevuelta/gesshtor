//! Arrastrar documentos al terminal y que Claude Haiku diga qué son —una nómina, un gasto o un ingreso— y saque sus
//! datos, para apuntarlos donde van: Empleo › Nóminas, Autónomo › Gastos o Autónomo › Facturas. Es opcional y
//! viene apagado: se enciende con la variable `PANEL_CLASIFICAR` (ver `Modo`). Apagado, la app no habla con nadie
//! más que Hacienda y la Seguridad Social; encendido, cada documento que arrastras va a Anthropic (por el comando
//! `claude` o por su API) con un prompt fijo, sin más datos tuyos.
//!
//! - `mod.rs`: si está encendido, las rutas que pega el terminal al soltar ficheros, qué ficheros valen, su huella
//!   (para no meter dos veces el mismo) y su copia en `documentos/`; y `clasificar`, que lo junta todo.
//! - `haiku.rs`: las dos formas de preguntar a Claude Haiku y los prompts, uno pequeño para cada cosa.
//! - `entendido.rs`: lo que contesta, comprobado (no es de fiar), y su paso a nómina, gasto o factura con las reglas
//!   del IVA y del IRPF.
mod entendido;
mod haiku;

pub use entendido::{categoria, guardar, repetido, Apunte, Entendido};

use crate::dominio::{almacen, fmt, Datos};
use std::io;
use std::path::{Path, PathBuf};

/// La variable que lo enciende: `claude` o `api`.
pub const VARIABLE: &str = "PANEL_CLASIFICAR";

/// Cómo se enciende, para decírselo a quien arrastra algo con esto apagado.
pub const COMO_ENCENDER: &str = "Para que Claude clasifique lo que arrastras, abre la app con PANEL_CLASIFICAR=claude (si tienes Claude Code) o con PANEL_CLASIFICAR=api y tu clave en ANTHROPIC_API_KEY.";

/// Cómo se pregunta a Claude Haiku.
#[derive(Debug, Clone, PartialEq)]
pub enum Modo {
    /// Con el comando `claude` (Claude Code) ya instalado y con sesión: `PANEL_CLASIFICAR=claude`.
    Claude,
    /// Con la API de Anthropic y tu clave: `PANEL_CLASIFICAR=api` y `ANTHROPIC_API_KEY`.
    Api(String),
}

/// El modo elegido con las variables de entorno; si no hay ninguno, cómo encenderlo.
pub fn modo() -> Result<Modo, String> {
    modo_de(std::env::var(VARIABLE).ok().as_deref(), std::env::var("ANTHROPIC_API_KEY").ok().as_deref())
}

fn modo_de(variable: Option<&str>, clave: Option<&str>) -> Result<Modo, String> {
    match variable.unwrap_or_default().trim().to_lowercase().as_str() {
        "claude" => Ok(Modo::Claude),
        "api" => match clave.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => Ok(Modo::Api(c.to_string())),
            None => Err("Con PANEL_CLASIFICAR=api hace falta tu clave de la API en ANTHROPIC_API_KEY.".into()),
        },
        "" | "no" | "0" => Err(COMO_ENCENDER.into()),
        x => Err(format!("PANEL_CLASIFICAR={x} no vale: usa «claude» o «api».")),
    }
}

/* ================= lo que pega el terminal ================= */

/// Lo que pega el terminal al soltar uno o varios ficheros, como rutas: (las que son ficheros, lo que no se
/// encuentra). Cada terminal lo pega a su manera: con los espacios escapados (`/a/mi\ nómina.pdf`), entre comillas,
/// como `file:///a/mi%20n%C3%B3mina.pdf`, uno por línea o varios en la misma separados por espacios. Una línea
/// entera que ya es un fichero (con espacios sin escapar) vale tal cual.
pub fn rutas_pegadas(texto: &str) -> (Vec<PathBuf>, Vec<String>) {
    let (mut si, mut no) = (Vec::new(), Vec::new());
    for linea in texto.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let entera = ruta_de(linea.trim_matches(|c| c == '"' || c == '\''));
        let trozos = if entera.is_file() { vec![entera] } else { trocear(linea).iter().map(|t| ruta_de(t)).collect() };
        for p in trozos {
            if p.is_file() {
                if !si.contains(&p) {
                    si.push(p);
                }
            } else {
                no.push(p.to_string_lossy().to_string());
            }
        }
    }
    (si, no)
}

/// Una línea en trozos como lo haría la shell: separados por espacios, con comillas y `\` para escapar (en Windows
/// la `\` separa carpetas: no escapa nada).
fn trocear(linea: &str) -> Vec<String> {
    let escapa = !cfg!(windows);
    let (mut v, mut t, mut comilla, mut hay) = (Vec::new(), String::new(), None, false);
    let mut it = linea.chars();
    while let Some(c) = it.next() {
        match (comilla, c) {
            (_, '\\') if escapa && comilla != Some('\'') => {
                if let Some(x) = it.next() {
                    t.push(x);
                }
                hay = true;
            }
            (Some(q), c) if c == q => comilla = None,
            (Some(_), c) => t.push(c),
            (None, '"' | '\'') => {
                comilla = Some(c);
                hay = true;
            }
            (None, c) if c.is_whitespace() => {
                if hay || !t.is_empty() {
                    v.push(std::mem::take(&mut t));
                }
                hay = false;
            }
            (None, c) => t.push(c),
        }
    }
    if hay || !t.is_empty() {
        v.push(t);
    }
    v
}

/// Un trozo como ruta: `file://…` sin el esquema y con los `%XX` descodificados (un `+` es un `+`).
fn ruta_de(t: &str) -> PathBuf {
    let Some(resto) = t.strip_prefix("file://") else { return PathBuf::from(t) };
    let resto = resto.strip_prefix("localhost").unwrap_or(resto);
    let b = resto.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |j: usize| b.get(j).and_then(|c| (*c as char).to_digit(16));
        match (b[i], hex(i + 1), hex(i + 2)) {
            (b'%', Some(a), Some(c)) => {
                out.push((a * 16 + c) as u8);
                i += 3;
            }
            (x, ..) => {
                out.push(x);
                i += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&out).to_string())
}

/* ================= los ficheros ================= */

/// Lo más grande que se manda: la API admite PDF de hasta 32 MB por petición e imágenes de 5 MB.
const MAX_PDF: usize = 20 * 1024 * 1024;
const MAX_IMAGEN: usize = 5 * 1024 * 1024;

/// Un documento arrastrado, leído: su nombre, su tipo, su contenido y su huella.
#[derive(Debug, Clone)]
pub struct Fichero {
    pub nombre: String,
    /// `application/pdf`, `image/jpeg`…
    pub tipo: &'static str,
    pub bytes: Vec<u8>,
    /// Huella del contenido: el mismo documento, con otro nombre, tiene la misma.
    pub huella: String,
}

/// El tipo de un fichero que se puede mandar (PDF o imagen), por su extensión.
fn tipo_de(ruta: &Path) -> Option<&'static str> {
    let ext = ruta.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Lee un fichero arrastrado, si es de los que valen (PDF, JPG, PNG, GIF o WebP) y no es demasiado grande.
pub fn leer(ruta: &Path) -> Result<Fichero, String> {
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let Some(tipo) = tipo_de(ruta) else {
        return Err(format!("{nombre}: solo se pueden leer PDF y fotos (JPG, PNG, GIF o WebP)."));
    };
    let bytes = std::fs::read(ruta).map_err(|e| format!("{nombre}: no se puede leer ({e})."))?;
    let max = if tipo == "application/pdf" { MAX_PDF } else { MAX_IMAGEN };
    if bytes.len() > max {
        return Err(format!("{nombre}: es demasiado grande (más de {} MB).", max / 1024 / 1024));
    }
    if bytes.is_empty() {
        return Err(format!("{nombre}: está vacío."));
    }
    let huella = huella(&bytes);
    Ok(Fichero { nombre, tipo, bytes, huella })
}

/// Las 12 primeras cifras del FNV-1a de 64 bits (el de `fuente::Huella`): para reconocer el mismo documento, no para
/// seguridad.
fn huella(bytes: &[u8]) -> String {
    crate::dominio::fuente::Huella::de(bytes).fnv[..12].to_string()
}

/// El nombre de la copia en `documentos/`: el original, sin caracteres raros, y la huella («recibo-taxi-1a2b….jpg»).
pub fn nombre_copia(f: &Fichero) -> String {
    let p = Path::new(&f.nombre);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let base = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut limpio = String::new();
    for c in base.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' };
        if !(c == '-' && (limpio.is_empty() || limpio.ends_with('-'))) {
            limpio.push(c);
        }
    }
    let limpio: String = limpio.trim_matches('-').chars().take(60).collect();
    let limpio = if limpio.is_empty() { "documento".to_string() } else { limpio };
    format!("{limpio}-{}.{ext}", f.huella)
}

/// Guarda la copia del documento en `documentos/` (si no estaba ya) y devuelve su nombre.
pub fn guardar_copia(dir: &Path, f: &Fichero) -> io::Result<String> {
    let nombre = nombre_copia(f);
    let carpeta = dir.join("documentos");
    std::fs::create_dir_all(&carpeta)?;
    let ruta = carpeta.join(&nombre);
    if !ruta.is_file() {
        almacen::escribir_atomico(&ruta, &f.bytes)?;
    }
    Ok(nombre)
}

/// ¿Ya está metido este documento (por su huella, en el adjunto de un gasto, una factura o una nómina)? Dice dónde.
pub fn ya_metido(d: &Datos, huella: &str) -> Option<String> {
    let suyo = |doc: &str| !doc.is_empty() && doc.contains(&format!("-{huella}."));
    if let Some(g) = d.gastos.iter().find(|g| suyo(&g.doc)) {
        return Some(format!("ya está en tus gastos ({}, {})", g.proveedor, fmt::fecha_corta(&g.fecha)));
    }
    if let Some(f) = d.facturas.iter().find(|f| suyo(&f.doc)) {
        return Some(format!("ya está en tus facturas ({}, {})", f.cliente.nombre, fmt::fecha_corta(&f.fecha)));
    }
    if let Some(n) = d.nominas.iter().find(|n| suyo(&n.doc)) {
        return Some(format!("ya está en tus nóminas ({} de {})", fmt::MESES.get((n.mes as usize).wrapping_sub(1)).unwrap_or(&"?"), n.anio));
    }
    None
}

/* ================= clasificar ================= */

/// Lo que ha salido de un documento: lo entendido (o por qué no) y lo que ha costado preguntar (en dólares).
#[derive(Debug)]
pub struct Leido {
    pub fichero: Fichero,
    pub resultado: Result<Entendido, String>,
    pub coste: f64,
}

/// Pregunta a Claude Haiku qué es el documento y, si es una nómina, un gasto o un ingreso, sus datos (dos preguntas
/// pequeñas); y lo comprueba. `nif` es el tuyo (para saber si la factura va a tu nombre) y `hoy`, la fecha de hoy.
/// Tarda unos segundos: va en segundo plano.
pub fn clasificar(modo: &Modo, f: Fichero, nif: &str, hoy: &str) -> Leido {
    let mut coste = 0.0;
    let resultado = (|| {
        let (v, c) = haiku::preguntar(modo, &f, &haiku::clase())?;
        coste += c;
        let (tipo, que) = entendido::clase(&v)?;
        let pregunta = match tipo {
            entendido::Tipo::Nomina => haiku::nomina(),
            entendido::Tipo::Gasto => haiku::gasto(),
            entendido::Tipo::Ingreso => haiku::ingreso(),
            entendido::Tipo::Otro => return Err(format!("No parece una nómina, un gasto ni un ingreso ({que}).")),
        };
        let (v, c) = haiku::preguntar(modo, &f, &pregunta)?;
        coste += c;
        entendido::entender(tipo, &v, que, nif, hoy)
    })();
    Leido { fichero: f, resultado, coste }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::{Gasto, Nomina};

    /// Una carpeta temporal con ficheros inventados, con los nombres raros que dan los terminales.
    fn carpeta(nombres: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gesshtor-pegar-{}", crate::dominio::uid()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in nombres {
            std::fs::write(dir.join(n), b"%PDF-1.4 inventado").unwrap();
        }
        dir
    }

    #[test]
    fn rutas_como_las_pega_cada_terminal() {
        let dir = carpeta(&["Nómina, marzo +.pdf", "tique.jpg", "otro (2).pdf"]);
        let d = dir.to_string_lossy().to_string();
        let a = dir.join("Nómina, marzo +.pdf");
        let b = dir.join("tique.jpg");
        let c = dir.join("otro (2).pdf");
        let escapada = |p: &Path| p.to_string_lossy().replace(' ', "\\ ").replace('(', "\\(").replace(')', "\\)").replace(',', "\\,");
        let casos: Vec<(String, Vec<&PathBuf>)> = vec![
            // macOS (Terminal, iTerm): los espacios y lo raro, escapados; varios, separados por un espacio.
            (format!("{} {}", escapada(&a), escapada(&b)), vec![&a, &b]),
            // Entre comillas simples (GNOME) o dobles (Windows Terminal), con un espacio al final.
            (format!("'{}' \"{}\" ", a.display(), c.display()), vec![&a, &c]),
            // Como URL, con los %XX (el «+» es un «+»), uno por línea.
            (format!("file://{}/N%C3%B3mina,%20marzo%20+.pdf\nfile://localhost{}/tique.jpg\n", d, d), vec![&a, &b]),
            // Una línea entera con espacios sin escapar.
            (format!("{}\n", c.display()), vec![&c]),
            // El mismo dos veces: una.
            (format!("{} {}", escapada(&b), escapada(&b)), vec![&b]),
        ];
        for (texto, esperado) in casos {
            let (si, no) = rutas_pegadas(&texto);
            assert_eq!(si.iter().collect::<Vec<_>>(), esperado, "{texto}");
            assert!(no.is_empty(), "{texto}: {no:?}");
        }
        // Lo que no es un fichero se dice; un texto cualquiera no da ninguna ruta.
        let (si, no) = rutas_pegadas(&format!("{} {}/no-existe.pdf", escapada(&b), d));
        assert_eq!((si.len(), no.len()), (1, 1));
        assert!(no[0].ends_with("no-existe.pdf"));
        let (si, _) = rutas_pegadas("hola, esto es texto");
        assert!(si.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn modo_y_como_encenderlo() {
        assert_eq!(modo_de(Some("claude"), None), Ok(Modo::Claude));
        assert_eq!(modo_de(Some(" API "), Some("sk-x")), Ok(Modo::Api("sk-x".into())));
        assert!(modo_de(Some("api"), Some(" ")).unwrap_err().contains("ANTHROPIC_API_KEY"));
        assert_eq!(modo_de(None, Some("sk-x")).unwrap_err(), COMO_ENCENDER);
        assert!(modo_de(Some("sí"), None).unwrap_err().contains("no vale"));
    }

    #[test]
    fn ficheros_huella_y_copia() {
        let dir = carpeta(&["Mi tique (1).PDF", "nota.txt", "foto.heic"]);
        let f = leer(&dir.join("Mi tique (1).PDF")).unwrap();
        assert_eq!(f.tipo, "application/pdf");
        assert_eq!(f.huella.len(), 12);
        // La misma de siempre (va en los nombres de las copias): FNV-1a de 64 bits, sus 12 primeras cifras.
        assert_eq!(huella(b"hola"), "4029fbcc7e6d");
        assert_eq!(nombre_copia(&f), format!("Mi-tique-1-{}.pdf", f.huella));
        assert!(leer(&dir.join("nota.txt")).unwrap_err().contains("solo se pueden leer PDF"));
        assert!(leer(&dir.join("foto.heic")).is_err());
        // La copia va a documentos/ y la app la encuentra; un documento con esa huella ya está metido.
        let nombre = guardar_copia(&dir, &f).unwrap();
        assert!(almacen::documento(&dir, &nombre).is_some());
        let mut d = Datos::default();
        assert!(ya_metido(&d, &f.huella).is_none());
        d.gastos.push(Gasto { proveedor: "Taxi Inventado".into(), fecha: "2026-01-02".into(), doc: nombre.clone(), ..Default::default() });
        assert!(ya_metido(&d, &f.huella).unwrap().contains("gastos"));
        d.gastos.clear();
        d.nominas.push(Nomina { anio: 2025, mes: 3, doc: nombre, ..Default::default() });
        assert!(ya_metido(&d, &f.huella).unwrap().contains("marzo de 2025"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Con documentos de verdad y Claude de verdad (tarda y cuesta unos céntimos):
    ///
    ///     CLASIFICAR=carpeta [PANEL_CLASIFICAR=api] [SOLO=gasto0] cargo test clasificar_de_verdad -- --ignored --nocapture
    ///
    /// La carpeta tiene `nominas/`, `gastos/` e `ingresos/` (lo que debería salir) y, si quieres, `esperado.json`
    /// con los campos clave de cada fichero («gastos/x.pdf»: {"fecha": …, "total": …}). Saca una tabla sin datos de
    /// nadie: cada fichero por su número, qué debería ser, qué ha salido, los campos clave bien o mal y el coste. Lo
    /// que ha leído, con los importes, va aparte (a stderr), para revisarlo.
    #[test]
    #[ignore]
    fn clasificar_de_verdad() {
        let Ok(dir) = std::env::var("CLASIFICAR") else { return };
        let dir = PathBuf::from(dir);
        let modo = modo().unwrap_or(Modo::Claude);
        let solo = std::env::var("SOLO").unwrap_or_default();
        let nif = std::env::var("NIF").unwrap_or_default();
        let esperado: serde_json::Value = std::fs::read_to_string(dir.join("esperado.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut trabajos = Vec::new();
        for (carpeta, tipo) in [("nominas", "nómina"), ("gastos", "gasto"), ("ingresos", "ingreso")] {
            let mut v: Vec<PathBuf> = std::fs::read_dir(dir.join(carpeta)).map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
            v.sort();
            for (k, p) in v.into_iter().enumerate() {
                let clave = format!("{carpeta}/{}", p.file_name().unwrap().to_string_lossy());
                if clave.contains(&solo) {
                    trabajos.push((format!("{tipo} {:02}", k + 1), tipo, clave, p));
                }
            }
        }
        let hoy = fmt::hoy();
        let mut filas = Vec::new();
        for tanda in trabajos.chunks(4) {
            let hilos: Vec<_> = tanda
                .iter()
                .cloned()
                .map(|(alias, tipo, clave, p)| {
                    let (modo, nif, hoy) = (modo.clone(), nif.clone(), hoy.clone());
                    std::thread::spawn(move || {
                        let l = leer(&p).map(|f| clasificar(&modo, f, &nif, &hoy));
                        (alias, tipo, clave, l)
                    })
                })
                .collect();
            filas.extend(hilos.into_iter().map(|h| h.join().unwrap()));
        }
        let mut total = 0.0;
        println!("| Fichero | Esperado | Obtenido | Campos clave | Coste |\n|---|---|---|---|---|");
        for (alias, tipo, clave, l) in filas {
            let (obtenido, campos, coste) = match l {
                Err(e) => ("error".to_string(), e, 0.0),
                Ok(l) => {
                    total += l.coste;
                    match &l.resultado {
                        Err(e) => {
                            eprintln!("{alias}: {e}");
                            ("sin clasificar".to_string(), corto_error(e), l.coste)
                        }
                        Ok(e) => {
                            eprintln!("{alias}: {:?} avisos={:?} moneda={:?}", e.apunte, e.avisos, e.moneda);
                            let (t, c) = comparar(e, &esperado[&clave]);
                            (t.to_string(), c, l.coste)
                        }
                    }
                }
            };
            println!("| {alias} | {tipo} | {obtenido} | {campos} | {coste:.4} $ |");
        }
        println!("\nTotal: {total:.4} $");
    }

    /// El motivo de un error, sin importes ni nombres: hasta los dos puntos o el primer número.
    fn corto_error(e: &str) -> String {
        e.split([':', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9']).next().unwrap_or(e).trim().to_string()
    }

    /// Qué tipo ha salido y, campo a campo, si es lo esperado («fecha bien, total mal…»).
    fn comparar(e: &Entendido, esp: &serde_json::Value) -> (&'static str, String) {
        use serde_json::json;
        let mut v: Vec<String> = Vec::new();
        let mut mira = |campo: &str, valor: serde_json::Value| {
            let x = &esp[campo];
            let bien = match (x, &valor) {
                (serde_json::Value::Null, _) => return,
                (serde_json::Value::Array(a), _) => a.contains(&valor),
                (serde_json::Value::Number(a), serde_json::Value::Number(b)) => (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 0.015,
                (serde_json::Value::Bool(true), serde_json::Value::String(s)) => !s.is_empty(),
                (a, b) => a == b,
            };
            v.push(format!("{campo} {}", if bien { "bien" } else { "MAL" }));
        };
        let t = match &e.apunte {
            Apunte::Nomina(n) => {
                for (c, x) in [("anio", json!(n.anio)), ("mes", json!(n.mes)), ("dinerario", json!(n.dinerario)), ("especie", json!(n.especie)), ("retenciones", json!(n.retenciones)), ("ss", json!(n.ss))] {
                    mira(c, x);
                }
                "nómina"
            }
            Apunte::Gasto(g) => {
                let total = if g.origen == "es" { g.base.v() + g.cuota.0.unwrap_or(fmt::r2(g.base.v() * g.iva_tipo.v() / 100.0)) } else { g.base.v() };
                let moneda = e.moneda.as_ref().map_or("EUR".to_string(), |m| m.0.clone());
                for (c, x) in [("fecha", json!(g.fecha)), ("total", json!(fmt::r2(total))), ("moneda", json!(moneda)), ("origen", json!(g.origen)), ("iva_no_ded", json!(g.iva_no_ded)), ("categoria", json!(g.categoria))] {
                    mira(c, x);
                }
                "gasto"
            }
            Apunte::Ingreso(f) => {
                let base: f64 = f.lineas.iter().map(|l| l.cantidad.v() * l.precio.v()).sum();
                let moneda = e.moneda.as_ref().map_or("EUR".to_string(), |m| m.0.clone());
                for (c, x) in [("fecha", json!(f.fecha)), ("base", json!(fmt::r2(base))), ("moneda", json!(moneda)), ("operacion", json!(f.operacion)), ("estado", json!(f.estado)), ("cobrada", json!(f.cobrada.clone().unwrap_or_default()))] {
                    mira(c, x);
                }
                "ingreso"
            }
        };
        (t, v.join(", "))
    }
}
