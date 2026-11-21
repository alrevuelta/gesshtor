//! Lo que contesta Claude Haiku no es de fiar: se lee con tipos estrictos (sin campos de más ni de menos), se
//! comprueba (fechas que existen y no son futuras, importes que cuadran, tipos de IVA y de retención que existen) y,
//! si algo falla, no se apunta nada y se dice por qué. Si está bien, se pasa a lo que ya tiene la app: una `Nomina`,
//! un `Gasto` (con sus categorías y lo deducible por defecto de `dominio::gasto`) o una `Factura` emitida.
//!
//! Lo fiscal lo decide la app, no el modelo, con lo que se ve en el documento:
//! - Qué IVA es (art. 90.Uno y 91 de la Ley 37/1992, del IVA, LIVA): el general del 21 %; el 10 % (transporte de
//!   viajeros, hostelería y restaurantes: art. 91.Uno.2, 1.º y 2.º); el 4 % (libros y periódicos: art. 91.Dos.1.2.º);
//!   y sin IVA, lo exento (sanidad, enseñanza, seguros, operaciones financieras: art. 20.Uno, 2.º, 3.º, 9.º, 16.º y
//!   18.º) y lo no sujeto.
//! - De dónde viene un gasto (`Gasto::origen`): con IVA español, «es»; sin IVA, de una empresa de otro país de la UE,
//!   «ue» (adquisición intracomunitaria, art. 13 y 85 LIVA, o servicio con inversión del sujeto pasivo); de fuera de
//!   la UE, «extra» (servicios de una empresa no establecida: art. 69.Uno.1.º y 84.Uno.2.º a) LIVA). En esos dos, el
//!   IVA lo autoliquidas tú (al tipo de la categoría). Con IVA de otro país, no se deduce aquí: cuenta entero.
//! - Si su IVA se deduce: hace falta la factura (art. 97.Uno.1.º LIVA) con tu NIF y tu domicilio y la cuota aparte
//!   (art. 6.1 y 7.2 del Reglamento de facturación, RD 1619/2012). Un tique sin tu NIF o con el IVA incluido sin
//!   desglosar: `iva_no_ded`. Lo demás (restaurantes, art. 96.Uno.6.º LIVA; el 50 % de los vehículos, art.
//!   95.Tres.2.ª) ya lo hacen las categorías con su % deducible por defecto.
//! - Una factura emitida: con IVA, «nacional»; sin IVA, a una empresa de la UE, «ue»; de fuera, «extra» (no sujeta
//!   por el art. 69 LIVA); a alguien de España sin IVA, «exenta». La retención de IRPF, la que diga: el 15 % o el 7 %
//!   los primeros años (art. 101.5.a) de la Ley 35/2006 y art. 95.1 del RD 439/2007); solo retienen los pagadores de
//!   España (art. 76.1 del RD 439/2007).
//! - En otra moneda: se pide el importe en euros antes de guardar. Para el IVA cuenta el cambio vendedor del Banco
//!   de España del día del devengo (art. 79.Once LIVA); lo que cobra el banco se le parece, y se puede afinar después.
use crate::dominio::fmt::{self, r2};
use crate::dominio::gasto::{cat_de, CATS};
use crate::dominio::{uid, Cliente, Datos, Factura, Gasto, Linea, Nomina, Num, PRIMER_ANIO};
use crate::hacienda::modelo_303::TipoIva;
use serde::Deserialize;
use serde_json::Value;

/// Lo que puede ser un documento.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tipo {
    Nomina,
    Gasto,
    Ingreso,
    Otro,
}

/// Un documento entendido y comprobado, listo para apuntar.
#[derive(Debug, Clone)]
pub struct Entendido {
    pub apunte: Apunte,
    /// Qué es, en palabras del modelo («factura de un hotel»).
    pub que: String,
    /// Si no está en euros: (moneda, total en esa moneda). Antes de guardarlo hay que poner los euros (`en_euros`).
    pub moneda: Option<(String, f64)>,
    /// Lo que conviene saber (que el IVA no se deduce, que la factura queda en borrador…).
    pub avisos: Vec<String>,
}

/// Una nómina, un gasto o una factura emitida, como los guarda la app. (Hay pocos a la vez: no hace falta meterlos
/// en una caja para que ocupen menos.)
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Apunte {
    Nomina(Nomina),
    Gasto(Gasto),
    Ingreso(Factura),
}

/* ================= lo que se espera de Claude ================= */

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clase {
    tipo: String,
    que: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
struct NominaLeida {
    empresa: String,
    anio: i32,
    mes: u32,
    total_devengado: f64,
    en_especie: f64,
    irpf: f64,
    seguridad_social: f64,
    total_a_deducir: f64,
    liquido: f64,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
struct GastoLeido {
    proveedor: String,
    proveedor_nif: String,
    proveedor_pais: String,
    cliente_nif: String,
    numero: String,
    fecha: String,
    concepto: String,
    categoria: String,
    moneda: String,
    base: f64,
    iva_tipo: f64,
    iva_cuota: f64,
    iva_pais: String,
    iva_desglosado: bool,
    total: f64,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
struct IngresoLeido {
    cliente: String,
    cliente_nif: String,
    cliente_pais: String,
    emisor_nif: String,
    numero: String,
    fecha: String,
    concepto: String,
    moneda: String,
    base: f64,
    iva_tipo: f64,
    iva_cuota: f64,
    retencion_tipo: f64,
    retencion_cuota: f64,
    total: f64,
    cobrado: bool,
}

/// Lee una respuesta con su tipo: si falta un campo, sobra uno o no es del tipo, no vale.
fn leer<T: for<'de> Deserialize<'de>>(v: &Value) -> Result<T, String> {
    T::deserialize(v).map_err(|e| format!("La respuesta de Claude no es la esperada ({e}): no se apunta nada."))
}

/// Qué es el documento, y en pocas palabras.
pub fn clase(v: &Value) -> Result<(Tipo, String), String> {
    let c: Clase = leer(v)?;
    let tipo = match c.tipo.as_str() {
        "nomina" => Tipo::Nomina,
        "gasto" => Tipo::Gasto,
        "ingreso" => Tipo::Ingreso,
        "otro" => Tipo::Otro,
        x => return Err(format!("Claude dice que es «{x}», que no es nada de lo que la app sabe apuntar.")),
    };
    Ok((tipo, limpio(&c.que, 80)))
}

/// Los datos de un documento de ese tipo, comprobados y pasados a lo que guarda la app.
pub fn entender(tipo: Tipo, v: &Value, que: String, nif: &str, hoy: &str) -> Result<Entendido, String> {
    let mut e = match tipo {
        Tipo::Nomina => nomina(leer(v)?, hoy)?,
        Tipo::Gasto => gasto(leer(v)?, nif, hoy)?,
        Tipo::Ingreso => ingreso(leer(v)?, nif, hoy)?,
        Tipo::Otro => return Err("No es una nómina, un gasto ni un ingreso.".into()),
    };
    e.que = que;
    Ok(e)
}

/* ================= comprobaciones ================= */

/// Dos importes son el mismo si no se diferencian en más de 5 céntimos (lo que se va al redondear línea a línea).
fn cuadra(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.05
}

/// Un importe que puede ser: sin decimales raros ni negativo ni enorme.
fn importe(x: f64, que: &str) -> Result<f64, String> {
    if !x.is_finite() || !(0.0..1e7).contains(&x) {
        return Err(format!("{que} no puede ser {x}: no se apunta."));
    }
    Ok(r2(x))
}

/// Una fecha «AAAA-MM-DD» que existe, no es futura y es de 2020 en adelante.
fn fecha(s: &str, hoy: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() != 10 || fmt::fecha(s).is_none() {
        return Err(format!("La fecha «{s}» no es una fecha: no se apunta."));
    }
    if s > hoy {
        return Err(format!("La fecha ({}) es futura: no se apunta.", fmt::fecha_corta(s)));
    }
    if fmt::anio_de(s) < PRIMER_ANIO {
        return Err(format!("Es de {}: la app empieza en {PRIMER_ANIO}.", fmt::anio_de(s)));
    }
    Ok(s.to_string())
}

/// Un texto de una línea, sin espacios de más y no más largo de `n` (si se corta, con «…»).
fn limpio(s: &str, n: usize) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= n {
        return t;
    }
    t.chars().take(n - 1).collect::<String>().trim_end().to_string() + "…"
}

/// Un NIF sin espacios, guiones ni puntos, en mayúsculas y sin el «ES» delante de los números de IVA españoles.
fn nif_limpio(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_uppercase()).collect();
    match n.strip_prefix("ES") {
        Some(r) if r.len() == 9 => r.to_string(),
        _ => n,
    }
}

/// Un país en dos letras mayúsculas («GR» es «EL» para el IVA); vacío si no lo es.
fn pais(s: &str) -> String {
    let p = s.trim().to_uppercase();
    if p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic()) { p } else { String::new() }
}

/// Los países de la UE (para el IVA, Grecia es «EL»).
const UE: [&str; 28] = ["AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "GR", "ES", "FI", "FR", "HR", "HU", "IE", "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK"];

/// Una moneda en tres letras mayúsculas («€» y vacío son euros).
fn moneda(s: &str) -> Result<String, String> {
    let m = s.trim().to_uppercase();
    match m.as_str() {
        "" | "€" | "EUR" | "EURO" | "EUROS" => Ok("EUR".into()),
        "$" | "US$" | "USD" => Ok("USD".into()),
        x if x.len() == 3 && x.chars().all(|c| c.is_ascii_alphabetic()) => Ok(m),
        x => Err(format!("No sé qué moneda es «{x}»: no se apunta.")),
    }
}

/* ================= nómina ================= */

/// Una nómina: lo devengado menos lo deducido tiene que dar el líquido (que puede ser negativo, si cobras en
/// especie), y el IRPF y la Seguridad Social no pueden ser más que lo deducido.
fn nomina(r: NominaLeida, hoy: &str) -> Result<Entendido, String> {
    let empresa = limpio(&r.empresa, 80);
    if empresa.is_empty() {
        return Err("No se ve de qué empresa es la nómina: no se apunta.".into());
    }
    if !(1..=12).contains(&r.mes) || r.anio < PRIMER_ANIO || format!("{}-{:02}", r.anio, r.mes).as_str() > fmt::ym(hoy) {
        return Err(format!("La nómina dice que es del mes {} de {}, que no vale: no se apunta.", r.mes, r.anio));
    }
    let devengado = importe(r.total_devengado, "Lo devengado")?;
    let especie = importe(r.en_especie, "Lo cobrado en especie")?;
    let irpf = importe(r.irpf, "El IRPF")?;
    let ss = importe(r.seguridad_social, "La Seguridad Social")?;
    let deducir = importe(r.total_a_deducir, "El total a deducir")?;
    if !r.liquido.is_finite() || !cuadra(devengado - deducir, r.liquido) {
        return Err(format!("No cuadra: devengado {} menos deducciones {} no da el líquido {}. No se apunta.", fmt::eur(devengado), fmt::eur(deducir), fmt::eur(r.liquido)));
    }
    if irpf + ss > deducir + 0.05 {
        return Err("No cuadra: el IRPF y la Seguridad Social suman más que las deducciones. No se apunta.".into());
    }
    if especie > devengado + 0.05 {
        return Err("No cuadra: lo cobrado en especie es más que lo devengado. No se apunta.".into());
    }
    let n = Nomina { id: uid(), anio: r.anio, empresa, mes: r.mes, dinerario: r2((devengado - especie).max(0.0)), especie, retenciones: irpf, ss, doc: String::new() };
    n.validar().map_err(|e| format!("{e} No se apunta."))?;
    Ok(Entendido { apunte: Apunte::Nomina(n), que: String::new(), moneda: None, avisos: Vec::new() })
}

/* ================= gasto ================= */

/// Un tipo de IVA español de hoy (4, 10 o 21 %: los del 303, `modelo_303::TipoIva`; el 0 % es sin IVA).
fn tipo_iva(t: f64) -> Result<f64, String> {
    match TipoIva::de(t) {
        Some(TipoIva::Superreducido | TipoIva::Reducido | TipoIva::General) => Ok(t),
        _ if (t - 5.0).abs() < 0.01 => Err("Un IVA del 5 % (solo lo hubo hasta 2024) no lo calcula la app en el 303: no se apunta.".into()),
        _ => Err(format!("Un IVA del {} no existe en España: no se apunta.", fmt::pct(t))),
    }
}

/// Cuándo hubo un IVA del 5 %: la luz, del 1-7-2022 (Real Decreto-ley 11/2022, art. 18, BOE-A-2022-10557) al
/// 31-12-2023; el gas, las briquetas, los pellets y la leña, del 1-10-2022 (Real Decreto-ley 17/2022,
/// BOE-A-2022-15354) al 31-12-2023; y los aceites de semillas y las pastas, del 1-1-2023 (Real Decreto-ley 20/2022,
/// art. 72, BOE-A-2022-22685) al 30-9-2024 (Real Decreto-ley 4/2024, art. 1, BOE-A-2024-12944). Un gasto de entonces
/// puede llevarlo (en el 303 va con los demás: lo deducible no se separa por tipos).
const CINCO_POR_CIENTO: (&str, &str) = ("2022-07-01", "2024-09-30");

/// El tipo de IVA español de un gasto: uno de hoy o, si es de entonces, el 5 % (`CINCO_POR_CIENTO`).
fn tipo_iva_gasto(t: f64, fecha: &str) -> Result<f64, String> {
    let (desde, hasta) = CINCO_POR_CIENTO;
    if (t - 5.0).abs() < 0.01 && (desde..=hasta).contains(&fecha) {
        return Ok(5.0);
    }
    if (t - 5.0).abs() < 0.01 {
        return Err(format!("Un IVA del 5 % solo lo hubo del {} al {} (la luz, el gas, algunos alimentos), y este gasto es del {}: no se apunta.", fmt::fecha_corta(desde), fmt::fecha_corta(hasta), fmt::fecha_corta(fecha)));
    }
    tipo_iva(t)
}

/// Un gasto. Con el IVA aparte, base + IVA tiene que dar el total; con el IVA incluido, la base sale del total; sin
/// IVA, lo que cuenta es lo pagado (con las tasas que no son IVA, como las de un aeropuerto).
fn gasto(r: GastoLeido, nif: &str, hoy: &str) -> Result<Entendido, String> {
    let fecha = fecha(&r.fecha, hoy)?;
    let moneda = moneda(&r.moneda)?;
    let (base, cuota, total) = (importe(r.base, "La base")?, importe(r.iva_cuota, "El IVA")?, importe(r.total, "El total")?);
    let tipo = importe(r.iva_tipo, "El tipo de IVA")?;
    if total <= fmt::DIF {
        return Err("El gasto sale a 0: no se apunta.".into());
    }
    let cat = CATS.iter().find(|c| c.id == r.categoria).ok_or_else(|| format!("La categoría «{}» no es de la app: no se apunta.", r.categoria))?;
    let proveedor_pais = pais(&r.proveedor_pais);
    let con_iva = tipo > 0.0 || cuota > 0.0;
    let iva_pais = match pais(&r.iva_pais) {
        p if p.is_empty() => proveedor_pais.clone(),
        p => p,
    };
    if con_iva && r.iva_desglosado && !cuadra(base + cuota, total) {
        return Err(format!("No cuadra: base {} más IVA {} no da el total {}. No se apunta.", num(base), num(cuota), num(total)));
    }
    let mut avisos = Vec::new();
    let mut g = Gasto {
        id: uid(),
        proveedor: limpio(&r.proveedor, 80),
        nif: nif_limpio(&r.proveedor_nif),
        concepto: limpio(&r.concepto, 80),
        categoria: cat.id.into(),
        fecha,
        num_factura: limpio(&r.numero, 40),
        ..Default::default()
    };
    if g.proveedor.is_empty() {
        return Err("No se ve quién es el proveedor: no se apunta.".into());
    }
    if con_iva && iva_pais == "ES" {
        // IVA español: con la cuota aparte, la del documento (puede mezclar tipos); incluido, sale del total.
        let tipo = tipo_iva_gasto(tipo, &g.fecha)?;
        let (base, cuota) = if r.iva_desglosado { (base, cuota) } else { (r2(total / (1.0 + tipo / 100.0)), r2(total - r2(total / (1.0 + tipo / 100.0)))) };
        if cuota > r2(base * 0.21) + 0.05 {
            return Err("No cuadra: el IVA es más del 21 % de la base. No se apunta.".into());
        }
        let mio = !nif_limpio(&r.cliente_nif).is_empty() && (nif.is_empty() || nif_limpio(&r.cliente_nif) == nif_limpio(nif));
        g.origen = "es".into();
        g.base = Num::de(base);
        g.iva_tipo = Num::de(tipo);
        // La cuota, solo si no es la de base × tipo (si no, la calcula la app y sigue a la base si se cambia).
        g.cuota = if (r2(base * tipo / 100.0) - cuota).abs() < fmt::DIF { Num(None) } else { Num::de(cuota) };
        g.iva_no_ded = !(r.iva_desglosado && mio);
        if !r.iva_desglosado {
            avisos.push("Su IVA no se deduce: viene incluido, sin la cuota aparte (art. 7.2 del Reglamento de facturación). Cuenta entero en el IRPF.".into());
        } else if !mio {
            avisos.push("Su IVA no se deduce: la factura no va a tu nombre con tu NIF (art. 97 LIVA). Cuenta entero en el IRPF.".into());
        }
    } else if con_iva {
        // IVA de otro país: no se deduce en tu 303, así que todo es gasto.
        g.origen = "sin".into();
        g.base = Num::de(total);
        g.iva_tipo = Num::de(0.0);
        avisos.push(format!("Lleva IVA de otro país ({}): no se deduce en tu 303; cuenta entero como gasto.", if iva_pais.is_empty() { "?" } else { &iva_pais }));
    } else {
        if base > total + 0.05 {
            return Err(format!("No cuadra: la base {} es más que el total {}. No se apunta.", num(base), num(total)));
        }
        g.base = Num::de(total);
        let ue = UE.contains(&proveedor_pais.as_str()) && proveedor_pais != "ES";
        let fuera = !proveedor_pais.is_empty() && !UE.contains(&proveedor_pais.as_str());
        if cat.origen == "sin" || !(ue || fuera) {
            g.origen = "sin".into();
            g.iva_tipo = Num::de(0.0);
            if proveedor_pais.is_empty() {
                avisos.push("No se ve de qué país es el proveedor: queda «sin IVA». Revísalo.".into());
            }
        } else {
            // Inversión del sujeto pasivo: el IVA lo pones tú (y te lo deduces), al tipo de la categoría.
            g.origen = if ue { "ue" } else { "extra" }.into();
            g.iva_tipo = Num::de(if cat.iva > 0.0 { cat.iva } else { 21.0 });
            avisos.push(format!("Sin IVA de una empresa {}: lo autoliquidas tú en el 303 (inversión del sujeto pasivo).", if ue { "de la UE" } else { "de fuera de la UE" }));
        }
    }
    let original = (moneda != "EUR").then_some((moneda, total));
    Ok(Entendido { apunte: Apunte::Gasto(g), que: String::new(), moneda: original, avisos })
}

/* ================= ingreso ================= */

/// Una factura emitida (o el aviso de pago de un cliente): base + IVA − retención tiene que dar el total (o base +
/// IVA, si el total no resta la retención), con un solo tipo de IVA y una retención de las que hay.
fn ingreso(r: IngresoLeido, nif: &str, hoy: &str) -> Result<Entendido, String> {
    let fecha = fecha(&r.fecha, hoy)?;
    let moneda = moneda(&r.moneda)?;
    let (base, cuota, total) = (importe(r.base, "La base")?, importe(r.iva_cuota, "El IVA")?, importe(r.total, "El total")?);
    let (tipo, ret_tipo, ret) = (importe(r.iva_tipo, "El tipo de IVA")?, importe(r.retencion_tipo, "La retención")?, importe(r.retencion_cuota, "La retención")?);
    if base <= fmt::DIF {
        return Err("El ingreso sale a 0: no se apunta.".into());
    }
    let tipo = if tipo > 0.0 || cuota > 0.0 { tipo_iva(tipo)? } else { 0.0 };
    if !cuadra(r2(base * tipo / 100.0), cuota) {
        return Err(format!("No cuadra: el {} de {} no es {} (¿varios tipos de IVA?). Apúntala a mano.", fmt::pct(tipo), num(base), num(cuota)));
    }
    if ![0.0, 7.0, 15.0].iter().any(|x| (x - ret_tipo).abs() < 0.01) {
        return Err(format!("Una retención del {} no es la de un profesional (15 % o 7 %): no se apunta.", fmt::pct(ret_tipo)));
    }
    if !cuadra(r2(base * ret_tipo / 100.0), ret) {
        return Err(format!("No cuadra: el {} de {} no es {}. No se apunta.", fmt::pct(ret_tipo), num(base), num(ret)));
    }
    if !cuadra(base + cuota - ret, total) && !cuadra(base + cuota, total) {
        return Err(format!("No cuadra: base {} más IVA {} menos retención {} no da el total {}. No se apunta.", num(base), num(cuota), num(ret), num(total)));
    }
    let cliente_pais = pais(&r.cliente_pais);
    let mut avisos = Vec::new();
    let operacion = if tipo > 0.0 {
        "nacional"
    } else if cliente_pais == "ES" {
        avisos.push("Sin IVA a un cliente de España: queda como exenta. Revisa el motivo en la factura.".into());
        "exenta"
    } else if UE.contains(&cliente_pais.as_str()) {
        "ue"
    } else {
        if cliente_pais.is_empty() {
            avisos.push("No se ve de qué país es el cliente: queda «fuera de la UE». Revísalo.".into());
        }
        "extra"
    };
    let cliente = Cliente { nombre: limpio(&r.cliente, 80), nif: nif_limpio(&r.cliente_nif), pais: cliente_pais, operacion: operacion.into(), ..Default::default() };
    if cliente.nombre.is_empty() {
        return Err("No se ve quién es el cliente: no se apunta.".into());
    }
    // Tuya y con número: ya está emitida. Si no (el aviso de pago de un cliente), en borrador: al emitirla le da
    // número tu serie. Hay que hacer factura siempre, también sin IVA (art. 2.1 del Reglamento de facturación).
    let tuya = !r.emisor_nif.trim().is_empty() && (nif.is_empty() || nif_limpio(&r.emisor_nif) == nif_limpio(nif));
    let numero = limpio(&r.numero, 40);
    let emitida = tuya && !numero.is_empty();
    if !emitida {
        avisos.push("Queda en borrador: revísala en Facturas y emítela con E (le da número de tu serie).".into());
    }
    let ahora = fmt::ahora();
    let f = Factura {
        id: uid(),
        estado: if emitida { "emitida" } else { "borrador" }.into(),
        numero: if emitida { numero } else { String::new() },
        cobrada: r.cobrado.then(|| fecha.clone()),
        fecha,
        operacion: operacion.into(),
        cliente,
        lineas: vec![Linea { concepto: limpio(&r.concepto, 120), cantidad: Num::de(1.0), precio: Num::de(base), iva: Num::de(tipo) }],
        retencion: Num::de(ret_tipo),
        creada: ahora.clone(),
        emitida: emitida.then_some(ahora),
        ..Default::default()
    };
    let original = (moneda != "EUR").then_some((moneda, total));
    Ok(Entendido { apunte: Apunte::Ingreso(f), que: String::new(), moneda: original, avisos })
}

/// «1.234,50» (sin «€»: puede ser otra moneda).
fn num(x: f64) -> String {
    fmt::num(x)
}

/* ================= en euros, repetidos y guardar ================= */

impl Entendido {
    /// Pasa a euros un documento en otra moneda con lo que te costó (o te pagaron) en euros: todo en proporción.
    pub fn en_euros(&mut self, euros: f64) -> Result<(), String> {
        let Some((m, total)) = self.moneda.clone() else { return Ok(()) };
        if !euros.is_finite() || euros <= fmt::DIF || euros > 1e7 {
            return Err("Escribe cuántos euros fueron.".into());
        }
        let k = euros / total;
        match &mut self.apunte {
            Apunte::Gasto(g) => {
                if g.origen == "es" {
                    let base = r2(g.base.v() * k);
                    g.base = Num::de(base);
                    g.cuota = Num::de(r2(euros - base));
                } else {
                    g.base = Num::de(r2(euros));
                }
                g.notas = limpio(&format!("{} En {m}: {}.", g.notas, num(total)), 200);
            }
            Apunte::Ingreso(f) => {
                for l in &mut f.lineas {
                    l.precio = Num::de(r2(l.precio.v() * k));
                }
                f.notas = limpio(&format!("{} En {m}: {}.", f.notas, num(total)), 200);
            }
            Apunte::Nomina(_) => {}
        }
        self.moneda = None;
        Ok(())
    }
}

/// ¿Ya tienes apuntado algo igual? (aunque sea de otro fichero): dice cuál.
pub fn repetido(d: &Datos, e: &Entendido) -> Option<String> {
    let igual = |a: &str, b: &str| !a.trim().is_empty() && a.trim().to_lowercase() == b.trim().to_lowercase();
    match &e.apunte {
        Apunte::Nomina(n) => d
            .nominas
            .iter()
            .find(|x| x.anio == n.anio && x.mes == n.mes && igual(&x.empresa, &n.empresa) && cuadra(x.dinerario, n.dinerario) && cuadra(x.especie, n.especie) && cuadra(x.retenciones, n.retenciones))
            .map(|_| format!("Ya tienes esta nómina ({} de {}).", fmt::MESES[n.mes as usize - 1], n.anio)),
        Apunte::Gasto(g) => d
            .gastos
            .iter()
            .filter(|x| !x.es_recibo())
            .find(|x| (igual(&x.num_factura, &g.num_factura) && (igual(&x.proveedor, &g.proveedor) || igual(&x.nif, &g.nif))) || (x.fecha == g.fecha && igual(&x.proveedor, &g.proveedor) && cuadra(x.base.v(), g.base.v())))
            .map(|x| format!("Ya tienes este gasto ({}, {}).", x.proveedor, fmt::fecha_corta(&x.fecha))),
        Apunte::Ingreso(f) => {
            let base = |x: &Factura| x.lineas.iter().map(|l| l.cantidad.v() * l.precio.v()).sum::<f64>();
            d.facturas
                .iter()
                .find(|x| (f.emitida() && x.emitida() && x.numero == f.numero) || (x.fecha == f.fecha && igual(&x.cliente.nombre, &f.cliente.nombre) && cuadra(base(x), base(f))))
                .map(|x| format!("Ya tienes esta factura ({}, {}).", x.cliente.nombre, fmt::fecha_corta(&x.fecha)))
        }
    }
}

/// Mete lo entendido en los datos, con su documento (el nombre en `documentos/`), y dice dónde ha ido. Una factura
/// usa tu cliente si ya lo tienes (por NIF o nombre); si ya está emitida, el cliente nuevo queda en tus clientes.
pub fn guardar(d: &mut Datos, e: Entendido, doc: String) -> &'static str {
    match e.apunte {
        Apunte::Nomina(mut n) => {
            n.doc = doc;
            d.nominas.push(n);
            "Empleo › Nóminas"
        }
        Apunte::Gasto(mut g) => {
            g.doc = doc;
            d.gastos.push(g);
            "Autónomo › Gastos"
        }
        Apunte::Ingreso(mut f) => {
            f.doc = doc;
            let c = &f.cliente;
            let tuyo = d.clientes.iter().find(|x| (!c.nif.is_empty() && nif_limpio(&x.nif) == c.nif) || igual_nombre(&x.nombre, &c.nombre));
            match tuyo {
                Some(x) => f.cliente = Cliente { operacion: f.operacion.clone(), ..x.clone() },
                None => {
                    f.cliente.id = uid();
                    if f.emitida() {
                        d.clientes.push(f.cliente.clone());
                    }
                }
            }
            d.facturas.push(f);
            "Autónomo › Facturas"
        }
    }
}

fn igual_nombre(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// La categoría de un gasto, para enseñarla.
pub fn categoria(g: &Gasto) -> &'static str {
    cat_de(&g.categoria).label
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HOY: &str = "2026-11-20";
    const YO: &str = "12345678Z";

    fn g(v: Value) -> Result<Entendido, String> {
        entender(Tipo::Gasto, &v, "un tique".into(), YO, HOY)
    }
    fn i(v: Value) -> Result<Entendido, String> {
        entender(Tipo::Ingreso, &v, "una factura".into(), YO, HOY)
    }
    fn n(v: Value) -> Result<Entendido, String> {
        entender(Tipo::Nomina, &v, "una nómina".into(), YO, HOY)
    }
    fn gasto_de(e: &Entendido) -> &Gasto {
        match &e.apunte {
            Apunte::Gasto(g) => g,
            x => panic!("{x:?}"),
        }
    }
    fn factura_de(e: &Entendido) -> &Factura {
        match &e.apunte {
            Apunte::Ingreso(f) => f,
            x => panic!("{x:?}"),
        }
    }

    /// Un gasto inventado, con IVA español aparte y a tu nombre; cada prueba cambia lo suyo.
    fn tique() -> Value {
        json!({"proveedor": "Papelería Inventada SL", "proveedor_nif": "B00000000", "proveedor_pais": "ES", "cliente_nif": "ES12345678Z",
               "numero": "A-17", "fecha": "2026-10-02", "concepto": "Folios", "categoria": "material", "moneda": "EUR",
               "base": 100.0, "iva_tipo": 21.0, "iva_cuota": 21.0, "iva_pais": "ES", "iva_desglosado": true, "total": 121.0})
    }
    fn con(mut v: Value, cambios: Value) -> Value {
        for (k, x) in cambios.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    }

    #[test]
    fn que_es() {
        assert_eq!(clase(&json!({"tipo": "gasto", "que": "  un   tique "})).unwrap(), (Tipo::Gasto, "un tique".into()));
        assert!(clase(&json!({"tipo": "factura", "que": ""})).unwrap_err().contains("factura"));
        assert!(clase(&json!({"tipo": "gasto"})).unwrap_err().contains("no es la esperada"));
        assert!(clase(&json!({"tipo": "gasto", "que": "", "extra": 1})).unwrap_err().contains("no es la esperada"));
        assert!(clase(&json!("gasto")).is_err());
    }

    /// Con IVA español aparte y a tu nombre: se deduce. Sin tu NIF o con el IVA incluido: no, y la base sale del total.
    #[test]
    fn gasto_con_iva_espanol() {
        let e = g(tique()).unwrap();
        let x = gasto_de(&e);
        assert_eq!((x.origen.as_str(), x.base.v(), x.iva_tipo.v(), x.iva_no_ded, x.categoria.as_str()), ("es", 100.0, 21.0, false, "material"));
        assert!(x.cuota.vacio() && e.avisos.is_empty() && e.moneda.is_none());
        let e = g(con(tique(), json!({"cliente_nif": ""}))).unwrap();
        assert!(gasto_de(&e).iva_no_ded && e.avisos[0].contains("tu NIF"));
        // Un taxi: «IVA incluido 10 %», sin cuota ni base.
        let e = g(con(tique(), json!({"categoria": "viajes", "cliente_nif": "", "iva_tipo": 10.0, "iva_cuota": 0.0, "base": 0.0, "iva_desglosado": false, "total": 42.0}))).unwrap();
        let x = gasto_de(&e);
        assert_eq!((x.base.v(), x.iva_tipo.v(), x.iva_no_ded), (38.18, 10.0, true));
        assert!(x.cuota.vacio(), "3,82 € es el 10 % de 38,18 €: la calcula la app");
        assert!(e.avisos[0].contains("incluido"));
        // Varios tipos en el mismo tique: la cuota, la del tique.
        let e = g(con(tique(), json!({"iva_tipo": 10.0, "iva_cuota": 15.0, "total": 115.0}))).unwrap();
        assert_eq!(gasto_de(&e).cuota.0, Some(15.0));
    }

    /// De fuera: sin IVA de la UE o de fuera, lo autoliquidas tú al tipo de la categoría; con IVA de otro país, todo
    /// es gasto; y un seguro sin IVA es «sin IVA» venga de donde venga.
    #[test]
    fn gasto_de_fuera() {
        let sin_iva = json!({"iva_tipo": 0.0, "iva_cuota": 0.0, "iva_pais": "", "iva_desglosado": false, "base": 40.0, "total": 40.0, "categoria": "software"});
        let e = g(con(con(tique(), sin_iva.clone()), json!({"proveedor_pais": "US", "moneda": "USD"}))).unwrap();
        let x = gasto_de(&e);
        assert_eq!((x.origen.as_str(), x.base.v(), x.iva_tipo.v()), ("extra", 40.0, 21.0));
        assert_eq!(e.moneda, Some(("USD".into(), 40.0)));
        let e = g(con(con(tique(), sin_iva.clone()), json!({"proveedor_pais": "IE"}))).unwrap();
        assert_eq!(gasto_de(&e).origen, "ue");
        let e = g(con(con(tique(), sin_iva.clone()), json!({"proveedor_pais": "IE", "categoria": "seguros"}))).unwrap();
        assert_eq!(gasto_de(&e).origen, "sin");
        // Sin IVA y de España (un seguro, un banco), o con tasas que no son IVA: cuenta lo pagado.
        let e = g(con(con(tique(), sin_iva), json!({"base": 30.0, "total": 33.5}))).unwrap();
        assert_eq!((gasto_de(&e).origen.as_str(), gasto_de(&e).base.v()), ("sin", 33.5));
        // Con el IVA de otro país.
        let e = g(con(tique(), json!({"proveedor_pais": "DE", "iva_pais": "DE", "iva_tipo": 19.0, "iva_cuota": 19.0, "total": 119.0}))).unwrap();
        assert_eq!((gasto_de(&e).origen.as_str(), gasto_de(&e).base.v()), ("sin", 119.0));
        // Una empresa de fuera que cobra IVA español sin tu NIF: IVA español, no deducible.
        let e = g(con(tique(), json!({"proveedor_pais": "US", "cliente_nif": "", "moneda": "USD"}))).unwrap();
        assert_eq!((gasto_de(&e).origen.as_str(), gasto_de(&e).iva_no_ded), ("es", true));
    }

    /// Respuestas malas: no se apunta nada.
    #[test]
    fn gasto_que_no_cuadra() {
        let malos = [
            (json!({"total": 125.0}), "no da el total"),
            (json!({"fecha": "2026-02-30"}), "no es una fecha"),
            (json!({"fecha": "02/10/2026"}), "no es una fecha"),
            (json!({"fecha": "2027-01-01"}), "futura"),
            (json!({"fecha": "2019-12-31"}), "2020"),
            (json!({"iva_tipo": 19.0, "iva_cuota": 19.0, "total": 119.0}), "no existe en España"),
            (json!({"base": -100.0, "total": -79.0}), "no puede ser"),
            (json!({"total": 0.0, "base": 0.0, "iva_cuota": 0.0}), "0"),
            (json!({"categoria": "caprichos"}), "no es de la app"),
            (json!({"moneda": "dólares"}), "moneda"),
            (json!({"proveedor": " "}), "proveedor"),
            (json!({"iva_tipo": 21.0, "iva_cuota": 40.0, "base": 100.0, "total": 140.0}), "21 %"),
        ];
        for (cambio, error) in malos {
            let r = g(con(tique(), cambio.clone()));
            assert!(r.as_ref().is_err_and(|e| e.contains(error)), "{cambio}: {r:?}");
        }
        // Rotas: un campo que falta, uno que sobra, un importe en texto.
        let mut v = tique();
        v.as_object_mut().unwrap().remove("total");
        assert!(g(v).unwrap_err().contains("no es la esperada"));
        assert!(g(con(tique(), json!({"propina": 2.0}))).unwrap_err().contains("no es la esperada"));
        assert!(g(con(tique(), json!({"total": "121,00"}))).unwrap_err().contains("no es la esperada"));
    }

    /// El 5 %: solo del 1-7-2022 al 30-9-2024 (la luz, el gas, algunos alimentos), y solo en un gasto: hoy no existe y
    /// el 303 no lo calcula.
    #[test]
    fn el_cinco_por_ciento_solo_cuando_existio() {
        let cinco = json!({"iva_tipo": 5.0, "iva_cuota": 5.0, "total": 105.0});
        let e = g(con(con(tique(), cinco.clone()), json!({"fecha": "2023-02-01"}))).unwrap();
        assert_eq!(gasto_de(&e).iva_tipo.v(), 5.0);
        for fecha in ["2022-06-30", "2024-10-01", "2026-10-02"] {
            let r = g(con(con(tique(), cinco.clone()), json!({"fecha": fecha})));
            assert!(r.as_ref().is_err_and(|e| e.contains("5 %")), "{fecha}: {r:?}");
        }
        let r = i(con(factura(), json!({"fecha": "2023-02-01", "iva_tipo": 5.0, "iva_cuota": 50.0, "total": 900.0})));
        assert!(r.as_ref().is_err_and(|e| e.contains("5 %")), "{r:?}");
    }

    /// Una nómina: el líquido puede ser negativo (las de en especie); lo que no cuadra no se apunta.
    #[test]
    fn nominas() {
        let base = json!({"empresa": "Empresa Inventada SL", "anio": 2025, "mes": 3, "total_devengado": 3000.0, "en_especie": 0.0,
                          "irpf": 450.0, "seguridad_social": 195.0, "total_a_deducir": 645.0, "liquido": 2355.0});
        let e = n(base.clone()).unwrap();
        let Apunte::Nomina(x) = &e.apunte else { panic!() };
        assert_eq!((x.anio, x.mes, x.dinerario, x.especie, x.retenciones, x.ss), (2025, 3, 3000.0, 0.0, 450.0, 195.0));
        // En especie: lo devengado se descuenta y sale a pagar.
        let especie = json!({"total_devengado": 1000.0, "en_especie": 1000.0, "irpf": 380.0, "seguridad_social": 2.0, "total_a_deducir": 1382.0, "liquido": -382.0});
        let e = n(con(base.clone(), especie)).unwrap();
        let Apunte::Nomina(x) = &e.apunte else { panic!() };
        assert_eq!((x.dinerario, x.especie), (0.0, 1000.0));
        for (cambio, error) in [
            (json!({"liquido": 2400.0}), "no da el líquido"),
            (json!({"irpf": 700.0}), "suman más"),
            (json!({"en_especie": 4000.0}), "especie"),
            (json!({"mes": 13}), "no vale"),
            (json!({"anio": 2026, "mes": 12}), "no vale"),
            (json!({"empresa": ""}), "empresa"),
            (json!({"total_devengado": 0.0, "total_a_deducir": 0.0, "irpf": 0.0, "seguridad_social": 0.0, "liquido": 0.0}), "bruto"),
        ] {
            let r = n(con(base.clone(), cambio.clone()));
            assert!(r.as_ref().is_err_and(|e| e.contains(error)), "{cambio}: {r:?}");
        }
        assert!(n(con(base, json!({"mes": 3.5}))).unwrap_err().contains("no es la esperada"));
    }

    fn factura() -> Value {
        json!({"cliente": "Cliente Inventado SL", "cliente_nif": "B11111111", "cliente_pais": "ES", "emisor_nif": "12345678-Z", "numero": "2026-007",
               "fecha": "2026-10-05", "concepto": "Desarrollo web", "moneda": "EUR", "base": 1000.0, "iva_tipo": 21.0, "iva_cuota": 210.0,
               "retencion_tipo": 15.0, "retencion_cuota": 150.0, "total": 1060.0, "cobrado": false})
    }

    /// Tu factura con número: emitida, con su número, IVA y retención. El aviso de pago de un cliente de fuera: en
    /// borrador, sin IVA, «fuera de la UE» y cobrada.
    #[test]
    fn ingresos() {
        let e = i(factura()).unwrap();
        let f = factura_de(&e);
        assert_eq!((f.estado.as_str(), f.numero.as_str(), f.operacion.as_str(), f.retencion.v()), ("emitida", "2026-007", "nacional", 15.0));
        let t = crate::dominio::factura::totales_factura(f);
        assert_eq!((t.base, t.cuota, t.retencion, t.total), (1000.0, 210.0, 150.0, 1060.0));
        // Total sin restar la retención: también vale.
        assert!(i(con(factura(), json!({"total": 1210.0}))).is_ok());
        let aviso = json!({"cliente": "Inventada Inc", "cliente_nif": "", "cliente_pais": "US", "emisor_nif": "", "numero": "", "moneda": "USD",
                           "iva_tipo": 0.0, "iva_cuota": 0.0, "retencion_tipo": 0.0, "retencion_cuota": 0.0, "total": 1000.0, "cobrado": true});
        let e = i(con(factura(), aviso)).unwrap();
        let f = factura_de(&e);
        assert_eq!((f.estado.as_str(), f.numero.as_str(), f.operacion.as_str(), f.cobrada.as_deref()), ("borrador", "", "extra", Some("2026-10-05")));
        assert!(e.avisos.iter().any(|a| a.contains("borrador")));
        assert_eq!(e.moneda, Some(("USD".into(), 1000.0)));
        // Sin IVA a una empresa de la UE.
        let e = i(con(factura(), json!({"cliente_pais": "FR", "iva_tipo": 0.0, "iva_cuota": 0.0, "retencion_tipo": 0.0, "retencion_cuota": 0.0, "total": 1000.0}))).unwrap();
        assert_eq!(factura_de(&e).operacion, "ue");
        // De otro NIF (no la emites tú): borrador.
        let e = i(con(factura(), json!({"emisor_nif": "B99999999"}))).unwrap();
        assert_eq!(factura_de(&e).estado, "borrador");
        for (cambio, error) in [
            (json!({"iva_cuota": 100.0}), "varios tipos"),
            (json!({"retencion_tipo": 19.0, "retencion_cuota": 190.0}), "profesional"),
            (json!({"retencion_cuota": 100.0}), "no es"),
            (json!({"total": 999.0}), "no da el total"),
            (json!({"cliente": ""}), "cliente"),
            (json!({"base": 0.0, "iva_cuota": 0.0, "retencion_cuota": 0.0, "total": 0.0}), "0"),
        ] {
            let r = i(con(factura(), cambio.clone()));
            assert!(r.as_ref().is_err_and(|e| e.contains(error)), "{cambio}: {r:?}");
        }
    }

    /// En dólares: con los euros de verdad, todo en proporción; sin ellos, no se puede guardar.
    #[test]
    fn otra_moneda() {
        let mut e = g(con(tique(), json!({"proveedor_pais": "US", "moneda": "USD", "base": 20.0, "iva_cuota": 4.2, "total": 24.2}))).unwrap();
        assert!(e.en_euros(0.0).is_err());
        e.en_euros(20.7).unwrap();
        let x = gasto_de(&e);
        assert_eq!((x.base.v(), x.cuota.0), (17.11, Some(3.59)));
        assert!(x.notas.contains("USD") && e.moneda.is_none());
        let mut e = i(con(factura(), json!({"moneda": "USD", "cliente_pais": "US", "iva_tipo": 0.0, "iva_cuota": 0.0, "retencion_tipo": 0.0, "retencion_cuota": 0.0, "total": 1000.0}))).unwrap();
        e.en_euros(900.0).unwrap();
        assert_eq!(crate::dominio::factura::totales_factura(factura_de(&e)).total, 900.0);
    }

    /// Guardar: cada cosa a su sitio, con su documento; lo repetido se reconoce; el cliente que ya tienes se usa.
    #[test]
    fn a_su_sitio_y_sin_repetir() {
        let mut d = Datos::default();
        d.clientes.push(Cliente { id: "c1".into(), nombre: "Cliente Inventado SL".into(), nif: "B-11111111".into(), pais: "ES".into(), email: "a@b.es".into(), ..Default::default() });
        let gasto = g(tique()).unwrap();
        assert!(repetido(&d, &gasto).is_none());
        assert_eq!(guardar(&mut d, gasto.clone(), "tique-abc.pdf".into()), "Autónomo › Gastos");
        assert_eq!(d.gastos[0].doc, "tique-abc.pdf");
        assert!(repetido(&d, &gasto).unwrap().contains("Ya tienes este gasto"));
        // Otro tique del mismo proveedor y día, con otro importe y sin número: no es el mismo.
        assert!(repetido(&d, &g(con(tique(), json!({"numero": "", "base": 50.0, "iva_cuota": 10.5, "total": 60.5}))).unwrap()).is_none());
        let fac = i(factura()).unwrap();
        assert_eq!(guardar(&mut d, fac.clone(), String::new()), "Autónomo › Facturas");
        assert_eq!((d.facturas[0].cliente.id.as_str(), d.facturas[0].cliente.email.as_str()), ("c1", "a@b.es"));
        assert_eq!(d.clientes.len(), 1);
        assert!(repetido(&d, &fac).is_some());
        let nom = n(json!({"empresa": "Empresa Inventada SL", "anio": 2025, "mes": 4, "total_devengado": 3000.0, "en_especie": 0.0, "irpf": 450.0, "seguridad_social": 195.0, "total_a_deducir": 645.0, "liquido": 2355.0})).unwrap();
        assert_eq!(guardar(&mut d, nom.clone(), "n.pdf".into()), "Empleo › Nóminas");
        assert!(repetido(&d, &nom).unwrap().contains("abril de 2025") || repetido(&d, &nom).unwrap().contains("abril"));
        // Otra nómina del mismo mes y empresa (la de en especie): no es la misma.
        let especie = n(json!({"empresa": "Empresa Inventada SL", "anio": 2025, "mes": 4, "total_devengado": 1000.0, "en_especie": 1000.0, "irpf": 380.0, "seguridad_social": 2.0, "total_a_deducir": 1382.0, "liquido": -382.0})).unwrap();
        assert!(repetido(&d, &especie).is_none());
    }
}
