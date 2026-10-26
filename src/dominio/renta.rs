//! Una Renta (modelo 100) leída del PDF de Hacienda: las casillas y lo que significan, por bloques.
//! Es lo que sale del lector (`hacienda::modelo_100`), lo que se guarda en `cache.json` y lo que usan los informes.
//! Cada bloque solo está si el lector lo encontró y cuadran sus sumas; si no, `None`.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Renta {
    /// Versión del lector que la leyó.
    pub v: u32,
    /// Numeración de casillas con la que se leyó: «2015», «2016», «2017», «2018» (2018 en adelante).
    pub numeracion: String,
    pub ejercicio: i32,
    /// Todas las casillas leídas: número → importe.
    pub casillas: super::Casillas,
    pub trabajo: Option<Trabajo>,
    pub capital: Option<Capital>,
    pub actividad: Option<Actividad>,
    pub ganancias: Option<Ganancias>,
    pub inmuebles: Option<Inmuebles>,
    pub bases: Option<Bases>,
    pub cuotas: Option<Cuotas>,
    /// Retenciones y pagos a cuenta, por origen.
    pub pagos: Option<Pagos>,
    /// Comunidad autónoma (como la escribe la Renta).
    pub comunidad: String,
    /// Declaración conjunta: las cifras son de la familia, no solo tuyas.
    pub conjunta: bool,
    /// Resultado de la declaración: > 0 a pagar, < 0 a devolver.
    pub resultado: f64,
    /// Cuota resultante de la autoliquidación (el IRPF del año).
    pub cuota: f64,
    pub pagos_cuenta: f64,
    pub fraccionados: Option<f64>,
    pub diferencial: f64,
    /// Es una rectificativa / complementaria de otra.
    pub rectificativa: bool,
    pub anterior: String,
    pub pagado_antes: f64,
    pub devuelto_antes: f64,
    pub regularizacion: Option<f64>,
    /// Cómo se paga o se devuelve (domiciliado, a devolver…), si se sabe.
    #[serde(with = "super::hacienda::letra")]
    pub forma: Option<super::hacienda::Tipo>,
    pub plazos: Vec<Plazo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Trabajo {
    /// Retribuciones dinerarias.
    pub dinerario: f64,
    /// En especie (valoración + ingresos a cuenta).
    pub especie: f64,
    /// Ingresos íntegros del trabajo.
    pub integros: f64,
    /// Cotizaciones a la Seguridad Social.
    pub ss: f64,
    pub neto: Option<f64>,
    pub neto_reducido: Option<f64>,
    pub retenciones: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Capital {
    pub intereses: f64,
    pub dividendos: f64,
    pub otros: f64,
    pub integros: f64,
    pub gastos: f64,
    pub neto: f64,
    /// Solo se sabe el total (declaraciones conjuntas): sin desglose.
    pub solo_total: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Actividad {
    /// Tipo, epígrafe IAE y modalidad (solo en la numeración de 2018 en adelante).
    pub tipo: String,
    pub iae: String,
    pub modalidad: String,
    pub ingresos: f64,
    /// Seguridad Social del titular (la cuota de autónomo).
    pub ss_titular: Option<f64>,
    /// Todos los gastos deducibles (con la cuota y el de difícil justificación).
    pub gastos: f64,
    /// Provisiones y gastos de difícil justificación.
    pub gdj: Option<f64>,
    pub neto: f64,
    pub reduccion: Option<f64>,
    pub neto_reducido: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct GananciasPerdidas {
    pub ganancias: f64,
    pub perdidas: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Ganancias {
    pub ganancias: f64,
    pub perdidas: f64,
    /// Saldo que cuenta (las pérdidas se compensan en años siguientes: el saldo nunca es negativo en la base).
    pub neto: f64,
    pub cripto: Option<GananciasPerdidas>,
    pub otros: Option<GananciasPerdidas>,
}

/// Alquileres e inmuebles.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Inmuebles {
    /// Lo cobrado de alquileres.
    pub ingresos: f64,
    /// Rendimiento neto reducido (tras la reducción por alquiler de vivienda).
    pub reducidos: f64,
    /// La reducción por alquiler de vivienda (solo es fiscal).
    pub reduccion: f64,
    /// Rendimiento neto de los alquileres, sin la reducción.
    pub neto: f64,
    /// Renta imputada de inmuebles vacíos o a disposición: Hacienda la suma, pero no es dinero cobrado.
    pub imputaciones: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Bases {
    pub general: f64,
    pub ahorro: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Cuotas {
    pub base_general: f64,
    pub base_ahorro: f64,
    pub liquidable_general: f64,
    pub liquidable_ahorro: f64,
    pub general: f64,
    pub ahorro: f64,
    pub integra: f64,
    pub liquida: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Pagos {
    /// Retenciones del trabajo (nómina).
    pub trabajo: f64,
    /// Retenciones del capital (banco).
    pub capital: f64,
    /// Retenciones de actividades (tus clientes).
    pub actividades: f64,
    /// Pagos fraccionados (modelo 130).
    pub fraccionados: f64,
    pub total: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Plazo {
    pub fecha: String,
    pub importe: f64,
}

impl Renta {
    /// El importe de una casilla, si se leyó.
    pub fn casilla(&self, n: u32) -> Option<f64> {
        self.casillas.get(&n).copied()
    }
}
