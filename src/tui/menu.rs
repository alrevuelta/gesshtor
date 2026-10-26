//! Las secciones del menú y sus pestañas, en su orden, y la letra de cada una (Espacio y ella van allí).

use super::{anios, clientes, cuotas, declaraciones, empleo, facturas, gastos, impuestos, inicio, Area, Seccion};

pub fn secciones() -> Vec<Seccion> {
    let resumen: Vec<Box<dyn super::Pantalla>> = vec![
        Box::new(inicio::Inicio::default()),
        Box::new(anios::Anios::default()),
        Box::new(declaraciones::Declaraciones::default()),
    ];
    let mut autonomo: Vec<Box<dyn super::Pantalla>> = vec![
        Box::new(facturas::Facturas::default()),
        Box::new(gastos::Gastos::default()),
        Box::new(clientes::Clientes::default()),
        Box::new(impuestos::Impuestos::default()),
    ];
    autonomo.extend(cuotas::pantallas());
    vec![
        Seccion { area: Area::Resumen, nombre: "Resumen", pantallas: resumen, activa: 0 },
        Seccion { area: Area::Empleo, nombre: "Empleo", pantallas: empleo::pantallas(), activa: 0 },
        Seccion { area: Area::Autonomo, nombre: "Autónomo", pantallas: autonomo, activa: 0 },
    ]
}

/// La letra de cada entrada del menú: Espacio y ella van allí, estés donde estés.
pub fn letra(titulo: &str) -> Option<char> {
    Some(match titulo {
        "Inicio" => 'i',
        "Años" => 'a',
        "Declaraciones" => 'd',
        "Sueldo" => 's',
        "Nóminas" => 'n',
        "Facturas" => 'f',
        "Gastos" => 'g',
        "Clientes" => 'c',
        "Impuestos" => 't',
        "Cuotas" => 'u',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada entrada tiene su letra, y no hay dos con la misma.
    #[test]
    fn una_letra_por_entrada() {
        let titulos: Vec<&str> = secciones().iter().flat_map(|s| s.pantallas.iter().map(|p| p.titulo())).collect();
        let mut letras: Vec<char> = titulos.iter().map(|t| letra(t).unwrap_or_else(|| panic!("«{t}» sin letra"))).collect();
        letras.sort();
        letras.dedup();
        assert_eq!(letras.len(), titulos.len(), "{titulos:?}");
    }
}
