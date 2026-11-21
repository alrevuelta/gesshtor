//! gesshtor: tus impuestos y tu Seguridad Social, como asalariado y como autónomo, en la terminal. Con tu
//! certificado consulta Hacienda y la Seguridad Social (nunca presenta nada) y guarda todo en una carpeta
//! (`datos.json` con lo que apuntas, lo traído y lo leído en `fuentes/` y los PDF de Hacienda en `justificantes/`).
//!
//! - `dominio`: los datos de una persona (también lo traído de Hacienda y la Seguridad Social), su carpeta y cómo se
//!   escriben importes y fechas.
//! - `hacienda`: todo lo de Hacienda: lo presentado y sus PDF, leerlos y los cálculos (IVA, IRPF, 303 y 130).
//! - `tgss`: la Seguridad Social: tus datos de autónomo y las cotizaciones de tus nóminas.
//! - `brokers`: los conectores con brókeres (DEGIRO): cada uno trae lo de tu cuenta, en un formato común.
//! - `red`: lo que comparten todos: el certificado y la conexión.
//! - `sincronizar`: traer lo último de las dos con el certificado.
//! - `clasificar`: si lo enciendes, Claude Haiku dice qué son los documentos que arrastras y saca sus datos.
//! - `informe`: las cifras que enseñan las pantallas.
//! - `tui`: la app de terminal.
// El `Error` de lo que no se entiende lleva todo su informe (`dominio::fallo`): es grande a propósito, y solo se crea
// cuando algo falla.
#![allow(clippy::result_large_err)]
pub mod brokers;
pub mod clasificar;
pub mod dominio;
pub mod hacienda;
pub mod informe;
pub mod red;
pub mod sincronizar;
pub mod tgss;
pub mod tui;

/// La versión y el commit del que sale el ejecutable («0.1.0 · 1a2b3c4d5e6f», «-sucio» si había cambios sin
/// commit): lo pone `build.rs`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " · ", env!("GESSHTOR_COMMIT"));
