//! Modelo 390 · el resumen anual del IVA. Todo lo que la app sabe de él:
//!
//! 1. **Qué casillas tiene**: las que se vean en el PDF. Hay diseño de registro oficial (`dr390e2025.xlsx`, en
//!    sede.agenciatributaria.gob.es → Ayuda → Diseños de registro → Modelos 300 al 399), pero la app aún no lo usa.
//! 2. **Cómo se lee** de su PDF: como cualquier formulario (`formulario::leer`), y lo leído tiene que cuadrar con
//!    sus sumas (`reglas`).
//! 3. **Cuándo se presenta**: con el 303 del 4T, hasta el 30 de enero (`hacienda::plazos_trimestres`).
//!
//! La app no lo calcula: lo enseña tal como se presentó.
use super::formulario::{suma, Formulario, QueCasillas};
use super::modelo::{Modelo, Periodicidad};
use crate::dominio::{Casillas, Valor};

/// El modelo 390, para leerlo (`formulario::leer::<M390>`).
pub struct M390;

impl Formulario for M390 {
    const MODELO: Modelo = Modelo::M390;
    const PERIODICIDAD: Periodicidad = Periodicidad::Anual;
    const CASILLAS: QueCasillas = QueCasillas::LasQueSeVean;

    fn reglas(c: &Casillas) -> Vec<(u32, f64)> {
        let v = |n| c.v(n);
        let mut r = vec![
            (33, suma(c, &BASES)),
            (34, suma(c, &CUOTAS)),
            (47, suma(c, &[34, 664, 692, 36, 666, 694, 600, 602, 42, 44, 46])),
            (64, suma(c, &[49, 513, 51, 521, 53, 55, 57, 59, 598, 61, 661, 62, 652, 63, 522])),
            (65, v(47) - v(64)),
            (108, suma(c, &[99, 653, 103, 104, 105, 110, 100, 101, 102, 125, 126, 127, 128, 227, 228]) - v(106) - v(107)),
        ];
        // La liquidación anual (apartado 7) solo la rellena quien tributa solo en territorio común.
        if c.contains_key(&84) {
            r.push((84, v(65) + v(83) + v(658)));
        }
        if c.contains_key(&86) {
            r.push((86, v(84) + v(659) - v(85)));
        }
        r
    }
}

/// Bases del IVA devengado (de todos los años desde 2020: las que no tiene un año valen 0). Su suma es la 33.
const BASES: [u32; 47] = [
    700, 667, 1, 702, 669, 3, 5, // régimen ordinario
    704, 671, 500, 706, 673, 502, 504, // operaciones intragrupo
    708, 675, 643, 710, 677, 645, 647, // criterio de caja
    712, 679, 7, 714, 681, 9, 11, // bienes usados
    13, // agencias de viajes
    716, 683, 21, 718, 685, 23, 25, // adquisiciones intracomunitarias de bienes
    720, 687, 545, 722, 689, 547, 551, // adquisiciones intracomunitarias de servicios
    27, 29, 649, 31, // inversión del sujeto pasivo y modificaciones
];
/// Sus cuotas, en el mismo orden. Su suma es la 34.
const CUOTAS: [u32; 47] = [
    701, 668, 2, 703, 670, 4, 6, //
    705, 672, 501, 707, 674, 503, 505, //
    709, 676, 644, 711, 678, 646, 648, //
    713, 680, 8, 715, 682, 10, 12, //
    14, //
    717, 684, 22, 719, 686, 24, 26, //
    721, 688, 546, 723, 690, 548, 552, //
    28, 30, 650, 32,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hacienda::formulario::descuadres;

    #[test]
    fn sumas_del_modelo() {
        // 390 de 2023 (las cifras conocidas): 05 − modificación de bases (29) = 33.
        let r: Casillas = [(5, 23500.0), (29, -18901.42), (33, 4598.58), (99, 23500.0), (108, 23500.0)].into_iter().collect();
        assert!(descuadres::<M390>(&r).iter().all(|&n| n != 33 && n != 108), "{:?}", descuadres::<M390>(&r));
    }
}
