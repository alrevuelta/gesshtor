//! Lo presentado en Hacienda: lo que dio la consulta de un año, modelo a modelo (ver `hacienda::consulta`).
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// La consulta de un año.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Consulta {
    /// De quién (si cambia el NIF, se empieza de cero).
    pub nif: String,
    /// Cuándo se consultaron todos los modelos del año («2026-10-15T09:05:30»); vacío si quedó a medias.
    pub fecha: String,
    /// Por modelo («303», «036»…): cuándo se miró y lo que había presentado.
    pub modelos: BTreeMap<String, Modelo>,
    /// Los modelos que no se pudieron consultar, con el motivo.
    pub errores: BTreeMap<String, Error>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Modelo {
    pub fecha: String,
    pub decl: Vec<Decl>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Error {
    pub fecha: String,
    pub error: String,
}

/// Una declaración presentada.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Decl {
    pub modelo: String,
    pub ejercicio: i32,
    /// «1T»…«4T», «0A» (anual), «01»…«12» (mensual)
    pub periodo: String,
    pub csv: String,
    pub justificante: String,
    /// Fecha y hora de presentación: «2026-10-15T09:05:30»
    pub fecha: String,
    /// Justificante de la declaración anterior, si es complementaria o sustitutiva.
    pub anterior: String,
}

impl Decl {
    /// Trimestre (1–4) si es trimestral.
    pub fn trimestre(&self) -> Option<u32> {
        let p = self.periodo.strip_suffix('T')?;
        p.parse().ok().filter(|q| (1..=4).contains(q))
    }
    /// «2026-3T-303», «2025-100».
    pub fn clave(&self) -> String {
        clave(self.ejercicio, self.trimestre().unwrap_or(0), &self.modelo)
    }
}

/// «2026-3T-303» (trimestral) o «2025-100» (anual).
pub fn clave(anio: i32, q: u32, modelo: &str) -> String {
    if q > 0 { format!("{anio}-{q}T-{modelo}") } else { format!("{anio}-{modelo}") }
}

/// El tipo de una declaración: cómo se presentó. Es la letra que usa Hacienda en sus diseños de registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// I: a ingresar, pagado en el banco (con NRC).
    Ingreso,
    /// U: a ingresar, domiciliado.
    Domiciliado,
    /// G: a ingresar en la cuenta corriente tributaria.
    CuentaCorriente,
    /// R: a ingresar con reconocimiento de deuda (para aplazarlo o fraccionarlo).
    ReconocimientoDeuda,
    /// C: negativa, a compensar en las siguientes.
    Compensar,
    /// D: negativa, a devolver.
    Devolver,
    /// V: a devolver en la cuenta corriente tributaria.
    DevolverEnCuentaCorriente,
    /// X: a devolver por transferencia al extranjero.
    DevolverAlExtranjero,
    /// N: sin actividad o con resultado cero.
    Cero,
    /// B: negativa, sin nada que compensar ni devolver (130).
    Negativa,
}

impl Tipo {
    const LETRAS: [(Tipo, &'static str); 10] = [
        (Tipo::Ingreso, "I"),
        (Tipo::Domiciliado, "U"),
        (Tipo::CuentaCorriente, "G"),
        (Tipo::ReconocimientoDeuda, "R"),
        (Tipo::Compensar, "C"),
        (Tipo::Devolver, "D"),
        (Tipo::DevolverEnCuentaCorriente, "V"),
        (Tipo::DevolverAlExtranjero, "X"),
        (Tipo::Cero, "N"),
        (Tipo::Negativa, "B"),
    ];

    /// Su letra («C»).
    pub fn letra(self) -> &'static str {
        Self::LETRAS.iter().find(|(t, _)| *t == self).map_or("", |x| x.1)
    }

    /// El tipo de una letra («C»), si es una de las conocidas.
    pub fn de_letra(s: &str) -> Option<Tipo> {
        Self::LETRAS.iter().find(|x| x.1 == s).map(|x| x.0)
    }

    /// ¿Se pidió que Hacienda devuelva el dinero?
    pub fn devuelve(self) -> bool {
        matches!(self, Tipo::Devolver | Tipo::DevolverEnCuentaCorriente | Tipo::DevolverAlExtranjero)
    }
}

/// Un `Option<Tipo>` en JSON: su letra, o «» si no se sabe (como se ha guardado siempre).
pub mod letra {
    use super::Tipo;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(t: &Option<Tipo>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(t.map_or("", Tipo::letra))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Tipo>, D::Error> {
        Ok(Tipo::de_letra(&String::deserialize(d)?))
    }
}

impl Consulta {
    /// Guarda encima una consulta nueva: si es de otro NIF se empieza de cero; lo consultado bien sustituye a lo
    /// anterior; si un modelo falló, se apunta el error y se conserva lo que había de él.
    pub fn mezclar(anterior: Option<&Consulta>, nueva: Consulta) -> Consulta {
        let mut h = anterior.filter(|a| a.nif == nueva.nif).cloned().unwrap_or_else(|| Consulta { nif: nueva.nif.clone(), ..Default::default() });
        h.errores.extend(nueva.errores);
        for (m, e) in nueva.modelos {
            h.errores.remove(&m);
            h.modelos.insert(m, e);
        }
        if !nueva.fecha.is_empty() {
            h.fecha = nueva.fecha;
        }
        h
    }
}
