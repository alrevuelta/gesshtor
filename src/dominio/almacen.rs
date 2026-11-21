//! Leer y guardar los datos de una persona. Su carpeta:
//!
//! - `datos.json`: lo que apunta (ver `dominio`). Es lo único que no se puede rehacer.
//! - `fuentes/`: lo traído de Hacienda y la Seguridad Social y lo leído de los PDF, cada cosa en su fichero (ver
//!   `dominio::fuente`): `tgss.json`, `aeat-2025-consulta.json` (lo presentado ese año) y uno por PDF leído
//!   (`aeat-2025-1T-303-CSV.json`, ver `hacienda::lectura`) y lo de cada bróker conectado (`degiro.json`). Se puede
//!   borrar: se vuelve a traer o a leer.
//! - `justificantes/` (los PDF de Hacienda), `documentos/` (los tuyos) y `copias/`.
//!
//! Para que no se pierda nada: se escribe en un temporal, se fuerza a disco, se renombra (o todo o nada) y se fuerza
//! a disco también la carpeta, para que el renombrado sobreviva a un corte de luz; una sola instancia por carpeta
//! (candado del sistema, que se suelta solo si la app se cierra o se cuelga); y, la primera vez que se guarda en
//! cada sesión, una copia de lo que había en `copias/`, donde se quedan las últimas 20.
use crate::dominio::fuente::{Error, Fuente};
use crate::dominio::hacienda::Consulta;
use crate::dominio::inversiones::Cartera;
use crate::dominio::tgss::Tgss;
use crate::dominio::{Datos, VERSION};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Copias que se guardan en `copias/` (las más viejas se borran).
const COPIAS: usize = 20;

/// Las migraciones: de cada versión anterior de la carpeta, el paso que la deja en la siguiente (con su
/// `datos.json`). Una versión nueva es subir `dominio::VERSION` y añadir aquí su paso; `migrar` los encadena.
const MIGRACIONES: &[(u32, Paso)] = &[(2, de_la_2_a_la_3)];

/// Un paso de `MIGRACIONES`: deja la carpeta en la versión siguiente.
type Paso = fn(&Path) -> io::Result<()>;

/// Lo de la Seguridad Social, en `fuentes/`.
pub const TGSS: &str = "tgss.json";

pub fn ruta_datos(dir: &Path) -> PathBuf {
    dir.join("datos.json")
}

pub fn ruta_fuentes(dir: &Path) -> PathBuf {
    dir.join("fuentes")
}

/// El fichero de `fuentes/` con lo presentado en un año: «aeat-2025-consulta.json».
pub fn consulta(anio: i32) -> String {
    format!("aeat-{anio}-consulta.json")
}

/// El fichero de `fuentes/` con lo traído de un bróker: «degiro.json».
fn cartera(conector: &str) -> String {
    format!("{conector}.json")
}

/// Los datos de una carpeta: `datos.json` y lo traído, de `fuentes/` (con lo que falló la última vez, o no se
/// entiende, en `fallos`). Uno de la versión 2 se lee tal cual (lo traído está dentro), sin tocar nada: se
/// convierte al abrirlo en la app (`migrar`).
pub fn cargar(dir: &Path) -> io::Result<Datos> {
    let (mut d, version) = leer_datos(dir)?;
    if version == VERSION {
        d.hacienda = consultas(dir, &mut d.fallos);
        d.tgss = dato(abrir_fuente(dir, TGSS), TGSS, &mut d.fallos).unwrap_or_default();
        // Lo de cada bróker activado, entero (la pantalla enseña también cuándo y el último fallo).
        let id_valido = |id: &&String| id.chars().all(|c| c.is_ascii_lowercase() || c == '-');
        let ids: Vec<String> = d.inversiones.conectores.keys().filter(id_valido).cloned().collect();
        for id in ids {
            let nombre = cartera(&id);
            match abrir_fuente(dir, &nombre) {
                Ok(Some(f)) => {
                    if let Some(e) = &f.error {
                        d.fallos.insert(nombre, e.clone());
                    }
                    d.carteras.insert(id, f);
                }
                Ok(None) => {}
                Err(e) => {
                    d.fallos.insert(nombre, e);
                }
            }
        }
    }
    super::gasto::con_recibos(&mut d);
    Ok(d)
}

/// `datos.json` y su versión (esta o la 2; otra, no).
fn leer_datos(dir: &Path) -> io::Result<(Datos, u32)> {
    let txt = fs::read_to_string(ruta_datos(dir))?;
    let version = version_de(&txt)?;
    let d = serde_json::from_str(&txt).map_err(|e| mal(format!("datos.json no válido: {e}")))?;
    Ok((d, version))
}

/// La versión de un `datos.json`: esta o una que se sepa poner al día (`MIGRACIONES`); otra, no se abre.
fn version_de(txt: &str) -> io::Result<u32> {
    #[derive(Deserialize)]
    struct Version {
        version: Option<u32>,
    }
    let version = serde_json::from_str::<Version>(txt).map_err(|e| mal(format!("datos.json no válido: {e}")))?.version;
    version.filter(|v| *v == VERSION || MIGRACIONES.iter().any(|(m, _)| m == v)).ok_or_else(|| {
        mal(format!("datos.json es de otra versión ({}): esta app usa la {VERSION}", version.map_or("?".into(), |v| v.to_string())))
    })
}

fn mal(e: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// Lo presentado de cada año, de `fuentes/aeat-AAAA-consulta.json`.
fn consultas(dir: &Path, fallos: &mut BTreeMap<String, Error>) -> BTreeMap<i32, Consulta> {
    let Ok(it) = fs::read_dir(ruta_fuentes(dir)) else { return BTreeMap::new() };
    it.filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| {
            let anio = n.strip_prefix("aeat-")?.strip_suffix("-consulta.json")?.parse().ok()?;
            Some((anio, dato(abrir_fuente(dir, &n), &n, fallos)?))
        })
        .collect()
}

/// El último dato bueno de una fuente; lo que falló la última vez (o que no se entiende), a `fallos`.
fn dato<T>(f: Result<Option<Fuente<T>>, Error>, nombre: &str, fallos: &mut BTreeMap<String, Error>) -> Option<T> {
    match f {
        Ok(Some(f)) => {
            if let Some(e) = f.error {
                fallos.insert(nombre.into(), e);
            }
            f.dato
        }
        Ok(None) => None,
        Err(e) => {
            fallos.insert(nombre.into(), e);
            None
        }
    }
}

/// Pone al día una carpeta de una versión anterior, paso a paso (`MIGRACIONES`): antes, una copia de `datos.json`
/// en `copias/` («antes-de-la-vN-…», que no se borra nunca). Cada paso deja la carpeta en la versión siguiente, así
/// que si se corta a medias, la próxima vez sigue desde donde se quedó. Con el candado puesto. ¿Había algo que
/// poner al día?
pub fn migrar(dir: &Path) -> io::Result<bool> {
    let leer_version = || version_de(&fs::read_to_string(ruta_datos(dir))?);
    let mut version = leer_version()?;
    if version == VERSION {
        return Ok(false);
    }
    let copias = dir.join("copias");
    fs::create_dir_all(&copias)?;
    fs::copy(ruta_datos(dir), copias.join(format!("antes-de-la-v{VERSION}-{}.json", ahora_ms())))?;
    while version != VERSION {
        let paso = MIGRACIONES.iter().find(|(v, _)| *v == version).map(|(_, paso)| paso);
        paso.ok_or_else(|| mal(format!("No sé poner al día datos.json desde la versión {version}.")))?(dir)?;
        let nueva = leer_version()?;
        if nueva <= version {
            return Err(mal(format!("La migración de la versión {version} no ha subido la versión de datos.json.")));
        }
        version = nueva;
    }
    Ok(true)
}

/// De la 2 a la 3: lo traído (`hacienda`, `tgss`) sale de `datos.json` a `fuentes/`, y `cache.json` (lo leído de los
/// PDF) se borra: se vuelve a leer, a `fuentes/`.
fn de_la_2_a_la_3(dir: &Path) -> io::Result<()> {
    let (d, _) = leer_datos(dir)?;
    if d.tgss != Tgss::default() {
        guardar_tgss(dir, &d.tgss.consultado, Some(&d.tgss), None)?;
    }
    for (anio, c) in &d.hacienda {
        let cuando = if c.fecha.is_empty() { c.modelos.values().map(|m| m.fecha.as_str()).max().unwrap_or("") } else { c.fecha.as_str() };
        guardar_consulta(dir, *anio, cuando, Some(c), None)?;
    }
    guardar(dir, &Datos { version: 3, ..d }, &mut false)?;
    match fs::remove_file(dir.join("cache.json")) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Lo guardado en `fuentes/<nombre>`, si hay y se entiende (si no, como si no hubiera: se vuelve a traer o a leer).
pub fn leer_fuente<T: DeserializeOwned>(dir: &Path, nombre: &str) -> Option<Fuente<T>> {
    abrir_fuente(dir, nombre).ok().flatten()
}

/// Lo guardado en `fuentes/<nombre>`: nada si no está; un fallo si está pero no se puede leer o no se entiende.
pub fn abrir_fuente<T: DeserializeOwned>(dir: &Path, nombre: &str) -> Result<Option<Fuente<T>>, Error> {
    let ruta = ruta_fuentes(dir).join(nombre);
    let bytes = match fs::read(&ruta) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        r => r,
    };
    let mal = |que: String| {
        Error::formato("FUENTE_ILEGIBLE", crate::donde!("abrir_fuente"), format!("No se entiende fuentes/{nombre}: no se usa (se vuelve a traer o a leer)."))
            .en(format!("fuentes/{nombre}"))
            .esperado("un JSON como los que escribe gesshtor (`dominio::fuente::Fuente`)")
            .encontrado(que)
            .reproducir(format!("abre fuentes/{nombre} (o `gesshtor json tgss <carpeta>`); si se borra, se vuelve a traer o a leer"))
    };
    let bytes = bytes.map_err(|e| mal(e.to_string()))?;
    serde_json::from_slice(&bytes).map(Some).map_err(|e| mal(e.to_string()))
}

/// Guarda `fuentes/<nombre>` de una vez (como `datos.json`), legible.
pub fn guardar_fuente<T: Serialize>(dir: &Path, nombre: &str, f: &Fuente<T>) -> io::Result<()> {
    let carpeta = ruta_fuentes(dir);
    fs::create_dir_all(&carpeta)?;
    let txt = serde_json::to_string_pretty(f).map_err(io::Error::other)?;
    escribir_atomico(&carpeta.join(nombre), txt.as_bytes())
}

/// Apunta lo que se ha traído (`dato`) o por qué no (`error`; las dos cosas si fue a medias) en `fuentes/<nombre>`:
/// un fallo no pisa el último dato bueno. Si lo que había no se entiende, se aparta antes («<nombre>.ilegible»).
fn apuntar<T: Serialize + DeserializeOwned + Clone>(dir: &Path, nombre: &str, origen: String, cuando: &str, dato: Option<&T>, error: Option<&Error>) -> io::Result<()> {
    let antes = abrir_fuente(dir, nombre).unwrap_or_else(|_| {
        let ruta = ruta_fuentes(dir).join(nombre);
        let _ = fs::rename(&ruta, ruta.with_file_name(format!("{nombre}.ilegible")));
        None
    });
    let f = Fuente::nueva(origen, cuando, dato.cloned(), error.cloned()).encima(antes);
    guardar_fuente(dir, nombre, &f)
}

/// Lo traído de la Seguridad Social, a `fuentes/tgss.json` (ver `apuntar`).
pub fn guardar_tgss(dir: &Path, cuando: &str, dato: Option<&Tgss>, error: Option<&Error>) -> io::Result<()> {
    apuntar(dir, TGSS, "Seguridad Social: Importass y la Sede, con tu certificado".into(), cuando, dato, error)
}

/// Lo presentado en un año según Hacienda, a `fuentes/aeat-AAAA-consulta.json` (ver `apuntar`).
pub fn guardar_consulta(dir: &Path, anio: i32, cuando: &str, dato: Option<&Consulta>, error: Option<&Error>) -> io::Result<()> {
    apuntar(dir, &consulta(anio), format!("Hacienda: lo presentado en {anio}, con tu certificado"), cuando, dato, error)
}

/// Lo traído de un bróker, a `fuentes/<conector>.json` (ver `apuntar`). Devuelve cómo ha quedado (lo de antes si ha
/// fallado).
pub fn guardar_cartera(dir: &Path, conector: &str, origen: &str, cuando: &str, dato: Option<&Cartera>, error: Option<&Error>) -> io::Result<Fuente<Cartera>> {
    apuntar(dir, &cartera(conector), origen.into(), cuando, dato, error)?;
    leer_fuente(dir, &cartera(conector)).ok_or_else(|| io::Error::other("no se puede volver a leer lo guardado"))
}

/// Guarda `datos.json` de forma atómica. La primera vez de cada sesión deja una copia de lo que había. Si no ha
/// cambiado nada, ni lo escribe ni hace copia (así una sesión sin cambios no gasta una de las 20).
pub fn guardar(dir: &Path, d: &Datos, primera: &mut bool) -> io::Result<()> {
    let ruta = ruta_datos(dir);
    let txt = serde_json::to_string_pretty(d).map_err(io::Error::other)?;
    if fs::read(&ruta).is_ok_and(|antes| antes == txt.as_bytes()) {
        return Ok(());
    }
    if *primera && ruta.exists() {
        copiar(dir, &ruta, ahora_ms())?;
        *primera = false;
    }
    escribir_atomico(&ruta, txt.as_bytes())
}

fn ahora_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

/// Escribe en un temporal al lado, lo fuerza a disco y lo renombra: nunca queda un fichero a medias. Después fuerza
/// a disco la carpeta, para que el renombrado no se pierda si se va la luz (en Windows no hace falta). Cada
/// escritura usa su temporal: dos hilos pueden escribir a la vez el mismo fichero (gana el último).
pub fn escribir_atomico(ruta: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = ruta.with_file_name(format!(".{nombre}.{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let escrito = (|| {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, ruta)
    })();
    if escrito.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    escrito?;
    #[cfg(unix)]
    if let Some(carpeta) = ruta.parent() {
        // Lo mejor que se puede: algunos sistemas de ficheros no lo permiten, y el fichero ya está escrito.
        let carpeta = if carpeta.as_os_str().is_empty() { Path::new(".") } else { carpeta };
        let _ = File::open(carpeta).and_then(|c| c.sync_all());
    }
    Ok(())
}

fn copiar(dir: &Path, ruta: &Path, ms: u128) -> io::Result<()> {
    let copias = dir.join("copias");
    fs::create_dir_all(&copias)?;
    fs::copy(ruta, copias.join(format!("datos-{ms}.json")))?;
    let mut v: Vec<PathBuf> = fs::read_dir(&copias)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("datos-") && n.ends_with(".json")))
        .collect();
    v.sort();
    let sobran = v.len().saturating_sub(COPIAS);
    for p in v.into_iter().take(sobran) {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

/// El candado de una carpeta: mientras exista, nadie más la abre. Se suelta solo al cerrar (o colgarse) la app.
#[derive(Debug)]
pub struct Candado(#[allow(dead_code)] File);

pub fn bloquear(dir: &Path) -> io::Result<Candado> {
    let f = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(".panel.lock"))?;
    match f.try_lock() {
        Ok(()) => Ok(Candado(f)),
        Err(fs::TryLockError::WouldBlock) => Err(io::Error::new(io::ErrorKind::WouldBlock, "Ya hay otra ventana de la app abierta con estos datos.")),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Las personas de una carpeta: si tiene `datos.json`, es una; si no, cada subcarpeta que lo tenga
/// (por ejemplo `mock/ana`, `mock/luis`). Ordenadas por nombre.
pub fn perfiles(dir: &Path) -> Vec<PathBuf> {
    if ruta_datos(dir).is_file() {
        return vec![dir.to_path_buf()];
    }
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| ruta_datos(p).is_file()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Un documento adjunto (factura, tique, justificante) de la carpeta de datos, si existe.
pub fn documento(dir: &Path, nombre: &str) -> Option<PathBuf> {
    if nombre.is_empty() || nombre.contains("..") || nombre.contains('/') {
        return None;
    }
    ["documentos", "justificantes", "presentaciones"]
        .iter()
        .map(|c| dir.join(c).join(nombre))
        .find(|p| p.is_file())
}

/// Abre un fichero con la app del sistema (Vista Previa en el Mac).
pub fn abrir(ruta: &Path) -> io::Result<()> {
    let prog = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
    std::process::Command::new(prog)
        .arg(ruta)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::hacienda::{Decl, Modelo};
    use crate::dominio::{Cliente, Factura, Linea, Num};

    /// Una carpeta temporal, que se borra al acabar.
    struct Carpeta(PathBuf);

    impl Carpeta {
        fn nueva(nombre: &str) -> Self {
            let d = std::env::temp_dir().join(format!("gesshtor-almacen-{nombre}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            Carpeta(d)
        }
        fn hay(&self, ruta: &str) -> bool {
            self.0.join(ruta).exists()
        }
    }

    impl Drop for Carpeta {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn escribe_entero_y_rota_copias() {
        let c = Carpeta::nueva("copias");
        let dir = &c.0;
        let f = dir.join("datos.json");
        escribir_atomico(&f, b"{}").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "{}");
        assert_eq!(fs::read_dir(dir).unwrap().count(), 1, "sin temporales");
        for i in 0..(COPIAS as u128 + 5) {
            copiar(dir, &f, 1000 + i).unwrap();
        }
        assert_eq!(fs::read_dir(dir.join("copias")).unwrap().count(), COPIAS);
        let candado = bloquear(dir).unwrap();
        assert!(bloquear(dir).is_err(), "una segunda instancia no puede abrir la misma carpeta");
        drop(candado);
        assert!(bloquear(dir).is_ok());
        // Se guarda y se vuelve a cargar igual; uno de otra versión no se abre.
        let mut d = Datos::default();
        d.perfil.nombre = "Ana".into();
        guardar(dir, &d, &mut false).unwrap();
        assert_eq!(cargar(dir).unwrap().perfil.nombre, "Ana");
        escribir_atomico(&f, br#"{"version": 1}"#).unwrap();
        assert!(cargar(dir).unwrap_err().to_string().contains("otra versión (1)"));
    }

    #[test]
    fn sin_cambios_no_se_escribe_ni_se_copia() {
        let c = Carpeta::nueva("sin-cambios");
        let mut d = Datos::default();
        guardar(&c.0, &d, &mut true).unwrap();
        let mut primera = true;
        guardar(&c.0, &d, &mut primera).unwrap();
        assert!(primera && !c.hay("copias"), "igual que lo que había: ni copia");
        d.perfil.nombre = "Ana".into();
        guardar(&c.0, &d, &mut primera).unwrap();
        assert!(!primera && fs::read_dir(c.0.join("copias")).unwrap().count() == 1, "con un cambio, la copia de lo de antes");
        assert_eq!(cargar(&c.0).unwrap().perfil.nombre, "Ana");
    }

    /// Lo presentado en 2025 (inventado) y lo de la Seguridad Social, como lo trae la app.
    fn traido() -> (Consulta, Tgss) {
        let decl = Decl { modelo: "303".into(), ejercicio: 2025, periodo: "1T".into(), csv: "PRUEBA303".into(), fecha: "2025-04-15T10:00:00".into(), ..Default::default() };
        let consulta = Consulta { nif: "00000000T".into(), fecha: "2026-10-21T10:00:00".into(), modelos: [("303".to_string(), Modelo { fecha: "2026-10-21T10:00:00".into(), decl: vec![decl] })].into(), ..Default::default() };
        let mut tgss = Tgss { reta: true, consultado: "2026-10-21T10:00:00".into(), ..Default::default() };
        tgss.recibos.push(crate::dominio::tgss::Recibo { periodo: "2026-09".into(), importe: 300.0, ..Default::default() });
        (consulta, tgss)
    }

    #[test]
    fn lo_traido_va_a_fuentes_y_un_fallo_no_lo_pisa() {
        let c = Carpeta::nueva("fuentes");
        let (consulta, tgss) = traido();
        guardar(&c.0, &Datos::default(), &mut false).unwrap();
        guardar_consulta(&c.0, 2025, "2026-10-21T10:00:00", Some(&consulta), None).unwrap();
        guardar_tgss(&c.0, "2026-10-21T10:00:00", Some(&tgss), None).unwrap();
        let d = cargar(&c.0).unwrap();
        assert_eq!((d.hacienda.get(&2025), &d.tgss), (Some(&consulta), &tgss));
        assert_eq!(d.gastos.len(), 1, "el recibo, entre los gastos");
        // Ni lo traído ni los recibos van a datos.json.
        let txt = fs::read_to_string(ruta_datos(&c.0)).unwrap();
        assert!(!txt.contains("\"tgss\"") && !txt.contains("\"hacienda\"") && !txt.contains("Seguridad Social"), "{txt}");
        // Falla la Seguridad Social: se apunta por qué y se queda lo de antes.
        guardar_tgss(&c.0, "2026-10-22T10:00:00", None, Some(&Error::de("No contesta."))).unwrap();
        let f: Fuente<Tgss> = leer_fuente(&c.0, TGSS).unwrap();
        assert_eq!((f.cuando.as_str(), f.error.map(|e| e.mensaje)), ("2026-10-22T10:00:00", Some("No contesta.".to_string())));
        let d = cargar(&c.0).unwrap();
        assert_eq!(d.tgss, tgss);
        assert_eq!(d.fallos.get(TGSS).map(|e| e.mensaje.as_str()), Some("No contesta."), "y el fallo, a la vista");
        // Un año que nunca se pudo consultar: el error, sin dato (y no cuenta como consultado).
        guardar_consulta(&c.0, 2024, "2026-10-22T10:00:00", None, Some(&Error::de("No contesta."))).unwrap();
        assert!(c.hay("fuentes/aeat-2024-consulta.json"));
        let d = cargar(&c.0).unwrap();
        assert_eq!(d.hacienda.keys().collect::<Vec<_>>(), [&2025]);
        assert_eq!(d.fallos.keys().collect::<Vec<_>>(), ["aeat-2024-consulta.json", TGSS]);
        // Uno que no se entiende no se usa, y se dice.
        fs::write(c.0.join("fuentes/tgss.json"), b"{ roto").unwrap();
        let d = cargar(&c.0).unwrap();
        assert_eq!(d.tgss, Tgss::default());
        assert_eq!(d.fallos[TGSS].codigo, "FUENTE_ILEGIBLE");
        assert!(d.fallos[TGSS].encontrado.contains("line 1"), "{:?}", d.fallos[TGSS]);
        // Al volver a traerlo, lo que no se entendía se aparta (no se pierde) y vale lo nuevo.
        guardar_tgss(&c.0, "2026-10-23T10:00:00", Some(&tgss), None).unwrap();
        assert!(c.hay("fuentes/tgss.json.ilegible"));
        let d = cargar(&c.0).unwrap();
        assert!(d.tgss == tgss && !d.fallos.contains_key(TGSS));
    }

    #[test]
    fn lo_de_un_broker_y_un_fallo_no_lo_pisa() {
        let c = Carpeta::nueva("broker");
        let mut d = Datos::default();
        d.inversiones.conectores.insert("prueba".into(), true);
        guardar(&c.0, &d, &mut false).unwrap();
        let cartera = Cartera { broker: "Bróker de prueba".into(), hasta: "2026-11-20".into(), ..Default::default() };
        let f = guardar_cartera(&c.0, "prueba", "Bróker de prueba", "2026-11-20T10:00:00", Some(&cartera), None).unwrap();
        assert_eq!(f.dato.as_ref(), Some(&cartera));
        assert_eq!(cargar(&c.0).unwrap().carteras["prueba"].dato.as_ref(), Some(&cartera), "se carga con los datos");
        // Falla: se apunta por qué y se queda lo de antes.
        let f = guardar_cartera(&c.0, "prueba", "Bróker de prueba", "2026-11-21T10:00:00", None, Some(&Error::de("DEGIRO ha cambiado."))).unwrap();
        assert_eq!((f.dato.as_ref(), f.cuando.as_str(), f.error.map(|e| e.mensaje)), (Some(&cartera), "2026-11-21T10:00:00", Some("DEGIRO ha cambiado.".into())));
        // Un conector con un nombre raro en datos.json no lee fuera de fuentes/.
        let mut d = cargar(&c.0).unwrap();
        d.inversiones.conectores.insert("../datos".into(), true);
        guardar(&c.0, &d, &mut false).unwrap();
        assert_eq!(cargar(&c.0).unwrap().carteras.len(), 1);
    }

    #[test]
    fn convierte_la_version_2() {
        let c = Carpeta::nueva("migrar");
        let (consulta, tgss) = traido();
        let mut v2 = serde_json::to_value(Datos { version: 2, ..Default::default() }).unwrap();
        v2["perfil"]["nombre"] = "Ana".into();
        v2["hacienda"] = serde_json::json!({ "2025": consulta });
        v2["tgss"] = serde_json::to_value(&tgss).unwrap();
        let antes = serde_json::to_string_pretty(&v2).unwrap();
        fs::write(ruta_datos(&c.0), &antes).unwrap();
        fs::write(c.0.join("cache.json"), b"{}").unwrap();
        // Sin convertir, se lee igual (sin tocar nada).
        let d = cargar(&c.0).unwrap();
        assert_eq!((d.hacienda.get(&2025), &d.tgss, d.perfil.nombre.as_str()), (Some(&consulta), &tgss, "Ana"));
        assert_eq!(fs::read_to_string(ruta_datos(&c.0)).unwrap(), antes);

        assert!(migrar(&c.0).unwrap());
        let copia: Vec<_> = fs::read_dir(c.0.join("copias")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        assert!(copia.len() == 1 && copia[0].starts_with("antes-de-la-v3-"), "{copia:?}");
        assert_eq!(fs::read_to_string(c.0.join("copias").join(&copia[0])).unwrap(), antes);
        assert!(c.hay("fuentes/tgss.json") && c.hay("fuentes/aeat-2025-consulta.json") && !c.hay("cache.json"));
        let ahora: serde_json::Value = serde_json::from_slice(&fs::read(ruta_datos(&c.0)).unwrap()).unwrap();
        assert!(ahora["version"] == VERSION && ahora.get("tgss").is_none() && ahora.get("hacienda").is_none());
        let f: Fuente<Consulta> = leer_fuente(&c.0, "aeat-2025-consulta.json").unwrap();
        assert_eq!((f.cuando.as_str(), f.dato.as_ref()), ("2026-10-21T10:00:00", Some(&consulta)));
        // Se lee lo mismo, y no hay nada más que convertir.
        let d2 = cargar(&c.0).unwrap();
        assert_eq!((d2.hacienda, d2.tgss, d2.perfil.nombre), (d.hacienda, d.tgss, d.perfil.nombre));
        assert!(!migrar(&c.0).unwrap());
    }

    /// ¿Escala un único `datos.json`? `cargo test --release mide_guardar_y_cargar -- --ignored --nocapture`:
    /// guardar y cargar con 200 y 5.000 facturas inventadas.
    #[test]
    #[ignore]
    fn mide_guardar_y_cargar() {
        for n in [200usize, 5000] {
            let c = Carpeta::nueva(&format!("mide-{n}"));
            let mut d = Datos::default();
            for i in 0..n {
                let cliente = Cliente { id: format!("c{}", i % 30), nombre: format!("Cliente {}", i % 30), nif: "B00000000".into(), pais: "ES".into(), direccion: "Calle Falsa 1".into(), cp: "28000".into(), ciudad: "Madrid".into(), email: "cliente@example.com".into(), operacion: "nacional".into() };
                let lineas = (1..=3).map(|j| Linea { concepto: format!("Servicio {j}"), cantidad: Num::de(j as f64), precio: Num::de(100.0 * j as f64), iva: Num::de(21.0) }).collect();
                let fecha = format!("2026-{:02}-10", i % 12 + 1);
                d.facturas.push(Factura { id: format!("f{i}"), estado: "emitida".into(), numero: format!("2026-{i:04}"), fecha: fecha.clone(), cliente, lineas, retencion: Num::de(15.0), forma_pago: "Transferencia".into(), creada: fecha.clone(), emitida: Some(fecha), ..Default::default() });
            }
            guardar(&c.0, &d, &mut false).unwrap();
            let kb = fs::metadata(ruta_datos(&c.0)).unwrap().len() / 1024;
            let veces = 20;
            let t = std::time::Instant::now();
            for i in 0..veces {
                d.perfil.nombre = format!("Ana {i}"); // si no cambia nada, no se escribe
                guardar(&c.0, &d, &mut false).unwrap();
            }
            let guardar_ms = t.elapsed().as_secs_f64() * 1000.0 / veces as f64;
            let t = std::time::Instant::now();
            for _ in 0..veces {
                assert_eq!(cargar(&c.0).unwrap().facturas.len(), n);
            }
            let cargar_ms = t.elapsed().as_secs_f64() * 1000.0 / veces as f64;
            println!("{n} facturas: datos.json de {kb} KB · guardar {guardar_ms:.1} ms · cargar {cargar_ms:.1} ms");
        }
    }
}
