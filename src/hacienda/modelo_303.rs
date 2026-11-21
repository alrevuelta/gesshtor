//! Modelo 303 · el IVA del trimestre. Todo lo que la app sabe de él:
//!
//! 1. **Qué casillas tiene** (`DISENOS`): las de los diseños de registro oficiales de la AEAT, uno por cada vez
//!    que ha cambiado el modelo desde 2019 (`DR303e20` … `DR303e26v101.xlsx`, en sede.agenciatributaria.gob.es →
//!    Ayuda → Diseños de registro → Modelos 300 al 399). Si Hacienda saca uno nuevo, se añade al final.
//! 2. **Cómo se lee** de su PDF: como cualquier formulario (`formulario::leer`), y lo leído tiene que cuadrar con
//!    sus sumas (`reglas`: las fórmulas impresas en el propio modelo).
//! 3. **Cuándo se presenta**: cada trimestre, del 1 al 20 del mes siguiente; el del 4T, hasta el 30 de enero
//!    (`hacienda::plazos_trimestres`).
//! 4. **Cómo se calcula** con tus facturas y gastos (`apuntes`: qué pone cada documento y en qué casilla;
//!    `calcular`: el modelo entero), según las instrucciones oficiales del 303 de 2025 y 2026
//!    (sede.agenciatributaria.gob.es → Todas las gestiones → IVA → Modelo 303 → Instrucciones). Los trimestres se
//!    encadenan: lo pendiente de compensar (casilla 110) sale de las 87 y 71 del trimestre anterior.
use super::formulario::{self, Diseno, Formulario, QueCasillas};
use super::modelo::{Memo, Modelo, Periodicidad, Trimestral};
use crate::dominio::factura::{emitidas, totales_factura};
use crate::dominio::fmt::{en_rango, periodo, r2, DIF};
use crate::dominio::gasto::{gasto_calc, GastoCalc};
use crate::dominio::{Casillas, Datos, Factura, Gasto, Valor};

/// El modelo 303, para leerlo (`formulario::leer::<M303>`).
pub struct M303;

impl Formulario for M303 {
    const MODELO: Modelo = Modelo::M303;
    const PERIODICIDAD: Periodicidad = Periodicidad::Trimestral;
    const CASILLAS: QueCasillas = QueCasillas::Disenos(DISENOS);
    /// El resultado.
    const ANCLAS: &'static [u32] = &[RESULTADO];

    fn reglas(c: &Casillas) -> Vec<(u32, f64)> {
        let v = |n| c.v(n);
        vec![
            (27, formulario::suma(c, &CUOTAS)),
            (45, formulario::suma(c, &DEDUCIBLE)),
            (46, v(27) - v(45)),
            (64, v(46) + v(58) + v(76)),
            (66, v(64) * v(65) / 100.0),
            (69, v(66) + v(77) - v(78) - v(67) + v(68) + v(108)),
            (71, v(69) - v(70) + v(109) - v(112)),
            (87, v(110) - v(78)),
        ]
    }
}

/// La casilla del resultado: a ingresar si es positiva; si es negativa, a compensar o a devolver.
pub const RESULTADO: u32 = 71;

/// Lo que sale por el periodo entero: en una complementaria (con algo en la 70, lo ya ingresado, o en la 109, lo ya
/// devuelto), la 69, que suma lo de antes y lo de ahora; si no, la del resultado (71).
pub fn resultado_del_periodo(c: &Casillas) -> f64 {
    if (c.v(70).abs() > DIF || c.v(109).abs() > DIF) && c.contains_key(&69) { r2(c.v(69)) } else { r2(c.v(RESULTADO)) }
}

/* ================= 1. las casillas de cada diseño de registro ================= */

/// Los diseños del 303 que se usan desde 2020, del más viejo al más nuevo.
const DISENOS: &[Diseno] = &[
    Diseno {
        id: "e20",
        desde: 201901,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 74,
            75, 76, 77, 79, 80, 81, 83, 84, 85, 86, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
            98, 99, 107,
        ],
    },
    Diseno {
        id: "e21a",
        desde: 202101,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 61, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75,
            76, 77, 78, 87, 110,
        ],
    },
    Diseno {
        id: "e21b",
        desde: 202107,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 61, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75,
            76, 77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
            98, 99, 107, 110, 120, 122, 123, 124, 125, 126, 127, 128,
        ],
    },
    Diseno {
        id: "e22",
        desde: 202201,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 110, 120, 122, 123, 124, 125, 126, 127, 128,
        ],
    },
    Diseno {
        id: "e23",
        desde: 202301,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 109, 110, 120, 122, 123, 124, 125, 126, 127, 128, 150, 151, 152, 153, 154, 155, 156, 157,
            158,
        ],
    },
    Diseno {
        id: "e24a",
        desde: 202401,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 109, 110, 120, 122, 123, 124, 125, 126, 127, 128, 150, 151, 152, 153, 154, 155, 156, 157,
            158,
        ],
    },
    Diseno {
        id: "e24b",
        desde: 202409,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 108, 109, 110, 111, 120, 122, 123, 124, 125, 126, 127, 128, 150, 151, 152, 153, 154, 155,
            156, 157, 158, 165, 166, 167, 168, 169, 170,
        ],
    },
    Diseno {
        id: "e25",
        desde: 202501,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 108, 109, 110, 111, 120, 122, 123, 124, 125, 126, 127, 128, 150, 151, 152, 153, 154, 155,
            156, 157, 158, 165, 166, 167, 168, 169, 170,
        ],
    },
    Diseno {
        id: "e26",
        desde: 202601,
        casillas: &[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 59, 60, 62, 63, 64, 65, 66, 68, 69, 70, 71, 74, 75, 76,
            77, 78, 79, 80, 81, 83, 84, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,
            99, 107, 108, 109, 110, 111, 112, 120, 122, 123, 124, 125, 126, 127, 128, 150, 151, 152, 153, 154,
            155, 156, 157, 158, 165, 166, 167, 168, 169, 170,
        ],
    },
];

/* ================= 2. las sumas del modelo ================= */

/// Las casillas que llevan un tipo o un porcentaje, no un importe (los PDF traen los tipos del recargo aunque no se
/// usen).
pub const TIPOS: [u32; 12] = [2, 5, 8, 17, 20, 23, 65, 151, 154, 157, 166, 169];
/// Las cuotas del IVA devengado: su suma es la 27.
const CUOTAS: [u32; 15] = [152, 167, 3, 155, 6, 9, 11, 13, 15, 158, 170, 18, 21, 24, 26];
/// El IVA deducible: su suma es la 45.
const DEDUCIBLE: [u32; 10] = [29, 31, 33, 35, 37, 39, 41, 42, 43, 44];

/* ================= 4. calcularlo con tus facturas y gastos ================= */

/// Qué se hace con un 303 negativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Negativo {
    Devolver,
    Compensar,
}

/// Si un 303 negativo se pidió devolver o se dejó a compensar: según lo leído de su PDF o, si no, lo normal
/// (devolver en el 4T, compensar en los demás).
pub fn negativo(d: &Datos, anio: i32, q: u32) -> Negativo {
    match Modelo::M303.leido(d, anio, q).and_then(|l| l.tipo) {
        Some(t) if t.devuelve() => Negativo::Devolver,
        Some(_) => Negativo::Compensar,
        None if q == 4 => Negativo::Devolver,
        None => Negativo::Compensar,
    }
}

/// Las casillas de IVA que salen siempre, aunque sea a 0 (y los tipos del régimen general).
fn vacio() -> Casillas {
    let mut c: Casillas = [1, 3, 4, 6, 7, 9, 10, 11, 12, 13, 14, 15, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 59, 60, 120, 122]
        .into_iter()
        .map(|k| (k, 0.0))
        .collect();
    c.insert(2, 4.0);
    c.insert(5, 10.0);
    c.insert(8, 21.0);
    c
}

/// Uno de tus documentos: una factura emitida o un gasto.
#[derive(Debug, Clone, Copy)]
pub enum Doc<'a> {
    Factura(&'a Factura),
    Gasto(&'a Gasto),
}

impl Doc<'_> {
    /// La fecha que cuenta para los impuestos (en una factura, la de la operación).
    pub fn fecha(&self) -> &str {
        match self {
            Doc::Factura(f) => f.devengo(),
            Doc::Gasto(g) => &g.fecha,
        }
    }
}

/// Los tipos del IVA con los que la app calcula el 303, cada uno con sus casillas. Según las instrucciones del 303 de
/// 2026 (sede.agenciatributaria.gob.es → IVA → Modelo 303 → Instrucciones 2026): «En las casillas 150, 151 y 152 se
/// harán constar las bases imponibles gravadas al tipo del 0% y las cuotas resultantes» (el 0 % de hoy es el de los
/// bienes donados a entidades sin fines lucrativos: art. 91.Cuatro de la Ley 37/1992 del IVA), y lo mismo las 01-03
/// «al tipo del 4%», las 04-06 «del 10%» y las 07-09 «del 21%». No hay otro tipo hoy (Ley del IVA, arts. 90 y 91,
/// texto consolidado en boe.es, BOE-A-1992-28740): el 5 % acabó el 30-9-2024 y el 2 % y el 7,5 %, el 31-12-2024
/// (Real Decreto-ley 4/2024, art. 1, BOE-A-2024-12944). Una línea con otro tipo no va al 303 (`fuera`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoIva {
    Cero,
    Superreducido,
    Reducido,
    General,
}

impl TipoIva {
    pub fn de(t: f64) -> Option<TipoIva> {
        [(0.0, TipoIva::Cero), (4.0, TipoIva::Superreducido), (10.0, TipoIva::Reducido), (21.0, TipoIva::General)].into_iter().find(|(x, _)| (x - t).abs() < 0.005).map(|(_, t)| t)
    }

    /// Las casillas del 303 de su base y su cuota.
    pub fn casillas(self) -> (u32, u32) {
        match self {
            TipoIva::Cero => (150, 152),
            TipoIva::Superreducido => (1, 3),
            TipoIva::Reducido => (4, 6),
            TipoIva::General => (7, 9),
        }
    }
}

/// Las líneas de tus facturas nacionales de [desde, hasta] con un tipo de IVA que no es de hoy (`TipoIva`): no van
/// al 303, y la pantalla lo dice. (la factura, el tipo)
pub fn fuera<'a>(d: &'a Datos, desde: &str, hasta: &str) -> Vec<(&'a Factura, f64)> {
    // (la rectificativa de una de otro periodo va entera a «modificación de bases y cuotas», con su tipo que sea)
    let nacionales = emitidas(d).filter(|f| f.operacion == "nacional" && en_rango(f.devengo(), desde, hasta) && f.rectifica_fecha().is_none_or(|r| en_rango(r, desde, hasta)));
    nacionales.flat_map(|f| totales_factura(f).desglose.into_iter().filter(|g| TipoIva::de(g.tipo).is_none()).map(move |g| (f, g.tipo))).collect()
}

/// Lo que pone un documento en el 303: su base en la casilla `kb` y su cuota en la `kc` (0: sin cuota), al tipo
/// `tipo`. Un gasto de la UE o de fuera da dos: el IVA que autoliquidas (devengado) y el que te deduces.
#[derive(Debug, Clone)]
pub struct Apunte<'a> {
    pub doc: Doc<'a>,
    pub kb: u32,
    pub kc: u32,
    pub base: f64,
    pub cuota: f64,
    pub tipo: f64,
}

/// Lo que ponen tus facturas y gastos de [desde, hasta] en el 303, documento a documento. Según las instrucciones
/// del 303 (sede.agenciatributaria.gob.es → IVA → Modelo 303 → Instrucciones 2026):
/// - Facturas con IVA: 150-152 (0 %), 01-03 (4 %), 04-06 (10 %), 07-09 (21 %); otro tipo, en ninguna (`fuera`);
///   la rectificativa de una factura de otro periodo, en «modificación de bases y cuotas» (14-15, art. 80 LIVA).
/// - A empresas de la UE (servicios o entregas intracomunitarias): 59. A clientes de fuera de la UE: 120,
///   «operaciones no sujetas por reglas de localización» (art. 69 LIVA; la 60 es para exportaciones de bienes).
/// - Gastos de la UE: el IVA que autoliquidas, en 10-11 (adquisiciones intracomunitarias); de fuera de la UE, en
///   12-13 («otras operaciones con inversión del sujeto pasivo», art. 84.Uno.2º LIVA).
/// - El IVA que te deduces: 28-31 los interiores (y los de fuera de la UE), 36-39 los de la UE; corrientes o
///   bienes de inversión (más de 3.005,06 €, art. 108 LIVA). La base va entera («sin prorratear») cuando su cuota
///   es deducible total o parcialmente; si no te deduces nada, no va.
pub fn apuntes<'a>(d: &'a Datos, desde: &str, hasta: &str) -> Vec<Apunte<'a>> {
    let mut v = Vec::new();
    for f in emitidas(d).filter(|f| en_rango(f.devengo(), desde, hasta)) {
        let doc = Doc::Factura(f);
        let t = totales_factura(f);
        let mut apunte = |kb, kc, base, cuota, tipo| v.push(Apunte { doc, kb, kc, base, cuota, tipo });
        match f.operacion.as_str() {
            "nacional" if f.rectifica_fecha().is_some_and(|r| !en_rango(r, desde, hasta)) => apunte(14, 15, t.base, t.cuota, 0.0),
            "nacional" => {
                for g in &t.desglose {
                    if let Some((kb, kc)) = TipoIva::de(g.tipo).map(TipoIva::casillas) {
                        apunte(kb, kc, g.base, g.cuota, g.tipo);
                    }
                }
            }
            "ue" => apunte(59, 0, t.base, 0.0, 0.0),
            "extra" => apunte(120, 0, t.base, 0.0, 0.0),
            _ => {}
        }
    }
    for g in d.gastos.iter().filter(|g| en_rango(&g.fecha, desde, hasta)) {
        let doc = Doc::Gasto(g);
        let gc = gasto_calc(g, d);
        let devengado = match gc.origen.as_str() {
            "es" if gc.cuota != 0.0 => None,
            "ue" => Some((10, 11)),
            "extra" => Some((12, 13)),
            _ => continue,
        };
        if let Some((kb, kc)) = devengado {
            v.push(Apunte { doc, kb, kc, base: gc.base, cuota: gc.cuota, tipo: gc.tipo });
        }
        if gc.iva_ded != 0.0 {
            let (kb, kc) = casillas_deducible(&gc);
            v.push(Apunte { doc, kb, kc, base: gc.base, cuota: gc.iva_ded, tipo: gc.tipo });
        }
    }
    v
}

/// Dónde va el IVA que te deduces de un gasto (base y cuota): interiores (y de fuera de la UE) o de la UE,
/// corrientes o bienes de inversión.
pub fn casillas_deducible(gc: &GastoCalc) -> (u32, u32) {
    match (gc.origen == "ue", gc.inversion_iva) {
        (false, false) => (28, 29),
        (false, true) => (30, 31),
        (true, false) => (36, 37),
        (true, true) => (38, 39),
    }
}

/// Las casillas de IVA de [desde, hasta]: lo devengado de tus facturas y lo deducible de tus gastos.
pub fn iva_periodo(d: &Datos, desde: &str, hasta: &str) -> Casillas {
    let mut c = vacio();
    fn add(c: &mut Casillas, k: u32, v: f64) {
        let x = c.entry(k).or_insert(0.0);
        *x = r2(*x + v);
    }
    for a in apuntes(d, desde, hasta) {
        add(&mut c, a.kb, a.base);
        if a.kc > 0 {
            add(&mut c, a.kc, a.cuota);
        }
    }
    // Si una rectificativa deja negativa la base de un tipo, va a «modificación de bases y cuotas» (14-15):
    // las casillas de cada tipo no admiten negativos.
    for (b, q) in [(150, 152), (1, 3), (4, 6), (7, 9)] {
        let (vb, vq) = (c.v(b), c.v(q));
        if vb < 0.0 || vq < 0.0 {
            add(&mut c, 14, vb);
            add(&mut c, 15, vq);
            c.insert(b, 0.0);
            c.insert(q, 0.0);
        }
    }
    let c27 = r2(c.v(152) + c.v(3) + c.v(6) + c.v(9) + c.v(11) + c.v(13) + c.v(15));
    c.insert(27, c27);
    let c45 = r2(c.v(29) + c.v(31) + c.v(33) + c.v(35) + c.v(37) + c.v(39) + c.v(41) + c.v(42) + c.v(43) + c.v(44));
    c.insert(45, c45);
    c.insert(46, r2(c27 - c45));
    c
}

/// El trimestre de antes: (2026, 1) → (2025, 4).
pub fn trimestre_anterior(anio: i32, q: u32) -> (i32, u32) {
    if q <= 1 { (anio - 1, 4) } else { (anio, q - 1) }
}

/// ¿Estabas de alta en la actividad ese trimestre? Según las fechas de tu perfil (`Datos::fechas_actividad`): no
/// antes del alta ni después de la baja (salvo que te hayas vuelto a dar de alta).
pub fn en_actividad(d: &Datos, anio: i32, q: u32) -> bool {
    let (ini, fin) = periodo(anio, q);
    let (alta, baja) = d.fechas_actividad();
    let antes_del_alta = !alta.is_empty() && alta > fin.as_str();
    let de_baja = !baja.is_empty() && baja < ini.as_str() && alta <= baja;
    !antes_del_alta && !de_baja
}

/// IVA pendiente de compensar al empezar el trimestre (casilla 110).
pub fn pendiente_compensar(d: &Datos, anio: i32, q: u32, memo: &mut Memo) -> f64 {
    let (py, pq) = trimestre_anterior(anio, q);
    if py < d.primer_anio_con_apuntes() && Modelo::M303.leido(d, py, pq).is_none() {
        return r2(d.fiscal.compensar_inicial.v());
    }
    let p = Trimestral::M303.cuenta(d, py, pq, memo);
    let mut arrastre = p.v(87);
    let c71 = p.v(71);
    if c71 < 0.0 && negativo(d, py, pq) != Negativo::Devolver {
        arrastre = r2(arrastre - c71);
    }
    r2(arrastre)
}

/// El 303 calculado con tus facturas y gastos.
pub fn calcular(d: &Datos, anio: i32, q: u32, memo: &mut Memo) -> Casillas {
    let (desde, hasta) = periodo(anio, q);
    let mut c = iva_periodo(d, &desde, &hasta);
    let c64 = c.v(46);
    let c65 = 100.0;
    let c66 = r2(c64 * c65 / 100.0);
    let c77 = 0.0;
    let c110 = pendiente_compensar(d, anio, q, memo);
    // 78: al compensar, como mucho 66 + 77 («la cuantía máxima posible»); si sale negativo y se pide devolver (en el
    // 4T), entero: «se consignará la totalidad del saldo a su favor, que deberá incluir el importe total de la
    // casilla 110» (instrucciones del 303, apartados 6 «Compensación» y 8 «Devolución»).
    let saldo = c66 + c77;
    let c78 = if saldo < c110 && negativo(d, anio, q) == Negativo::Devolver { c110 } else { r2(c110.min(saldo.max(0.0))) };
    let c87 = r2(c110 - c78);
    let c68 = 0.0;
    let c69 = r2(c66 + c77 - c78 + c68);
    let c70 = 0.0;
    let c71 = r2(c69 - c70);
    for (k, v) in [(64, c64), (65, c65), (66, c66), (77, c77), (110, c110), (78, c78), (87, c87), (68, c68), (69, c69), (70, c70), (71, c71)] {
        c.insert(k, v);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hacienda::formulario::descuadres;

    #[test]
    fn sumas_del_modelo() {
        let c = |v: &[(u32, f64)]| v.iter().copied().collect::<Casillas>();
        assert!(descuadres::<M303>(&c(&[(9, 21.0), (27, 21.0), (46, 21.0), (64, 21.0), (65, 100.0), (66, 21.0), (69, 21.0), (71, 21.0)])).is_empty());
        assert_eq!(descuadres::<M303>(&c(&[(9, 21.0), (27, 20.0)])), [27, 46]);
    }

    use crate::dominio::{Linea, Num};

    fn factura(fecha: &str, operacion: &str, precio: f64, iva: f64) -> Factura {
        let lineas = vec![Linea { concepto: "Servicio".into(), cantidad: Num::de(1.0), precio: Num::de(precio), iva: Num::de(iva) }];
        Factura { estado: "emitida".into(), numero: fecha.into(), fecha: fecha.into(), operacion: operacion.into(), lineas, ..Default::default() }
    }

    fn gasto(fecha: &str, origen: &str, base: f64, iva: f64) -> Gasto {
        Gasto { categoria: "software".into(), origen: origen.into(), base: Num::de(base), iva_tipo: Num::de(iva), fecha: fecha.into(), ..Default::default() }
    }

    /// Cada documento, en su casilla: los de la UE y de fuera, con su IVA autoliquidado; la base deducible, entera.
    #[test]
    fn cada_documento_en_su_casilla() {
        let facturas = vec![factura("2026-04-10", "nacional", 1000.0, 21.0), factura("2026-05-10", "ue", 500.0, 21.0), factura("2026-06-10", "extra", 300.0, 21.0)];
        let gastos = vec![
            gasto("2026-04-02", "es", 100.0, 21.0),
            Gasto { categoria: "vehiculo".into(), ..gasto("2026-04-03", "es", 200.0, 21.0) },
            Gasto { iva_no_ded: true, ..gasto("2026-04-04", "es", 10.0, 21.0) },
            gasto("2026-05-02", "ue", 50.0, 21.0),
            gasto("2026-05-03", "extra", 20.0, 21.0),
        ];
        let d = Datos { facturas, gastos, ..Default::default() };
        let c = iva_periodo(&d, "2026-04-01", "2026-06-30");
        assert_eq!((c.v(7), c.v(9), c.v(59), c.v(120)), (1000.0, 210.0, 500.0, 300.0));
        assert_eq!((c.v(10), c.v(11), c.v(12), c.v(13)), (50.0, 10.5, 20.0, 4.2));
        assert_eq!(c.v(27), 224.7);
        // El coche, al 50 %: su base entera y la mitad de la cuota; el tique sin tu NIF no entra; el de fuera de la
        // UE, como interior.
        assert_eq!((c.v(28), c.v(29)), (320.0, r2(21.0 + 21.0 + 4.2)));
        assert_eq!((c.v(36), c.v(37)), (50.0, 10.5));
        assert_eq!(c.v(46), r2(224.7 - 46.2 - 10.5));
    }

    /// Lo pendiente de compensar: se aplica como mucho lo que sale a ingresar; en el 4T, si se pide devolver, entero.
    /// Cada tipo de IVA, en sus casillas: el 0 % en la 150-152 (como dicen las instrucciones); un tipo que no existe
    /// hoy (el 5 %) no va a ninguna, ni a la del 21 %, y se dice.
    #[test]
    fn cada_tipo_en_sus_casillas() {
        let l = |p: f64, iva: f64| Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(p), iva: Num::de(iva) };
        let mut d = Datos::default();
        d.facturas.push(Factura { lineas: vec![l(1000.0, 21.0), l(500.0, 0.0), l(200.0, 10.0), l(100.0, 4.0)], ..factura("2026-02-01", "nacional", 0.0, 21.0) });
        let c = iva_periodo(&d, "2026-01-01", "2026-03-31");
        assert_eq!((c.v(7), c.v(9), c.v(150), c.v(152), c.v(4), c.v(6), c.v(1), c.v(3)), (1000.0, 210.0, 500.0, 0.0, 200.0, 20.0, 100.0, 4.0));
        assert!(fuera(&d, "2026-01-01", "2026-03-31").is_empty());
        d.facturas.push(Factura { lineas: vec![l(1000.0, 5.0)], ..factura("2026-03-01", "nacional", 0.0, 21.0) });
        let c = iva_periodo(&d, "2026-01-01", "2026-03-31");
        assert_eq!((c.v(7), c.v(9), c.v(27)), (1000.0, 210.0, 234.0), "el 5 % no va al 21 %");
        let f = fuera(&d, "2026-01-01", "2026-03-31");
        assert_eq!(f.iter().map(|(f, t)| (f.fecha.as_str(), *t)).collect::<Vec<_>>(), [("2026-03-01", 5.0)]);
        assert_eq!((TipoIva::de(21.0), TipoIva::de(0.0), TipoIva::de(5.0), TipoIva::de(7.5)), (Some(TipoIva::General), Some(TipoIva::Cero), None, None));
    }

    #[test]
    fn compensar_y_devolver() {
        let mut d = Datos::default();
        let mut memo = Memo::default();
        // 1T: 210 € de IVA de gastos → −210, a compensar.
        d.gastos = vec![gasto("2026-02-01", "es", 1000.0, 21.0)];
        assert_eq!(calcular(&d, 2026, 1, &mut memo).v(71), -210.0);
        // 2T: 21 € a ingresar: se aplican 21 de los 210.
        d.facturas = vec![factura("2026-05-01", "nacional", 100.0, 21.0)];
        let c = calcular(&d, 2026, 2, &mut Memo::default());
        assert_eq!((c.v(110), c.v(78), c.v(87), c.v(71)), (210.0, 21.0, 189.0, 0.0));
        // 3T: sale negativo: no se aplica nada y pasa entero.
        d.gastos.push(gasto("2026-08-01", "es", 100.0, 21.0));
        let c = calcular(&d, 2026, 3, &mut Memo::default());
        assert_eq!((c.v(110), c.v(78), c.v(87), c.v(71)), (189.0, 0.0, 189.0, -21.0));
        // 4T: −10,50 y se pide devolver: con los 210 pendientes.
        d.gastos.push(gasto("2026-11-01", "es", 50.0, 21.0));
        let c = calcular(&d, 2026, 4, &mut Memo::default());
        assert_eq!((c.v(110), c.v(78), c.v(87), c.v(71)), (210.0, 210.0, 0.0, -220.5));
        assert_eq!(negativo(&d, 2026, 4), Negativo::Devolver);
    }

    #[test]
    fn trimestres_encadenados() {
        assert_eq!(trimestre_anterior(2026, 1), (2025, 4));
        assert_eq!(trimestre_anterior(2026, 3), (2026, 2));
    }
}
