//! Las secciones del menú, en su orden: cada una con su letra, su nombre, sus pestañas (que van con `1`, `2`, `3`…,
//! en este orden) y, si no sale siempre, cuándo sale. Añadir una sección es una línea aquí; su letra, minúscula y
//! una de las reservadas para navegar (`RESERVADAS`).

use super::{anios, declaraciones, empleo, facturas, gastos, impuestos, inicio, inversiones, utilidades, Seccion};
use ratatui::style::Color;

/// Las letras de las secciones (las que hay y las que vendrán): ninguna pantalla las usa salvo escribiendo.
pub const RESERVADAS: &str = "reapu";

pub fn secciones() -> Vec<Seccion> {
    vec![
        Seccion::new('r', "Resumen", vec![Box::new(inicio::Inicio::default()), Box::new(anios::Anios::default()), Box::new(declaraciones::Declaraciones::default())]),
        Seccion::new('e', "Empleo", empleo::pantallas()).si(|s| s.empleo),
        Seccion::new('a', "Autónomo", vec![Box::new(facturas::Facturas::default()), Box::new(gastos::Gastos::default()), Box::new(impuestos::Impuestos::default())]).si(|s| s.autonomo),
        Seccion::new('p', "Patrimonio", inversiones::pantallas()),
        Seccion::new('u', "Utilidades", vec![Box::new(utilidades::Calculadora::default())]),
    ]
}

/// El nombre, «GESSHTOR», arriba a la izquierda del carril: dos filas de sextantes (letras de 3 × 5 puntos, la H de
/// 4; cada celda, 2 × 3 puntos). Para cambiarlo basta con estas constantes: las dos filas tienen que medir lo mismo
/// y caber antes de la columna del contenido (lo comprueba un test de `mod.rs`).
pub const NOMBRE: [&str; 2] = ["🬕🬐🬴🬀🬴🬐🬴🬐🬲🬷🬁🬕▐🬨▐🬙", "🬌🬄🬌🬃🬋🬄🬋🬄🬄🬉 🬄🬉🬍🬉🬉"];
/// Las celdas [desde, hasta) de cada fila que van en color («SSH»: gestor con ssh dentro); las demás, en TINTA.
pub const NOMBRE_COLOR: (usize, usize) = (4, 10);
/// El color de esas celdas en cada fila: cian arriba y violeta abajo. Sin negrita.
pub const NOMBRE_TONOS: [Color; 2] = [Color::Rgb(0x9f, 0xf3, 0xff), Color::Rgb(0x8a, 0x7d, 0xff)];
/// La columna donde empieza el nombre.
pub const NOMBRE_X: u16 = 2;
