//! Los datos de una persona, tal cual se guardan en `datos.json` (ver `dominio::almacen`): lo que apunta (facturas, gastos,
//! clientes, nóminas, ajustes) y lo que se trae con el certificado (lo presentado en Hacienda y lo de la
//! Seguridad Social). Lo leído de los PDF no se guarda aquí: está en `cache.json` y vive en `lecturas`.
//!
//! Las facturas y los gastos, con lo que se calcula con ellos, están en `factura` y `gasto`.
pub mod almacen;
pub mod factura;
pub mod fmt;
pub mod gasto;
pub mod hacienda;
pub mod renta;
pub mod tgss;

pub use factura::{Cliente, Factura, Linea, Rectifica};
pub use gasto::Gasto;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Un importe que puede estar sin poner (`None`): «sin poner» no es lo mismo que 0 (un IVA sin poner usa el de
/// la categoría; uno a 0 es 0).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(transparent)]
pub struct Num(pub Option<f64>);

impl Num {
    pub fn de(x: f64) -> Self {
        Num(Some(x))
    }
    /// El valor, con «sin poner» = 0.
    pub fn v(&self) -> f64 {
        self.0.unwrap_or(0.0)
    }
    pub fn vacio(&self) -> bool {
        self.0.is_none()
    }
}

/// La versión de `datos.json`. Uno de otra versión no se abre.
pub const VERSION: u32 = 2;

/// El primer año que trata la app: lo anterior no se lee, ni se consulta, ni se enseña.
pub const PRIMER_ANIO: i32 = 2020;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Datos {
    pub version: u32,
    pub perfil: Perfil,
    pub facturacion: Facturacion,
    pub fiscal: Fiscal,
    pub clientes: Vec<Cliente>,
    pub facturas: Vec<Factura>,
    pub gastos: Vec<Gasto>,
    /// Nóminas apuntadas a mano, una por mes.
    pub nominas: Vec<Nomina>,
    /// Lo presentado en Hacienda, por año (lo que dio la última consulta).
    pub hacienda: BTreeMap<i32, hacienda::Consulta>,
    /// Lo de la Seguridad Social: autónomo y cotizaciones de las nóminas.
    pub tgss: tgss::Tgss,
    /// Lo leído de los PDF de Hacienda de la carpeta (lo pone `hacienda::lectura` al arrancar; se guarda en `cache.json`).
    #[serde(skip)]
    pub lecturas: Lecturas,
}

impl Datos {
    /// Tu alta y tu baja de ahora como autónomo («AAAA-MM-DD»; vacías si no constan): las de la Seguridad Social si
    /// las tiene; si no, las de tu perfil. Tras una baja y un alta nueva, es la nueva: para saber desde cuándo tienes
    /// actividad (el primer año, los trimestres que tocan), `fechas_actividad`.
    pub fn alta_autonomo(&self) -> (&str, &str) {
        let ss = &self.tgss.alta;
        if ss.fecha.is_empty() { (&self.perfil.fecha_alta, &self.perfil.fecha_baja) } else { (&ss.fecha, &ss.baja) }
    }

    /// Cuándo empezó (y acabó) tu actividad, para Hacienda: las fechas de tu perfil (vacías si no las has puesto).
    /// No sale de la Seguridad Social: tras una baja y un alta nueva, su fecha de alta es la nueva, no la del
    /// principio.
    pub fn fechas_actividad(&self) -> (&str, &str) {
        (&self.perfil.fecha_alta, &self.perfil.fecha_baja)
    }

    /// El primer año con facturas o gastos apuntados; como mucho, el actual.
    pub fn primer_anio_con_apuntes(&self) -> i32 {
        let fechas = self.facturas.iter().map(|f| f.fecha.as_str()).chain(self.gastos.iter().map(|g| g.fecha.as_str()));
        fechas.map(fmt::anio_de).filter(|a| *a > 0).fold(fmt::anio_actual(), i32::min)
    }
}

impl Default for Datos {
    fn default() -> Self {
        Datos {
            version: VERSION,
            perfil: Perfil::default(),
            facturacion: Facturacion::default(),
            fiscal: Fiscal::default(),
            clientes: Vec::new(),
            facturas: Vec::new(),
            gastos: Vec::new(),
            nominas: Vec::new(),
            hacienda: BTreeMap::new(),
            tgss: tgss::Tgss::default(),
            lecturas: Lecturas::default(),
        }
    }
}

/// Las casillas de una declaración: número → importe.
pub type Casillas = BTreeMap<u32, f64>;

/// El importe de una casilla, con las que no están a 0 (como en los modelos: la que no se rellena vale 0).
pub trait Valor {
    fn v(&self, casilla: u32) -> f64;
}

impl Valor for Casillas {
    fn v(&self, casilla: u32) -> f64 {
        self.get(&casilla).copied().unwrap_or(0.0)
    }
}

/// Lo leído de los PDF de Hacienda que hay en la carpeta de datos.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Lecturas {
    /// Las Rentas, por año.
    pub rentas: BTreeMap<i32, renta::Renta>,
    /// Las declaraciones con casillas (303, 130, 390…), por clave «2026-3T-303» / «2025-390».
    pub formularios: BTreeMap<String, FormularioLeido>,
    /// Lo que no se ha podido leer: clave → motivo.
    pub fallos: BTreeMap<String, String>,
}

/// Una declaración leída de su PDF: las casillas que se ven, con su número.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormularioLeido {
    pub casillas: Casillas,
    /// Cómo se presentó (a ingresar, a compensar, a devolver…), si se ve en la portada.
    #[serde(with = "hacienda::letra")]
    pub tipo: Option<hacienda::Tipo>,
    /// Nombre del PDF en justificantes/.
    pub pdf: String,
    /// Número de justificante que figura en el PDF.
    pub justificante: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Perfil {
    /// El nombre completo (o la razón social).
    pub nombre: String,
    /// Nombre de pila y apellidos, del certificado (para saludar por el nombre).
    pub nombre_pila: String,
    pub apellidos: String,
    pub nif: String,
    pub direccion: String,
    pub cp: String,
    pub ciudad: String,
    pub provincia: String,
    pub email: String,
    pub iban: String,
    pub actividad: String,
    /// Alta y baja como autónomo (YYYY-MM-DD), si se saben.
    pub fecha_alta: String,
    pub fecha_baja: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Facturacion {
    /// Patrón de la serie: «AAAA-» se convierte en «2026-».
    pub serie: String,
    pub serie_rect: String,
    pub digitos: Num,
    /// Retención por defecto de las facturas nuevas (%).
    pub retencion: Num,
    /// IVA por defecto de las líneas nuevas (%).
    pub iva: Num,
    pub forma_pago: String,
    pub dias_vencimiento: Num,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Fiscal {
    /// ¿Se aplica el 5 % (7 % en 2023) de gastos de difícil justificación?
    pub gdj: bool,
    /// ¿También en los 130? (por defecto no: solo en la Renta)
    pub gdj130: bool,
    /// Rendimiento neto del año anterior escrito a mano, por año.
    pub rn_anterior: BTreeMap<i32, f64>,
    pub m2_vivienda: Num,
    pub m2_despacho: Num,
    pub personas_salud: Num,
    pub compensar_inicial: Num,
    /// ¿Se libra del 130? «auto» (lo calcula), «si» o «no».
    pub exento130: String,
}

impl Default for Fiscal {
    /// Con el de difícil justificación.
    fn default() -> Self {
        Fiscal {
            gdj: true,
            gdj130: false,
            rn_anterior: BTreeMap::new(),
            m2_vivienda: Num(None),
            m2_despacho: Num(None),
            personas_salud: Num::de(1.0),
            compensar_inicial: Num::de(0.0),
            exento130: String::new(),
        }
    }
}

/// Una nómina apuntada: la de un mes (si cobras de dos empresas, una de cada).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Nomina {
    pub id: String,
    pub anio: i32,
    pub empresa: String,
    /// Mes de la nómina (1–12).
    pub mes: u32,
    /// Bruto en dinero (total devengado).
    pub dinerario: f64,
    /// En especie (seguro, acciones, coche…).
    pub especie: f64,
    /// Retención de IRPF.
    pub retenciones: f64,
    /// Seguridad Social que te descuentan.
    pub ss: f64,
}

impl Nomina {
    /// El bruto: en dinero y en especie.
    pub fn bruto(&self) -> f64 {
        self.dinerario + self.especie
    }
    /// Lo que no puede tener una nómina: un mes de 1 a 12, algo de bruto, y que la retención y la Seguridad Social
    /// no sumen más que el bruto.
    pub fn validar(&self) -> Result<(), &'static str> {
        use fmt::DIF;
        if !(1..=12).contains(&self.mes) {
            return Err("Elige el mes de la nómina.");
        }
        if self.bruto() <= DIF {
            return Err("Escribe el bruto cobrado.");
        }
        if self.retenciones + self.ss > self.bruto() + DIF {
            return Err("La retención y la Seguridad Social no pueden sumar más que el bruto.");
        }
        Ok(())
    }
}

/// Los nombres sin repetir, sin vacíos ni espacios alrededor, en el orden en que salen.
pub fn sin_repetir<'a>(nombres: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for n in nombres.into_iter().map(str::trim).filter(|n| !n.is_empty()) {
        if !v.iter().any(|x| x == n) {
            v.push(n.to_string());
        }
    }
    v
}

/// Id corto y único.
pub fn uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ms = t.as_millis() as u64;
    let mut azar = (t.subsec_nanos() as u64) ^ (ms.rotate_left(17)) ^ 0x9E37_79B9_7F4A_7C15;
    let mut s = base36(ms);
    for _ in 0..6 {
        azar ^= azar << 13;
        azar ^= azar >> 7;
        azar ^= azar << 17;
        s.push(char::from_digit((azar % 36) as u32, 36).unwrap());
    }
    s
}

fn base36(mut n: u64) -> String {
    let mut v = Vec::new();
    loop {
        v.push(char::from_digit((n % 36) as u32, 36).unwrap());
        n /= 36;
        if n == 0 {
            break;
        }
    }
    v.iter().rev().collect()
}
