//! `cache.json`: lo leído de cada PDF, con su huella (nombre y tamaño) y la versión del lector, para no releerlo.
//! También se guardan los fallos: un PDF que no se puede leer no se vuelve a intentar en cada arranque (solo si
//! cambia el PDF o el lector). Se puede borrar sin perder nada: se rehace leyendo los PDF.
use super::lectura::VERSION_LECTOR;
use crate::dominio::{renta::Renta, FormularioLeido};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cache {
    /// `VERSION_LECTOR` con la que se leyó: si no es la de ahora, no vale nada.
    pub version: u32,
    /// Por nombre del PDF en `justificantes/`.
    pub pdfs: BTreeMap<String, Entrada>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrada {
    /// Tamaño del PDF en bytes: si cambia, se relee.
    pub tamano: u64,
    /// Cuándo se presentó, según su portada («2026-10-15T09:05:30»): si hay varios PDF del mismo periodo, vale el
    /// último.
    #[serde(default)]
    pub presentada: String,
    pub leido: Leido,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Leido {
    Renta(Box<Renta>),
    Formulario(FormularioLeido),
    /// No se ha podido leer: el motivo.
    Fallo(String),
}

pub fn ruta(dir: &Path) -> PathBuf {
    dir.join("cache.json")
}

/// La caché de la carpeta. Si no hay, no se entiende o es de otra versión del lector, una vacía.
pub fn cargar(dir: &Path) -> Cache {
    std::fs::read(ruta(dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
        .filter(|c| c.version == VERSION_LECTOR)
        .unwrap_or_else(|| Cache { version: VERSION_LECTOR, pdfs: BTreeMap::new() })
}

/// La guarda de una vez (o entera o nada).
pub fn guardar(dir: &Path, c: &Cache) -> io::Result<()> {
    let txt = serde_json::to_vec(c).map_err(io::Error::other)?;
    crate::dominio::almacen::escribir_atomico(&ruta(dir), &txt)
}
