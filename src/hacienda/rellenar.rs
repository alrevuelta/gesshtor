//! Qué casillas de un modelo tiene que rellenar la persona, según lo que dicen los documentos oficiales de la AEAT:
//!
//! - **Las que son una operación con otras** no se enseñan: son las sumas y restas de cada modelo
//!   (`Formulario::reglas`), que salen del diseño de registro y de las instrucciones.
//! - **Las que trae Hacienda** si tiene el dato se enseñan para comprobarlas: en el 303, la 110, la 77 y la 70 (servicio
//!   Pre303, «Cuestiones generales»); en el 130, la 05, la 13 y la 15 (servicio Pre130).
//! - **Las demás se escriben.** Unas pocas (las cuotas de cada tipo, el % de la 65, la 04 del 130 y los subtotales del
//!   390) van aparte, en `SiNoSale`: las instrucciones no dicen si el formulario las rellena al escribir la base, así
//!   que se enseñan, pero diciendo que si ya vienen basta con mirarlas.
//!
//! Cambian de un año a otro: al salir unas instrucciones nuevas, revisar estas listas junto a los `DISENOS` del modelo.
use super::formulario::Formulario;
use super::modelo::Modelo;
use super::modelo_130::M130;
use super::modelo_303::M303;
use super::modelo_390::M390;
use crate::dominio::Casillas;

/// Qué tiene que hacer la persona con una casilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uso {
    /// La escribe ella.
    Escribir,
    /// La escribe ella si el formulario no la rellena solo (eso no lo dicen las instrucciones).
    SiNoSale,
    /// La trae Hacienda si tiene el dato: basta con comprobarla.
    Comprobar,
    /// Es una operación con otras casillas: no se enseña.
    Formula,
}

const COMPROBAR_303: [u32; 3] = [70, 77, 110];
const COMPROBAR_130: [u32; 3] = [5, 13, 15];

/// Las cuotas de cada tipo (régimen general y recargo), el % atribuible al Estado y el total del volumen.
const SIN_CONFIRMAR_303: [u32; 13] = [3, 6, 9, 18, 21, 24, 65, 88, 152, 155, 158, 167, 170];
const SIN_CONFIRMAR_130: [u32; 1] = [4];
/// Las cuotas de cada tipo y los subtotales de lo deducible.
const SIN_CONFIRMAR_390: [u32; 36] = [
    2, 4, 6, 22, 24, 26, 28, 30, 546, 548, 552, // cuotas devengadas
    191, 604, 606, 197, 612, 614, 215, 628, 630, 221, 632, 634, 588, 636, 638, // cuotas deducibles
    48, 49, 50, 51, 56, 57, 58, 59, 597, 598, // subtotales
];

/// Las casillas que son una operación con otras: las de las reglas de cada modelo.
fn formulas(m: Modelo) -> Vec<u32> {
    let vacias = Casillas::new();
    let claves = |r: Vec<(u32, f64)>| r.into_iter().map(|x| x.0).collect();
    match m {
        Modelo::M303 => claves(M303::reglas(&vacias)),
        Modelo::M130 => claves(M130::reglas(&vacias)),
        // Las dos últimas solo salen si las tiene quien tributa solo en territorio común.
        Modelo::M390 => claves(M390::reglas(&[(84, 0.0), (86, 0.0)].into_iter().collect())),
        Modelo::M100 => Vec::new(),
    }
}

/// Qué hacer con la casilla `k` del modelo `m`.
pub fn uso(m: Modelo, k: u32) -> Uso {
    if formulas(m).contains(&k) {
        return Uso::Formula;
    }
    let (comprobar, sin_confirmar): (&[u32], &[u32]) = match m {
        Modelo::M303 => (&COMPROBAR_303, &SIN_CONFIRMAR_303),
        Modelo::M130 => (&COMPROBAR_130, &SIN_CONFIRMAR_130),
        Modelo::M390 => (&[], &SIN_CONFIRMAR_390),
        Modelo::M100 => (&[], &[]),
    };
    if comprobar.contains(&k) {
        Uso::Comprobar
    } else if sin_confirmar.contains(&k) {
        Uso::SiNoSale
    } else {
        Uso::Escribir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casillas_del_303() {
        for k in [27, 45, 46, 64, 66, 69, 71, 87] {
            assert_eq!(uso(Modelo::M303, k), Uso::Formula, "{k}");
        }
        for k in [110, 77, 70] {
            assert_eq!(uso(Modelo::M303, k), Uso::Comprobar, "{k}");
        }
        for k in [7, 1, 4, 150, 10, 11, 12, 13, 14, 15, 28, 29, 59, 60, 120, 78] {
            assert_eq!(uso(Modelo::M303, k), Uso::Escribir, "{k}");
        }
        for k in [9, 3, 6, 152, 65] {
            assert_eq!(uso(Modelo::M303, k), Uso::SiNoSale, "{k}");
        }
    }

    #[test]
    fn casillas_del_130() {
        for k in [3, 7, 11, 12, 14, 17, 19] {
            assert_eq!(uso(Modelo::M130, k), Uso::Formula, "{k}");
        }
        for k in [5, 13, 15] {
            assert_eq!(uso(Modelo::M130, k), Uso::Comprobar, "{k}");
        }
        for k in [1, 2, 6, 16] {
            assert_eq!(uso(Modelo::M130, k), Uso::Escribir, "{k}");
        }
        assert_eq!(uso(Modelo::M130, 4), Uso::SiNoSale);
    }

    #[test]
    fn casillas_del_390() {
        for k in [33, 34, 47, 64, 65, 84, 86, 108] {
            assert_eq!(uso(Modelo::M390, k), Uso::Formula, "{k}");
        }
        for k in [5, 605, 85, 95, 97, 99] {
            assert_eq!(uso(Modelo::M390, k), Uso::Escribir, "{k}");
        }
        for k in [6, 606, 48, 49] {
            assert_eq!(uso(Modelo::M390, k), Uso::SiNoSale, "{k}");
        }
    }
}
