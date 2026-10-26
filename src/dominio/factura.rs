//! Las facturas que emites: lo que se guarda de cada una (`Factura`, con su `Cliente` y sus `Linea`s) y lo que
//! se calcula con ellas (totales, numeración de la serie, mención legal del IVA).
use super::fmt::r2;
use super::{Datos, Num};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Cliente {
    pub id: String,
    pub nombre: String,
    pub nif: String,
    /// Código ISO de dos letras: ES, US, IE…
    pub pais: String,
    pub direccion: String,
    pub cp: String,
    pub ciudad: String,
    pub email: String,
    /// La operación habitual con este cliente: nacional | ue | extra | exenta
    pub operacion: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Factura {
    pub id: String,
    /// 'borrador' | 'emitida'
    pub estado: String,
    /// Se asigna al emitir.
    pub numero: String,
    pub fecha: String,
    /// Si la operación es de otra fecha (devengo); si no, vacío.
    pub fecha_operacion: String,
    pub vencimiento: String,
    /// nacional | ue | extra | exenta
    pub operacion: String,
    pub cliente: Cliente,
    pub lineas: Vec<Linea>,
    /// Retención de IRPF (%).
    pub retencion: Num,
    pub forma_pago: String,
    pub notas: String,
    /// Motivo de la exención (operación exenta).
    pub exencion: String,
    pub creada: String,
    pub emitida: Option<String>,
    /// Fecha de cobro (YYYY-MM-DD) o null.
    pub cobrada: Option<String>,
    /// Justificante adjunto (en documentos/).
    pub doc: String,
    /// Factura de prueba: no cuenta para impuestos.
    pub prueba: bool,
    /// La factura que rectifica, si es rectificativa.
    pub rectifica: Option<Rectifica>,
}

/// La factura que rectifica una rectificativa.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Rectifica {
    pub id: String,
    pub numero: String,
    pub fecha: String,
}

impl Factura {
    pub fn emitida(&self) -> bool {
        self.estado == "emitida"
    }
    pub fn borrador(&self) -> bool {
        self.estado == "borrador"
    }
    pub fn cobrada(&self) -> bool {
        self.cobrada.as_deref().is_some_and(|s| !s.is_empty())
    }
    /// Fecha que cuenta para los impuestos: la de la operación si la hay.
    pub fn devengo(&self) -> &str {
        if self.fecha_operacion.is_empty() { &self.fecha } else { &self.fecha_operacion }
    }
    /// Fecha de la factura que rectifica, si es rectificativa.
    pub fn rectifica_fecha(&self) -> Option<&str> {
        self.rectifica.as_ref().map(|r| r.fecha.as_str()).filter(|s| !s.is_empty())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Linea {
    pub concepto: String,
    pub cantidad: Num,
    pub precio: Num,
    /// IVA de la línea (%): solo cuenta en las operaciones nacionales.
    pub iva: Num,
}

/* ================= tipos de operación ================= */

pub const OPERACIONES: [(&str, &str); 4] = [
    ("nacional", "Con IVA: cliente en España o particular de la UE"),
    ("ue", "Sin IVA: empresa de otro país de la UE (inversión del sujeto pasivo)"),
    ("extra", "Sin IVA: cliente de fuera de la UE, Canarias, Ceuta o Melilla"),
    ("exenta", "Exenta de IVA (art. 20 LIVA: sanidad, enseñanza…)"),
];

pub fn nombre_operacion(op: &str) -> &'static str {
    match op {
        "nacional" => "con IVA",
        "ue" => "UE, sin IVA",
        "extra" => "fuera de la UE",
        "exenta" => "exenta",
        _ => "",
    }
}

/// Texto legal de la factura para las operaciones sin IVA.
pub fn mencion_iva(f: &Factura) -> Option<String> {
    match f.operacion.as_str() {
        "ue" => Some("Operación con inversión del sujeto pasivo (art. 84.Uno.2º y 69 Ley 37/1992).".into()),
        "extra" => Some("Operación no sujeta a IVA por reglas de localización (art. 69 Ley 37/1992).".into()),
        "exenta" => Some(format!("Operación exenta de IVA ({}).", if f.exencion.is_empty() { "art. 20 Ley 37/1992" } else { &f.exencion })),
        _ => None,
    }
}

/* ================= totales ================= */

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GrupoIva {
    pub tipo: f64,
    pub base: f64,
    pub cuota: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Totales {
    /// Bases y cuotas por tipo de IVA, de mayor a menor tipo.
    pub desglose: Vec<GrupoIva>,
    pub base: f64,
    pub cuota: f64,
    pub retencion: f64,
    /// Importe de la factura (base + IVA): no resta la retención.
    pub importe_total: f64,
    /// Lo que paga el cliente (importe − retención).
    pub total: f64,
}

pub fn totales_factura(f: &Factura) -> Totales {
    let mut grupos: Vec<GrupoIva> = Vec::new();
    let mut base = 0.0;
    for l in &f.lineas {
        let b = r2(l.cantidad.v() * l.precio.v());
        let tipo = if f.operacion == "nacional" { l.iva.v() } else { 0.0 };
        match grupos.iter_mut().find(|g| g.tipo == tipo) {
            Some(g) => g.base = r2(g.base + b),
            None => grupos.push(GrupoIva { tipo, base: b, cuota: 0.0 }),
        }
        base = r2(base + b);
    }
    let mut cuota = 0.0;
    for g in &mut grupos {
        g.cuota = r2(g.base * g.tipo / 100.0);
        cuota = r2(cuota + g.cuota);
    }
    grupos.sort_by(|a, b| b.tipo.total_cmp(&a.tipo));
    let retencion = r2(base * f.retencion.v() / 100.0);
    let importe_total = r2(base + cuota);
    Totales { desglose: grupos, base, cuota, retencion, importe_total, total: r2(importe_total - retencion) }
}

/// Facturas que cuentan para impuestos: las emitidas que no son de prueba.
pub fn emitidas(d: &Datos) -> impl Iterator<Item = &Factura> {
    d.facturas.iter().filter(|f| f.emitida() && !f.prueba)
}

/* ================= numeración ================= */

/// «AAAA-» → «2026-»
fn serie_de(patron: &str, anio: i32) -> String {
    patron.replace("AAAA", &anio.to_string())
}

pub fn serie_factura(d: &Datos, rect: bool, anio: i32) -> String {
    serie_de(if rect { &d.facturacion.serie_rect } else { &d.facturacion.serie }, anio)
}

/// El número de la próxima factura de la serie: el mayor emitido + 1, con sus dígitos.
pub fn siguiente_numero(d: &Datos, rect: bool, anio: i32) -> String {
    let serie = serie_factura(d, rect, anio);
    let max = d
        .facturas
        .iter()
        .filter(|f| f.emitida() && f.numero.starts_with(&serie))
        .filter_map(|f| f.numero[serie.len()..].trim().parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let dig = d.facturacion.digitos.0.map(|x| x as usize).filter(|x| *x > 0).unwrap_or(3);
    format!("{serie}{:0dig$}", max + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totales() {
        let f = Factura {
            operacion: "nacional".into(),
            retencion: Num::de(15.0),
            lineas: vec![
                Linea { cantidad: Num::de(2.0), precio: Num::de(100.0), iva: Num::de(21.0), ..Default::default() },
                Linea { cantidad: Num::de(1.0), precio: Num::de(50.0), iva: Num::de(10.0), ..Default::default() },
            ],
            ..Default::default()
        };
        let t = totales_factura(&f);
        assert_eq!(t.base, 250.0);
        assert_eq!(t.cuota, 47.0);
        assert_eq!(t.retencion, 37.5);
        assert_eq!(t.total, 259.5);
        assert_eq!(t.desglose[0].tipo, 21.0);
    }
}
