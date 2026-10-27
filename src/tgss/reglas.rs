//! Las reglas de cotización de cada año, para la calculadora de sueldos: lo que pagan la empresa y el trabajador
//! por un sueldo (Régimen General) y lo que paga un autónomo según lo que gana (los tramos y el tipo). Para añadir
//! un año: copiar el último y cambiar lo que diga la Orden de cotización de ese año.
//!
//! Fuentes: la página de bases y tipos de la Seguridad Social (seg-social.es, «Bases y tipos de cotización»), las
//! Órdenes de cotización de cada año y, para los autónomos, el RDL 13/2022 (tablas de 2025, prorrogadas para 2026
//! por el RDL 3/2026 y la Orden PJC/297/2026).

/// Lo del Régimen General de un año. Los tipos, en %: (empresa, trabajador).
#[derive(Debug)]
pub struct General {
    pub anio: i32,
    /// Base máxima de cotización, al mes.
    pub base_maxima: f64,
    pub comunes: (f64, f64),
    pub desempleo_indefinido: (f64, f64),
    pub desempleo_temporal: (f64, f64),
    /// Fondo de Garantía Salarial: solo la empresa.
    pub fogasa: f64,
    pub formacion: (f64, f64),
    /// Mecanismo de Equidad Intergeneracional (sobre la base de contingencias comunes).
    pub mei: (f64, f64),
    /// Cotización adicional de solidaridad (desde 2025), por la parte del sueldo de cada mes que pasa de la base
    /// máxima: (hasta, en veces la base máxima; tipo total %). Se reparte entre empresa y trabajador como las
    /// contingencias comunes.
    pub solidaridad: &'static [(f64, f64)],
    pub norma: &'static str,
}

/// Accidentes de trabajo y enfermedades profesionales: depende de la actividad (la tarifa de la disposición
/// adicional 61.ª de la Ley General de la Seguridad Social). Este es el de los trabajos exclusivos de oficina
/// (código «a»: 0,65 % de incapacidad temporal y 0,85 % de incapacidad, muerte y supervivencia). Solo la empresa.
pub const AT_EP_OFICINA: f64 = 1.50;

/// Lo de los autónomos de un año.
#[derive(Debug)]
pub struct Autonomos {
    pub anio: i32,
    /// Tipo total, en %, y por qué.
    pub tipo: f64,
    pub desglose: &'static [(&'static str, f64)],
    /// Los tramos por rendimientos netos al mes: (hasta, base mínima, base máxima).
    pub tramos: &'static [(f64, f64, f64)],
    /// Lo que se descuenta de los rendimientos por gastos genéricos, en %, para elegir el tramo (7 %; 3 % los
    /// societarios).
    pub gastos_genericos: f64,
    pub norma: &'static str,
}

const GENERAL: &[General] = &[
    General {
        anio: 2025,
        base_maxima: 4909.50,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.67, 0.13),
        solidaridad: &[(1.10, 0.92), (1.50, 1.00), (f64::INFINITY, 1.17)],
        norma: "Orden PJC/178/2025",
    },
    General {
        anio: 2026,
        base_maxima: 5101.20,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.75, 0.15),
        solidaridad: &[(1.10, 1.15), (1.50, 1.25), (f64::INFINITY, 1.46)],
        norma: "Orden PJC/297/2026",
    },
];

/// Las bases mínimas de cada tramo, iguales en 2025 y 2026; la máxima de los dos últimos es la del año.
const fn tramos(maxima: f64) -> [(f64, f64, f64); 15] {
    [
        (670.0, 653.59, 718.94),
        (900.0, 718.95, 900.0),
        (1166.70, 849.67, 1166.70),
        (1300.0, 950.98, 1300.0),
        (1500.0, 960.78, 1500.0),
        (1700.0, 960.78, 1700.0),
        (1850.0, 1143.79, 1850.0),
        (2030.0, 1209.15, 2030.0),
        (2330.0, 1274.51, 2330.0),
        (2760.0, 1356.21, 2760.0),
        (3190.0, 1437.91, 3190.0),
        (3620.0, 1519.61, 3620.0),
        (4050.0, 1601.31, 4050.0),
        (6000.0, 1732.03, maxima),
        (f64::INFINITY, 1928.10, maxima),
    ]
}
const TRAMOS_2025: [(f64, f64, f64); 15] = tramos(4909.50);
const TRAMOS_2026: [(f64, f64, f64); 15] = tramos(5101.20);

const AUTONOMOS: &[Autonomos] = &[
    Autonomos {
        anio: 2025,
        tipo: 31.40,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.80)],
        tramos: &TRAMOS_2025,
        gastos_genericos: 7.0,
        norma: "RDL 13/2022",
    },
    Autonomos {
        anio: 2026,
        tipo: 31.50,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.90)],
        tramos: &TRAMOS_2026,
        gastos_genericos: 7.0,
        norma: "RDL 3/2026 y Orden PJC/297/2026, con las tablas de 2025 prorrogadas",
    },
];

/// Lo de un año en una lista ordenada: ese año o, si no lo tiene la app, el último de antes (o el primero).
fn del_anio<T>(v: &'static [T], anio: i32, de: impl Fn(&T) -> i32) -> &'static T {
    v.iter().rev().find(|x| de(x) <= anio).unwrap_or(&v[0])
}

/// Las del Régimen General de un año (si la app no las tiene, las del último que tiene antes, o las primeras).
pub fn general(anio: i32) -> &'static General {
    del_anio(GENERAL, anio, |g| g.anio)
}

/// Las de autónomos de un año (como `general`).
pub fn autonomos(anio: i32) -> &'static Autonomos {
    del_anio(AUTONOMOS, anio, |a| a.anio)
}

impl Autonomos {
    /// El tramo (desde 1) y su base mínima para unos rendimientos netos al mes.
    pub fn tramo(&self, mensual: f64) -> (usize, f64) {
        let i = self.tramos.iter().position(|t| mensual <= t.0).unwrap_or(self.tramos.len() - 1);
        (i + 1, self.tramos[i].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_de_cada_anio() {
        assert_eq!(general(2026).base_maxima, 5101.20);
        assert_eq!(general(2024).anio, 2025, "sin las de un año, las primeras");
        assert_eq!(general(2030).anio, 2026, "y después, las últimas");
        // Los tipos de cada parte suman lo que dice la Seguridad Social.
        for g in GENERAL {
            let total = |t: (f64, f64)| ((t.0 + t.1) * 100.0).round() / 100.0;
            assert_eq!(total(g.comunes), 28.30);
            assert_eq!(total(g.mei), if g.anio == 2025 { 0.80 } else { 0.90 });
        }
        for a in AUTONOMOS {
            let suma: f64 = a.desglose.iter().map(|d| d.1).sum();
            assert!((suma - a.tipo).abs() < 0.001, "{}: {suma}", a.anio);
            assert!(a.tramos.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1), "tramos ordenados");
        }
        let a = autonomos(2026);
        assert_eq!(a.tramo(600.0), (1, 653.59));
        assert_eq!(a.tramo(1500.0), (5, 960.78));
        assert_eq!(a.tramo(7000.0), (15, 1928.10));
    }
}
