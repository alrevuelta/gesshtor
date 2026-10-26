//! El IRPF que calcula la app:
//!
//! 1. **El de la actividad** (`irpf_acumulado`): lo acumulado desde el 1 de enero con tus facturas y gastos
//!    (ingresos, gastos por línea de la Renta, difícil justificación y rendimiento neto). Lo usan el 130, la
//!    pantalla de actividad y el informe.
//! 2. **El de un sueldo** (`cuota_sueldo`): lo que toca pagar en la Renta por los rendimientos del trabajo y nada
//!    más. Sirve para saber si te retienen bien. Supone la situación más común: sin hijos ni ascendientes a cargo,
//!    sin discapacidad, con menos de 65 años y sin otras rentas.
//! 3. **Las reglas de cada año** (`GDJ`, `ESTATAL`, `AUTONOMICAS`): la difícil justificación, y escalas, mínimo
//!    personal, reducción y deducción por rendimientos del trabajo, sacadas del «Manual práctico de Renta» de cada
//!    año. Para añadir un año: copiar el último y cambiar lo que cambie según el Manual de ese año.

use crate::dominio::factura::{emitidas, totales_factura};
use crate::dominio::fmt::{en_rango, r2};
use crate::dominio::gasto::{amort_en_rango, gasto_calc, RENTA_LINEAS};
use crate::dominio::Datos;
use std::collections::BTreeMap;

/* ================= 1. el de la actividad (acumulado desde el 1 de enero) ================= */

/// % y máximo de los gastos de difícil justificación de un año (ver `GDJ`); para otro año usa el último que tenga
/// anterior o igual, y si no hay ninguno, el primero.
pub fn gdj_reglas(anio: i32) -> (f64, f64) {
    let r = GDJ.iter().rev().find(|g| g.0 <= anio).unwrap_or(&GDJ[0]);
    (r.1, r.2)
}

/// Lo acumulado del IRPF de la actividad desde el 1 de enero hasta una fecha.
#[derive(Debug, Clone, Default)]
pub struct Acumulado {
    pub ingresos: f64,
    pub retenciones: f64,
    /// Ingresos de facturas con retención (para la exención del 130).
    pub ingresos_con_ret: f64,
    /// Gastos por línea de la Renta (claves de `RENTA_LINEAS`).
    pub lineas: BTreeMap<&'static str, f64>,
    /// Todos los gastos, con el de difícil justificación.
    pub gastos: f64,
    pub gdj: f64,
    /// Rendimiento neto (ingresos − gastos).
    pub rn: f64,
}

pub fn irpf_acumulado(d: &Datos, anio: i32, hasta: &str) -> Acumulado {
    let desde = format!("{anio}-01-01");
    let mut lineas: BTreeMap<&'static str, f64> = RENTA_LINEAS.iter().map(|l| (l.0, 0.0)).collect();
    let (mut ingresos, mut retenciones, mut ingresos_con_ret) = (0.0, 0.0, 0.0);
    for f in emitidas(d) {
        if !en_rango(f.devengo(), &desde, hasta) {
            continue;
        }
        let t = totales_factura(f);
        ingresos = r2(ingresos + t.base);
        retenciones = r2(retenciones + t.retencion);
        if t.retencion > 0.0 {
            ingresos_con_ret = r2(ingresos_con_ret + t.base);
        }
    }
    let mut salud = 0.0;
    fn sumar(lineas: &mut BTreeMap<&'static str, f64>, k: &'static str, v: f64) {
        let x = lineas.entry(k).or_insert(0.0);
        *x = r2(*x + v);
    }
    for g in &d.gastos {
        let gc = gasto_calc(g, d);
        if gc.amortizable {
            sumar(&mut lineas, "amortizaciones", amort_en_rango(g, &gc, &desde, hasta));
        } else if en_rango(&g.fecha, &desde, hasta) {
            if gc.cat.especial == "salud" {
                salud = r2(salud + gc.coste);
            } else {
                sumar(&mut lineas, gc.cat.renta, gc.coste);
            }
        }
    }
    // 500 € por persona cubierta (`Math.max(1, +personasSalud || 1)`).
    let personas = d.fiscal.personas_salud.v();
    let personas = if personas == 0.0 || personas.is_nan() { 1.0 } else { personas };
    let salud_max = 500.0 * personas.max(1.0);
    let salud_ded = salud.min(salud_max);
    sumar(&mut lineas, "seguros", salud_ded);
    let mut subtotal = 0.0;
    for (k, _, _) in RENTA_LINEAS {
        if k != "provisiones" {
            subtotal = r2(subtotal + lineas[k]);
        }
    }
    let rn_previo = r2(ingresos - subtotal);
    let (pct, max) = gdj_reglas(anio); // 5 % (7 % en 2023), con un máximo
    let gdj = if d.fiscal.gdj { max.min(r2(rn_previo * pct / 100.0).max(0.0)) } else { 0.0 };
    lineas.insert("provisiones", gdj);
    let gastos = r2(subtotal + gdj);
    Acumulado { ingresos, retenciones, ingresos_con_ret, lineas, gastos, gdj, rn: r2(ingresos - gastos) }
}

/* ================= 2. el de un sueldo ================= */

/// Una escala: (hasta, tipo %, cuota al empezar el tramo).
pub type Escala = &'static [(f64, f64, f64)];

/// Un tramo de una reducción o una deducción que baja en línea recta: hasta `hasta`,
/// `base − pendiente × (x − desde)`.
#[derive(Debug, Clone, Copy)]
pub struct Lineal {
    pub desde: f64,
    pub hasta: f64,
    pub base: f64,
    pub pendiente: f64,
}

/// Lo estatal de un año.
#[derive(Debug)]
pub struct Estatal {
    pub escala: Escala,
    /// Mínimo personal.
    pub minimo: f64,
    /// Otros gastos del trabajo (art. 19.2.f).
    pub otros_gastos: f64,
    /// Reducción por rendimientos del trabajo (art. 20), sobre el rendimiento neto previo.
    pub reduccion: &'static [Lineal],
    /// Deducción por obtención de rendimientos del trabajo (desde 2025), sobre el bruto.
    pub deduccion: &'static [Lineal],
}

/// Lo de una comunidad en un año.
#[derive(Debug)]
pub struct Autonomica {
    pub escala: Escala,
    pub minimo: f64,
}

/// Las comunidades: (clave, nombre). Las cuatro últimas no tienen reglas: Ceuta y Melilla tienen su propia
/// deducción, y País Vasco y Navarra su IRPF.
pub const COMUNIDADES: [(&str, &str); 19] = [
    ("andalucia", "Andalucía"),
    ("aragon", "Aragón"),
    ("asturias", "Principado de Asturias"),
    ("baleares", "Illes Balears"),
    ("canarias", "Canarias"),
    ("cantabria", "Cantabria"),
    ("castilla-mancha", "Castilla-La Mancha"),
    ("castilla-leon", "Castilla y León"),
    ("cataluna", "Cataluña"),
    ("extremadura", "Extremadura"),
    ("galicia", "Galicia"),
    ("madrid", "Comunidad de Madrid"),
    ("murcia", "Región de Murcia"),
    ("rioja", "La Rioja"),
    ("valenciana", "Comunitat Valenciana"),
    ("ceuta", "Ceuta"),
    ("melilla", "Melilla"),
    ("pais-vasco", "País Vasco (régimen foral)"),
    ("navarra", "Navarra (régimen foral)"),
];

/// «castilla-leon» → «Castilla y León» (la clave tal cual si no se conoce).
pub fn nombre_comunidad(clave: &str) -> &str {
    COMUNIDADES.iter().find(|c| c.0 == clave).map_or(clave, |c| c.1)
}

/// Las reglas con que se calcula un año.
#[derive(Debug, Clone, Copy)]
pub struct Reglas {
    /// El año de las reglas: si la app no tiene las del año pedido, las del último que tiene antes (o el primero).
    pub anio: i32,
    pub estatal: &'static Estatal,
    /// Las de tu comunidad; None si no se sabe o no las tiene la app (entonces la escala estatal hace también de
    /// autonómica: una aproximación).
    pub autonomica: Option<&'static Autonomica>,
}

/// Las reglas de un año para una comunidad (por su clave; "" si no se sabe).
pub fn reglas(anio: i32, comunidad: &str) -> Reglas {
    let e = ESTATAL;
    let (y, estatal) = e.iter().rev().find(|(y, _)| *y <= anio).unwrap_or(&e[0]);
    let autonomica = AUTONOMICAS.iter().find(|(c, _)| *c == comunidad).and_then(|(_, v)| v.iter().find(|(a, _)| a == y)).map(|(_, a)| a);
    Reglas { anio: *y, estatal, autonomica }
}

/// La cuota de una base con una escala, como las tablas oficiales: la del principio del tramo y el resto al tipo.
pub fn por_tramos(escala: Escala, base: f64) -> f64 {
    let base = base.max(0.0);
    let mut desde = 0.0;
    for &(hasta, tipo, cuota) in escala {
        if base <= hasta {
            return r2(cuota + (base - desde) * tipo / 100.0);
        }
        desde = hasta;
    }
    escala.last().map_or(0.0, |&(_, tipo, cuota)| r2(cuota + (base - desde) * tipo / 100.0))
}

/// Lo que da una reducción o deducción por tramos en `x` (0 si pasa del último).
fn lineal(t: &[Lineal], x: f64) -> f64 {
    t.iter().find(|t| x <= t.hasta).map_or(0.0, |t| (t.base - t.pendiente * (x - t.desde)).max(0.0))
}

/// Lo que toca de IRPF por un sueldo de un año entero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cuota {
    /// Base liquidable: el rendimiento neto del trabajo, con sus reducciones.
    pub base: f64,
    pub estatal: f64,
    pub autonomica: f64,
    /// Deducción por obtención de rendimientos del trabajo.
    pub deduccion: f64,
    /// Lo que toca pagar (cuota líquida).
    pub total: f64,
}

/// El IRPF de un sueldo bruto anual (`integros`, con lo de en especie) del que te descuentan `ss` de Seguridad
/// Social, sin otras rentas.
pub fn cuota_sueldo(r: &Reglas, integros: f64, ss: f64) -> Cuota {
    let e = r.estatal;
    let previo = r2(integros - ss).max(0.0);
    let otros = e.otros_gastos.min(previo);
    let reduccion = r2(lineal(e.reduccion, previo).min(previo - otros));
    let base = r2(previo - otros - reduccion);
    let (escala, minimo) = r.autonomica.map_or((e.escala, e.minimo), |a| (a.escala, a.minimo));
    let estatal = r2(por_tramos(e.escala, base) - por_tramos(e.escala, base.min(e.minimo)));
    let autonomica = r2(por_tramos(escala, base) - por_tramos(escala, base.min(minimo)));
    let integra = r2(estatal + autonomica);
    let deduccion = r2(lineal(e.deduccion, integros).min(integra));
    Cuota { base, estatal, autonomica, deduccion, total: r2(integra - deduccion) }
}

/* ================= 3. las reglas de cada año ================= */

/// Gastos de difícil justificación: (año, %, máximo). 7 % en 2023 y 5 % los demás, hasta 2.000 €.
const GDJ: [(i32, f64, f64); 4] = [(2022, 5.0, 2000.0), (2023, 7.0, 2000.0), (2024, 5.0, 2000.0), (2025, 5.0, 2000.0)];

// Las del sueldo, de territorio común: sin País Vasco ni Navarra, que tienen IRPF foral, ni Ceuta y Melilla.
// Escalas: (hasta, tipo %, cuota al empezar el tramo), como las tablas oficiales.

/// Las estatales.
const ESTATAL: &[(i32, Estatal)] = &[
    (2022, Estatal {
        escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (300000.0, 22.5, 8950.75), (f64::INFINITY, 24.5, 62950.75)],
        minimo: 5550.0,
        otros_gastos: 2000.0,
        reduccion: &[Lineal { desde: 0.0, hasta: 13115.0, base: 5565.0, pendiente: 0.0 }, Lineal { desde: 13115.0, hasta: 16825.0, base: 5565.0, pendiente: 1.5 }],
        deduccion: &[],
    }),
    (2023, Estatal {
        escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (300000.0, 22.5, 8950.75), (f64::INFINITY, 24.5, 62950.75)],
        minimo: 5550.0,
        otros_gastos: 2000.0,
        reduccion: &[Lineal { desde: 0.0, hasta: 14047.5, base: 6498.0, pendiente: 0.0 }, Lineal { desde: 14047.5, hasta: 19747.5, base: 6498.0, pendiente: 1.14 }],
        deduccion: &[],
    }),
    (2024, Estatal {
        escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (300000.0, 22.5, 8950.75), (f64::INFINITY, 24.5, 62950.75)],
        minimo: 5550.0,
        otros_gastos: 2000.0,
        reduccion: &[Lineal { desde: 0.0, hasta: 14852.0, base: 7302.0, pendiente: 0.0 }, Lineal { desde: 14852.0, hasta: 17673.52, base: 7302.0, pendiente: 1.75 }, Lineal { desde: 17673.52, hasta: 19747.5, base: 2364.34, pendiente: 1.14 }],
        deduccion: &[],
    }),
    (2025, Estatal {
        escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (300000.0, 22.5, 8950.75), (f64::INFINITY, 24.5, 62950.75)],
        minimo: 5550.0,
        otros_gastos: 2000.0,
        reduccion: &[Lineal { desde: 0.0, hasta: 14852.0, base: 7302.0, pendiente: 0.0 }, Lineal { desde: 14852.0, hasta: 17673.52, base: 7302.0, pendiente: 1.75 }, Lineal { desde: 17673.52, hasta: 19747.5, base: 2364.34, pendiente: 1.14 }],
        deduccion: &[Lineal { desde: 0.0, hasta: 16576.0, base: 340.0, pendiente: 0.0 }, Lineal { desde: 16576.0, hasta: 18276.0, base: 340.0, pendiente: 0.2 }],
    }),
];

/// Las de cada comunidad, por su clave (la de `COMUNIDADES`).
const AUTONOMICAS: &[(&str, &[(i32, Autonomica)])] = &[
    (
        "andalucia",
        &[
            (2022, Autonomica { escala: &[(13000.0, 9.5, 0.0), (21100.0, 12.0, 1235.0), (35200.0, 15.0, 2207.0), (60000.0, 18.5, 4322.0), (f64::INFINITY, 22.5, 8910.0)], minimo: 5790.0 }),
            (2023, Autonomica { escala: &[(13000.0, 9.5, 0.0), (21100.0, 12.0, 1235.0), (35200.0, 15.0, 2207.0), (60000.0, 18.5, 4322.0), (f64::INFINITY, 22.5, 8910.0)], minimo: 5790.0 }),
            (2024, Autonomica { escala: &[(13000.0, 9.5, 0.0), (21100.0, 12.0, 1235.0), (35200.0, 15.0, 2207.0), (60000.0, 18.5, 4322.0), (f64::INFINITY, 22.5, 8910.0)], minimo: 5790.0 }),
            (2025, Autonomica { escala: &[(13000.0, 9.5, 0.0), (21100.0, 12.0, 1235.0), (35200.0, 15.0, 2207.0), (60000.0, 18.5, 4322.0), (f64::INFINITY, 22.5, 8910.0)], minimo: 5790.0 }),
        ],
    ),
    (
        "aragon",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (50000.0, 18.5, 4362.75), (60000.0, 20.5, 7100.75), (80000.0, 23.0, 9150.75), (90000.0, 24.0, 13750.75), (130000.0, 25.0, 16150.75), (f64::INFINITY, 25.5, 26150.75)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(13072.5, 9.5, 0.0), (21210.0, 12.0, 1241.89), (36960.0, 15.0, 2218.39), (52500.0, 18.5, 4580.89), (60000.0, 20.5, 7455.79), (80000.0, 23.0, 8993.29), (90000.0, 24.0, 13593.29), (130000.0, 25.0, 15993.29), (f64::INFINITY, 25.5, 25993.29)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(13072.5, 9.5, 0.0), (21210.0, 12.0, 1241.89), (36960.0, 15.0, 2218.39), (52500.0, 18.5, 4580.89), (60000.0, 20.5, 7455.79), (80000.0, 23.0, 8993.29), (90000.0, 24.0, 13593.29), (130000.0, 25.0, 15993.29), (f64::INFINITY, 25.5, 25993.29)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(13072.5, 9.5, 0.0), (21210.0, 12.0, 1241.89), (36960.0, 15.0, 2218.39), (52500.0, 18.5, 4580.89), (60000.0, 20.5, 7455.79), (80000.0, 23.0, 8993.29), (90000.0, 24.0, 13593.29), (130000.0, 25.0, 15993.29), (f64::INFINITY, 25.5, 25993.29)], minimo: 5550.0 }),
        ],
    ),
    (
        "asturias",
        &[
            (2022, Autonomica { escala: &[(12450.0, 10.0, 0.0), (17707.2, 12.0, 1245.0), (33007.2, 14.0, 1875.86), (53407.2, 18.5, 4017.86), (70000.0, 21.5, 7791.86), (90000.0, 22.5, 11359.32), (175000.0, 25.0, 15859.32), (f64::INFINITY, 25.5, 37109.32)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 10.0, 0.0), (17707.2, 12.0, 1245.0), (33007.2, 14.0, 1875.86), (53407.2, 18.5, 4017.86), (70000.0, 21.5, 7791.86), (90000.0, 22.5, 11359.32), (175000.0, 25.0, 15859.32), (f64::INFINITY, 25.5, 37109.32)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 10.0, 0.0), (17707.2, 12.0, 1245.0), (33007.2, 14.0, 1875.86), (53407.2, 18.5, 4017.86), (70000.0, 21.5, 7791.86), (90000.0, 22.5, 11359.32), (175000.0, 25.0, 15859.32), (f64::INFINITY, 25.5, 37109.32)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 9.0, 0.0), (17707.2, 12.0, 1120.5), (33007.2, 14.0, 1751.36), (53407.2, 19.2, 3893.36), (70000.0, 21.5, 7810.16), (90000.0, 22.5, 11377.62), (175000.0, 25.0, 15877.62), (f64::INFINITY, 26.0, 37127.62)], minimo: 6105.0 }),
        ],
    ),
    (
        "baleares",
        &[
            (2022, Autonomica { escala: &[(10000.0, 9.5, 0.0), (18000.0, 11.75, 950.0), (30000.0, 14.75, 1890.0), (48000.0, 17.75, 3660.0), (70000.0, 19.25, 6855.0), (90000.0, 22.0, 11090.0), (120000.0, 23.0, 15490.0), (175000.0, 24.0, 22390.0), (f64::INFINITY, 25.0, 35590.0)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(10000.0, 9.5, 0.0), (18000.0, 11.75, 950.0), (30000.0, 14.75, 1890.0), (48000.0, 17.75, 3660.0), (70000.0, 19.25, 6855.0), (90000.0, 22.0, 11090.0), (120000.0, 23.0, 15490.0), (175000.0, 24.0, 22390.0), (f64::INFINITY, 25.0, 35590.0)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(10000.0, 9.0, 0.0), (18000.0, 11.25, 900.0), (30000.0, 14.25, 1800.0), (48000.0, 17.5, 3510.0), (70000.0, 19.0, 6660.0), (90000.0, 21.75, 10840.0), (120000.0, 22.75, 15190.0), (175000.0, 23.75, 22015.0), (f64::INFINITY, 24.75, 35077.5)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(10000.0, 9.0, 0.0), (18000.0, 11.25, 900.0), (30000.0, 14.25, 1800.0), (48000.0, 17.5, 3510.0), (70000.0, 19.0, 6660.0), (90000.0, 21.75, 10840.0), (120000.0, 22.75, 15190.0), (175000.0, 23.75, 22015.0), (f64::INFINITY, 24.75, 35077.5)], minimo: 5550.0 }),
        ],
    ),
    (
        "canarias",
        &[
            (2022, Autonomica { escala: &[(13010.0, 9.0, 0.0), (18468.0, 11.5, 1170.9), (34327.0, 14.0, 1798.57), (55276.0, 18.5, 4018.83), (90000.0, 23.5, 7894.39), (120000.0, 25.0, 16054.53), (f64::INFINITY, 26.0, 23554.53)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(13010.0, 9.0, 0.0), (18468.0, 11.5, 1170.9), (34327.0, 14.0, 1798.57), (55276.0, 18.5, 4018.83), (90000.0, 23.5, 7894.39), (120000.0, 25.0, 16054.53), (f64::INFINITY, 26.0, 23554.53)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(13465.0, 9.0, 0.0), (19022.0, 11.5, 1211.85), (35185.0, 14.0, 1850.91), (56382.0, 18.5, 4113.73), (91350.0, 23.5, 8035.17), (121200.0, 25.0, 16252.65), (f64::INFINITY, 26.0, 23715.15)], minimo: 5606.0 }),
            (2025, Autonomica { escala: &[(13748.0, 9.0, 0.0), (19422.0, 11.5, 1237.32), (35924.0, 14.0, 1889.83), (57566.0, 18.5, 4200.11), (93268.0, 23.5, 8203.88), (123745.0, 25.0, 16593.85), (f64::INFINITY, 26.0, 24213.1)], minimo: 5606.0 }),
        ],
    ),
    (
        "cantabria",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (34000.0, 15.0, 2112.75), (46000.0, 18.5, 4182.75), (60000.0, 19.5, 6402.75), (90000.0, 24.5, 9132.75), (f64::INFINITY, 25.5, 16482.75)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (34000.0, 15.0, 2112.75), (46000.0, 18.5, 4182.75), (60000.0, 19.5, 6402.75), (90000.0, 24.5, 9132.75), (f64::INFINITY, 25.5, 16482.75)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(13000.0, 8.5, 0.0), (21000.0, 11.0, 1105.0), (35200.0, 14.5, 1985.0), (60000.0, 18.0, 4044.0), (90000.0, 22.5, 8508.0), (f64::INFINITY, 24.5, 15258.0)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(13000.0, 8.5, 0.0), (21000.0, 11.0, 1105.0), (35200.0, 14.5, 1985.0), (60000.0, 18.0, 4044.0), (90000.0, 22.5, 8508.0), (f64::INFINITY, 24.5, 15258.0)], minimo: 5550.0 }),
        ],
    ),
    (
        "castilla-mancha",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (f64::INFINITY, 22.5, 8950.75)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (f64::INFINITY, 22.5, 8950.75)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (f64::INFINITY, 22.5, 8950.75)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.0, 1182.75), (35200.0, 15.0, 2112.75), (60000.0, 18.5, 4362.75), (f64::INFINITY, 22.5, 8950.75)], minimo: 5550.0 }),
        ],
    ),
    (
        "castilla-leon",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 12.0, 1120.5), (35200.0, 14.0, 2050.5), (53407.2, 18.5, 4150.5), (f64::INFINITY, 21.5, 7518.83)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 12.0, 1120.5), (35200.0, 14.0, 2050.5), (53407.2, 18.5, 4150.5), (f64::INFINITY, 21.5, 7518.83)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 12.0, 1120.5), (35200.0, 14.0, 2050.5), (53407.2, 18.5, 4150.5), (f64::INFINITY, 21.5, 7518.83)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 12.0, 1120.5), (35200.0, 14.0, 2050.5), (53407.2, 18.5, 4150.5), (f64::INFINITY, 21.5, 7518.83)], minimo: 5550.0 }),
        ],
    ),
    (
        "cataluna",
        &[
            (2022, Autonomica { escala: &[(12450.0, 10.5, 0.0), (17707.2, 12.0, 1307.25), (21000.0, 14.0, 1938.11), (33007.2, 15.0, 2399.1), (53407.2, 18.8, 4200.18), (90000.0, 21.5, 8035.38), (120000.0, 23.5, 15902.83), (175000.0, 24.5, 22952.83), (f64::INFINITY, 25.5, 36427.83)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 10.5, 0.0), (17707.2, 12.0, 1307.25), (21000.0, 14.0, 1938.11), (33007.2, 15.0, 2399.1), (53407.2, 18.8, 4200.18), (90000.0, 21.5, 8035.38), (120000.0, 23.5, 15902.83), (175000.0, 24.5, 22952.83), (f64::INFINITY, 25.5, 36427.83)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 10.5, 0.0), (17707.2, 12.0, 1307.25), (21000.0, 14.0, 1938.11), (33007.2, 15.0, 2399.1), (53407.2, 18.8, 4200.18), (90000.0, 21.5, 8035.38), (120000.0, 23.5, 15902.83), (175000.0, 24.5, 22952.83), (f64::INFINITY, 25.5, 36427.83)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12500.0, 9.5, 0.0), (22000.0, 12.5, 1187.5), (33000.0, 16.0, 2375.0), (53000.0, 19.0, 4135.0), (90000.0, 21.5, 7935.0), (120000.0, 23.5, 15890.0), (175000.0, 24.5, 22940.0), (f64::INFINITY, 25.5, 36415.0)], minimo: 5550.0 }),
        ],
    ),
    (
        "extremadura",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 12.5, 1182.75), (24200.0, 15.5, 2151.5), (35200.0, 16.5, 2771.5), (60000.0, 20.5, 4586.5), (80200.0, 23.5, 9670.5), (99200.0, 24.0, 14417.5), (120200.0, 24.5, 18977.5), (f64::INFINITY, 25.0, 24122.5)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 8.0, 0.0), (20200.0, 10.0, 996.0), (24200.0, 16.0, 1771.0), (35200.0, 17.5, 2411.0), (60000.0, 21.0, 4336.0), (80200.0, 23.5, 9544.0), (99200.0, 24.0, 14291.0), (120200.0, 24.5, 18851.0), (f64::INFINITY, 25.0, 23996.0)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 8.0, 0.0), (20200.0, 10.0, 996.0), (24200.0, 16.0, 1771.0), (35200.0, 17.5, 2411.0), (60000.0, 21.0, 4336.0), (80200.0, 23.5, 9544.0), (99200.0, 24.0, 14291.0), (120200.0, 24.5, 18851.0), (f64::INFINITY, 25.0, 23996.0)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 8.0, 0.0), (20200.0, 10.0, 996.0), (24200.0, 16.0, 1771.0), (35200.0, 17.5, 2411.0), (60000.0, 21.0, 4336.0), (80200.0, 23.5, 9544.0), (99200.0, 24.0, 14291.0), (120200.0, 24.5, 18851.0), (f64::INFINITY, 25.0, 23996.0)], minimo: 5550.0 }),
        ],
    ),
    (
        "galicia",
        &[
            (2022, Autonomica { escala: &[(12985.35, 9.0, 0.0), (21068.6, 11.65, 1168.68), (35200.0, 14.9, 2110.38), (60000.0, 18.4, 4215.96), (f64::INFINITY, 22.5, 8779.16)], minimo: 5789.0 }),
            (2023, Autonomica { escala: &[(12985.35, 9.0, 0.0), (21068.6, 11.65, 1168.68), (35200.0, 14.9, 2110.38), (60000.0, 18.4, 4215.96), (f64::INFINITY, 22.5, 8779.16)], minimo: 5789.0 }),
            (2024, Autonomica { escala: &[(12985.35, 9.0, 0.0), (21068.6, 11.65, 1168.68), (35200.0, 14.9, 2110.38), (60000.0, 18.4, 4215.96), (f64::INFINITY, 22.5, 8779.16)], minimo: 5789.0 }),
            (2025, Autonomica { escala: &[(12985.35, 9.0, 0.0), (21068.6, 11.65, 1168.68), (35200.0, 14.9, 2110.38), (60000.0, 18.4, 4215.96), (f64::INFINITY, 22.5, 8779.16)], minimo: 5789.0 }),
        ],
    ),
    (
        "madrid",
        &[
            (2022, Autonomica { escala: &[(12960.45, 8.5, 0.0), (18433.2, 10.7, 1101.64), (34360.5, 12.8, 1687.22), (55596.9, 17.4, 3725.91), (f64::INFINITY, 20.5, 7421.04)], minimo: 5777.55 }),
            (2023, Autonomica { escala: &[(13362.22, 8.5, 0.0), (19004.63, 10.7, 1135.79), (35425.68, 12.8, 1739.53), (57320.4, 17.4, 3841.42), (f64::INFINITY, 20.5, 7651.1)], minimo: 5956.65 }),
            (2024, Autonomica { escala: &[(13362.22, 8.5, 0.0), (19004.63, 10.7, 1135.79), (35425.68, 12.8, 1739.53), (57320.4, 17.4, 3841.42), (f64::INFINITY, 20.5, 7651.1)], minimo: 5956.65 }),
            (2025, Autonomica { escala: &[(13362.22, 8.5, 0.0), (19004.63, 10.7, 1135.79), (35425.68, 12.8, 1739.53), (57320.4, 17.4, 3841.42), (f64::INFINITY, 20.5, 7651.1)], minimo: 5956.65 }),
        ],
    ),
    (
        "murcia",
        &[
            (2022, Autonomica { escala: &[(12960.45, 9.6, 0.0), (21028.2, 11.46, 1244.2), (35394.0, 13.74, 2168.76), (60000.0, 18.22, 4142.62), (f64::INFINITY, 22.7, 8716.67)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 11.2, 1182.75), (34000.0, 13.3, 2050.75), (60000.0, 17.9, 3886.15), (f64::INFINITY, 22.5, 8540.15)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 11.2, 1182.75), (34000.0, 13.3, 2050.75), (60000.0, 17.9, 3886.15), (f64::INFINITY, 22.5, 8540.15)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 9.5, 0.0), (20200.0, 11.2, 1182.75), (34000.0, 13.3, 2050.75), (60000.0, 17.9, 3886.15), (f64::INFINITY, 22.5, 8540.15)], minimo: 5550.0 }),
        ],
    ),
    (
        "rioja",
        &[
            (2022, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 11.6, 1120.5), (35200.0, 14.6, 2019.5), (50000.0, 18.8, 4209.5), (60000.0, 19.5, 6991.9), (120000.0, 25.0, 8941.9), (f64::INFINITY, 27.0, 23941.9)], minimo: 5550.0 }),
            (2023, Autonomica { escala: &[(12450.0, 9.0, 0.0), (20200.0, 11.6, 1120.5), (35200.0, 14.6, 2019.5), (50000.0, 18.8, 4209.5), (60000.0, 19.5, 6991.9), (120000.0, 25.0, 8941.9), (f64::INFINITY, 27.0, 23941.9)], minimo: 5550.0 }),
            (2024, Autonomica { escala: &[(12450.0, 8.0, 0.0), (20200.0, 10.6, 996.0), (35200.0, 13.6, 1817.5), (40000.0, 17.8, 3857.5), (50000.0, 18.3, 4711.9), (60000.0, 19.0, 6541.9), (120000.0, 24.5, 8441.9), (f64::INFINITY, 27.0, 23141.9)], minimo: 5550.0 }),
            (2025, Autonomica { escala: &[(12450.0, 8.0, 0.0), (20200.0, 10.6, 996.0), (35200.0, 13.6, 1817.5), (40000.0, 17.8, 3857.5), (50000.0, 18.3, 4711.9), (60000.0, 19.0, 6541.9), (120000.0, 24.5, 8441.9), (f64::INFINITY, 27.0, 23141.9)], minimo: 5550.0 }),
        ],
    ),
    (
        "valenciana",
        &[
            (2022, Autonomica { escala: &[(12000.0, 9.0, 0.0), (22000.0, 12.0, 1080.0), (32000.0, 15.0, 2280.0), (42000.0, 17.5, 3780.0), (52000.0, 20.0, 5530.0), (65000.0, 24.17, 7530.0), (80000.0, 24.5, 10672.1), (120000.0, 25.0, 14347.1), (140000.0, 25.5, 24347.1), (175000.0, 27.5, 29447.1), (f64::INFINITY, 29.5, 39072.1)], minimo: 6105.0 }),
            (2023, Autonomica { escala: &[(12000.0, 9.0, 0.0), (22000.0, 12.0, 1080.0), (32000.0, 15.0, 2280.0), (42000.0, 17.5, 3780.0), (52000.0, 20.0, 5530.0), (62000.0, 22.5, 7530.0), (72000.0, 25.0, 9780.0), (100000.0, 26.5, 12280.0), (150000.0, 27.5, 19700.0), (200000.0, 28.5, 33450.0), (f64::INFINITY, 29.5, 47700.0)], minimo: 6105.0 }),
            (2024, Autonomica { escala: &[(12000.0, 9.0, 0.0), (22000.0, 12.0, 1080.0), (32000.0, 15.0, 2280.0), (42000.0, 17.5, 3780.0), (52000.0, 20.0, 5530.0), (62000.0, 22.5, 7530.0), (72000.0, 25.0, 9780.0), (100000.0, 26.5, 12280.0), (150000.0, 27.5, 19700.0), (200000.0, 28.5, 33450.0), (f64::INFINITY, 29.5, 47700.0)], minimo: 6105.0 }),
            (2025, Autonomica { escala: &[(12000.0, 9.0, 0.0), (22000.0, 12.0, 1080.0), (32000.0, 15.0, 2280.0), (42000.0, 17.5, 3780.0), (52000.0, 20.0, 5530.0), (62000.0, 22.5, 7530.0), (72000.0, 25.0, 9780.0), (100000.0, 26.5, 12280.0), (150000.0, 27.5, 19700.0), (200000.0, 28.5, 33450.0), (f64::INFINITY, 29.5, 47700.0)], minimo: 6105.0 }),
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dificil_justificacion() {
        assert_eq!(gdj_reglas(2023), (7.0, 2000.0));
        assert_eq!(gdj_reglas(2024), (5.0, 2000.0));
        assert_eq!(gdj_reglas(2015), (5.0, 2000.0)); // antes de 2022: la de 2022
        assert_eq!(gdj_reglas(2030), (5.0, 2000.0)); // después de 2025: la de 2025
    }

    #[test]
    fn que_reglas_se_usan() {
        let r = reglas(2025, "madrid");
        assert_eq!(r.anio, 2025);
        assert!(r.autonomica.is_some() && !r.estatal.deduccion.is_empty());
        // Sin las de ese año: las del último que hay antes; antes del primero, las del primero.
        assert_eq!(reglas(2026, "madrid").anio, 2025);
        assert_eq!(reglas(2021, "madrid").anio, 2022);
        assert!(reglas(2022, "madrid").estatal.deduccion.is_empty(), "la deducción es de 2025");
        // Sin comunidad, o foral: sin escala autonómica.
        assert!(reglas(2025, "").autonomica.is_none() && reglas(2025, "navarra").autonomica.is_none());
        // Las quince de territorio común, los cuatro años.
        for (c, _) in &COMUNIDADES[..15] {
            for y in 2022..=2025 {
                assert!(reglas(y, c).autonomica.is_some(), "{c} {y}");
            }
        }
        assert_eq!(nombre_comunidad("castilla-leon"), "Castilla y León");
        assert_eq!(nombre_comunidad("x"), "x");
    }

    #[test]
    fn escalas() {
        let e = reglas(2025, "").estatal.escala;
        assert_eq!(por_tramos(e, 0.0), 0.0);
        assert_eq!(por_tramos(e, 5550.0), 527.25);
        assert_eq!(por_tramos(e, 26095.0), 2997.0);
        assert_eq!(por_tramos(e, 1_000_000.0), r2(62950.75 + 700_000.0 * 0.245));
        assert_eq!(por_tramos(e, -5.0), 0.0);
    }

    // Las cifras, comprobadas con la web de la que salió la app.
    #[test]
    fn cuota_de_un_sueldo() {
        // 30.000 € en Madrid, 2025: sin reducción (pasa de 19.747,50 €) ni deducción.
        let c = cuota_sueldo(&reglas(2025, "madrid"), 30000.0, 1905.0);
        assert_eq!((c.base, c.estatal, c.autonomica, c.deduccion, c.total), (26095.0, 2469.75, 2140.78, 0.0, 4610.53));
        // 17.000 € en Castilla y León, 2025: con reducción y con deducción.
        let c = cuota_sueldo(&reglas(2025, "castilla-leon"), 17000.0, 1079.5);
        assert_eq!((c.base, c.estatal, c.autonomica, c.deduccion, c.total), (8488.37, 279.15, 264.45, 255.2, 288.4));
        // 16.000 €: la deducción se come toda la cuota.
        let c = cuota_sueldo(&reglas(2025, "castilla-leon"), 16000.0, 1016.0);
        assert_eq!((c.base, c.deduccion, c.total), (5913.0, 67.16, 0.0));
        // 45.000 € en Cataluña.
        assert_eq!(cuota_sueldo(&reglas(2025, "cataluna"), 45000.0, 2857.5).total, 9714.69);
        // Por debajo del mínimo, nada.
        assert_eq!(cuota_sueldo(&reglas(2025, "madrid"), 12000.0, 762.0).total, 0.0);
        // 60.000 € sin comunidad: la estatal dos veces.
        let c = cuota_sueldo(&reglas(2024, ""), 60000.0, 3810.0);
        assert_eq!((c.base, c.estatal, c.autonomica, c.total), (54190.0, 7348.65, 7348.65, 14697.3));
    }
}
