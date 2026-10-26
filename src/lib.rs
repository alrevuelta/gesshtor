//! gesshtor: tus impuestos y tu Seguridad Social, como asalariado y como autónomo, en la terminal. Con tu
//! certificado consulta Hacienda y la Seguridad Social (nunca presenta nada) y guarda todo en una carpeta
//! (`datos.json` y los PDF de Hacienda en `justificantes/`).
//!
//! - `dominio`: los datos de una persona (también lo traído de Hacienda y la Seguridad Social), su carpeta y cómo se
//!   escriben importes y fechas.
//! - `hacienda`: todo lo de Hacienda: lo presentado y sus PDF, leerlos y los cálculos (IVA, IRPF, 303 y 130).
//! - `tgss`: la Seguridad Social: tus datos de autónomo y las cotizaciones de tus nóminas.
//! - `red`: lo que comparten las dos: el certificado y la conexión.
//! - `sincronizar`: traer lo último de las dos con el certificado.
//! - `informe`: las cifras que enseñan las pantallas.
//! - `tui`: la app de terminal.
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
