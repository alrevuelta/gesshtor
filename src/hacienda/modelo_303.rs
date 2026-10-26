//! Modelo 303 · el IVA del trimestre. Todo lo que la app sabe de él:
//!
//! 1. **Qué casillas tiene** (`DISENOS`): las de los diseños de registro oficiales de la AEAT, uno por cada vez
//!    que ha cambiado el modelo desde 2019 (`DR303e20` … `DR303e26v101.xlsx`, en sede.agenciatributaria.gob.es →
//!    Ayuda → Diseños de registro → Modelos 300 al 399). Si Hacienda saca uno nuevo, se añade al final.
//! 2. **Cómo se lee** de su PDF: como cualquier formulario (`formulario::leer`), y lo leído tiene que cuadrar con
//!    sus sumas (`reglas`: las fórmulas impresas en el propio modelo).
//! 3. **Cuándo se presenta**: cada trimestre, del 1 al 20 del mes siguiente; el del 4T, hasta el 30 de enero
//!    (`hacienda::plazos_trimestres`).
//! 4. **Cómo se calcula** con tus facturas y gastos (`calcular`). Los trimestres se encadenan: lo pendiente de
//!    compensar (casilla 110) sale de las 87 y 71 del trimestre anterior.
use super::formulario::{self, Diseno, Formulario, QueCasillas};
use super::modelo::{Memo, Modelo, Periodicidad, Trimestral};
use crate::dominio::factura::{emitidas, totales_factura};
use crate::dominio::fmt::{en_rango, periodo, r2, DIF};
use crate::dominio::gasto::gasto_calc;
use crate::dominio::{Casillas, Datos, Valor};

/// El modelo 303, para leerlo (`formulario::leer::<M303>`).
pub struct M303;

impl Formulario for M303 {
    const MODELO: Modelo = Modelo::M303;
    const PERIODICIDAD: Periodicidad = Periodicidad::Trimestral;
    const CASILLAS: QueCasillas = QueCasillas::Disenos(DISENOS);

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

/// Las casillas de IVA de [desde, hasta]: lo devengado de tus facturas y lo deducible de tus gastos.
pub fn iva_periodo(d: &Datos, desde: &str, hasta: &str) -> Casillas {
    let mut c = vacio();
    fn add(c: &mut Casillas, k: u32, v: f64) {
        let x = c.entry(k).or_insert(0.0);
        *x = r2(*x + v);
    }
    for f in emitidas(d) {
        if !en_rango(f.devengo(), desde, hasta) {
            continue;
        }
        let t = totales_factura(f);
        match f.operacion.as_str() {
            "nacional" => {
                // Rectificativa de una factura de otro periodo: «modificación de bases y cuotas» (14-15).
                if f.rectifica_fecha().is_some_and(|r| !en_rango(r, desde, hasta)) {
                    add(&mut c, 14, t.base);
                    add(&mut c, 15, t.cuota);
                    continue;
                }
                for g in &t.desglose {
                    let (kb, kc) = if g.tipo == 4.0 {
                        (1, 3)
                    } else if g.tipo == 10.0 {
                        (4, 6)
                    } else {
                        (7, 9)
                    };
                    add(&mut c, kb, g.base);
                    add(&mut c, kc, g.cuota);
                }
            }
            "ue" => add(&mut c, 59, t.base),
            "extra" => add(&mut c, 120, t.base),
            _ => {}
        }
    }
    for g in &d.gastos {
        if !en_rango(&g.fecha, desde, hasta) {
            continue;
        }
        let gc = gasto_calc(g, d);
        if gc.origen == "sin" || gc.cuota == 0.0 && gc.origen == "es" {
            continue;
        }
        let b_ded = r2(gc.base * gc.p_iva / 100.0);
        // Interiores (y de fuera de la UE): 28-29 corrientes, 30-31 bienes de inversión. UE: 36-37 / 38-39.
        let (kb, kc) = match (gc.origen.as_str(), gc.inversion_iva) {
            ("es" | "extra", false) => (28, 29),
            ("es" | "extra", true) => (30, 31),
            ("ue", false) => (36, 37),
            ("ue", true) => (38, 39),
            _ => continue,
        };
        match gc.origen.as_str() {
            "ue" => {
                add(&mut c, 10, gc.base);
                add(&mut c, 11, gc.cuota);
            }
            "extra" => {
                add(&mut c, 12, gc.base);
                add(&mut c, 13, gc.cuota);
            }
            _ => {}
        }
        add(&mut c, kb, b_ded);
        add(&mut c, kc, gc.iva_ded);
    }
    // Si una rectificativa deja negativa la base de un tipo, va a «modificación de bases y cuotas» (14-15):
    // las casillas 01-09 no admiten negativos.
    for (b, q) in [(1, 3), (4, 6), (7, 9)] {
        let (vb, vq) = (c.v(b), c.v(q));
        if vb < 0.0 || vq < 0.0 {
            add(&mut c, 14, vb);
            add(&mut c, 15, vq);
            c.insert(b, 0.0);
            c.insert(q, 0.0);
        }
    }
    let c27 = r2(c.v(3) + c.v(6) + c.v(9) + c.v(11) + c.v(13) + c.v(15));
    c.insert(27, c27);
    let c45 = r2(c.v(29) + c.v(31) + c.v(33) + c.v(35) + c.v(37) + c.v(39) + c.v(41) + c.v(42) + c.v(43) + c.v(44));
    c.insert(45, c45);
    c.insert(46, r2(c27 - c45));
    c
}

fn trimestre_anterior(anio: i32, q: u32) -> (i32, u32) {
    if q <= 1 { (anio - 1, 4) } else { (anio, q - 1) }
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
    let c78 = r2(c110.min((c66 + c77).max(0.0)));
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

    #[test]
    fn trimestres_encadenados() {
        assert_eq!(trimestre_anterior(2026, 1), (2025, 4));
        assert_eq!(trimestre_anterior(2026, 3), (2026, 2));
    }
}

#[cfg(test)]
pub(crate) mod pruebas {
    //! Pruebas con un 303 hecho por Hacienda con datos inventados (`pruebas/pdf/`, ver `generar.py`).
    use super::M303;
    use crate::dominio::hacienda::Tipo;
    use crate::dominio::Valor;
    use crate::hacienda::formulario::{descuadres, leer};
    use crate::hacienda::modelo::Periodo;
    use crate::hacienda::pdf::{self, Documento, Linea, Pagina, Palabra};
    use crate::hacienda::modelo_130::M130;
    use std::path::Path;

    /// El 303 del 1T de 2025, a compensar: 2.000 € de ventas al 21 % y 5.000 € de compras al 21 %.
    pub(crate) fn pdf_de_prueba() -> Documento {
        pdf::abrir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("pruebas/pdf/2025_1T_303_compensar.pdf")).unwrap()
    }

    /// El mismo con la página de la presentación delante, como los que se bajan de Hacienda (los de prueba no la
    /// llevan: no se han presentado). Con un justificante y una fecha inventados; `resultado`, «A COMPENSAR»…
    pub(crate) fn con_portada(resultado: &str) -> Documento {
        let linea = |top: f64, texto: &str| {
            let palabras: Vec<Palabra> = texto
                .split(' ')
                .enumerate()
                .map(|(i, t)| Palabra { texto: t.into(), x0: 60.0 + 60.0 * i as f64, x1: 110.0 + 60.0 * i as f64, top, bottom: top + 10.0, fuente: "NimbusSanL-Bold".into(), tamano: 10.0 })
                .collect();
            Linea { top, bottom: top + 10.0, x0: 60.0, x1: palabras.last().map_or(60.0, |p| p.x1), palabras }
        };
        let portada = Pagina {
            numero: 1,
            ancho: 595.0,
            alto: 842.0,
            lineas: vec![
                linea(62.0, "INFORMACIÓN DE LA PRESENTACIÓN DE LA DECLARACIÓN"),
                linea(102.0, "Modelo 303"),
                linea(134.0, "Registro"),
                linea(152.0, "Presentación realizada el: 15-04-2025 a las 09:05:30"),
                linea(212.0, "Número de justificante: 3039999999999"),
                linea(232.0, "Vía de entrada: Presentación por Internet"),
                linea(347.0, resultado),
            ],
        };
        let mut d = pdf_de_prueba();
        for p in &mut d.paginas {
            p.numero += 1;
        }
        d.paginas.insert(0, portada);
        d
    }

    #[test]
    fn lee_el_303_de_prueba() {
        let d = pdf_de_prueba();
        let p = Periodo::de;
        let l = leer::<M303>(&d, 2025, &p("1T")).unwrap();
        // Las del diseño de 2025: todas, las que no se ven a 0.
        assert_eq!(l.casillas.len(), 109);
        for (n, v) in [(7, 2000.0), (9, 420.0), (28, 5000.0), (29, 1050.0), (45, 1050.0), (46, -630.0), (65, 100.0), (71, -630.0), (78, 0.0)] {
            assert_eq!(l.casillas.v(n), v, "casilla {n}");
        }
        assert!(descuadres::<M303>(&l.casillas).is_empty());
        // Sin la página de la presentación no se sabe cómo se presentó ni su justificante.
        assert_eq!((l.tipo, l.justificante.as_str()), (None, ""));
        let l = leer::<M303>(&con_portada("A COMPENSAR"), 2025, &p("1T")).unwrap();
        assert_eq!((l.tipo, l.justificante.as_str()), (Some(Tipo::Compensar), "3039999999999"));
        assert_eq!(leer::<M303>(&con_portada("A DEVOLVER"), 2025, &p("1T")).unwrap().tipo, Some(Tipo::Devolver));
        // De otro periodo o de otro modelo, no.
        let error = |r: Result<_, crate::hacienda::modelo::Fallo>| r.unwrap_err().to_string();
        assert_eq!(error(leer::<M303>(&d, 2025, &p("2T"))), "El PDF es del 1T de 2025, no del 2T de 2025.");
        assert_eq!(error(leer::<M303>(&d, 2024, &p("1T"))), "El PDF es del 1T de 2025, no del 1T de 2024.");
        assert_eq!(error(leer::<M130>(&d, 2025, &p("1T"))), "El PDF es de un modelo 303, no de un 130.");
        // Si se toca un importe (la cuota deducible, 29), ya no cuadra con el total a deducir (45).
        let mut mal = d.clone();
        let w = mal.paginas.iter_mut().flat_map(|p| p.lineas.iter_mut()).flat_map(|l| l.palabras.iter_mut()).find(|w| w.texto == "1.050,00").unwrap();
        w.texto = "1.060,00".into();
        assert_eq!(error(leer::<M303>(&mal, 2025, &p("1T"))), "Lo que he leído no cuadra (casilla 45).");
    }
}
