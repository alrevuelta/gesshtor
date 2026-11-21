//! Los conectores con brókeres: cada uno, en su fichero, entra en la web de su bróker y traduce lo que trae al
//! formato común (`dominio::inversiones::Cartera`). No calculan nada: el cálculo fiscal (`informe::inversiones`) y
//! la pantalla solo conocen ese formato.
//!
//! **Para añadir uno** (Interactive Brokers, un CSV…): un fichero `brokers/<id>.rs` con su `pub const CONECTOR`
//! (qué se pide para entrar, con qué dominios habla y su función `traer`), su `mod` aquí y su línea en
//! `CONECTORES`. Si habla con la red, sus dominios van en `red/cliente.rs` y en la lista de con quién habla la app
//! (README y AVANZADO.md).
//!
//! Todos van desactivados: la app no habla con un bróker hasta que activas su conector (`traer` lo comprueba), y
//! lo que se pide para entrar (usuario, contraseña, código) se pide cada vez y no se guarda nunca.
pub mod degiro;

use crate::dominio::fuente::Error;
use crate::dominio::inversiones::{Cartera, Inversiones};
use crate::red::cliente::Cliente;

/// Lo que hace un conector: con una conexión que solo puede ir a sus dominios y lo pedido (en el orden de
/// `Conector::pide`), trae todo lo de la cuenta, diciendo por dónde va.
pub type Traer = fn(Cliente, &[String], &mut dyn FnMut(String)) -> Result<Cartera, Error>;

/// Algo que se pide para entrar.
#[derive(Debug, Clone, Copy)]
pub struct Pide {
    pub etiqueta: &'static str,
    /// No se ve al escribirlo (la contraseña).
    pub secreto: bool,
    /// Sirve una sola vez (un código de verificación): si falla, se vuelve a pedir vacío.
    pub una_vez: bool,
    pub ayuda: &'static str,
}

/// Un conector.
#[derive(Debug, Clone, Copy)]
pub struct Conector {
    /// Su nombre en `datos.json` y en `fuentes/` («degiro» → `fuentes/degiro.json`): letras minúsculas y guiones.
    pub id: &'static str,
    /// Cómo se llama el bróker («DEGIRO»).
    pub nombre: &'static str,
    /// Los únicos dominios con los que habla (ver `red::cliente`).
    pub dominios: &'static [&'static str],
    /// Lo que se le dice a la persona antes de activarlo: qué hace y qué riesgos tiene.
    pub aviso: &'static [&'static str],
    /// Lo que se pide para entrar, en orden.
    pub pide: &'static [Pide],
    /// Para el 720: dónde guarda el bróker tus valores y tu efectivo (qué entidad, en qué país) y de dónde se saca.
    pub deposito: &'static str,
    /// Trae todo lo de la cuenta, con lo pedido (en el orden de `pide`), por una conexión que solo puede ir a sus
    /// dominios. `paso` dice por dónde va. Todo o nada: si falla algo, un error que diga qué esperaba y qué llegó.
    pub traer: Traer,
}

/// Los que hay.
pub const CONECTORES: &[Conector] = &[degiro::CONECTOR];

pub fn conector(id: &str) -> Option<&'static Conector> {
    CONECTORES.iter().find(|c| c.id == id)
}

/// De dónde sale lo traído (`Fuente::origen`).
pub fn origen(c: &Conector) -> String {
    format!("{}: {}, con tu usuario", c.nombre, c.dominios.join(", "))
}

/// Trae lo de un bróker, solo si su conector está activado. La conexión solo puede ir a sus dominios.
pub fn traer(inversiones: &Inversiones, c: &Conector, datos: &[String], paso: &mut dyn FnMut(String)) -> Result<Cartera, Error> {
    if !inversiones.activado(c.id) {
        return Err(Error::de(format!("La conexión con {} está desactivada: actívala antes.", c.nombre)));
    }
    if datos.len() != c.pide.len() {
        return Err(Error::de(format!("Para entrar en {} hacen falta {} datos.", c.nombre, c.pide.len())));
    }
    (c.traer)(Cliente::new(None, c.dominios), datos, paso)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_conector_bien_puesto() {
        for c in CONECTORES {
            assert!(!c.id.is_empty() && c.id.chars().all(|x| x.is_ascii_lowercase() || x == '-'), "{}", c.id);
            assert!(!c.dominios.is_empty() && !c.aviso.is_empty() && !c.pide.is_empty() && !c.deposito.is_empty(), "{}", c.id);
            assert_eq!(CONECTORES.iter().filter(|o| o.id == c.id).count(), 1, "{} repetido", c.id);
        }
        assert!(conector("degiro").is_some() && conector("otro").is_none());
    }

    #[test]
    fn desactivado_no_sale_a_la_red() {
        let c = conector("degiro").unwrap();
        let mut pasos = Vec::new();
        let e = traer(&Inversiones::default(), c, &["a".into(), "b".into(), String::new()], &mut |p| pasos.push(p)).unwrap_err();
        assert!(e.mensaje.contains("desactivada"), "{}", e.mensaje);
        assert!(pasos.is_empty(), "ni lo intenta");
    }
}
