//! Lo traído o leído de fuera, cada cosa en su fichero de `fuentes/` (ver `almacen`): lo leído de cada PDF de
//! Hacienda, lo presentado cada año según Hacienda y lo de la Seguridad Social. Todos tienen la misma forma,
//! `Fuente`: de dónde sale, cuándo y con qué se leyó por última vez, el último dato bueno y, si el último intento
//! falló, por qué (`Error`, con su informe: ver `fallo`). Se pueden borrar sin perder nada que no se pueda volver a
//! traer o a leer.
//!
//! La regla: un fallo nunca pisa el último dato bueno (`Fuente::encima`).
pub use super::fallo::Error;
use super::{renta::Renta, FormularioLeido};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Fuente<T> {
    /// De dónde sale: «justificantes/2025_1T_303_CSV.pdf», «Seguridad Social»…
    pub origen: String,
    /// La versión del lector con la que se leyó (solo los PDF; si cambia, se vuelve a leer).
    #[serde(skip_serializing_if = "es_cero")]
    pub lector: u32,
    /// La huella del PDF que se leyó (si cambia, se vuelve a leer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub huella: Option<Huella>,
    /// La app que lo leyó o lo trajo («0.1.0 · 1a2b3c4d5e6f»).
    pub gesshtor: String,
    /// Cuándo se leyó o se trajo por última vez («2026-10-15T09:05:30»).
    pub cuando: String,
    /// Cuándo se presentó, según la portada del PDF (solo los PDF): de varios del mismo periodo, vale el último.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub presentada: String,
    /// El último dato bueno (aunque el último intento fallara).
    pub dato: Option<T>,
    /// Si el último intento falló (en todo o en parte), por qué.
    pub error: Option<Error>,
}

impl<T> Default for Fuente<T> {
    fn default() -> Self {
        Fuente { origen: String::new(), lector: 0, huella: None, gesshtor: String::new(), cuando: String::new(), presentada: String::new(), dato: None, error: None }
    }
}

fn es_cero(n: &u32) -> bool {
    *n == 0
}

impl<T> Fuente<T> {
    /// Un intento de ahora (`cuando`, con esta versión de la app): lo que trajo y, si falló (en todo o en parte),
    /// por qué.
    pub fn nueva(origen: impl Into<String>, cuando: &str, dato: Option<T>, error: Option<Error>) -> Self {
        Fuente { origen: origen.into(), gesshtor: crate::VERSION.into(), cuando: cuando.into(), dato, error, ..Default::default() }
    }

    /// Este intento, guardado sobre el de antes: si no trae dato (ha fallado), se queda el último dato bueno (con
    /// su fecha de presentación, que va con él).
    pub fn encima(self, antes: Option<Fuente<T>>) -> Fuente<T> {
        match antes {
            Some(a) if self.dato.is_none() && a.dato.is_some() => Fuente { dato: a.dato, presentada: a.presentada, ..self },
            _ => self,
        }
    }
}

/// La huella de un fichero: su tamaño y un resumen de su contenido (FNV-1a de 64 bits, en hexadecimal). Si
/// cambia cualquiera de los dos, el fichero es otro.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Huella {
    pub tamano: u64,
    pub fnv: String,
}

impl Huella {
    pub fn de(bytes: &[u8]) -> Huella {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Huella { tamano: bytes.len() as u64, fnv: format!("{h:016x}") }
    }
}

/// Lo leído de un PDF de Hacienda: una Renta o una declaración con casillas.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Leido {
    Renta(Box<Renta>),
    Formulario(FormularioLeido),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_fallo_no_pisa_el_ultimo_dato_bueno() {
        let bien = Fuente { presentada: "2025-04-15T10:00:00".into(), ..Fuente::nueva("x", "2026-01-01T00:00:00", Some(1), None) };
        let mal = Fuente::<i32>::nueva("x", "2026-02-01T00:00:00", None, Some(Error::de("no contesta")));
        let f = mal.clone().encima(Some(bien.clone()));
        assert_eq!((f.dato, f.cuando.as_str(), f.error.as_ref().map(|e| e.mensaje.as_str())), (Some(1), "2026-02-01T00:00:00", Some("no contesta")));
        assert_eq!(f.presentada, "2025-04-15T10:00:00", "con su fecha de presentación");
        // Lo bueno sustituye a lo de antes (y se olvida el fallo); sin nada antes, el fallo se queda solo.
        assert_eq!(bien.clone().encima(Some(f)), bien);
        assert_eq!(mal.clone().encima(None), mal);
    }

    #[test]
    fn huellas() {
        assert_eq!(Huella::de(b""), Huella { tamano: 0, fnv: "cbf29ce484222325".into() });
        assert_eq!(Huella::de(b"a").fnv, "af63dc4c8601ec8c");
        assert_ne!(Huella::de(b"ab"), Huella::de(b"ba"));
    }
}
