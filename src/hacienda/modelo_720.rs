//! Modelos 720 y 721 · lo que tienes fuera de España: la declaración informativa de bienes y derechos situados en
//! el extranjero (720) y la de monedas virtuales situadas en el extranjero (721). La app no los presenta ni los lee:
//! con lo que traen tus brókeres dice si te toca presentarlos, qué irían y por qué (`informe::inversiones::fuera`).
//!
//! Las reglas: la disposición adicional 18.ª de la Ley 58/2003 General Tributaria y los arts. 42 bis (cuentas),
//! 42 ter (valores, IIC, seguros y rentas) y 42 quater (monedas virtuales) del Reglamento (RD 1065/2007); la Orden
//! HAP/72/2013 (el 720) y la Orden HFP/886/2023 (el 721), y las preguntas frecuentes del 720 de la AEAT:
//!
//! - **Cada bloque, por su lado**, desde 50.000 € (`UMBRAL`): las cuentas (si su saldo a 31 de diciembre o su
//!   saldo medio del último trimestre pasa de ahí: la app no calcula el medio), los valores, fondos y seguros
//!   (juntos), los inmuebles y, en el 721, las monedas virtuales (desde el ejercicio 2023).
//! - **Los años siguientes**, solo si un bloque sube más de 20.000 € (`SUBIDA`) sobre el valor con que se presentó
//!   la última vez, y siempre que dejes de tener algo de lo declarado.
//! - **Se presenta** del 1 de enero al 31 de marzo del año siguiente (`plazo`).
//! - **Dónde están**: los valores, en el país de la entidad donde están depositados (las preguntas frecuentes:
//!   unas acciones francesas en un bróker alemán van con «Alemania»), no en el del emisor.
//! - **Cómo se valoran**: los fondos (también los cotizados, que van con la clave I), por su valor liquidativo a 31
//!   de diciembre; las acciones que cotizan, por su valor medio de negociación del cuarto trimestre (las que
//!   cotizan fuera, también por su cotización a 31 de diciembre); todo en euros al cambio del 31 de diciembre. La
//!   app usa lo que dice el bróker que valían el 31 de diciembre: para las acciones que cotizan en España, es una
//!   aproximación.
//!
//! Tras la sentencia del Tribunal de Justicia de la UE de 27 de enero de 2022 (C-788/19), la Ley 5/2022 quitó las
//! multas propias del 720 y la imprescriptibilidad de las ganancias no justificadas (art. 39.2 de la Ley del IRPF);
//! presentarlo sigue siendo obligatorio.
use crate::dominio::inversiones::Bien;

/// Desde este valor (de cada bloque), toca presentarlo.
pub const UMBRAL: f64 = 50_000.0;
/// Si ya lo presentaste, solo toca otra vez si el valor de un bloque sube más que esto sobre el de la última vez.
pub const SUBIDA: f64 = 20_000.0;

/// Cada bloque se mira por separado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bloque {
    /// Cuentas en entidades financieras (art. 42 bis).
    Cuentas,
    /// Valores, participaciones en fondos, seguros y rentas (art. 42 ter).
    Valores,
    /// Monedas virtuales (art. 42 quater): el 721.
    Cripto,
}

impl Bloque {
    pub const TODOS: [Bloque; 3] = [Bloque::Valores, Bloque::Cuentas, Bloque::Cripto];

    pub fn de(b: Bien) -> Bloque {
        match b {
            Bien::Cuenta => Bloque::Cuentas,
            Bien::Valores | Bien::Iic => Bloque::Valores,
            Bien::Cripto => Bloque::Cripto,
        }
    }

    /// El modelo en que va.
    pub fn modelo(self) -> &'static str {
        if self == Bloque::Cripto { "721" } else { "720" }
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Bloque::Cuentas => "Cuentas (efectivo)",
            Bloque::Valores => "Valores y fondos",
            Bloque::Cripto => "Criptomonedas",
        }
    }

    /// El primer año del que se presenta (el 721, desde 2023).
    pub fn desde(self) -> i32 {
        if self == Bloque::Cripto { 2023 } else { 2012 }
    }
}

/// Cuándo se presenta lo de un año: del 1 de enero al 31 de marzo del siguiente.
pub fn plazo(anio: i32) -> (String, String) {
    (format!("{}-01-01", anio + 1), format!("{}-03-31", anio + 1))
}

/// ¿Toca presentarlo por un bloque?
#[derive(Debug, Clone, PartialEq)]
pub enum Toca {
    /// No, y por qué.
    No(String),
    Si(String),
    /// No se sabe: no se ha traído todo lo que hace falta para saberlo (y qué falta).
    NoSe(String),
}

/// Con el valor de un bloque a 31 de diciembre, el último presentado en que iba ese bloque (y lo que valía
/// entonces, si se sabe) y si este año has dejado de tener algo de lo que declaraste: entonces toca siempre
/// (RGAT, arts. 42 bis.5, 42 ter.5 y 42 quater.6). `extinguido` es None si no se sabe (no se ha traído lo que tenías
/// algún 31 de diciembre desde entonces): si por lo demás no tocaría, no se sabe.
pub fn toca(b: Bloque, valor: f64, ultima: Option<(i32, Option<f64>)>, extinguido: Option<bool>) -> Toca {
    use crate::dominio::fmt::eur0;
    let m = b.modelo();
    let t = match ultima {
        Some((y, _)) if extinguido == Some(true) => Toca::Si(format!("has dejado de tener algo de lo que declaraste en tu {m} de {y}, y eso hay que decirlo")),
        None if valor > UMBRAL => Toca::Si(format!("pasa de {}", eur0(UMBRAL))),
        None => Toca::No(format!("no llega a {}", eur0(UMBRAL))),
        Some((y, Some(antes))) if valor - antes > SUBIDA => Toca::Si(format!("ha subido {} desde tu {m} de {y}: más de {}", eur0(valor - antes), eur0(SUBIDA))),
        Some((y, Some(antes))) => Toca::No(format!("desde tu {m} de {y} ha cambiado {}: no sube más de {}", eur0(valor - antes), eur0(SUBIDA))),
        Some((y, None)) if valor > UMBRAL => Toca::NoSe(format!("presentaste el de {y}, pero no sé con qué valor: si ha subido más de {}, toca", eur0(SUBIDA))),
        Some((y, None)) => Toca::No(format!("presentaste el de {y} y ahora no llega a {}", eur0(UMBRAL))),
    };
    match (t, ultima) {
        (Toca::No(por), Some((y, _))) if extinguido.is_none() => Toca::NoSe(format!(
            "{por}, pero no sé si has dejado de tener algo de lo que declaraste en tu {m} de {y} (no se ha traído lo que tenías algún 31 de diciembre desde entonces): si es así, toca"
        )),
        (t, _) => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuando_toca() {
        let v = Bloque::Valores;
        assert!(matches!(toca(v, 50_000.0, None, Some(false)), Toca::No(_)), "hay que pasar de 50.000");
        assert!(matches!(toca(v, 50_000.01, None, Some(false)), Toca::Si(_)));
        assert!(matches!(toca(v, 80_000.0, Some((2024, Some(61_000.0))), Some(false)), Toca::No(_)), "sube 19.000");
        assert!(matches!(toca(v, 81_000.01, Some((2024, Some(61_000.0))), Some(false)), Toca::Si(_)), "sube más de 20.000");
        assert!(matches!(toca(v, 10_000.0, Some((2024, Some(61_000.0))), Some(true)), Toca::Si(_)), "vendiste algo declarado");
        // Sin saber si has dejado de tener algo (no se trajo lo de algún 31 de diciembre): no se sabe, y se dice.
        assert!(matches!(toca(v, 70_000.0, Some((2024, Some(61_000.0))), None), Toca::NoSe(_)));
        assert!(matches!(toca(v, 90_000.0, Some((2024, Some(61_000.0))), None), Toca::Si(_)), "toca igual: sube más de 20.000");
        assert!(matches!(toca(v, 90_000.0, Some((2015, None)), Some(false)), Toca::NoSe(_)), "sin el valor con que se presentó");
        let Toca::Si(por) = toca(Bloque::Cripto, 90_000.0, Some((2023, Some(60_000.0))), Some(false)) else { panic!() };
        assert!(por.contains("tu 721 de 2023"), "{por}");
        assert_eq!(plazo(2025), ("2026-01-01".to_string(), "2026-03-31".to_string()));
        assert_eq!((Bloque::de(Bien::Iic), Bloque::de(Bien::Cuenta).modelo(), Bloque::Cripto.modelo()), (Bloque::Valores, "720", "721"));
    }
}
