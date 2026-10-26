//! «Este año»: el año en curso, que aún no tiene Renta. Sus cifras, separadas por de dónde salen: lo que consta en
//! Hacienda y en la Seguridad Social, lo que has apuntado y lo que se estima (también, si se puede, la Renta).
use crate::dominio::fmt::DIF;
use super::anio::{self, Concepto, Detalle, Fila, Fuente, Origen, OrigenCuota};
use super::empleo;
use crate::dominio::Datos;
use crate::dominio::fmt::{self, eur};
use crate::hacienda;


/// Lo que saldría en la Renta del año.
#[derive(Debug, Clone, PartialEq)]
pub enum Prevision {
    /// > 0 a pagar, < 0 a devolver (redondeado a la decena), y con qué se ha calculado.
    Estimada { resultado: f64, supuestos: String },
    /// No se puede estimar: por qué.
    NoSe(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EsteAnio {
    pub y: i32,
    /// Lo que llevas ganado, antes de impuestos.
    pub ganado: f64,
    /// El sueldo es aproximado (lo cotizado).
    pub sueldo_aprox: bool,
    /// El IRPF adelantado: lo retenido en tu nómina y por tus clientes, y tus 130.
    pub adelantado: f64,
    /// Tiene nómina, pero no hay nóminas apuntadas: falta lo retenido.
    pub falta_nomina: bool,
    /// Lo cotizado: tu parte de la nómina y tu cuota de autónomo.
    pub cotizado: f64,
    /// Las cifras: primero lo que consta, luego lo apuntado y al final lo estimado.
    pub filas: Vec<Fila>,
    pub renta: Prevision,
    pub notas: Vec<String>,
}

impl EsteAnio {
    pub fn vacio(&self) -> bool {
        self.filas.is_empty()
    }
}

/// El año `y` (el actual), sin Renta todavía.
pub fn este_anio(d: &Datos, y: i32) -> EsteAnio {
    let det = anio::detalle(d, y);
    let a = &det.anio;
    let mut filas: Vec<Fila> = det.ganado.filas.clone();
    filas.extend(det.irpf.filas.iter().map(|f| Fila { concepto: Some(Concepto::Irpf), ..f.clone() }));
    if let Some(n) = det.nomina.as_ref().filter(|n| n.con_nomina > 0) {
        let ss = Fuente::Texto(format!("Seguridad Social · {}", n.texto_meses()));
        let sol = if a.tu_solidaridad > DIF { format!("{} de solidaridad", eur(a.tu_solidaridad)) } else { String::new() };
        filas.push(Fila { nombre: "Seguridad Social: tu parte de la nómina".into(), importe: Some(n.cuota.trabajador), nota: sol, fuentes: vec![ss.clone()], concepto: Some(Concepto::TuNomina), ..Default::default() });
        let nota = "su parte de la Seguridad Social: no sale de tu sueldo".to_string();
        filas.push(Fila { nombre: "Tu empresa pagó por ti".into(), importe: Some(n.cuota.empresa), nota, fuentes: vec![ss], aparte: true, ..Default::default() });
    }
    if let Some(c) = &det.cuota {
        let origen = if c.origen == OrigenCuota::Gastos { Origen::Apuntado } else { Origen::Consta };
        filas.push(Fila { nombre: "Seguridad Social: tu cuota de autónomo".into(), importe: Some(c.total), fuentes: vec![Fuente::Texto(c.de.clone())], concepto: Some(Concepto::Cuota), origen, ..Default::default() });
    }
    filas.sort_by_key(|f| f.origen);
    let renta = prevision(d, y, &det);
    let mut notas: Vec<String> = det.notas.iter().filter(|n| !n.starts_with("No hay datos")).cloned().collect();
    // La Renta del año pasado, mientras se puede presentar.
    if let Some(fin) = super::renta_pendiente(d, y - 1)
        && !anio::anio(d, y - 1).vacio()
    {
        notas.push(format!("Tu Renta de {} se presenta hasta el {}: hasta entonces, {} sale en Años como pendiente.", y - 1, fmt::fecha_corta(&fin), y - 1));
    }
    let cotizado = a.seguridad_social();
    EsteAnio { y, ganado: a.ganas, sueldo_aprox: a.sueldo_aprox, adelantado: det.irpf.total, falta_nomina: a.falta_nomina, cotizado, filas, renta, notas }
}

/// Lo que saldría en la Renta: solo con sueldo (por tus nóminas apuntadas, como «¿Te retienen bien?»).
fn prevision(d: &Datos, y: i32, det: &Detalle) -> Prevision {
    let (ini, fin) = hacienda::plazo_renta(y);
    let cuando = format!("la Renta de {y} se presenta del {} al {}", fmt::fecha_corta(&ini), fmt::fecha_corta(&fin));
    if det.anio.actividad.is_some() || det.cuota.is_some() || !anio::presentados130(d, y).is_empty() {
        return Prevision::NoSe(format!("Con actividad de autónomo la app aún no la estima: sale al hacerla ({cuando})."));
    }
    let meses = empleo::meses_sueldo(d, &empleo::empleo_anio(d, y));
    match empleo::retencion(d, &meses, y) {
        Some(r) => Prevision::Estimada { resultado: r.renta(), supuestos: r.supuestos(y) },
        None if det.nomina.as_ref().is_some_and(|n| n.con_nomina > 0) => {
            Prevision::NoSe("Para estimarla, apunta tus nóminas en Empleo: lo que te retienen solo viene en ellas.".into())
        }
        None => Prevision::NoSe(format!("Aún no hay ingresos de {y} con los que estimarla.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::{Gasto, Nomina};

    #[test]
    fn separa_lo_que_consta_lo_apuntado_y_lo_estimado() {
        let y = fmt::anio_actual();
        let mut d = Datos::default();
        // Solo nóminas apuntadas: todo es apuntado y la Renta se estima.
        for mes in 1..=2 {
            d.nominas.push(Nomina { id: format!("n{mes}"), anio: y, empresa: "Una".into(), mes, dinerario: 2500.0, retenciones: 300.0, ss: 160.0, ..Default::default() });
        }
        let e = este_anio(&d, y);
        assert_eq!(e.ganado, 5000.0);
        assert_eq!(e.adelantado, 600.0);
        assert!(e.filas.iter().all(|f| f.origen == Origen::Apuntado), "{:?}", e.filas);
        let Prevision::Estimada { supuestos, .. } = &e.renta else { panic!("{:?}", e.renta) };
        assert!(supuestos.starts_with("Calculado para"), "{supuestos}");
        // Con actividad de autónomo no se estima.
        d.gastos.push(Gasto { id: "g".into(), fecha: format!("{y}-01-15"), categoria: "ss".into(), base: crate::dominio::Num::de(300.0), ..Default::default() });
        let e = este_anio(&d, y);
        assert!(matches!(&e.renta, Prevision::NoSe(t) if t.starts_with("Con actividad de autónomo")), "{:?}", e.renta);
        let origenes: Vec<Origen> = e.filas.iter().map(|f| f.origen).collect();
        assert!(origenes.is_sorted(), "{origenes:?}");
    }
}
