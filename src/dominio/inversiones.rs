//! Tus inversiones: lo que trae un bróker, en un formato común y neutro (una `Cartera`), y qué conectores has
//! activado.
//!
//! Tres piezas que no se mezclan:
//!
//! - **Este formato**: compras y ventas, dividendos e intereses con sus retenciones y lo que había a cada 31 de
//!   diciembre. Lo traído de cada bróker va a `fuentes/<conector>.json` (`Fuente<Cartera>`).
//! - **Los conectores** (`brokers/`): cada uno solo traduce de la web de su bróker a este formato.
//! - **El cálculo fiscal** (`informe::inversiones`): solo conoce este formato.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Lo que se guarda en `datos.json`: los conectores que has activado alguna vez (su id → si lo está ahora). Sin
/// activarlo, la app no habla con ese bróker; desactivado, no vuelve a hablar con él, pero lo ya traído se sigue
/// viendo.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Inversiones {
    pub conectores: BTreeMap<String, bool>,
}

impl Inversiones {
    pub fn activado(&self, id: &str) -> bool {
        self.conectores.get(id).copied().unwrap_or(false)
    }
}

/// Lo que se sabe de tu cuenta en un bróker: todo lo que ha pasado en ella desde que la abriste (para saber lo que
/// te costó lo que vendes hacen falta todas las compras) y lo que había en ella a cada 31 de diciembre.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Cartera {
    /// El bróker, como se llama («DEGIRO»).
    pub broker: String,
    /// Hasta qué día llega («2026-11-20»).
    pub hasta: String,
    pub operaciones: Vec<Operacion>,
    pub cobros: Vec<Cobro>,
    pub posiciones: Vec<Posicion>,
    /// Las fechas de las que se ha traído lo que había («2025-12-31»…), aunque no hubiera nada.
    pub cierres: Vec<String>,
    /// Lo que el conector ha visto y no sabe traducir (un movimiento que no conoce, un split…): se enseña tal cual.
    pub avisos: Vec<String>,
    /// Los valores cuyo coste deja de saberse desde un día, porque hubo algo que no es una compra ni una venta y el
    /// conector no sabe traducir (un split, una fusión, un cambio de ISIN, un traspaso): ISIN → la primera de esas
    /// fechas («2026-05-01T10:00:00»). Lo que se venda de él desde ese día no entra en las cuentas.
    pub sin_coste_desde: BTreeMap<String, String>,
}

/// Qué es lo que se compra o se vende (para Hacienda no es lo mismo una acción que un fondo).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Producto {
    #[default]
    Accion,
    /// Fondo cotizado.
    Etf,
    /// Fondo de inversión que no cotiza.
    Fondo,
    Bono,
    /// Derivados, warrants, productos apalancados…
    Otro,
}

/// Una compra o una venta.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Operacion {
    /// Cuándo («2026-03-02T15:32:10», hora local de la operación).
    pub fecha: String,
    pub isin: String,
    pub nombre: String,
    pub producto: Producto,
    pub compra: bool,
    /// Cuántos (siempre positivo).
    pub cantidad: f64,
    /// A cuánto cada uno, en su divisa.
    pub precio: f64,
    pub divisa: String,
    /// El cambio de esa operación: unidades de la divisa por euro (1 si es en euros; 0 si no se sabe).
    pub cambio: f64,
    /// Lo que se paga o se cobra, en euros, sin comisiones (siempre positivo).
    pub euros: f64,
    /// Comisiones y gastos de la operación, en euros (positivo): los del bróker, los de la bolsa y el del cambio de
    /// divisa.
    pub comisiones: f64,
}

/// Qué se cobra.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Clase {
    #[default]
    Dividendo,
    /// Intereses (del efectivo de la cuenta, de un bono…).
    Interes,
}

/// Un dividendo o unos intereses, con lo que te retuvieron.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Cobro {
    /// El día del cobro («2026-03-02»).
    pub fecha: String,
    pub clase: Clase,
    /// De qué (vacío en los intereses de la cuenta).
    pub isin: String,
    pub nombre: String,
    pub divisa: String,
    /// Lo bruto, en su divisa.
    pub bruto: f64,
    /// Lo que te retuvo el país de donde viene (positivo, en su divisa).
    pub retenido_origen: f64,
    /// Lo que te retuvieron para Hacienda (positivo, en su divisa): es un pago a cuenta de tu Renta.
    pub retenido_espana: f64,
    /// El cambio de ese día (unidades de la divisa por euro; 1 si es en euros), si se sabe.
    pub cambio: Option<f64>,
}

impl Cobro {
    /// Un importe suyo en euros, si se sabe el cambio.
    pub fn en_euros(&self, x: f64) -> Option<f64> {
        self.cambio.filter(|c| *c > 0.0).map(|c| x / c)
    }
}

/// Qué es para el modelo 720 (o el 721) lo que tienes en un bróker.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Bien {
    /// Acciones y otros valores.
    #[default]
    Valores,
    /// Participaciones en fondos (también los cotizados).
    Iic,
    /// Efectivo en una cuenta.
    Cuenta,
    /// Monedas virtuales (van en el 721).
    Cripto,
}

/// Lo que tenías de algo en una fecha.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Posicion {
    /// «2025-12-31».
    pub fecha: String,
    pub bien: Bien,
    /// Vacío en el efectivo.
    pub isin: String,
    pub nombre: String,
    pub cantidad: f64,
    /// Lo que valía, en euros, ese día.
    pub euros: f64,
    /// Dónde está depositado: el país (código ISO, «DE») y la entidad, según el bróker.
    pub pais: String,
    pub depositario: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_guarda_y_se_lee_igual() {
        let c = Cartera {
            broker: "Bróker de prueba".into(),
            operaciones: vec![Operacion { isin: "XX0000000001".into(), producto: Producto::Etf, cantidad: 2.0, ..Default::default() }],
            cobros: vec![Cobro { clase: Clase::Interes, cambio: Some(1.0), ..Default::default() }],
            posiciones: vec![Posicion { bien: Bien::Cuenta, ..Default::default() }],
            ..Default::default()
        };
        let txt = serde_json::to_string(&c).unwrap();
        assert!(txt.contains("\"producto\":\"etf\"") && txt.contains("\"clase\":\"interes\"") && txt.contains("\"bien\":\"cuenta\""), "{txt}");
        assert_eq!(serde_json::from_str::<Cartera>(&txt).unwrap(), c);
        let d = Cobro { bruto: 10.0, cambio: Some(1.25), ..Default::default() };
        assert_eq!((d.en_euros(10.0), Cobro::default().en_euros(10.0)), (Some(8.0), None));
        let mut i = Inversiones::default();
        assert!(!i.activado("degiro"), "desactivado por defecto");
        i.conectores.insert("degiro".into(), true);
        assert!(i.activado("degiro"));
    }
}
