//! Lo de la Seguridad Social (`tgss` en datos.json): autónomo (alta, actividad, cuota, recibos, regularizaciones)
//! y las cotizaciones de las nóminas mes a mes. Es lo que dan los lectores de sus páginas (`tgss`).
//!
//! Al leer `datos.json` se acepta lo que haya: un importe que no viene (o no es un número) cuenta como 0, y un texto
//! que no viene, como vacío.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

fn cero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => crate::dominio::fmt::parse_num(&s),
        _ => 0.0,
    })
}

fn txt<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Null => String::new(),
        v => v.to_string(),
    })
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Tgss {
    /// ¿Está (o estuvo) en el régimen de autónomos?
    pub reta: bool,
    pub alta: Alta,
    /// Lo que figura como actividad («Trabajador por mi cuenta», «Socio…»).
    pub actividades: Vec<Actividad>,
    /// La mutua que cubre las contingencias profesionales.
    pub mutua: Mutua,
    pub cotizacion: Cotizacion,
    /// Recibos de la cuota de autónomo.
    pub recibos: Vec<Recibo>,
    /// Regularizaciones anuales de la cuota (según los rendimientos reales).
    pub regularizaciones: Vec<Regularizacion>,
    /// Cuándo se consultó.
    #[serde(deserialize_with = "txt")]
    pub consultado: String,
    /// Cotizaciones por cuenta ajena, mes a mes (lo que calcula la Seguridad Social de tu nómina).
    pub nomina: NominaSs,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Alta {
    /// Fecha de alta como autónomo (YYYY-MM-DD).
    #[serde(deserialize_with = "txt")]
    pub fecha: String,
    #[serde(deserialize_with = "txt")]
    pub cotiza_desde: String,
    #[serde(deserialize_with = "txt")]
    pub baja: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Actividad {
    /// Fecha de inicio (YYYY-MM-DD).
    #[serde(deserialize_with = "txt")]
    pub inicio: String,
    #[serde(deserialize_with = "txt")]
    pub tipo: String,
    /// Código CNAE y su descripción.
    #[serde(deserialize_with = "txt")]
    pub cnae: String,
    #[serde(deserialize_with = "txt")]
    pub cnae_texto: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Mutua {
    #[serde(deserialize_with = "txt")]
    pub entidad: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Cotizacion {
    /// Rendimientos netos previstos que declaraste (para elegir la base).
    pub rendimientos_previstos: Option<f64>,
    pub base: Option<f64>,
    pub base_proxima: Option<f64>,
    #[serde(deserialize_with = "txt")]
    pub base_proxima_desde: String,
    pub ultima_cuota: Option<f64>,
    #[serde(deserialize_with = "txt")]
    pub ultima_cuota_mes: String,
}

/// Un recibo mensual de autónomo, con su desglose.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Recibo {
    /// Mes al que corresponde: «2026-10».
    #[serde(deserialize_with = "txt")]
    pub periodo: String,
    /// Mes en que se emitió.
    #[serde(deserialize_with = "txt")]
    pub emision: String,
    #[serde(deserialize_with = "txt")]
    pub concepto: String,
    #[serde(deserialize_with = "cero")]
    pub importe: f64,
    /// Su desglose (en `datos.json`, junto a lo demás: `cc`, `cp`…).
    #[serde(flatten)]
    pub conceptos: Conceptos,
    /// Fecha de cargo en tu cuenta (si consta: mejor `fecha_cargo`).
    #[serde(deserialize_with = "txt")]
    pub cargo: String,
}

impl Tgss {
    /// El último recibo que es la cuota de un mes (no unos atrasos): el del mes más reciente.
    pub fn ultima_cuota(&self) -> Option<&Recibo> {
        self.recibos.iter().filter(|r| r.es_cuota()).max_by(|a, b| a.periodo.cmp(&b.periodo))
    }
}

/// % de contingencias comunes: la base de cotización ≈ contingencias comunes ÷ 28,3 %.
pub const TIPO_CC: f64 = 28.3;

impl Recibo {
    /// El día del cargo en tu cuenta; si no consta, el mes de emisión o, si tampoco, el que paga.
    pub fn fecha_cargo(&self) -> &str {
        [self.cargo.as_str(), self.emision.as_str(), self.periodo.as_str()].into_iter().find(|s| !s.is_empty()).unwrap_or("")
    }
    /// ¿Es la cuota de un mes («Cuota autónomos»)? Si no, son atrasos u otra cosa.
    pub fn es_cuota(&self) -> bool {
        let c = self.concepto.trim().to_lowercase();
        c.is_empty() || c.starts_with("cuota")
    }
    /// ¿Son atrasos de otro mes (una regularización de la cuota mensual)?
    pub fn atrasos(&self) -> bool {
        self.concepto.trim().to_lowercase().starts_with("regulariz")
    }
    /// ¿Se carga después de `hoy`?
    pub fn por_cargar(&self, hoy: &str) -> bool {
        let c = self.fecha_cargo();
        c.len() >= 10 && c > hoy
    }
    /// La base de cotización que sale del recibo (contingencias comunes ÷ 28,3 %), en las cuotas de un mes.
    pub fn base(&self) -> Option<f64> {
        let cc = self.conceptos.cc;
        (self.es_cuota() && cc > 0.0).then(|| super::fmt::r2(cc / TIPO_CC * 100.0))
    }
}

/// Lo que pagas en una cuota, por conceptos.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(default)]
pub struct Conceptos {
    /// Contingencias comunes: pensión, bajas por enfermedad, nacimiento…
    #[serde(deserialize_with = "cero")]
    pub cc: f64,
    /// Contingencias profesionales: accidentes de trabajo y enfermedades profesionales.
    #[serde(deserialize_with = "cero")]
    pub cp: f64,
    /// Cese de actividad (el «paro» del autónomo).
    #[serde(deserialize_with = "cero")]
    pub cese: f64,
    /// Formación profesional.
    #[serde(deserialize_with = "cero")]
    pub fp: f64,
    /// Mecanismo de equidad intergeneracional.
    #[serde(deserialize_with = "cero")]
    pub mei: f64,
}

impl Conceptos {
    pub fn suma(&self) -> f64 {
        super::fmt::r2(self.cc + self.cp + self.cese + self.fp + self.mei)
    }
    pub fn sumar(&mut self, o: &Conceptos) {
        let r2 = super::fmt::r2;
        self.cc = r2(self.cc + o.cc);
        self.cp = r2(self.cp + o.cp);
        self.cese = r2(self.cese + o.cese);
        self.fp = r2(self.fp + o.fp);
        self.mei = r2(self.mei + o.mei);
    }
}

/// La regularización de un año: lo que sale al comparar la cuota pagada con la que tocaba por tus rendimientos.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Regularizacion {
    pub anio: i32,
    pub resultado: Resultado,
    #[serde(deserialize_with = "cero")]
    pub importe: f64,
    pub base: Option<f64>,
    pub rendimiento_anual: Option<f64>,
    pub rendimiento_mensual: Option<f64>,
    pub promedio_provisional: Option<f64>,
    pub pagado: Option<bool>,
}

/// Lo que sale de una regularización. En `datos.json`: «ingresar», «devolver», «sin» o «» (no se sabe).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resultado {
    Ingresar,
    Devolver,
    SinDiferencias,
    #[default]
    NoSe,
}

impl Resultado {
    fn texto(self) -> &'static str {
        match self {
            Resultado::Ingresar => "ingresar",
            Resultado::Devolver => "devolver",
            Resultado::SinDiferencias => "sin",
            Resultado::NoSe => "",
        }
    }
}

impl Serialize for Resultado {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.texto())
    }
}

/// Lo que no se conoce, `NoSe` (no falla: así se abre cualquier `datos.json`).
impl<'de> Deserialize<'de> for Resultado {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match txt(d)?.as_str() {
            "ingresar" => Resultado::Ingresar,
            "devolver" => Resultado::Devolver,
            "sin" => Resultado::SinDiferencias,
            _ => Resultado::NoSe,
        })
    }
}

/// «2025-12» → las liquidaciones de ese mes (una por empresa y tipo). Vacío: ese mes no hubo.
pub type Meses = BTreeMap<String, Vec<Liquidacion>>;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct NominaSs {
    pub meses: Meses,
    /// «2025-12» → cuándo se consultó ese mes (`tgss::ahora`).
    pub consultado: BTreeMap<String, String>,
}

/// Una liquidación de cotizaciones de un mes y una empresa.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Liquidacion {
    #[serde(deserialize_with = "txt")]
    pub empresa: String,
    /// Código de cuenta de cotización de la empresa.
    #[serde(deserialize_with = "txt")]
    pub ccc: String,
    /// Tipo de liquidación (L00 normal, L03 atrasos…) y su descripción.
    #[serde(deserialize_with = "txt")]
    pub tipo: String,
    #[serde(deserialize_with = "txt")]
    pub descripcion: String,
    #[serde(deserialize_with = "txt")]
    pub calculada: String,
    /// Mes en que se presentó («2026-01»).
    #[serde(deserialize_with = "txt")]
    pub presentada: String,
    pub conceptos: Vec<ConceptoSs>,
    #[serde(deserialize_with = "cero")]
    pub cuota_empresa: f64,
    #[serde(deserialize_with = "cero")]
    pub cuota_trabajador: f64,
}

impl Liquidacion {
    /// ¿Es una complementaria (atrasos…), no la normal del mes (L00)?
    pub fn complementaria(&self) -> bool {
        !self.tipo.is_empty() && self.tipo != "L00"
    }
    /// El sueldo por el que se cotiza: la base de contingencias comunes y, si pasa de la máxima, lo de los tramos
    /// de solidaridad.
    pub fn sueldo(&self) -> f64 {
        let cuenta = |c: &&ConceptoSs| {
            let n = c.concepto.trim().to_uppercase();
            n == "CONTINGENCIAS COMUNES" || n.contains("SOLIDARIDAD")
        };
        super::fmt::r2(self.conceptos.iter().filter(cuenta).map(|c| c.base).sum())
    }
}

/// Un concepto de una liquidación: contingencias comunes, desempleo, MEI, solidaridad…
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct ConceptoSs {
    /// Tal cual lo escribe la Seguridad Social, en mayúsculas.
    #[serde(deserialize_with = "txt")]
    pub concepto: String,
    #[serde(deserialize_with = "cero")]
    pub base: f64,
    #[serde(deserialize_with = "cero")]
    pub empresa: f64,
    #[serde(deserialize_with = "cero")]
    pub trabajador: f64,
    #[serde(deserialize_with = "cero")]
    pub total: f64,
}

impl ConceptoSs {
    /// ¿Es una línea de totales («LIQUIDO DE TOTALES», «LIQUIDO CONTINGENCIAS COMUNES»…)? Repite lo de los demás.
    pub fn es_total(&self) -> bool {
        self.concepto.trim().to_uppercase().starts_with("LIQUIDO")
    }
}
