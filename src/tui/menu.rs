//! Las secciones del menú y sus pestañas, en su orden.

use super::{actividad, anios, clientes, cuotas, declaraciones, empleo, este_anio, facturas, gastos, impuestos, inicio, Area, Seccion};

pub fn secciones() -> Vec<Seccion> {
    let resumen: Vec<Box<dyn super::Pantalla>> = vec![
        Box::new(inicio::Inicio::default()),
        Box::new(este_anio::EsteAnio::default()),
        Box::new(anios::Anios::default()),
        Box::new(declaraciones::Declaraciones::default()),
    ];
    let mut autonomo: Vec<Box<dyn super::Pantalla>> = vec![
        Box::new(facturas::Facturas::default()),
        Box::new(gastos::Gastos::default()),
        Box::new(clientes::Clientes::default()),
        Box::new(impuestos::Impuestos::default()),
        Box::new(actividad::Actividad::default()),
    ];
    autonomo.extend(cuotas::pantallas());
    vec![
        Seccion { area: Area::Resumen, nombre: "Resumen", pantallas: resumen, activa: 0 },
        Seccion { area: Area::Empleo, nombre: "Empleo", pantallas: empleo::pantallas(), activa: 0 },
        Seccion { area: Area::Autonomo, nombre: "Autónomo", pantallas: autonomo, activa: 0 },
    ]
}
