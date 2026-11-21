//! Modelo 390 · el resumen anual del IVA. Todo lo que la app sabe de él:
//!
//! 1. **Qué casillas tiene**: las que se vean en el PDF. Hay diseño de registro oficial (`dr390e2025.xlsx`, en
//!    sede.agenciatributaria.gob.es → Ayuda → Diseños de registro → Modelos 300 al 399), pero la app aún no lo usa.
//! 2. **Cómo se lee** de su PDF: como cualquier formulario (`formulario::leer`), y lo leído tiene que cuadrar con
//!    sus sumas (`reglas`).
//! 3. **Cuándo se presenta**: con el 303 del 4T, hasta el 30 de enero (`hacienda::plazos_trimestres`). No lo
//!    presentan quienes llevan los libros del IVA en la sede de la AEAT (SII) ni quienes solo tienen el régimen
//!    simplificado o alquileres.
//! 4. **Cómo se calcula** (`calcular`): es el resumen de tus cuatro 303, con lo devengado y lo deducible por tipos,
//!    según las instrucciones oficiales del 390 de 2025 (sede.agenciatributaria.gob.es → IVA → Modelo 390 →
//!    Instrucciones).
use super::formulario::{suma, Formulario, QueCasillas};
use super::modelo::{Memo, Modelo, Periodicidad, Trimestral};
use super::modelo_303::{self, Doc, Negativo, TipoIva};
use crate::dominio::fmt::{periodo, r2, DIF};
use crate::dominio::gasto::cat_de;
use crate::dominio::{Casillas, Datos, Valor};

/// El modelo 390, para leerlo (`formulario::leer::<M390>`).
pub struct M390;

impl Formulario for M390 {
    const MODELO: Modelo = Modelo::M390;
    const PERIODICIDAD: Periodicidad = Periodicidad::Anual;
    const CASILLAS: QueCasillas = QueCasillas::LasQueSeVean;
    /// El resultado del régimen general y el volumen de operaciones.
    const ANCLAS: &'static [u32] = &[65, 108];
    /// Es un resumen: no se ingresa ni se devuelve nada con él.
    const TIPO_EN_PORTADA: bool = false;

    fn reglas(c: &Casillas) -> Vec<(u32, f64)> {
        let v = |n| c.v(n);
        let mut r = vec![
            (33, suma(c, &BASES)),
            (34, suma(c, &CUOTAS)),
            (47, suma(c, &[34, 664, 692, 36, 666, 694, 600, 602, 42, 44, 46])),
            (64, suma(c, &DEDUCCIONES)),
            (65, v(47) - v(64)),
            (108, suma(c, &[99, 653, 103, 104, 105, 110, 100, 101, 102, 125, 126, 127, 128, 227, 228]) - v(106) - v(107)),
        ];
        // La liquidación anual (apartado 7) solo la rellena quien tributa solo en territorio común.
        if c.contains_key(&84) {
            r.push((84, v(65) + v(83) + v(658)));
        }
        if c.contains_key(&86) {
            r.push((86, v(84) + v(659) - v(85)));
        }
        r
    }
}

/// Bases del IVA devengado (de todos los años desde 2020: las que no tiene un año valen 0). Su suma es la 33.
const BASES: [u32; 47] = [
    700, 667, 1, 702, 669, 3, 5, // régimen ordinario
    704, 671, 500, 706, 673, 502, 504, // operaciones intragrupo
    708, 675, 643, 710, 677, 645, 647, // criterio de caja
    712, 679, 7, 714, 681, 9, 11, // bienes usados
    13, // agencias de viajes
    716, 683, 21, 718, 685, 23, 25, // adquisiciones intracomunitarias de bienes
    720, 687, 545, 722, 689, 547, 551, // adquisiciones intracomunitarias de servicios
    27, 29, 649, 31, // inversión del sujeto pasivo y modificaciones
];
/// Sus cuotas, en el mismo orden. Su suma es la 34.
const CUOTAS: [u32; 47] = [
    701, 668, 2, 703, 670, 4, 6, //
    705, 672, 501, 707, 674, 503, 505, //
    709, 676, 644, 711, 678, 646, 648, //
    713, 680, 8, 715, 682, 10, 12, //
    14, //
    717, 684, 22, 719, 686, 24, 26, //
    721, 688, 546, 723, 690, 548, 552, //
    28, 30, 650, 32,
];

/* ================= 4. calcularlo con tus 303 ================= */

/// Cómo pasa al 390 una pareja de casillas (base y cuota) del 303.
enum Destino {
    /// A una pareja fija del 390.
    Fija(u32, u32),
    /// Por tipos (4, 10 y 21 %), con su total si lo tiene; las adquisiciones intracomunitarias, también separando
    /// bienes y servicios (bienes, servicios). Lo de otro tipo (el 5 % de 2022–2024), al total, y se dice
    /// (`Calculado::sin_repartir`): la app no reparte esos tipos.
    PorTipo { bienes: [(u32, u32); 3], servicios: [(u32, u32); 3], total: Option<[(u32, u32); 2]> },
}

/// De cada pareja del 303 a la del 390 (instrucciones del 390, apartado 5: régimen general).
const PAREJAS: [(u32, u32, Destino); 11] = [
    // El 0 %: «01 a 06, 700 a 703 y 667 a 670 Régimen ordinario: … gravadas, a los tipos del 0%, 4%,10% y 21 %»
    // (instrucciones del 390 de 2025).
    (150, 152, Destino::Fija(700, 701)),
    (1, 3, Destino::Fija(1, 2)),
    (4, 6, Destino::Fija(3, 4)),
    (7, 9, Destino::Fija(5, 6)),
    (12, 13, Destino::Fija(27, 28)),
    (14, 15, Destino::Fija(29, 30)),
    // Adquisiciones intracomunitarias: de bienes (21-26) y de servicios (545-552).
    (10, 11, Destino::PorTipo { bienes: [(21, 22), (23, 24), (25, 26)], servicios: [(545, 546), (547, 548), (551, 552)], total: None }),
    // IVA deducible en operaciones interiores, corrientes (48-49) y de bienes de inversión (50-51).
    (28, 29, Destino::PorTipo { bienes: [(190, 191), (603, 604), (605, 606)], servicios: [(190, 191), (603, 604), (605, 606)], total: Some([(48, 49), (48, 49)]) }),
    (30, 31, Destino::PorTipo { bienes: [(196, 197), (611, 612), (613, 614)], servicios: [(196, 197), (611, 612), (613, 614)], total: Some([(50, 51), (50, 51)]) }),
    // En adquisiciones intracomunitarias: bienes corrientes (56-57) y servicios (597-598); bienes de inversión (58-59).
    (36, 37, Destino::PorTipo { bienes: [(214, 215), (627, 628), (629, 630)], servicios: [(587, 588), (635, 636), (637, 638)], total: Some([(56, 57), (597, 598)]) }),
    (38, 39, Destino::PorTipo { bienes: [(220, 221), (631, 632), (633, 634)], servicios: [(220, 221), (631, 632), (633, 634)], total: Some([(58, 59), (58, 59)]) }),
];

/// Volumen de operaciones (apartado 10): de la casilla del 303 a la del 390.
const VOLUMEN: [(u32, u32); 4] = [(59, 103), (60, 104), (120, 110), (122, 125)];

/// Lo calculado del 390 y lo que la app no ha sabido repartir por tipos.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calculado {
    pub casillas: Casillas,
    /// Las casillas de total (48, 597…) que llevan algo que no se sabe a qué tipo va: de un 303 presentado sin
    /// tus gastos de ese trimestre. Hay que repartirlas a mano.
    pub sin_repartir: Vec<u32>,
    /// Se conocen todos los 303 del año (leídos de su PDF o con tus facturas y gastos): si no, el 390 sale a medias.
    pub completo: bool,
}

/// Las casillas del 390 que reparten por tipos lo de una casilla del 303.
pub fn por_tipos() -> impl Iterator<Item = u32> {
    PAREJAS.iter().flat_map(|(_, _, d)| match d {
        Destino::PorTipo { bienes, servicios, .. } => bienes.iter().chain(servicios.iter()).flat_map(|(b, c)| [*b, *c]).collect(),
        Destino::Fija(..) => Vec::new(),
    })
}

/// ¿Es un servicio (y no un bien) lo que compraste a una empresa de la UE? Lo dice su categoría.
fn servicio(g: &crate::dominio::Gasto) -> bool {
    let c = cat_de(&g.categoria);
    c.amort.is_empty() && !matches!(c.id, "material" | "compras")
}

/// La última autoliquidación del año: la del 4T, o la del último trimestre presentado o con datos si en tu perfil
/// no está la fecha de alta y después no hay nada (te diste de baja).
fn ultimo_trimestre(d: &Datos, anio: i32) -> u32 {
    if !d.fechas_actividad().0.is_empty() {
        return (1..=4).rev().find(|&q| modelo_303::en_actividad(d, anio, q)).unwrap_or(4);
    }
    let con_algo = |q| {
        let (desde, hasta) = periodo(anio, q);
        super::consta(d, anio, q, Modelo::M303) || !modelo_303::apuntes(d, &desde, &hasta).is_empty()
    };
    (1..=4).rev().find(|&q| con_algo(q)).unwrap_or(4)
}

/// El 390 de un año con tus 303: los que cuentan (presentados, si se han leído; si no, calculados con tus datos).
/// Lo de cada 303 se reparte por tipos con tus facturas y gastos de ese trimestre si suman lo mismo; si no (un 303
/// presentado sin tus gastos), por la proporción de su cuota si es justo un tipo; y si no, va al total y se dice.
pub fn calcular(d: &Datos, anio: i32, memo: &mut Memo) -> Calculado {
    let mut c = Casillas::new();
    let mut sin_repartir = Vec::new();
    fn add(c: &mut Casillas, k: u32, v: f64) {
        if v.abs() > DIF {
            let x = c.entry(k).or_insert(0.0);
            *x = r2(*x + v);
        }
    }
    let ultimo = ultimo_trimestre(d, anio);
    let cuenta: Vec<Casillas> = (1..=ultimo).map(|q| Trimestral::M303.cuenta(d, anio, q, memo)).collect();
    let completo = (1..=ultimo).all(|q| {
        let (desde, hasta) = periodo(anio, q);
        Modelo::M303.leido(d, anio, q).is_some() || !modelo_303::apuntes(d, &desde, &hasta).is_empty()
    });
    for (i, c303) in cuenta.iter().enumerate() {
        let q = i as u32 + 1;
        let (desde, hasta) = periodo(anio, q);
        let apuntes = modelo_303::apuntes(d, &desde, &hasta);
        for (kb, kc, destino) in &PAREJAS {
            let (b, cu) = (c303.v(*kb), c303.v(*kc));
            match destino {
                Destino::Fija(xb, xc) => {
                    add(&mut c, *xb, b);
                    add(&mut c, *xc, cu);
                }
                Destino::PorTipo { bienes, servicios, total } => {
                    let suyos: Vec<_> = apuntes.iter().filter(|a| a.kb == *kb).collect();
                    let (sb, sc) = (suyos.iter().map(|a| a.base).sum::<f64>(), suyos.iter().map(|a| a.cuota).sum::<f64>());
                    // El de cada tipo del 390 (4, 10 y 21 %); otro, ninguno.
                    let tipo = |t: f64| match TipoIva::de(t) {
                        Some(TipoIva::Superreducido) => Some(0),
                        Some(TipoIva::Reducido) => Some(1),
                        Some(TipoIva::General) => Some(2),
                        Some(TipoIva::Cero) | None => None,
                    };
                    // (es un servicio, su tipo, base, cuota): con tus documentos si suman lo mismo; si no, por la
                    // proporción de la cuota si es justo un tipo; y si no, sin tipo.
                    let partes: Vec<(bool, Option<usize>, f64, f64)> = if (sb - b).abs() <= DIF && (sc - cu).abs() <= DIF {
                        let es_servicio = |doc: &Doc| match doc {
                            Doc::Gasto(g) => servicio(g),
                            Doc::Factura(_) => true,
                        };
                        suyos.iter().map(|a| (es_servicio(&a.doc), tipo(a.tipo), a.base, a.cuota)).collect()
                    } else {
                        let t = [4.0, 10.0, 21.0].into_iter().find(|t| (r2(b * t / 100.0) - cu).abs() <= DIF && b.abs() > DIF);
                        vec![(true, t.and_then(tipo), b, cu)]
                    };
                    for (es_servicio, t, base, cuota) in partes {
                        match (t, total) {
                            (Some(t), _) => {
                                let (xb, xc) = if es_servicio { servicios[t] } else { bienes[t] };
                                add(&mut c, xb, base);
                                add(&mut c, xc, cuota);
                                if let Some(tot) = total {
                                    let (tb, tc) = tot[usize::from(es_servicio)];
                                    add(&mut c, tb, base);
                                    add(&mut c, tc, cuota);
                                }
                            }
                            // Sin tipo: al total, y se dice.
                            (None, Some(tot)) => {
                                let (tb, tc) = tot[usize::from(es_servicio)];
                                add(&mut c, tb, base);
                                add(&mut c, tc, cuota);
                                if base.abs() > DIF || cuota.abs() > DIF {
                                    sin_repartir.push(tb);
                                }
                            }
                            // Sin total en el 390 (adquisiciones intracomunitarias): a servicios al 21 %, y se dice.
                            (None, None) => {
                                let (xb, xc) = servicios[2];
                                add(&mut c, xb, base);
                                add(&mut c, xc, cuota);
                                sin_repartir.push(xb);
                            }
                        }
                    }
                }
            }
        }
        for (k303, k390) in VOLUMEN {
            add(&mut c, k390, c303.v(k303));
        }
        // Las operaciones exentas sin derecho a deducción (art. 20 LIVA) no van en el 303.
        for f in crate::dominio::factura::emitidas(d).filter(|f| f.operacion == "exenta" && crate::dominio::fmt::en_rango(f.devengo(), &desde, &hasta)) {
            add(&mut c, 105, crate::dominio::factura::totales_factura(f).base);
        }
    }
    // Totales y resultado (apartados 5, 7 y 10). El volumen en régimen general (99) son las bases de lo vendido con
    // IVA, sin las modificaciones de otros años (29); con las del 0 % («Se incluirán en esta casilla las operaciones a
    // tipo cero», instrucciones del 390 de 2025).
    let (c33, c34, c64) = (r2(suma(&c, &BASES)), r2(suma(&c, &CUOTAS)), r2(suma(&c, &DEDUCCIONES)));
    let c65 = r2(c34 - c64);
    let c99 = r2(c.v(700) + c.v(1) + c.v(3) + c.v(5));
    add(&mut c, 99, c99);
    let c108 = r2(suma(&c, &[99, 103, 104, 105, 110, 125]));
    // 85: lo pendiente de años anteriores (la 110 del 1T) que se aplicó en el año (en las 78).
    let pendiente = cuenta.first().map(|c| c.v(110)).unwrap_or(0.0);
    let aplicado: f64 = cuenta.iter().map(|c| c.v(78)).sum();
    let c85 = r2(pendiente.min(aplicado).max(0.0));
    add(&mut c, 85, c85);
    for (k, x) in [(33, c33), (34, c34), (47, c34), (64, c64), (65, c65), (84, c65), (86, r2(c65 - c85)), (108, c108)] {
        c.insert(k, x);
    }
    // 95: lo que salió a ingresar; 97/98: el resultado negativo de la última, a compensar o a devolver; 662: lo que
    // salió a compensar en los demás trimestres y no se aplicó en el año.
    let resultados: Vec<f64> = cuenta.iter().map(|c| Trimestral::M303.resultado_de(c)).collect();
    add(&mut c, 95, r2(resultados.iter().filter(|r| **r > 0.0).sum()));
    if let Some(&r) = resultados.last().filter(|r| **r < -DIF) {
        let k = if modelo_303::negativo(d, anio, ultimo) == Negativo::Devolver { 98 } else { 97 };
        add(&mut c, k, -r);
    }
    let generado: f64 = (1..ultimo).filter(|&q| resultados[q as usize - 1] < 0.0 && modelo_303::negativo(d, anio, q) == Negativo::Compensar).map(|q| -resultados[q as usize - 1]).sum();
    add(&mut c, 662, r2((generado - (aplicado - c85)).max(0.0)));
    sin_repartir.sort();
    sin_repartir.dedup();
    Calculado { casillas: c, sin_repartir, completo }
}

/// Las deducciones: su suma es la 64.
const DEDUCCIONES: [u32; 15] = [49, 513, 51, 521, 53, 55, 57, 59, 598, 61, 661, 62, 652, 63, 522];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hacienda::formulario::descuadres;

    use crate::dominio::{Factura, Gasto, Linea, Num};

    fn factura(fecha: &str, operacion: &str, precio: f64) -> Factura {
        let lineas = vec![Linea { concepto: "Servicio".into(), cantidad: Num::de(1.0), precio: Num::de(precio), iva: Num::de(21.0) }];
        Factura { estado: "emitida".into(), fecha: fecha.into(), operacion: operacion.into(), lineas, ..Default::default() }
    }

    fn gasto(fecha: &str, origen: &str, base: f64, iva: f64) -> Gasto {
        Gasto { categoria: "software".into(), origen: origen.into(), base: Num::de(base), iva_tipo: Num::de(iva), fecha: fecha.into(), ..Default::default() }
    }

    /// El 390 con tus facturas y gastos del año: por tipos, el volumen y lo que quedó a compensar.
    #[test]
    fn el_resumen_de_tus_303() {
        let facturas = vec![factura("2026-02-10", "nacional", 1000.0), factura("2026-05-10", "extra", 3000.0), factura("2026-11-10", "ue", 500.0)];
        let gastos = vec![gasto("2026-02-01", "es", 100.0, 21.0), gasto("2026-05-01", "es", 2000.0, 21.0), gasto("2026-08-01", "es", 50.0, 10.0), gasto("2026-11-01", "ue", 40.0, 21.0)];
        let d = Datos { facturas, gastos, ..Default::default() };
        let r = calcular(&d, 2026, &mut Memo::default());
        let c = &r.casillas;
        assert!(r.completo && r.sin_repartir.is_empty());
        assert_eq!((c.v(5), c.v(6), c.v(551), c.v(552)), (1000.0, 210.0, 40.0, 8.4));
        assert_eq!((c.v(605), c.v(606), c.v(603), c.v(604), c.v(48), c.v(49)), (2100.0, 441.0, 50.0, 5.0, 2150.0, 446.0));
        assert_eq!((c.v(637), c.v(638), c.v(597), c.v(598)), (40.0, 8.4, 40.0, 8.4));
        assert_eq!((c.v(33), c.v(34), c.v(64), c.v(65)), (1040.0, 218.4, 454.4, -236.0));
        assert_eq!((c.v(99), c.v(103), c.v(110), c.v(108)), (1000.0, 500.0, 3000.0, 4500.0));
        // 1T: 189 € a ingresar; 2T −420 y 3T −5, a compensar; el 4T sale 0 (8,40 − 8,40) y se pide devolver: con
        // los 425 pendientes (98); no queda nada en la 662.
        assert_eq!(c.v(95), 189.0);
        assert_eq!((c.v(97), c.v(98), c.v(662)), (0.0, 425.0, 0.0));
    }

    /// El 0 %, en su casilla del 390 (y en el volumen); un gasto con un tipo que no es de los de hoy (el 5 % de la luz
    /// de 2023) no va al 21 %: va al total, y se dice que hay que repartirlo a mano.
    #[test]
    fn cada_tipo_en_su_casilla() {
        let mut cero = factura("2026-02-10", "nacional", 500.0);
        cero.lineas[0].iva = Num::de(0.0);
        let d = Datos { facturas: vec![factura("2026-02-10", "nacional", 1000.0), cero], ..Default::default() };
        let r = calcular(&d, 2026, &mut Memo::default());
        let c = &r.casillas;
        assert_eq!((c.v(5), c.v(700), c.v(701), c.v(33), c.v(99)), (1000.0, 500.0, 0.0, 1500.0, 1500.0));
        assert!(r.sin_repartir.is_empty());
        let d = Datos { gastos: vec![gasto("2023-02-01", "es", 100.0, 21.0), gasto("2023-02-02", "es", 200.0, 5.0)], ..Default::default() };
        let r = calcular(&d, 2023, &mut Memo::default());
        let c = &r.casillas;
        assert_eq!((c.v(605), c.v(606), c.v(48), c.v(49)), (100.0, 21.0, 300.0, 31.0), "el 5 % no va al 21 %");
        assert_eq!(r.sin_repartir, [48]);
    }

    #[test]
    fn sumas_del_modelo() {
        // 05 − modificación de bases (29) = 33; el volumen (99) no resta la modificación.
        let r: Casillas = [(5, 10000.0), (29, -4000.0), (33, 6000.0), (99, 10000.0), (108, 10000.0)].into_iter().collect();
        assert!(descuadres::<M390>(&r).iter().all(|&n| n != 33 && n != 108), "{:?}", descuadres::<M390>(&r));
    }
}
