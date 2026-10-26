//! Lo que saldría en la Renta del año en curso, que aún no tiene: solo con sueldo (por tus nóminas apuntadas, como
//! «¿Te retienen bien?»). Se ve en «Años», al abrir ese año.
use super::anio::{self, Detalle};
use super::empleo;
use crate::dominio::Datos;
use crate::dominio::fmt;
use crate::hacienda;

/// Lo que saldría en la Renta del año.
#[derive(Debug, Clone, PartialEq)]
pub enum Prevision {
    /// > 0 a pagar, < 0 a devolver (redondeado a la decena), y con qué se ha calculado.
    Estimada { resultado: f64, supuestos: String },
    /// No se puede estimar: por qué.
    NoSe(String),
}

/// Lo que saldría en la Renta: solo con sueldo (por tus nóminas apuntadas, como «¿Te retienen bien?»).
pub fn prevision(d: &Datos, y: i32, det: &Detalle) -> Prevision {
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
    fn con_nominas_se_estima_y_con_actividad_no() {
        let y = fmt::anio_actual();
        let mut d = Datos::default();
        let prev = |d: &Datos| prevision(d, y, &anio::detalle(d, y));
        assert!(matches!(prev(&d), Prevision::NoSe(t) if t.starts_with("Aún no hay ingresos")));
        // Solo nóminas apuntadas: la Renta se estima.
        for mes in 1..=2 {
            d.nominas.push(Nomina { id: format!("n{mes}"), anio: y, empresa: "Una".into(), mes, dinerario: 2500.0, retenciones: 300.0, ss: 160.0, ..Default::default() });
        }
        let Prevision::Estimada { supuestos, .. } = prev(&d) else { panic!("{:?}", prev(&d)) };
        assert!(supuestos.starts_with("Calculado para"), "{supuestos}");
        // Con actividad de autónomo no se estima.
        d.gastos.push(Gasto { id: "g".into(), fecha: format!("{y}-01-15"), categoria: "ss".into(), base: crate::dominio::Num::de(300.0), ..Default::default() });
        assert!(matches!(prev(&d), Prevision::NoSe(t) if t.starts_with("Con actividad de autónomo")), "{:?}", prev(&d));
    }
}
