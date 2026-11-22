//! Las reglas de cotización de cada año, para la calculadora de sueldos: lo que pagan la empresa y el trabajador
//! por un sueldo (Régimen General) y lo que paga un autónomo según lo que gana (los tramos y el tipo). Para añadir
//! un año: copiar el último y cambiar lo que diga la Orden de cotización de ese año.
//!
//! Fuentes: la página de bases y tipos de la Seguridad Social (seg-social.es, «Bases y tipos de cotización»), las
//! Órdenes de cotización de cada año (con la ley o el RDL que fija las bases: en 2020 no hubo Orden y siguió la de
//! 2019; la de 2021 es de diciembre, con los mismos valores) y, para los autónomos, las leyes de presupuestos hasta
//! 2022 y el RDL 13/2022 desde 2023 (sus tablas de 2025, prorrogadas para 2026 por el RDL 3/2026 y la Orden
//! PJC/297/2026). Comprobado año a año en el BOE y, casi todo, también en seg-social.es (no las cotizaciones de
//! autónomos de 2020, que su página nunca actualizó, ni el reparto de 2021 por meses, que su tabla resume en un 30,3 %
//! para todo el año). Los totales de la tarifa plana desde 2023 salen de la ley (80 € más el MEI de 950,98 €): no
//! hay una cifra publicada.

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
/// adicional 4.ª de la Ley 42/2006, con el RDL 28/2018; desde 2026, la 61.ª de la Ley General de la Seguridad
/// Social). Este es el de los trabajos exclusivos de oficina (código «a»: 0,80 % de incapacidad temporal y 0,70 % de
/// incapacidad, muerte y supervivencia), el mismo desde 2019. Solo la empresa.
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
    /// La tarifa plana de los nuevos autónomos: lo que se paga al mes los doce primeros meses (con lo que se pague
    /// aparte, como el MEI) y qué cubre; None si para ese año no hay una cuantía fijada.
    pub tarifa_plana: Option<f64>,
    pub tarifa_plana_que: &'static str,
    /// Algo del año que conviene saber (vacío si nada).
    pub nota: &'static str,
    pub norma: &'static str,
}

const GENERAL: &[General] = &[
    General {
        anio: 2020,
        base_maxima: 4070.10,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.0, 0.0),
        solidaridad: &[],
        norma: "RDL 18/2019, con la Orden TMS/83/2019 prorrogada",
    },
    General {
        anio: 2021,
        base_maxima: 4070.10,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.0, 0.0),
        solidaridad: &[],
        norma: "Ley 11/2020 y Orden PCM/1353/2021",
    },
    General {
        anio: 2022,
        base_maxima: 4139.40,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.0, 0.0),
        solidaridad: &[],
        norma: "Orden PCM/244/2022",
    },
    General {
        anio: 2023,
        base_maxima: 4495.50,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.50, 0.10),
        solidaridad: &[],
        norma: "Orden PCM/74/2023",
    },
    General {
        anio: 2024,
        base_maxima: 4720.50,
        comunes: (23.60, 4.70),
        desempleo_indefinido: (5.50, 1.55),
        desempleo_temporal: (6.70, 1.60),
        fogasa: 0.20,
        formacion: (0.60, 0.10),
        mei: (0.58, 0.12),
        solidaridad: &[],
        norma: "Orden PJC/51/2024",
    },
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

/// La tarifa plana de 2020 a 2022 (art. 31 de la Ley del Trabajo Autónomo, con el RDL 28/2018): una cuota única de
/// 60 €, solo con la base mínima.
const TARIFA_60: &str = "60 € de contingencias comunes (51,50 €) y profesionales (8,50 €); sin cese ni formación";
/// Desde 2023 (art. 38 ter y DT 5.ª del RDL 13/2022): 80 € por contingencias comunes y profesionales, más el MEI, que
/// no se reduce, de la base de esos meses (950,98 €): 5,71 € en 2023, 6,66 € en 2024 y 7,61 € en 2025.
const TARIFA_80: &str = "80 € de contingencias comunes y profesionales, más el MEI de una base de 950,98 €; sin cese ni formación";

/// Antes de 2023, sin tramos: uno solo, con las bases mínima y máxima de todos.
const fn sin_tramos(minima: f64, maxima: f64) -> [(f64, f64, f64); 1] {
    [(f64::INFINITY, minima, maxima)]
}
const SIN_TRAMOS_2020: [(f64, f64, f64); 1] = sin_tramos(944.40, 4070.10);
const SIN_TRAMOS_2022: [(f64, f64, f64); 1] = sin_tramos(960.60, 4139.40);

/// Los tramos de 2023 y 2024 (RDL 13/2022, DT 1.ª; con la base máxima de cada año en los dos últimos).
const TRAMOS_2023: [(f64, f64, f64); 15] = [
    (670.0, 751.63, 849.66),
    (900.0, 849.67, 900.0),
    (1166.70, 898.69, 1166.70),
    (1300.0, 950.98, 1300.0),
    (1500.0, 960.78, 1500.0),
    (1700.0, 960.78, 1700.0),
    (1850.0, 1013.07, 1850.0),
    (2030.0, 1029.41, 2030.0),
    (2330.0, 1045.75, 2330.0),
    (2760.0, 1078.43, 2760.0),
    (3190.0, 1143.79, 3190.0),
    (3620.0, 1209.15, 3620.0),
    (4050.0, 1274.51, 4050.0),
    (6000.0, 1372.55, 4495.50),
    (f64::INFINITY, 1633.99, 4495.50),
];
const TRAMOS_2024: [(f64, f64, f64); 15] = [
    (670.0, 735.29, 816.98),
    (900.0, 816.99, 900.0),
    (1166.70, 872.55, 1166.70),
    (1300.0, 950.98, 1300.0),
    (1500.0, 960.78, 1500.0),
    (1700.0, 960.78, 1700.0),
    (1850.0, 1045.75, 1850.0),
    (2030.0, 1062.09, 2030.0),
    (2330.0, 1078.43, 2330.0),
    (2760.0, 1111.11, 2760.0),
    (3190.0, 1176.47, 3190.0),
    (3620.0, 1241.83, 3620.0),
    (4050.0, 1307.19, 4050.0),
    (6000.0, 1454.25, 4720.50),
    (f64::INFINITY, 1732.03, 4720.50),
];

/// 2021 tuvo tres tipos: el 30,6 % en enero (Ley 11/2020), el 30,3 % de febrero a agosto (el RDL 2/2021 congeló la
/// subida de profesionales y cese hasta que subiera el salario mínimo) y otra vez el 30,6 % de septiembre a diciembre
/// (Orden PCM/1353/2021). Aquí, la media de los doce meses.
const PROFESIONALES_2021: f64 = (1.30 * 5.0 + 1.10 * 7.0) / 12.0;
const CESE_2021: f64 = (0.90 * 5.0 + 0.80 * 7.0) / 12.0;

const AUTONOMOS: &[Autonomos] = &[
    Autonomos {
        anio: 2020,
        tipo: 30.30,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.10), ("Cese de actividad", 0.80), ("Formación profesional", 0.10)],
        tramos: &SIN_TRAMOS_2020,
        gastos_genericos: 0.0,
        tarifa_plana: Some(60.0),
        tarifa_plana_que: TARIFA_60,
        nota: "Antes de 2023 no había tramos: cada uno elegía su base entre la mínima y la máxima (con 47 o más años, con límites).",
        norma: "RDL 18/2019, con lo del RDL 28/2018 prorrogado",
    },
    Autonomos {
        anio: 2021,
        tipo: 28.30 + PROFESIONALES_2021 + CESE_2021 + 0.10,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", PROFESIONALES_2021), ("Cese de actividad", CESE_2021), ("Formación profesional", 0.10)],
        tramos: &SIN_TRAMOS_2020,
        gastos_genericos: 0.0,
        tarifa_plana: Some(60.0),
        tarifa_plana_que: TARIFA_60,
        nota: "En 2021 se cotizó al 30,6 % en enero y de septiembre a diciembre, y al 30,3 % de febrero a agosto: aquí, la media del año. Antes de 2023 no había tramos: cada uno elegía su base entre la mínima y la máxima.",
        norma: "Ley 11/2020, RDL 2/2021 y Orden PCM/1353/2021",
    },
    Autonomos {
        anio: 2022,
        tipo: 30.60,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10)],
        tramos: &SIN_TRAMOS_2022,
        gastos_genericos: 0.0,
        tarifa_plana: Some(60.0),
        tarifa_plana_que: TARIFA_60,
        nota: "Antes de 2023 no había tramos: cada uno elegía su base entre la mínima y la máxima (con 47 o más años, con límites).",
        norma: "Ley 22/2021 (presupuestos de 2022)",
    },
    Autonomos {
        anio: 2023,
        tipo: 31.20,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.60)],
        tramos: &TRAMOS_2023,
        gastos_genericos: 7.0,
        tarifa_plana: Some(80.0 + 5.71),
        tarifa_plana_que: TARIFA_80,
        nota: "",
        norma: "RDL 13/2022 y Ley 31/2022 (presupuestos de 2023)",
    },
    Autonomos {
        anio: 2024,
        tipo: 31.30,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.70)],
        tramos: &TRAMOS_2024,
        gastos_genericos: 7.0,
        tarifa_plana: Some(80.0 + 6.66),
        tarifa_plana_que: TARIFA_80,
        nota: "",
        norma: "RDL 13/2022 y RDL 8/2023",
    },
    Autonomos {
        anio: 2025,
        tipo: 31.40,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.80)],
        tramos: &TRAMOS_2025,
        gastos_genericos: 7.0,
        tarifa_plana: Some(80.0 + 7.61),
        tarifa_plana_que: TARIFA_80,
        nota: "",
        norma: "RDL 13/2022 y RDL 1/2025",
    },
    Autonomos {
        anio: 2026,
        tipo: 31.50,
        desglose: &[("Contingencias comunes", 28.30), ("Contingencias profesionales", 1.30), ("Cese de actividad", 0.90), ("Formación profesional", 0.10), ("Equidad intergeneracional (MEI)", 0.90)],
        tramos: &TRAMOS_2026,
        gastos_genericos: 7.0,
        // Los 80 € de la DT 5.ª del RDL 13/2022 eran de 2023 a 2025; para 2026, la cuantía la tienen que fijar unos
        // presupuestos que no hay, y ni el RDL 3/2026 ni la Orden la dicen.
        tarifa_plana: None,
        tarifa_plana_que: "",
        nota: "",
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
    /// El tramo (desde 1) y sus bases mínima y máxima para unos rendimientos netos al mes (antes de 2023, sin
    /// tramos: uno solo, con las de todos).
    pub fn tramo(&self, mensual: f64) -> (usize, f64, f64) {
        let i = self.tramos.iter().position(|t| mensual <= t.0).unwrap_or(self.tramos.len() - 1);
        (i + 1, self.tramos[i].1, self.tramos[i].2)
    }

    /// Las bases que se ofrecen para elegir en un tramo (desde 1), de menos a más: la mínima, las mínimas de los
    /// tramos de encima que caben en el tuyo y la máxima. Sin tramos (antes de 2023), de la mínima a la máxima de
    /// 100 en 100 €. La ley deja elegir cualquiera entre la mínima y la máxima: estas son las de referencia.
    pub fn bases(&self, tramo: usize) -> Vec<f64> {
        let (_, minima, maxima) = self.tramos[tramo - 1];
        let mut v = vec![minima];
        if self.tramos.len() > 1 {
            v.extend(self.tramos[tramo..].iter().map(|t| t.1).filter(|&b| b > minima && b < maxima));
        } else {
            let mut b = (minima / 100.0).floor() * 100.0 + 100.0;
            while b < maxima {
                v.push(b);
                b += 100.0;
            }
        }
        v.push(maxima);
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_de_cada_anio() {
        assert_eq!(general(2026).base_maxima, 5101.20);
        assert_eq!(general(2024).base_maxima, 4720.50);
        assert_eq!(general(2019).anio, 2020, "sin las de un año, las primeras");
        assert_eq!(general(2030).anio, 2026, "y después, las últimas");
        // Los tipos de cada parte suman lo que dice la Seguridad Social.
        for g in GENERAL {
            let total = |t: (f64, f64)| ((t.0 + t.1) * 100.0).round() / 100.0;
            assert_eq!(total(g.comunes), 28.30);
            let mei = match g.anio {
                ..=2022 => 0.0,
                2023 => 0.60,
                2024 => 0.70,
                2025 => 0.80,
                _ => 0.90,
            };
            assert_eq!(total(g.mei), mei, "{}", g.anio);
            assert_eq!(g.solidaridad.is_empty(), g.anio < 2025, "la solidaridad, desde 2025");
        }
        for a in AUTONOMOS {
            let suma: f64 = a.desglose.iter().map(|d| d.1).sum();
            assert!((suma - a.tipo).abs() < 0.001, "{}: {suma}", a.anio);
            assert!(a.tramos.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1), "tramos ordenados");
        }
        // La tarifa plana: 60 € hasta 2022; desde 2023, 80 € más el MEI de 950,98 € (redondeado al céntimo).
        const BASE_TARIFA_80: f64 = 950.98;
        for a in AUTONOMOS.iter().filter(|a| a.anio <= 2025) {
            let mei = a.desglose.iter().find(|d| d.0.contains("MEI")).map_or(0.0, |d| d.1);
            let esperada = if a.anio < 2023 { 60.0 } else { 80.0 + ((BASE_TARIFA_80 * mei).round() / 100.0) };
            assert!((a.tarifa_plana.unwrap() - esperada).abs() < 1e-9, "{}: {:?}", a.anio, a.tarifa_plana);
            assert_eq!(a.tramos.len() > 1, a.anio >= 2023, "los tramos, desde 2023");
        }
        assert!((autonomos(2021).tipo - 30.425).abs() < 1e-9, "la media de 2021");
        // Las cuotas mínimas: 944,40 € × 30,3 % = 286,15 € en 2020 y 960,60 € × 30,6 % = 293,94 € en 2022.
        let minima = |y: i32| ((autonomos(y).tramos[0].1 * autonomos(y).tipo).round()) / 100.0;
        assert_eq!((minima(2020), minima(2022)), (286.15, 293.94));
        assert_eq!(autonomos(2023).tramo(1250.0), (4, 950.98, 1300.0));
        assert_eq!(autonomos(2024).tramo(7000.0), (15, 1732.03, 4720.50));
        assert_eq!(autonomos(2022).tramo(7000.0), (1, 960.60, 4139.40), "sin tramos");
        // Las bases para elegir: en el tramo 10 de 2025 (de 1.356,21 a 2.760 €), las mínimas de los tramos 11 a 15 y
        // la máxima; sin tramos, de 100 en 100 €.
        assert_eq!(autonomos(2025).bases(10), [1356.21, 1437.91, 1519.61, 1601.31, 1732.03, 1928.10, 2760.0]);
        assert_eq!(autonomos(2025).bases(5), [960.78, 1143.79, 1209.15, 1274.51, 1356.21, 1437.91, 1500.0], "el 6 empieza igual");
        assert_eq!(autonomos(2025).bases(1), [653.59, 718.94]);
        let b = autonomos(2020).bases(1);
        assert_eq!((b[0], b[1], b[2], b[b.len() - 1]), (944.40, 1000.0, 1100.0, 4070.10));
        let a = autonomos(2026);
        assert_eq!(a.tramo(600.0).1, 653.59);
        assert_eq!(a.tramo(1500.0).0, 5);
        assert_eq!(a.tramo(7000.0).1, 1928.10);
    }
}
