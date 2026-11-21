//! Los gastos de la actividad: lo que se guarda de cada uno (`Gasto`), sus categorías (con su línea de la Renta,
//! su IVA y su % deducible por defecto) y lo que cuenta cada uno en el IVA y en el IRPF (`gasto_calc`), con las
//! amortizaciones. Los recibos de autónomo de la Seguridad Social también son gastos (`con_recibos`): no se apuntan
//! ni se guardan, salen de lo traído; y una cuota apuntada a mano en un mes con recibo no cuenta (`repetido`).
use super::fmt::{dias_entre, fecha, nombre_mes, r2};
use super::tgss::Recibo;
use super::{Datos, Num};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Gasto {
    pub id: String,
    pub proveedor: String,
    pub nif: String,
    pub concepto: String,
    /// Id de la categoría (ver `CATS`).
    pub categoria: String,
    /// es | ue | extra | sin
    pub origen: String,
    pub base: Num,
    pub iva_tipo: Num,
    /// Cuota de IVA de la factura (vacío: se calcula con base × tipo).
    pub cuota: Num,
    /// % del IVA que se deduce (vacío: el de la categoría).
    pub p_iva: Num,
    /// % del gasto que cuenta en el IRPF (vacío: el de la categoría).
    pub p_irpf: Num,
    /// Los % de una vivienda puestos a mano.
    pub p_manual: bool,
    /// Tipo de dieta (es, es_p, ex, ex_p).
    pub dieta: String,
    /// Tabla de amortización (informatica, mobiliario…); vacío: la de la categoría.
    pub amort_tipo: String,
    pub fecha: String,
    pub num_factura: String,
    pub notas: String,
    /// Factura española cuyo IVA no te puedes deducir (tique sin tu NIF…).
    pub iva_no_ded: bool,
    /// Factura o tique adjunto (en documentos/).
    pub doc: String,
}

/* ================= los recibos de autónomo ================= */

/// Las ids de los gastos que son recibos de la Seguridad Social: «ss:» y el índice del recibo en `tgss.recibos`.
const RECIBO: &str = "ss:";

impl Gasto {
    /// ¿Es un recibo de autónomo de la Seguridad Social? (no se cambia ni se guarda: sale de lo traído)
    pub fn es_recibo(&self) -> bool {
        self.id.starts_with(RECIBO)
    }
}

/// Pone los recibos de autónomo de la Seguridad Social entre tus gastos, como cuota de autónomos y con la fecha del
/// cargo, en lugar de los que hubiera: al cargar los datos y al traer lo último.
pub fn con_recibos(d: &mut Datos) {
    d.gastos.retain(|g| !g.es_recibo());
    let recibos: Vec<Gasto> = d
        .tgss
        .recibos
        .iter()
        .enumerate()
        .filter(|(_, r)| r.importe.abs() > 0.005)
        .map(|(i, r)| {
            let f = r.fecha_cargo();
            let fecha = match f.len() {
                7 => format!("{f}-01"),
                n if n >= 10 => f[..10].to_string(),
                _ => f.to_string(),
            };
            let concepto = if r.es_cuota() && r.periodo.len() >= 7 { format!("Cuota de {}", nombre_mes(&r.periodo)) } else { r.concepto.trim().to_string() };
            Gasto { id: format!("{RECIBO}{i}"), proveedor: "Seguridad Social".into(), concepto, categoria: "ss".into(), origen: "sin".into(), base: Num::de(r.importe), fecha, ..Default::default() }
        })
        .collect();
    d.gastos.extend(recibos);
}

/// Los gastos apuntados (sin los recibos de la Seguridad Social): para saber si hay actividad. Pagar la cuota de
/// autónomo no dice si toca presentar el 303 o el 130, ni que un año tenga datos en la app.
pub fn apuntados(d: &Datos) -> impl Iterator<Item = &Gasto> {
    d.gastos.iter().filter(|g| !g.es_recibo())
}

/// El recibo de la Seguridad Social de un gasto que lo es.
pub fn recibo_de<'a>(g: &Gasto, d: &'a Datos) -> Option<&'a Recibo> {
    g.id.strip_prefix(RECIBO)?.parse::<usize>().ok().and_then(|i| d.tgss.recibos.get(i))
}

/// Una cuota de autónomos apuntada a mano en un mes en que ya hay un recibo de la Seguridad Social: no cuenta (si
/// no, se contaría dos veces).
pub fn repetido(g: &Gasto, d: &Datos) -> bool {
    g.categoria == "ss" && !g.es_recibo() && g.fecha.len() >= 7 && d.tgss.recibos.iter().any(|r| r.fecha_cargo().get(..7) == g.fecha.get(..7))
}

/* ================= categorías ================= */

/// Líneas de gastos de la Renta (estimación directa): clave, casilla, nombre.
pub const RENTA_LINEAS: [(&str, &str, &str); 17] = [
    ("compras", "0181", "Compra de existencias"),
    ("consumos", "0183", "Otros consumos de explotación"),
    ("sueldos", "0184", "Sueldos y salarios"),
    ("ss_empresa", "0185", "Seguridad Social a cargo de la empresa"),
    ("ss_titular", "0186", "Seguridad Social del titular de la actividad"),
    ("manutencion", "0191", "Gastos de manutención del contribuyente"),
    ("arrendamientos", "0192", "Arrendamientos y cánones"),
    ("reparaciones", "0193", "Reparaciones y conservación"),
    ("suministros", "0194", "Suministros (electricidad, agua, gas, telefonía e internet)"),
    ("profesionales", "0199", "Servicios de profesionales independientes"),
    ("seguros", "0200", "Primas de seguros"),
    ("otros_servicios", "0202", "Otros servicios exteriores"),
    ("financieros", "0203", "Gastos financieros"),
    ("tributos", "0206", "Otros tributos fiscalmente deducibles"),
    ("amortizaciones", "0208", "Amortización del inmovilizado material"),
    ("otros_conceptos", "0217", "Otros conceptos fiscalmente deducibles"),
    ("provisiones", "0222", "Provisiones y gastos de difícil justificación"),
];

#[derive(Debug, Clone, Copy)]
pub struct Cat {
    pub id: &'static str,
    pub grupo: &'static str,
    pub label: &'static str,
    /// Línea de la Renta (clave de `RENTA_LINEAS`).
    pub renta: &'static str,
    /// IVA por defecto (%).
    pub iva: f64,
    /// % deducible por defecto del IVA y del IRPF.
    pub p_iva: f64,
    pub p_irpf: f64,
    /// Origen por defecto: es | sin
    pub origen: &'static str,
    /// vivienda | dietas | salud | ""
    pub especial: &'static str,
    /// Tabla de amortización (se amortiza si cuesta más de 300 €): informatica | mobiliario | ""
    pub amort: &'static str,
    pub ayuda: &'static str,
}

const fn cat(id: &'static str, grupo: &'static str, label: &'static str, renta: &'static str, iva: f64) -> Cat {
    Cat { id, grupo, label, renta, iva, p_iva: 100.0, p_irpf: 100.0, origen: "es", especial: "", amort: "", ayuda: "" }
}

pub const CATS: &[Cat] = &[
    Cat { ayuda: "Hosting, dominios, herramientas online… Sin IVA de una empresa de la UE: origen «UE»; de fuera de la UE: «fuera de la UE».", ..cat("software", "Habituales", "Software, apps y suscripciones", "otros_servicios", 21.0) },
    cat("publicidad", "Habituales", "Publicidad y marketing", "otros_servicios", 21.0),
    cat("material", "Habituales", "Material de oficina", "consumos", 21.0),
    cat("profesionales", "Habituales", "Gestoría, asesoría, abogados…", "profesionales", 21.0),
    Cat { ayuda: "Los libros en papel o digitales llevan IVA del 4 %.", ..cat("formacion", "Habituales", "Formación, cursos y libros", "otros_servicios", 21.0) },
    Cat { ayuda: "Para un local o una línea solo profesional. Si es de uso mixto, baja el % deducible.", ..cat("suministros", "Oficina y casa", "Luz, internet y teléfono del negocio", "suministros", 21.0) },
    Cat { especial: "vivienda", ayuda: "Se deduce el 30 % de la parte de la casa afecta (m² del despacho ÷ m² totales).", ..cat("vivienda", "Oficina y casa", "Suministros de casa (trabajo desde casa)", "suministros", 21.0) },
    Cat { ayuda: "Si alquilas un local a un particular o empresa, normalmente tienes que retener el 19 % (modelo 115).", ..cat("alquiler", "Oficina y casa", "Alquiler de local, oficina o coworking", "arrendamientos", 21.0) },
    cat("reparaciones", "Oficina y casa", "Reparaciones y mantenimiento", "reparaciones", 21.0),
    Cat { amort: "informatica", ayuda: "Hasta 300 € (sin IVA) se deduce entero; si cuesta más, se amortiza un 26 % al año.", ..cat("equipo", "Equipos (se amortizan)", "Ordenador, móvil, tablet y equipos informáticos", "amortizaciones", 21.0) },
    Cat { amort: "mobiliario", ayuda: "Hasta 300 € (sin IVA) se deduce entero; si no, se amortiza (10 % mobiliario, 12 % maquinaria).", ..cat("mobiliario", "Equipos (se amortizan)", "Mobiliario, maquinaria y otros bienes duraderos", "amortizaciones", 21.0) },
    cat("viajes", "Viajes y vehículo", "Viajes, transporte y alojamiento", "otros_servicios", 10.0),
    Cat { p_iva: 0.0, especial: "dietas", ayuda: "Con límite diario, pagando con tarjeta en un restaurante y por tu trabajo. El IVA, solo con factura completa a tu nombre.", ..cat("dietas", "Viajes y vehículo", "Comidas de trabajo (dietas)", "manutencion", 10.0) },
    Cat { p_iva: 50.0, p_irpf: 0.0, ayuda: "Por defecto: 50 % del IVA y nada en IRPF (solo si el coche es de uso exclusivo profesional).", ..cat("vehiculo", "Viajes y vehículo", "Vehículo (gasolina, parking, taller…)", "consumos", 21.0) },
    Cat { origen: "sin", ..cat("seguros", "Sin IVA", "Seguros del negocio (RC, local…)", "seguros", 0.0) },
    Cat { origen: "sin", especial: "salud", ayuda: "Deducible hasta 500 € al año por persona cubierta.", ..cat("salud", "Sin IVA", "Seguro de salud (tú, pareja e hijos)", "seguros", 0.0) },
    Cat { origen: "sin", ayuda: "Tu cuota mensual a la Seguridad Social.", ..cat("ss", "Sin IVA", "Cuota de autónomos (RETA / mutualidad)", "ss_titular", 0.0) },
    Cat { origen: "sin", ..cat("banco", "Sin IVA", "Comisiones bancarias", "otros_servicios", 0.0) },
    Cat { origen: "sin", ..cat("intereses", "Sin IVA", "Intereses de préstamos del negocio", "financieros", 0.0) },
    Cat { origen: "sin", ayuda: "No son deducibles las multas, recargos ni el propio IRPF.", ..cat("tributos", "Sin IVA", "Impuestos y tasas (IBI, IAE, tasas…)", "tributos", 0.0) },
    cat("compras", "Otros", "Compras y mercaderías", "compras", 21.0),
    cat("otros", "Otros", "Otros gastos deducibles", "otros_conceptos", 21.0),
];

/// La categoría de un gasto; si no se conoce, «Otros gastos deducibles».
pub fn cat_de(id: &str) -> &'static Cat {
    CATS.iter().find(|c| c.id == id).unwrap_or(&CATS[CATS.len() - 1])
}

pub const ORIGENES: [(&str, &str); 4] = [
    ("es", "España (factura con IVA)"),
    ("ue", "Empresa de la UE (factura sin IVA)"),
    ("extra", "Empresa de fuera de la UE (sin IVA)"),
    ("sin", "Sin IVA (exento o no sujeto)"),
];

/// Tablas de amortización simplificada: clave, nombre, % anual.
pub const AMORT: [(&str, &str, f64); 5] = [
    ("informatica", "Equipos informáticos", 26.0),
    ("mobiliario", "Mobiliario y enseres", 10.0),
    ("maquinaria", "Maquinaria", 12.0),
    ("transporte", "Elementos de transporte", 16.0),
    ("herramientas", "Útiles y herramientas", 30.0),
];

/// Dietas: clave, nombre, límite diario.
pub const DIETAS: [(&str, &str, f64); 4] = [
    ("es", "España, sin pernoctar", 26.67),
    ("es_p", "España, pernoctando", 53.34),
    ("ex", "Extranjero, sin pernoctar", 48.08),
    ("ex_p", "Extranjero, pernoctando", 91.35),
];

/* ================= lo que cuenta cada gasto ================= */

#[derive(Debug, Clone)]
pub struct GastoCalc {
    pub cat: &'static Cat,
    pub origen: String,
    pub base: f64,
    pub tipo: f64,
    /// Cuota de IVA (en los de la UE o de fuera, la que autoliquidas tú).
    pub cuota: f64,
    pub p_iva: f64,
    pub p_irpf: f64,
    /// IVA que te deduces.
    pub iva_ded: f64,
    /// Coste fiscal en el IRPF: la parte afecta del gasto más el IVA que no recuperas.
    pub coste: f64,
    /// Se amortiza (equipos de más de 300 €).
    pub amortizable: bool,
    /// Bien de inversión para el IVA (más de 3.005,06 €).
    pub inversion_iva: bool,
    /// Lo que pagaste.
    pub pagado: f64,
    /// Una cuota apuntada a mano en un mes con recibo de la Seguridad Social: no cuenta.
    pub repetido: bool,
}

/// % de la vivienda afecto a la actividad (30 % de m² despacho ÷ m² totales).
pub fn pct_vivienda(d: &Datos) -> f64 {
    let v = d.fiscal.m2_vivienda.v();
    let dd = d.fiscal.m2_despacho.v();
    if v <= 0.0 || dd <= 0.0 {
        return 0.0;
    }
    r2(30.0 * (dd / v).min(1.0))
}

pub fn gasto_calc(g: &Gasto, d: &Datos) -> GastoCalc {
    let cat = cat_de(&g.categoria);
    let origen = if g.origen.is_empty() { "es".to_string() } else { g.origen.clone() };
    let base = g.base.v();
    let tipo = if origen == "sin" { 0.0 } else { g.iva_tipo.v() };
    let cuota = if origen == "es" {
        g.cuota.0.unwrap_or_else(|| r2(base * tipo / 100.0))
    } else if origen != "sin" {
        r2(base * tipo / 100.0) // inversión del sujeto pasivo: la autoliquidas tú
    } else {
        0.0
    };
    let mut p_iva = g.p_iva.0.unwrap_or(cat.p_iva);
    let mut p_irpf = g.p_irpf.0.unwrap_or(cat.p_irpf);
    if cat.especial == "vivienda" && !g.p_manual {
        p_iva = pct_vivienda(d);
        p_irpf = p_iva;
    }
    if origen == "sin" || (origen == "es" && g.iva_no_ded) {
        p_iva = 0.0;
    }
    let iva_ded = r2(cuota * p_iva / 100.0);
    let mut coste = r2((base + cuota) * p_irpf / 100.0 - iva_ded).max(0.0);
    if cat.especial == "dietas" {
        let lim = DIETAS.iter().find(|x| x.0 == g.dieta).or(DIETAS.first()).map(|x| x.2).unwrap_or(26.67);
        coste = coste.min(lim);
    }
    // Un recibo de la Seguridad Social cuenta entero (y los de devolución, restando).
    if g.es_recibo() {
        coste = base;
    }
    let amortizable = !cat.amort.is_empty() && base > 300.0;
    let inversion_iva = !cat.amort.is_empty() && base > 3005.06;
    let mut pagado = if origen == "es" { r2(base + cuota) } else { base };
    let repetido = repetido(g, d);
    if repetido {
        (coste, pagado) = (0.0, 0.0);
    }
    GastoCalc { cat, origen, base, tipo, cuota, p_iva, p_irpf, iva_ded, coste, amortizable, inversion_iva, pagado, repetido }
}

/* ================= amortizaciones ================= */

/// % anual de amortización de un gasto.
pub fn amort_coef(g: &Gasto) -> f64 {
    let clave = if g.amort_tipo.is_empty() { cat_de(&g.categoria).amort } else { g.amort_tipo.as_str() };
    AMORT.iter().find(|a| a.0 == clave).map(|a| a.2).unwrap_or(10.0)
}

/// Amortización de un bien en [desde, hasta] (ambos incluidos), por días.
pub fn amort_en_rango(g: &Gasto, gc: &GastoCalc, desde: &str, hasta: &str) -> f64 {
    let valor = gc.coste;
    if valor.is_nan() || valor <= 0.0 || g.fecha.as_str() > hasta {
        return 0.0;
    }
    // Sin fecha (o con una que no lo es): 0.
    if fecha(&g.fecha).is_none() {
        return 0.0;
    }
    let anual = valor * amort_coef(g) / 100.0;
    let a = if g.fecha.as_str() > desde { g.fecha.as_str() } else { desde };
    let ya = valor.min(anual * dias_entre(&g.fecha, a) as f64 / 365.0);
    let dias = dias_entre(a, hasta) + 1;
    r2((valor - ya).min(anual * dias as f64 / 365.0).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::tgss::Recibo;

    /// Los recibos de autónomo entran como gastos (sin repetirse al volver a ponerlos), no se guardan, y una cuota
    /// apuntada a mano en un mes con recibo no cuenta.
    #[test]
    fn los_recibos_son_gastos() {
        let mut d = Datos::default();
        let recibo = |periodo: &str, cargo: &str, importe: f64| Recibo { periodo: periodo.into(), concepto: "Cuota autónomos".into(), importe, cargo: cargo.into(), ..Default::default() };
        d.tgss.recibos = vec![recibo("2026-01", "2026-01-30", 300.0), recibo("2026-02", "2026-02-27", 300.0), recibo("2026-03", "", 0.0)];
        d.gastos.push(Gasto { id: "a".into(), categoria: "ss".into(), origen: "sin".into(), base: Num::de(300.0), fecha: "2026-02-27".into(), ..Default::default() });
        d.gastos.push(Gasto { id: "b".into(), categoria: "ss".into(), origen: "sin".into(), base: Num::de(300.0), fecha: "2026-04-30".into(), ..Default::default() });
        con_recibos(&mut d);
        con_recibos(&mut d);
        let recibos: Vec<&Gasto> = d.gastos.iter().filter(|g| g.es_recibo()).collect();
        assert_eq!(recibos.len(), 2, "sin repetirse y sin los de 0 €");
        assert_eq!((recibos[0].fecha.as_str(), recibos[0].concepto.as_str()), ("2026-01-30", "Cuota de enero de 2026"));
        assert_eq!(gasto_calc(recibos[0], &d).coste, 300.0);
        assert_eq!(recibo_de(recibos[1], &d).map(|r| r.periodo.as_str()), Some("2026-02"));
        // La de febrero apuntada a mano no cuenta (ya está el recibo); la de abril, sí.
        let (a, b) = (gasto_calc(&d.gastos[0], &d), gasto_calc(&d.gastos[1], &d));
        assert!(a.repetido && a.coste == 0.0 && a.pagado == 0.0);
        assert!(!b.repetido && b.coste == 300.0);
        // Al guardar, solo los apuntados.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("\"ss:") && json.contains("\"id\":\"a\"") && json.contains("\"id\":\"b\""), "{json}");
    }

    #[test]
    fn categorias() {
        assert_eq!(cat_de("viajes").iva, 10.0);
        assert_eq!(cat_de("no-existe").id, "otros");
        assert_eq!(CATS.len(), 22);
    }
}
