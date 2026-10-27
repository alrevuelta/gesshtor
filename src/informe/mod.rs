//! Lo que enseñan las pantallas, calculado aparte (funciones puras: datos → cifras). Las pantallas solo pintan.
pub mod anio;
pub mod autonomo;
pub mod calculadora;
pub mod empleo;
pub mod inicio;
pub mod prevision;
pub mod reparto;

use crate::dominio::{renta::Renta, Datos, PRIMER_ANIO};
use crate::dominio::fmt::anio_actual;

/// Qué es la persona (para enseñar unas secciones u otras).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Situacion {
    /// Tiene o tuvo nómina.
    pub empleo: bool,
    /// Es o fue autónoma.
    pub autonomo: bool,
}

/// Lo que se ve en sus datos (si no se ve nada, las dos cosas).
pub fn situacion(d: &Datos) -> Situacion {
    let rentas = || d.lecturas.rentas.values();
    let s = Situacion {
        empleo: !d.nominas.is_empty() || d.tgss.nomina.meses.values().any(|l| !l.is_empty()) || rentas().any(|r| r.trabajo.as_ref().is_some_and(|t| t.integros > 0.0)),
        autonomo: !d.alta_autonomo().0.is_empty() || !d.facturas.is_empty() || !d.gastos.is_empty() || d.tgss.reta || rentas().any(|r| r.actividad.is_some()),
    };
    if s.empleo || s.autonomo { s } else { Situacion { empleo: true, autonomo: true } }
}

/// La Renta de un año, si se ha leído su PDF.
pub fn renta_de(d: &Datos, anio: i32) -> Option<&Renta> {
    d.lecturas.rentas.get(&anio)
}

/// Si la Renta de un año ya acabado aún no está pero se puede presentar: hasta cuándo («2026-06-30»).
pub fn renta_pendiente(d: &Datos, anio: i32) -> Option<String> {
    let (_, fin) = crate::hacienda::plazo_renta(anio);
    (anio < anio_actual() && renta_de(d, anio).is_none() && crate::dominio::fmt::hoy() <= fin).then_some(fin)
}

/// ¿Se puede apuntar cosas en ese año (nóminas, facturas, gastos)? El año en curso, y el anterior mientras no
/// tenga Renta. Los demás son de solo lectura: se ven con lo que dicen Hacienda y la Seguridad Social.
pub fn editable(d: &Datos, anio: i32) -> bool {
    let hoy = anio_actual();
    anio == hoy || (anio == hoy - 1 && renta_de(d, anio).is_none())
}

/// El primer año del que hay algo (Rentas, cotizaciones, recibos, facturas…), para las series «año a año»;
/// nunca antes de `PRIMER_ANIO`.
pub fn primer_anio(d: &Datos) -> i32 {
    let hoy = anio_actual();
    let mut v = hoy;
    if let Some(a) = d.lecturas.rentas.keys().next() {
        v = v.min(*a);
    }
    if let Some(ym) = d.tgss.nomina.meses.iter().find(|(_, l)| !l.is_empty()).map(|(k, _)| k) {
        v = v.min(crate::dominio::fmt::anio_de(ym));
    }
    if let Some(r) = d.tgss.recibos.iter().map(|r| crate::dominio::fmt::anio_de(&r.periodo)).filter(|a| *a > 0).min() {
        v = v.min(r);
    }
    for f in d.facturas.iter().map(|f| f.fecha.as_str()).chain(d.gastos.iter().map(|g| g.fecha.as_str())) {
        let a = crate::dominio::fmt::anio_de(f);
        if a > 0 {
            v = v.min(a);
        }
    }
    v.max(PRIMER_ANIO)
}
