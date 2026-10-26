//! Leer y guardar los datos de una persona: una carpeta con `datos.json` (ver `dominio`), sus PDF en
//! `justificantes/`, sus documentos en `documentos/` y `cache.json` (lo leído de los PDF, que se puede rehacer).
//!
//! Para que no se pierda nada: se escribe en un temporal, se fuerza a disco y se renombra (o todo o nada); una
//! sola instancia por carpeta (candado del sistema, que se suelta solo si la app se cierra o se cuelga); y
//! copias que rotan en `copias/`.
use crate::dominio::{Datos, VERSION};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Copias que se guardan en `copias/` (las más viejas se borran).
const COPIAS: usize = 20;

pub fn ruta_datos(dir: &Path) -> PathBuf {
    dir.join("datos.json")
}

pub fn cargar(dir: &Path) -> io::Result<Datos> {
    let txt = fs::read_to_string(ruta_datos(dir))?;
    let mal = |e: String| io::Error::new(io::ErrorKind::InvalidData, e);
    let version = serde_json::from_str::<serde_json::Value>(&txt).map_err(|e| mal(format!("datos.json no válido: {e}")))?.get("version").and_then(|v| v.as_u64());
    if version != Some(VERSION as u64) {
        return Err(mal(format!("datos.json es de otra versión ({}): esta app usa la {VERSION}", version.map_or("?".into(), |v| v.to_string()))));
    }
    serde_json::from_str(&txt).map_err(|e| mal(format!("datos.json no válido: {e}")))
}

/// Guarda de forma atómica. La primera vez de cada sesión deja una copia de lo que había.
pub fn guardar(dir: &Path, d: &Datos, primera: &mut bool) -> io::Result<()> {
    let ruta = ruta_datos(dir);
    if *primera && ruta.exists() {
        let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
        copiar(dir, &ruta, ms)?;
        *primera = false;
    }
    let txt = serde_json::to_string_pretty(d).map_err(io::Error::other)?;
    escribir_atomico(&ruta, txt.as_bytes())
}

/// Escribe en un temporal al lado, lo fuerza a disco y lo renombra: nunca queda un fichero a medias.
pub fn escribir_atomico(ruta: &Path, bytes: &[u8]) -> io::Result<()> {
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = ruta.with_file_name(format!(".{nombre}.tmp"));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, ruta)
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

    #[test]
    fn escribe_entero_y_rota_copias() {
        let dir = std::env::temp_dir().join(format!("gesshtor-prueba-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("datos.json");
        escribir_atomico(&f, b"{}").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "{}");
        for i in 0..(COPIAS as u128 + 5) {
            copiar(&dir, &f, 1000 + i).unwrap();
        }
        assert_eq!(fs::read_dir(dir.join("copias")).unwrap().count(), COPIAS);
        let c = bloquear(&dir).unwrap();
        assert!(bloquear(&dir).is_err(), "una segunda instancia no puede abrir la misma carpeta");
        drop(c);
        assert!(bloquear(&dir).is_ok());
        // Se guarda y se vuelve a cargar igual; uno de otra versión no se abre.
        let mut d = Datos::default();
        d.perfil.nombre = "Ana".into();
        guardar(&dir, &d, &mut false).unwrap();
        assert_eq!(cargar(&dir).unwrap().perfil.nombre, "Ana");
        escribir_atomico(&f, br#"{"version": 1}"#).unwrap();
        assert!(cargar(&dir).unwrap_err().to_string().contains("otra versión (1)"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
