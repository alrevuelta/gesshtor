//! Todo lo de Hacienda. Aquí, lo presentado (según la última consulta), sus PDF y los plazos; y en cada fichero:
//!
//! - **Cada modelo que lee la app, con todo lo suyo**: `modelo_100` (la Renta), `modelo_130`, `modelo_303` y
//!   `modelo_390`. Lo que tienen en común, en `modelo` (cuál
//!   es, sus periodos, por qué no se ha podido leer) y en `formulario` (lo que tiene que decir de sí mismo un
//!   modelo con casillas y cómo se lee uno cualquiera).
//! - `consulta`: preguntar a Hacienda lo presentado cada año (servicio documentado) y bajar sus PDF (por el CSV),
//!   con `xml` para leer sus respuestas. Es igual para todos los modelos.
//! - `lectura`: leer los PDF de `justificantes/` (cada uno con el lector de su modelo), con `pdf` (sacar el texto),
//!   `generico` (las casillas de cualquier modelo, tal como se ven) y `cache` (lo ya leído); las pruebas
//!   usan PDF hechos por Hacienda con datos inventados (`pruebas/pdf/`; la Renta, de momento, `pruebas/renta_*.json`).
//! - `irpf`: el IRPF de la actividad (acumulado desde el 1 de enero) y el de un sueldo, con las reglas de cada año.
//!   Las facturas y los gastos, con lo que se calcula con ellos, están en `dominio::factura` y `dominio::gasto`.
mod cache;
pub mod consulta;
pub mod formulario;
pub mod generico;
pub mod irpf;
pub mod lectura;
pub mod modelo;
pub mod modelo_100;
pub mod modelo_130;
pub mod modelo_303;
pub mod modelo_390;
pub mod pdf;
mod xml;

pub use crate::dominio::hacienda::{clave, Decl};
pub use modelo::{Modelo, Trimestral};
use crate::dominio::Datos;
use crate::dominio::fmt::{self, sumar_dias};
use chrono::{Datelike, Weekday};
use std::path::{Path, PathBuf};

/// Las declaraciones presentadas de un año (todas las que trajo la última consulta), por modelo y periodo.
pub fn declaraciones(d: &Datos, anio: i32) -> Vec<Decl> {
    let mut v: Vec<Decl> = d.hacienda.get(&anio).into_iter().flat_map(|h| h.modelos.values()).flat_map(|m| m.decl.iter().cloned()).collect();
    v.sort_by(|a, b| (a.modelo.as_str(), a.periodo.as_str(), a.fecha.as_str()).cmp(&(b.modelo.as_str(), b.periodo.as_str(), b.fecha.as_str())));
    v
}

/// Los años de los que hay consulta a Hacienda, de más nuevo a más viejo.
pub fn anios_consultados(d: &Datos) -> Vec<i32> {
    d.hacienda.keys().rev().copied().collect()
}

/// ¿Cuándo se consultó ese año a Hacienda? («2026-10-15T…»)
pub fn consultado(d: &Datos, anio: i32) -> Option<String> {
    d.hacienda.get(&anio).map(|h| h.fecha.clone()).filter(|x| !x.is_empty())
}

/// La última presentada de un modelo y periodo (q = 0: anual), si Hacienda la tiene.
pub fn presentada(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> Option<Decl> {
    let periodo = if q > 0 { format!("{q}T") } else { "0A".to_string() };
    declaraciones(d, anio).into_iter().filter(|x| x.modelo == modelo.codigo() && x.periodo == periodo).max_by(|a, b| a.fecha.cmp(&b.fecha))
}

/// ¿Consta presentado? Lo dice la consulta a Hacienda o un PDF suyo leído.
pub fn consta(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> bool {
    presentada(d, anio, q, modelo).is_some() || leida(d, anio, q, modelo)
}

/// ¿Se ha leído su PDF? (de la Renta, el del año)
pub fn leida(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> bool {
    match modelo {
        Modelo::M100 => d.lecturas.rentas.contains_key(&anio),
        _ => modelo.leido(d, anio, q).is_some(),
    }
}

/// El PDF de una declaración, si está en la carpeta de datos: el que se leyó (`lecturas`) o el justificante
/// descargado (en `justificantes/`, con su nombre: `lectura::nombre_pdf`).
pub fn pdf_de(dir: &Path, d: &Datos, x: &Decl) -> Option<PathBuf> {
    let j = dir.join("justificantes");
    if let Some(f) = d.lecturas.formularios.get(&x.clave()) {
        let leido_es_este = f.justificante.is_empty() || f.justificante == x.justificante;
        if !f.pdf.is_empty() && leido_es_este && j.join(&f.pdf).is_file() {
            return Some(j.join(&f.pdf));
        }
    }
    let exacto = j.join(lectura::nombre_pdf(x.ejercicio, &x.periodo, &x.modelo, &x.csv));
    if !x.csv.is_empty() && exacto.is_file() {
        return Some(exacto);
    }
    None
}

/// Cualquier PDF guardado de ese modelo y periodo (aunque no sepamos su CSV).
pub fn pdf_periodo(dir: &Path, anio: i32, q: u32, modelo: &str) -> Option<PathBuf> {
    let periodo = if q > 0 { format!("{q}T") } else { "0A".to_string() };
    let pref = lectura::prefijo_pdf(anio, &periodo, modelo);
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir.join("justificantes"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&pref) && n.ends_with(".pdf")))
        .collect();
    v.sort();
    v.pop()
}

/* ================= plazos ================= */

/// Si cae en sábado o domingo, el plazo pasa al lunes.
pub fn habil(fecha: &str) -> String {
    let mut f = fecha.to_string();
    while let Some(d) = fmt::fecha(&f) {
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            break;
        }
        f = sumar_dias(&f, 1);
    }
    f
}

#[derive(Debug, Clone)]
pub struct Plazo {
    pub anio: i32,
    pub q: u32,
    pub ini: String,
    pub fin: String,
    /// Último día para domiciliar el pago.
    pub domiciliar: String,
    pub modelos: Vec<Modelo>,
}

/// Los plazos de los trimestres de un año (303 y 130; el 4T, también el 390).
pub fn plazos_trimestres(anio: i32) -> Vec<Plazo> {
    (1..=4)
        .map(|q| {
            let (ini, fin, domiciliar) = if q == 4 {
                (format!("{}-01-01", anio + 1), habil(&format!("{}-01-30", anio + 1)), format!("{}-01-25", anio + 1))
            } else {
                let m = q * 3 + 1;
                (format!("{anio}-{m:02}-01"), habil(&format!("{anio}-{m:02}-20")), format!("{anio}-{m:02}-15"))
            };
            let modelos = if q == 4 { vec![Modelo::M303, Modelo::M130, Modelo::M390] } else { vec![Modelo::M303, Modelo::M130] };
            Plazo { anio, q, ini, fin, domiciliar, modelos }
        })
        .collect()
}

/// Plazo de la Renta de un año: del 1 de abril al 30 de junio del siguiente.
pub fn plazo_renta(anio: i32) -> (String, String) {
    (format!("{}-04-01", anio + 1), habil(&format!("{}-06-30", anio + 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plazos() {
        // 20 de julio de 2025 fue domingo: pasa al lunes 21.
        assert_eq!(plazos_trimestres(2025)[1].fin, "2025-07-21");
        assert_eq!(plazos_trimestres(2026)[3].fin, "2027-02-01"); // 30 ene 2027 es sábado
        assert_eq!(clave(2026, 3, "303"), "2026-3T-303");
        assert_eq!(clave(2025, 0, "100"), "2025-100");
    }
}
