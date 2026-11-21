//! Las reglas del IRPF del ahorro (Ley 35/2006 del IRPF, texto consolidado del BOE, y el Manual práctico de Renta
//! de la AEAT) que usa `informe::inversiones` para estimar lo que te tocaría pagar por tus inversiones:
//!
//! - **La escala del ahorro** de cada año (arts. 66.1 la estatal y 76 la autonómica; las dos mitades son iguales):
//!   `ESCALAS`.
//! - **Integrar y compensar** (art. 49): los rendimientos del capital mobiliario y las ganancias y pérdidas van
//!   cada uno por su lado; si uno sale negativo, se compensa con el otro hasta el 25 % de este (`COMPENSACION`), y
//!   lo que quede, en los cuatro años siguientes.
//! - **Las pérdidas que no cuentan** si recompras valores homogéneos (art. 33.5.f y g): `meses_recompra`.
//! - **La deducción por doble imposición internacional** (art. 80): la menor de lo pagado fuera y lo que resulta de
//!   aplicar el tipo medio a esa renta; lo retenido de más sobre lo que permite el convenio no es «impuesto
//!   satisfecho», sino que se reclama al otro país: `TOPE_CONVENIO`.
use super::irpf::{por_tramos, Escala};
use crate::dominio::fmt::r2;
use crate::dominio::inversiones::Producto;

/// La escala del ahorro de cada año (la mitad estatal, art. 66.1, que es igual a la autonómica, art. 76), desde el
/// año en que se aplica: (hasta, tipo %, cuota al empezar el tramo), como las tablas oficiales.
const ESCALAS: &[(i32, Escala)] = &[
    // 19 %, 21 % y 23 % (desde 2016).
    (2020, &[(6000.0, 9.5, 0.0), (50000.0, 10.5, 570.0), (f64::INFINITY, 11.5, 5190.0)]),
    // Ley 11/2020 (Presupuestos de 2021): un tramo nuevo, del 26 %, desde 200.000 €.
    (2021, &[(6000.0, 9.5, 0.0), (50000.0, 10.5, 570.0), (200000.0, 11.5, 5190.0), (f64::INFINITY, 13.0, 22440.0)]),
    // Ley 31/2022 (Presupuestos de 2023): 27 % de 200.000 a 300.000 € y 28 % desde ahí.
    (2023, &[(6000.0, 9.5, 0.0), (50000.0, 10.5, 570.0), (200000.0, 11.5, 5190.0), (300000.0, 13.5, 22440.0), (f64::INFINITY, 14.0, 35940.0)]),
    // Ley 7/2024: 30 % desde 300.000 € (desde el 1 de enero de 2025).
    (2025, &[(6000.0, 9.5, 0.0), (50000.0, 10.5, 570.0), (200000.0, 11.5, 5190.0), (300000.0, 13.5, 22440.0), (f64::INFINITY, 15.0, 35940.0)]),
];

/// Hasta qué año está comprobada la escala (en el texto consolidado de la Ley del IRPF del BOE, actualizado el 18 de
/// noviembre de 2026: los arts. 66 y 76 no cambian desde la Ley 7/2024). Para uno posterior, se usa la última y se
/// dice.
pub const COMPROBADA: i32 = 2026;

/// Hasta dónde se compensa un saldo negativo con el positivo del otro grupo (art. 49.1.a y b: el 25 %).
pub const COMPENSACION: f64 = 0.25;

/// Lo que la app supone que permite retener en origen el convenio con el otro país para los dividendos (el 15 %, lo
/// más común): lo retenido de más no se deduce (DGT V0080-20: no puede exceder de «lo que, derivado de la correcta
/// aplicación del Convenio, correspondiera gravar al país extranjero»), se reclama a ese país.
pub const TOPE_CONVENIO: f64 = 0.15;

/// La escala del ahorro de un año (la mitad estatal, igual a la autonómica) y el año del que es: la del último año
/// que la cambió (para uno anterior a todos, la primera).
pub fn escala(anio: i32) -> (i32, Escala) {
    let (y, e) = ESCALAS.iter().rev().find(|(y, _)| *y <= anio).unwrap_or(&ESCALAS[0]);
    (*y, e)
}

/// La cuota de una base liquidable del ahorro: (estatal, autonómica).
pub fn cuota(anio: i32, base: f64) -> (f64, f64) {
    let (_, e) = escala(anio);
    let mitad = por_tramos(e, base);
    (mitad, mitad)
}

/// Los tipos de la escala de un año, enteros («19 %, 21 %, 23 %, 27 % y 30 %»), para explicarla.
pub fn tipos(anio: i32) -> Vec<(f64, f64)> {
    escala(anio).1.iter().map(|&(hasta, tipo, _)| (hasta, r2(tipo * 2.0))).collect()
}

/// Durante cuántos meses antes y después de vender con pérdidas una recompra de valores homogéneos hace que la
/// pérdida no cuente todavía: dos meses si cotizan (art. 33.5.f; también en las bolsas de EE. UU., DGT V0778-25) y
/// un año si no (art. 33.5.g). En los fondos, solo entre participaciones del mismo fondo: las de fondos distintos no
/// son homogéneas «aunque repliquen un mismo índice de referencia» (V2234-25). Que un fondo que no cotiza vaya con el año sale del
/// texto del artículo: no hay un texto de la AEAT que lo diga.
pub fn meses_recompra(p: Producto) -> u32 {
    match p {
        Producto::Fondo => 12,
        Producto::Accion | Producto::Etf | Producto::Bono | Producto::Otro => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_escala_de_cada_anio() {
        // 10.000 €: 6.000 al 19 % y 4.000 al 21 %, igual todos los años.
        for y in [2020, 2022, 2024, 2025, 2026] {
            let (e, a) = cuota(y, 10000.0);
            assert_eq!(r2(e + a), 1980.0, "{y}");
        }
        // 400.000 €: el último tramo ha cambiado.
        let total = |y| {
            let (e, a) = cuota(y, 400000.0);
            r2(e + a)
        };
        assert_eq!(total(2020), 1140.0 + 9240.0 + 350000.0 * 0.23);
        assert_eq!(total(2022), 1140.0 + 9240.0 + 34500.0 + 200000.0 * 0.26);
        assert_eq!(total(2024), 1140.0 + 9240.0 + 34500.0 + 27000.0 + 100000.0 * 0.28);
        assert_eq!(total(2025), 1140.0 + 9240.0 + 34500.0 + 27000.0 + 100000.0 * 0.30);
        assert_eq!((escala(2019).0, escala(2026).0), (2020, 2025));
        assert_eq!(tipos(2025).iter().map(|t| t.1).collect::<Vec<_>>(), vec![19.0, 21.0, 23.0, 27.0, 30.0]);
        assert_eq!((meses_recompra(Producto::Accion), meses_recompra(Producto::Fondo)), (2, 12));
    }
}
