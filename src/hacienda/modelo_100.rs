//! Modelo 100 · la Renta: el IRPF del año. Todo lo que la app sabe de ella:
//!
//! 1. **Qué casillas tiene** (`Numeracion`): las que usa la app, que son las mismas de 2018 en adelante.
//!    Coinciden con los diccionarios oficiales de la AEAT de cada año (`diccionarioXSD_2020.properties` …
//!    `diccionarioXSD_2025.properties`, en sede.agenciatributaria.gob.es → Ayuda → Diseños de registro → Modelos
//!    100 al 199).
//! 2. **Cómo se lee** de su PDF (`leer`): no es un formulario, sino una lista
//!    («Ingresos de explotación 23.500,00 0171»). Se elige la numeración de casillas en la que cuadra la cuota
//!    diferencial (cuota resultante − pagos a cuenta), y cada bloque (trabajo, capital, actividad…) solo se queda
//!    si cuadran sus propias sumas: si alguna casilla no es lo que se espera, ese bloque se queda fuera en vez de
//!    salir mal (las casillas se ven igualmente en la ficha de la Renta). Lo que busca en el texto del PDF, al
//!    final (`Texto`).
//! 3. **Cuándo se presenta**: de abril a junio del año siguiente (`hacienda::plazo_renta`).
//!
//! La app no la calcula: la lee. Para saber si te retienen bien, `irpf` estima el IRPF de un sueldo.

use super::modelo::{Fallo, Modelo};
use super::pdf::{Documento, Palabra};
use crate::dominio::fmt::r2;
use crate::dominio::hacienda::Tipo;
use crate::dominio::renta::{Actividad, Bases, Capital, Cuotas, Ganancias, GananciasPerdidas, Inmuebles, Pagos, Plazo, Renta, Trabajo};
use crate::dominio::PRIMER_ANIO;
use std::collections::{BTreeMap, HashMap};

/// Versión de lo que sale (`renta.v`).
pub const VERSION: u32 = 9;

/* ================= 1. las casillas ================= */

// Hacienda puede cambiar la numeración de un año a otro: cada numeración trae las casillas que usa la app, y al leer
// un PDF se elige la que cuadra (cuota diferencial = cuota resultante − pagos a cuenta). La app lee desde 2020: basta
// la numeración de 2018 en adelante, que coincide con los diccionarios oficiales de 2020 a 2025
// (`diccionarioXSD_<año>.properties`: cada dato con su número de casilla y su descripción). Si algún año trae otra,
// se añade aquí una numeración.

/// Una numeración de casillas de la Renta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Numeracion {
    /// Su nombre (`renta.numeracion`).
    pub id: &'static str,
    /// Primer ejercicio que la usa.
    pub desde: i32,
    /// Cuota resultante de la autoliquidación.
    pub cuota: u32,
    /// Total de pagos a cuenta.
    pub pagos: u32,
    pub diferencial: u32,
    /// Resultado de la declaración.
    pub resultado: u32,
    pub ret_trabajo: u32,
    pub ret_capital: u32,
    pub ret_actividades: u32,
    /// Pagos fraccionados (modelo 130).
    pub fraccionados: u32,
    pub trabajo: CasTrabajo,
    pub capital: CasCapital,
    pub actividad: CasActividad,
    pub ganancias: CasGanancias,
    pub inmuebles: CasInmuebles,
    pub cuotas: CasCuotas,
    /// Comunidad autónoma de residencia (texto: «2025 CASTILLA Y LEÓN 0070»).
    pub comunidad: u32,
    /// Rectificativas: justificante de la declaración anterior, lo ya ingresado y devuelto, y lo que resulta ahora.
    pub anterior: u32,
    pub pagado_antes: u32,
    pub devuelto_antes: u32,
    pub regularizacion: u32,
    /// Tipo medio de gravamen de la base general: estatal y autonómico (%).
    pub tipo_medio: [u32; 2],
}

/// Rendimientos del trabajo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasTrabajo {
    /// Retribuciones dinerarias.
    pub dinerario: u32,
    /// En especie (valoración + ingresos a cuenta − repercutidos).
    pub especie: u32,
    /// Valoración de lo cobrado en especie (lo que vale lo que recibes).
    pub valoracion_especie: u32,
    /// Ingresos a cuenta de la especie: el IRPF de lo cobrado en especie, que ingresa tu empresa.
    pub ingresos_cuenta: u32,
    /// Los que te repercutió tu empresa (te los descontó de la nómina).
    pub repercutidos: u32,
    pub integros: u32,
    /// Cotizaciones a la Seguridad Social.
    pub ss: u32,
    /// Otros gastos que se restan para el rendimiento previo.
    pub otros: [u32; 3],
    /// Rendimiento neto previo: íntegros − Seguridad Social − otros.
    pub previo: u32,
    pub neto: u32,
    pub reducido: u32,
}

/// Rendimientos del capital mobiliario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasCapital {
    pub intereses: u32,
    pub dividendos: u32,
    pub integros: u32,
    pub gastos: u32,
    pub neto: u32,
    /// Suma a integrar en la base del ahorro (de todos los declarantes).
    pub ahorro: u32,
}

/// Actividad económica (la principal, en estimación directa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasActividad {
    /// Tipo, epígrafe IAE y modalidad (texto).
    pub tipo: u32,
    pub iae: u32,
    pub modalidad: u32,
    pub ingresos: u32,
    /// Seguridad Social del titular; si está vacía, la «a cargo de la empresa» (hay quien pone ahí su cuota).
    pub ss: &'static [u32],
    /// Total de gastos: el de la simplificada y, si no cuadra, el de la normal.
    pub gastos: &'static [u32],
    /// Provisiones y gastos de difícil justificación.
    pub gdj: u32,
    pub neto: u32,
    pub reduccion: u32,
    pub reducido: u32,
}

/// Ganancias y pérdidas patrimoniales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasGanancias {
    pub ganancias: u32,
    pub perdidas: u32,
    pub neto: u32,
    /// Criptomonedas: (ganancias, pérdidas).
    pub cripto: (u32, u32),
    /// Otros bienes: (ganancias, pérdidas).
    pub otros: (u32, u32),
}

/// Alquileres, inmueble a inmueble (ingresos, rendimiento neto y reducido), y sus sumas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasInmuebles {
    pub ingresos: u32,
    pub neto: u32,
    pub reducido: u32,
    /// Imputaciones de renta de los que no están alquilados.
    pub imputaciones: u32,
    /// Suma de rendimientos netos reducidos.
    pub suma: u32,
}

/// Bases, cuotas íntegras (estatal + autonómica) y líquidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CasCuotas {
    pub base_general: u32,
    pub base_ahorro: u32,
    pub liquidable_general: u32,
    pub liquidable_ahorro: u32,
    pub general: [u32; 2],
    pub ahorro: [u32; 2],
    pub integra: [u32; 2],
    pub liquida: [u32; 2],
}

/// La de 2018 en adelante (sin las casillas de la actividad y los inmuebles de 2018, que la app no lee).
pub const NUMERACION_2018: Numeracion = Numeracion {
    id: "2018",
    desde: 2018,
    cuota: 595,
    pagos: 609,
    diferencial: 610,
    resultado: 670,
    ret_trabajo: 596,
    ret_capital: 597,
    ret_actividades: 599,
    fraccionados: 604,
    trabajo: CasTrabajo { dinerario: 3, especie: 7, valoracion_especie: 4, ingresos_cuenta: 5, repercutidos: 6, integros: 12, ss: 13, otros: [14, 15, 16], previo: 17, neto: 22, reducido: 25 },
    capital: CasCapital { intereses: 27, dividendos: 29, integros: 36, gastos: 37, neto: 38, ahorro: 41 },
    actividad: CasActividad {
        tipo: 166,
        iae: 167,
        modalidad: 168,
        ingresos: 180,
        ss: &[186, 185],
        gastos: &[223],
        gdj: 222,
        neto: 224,
        reduccion: 234,
        reducido: 235,
    },
    ganancias: CasGanancias { ganancias: 422, perdidas: 423, neto: 424, cripto: (1814, 1813), otros: (386, 385) },
    inmuebles: CasInmuebles { ingresos: 102, neto: 149, reducido: 154, imputaciones: 155, suma: 156 },
    cuotas: CasCuotas {
        base_general: 435,
        base_ahorro: 460,
        liquidable_general: 505,
        liquidable_ahorro: 510,
        general: [532, 533],
        ahorro: [540, 541],
        integra: [545, 546],
        liquida: [570, 571],
    },
    comunidad: 70,
    anterior: 104,
    pagado_antes: 676,
    devuelto_antes: 677,
    regularizacion: 685,
    tipo_medio: [534, 535],
};

/// Las numeraciones que conoce la app, de la más reciente a la más antigua.
pub const NUMERACIONES: &[Numeracion] = &[NUMERACION_2018];

/// La numeración con la que se leyó una Renta (`A.numeracion100`); si no se sabe, la actual.
pub fn numeracion(id: &str) -> &'static Numeracion {
    NUMERACIONES.iter().find(|n| n.id == id).unwrap_or(&NUMERACIONES[0])
}

/* ================= 2. leerla de su PDF ================= */

fn igual(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.006
}

/// Lee la Renta de un ejercicio. Error si no es una Renta de ese año o no se reconoce la numeración.
pub fn leer(doc: &Documento, ejercicio: i32) -> Result<Renta, Fallo> {
    if ejercicio < PRIMER_ANIO {
        return Err(Fallo::Anterior(Modelo::M100));
    }
    let t = Texto::de(doc);
    if !es_modelo_100(&t.min) || !es_del_anio(&t.todo, ejercicio) {
        return Err(Fallo::NoEsLaRenta(ejercicio));
    }
    let k = Casillas::leer(doc, &t.todo);
    let v = |n: u32| k.v(n);
    let hay = |n: u32| k.hay(n);

    // La numeración de ese año: la más reciente que no sea posterior al ejercicio y en la que cuadre la cuota diferencial.
    let cuadra = |n: &&Numeracion| hay(n.resultado) && hay(n.diferencial) && igual(v(n.diferencial), r2(v(n.cuota) - v(n.pagos)));
    let l = NUMERACIONES
        .iter()
        .filter(|n| n.desde <= ejercicio)
        .find(cuadra)
        .or_else(|| NUMERACIONES.iter().find(cuadra))
        .ok_or(Fallo::Numeracion(ejercicio))?;

    // Lo que se paga o devuelve con esta declaración (en una rectificativa, la diferencia con la anterior).
    let rectificativa = es_rectificativa(&t.todo);
    let regulariza = rectificativa && hay(l.regularizacion);
    let mut mal = Vec::new();
    if regulariza && !igual(v(l.regularizacion), r2(v(l.resultado) - v(l.pagado_antes) + v(l.devuelto_antes))) {
        mal.push(l.regularizacion);
    }
    let a_mover = if regulariza { v(l.regularizacion) } else { v(l.resultado) };
    // Documento de ingreso o devolución (última página con contenido)
    let doc_ingreso = pagina_de_ingreso(&t.paginas);
    if resultado_a_ingresar(doc_ingreso).is_some_and(|x| !igual(x, a_mover)) {
        mal.push(700);
    }
    if !mal.is_empty() {
        return Err(Fallo::NoCuadra { modelo: Modelo::M100, casillas: mal });
    }
    let (forma, plazos) = forma_y_plazos(doc_ingreso, a_mover, ejercicio);

    let actividad = actividad(&k, l).map(|mut a| {
        let c = &l.actividad;
        a.tipo = texto_de_casilla(&t.todo, c.tipo);
        a.iae = texto_de_casilla(&t.todo, c.iae);
        a.modalidad = texto_de_casilla(&t.todo, c.modalidad);
        a
    });
    let comunidad = texto_de_comunidad(&t.todo, l.comunidad).map(|c| comunidad_de_texto(&c)).unwrap_or_default();
    Ok(Renta {
        v: VERSION,
        numeracion: l.id.into(),
        ejercicio,
        trabajo: trabajo(&k, l),
        capital: capital(&k, l),
        actividad,
        ganancias: ganancias(&k, l),
        inmuebles: inmuebles(&k, l),
        bases: hay(l.cuotas.base_general).then(|| Bases { general: v(l.cuotas.base_general), ahorro: v(l.cuotas.base_ahorro) }),
        cuotas: cuotas(&k, l),
        pagos: pagos(&k, l),
        comunidad,
        conjunta: es_conjunta(&t.min),
        resultado: v(l.resultado),
        cuota: v(l.cuota),
        pagos_cuenta: v(l.pagos),
        fraccionados: k.c.get(&l.fraccionados).copied(),
        diferencial: v(l.diferencial),
        rectificativa,
        anterior: justificante_anterior(&t.todo, l.anterior),
        pagado_antes: v(l.pagado_antes),
        devuelto_antes: v(l.devuelto_antes),
        regularizacion: regulariza.then(|| v(l.regularizacion)),
        forma,
        plazos,
        casillas: k.c,
    })
}

/// Cómo se paga o se devuelve (domiciliado, con reconocimiento de deuda, en el banco, a devolver, a cero; None si
/// no se ve) y los plazos.
fn forma_y_plazos(doc: &str, a_mover: f64, ejercicio: i32) -> (Option<Tipo>, Vec<Plazo>) {
    if a_mover < -0.005 {
        return (Some(Tipo::Devolver), Vec::new());
    }
    if a_mover < 0.005 {
        return (Some(Tipo::Cero), Vec::new());
    }
    let fp = forma_de_pago(doc);
    let fpm = fp.to_uppercase();
    let forma = if fpm.contains("DOMICILIA") {
        Some(Tipo::Domiciliado)
    } else if fpm.contains("RECONOCIMIENTO DE DEUDA") {
        Some(Tipo::ReconocimientoDeuda)
    } else if !fp.trim().is_empty() {
        Some(Tipo::Ingreso)
    } else {
        None
    };
    let fechas = fechas_de_cargo(doc);
    let fecha = |i: usize| fechas.get(i).cloned().unwrap_or_default();
    let min = doc.to_lowercase();
    let plazo = |importe: f64, fecha: String| Plazo { fecha, importe };
    if fracciona(&min) {
        // 60 % al presentar y 40 % hasta el 5 de noviembre (si el PDF no da los importes, se aplica la regla).
        let i1 = importe_del_plazo(&min, "primer").unwrap_or_else(|| r2(a_mover * 0.6));
        let i2 = importe_del_plazo(&min, "segundo").unwrap_or_else(|| r2(a_mover - i1));
        let f2 = fechas.get(1).cloned().unwrap_or_else(|| crate::hacienda::habil(&format!("{}-11-05", ejercicio + 1)));
        return (forma, vec![plazo(i1, fecha(0)), plazo(i2, f2)]);
    }
    (forma, vec![plazo(a_mover, if forma == Some(Tipo::Domiciliado) { fecha(0) } else { String::new() })])
}

/// ¿Se pagó en dos plazos (60 % y 40 %)? Cada año lo escribe de una forma: «Fracciona el pago», la casilla
/// «Fraccionamiento del pago en dos plazos X», «SÍ FRACCIONA X» o el «Importe del primer plazo».
fn fracciona(min: &str) -> bool {
    let marcada = |l: &str| l.split_whitespace().last() == Some("x");
    (min.contains("fracciona el pago") && !min.contains("no fracciona el pago"))
        || min.contains("importe del primer plazo")
        || min.lines().any(|l| (l.contains("fraccionamiento del pago en dos plazos") || l.contains("sí fracciona") || l.contains("si fracciona")) && marcada(l))
}

/// Rendimientos del trabajo: solo si íntegros ≥ dinerarios ≥ … y el rendimiento previo cuadra.
fn trabajo(k: &Casillas, l: &Numeracion) -> Option<Trabajo> {
    let t = &l.trabajo;
    let v = |n| k.v(n);
    let otros: f64 = t.otros.iter().map(|&n| v(n)).sum();
    let previo_ok = !k.hay(t.previo) || igual(v(t.previo), r2(v(t.integros) - v(t.ss) - otros));
    (k.hay(t.integros) && v(t.integros) + 0.005 >= v(t.dinerario) && v(t.neto) <= v(t.integros) + 0.005 && previo_ok).then(|| Trabajo {
        dinerario: v(t.dinerario),
        especie: v(t.especie),
        integros: v(t.integros),
        ss: v(t.ss),
        neto: Some(v(t.neto)),
        neto_reducido: Some(if k.hay(t.reducido) { v(t.reducido) } else { v(t.neto) }),
        retenciones: Some(v(l.ret_trabajo)),
    })
}

/// Rentas del ahorro: capital mobiliario (intereses, dividendos, total, gastos y neto). En una conjunta con
/// rendimientos de los dos, cada uno trae su bloque: entonces solo se sabe el total a integrar en el ahorro.
fn capital(k: &Casillas, l: &Numeracion) -> Option<Capital> {
    let c = &l.capital;
    let v = |n| k.v(n);
    if k.hay(c.integros) && igual(v(c.neto), r2(v(c.integros) - v(c.gastos))) {
        let div = v(c.dividendos);
        return Some(Capital {
            intereses: v(c.intereses),
            dividendos: div,
            otros: r2(v(c.integros) - v(c.intereses) - div),
            integros: v(c.integros),
            gastos: v(c.gastos),
            neto: if k.hay(c.ahorro) { v(c.ahorro) } else { v(c.neto) },
            ..Default::default()
        });
    }
    k.hay(c.ahorro).then(|| Capital { solo_total: true, integros: v(c.ahorro), neto: v(c.ahorro), ..Default::default() })
}

/// Pagos a cuenta: retenciones del trabajo, del capital mobiliario y de actividades, y el 130.
fn pagos(k: &Casillas, l: &Numeracion) -> Option<Pagos> {
    let p = [l.ret_trabajo, l.ret_capital, l.ret_actividades, l.fraccionados].map(|n| k.v(n));
    (k.hay(l.pagos) && r2(p.iter().sum()) <= k.v(l.pagos) + 0.005).then(|| Pagos {
        trabajo: p[0],
        capital: p[1],
        actividades: p[2],
        fraccionados: p[3],
        total: k.v(l.pagos),
    })
}

/// Actividad económica (la principal): lo facturado, los gastos (con la Seguridad Social del titular y la provisión
/// de difícil justificación) y el rendimiento; solo si rendimiento = ingresos − gastos.
fn actividad(k: &Casillas, l: &Numeracion) -> Option<Actividad> {
    let a = &l.actividad;
    let v = |n| k.v(n);
    let gastos = *a.gastos.iter().find(|&&n| k.hay(n) && igual(v(a.neto), r2(v(a.ingresos) - v(n))))?;
    k.hay(a.ingresos).then(|| Actividad {
        ingresos: v(a.ingresos),
        ss_titular: Some(a.ss.iter().map(|&n| v(n)).find(|x| *x != 0.0).unwrap_or(0.0)),
        gastos: v(gastos),
        gdj: Some(v(a.gdj)),
        neto: v(a.neto),
        reduccion: Some(v(a.reduccion)),
        neto_reducido: Some(if k.hay(a.reducido) { v(a.reducido) } else { v(a.neto) }),
        ..Default::default()
    })
}

/// Ganancias y pérdidas patrimoniales: sumas y saldo neto (sin las pérdidas de años anteriores que se compensen),
/// y aparte las de criptomonedas y otros bienes.
fn ganancias(k: &Casillas, l: &Numeracion) -> Option<Ganancias> {
    let g = &l.ganancias;
    let v = |n| k.v(n);
    let gp = |(ganancias, perdidas): (u32, u32)| Some(GananciasPerdidas { ganancias: v(ganancias), perdidas: v(perdidas) });
    ((k.hay(g.ganancias) || k.hay(g.perdidas)) && igual(v(g.neto), r2(v(g.ganancias) - v(g.perdidas)).max(0.0))).then(|| Ganancias {
        ganancias: v(g.ganancias),
        perdidas: v(g.perdidas),
        neto: v(g.neto),
        cripto: gp(g.cripto),
        otros: gp(g.otros),
    })
}

/// Inmuebles: lo cobrado de alquiler y lo que te deja (sin la reducción por alquiler de vivienda, que solo es
/// fiscal), y la renta imputada de los que no están alquilados (Hacienda la suma, pero no es dinero que cobres).
fn inmuebles(k: &Casillas, l: &Numeracion) -> Option<Inmuebles> {
    let i = &l.inmuebles;
    let suma = |n| r2(k.todas(n).iter().sum());
    (k.hay(i.suma) || k.hay(i.imputaciones)).then(|| {
        let reduccion = r2(suma(i.neto) - suma(i.reducido)).max(0.0);
        Inmuebles {
            ingresos: suma(i.ingresos),
            reducidos: k.v(i.suma),
            reduccion,
            neto: r2(k.v(i.suma) + reduccion),
            imputaciones: k.v(i.imputaciones),
        }
    })
}

/// Bases y cuotas: la general y la del ahorro (estatal + autonómica), solo si cuadran con la cuota íntegra.
/// Deducciones: de la íntegra a la líquida.
fn cuotas(k: &Casillas, l: &Numeracion) -> Option<Cuotas> {
    let c = &l.cuotas;
    let v = |n| k.v(n);
    let dos = |[a, b]: [u32; 2]| r2(v(a) + v(b));
    let todas = [c.general, c.ahorro, c.integra, c.liquida].iter().flatten().all(|&n| k.hay(n));
    let general_ahorro = r2(v(c.general[0]) + v(c.general[1]) + v(c.ahorro[0]) + v(c.ahorro[1]));
    (todas && igual(dos(c.integra), general_ahorro)).then(|| Cuotas {
        base_general: v(c.base_general),
        base_ahorro: v(c.base_ahorro),
        liquidable_general: if k.hay(c.liquidable_general) { v(c.liquidable_general) } else { v(c.base_general) },
        liquidable_ahorro: if k.hay(c.liquidable_ahorro) { v(c.liquidable_ahorro) } else { v(c.base_ahorro) },
        general: dos(c.general),
        ahorro: dos(c.ahorro),
        integra: dos(c.integra),
        liquida: dos(c.liquida),
    })
}

/* ================= 2. leerla de su PDF: lo que busca en el texto ================= */

// El texto del PDF, las casillas que hay en él y los patrones que busca, escritos a mano: el crate no tiene expresiones
// regulares, y así cada búsqueda es una pasada. Sale de `pdf::Documento` (pdfplumber): cada línea con sus palabras
// separadas por un espacio, de arriba abajo.

/// El texto de cada página (líneas separadas por «\n») y todo junto (páginas separadas por «\n»).
struct Texto {
    paginas: Vec<String>,
    todo: String,
    /// `todo` en minúsculas, para buscar sin distinguir mayúsculas.
    min: String,
}

impl Texto {
    fn de(doc: &Documento) -> Self {
        let paginas: Vec<String> = doc.paginas.iter().map(|p| p.texto()).collect();
        let todo = paginas.join("\n");
        let min = todo.to_lowercase();
        Texto { paginas, todo, min }
    }
}

/// Casillas de la Renta: en los PDF actuales vienen en el texto («1.234,56 0670»); además se casa cada importe
/// con el número que tiene justo a su derecha en la misma línea («2.345,67   010», por posición).
struct Casillas {
    /// Casilla → importe (la primera vez que sale).
    c: BTreeMap<u32, f64>,
    en_texto: HashMap<u32, Vec<f64>>,
    por_posicion: HashMap<u32, Vec<f64>>,
}

impl Casillas {
    fn leer(doc: &Documento, todo: &str) -> Self {
        let mut s = Casillas { c: BTreeMap::new(), en_texto: HashMap::new(), por_posicion: HashMap::new() };
        for (n, x) in importes_con_casilla(todo) {
            s.apunta(true, n, x);
        }
        for p in &doc.paginas {
            let palabras: Vec<&Palabra> = p.palabras().collect();
            let nums: Vec<&Palabra> = palabras.iter().copied().filter(|w| es_entero(&w.texto, 2, 4)).collect();
            for f in palabras.iter().filter(|w| es_importe(&w.texto)) {
                // el número más cercano por la derecha, en la misma línea y a menos de 60 puntos
                let mut der: Option<&Palabra> = None;
                for e in nums.iter().filter(|e| (e.bottom - f.bottom).abs() <= 3.0 && e.x0 > f.x0 && e.x0 - f.x1 < 60.0) {
                    if der.is_none_or(|d| e.x0 < d.x0) {
                        der = Some(e);
                    }
                }
                if let Some(e) = der {
                    s.apunta(false, e.texto.parse().unwrap_or(0), importe(&f.texto));
                }
            }
        }
        s
    }

    fn apunta(&mut self, texto: bool, n: u32, x: f64) {
        let l = if texto { &mut self.en_texto } else { &mut self.por_posicion };
        l.entry(n).or_default().push(x);
        self.c.entry(n).or_insert(x);
    }

    /// El importe de la casilla, o 0.
    fn v(&self, n: u32) -> f64 {
        self.c.get(&n).copied().unwrap_or(0.0)
    }

    fn hay(&self, n: u32) -> bool {
        self.c.contains_key(&n)
    }

    /// Cada vez que sale la casilla (para las que se repiten, como las de cada inmueble): de la forma que más
    /// encuentre, en el texto o por posición.
    fn todas(&self, n: u32) -> &[f64] {
        let t = self.en_texto.get(&n).map(Vec::as_slice).unwrap_or_default();
        let p = self.por_posicion.get(&n).map(Vec::as_slice).unwrap_or_default();
        if t.len() >= p.len() { t } else { p }
    }
}

/* ---------------- números ---------------- */

/// «1.234,56» → 1234.56
fn importe(s: &str) -> f64 {
    s.replace('.', "").replacen(',', ".", 1).parse().unwrap_or(0.0)
}

/// Una palabra que es un importe: `^-?[\d.]*\d,\d\d$`.
fn es_importe(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s).as_bytes();
    let n = s.len();
    n >= 4
        && s[n - 3] == b','
        && s[n - 2..].iter().all(u8::is_ascii_digit)
        && s[n - 4].is_ascii_digit()
        && s[..n - 3].iter().all(|c| c.is_ascii_digit() || *c == b'.')
}

/// `^\d{min,max}$`
fn es_entero(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit())
}

fn digito(b: &[u8], i: usize) -> bool {
    b.get(i).is_some_and(u8::is_ascii_digit)
}

/// `\w` de JavaScript (sin la bandera u).
fn de_palabra(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `-?\d{1,3}(?:\.\d{3})*,\d\d` justo en `i`: dónde acaba.
fn importe_miles_en(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + usize::from(b.get(i) == Some(&b'-'));
    let d = j;
    while digito(b, j) && j - d < 3 {
        j += 1;
    }
    if j == d || digito(b, j) {
        return None;
    }
    while b.get(j) == Some(&b'.') && (1..=3).all(|k| digito(b, j + k)) {
        j += 4;
    }
    (b.get(j) == Some(&b',') && digito(b, j + 1) && digito(b, j + 2)).then_some(j + 3)
}

/// `-?[\d.]+,\d\d` justo en `i` (sin el signo si `signo` es falso): dónde acaba.
fn numero_en(b: &[u8], i: usize, signo: bool) -> Option<usize> {
    let mut j = i + usize::from(signo && b.get(i) == Some(&b'-'));
    let d = j;
    while b.get(j).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
        j += 1;
    }
    (j > d && b.get(j) == Some(&b',') && digito(b, j + 1) && digito(b, j + 2)).then_some(j + 3)
}

/// Salta los espacios (`\s*`) desde `i`.
fn espacios(s: &str, i: usize) -> usize {
    s[i..].char_indices().find(|(_, c)| !c.is_whitespace()).map_or(s.len(), |(k, _)| i + k)
}

/// «importe casilla» en el texto: `(-?\d{1,3}(?:\.\d{3})*,\d\d) ([01]\d{3})(?!\d)`, todas las veces.
fn importes_con_casilla(todo: &str) -> Vec<(u32, f64)> {
    let b = todo.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(e) = importe_miles_en(b, i)
            && b.get(e) == Some(&b' ')
            && matches!(b.get(e + 1), Some(b'0' | b'1'))
            && (2..=4).all(|k| digito(b, e + k))
            && !digito(b, e + 5)
        {
            v.push((todo[e + 1..e + 5].parse().unwrap_or(0), importe(&todo[i..e])));
            i = e + 5;
            continue;
        }
        i += 1;
    }
    v
}

/* ---------------- lo que la web busca en el texto ---------------- */

/// `/Modelo\s*100\b/i`
fn es_modelo_100(min: &str) -> bool {
    min.match_indices("modelo").any(|(i, m)| {
        let j = espacios(min, i + m.len());
        min[j..].starts_with("100") && !de_palabra(min[j + 3..].chars().next())
    })
}

/// Que la Renta sea de ese año: «Ejercicio 2025», «…Personas Físicas 2015» o la referencia de la presentación
/// («2025100…»). pdfplumber la deja detrás de su etiqueta: basta con que empiece una palabra.
fn es_del_anio(todo: &str, anio: i32) -> bool {
    let y = anio.to_string();
    let anio_en = |j: usize| todo[j..].starts_with(&y) && !de_palabra(todo[j + y.len()..].chars().next());
    let ejercicio = todo.match_indices("Ejercicio").any(|(i, m)| anio_en(espacios(todo, i + m.len())));
    // F[ií]sicas[^\d]{0,40}AAAA\b: el primer dígito en los 40 caracteres siguientes es el del año
    let fisicas = ["Físicas", "Fisicas"].iter().any(|f| {
        todo.match_indices(f).any(|(i, m)| {
            let j = i + m.len();
            todo[j..].char_indices().take(41).find(|(_, c)| c.is_ascii_digit()).is_some_and(|(k, _)| anio_en(j + k))
        })
    });
    let referencia = format!("{y}100");
    let b = todo.as_bytes();
    let presentacion = todo.match_indices(&referencia).any(|(i, m)| {
        let antes = todo[..i].chars().next_back();
        (antes.is_none() || antes.is_some_and(char::is_whitespace)) && (0..6).all(|k| digito(b, i + m.len() + k))
    });
    ejercicio || fisicas || presentacion
}

/// `/Reducci[oó]n por tributaci[oó]n conjunta\s+[\d.]+,\d\d/i`
fn es_conjunta(min: &str) -> bool {
    let b = min.as_bytes();
    ["reducción", "reduccion"].iter().any(|r| {
        ["tributación", "tributacion"].iter().any(|t| {
            min.match_indices(&format!("{r} por {t} conjunta")).any(|(i, m)| {
                let j = i + m.len();
                let k = espacios(min, j);
                k > j && numero_en(b, k, false).is_some()
            })
        })
    })
}

/// `/Autoliquidaci[oó]n rectificativa X/`
fn es_rectificativa(todo: &str) -> bool {
    todo.contains("Autoliquidación rectificativa X") || todo.contains("Autoliquidacion rectificativa X")
}

/// La página del documento de ingreso o devolución.
fn pagina_de_ingreso(paginas: &[String]) -> &str {
    paginas
        .iter()
        .find(|t| t.contains("Documento de ingreso o devolución") || t.contains("Documento de ingreso o devolucion"))
        .map_or("", String::as_str)
}

/// `/Resultado a ingresar o devolver:?\s*(-?[\d.]+,\d\d)/`
fn resultado_a_ingresar(doc: &str) -> Option<f64> {
    let b = doc.as_bytes();
    doc.match_indices("Resultado a ingresar o devolver").find_map(|(i, m)| {
        let mut j = i + m.len();
        j += usize::from(b.get(j) == Some(&b':'));
        let k = espacios(doc, j);
        numero_en(b, k, true).map(|e| importe(&doc[k..e]))
    })
}

/// `/Forma de pago:?\s*([^\n]*)/`: lo que pone detrás (vacío si no está).
fn forma_de_pago(doc: &str) -> &str {
    let Some(i) = doc.find("Forma de pago") else { return "" };
    let mut j = i + "Forma de pago".len();
    j += usize::from(doc.as_bytes().get(j) == Some(&b':'));
    let k = espacios(doc, j);
    doc[k..].split('\n').next().unwrap_or("")
}

/// `/se cargar[aá] el d[ií]a:?\s*(\d\d)\/(\d\d)\/(\d{4})/g` → fechas ISO.
fn fechas_de_cargo(doc: &str) -> Vec<String> {
    let b = doc.as_bytes();
    let mut v = Vec::new();
    for (i, m) in doc.match_indices("se cargar") {
        let mut r = &doc[i + m.len()..];
        let Some(x) = r.strip_prefix('a').or_else(|| r.strip_prefix('á')) else { continue };
        let Some(x) = x.strip_prefix(" el d") else { continue };
        let Some(x) = x.strip_prefix('i').or_else(|| x.strip_prefix('í')) else { continue };
        let Some(x) = x.strip_prefix('a') else { continue };
        r = x.strip_prefix(':').unwrap_or(x);
        let j = espacios(doc, doc.len() - r.len());
        let f = |k: usize, n: usize| (0..n).all(|d| digito(b, j + k + d));
        if f(0, 2) && b.get(j + 2) == Some(&b'/') && f(3, 2) && b.get(j + 5) == Some(&b'/') && f(6, 4) {
            v.push(format!("{}-{}-{}", &doc[j + 6..j + 10], &doc[j + 3..j + 5], &doc[j..j + 2]));
        }
    }
    v
}

/// `/(primer|segundo) plazo[^\d\n-]*(-?[\d.]+,\d\d)/i` (en el texto en minúsculas).
fn importe_del_plazo(min: &str, cual: &str) -> Option<f64> {
    let b = min.as_bytes();
    min.match_indices(&format!("{cual} plazo")).find_map(|(i, m)| {
        let j = i + m.len();
        let k = min[j..].char_indices().find(|(_, c)| c.is_ascii_digit() || *c == '\n' || *c == '-').map_or(min.len(), |(k, _)| j + k);
        // lo que se salta puede devolver puntos al número (vuelta atrás de la expresión)
        std::iter::once(k)
            .chain((j..k).rev().filter(|&p| b[p] == b'.'))
            .find_map(|p| numero_en(b, p, true).map(|e| importe(&min[p..e])))
    })
}

/// `/(\d{13}) 0104/`: el justificante de la declaración anterior (rectificativas).
fn justificante_anterior(todo: &str, casilla: u32) -> String {
    let b = todo.as_bytes();
    let pat = format!(" {casilla:04}");
    todo.match_indices(&pat)
        .find(|(i, _)| *i >= 13 && (i - 13..*i).all(|k| digito(b, k)))
        .map(|(i, _)| todo[i - 13..i].to_string())
        .unwrap_or_default()
}

/// `/(\S+) NNNN(?!\d)/`: la palabra que va delante de una casilla de texto (tipo de actividad, epígrafe…).
fn texto_de_casilla(todo: &str, casilla: u32) -> String {
    let b = todo.as_bytes();
    let pat = format!(" {casilla:04}");
    todo.match_indices(&pat)
        .find_map(|(i, m)| {
            if digito(b, i + m.len()) {
                return None;
            }
            let ini = todo[..i].char_indices().rev().find(|(_, c)| c.is_whitespace()).map_or(0, |(k, c)| k + c.len_utf8());
            (ini < i).then(|| todo[ini..i].to_string())
        })
        .unwrap_or_default()
}

/// La comunidad autónoma de residencia (casilla 0070): «2025 CASTILLA Y LEÓN 0070» → «CASTILLA Y LEÓN»
/// (en 2023 venía «…habitual en 2023 PRINCIPADO DE ASTURIAS 70»).
/// `/(?:^|\n|\b20\d\d\s+)([A-ZÁÉÍÓÚÜÑ][A-ZÁÉÍÓÚÜÑ ,.'-]{2,60}?)\s+(?:00)?70\b/`
fn texto_de_comunidad(todo: &str, casilla: u32) -> Option<String> {
    let (larga, corta) = (format!("{casilla:04}"), casilla.to_string());
    let c: Vec<char> = todo.chars().collect();
    let mayuscula = |x: char| x.is_ascii_uppercase() || "ÁÉÍÓÚÜÑ".contains(x);
    let de_clase = |x: char| mayuscula(x) || " ,.'-".contains(x);
    let palabra = |k: usize| k < c.len() && (c[k].is_ascii_alphanumeric() || c[k] == '_');
    // el grupo empieza en g: una mayúscula y de 2 a 60 más (las menos posibles), espacios, «0070» o «70» y fin de palabra
    let grupo = |g: usize| -> Option<String> {
        if g >= c.len() || !mayuscula(c[g]) {
            return None;
        }
        for n in 1..=60 {
            let fin = g + 1 + n;
            if fin > c.len() || !de_clase(c[fin - 1]) {
                return None;
            }
            if n < 2 {
                continue;
            }
            let mut k = fin;
            while k < c.len() && c[k].is_whitespace() {
                k += 1;
            }
            if k == fin {
                continue;
            }
            let es = |s: &str| s.chars().enumerate().all(|(d, x)| c.get(k + d) == Some(&x)) && !palabra(k + s.len());
            if es(&larga) || es(&corta) {
                return Some(c[g..fin].iter().collect());
            }
        }
        None
    };
    for p in 0..c.len() {
        if p == 0
            && let Some(s) = grupo(0)
        {
            return Some(s);
        }
        if c[p] == '\n'
            && let Some(s) = grupo(p + 1)
        {
            return Some(s);
        }
        let limite = !(p > 0 && palabra(p - 1));
        let cifra = |k: usize| c.get(k).is_some_and(char::is_ascii_digit);
        if limite && c[p] == '2' && c.get(p + 1) == Some(&'0') && cifra(p + 2) && cifra(p + 3) {
            let mut g = p + 4;
            while g < c.len() && c[g].is_whitespace() {
                g += 1;
            }
            if g > p + 4
                && let Some(s) = grupo(g)
            {
                return Some(s);
            }
        }
    }
    None
}

/// «2025 CASTILLA Y LEÓN» → «castilla-leon».
fn comunidad_de_texto(t: &str) -> String {
    let n: String = t
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'Á' | 'À' | 'Â' | 'Ä' => 'A',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Ö' => 'O',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'ñ' | 'Ñ' => 'N',
            'ç' | 'Ç' => 'C',
            c => c.to_ascii_uppercase(),
        })
        .collect();
    let mancha = n.match_indices("CASTILLA").any(|(i, m)| {
        let r: Vec<char> = n[i + m.len()..].chars().take(4 + 9).collect();
        (0..=4).any(|k| r.iter().skip(k).take(9).collect::<String>() == "LA MANCHA" && !r.iter().take(k).any(|x| *x == '\n'))
    });
    let pares: [(&[&str], &str); 17] = [
        (&["ANDALUCIA"], "andalucia"),
        (&["ARAGON"], "aragon"),
        (&["ASTURIAS"], "asturias"),
        (&["BALEAR"], "baleares"),
        (&["CANARIAS"], "canarias"),
        (&["CANTABRIA"], "cantabria"),
        (&["CATALU"], "cataluna"),
        (&["EXTREMADURA"], "extremadura"),
        (&["GALICIA"], "galicia"),
        (&["MADRID"], "madrid"),
        (&["MURCIA"], "murcia"),
        (&["RIOJA"], "rioja"),
        (&["VALENCIA"], "valenciana"),
        (&["CEUTA"], "ceuta"),
        (&["MELILLA"], "melilla"),
        (&["PAIS VASCO", "EUSKADI", "ARABA", "ALAVA", "BIZKAIA", "GIPUZKOA"], "pais-vasco"),
        (&["NAVARRA"], "navarra"),
    ];
    if n.contains("CASTILLA Y LEON") {
        return "castilla-leon".into();
    }
    if mancha {
        return "castilla-mancha".into();
    }
    pares.iter().find(|(m, _)| m.iter().any(|x| n.contains(x))).map_or(String::new(), |(_, id)| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antes_de_2020_no() {
        assert_eq!(leer(&Documento::default(), PRIMER_ANIO - 1).unwrap_err().to_string(), format!("La app lee las Rentas desde {PRIMER_ANIO}."));
        assert_eq!(leer(&Documento::default(), 2024).unwrap_err(), Fallo::NoEsLaRenta(2024));
    }

    #[test]
    fn pago_en_dos_plazos() {
        assert!(fracciona("opciones de fraccionamiento\n  fraccionamiento del pago en dos plazos x\n"));
        assert!(fracciona("importe del primer plazo (60% del resultado de la declaración) 1.234,56"));
        assert!(fracciona("primer declarante sí fracciona x"));
        assert!(fracciona("fracciona el pago"));
        assert!(!fracciona("no fracciona el pago\nforma de pago: domiciliación"));
        assert!(!fracciona("fraccionamiento del pago en dos plazos\n"));
    }

    #[test]
    fn importes_y_casillas_del_texto() {
        let t = "Rendimiento neto 12.345,67 0038\nTipo -1.234,50 0670 y 12,00 01234 no; 1234,56 0100\nx 3,00 2000";
        assert_eq!(importes_con_casilla(t), vec![(38, 12345.67), (670, -1234.5), (100, 234.56)]);
        assert!(es_importe("1.234,56") && es_importe("-0,50") && !es_importe("1.234") && !es_importe(",50"));
        assert!(es_entero("0070", 2, 4) && !es_entero("7", 2, 4) && !es_entero("00700", 2, 4));
    }

    #[test]
    fn patrones() {
        assert!(es_modelo_100("modelo 100 ejercicio") && es_modelo_100("modelo\n100") && !es_modelo_100("modelo 1000"));
        assert!(es_del_anio("Modelo 100 Ejercicio 2025\n", 2025) && !es_del_anio("Ejercicio 20256", 2025));
        assert!(es_del_anio("Referencia: 2016100000001", 2016) && es_del_anio("Personas Físicas\nTeléfono 2015", 2015));
        assert!(!es_del_anio("Personas Físicas 901 33 2015", 2015));
        assert!(es_conjunta("reducción por tributación conjunta 3.400,00 0450") && !es_conjunta("tributación conjunta x 0069"));
        assert_eq!(resultado_a_ingresar("Resultado a ingresar o devolver: 1.234,56"), Some(1234.56));
        assert_eq!(resultado_a_ingresar("Resultado a ingresar o devolver (casilla [0670]) 1,00"), None);
        assert_eq!(forma_de_pago("Forma de pago …\nDOMICILIACIÓN"), "…");
        assert_eq!(forma_de_pago("Forma de pago\n  DOMICILIACIÓN ES00\nx"), "DOMICILIACIÓN ES00");
        assert_eq!(fechas_de_cargo("se cargará el día 01/07/2024 y se cargara el dia: 05/11/2024"), vec!["2024-07-01", "2024-11-05"]);
        assert_eq!(importe_del_plazo("importe del primer plazo (60% del resultado) 1.234,56", "primer"), None);
        assert_eq!(importe_del_plazo("primer plazo .......... 1.234,56", "primer"), Some(1234.56));
        assert_eq!(importe_del_plazo("segundo plazo ..5,00", "segundo"), Some(5.0));
        assert_eq!(justificante_anterior("Justificante 1000000000001 0104 x", 104), "1000000000001");
        assert_eq!(texto_de_casilla("Código y tipo de actividad A05 0166\nIAE 411 01670\nx 411 0167", 166), "A05");
        assert_eq!(texto_de_casilla("IAE 411 01670\nx 412 0167", 167), "412");
        assert_eq!(texto_de_comunidad("Vivienda X 0070\nresidencia habitual en\n2025 CASTILLA Y LEÓN 0070", 70).as_deref(), Some("CASTILLA Y LEÓN"));
        assert_eq!(texto_de_comunidad("en 2023 PRINCIPADO DE ASTURIAS 70\n", 70).as_deref(), Some("PRINCIPADO DE ASTURIAS"));
        assert_eq!(texto_de_comunidad("Vivienda habitual en 2025 X 0070", 70), None);
        assert_eq!(texto_de_comunidad("2025 A\nBC 0070", 70), None);
        assert_eq!(texto_de_comunidad("2025 ABC 0071\nLA RIOJA 0070", 70).as_deref(), Some("LA RIOJA"));
        assert_eq!(comunidad_de_texto("CASTILLA Y LEÓN"), "castilla-leon");
        assert_eq!(comunidad_de_texto("CASTILLA - LA MANCHA"), "castilla-mancha");
        assert_eq!(comunidad_de_texto("COMUNITAT VALENCIANA"), "valenciana");
        assert_eq!(comunidad_de_texto("X"), "");
    }
}

#[cfg(test)]
mod pruebas {
    //! Pruebas con Rentas de verdad extraídas y anonimizadas (`pruebas/renta_*.json`).
    use crate::dominio::renta::Renta;
    use crate::dominio::fmt::r2;
    use crate::dominio::PRIMER_ANIO;
    use crate::dominio::hacienda::Tipo;
    use crate::hacienda::pdf;
    use serde_json::Value;
    use std::path::Path;
    use std::time::Instant;

    const TOLERANCIA: f64 = 0.005;

    #[derive(Default)]
    struct Comparacion {
        campos: usize,
        iguales: usize,
        /// (campo, guardado, leído)
        distintos: Vec<(String, String, String)>,
    }

    fn igual(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= TOLERANCIA,
            (Value::Null, Value::Bool(false)) | (Value::Bool(false), Value::Null) => true,
            _ => a == b,
        }
    }

    fn comparar(g: &Value, l: &Value, ruta: &str, c: &mut Comparacion) {
        match (g, l) {
            (Value::Object(x), Value::Object(y)) => {
                let mut claves: Vec<&String> = x.keys().chain(y.keys()).collect();
                claves.sort();
                claves.dedup();
                for k in claves {
                    let sub = if ruta.is_empty() { k.clone() } else { format!("{ruta}.{k}") };
                    comparar(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &sub, c);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (a, b)) in x.iter().zip(y).enumerate() {
                    comparar(a, b, &format!("{ruta}[{i}]"), c);
                }
            }
            _ => {
                c.campos += 1;
                if igual(g, l) {
                    c.iguales += 1;
                } else {
                    c.distintos.push((ruta.to_string(), g.to_string(), l.to_string()));
                }
            }
        }
    }

    /// Que lo leído cuadre consigo mismo (todos los bloques).
    fn coherente(r: &Renta) -> Vec<String> {
        let mut mal = Vec::new();
        let mut ok = |cond: bool, que: &str| {
            if !cond {
                mal.push(que.to_string());
            }
        };
        let eq = |a: f64, b: f64| (a - b).abs() <= TOLERANCIA;
        ok(eq(r.diferencial, r2(r.cuota - r.pagos_cuenta)), "diferencial = cuota − pagos a cuenta");
        ok(r.casilla(670) == Some(r.resultado), "resultado = casilla 0670");
        if let Some(t) = &r.trabajo {
            ok(t.integros + TOLERANCIA >= t.dinerario && t.neto.unwrap_or(0.0) <= t.integros + TOLERANCIA, "trabajo: dinerario ≤ íntegros ≥ neto");
            ok(r.pagos.as_ref().is_none_or(|p| eq(p.trabajo, t.retenciones.unwrap_or(0.0))), "trabajo: retenciones = pagos.trabajo");
        }
        if let Some(c) = &r.capital {
            ok(c.solo_total || eq(c.otros, r2(c.integros - c.intereses - c.dividendos)), "capital: otros = íntegros − intereses − dividendos");
            ok(c.gastos <= c.integros + TOLERANCIA, "capital: gastos ≤ íntegros");
        }
        if let Some(a) = &r.actividad {
            ok(eq(a.neto, r2(a.ingresos - a.gastos)), "actividad: neto = ingresos − gastos");
            ok(a.gdj.unwrap_or(0.0) <= a.gastos + TOLERANCIA && a.ss_titular.unwrap_or(0.0) <= a.gastos + TOLERANCIA, "actividad: gdj y cuota ≤ gastos");
            ok(!a.tipo.is_empty() && !a.iae.is_empty() && !a.modalidad.is_empty(), "actividad: tipo, epígrafe y modalidad");
        }
        if let Some(g) = &r.ganancias {
            ok(eq(g.neto, r2(g.ganancias - g.perdidas).max(0.0)), "ganancias: neto = máx(0, ganancias − pérdidas)");
            let partes = g.cripto.as_ref().map_or(0.0, |x| x.ganancias) + g.otros.as_ref().map_or(0.0, |x| x.ganancias);
            ok(partes <= g.ganancias + TOLERANCIA, "ganancias: cripto + otros ≤ total");
        }
        if let Some(p) = &r.pagos {
            ok(r2(p.trabajo + p.capital + p.actividades + p.fraccionados) <= p.total + TOLERANCIA, "pagos: partes ≤ total");
            ok(eq(p.total, r.pagos_cuenta), "pagos: total = pagos a cuenta");
        }
        if let Some(c) = &r.cuotas {
            ok(eq(c.integra, r2(c.general + c.ahorro)), "cuotas: íntegra = general + ahorro");
            ok(c.liquida <= c.integra + TOLERANCIA, "cuotas: líquida ≤ íntegra");
            ok(r.bases.as_ref().is_some_and(|b| eq(b.general, c.base_general) && eq(b.ahorro, c.base_ahorro)), "cuotas: bases = bases");
        }
        if let Some(i) = &r.inmuebles {
            ok(eq(i.neto, r2(i.reducidos + i.reduccion)) && i.reduccion >= 0.0, "inmuebles: neto = reducidos + reducción");
        }
        let a_mover = if r.rectificativa { r.regularizacion.unwrap_or(r.resultado) } else { r.resultado };
        match r.forma {
            Some(Tipo::Devolver) => ok(a_mover < 0.0 && r.plazos.is_empty(), "forma D: a devolver, sin plazos"),
            Some(Tipo::Cero) => ok(a_mover.abs() < TOLERANCIA && r.plazos.is_empty(), "forma N: cero, sin plazos"),
            _ => ok(a_mover > 0.0 && eq(r2(r.plazos.iter().map(|p| p.importe).sum()), a_mover), "a ingresar: plazos = resultado"),
        }
        mal
    }

    /* ---------------- Rentas de verdad (no van al repositorio) ---------------- */

    /// Las Rentas de prueba de `pruebas/` (`renta_*.json`): sacadas de PDF de verdad, no van al repositorio (con los
    /// importes multiplicados por un factor no basta: se puede deducir). Sus pruebas se pasan con `--ignored`; hasta
    /// que haya Rentas con datos inventados en `pruebas/pdf/`.
    fn documento(nombre: &str) -> pdf::Documento {
        de_pruebas(nombre)
    }

    fn de_pruebas<T: serde::de::DeserializeOwned>(nombre: &str) -> T {
        let ruta = Path::new(env!("CARGO_MANIFEST_DIR")).join("pruebas").join(nombre);
        serde_json::from_str(&std::fs::read_to_string(ruta).unwrap()).unwrap()
    }

    /// Lee `pruebas/<nombre>.json` y lo compara con `pruebas/<nombre>_esperado.json`.
    fn leer_renta(nombre: &str, anio: i32, casillas: usize) {
        let doc = documento(&format!("{nombre}.json"));
        let t = Instant::now();
        let r = super::leer(&doc, anio).unwrap();
        println!("{nombre}: leído en {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!((r.v, r.casillas.len()), (super::VERSION, casillas));
        let mal = coherente(&r);
        assert!(mal.is_empty(), "{mal:?}");
        let esperado: Value = de_pruebas(&format!("{nombre}_esperado.json"));
        let leido = serde_json::to_value(&r).unwrap();
        let mut c = Comparacion::default();
        for (k, e) in esperado.as_object().unwrap() {
            comparar(e, &leido[k], k, &mut c);
        }
        assert!(c.distintos.is_empty(), "{:#?}", c.distintos);
        assert!(c.campos >= 50, "{}", c.campos);
    }

    #[test]
    #[ignore = "con una Renta de verdad (pruebas/renta_*.json), que no va al repositorio"]
    fn renta_anonima_con_todos_los_bloques() {
        leer_renta("renta_2020", 2020, 124);
    }

    #[test]
    #[ignore = "con una Renta de verdad (pruebas/renta_*.json), que no va al repositorio"]
    fn rectificativa_anonima() {
        leer_renta("renta_2024_rectificativa", 2024, 87);
    }

    /// Cambia el importe que va delante de una casilla en todas las líneas en las que sale.
    fn cambiar_casilla(doc: &mut pdf::Documento, casilla: &str, importe: &str) {
        for l in doc.paginas.iter_mut().flat_map(|p| p.lineas.iter_mut()) {
            let n = l.palabras.len();
            if n >= 2 && l.palabras[n - 1].texto == casilla {
                l.palabras[n - 2].texto = importe.into();
            }
        }
    }

    #[test]
    #[ignore = "con una Renta de verdad (pruebas/renta_*.json), que no va al repositorio"]
    fn errores_de_lectura() {
        let doc = documento("renta_2020.json");
        let error = |r: Result<Renta, crate::hacienda::modelo::Fallo>| r.unwrap_err().to_string();
        assert_eq!(error(super::leer(&doc, 2021)), "El PDF no es la Renta de 2021.");
        assert_eq!(error(super::leer(&doc, PRIMER_ANIO - 1)), format!("La app lee las Rentas desde {PRIMER_ANIO}."));
        // Si la cuota diferencial no cuadra, no se sabe qué numeración es.
        let mut otro = doc.clone();
        cambiar_casilla(&mut otro, "0610", "1,00");
        assert_eq!(error(super::leer(&otro, 2020)), "No reconozco la numeración de las casillas de la Renta de 2020.");
        // Una rectificativa cuya regularización no es resultado − lo ya ingresado + lo ya devuelto (y entonces tampoco
        // es lo que dice el documento de ingreso o devolución).
        let mut rect = documento("renta_2024_rectificativa.json");
        cambiar_casilla(&mut rect, "0685", "1,00");
        assert_eq!(error(super::leer(&rect, 2024)), "Lo que he leído no cuadra (casillas 0685, 0700).");
    }

    #[test]
    #[ignore = "con una Renta de verdad (pruebas/renta_*.json), que no va al repositorio"]
    fn un_bloque_que_no_cuadra_se_queda_fuera() {
        let mut doc = documento("renta_2020.json");
        // Si el rendimiento de la actividad no es ingresos − gastos, no hay bloque de actividad (el resto sigue).
        cambiar_casilla(&mut doc, "0224", "1,00");
        let r = super::leer(&doc, 2020).unwrap();
        assert!(r.actividad.is_none() && r.capital.is_some() && r.trabajo.is_some() && r.cuotas.is_some());
    }
}
