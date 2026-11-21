//! La actividad como autónomo de un año: las cuotas (recibos de la Seguridad Social por conceptos, meses sin
//! recibo, regularización) y lo facturado y gastado: de la app si hay facturas o gastos de ese año; si no, de la
//! Renta. Y lo apuntado frente a lo presentado (`conciliar`): qué casillas de un 303 o un 130 no cuadran con tus
//! datos y por qué puede ser.
use super::{editable, renta_de};
use crate::dominio::factura;
use crate::dominio::fmt::{self, r2, DIF};
use crate::dominio::gasto::{self, gasto_calc};
use crate::dominio::tgss::{Conceptos, Recibo, Regularizacion, Resultado};
use crate::dominio::{Casillas, Datos, Factura, Valor};
use crate::hacienda::formulario::Formulario;
use crate::hacienda::modelo::Memo;
use crate::hacienda::modelo_130::{self, OrigenRn, M130};
use crate::hacienda::modelo_303::{self, Apunte, Doc, M303};
use crate::hacienda::modelo_390;
use crate::hacienda::{irpf, Modelo, Trimestral};
use std::collections::{BTreeMap, BTreeSet};


/// ¿Está cerrado ese año? Uno pasado que ya no es editable (tiene Renta, o es de antes del anterior): se ve con lo
/// que presentaste y no se apuntan ni cambian facturas ni gastos.
pub fn cerrado(d: &Datos, anio: i32) -> bool {
    anio < fmt::anio_actual() && !editable(d, anio)
}

/// «2024 está cerrado: se ve con lo que presentaste.»
pub fn aviso_cerrado(anio: i32) -> String {
    format!("{anio} está cerrado: se ve con lo que presentaste.")
}

/* ================= cuotas ================= */

/// Cómo está un mes del año.
#[derive(Debug, Clone, PartialEq)]
pub enum EstadoMes {
    /// Con algún recibo cargado ese mes.
    Pagado,
    /// Su cuota se cargó otro mes (la fecha de cargo).
    OtroMes(String),
    /// De alta y sin recibo: no tocaba pagar (exoneración, prestación…) o falta el recibo.
    SinRecibo,
    /// Aún no ha llegado su recibo (el mes no ha terminado o es posterior a la consulta).
    Pendiente,
    /// No consta de alta (o es de antes de los recibos que da la Seguridad Social).
    Fuera,
}

/// Un mes: los recibos cargados en él.
#[derive(Debug, Clone, PartialEq)]
pub struct MesCuotas {
    /// 1–12.
    pub mes: u32,
    /// La cuota del mes primero y luego, si los hay, atrasos de otros meses.
    pub recibos: Vec<Recibo>,
    pub importe: f64,
    pub conceptos: Conceptos,
    pub estado: EstadoMes,
}

/// Las cuotas de autónomo de un año, según los recibos de la Seguridad Social (por la fecha de cargo).
#[derive(Debug, Clone, PartialEq)]
pub struct CuotasAnio {
    pub anio: i32,
    /// Los 12 meses (en el año en curso, hasta el último con recibo o el actual).
    pub meses: Vec<MesCuotas>,
    /// Todo lo cargado en el año (con los atrasos y lo que aún está por cargar).
    pub total: f64,
    pub conceptos: Conceptos,
    pub n_recibos: usize,
    /// Meses con su cuota normal cargada.
    pub meses_con_cuota: usize,
    /// Lo de las cuotas normales (sin atrasos).
    pub normales: f64,
    /// Atrasos de otros meses y otros conceptos.
    pub atrasos: f64,
    /// Lo que se carga después de hoy (el recibo del mes en curso) y cuándo.
    pub por_cargar: f64,
    pub por_cargar_el: String,
    /// Meses de alta sin recibo (1–12).
    pub sin_recibo: Vec<u32>,
    /// La regularización de ese año (la hace la Seguridad Social con tu Renta, al año siguiente).
    pub regularizacion: Option<Regularizacion>,
    pub alta: String,
    pub baja: String,
    /// Desde qué mes hay recibos («2018-06»): la Seguridad Social solo da los de los últimos años.
    pub datos_desde: String,
    /// La cuota de autónomo de la Renta de ese año (casilla 0186), para comparar.
    pub renta: Option<f64>,
}

impl CuotasAnio {
    /// Cuota media de los meses con cuota.
    pub fn media(&self) -> f64 {
        if self.meses_con_cuota == 0 { 0.0 } else { r2(self.normales / self.meses_con_cuota as f64) }
    }
    /// Base media de las cuotas normales del año (sale de las contingencias comunes).
    pub fn base_media(&self) -> Option<f64> {
        let v: Vec<f64> = self.meses.iter().flat_map(|m| m.recibos.iter()).filter_map(Recibo::base).collect();
        (!v.is_empty()).then(|| r2(v.iter().sum::<f64>() / v.len() as f64))
    }
    /// Lo que suman los conceptos menos lo pagado: > 0 es una reducción (tarifa plana, bonificación).
    pub fn reduccion(&self) -> f64 {
        r2(self.conceptos.suma() - self.total)
    }
    /// La última cuota de un mes del año (la más reciente).
    pub fn ultima(&self) -> Option<&Recibo> {
        self.meses.iter().rev().flat_map(|m| m.recibos.iter()).find(|r| r.es_cuota())
    }
}

/// Las cuotas de un año, a fecha de `hoy`.
pub fn cuotas_anio(d: &Datos, y: i32, hoy: &str) -> CuotasAnio {
    let ss = &d.tgss;
    let todos = &ss.recibos;
    let datos_desde = todos.iter().map(|r| fmt::ym(&r.periodo)).filter(|p| p.len() == 7).min().unwrap_or_default().to_string();
    // De alta: desde que cotizas (o el alta) hasta la baja, si es posterior.
    let alta = if ss.alta.cotiza_desde.is_empty() { ss.alta.fecha.clone() } else { ss.alta.cotiza_desde.clone() };
    let baja = if !ss.alta.baja.is_empty() && ss.alta.baja >= alta { ss.alta.baja.clone() } else { String::new() };
    let (desde, hasta) = (fmt::ym(&alta), fmt::ym(&baja));
    let registrado = |m: &str| ss.reta && !desde.is_empty() && m >= desde && (hasta.is_empty() || m <= hasta) && !datos_desde.is_empty() && m >= datos_desde.as_str();
    // Hasta dónde llegan los recibos: la consulta (o hoy, si es anterior).
    let consulta = ss.consultado.get(..10).filter(|c| fmt::fecha(c).is_some()).unwrap_or(hoy);
    let corte = fmt::ym(if consulta < hoy { consulta } else { hoy });

    let anio = y.to_string();
    let del_anio: Vec<&Recibo> = todos.iter().filter(|r| r.fecha_cargo().starts_with(&anio)).collect();
    let ultimo_mes = if y == fmt::anio_de(hoy) {
        let con = del_anio.iter().map(|r| fmt::mes_de(r.fecha_cargo())).max().unwrap_or(0);
        con.max(fmt::mes_de(hoy)).clamp(1, 12)
    } else if y > fmt::anio_de(hoy) {
        0
    } else {
        12
    };
    let mut meses = Vec::new();
    let mut c = CuotasAnio {
        anio: y,
        meses: Vec::new(),
        total: 0.0,
        conceptos: Conceptos::default(),
        n_recibos: 0,
        meses_con_cuota: 0,
        normales: 0.0,
        atrasos: 0.0,
        por_cargar: 0.0,
        por_cargar_el: String::new(),
        sin_recibo: Vec::new(),
        regularizacion: ss.regularizaciones.iter().find(|g| g.anio == y).cloned(),
        alta: ss.alta.fecha.clone(),
        baja: ss.alta.baja.clone(),
        datos_desde: datos_desde.clone(),
        renta: renta_de(d, y).and_then(|r| r.actividad.as_ref()).and_then(|a| a.ss_titular),
    };
    for m in 1..=ultimo_mes {
        let clave = format!("{y}-{m:02}");
        let mut recibos: Vec<Recibo> = del_anio.iter().filter(|r| fmt::ym(r.fecha_cargo()) == clave).map(|r| (*r).clone()).collect();
        recibos.sort_by(|a, b| b.es_cuota().cmp(&a.es_cuota()).then_with(|| a.periodo.cmp(&b.periodo)).then_with(|| a.fecha_cargo().cmp(b.fecha_cargo())));
        let mut conceptos = Conceptos::default();
        let mut importe = 0.0;
        for r in &recibos {
            conceptos.sumar(&r.conceptos);
            importe = r2(importe + r.importe);
            c.n_recibos += 1;
            if r.es_cuota() {
                c.normales = r2(c.normales + r.importe);
            }
            if r.por_cargar(hoy) {
                c.por_cargar = r2(c.por_cargar + r.importe);
                c.por_cargar_el = r.fecha_cargo().to_string();
            }
        }
        if recibos.iter().any(Recibo::es_cuota) {
            c.meses_con_cuota += 1;
        }
        let estado = if !recibos.is_empty() {
            EstadoMes::Pagado
        } else if let Some(r) = todos.iter().find(|r| r.es_cuota() && r.periodo == clave) {
            EstadoMes::OtroMes(r.fecha_cargo().to_string())
        } else if !registrado(&clave) {
            EstadoMes::Fuera
        } else if clave.as_str() >= corte {
            EstadoMes::Pendiente
        } else {
            c.sin_recibo.push(m);
            EstadoMes::SinRecibo
        };
        c.conceptos.sumar(&conceptos);
        c.total = r2(c.total + importe);
        meses.push(MesCuotas { mes: m, recibos, importe, conceptos, estado });
    }
    c.atrasos = r2(c.total - c.normales);
    c.meses = meses;
    c
}

/// «Sin diferencias», «A ingresar 321,09 €», «A devolver 120,00 €».
pub fn texto_regularizacion(g: &Regularizacion) -> String {
    if g.resultado == Resultado::SinDiferencias || g.importe.abs() <= DIF {
        return "Sin diferencias".into();
    }
    match g.resultado {
        Resultado::Devolver => format!("A devolver {}", fmt::eur(g.importe)),
        _ => format!("A ingresar {}", fmt::eur(g.importe)),
    }
}

/* ================= la actividad del año ================= */

/// De dónde salen las cifras de la actividad de un año.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fuente {
    /// Tus facturas y gastos de la app.
    App,
    /// Tu Renta de ese año (bloque de actividades económicas).
    Renta,
    /// No hay nada.
    Nada,
}

/// Lo que ganaste como autónomo en un año.
#[derive(Debug, Clone, PartialEq)]
pub struct ActividadAnio {
    pub anio: i32,
    pub fuente: Fuente,
    /// Ingresos (sin IVA): casilla 0180 de la Renta.
    pub ingresos: f64,
    /// Gastos deducibles sin la cuota de autónomo ni el de difícil justificación.
    pub gastos: f64,
    /// Cuota de autónomo (casilla 0186; con la app, tus gastos de «Cuota de autónomos»).
    pub cuota: f64,
    /// Gastos de difícil justificación (5 %, 7 % en 2023).
    pub gdj: f64,
    /// Rendimiento neto: ingresos − todos los gastos (casilla 0224).
    pub neto: f64,
    pub reduccion: Option<f64>,
    pub neto_reducido: Option<f64>,
    /// Retenciones de tus clientes.
    pub retenciones: Option<f64>,
    /// Pagos fraccionados (130) que dice la Renta.
    pub fraccionados: Option<f64>,
    pub iae: String,
    pub modalidad: String,
    pub n_facturas: usize,
    pub n_gastos: usize,
    /// Hasta qué día (en el año en curso, hoy).
    pub hasta: String,
}

/// ¿Hay facturas emitidas o gastos de ese año en la app?
pub fn hay_datos_app(d: &Datos, y: i32) -> bool {
    factura::emitidas(d).any(|f| fmt::anio_de(f.devengo()) == y) || gasto::apuntados(d).any(|g| fmt::anio_de(&g.fecha) == y)
}

/// La actividad de un año: de tus facturas y gastos si los hay (en un año abierto, o si no hay Renta); si no, de la
/// Renta de ese año.
pub fn actividad_anio(d: &Datos, y: i32, hoy: &str) -> ActividadAnio {
    let renta = renta_de(d, y);
    let act = renta.and_then(|r| r.actividad.as_ref());
    let app = hay_datos_app(d, y);
    let hasta = if y == fmt::anio_de(hoy) { hoy.to_string() } else { format!("{y}-12-31") };
    let vacio = ActividadAnio {
        anio: y,
        fuente: Fuente::Nada,
        ingresos: 0.0,
        gastos: 0.0,
        cuota: 0.0,
        gdj: 0.0,
        neto: 0.0,
        reduccion: None,
        neto_reducido: None,
        retenciones: None,
        fraccionados: None,
        iae: String::new(),
        modalidad: String::new(),
        n_facturas: 0,
        n_gastos: 0,
        hasta: hasta.clone(),
    };
    if app && (editable(d, y) || act.is_none()) {
        let a = irpf::irpf_acumulado(d, y, &hasta);
        let cuota = a.lineas.get("ss_titular").copied().unwrap_or(0.0);
        return ActividadAnio {
            fuente: Fuente::App,
            ingresos: a.ingresos,
            gastos: r2(a.gastos - a.gdj - cuota),
            cuota,
            gdj: a.gdj,
            neto: a.rn,
            retenciones: Some(a.retenciones),
            n_facturas: factura::emitidas(d).filter(|f| fmt::anio_de(f.devengo()) == y && f.devengo() <= hasta.as_str()).count(),
            n_gastos: d.gastos.iter().filter(|g| fmt::anio_de(&g.fecha) == y).count(),
            ..vacio
        };
    }
    let (Some(r), Some(a)) = (renta, act) else { return vacio };
    let cuota = a.ss_titular.unwrap_or(0.0);
    let gdj = a.gdj.unwrap_or(0.0);
    let pagos = r.pagos.as_ref();
    ActividadAnio {
        fuente: Fuente::Renta,
        ingresos: a.ingresos,
        gastos: r2(a.gastos - cuota - gdj),
        cuota,
        gdj,
        neto: a.neto,
        reduccion: a.reduccion,
        neto_reducido: a.neto_reducido,
        retenciones: pagos.map(|p| p.actividades),
        fraccionados: pagos.map(|p| p.fraccionados).or(r.fraccionados),
        iae: a.iae.clone(),
        modalidad: a.modalidad.clone(),
        ..vacio
    }
}

/* ================= lo apuntado frente a lo presentado ================= */

/// ¿Hay facturas emitidas o gastos apuntados entre esas fechas? (los recibos de autónomo no cuentan: no dicen si
/// hubo actividad)
pub fn hay_datos(d: &Datos, desde: &str, hasta: &str) -> bool {
    factura::emitidas(d).any(|f| fmt::en_rango(f.devengo(), desde, hasta)) || gasto::apuntados(d).any(|g| fmt::en_rango(&g.fecha, desde, hasta))
}

/// Una casilla de origen (no una suma de otras) en la que lo que sale con tus datos no es lo presentado.
#[derive(Debug, Clone, PartialEq)]
pub struct Diferencia {
    pub casilla: u32,
    pub app: f64,
    pub presentado: f64,
    /// Lo que puede explicarla (lo que cuadra al céntimo, o una pista).
    pub causas: Vec<String>,
}

impl Diferencia {
    /// Lo presentado menos lo de tus datos.
    pub fn dif(&self) -> f64 {
        r2(self.presentado - self.app)
    }
}

/// Un 303 o un 130 presentado frente a lo que sale con tus datos.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conciliacion {
    /// Las casillas de origen que no cuadran, con sus causas.
    pub diferencias: Vec<Diferencia>,
    /// Las que son sumas o restas de otras y no cuadran por las de arriba.
    pub arrastradas: Vec<u32>,
    /// Hay facturas o gastos del periodo. Si no, solo se comparan las casillas que vienen de los trimestres
    /// anteriores (lo pendiente de compensar del 303; los pagos y negativos anteriores del 130).
    pub con_datos: bool,
    /// El resultado es el mismo (sin datos, no se mira).
    pub mismo_resultado: bool,
}

impl Conciliacion {
    pub fn cuadra(&self) -> bool {
        self.diferencias.is_empty() && self.arrastradas.is_empty()
    }
    /// Cuántas casillas no cuadran.
    pub fn distintas(&self) -> usize {
        self.diferencias.len() + self.arrastradas.len()
    }
}

/// Lo presentado de un trimestre (si se ha leído su PDF) frente a lo que sale con tus datos.
pub fn cuadre(d: &Datos, modelo: Trimestral, anio: i32, q: u32, memo: &mut Memo) -> Option<Conciliacion> {
    let pres = modelo.modelo().leido(d, anio, q).filter(|l| !l.casillas.is_empty())?.casillas.clone();
    let calc = modelo.calcular(d, anio, q, memo);
    Some(conciliar(d, modelo, anio, q, &calc, &pres, memo))
}

/// Compara casilla a casilla lo presentado (`pres`) con lo que sale con tus datos (`calc`) y busca por qué no
/// cuadra: el mismo importe en otra casilla, un documento de otro trimestre, un IVA deducido de otra forma…
pub fn conciliar(d: &Datos, modelo: Trimestral, anio: i32, q: u32, calc: &Casillas, pres: &Casillas, memo: &mut Memo) -> Conciliacion {
    let con_datos = hay_datos(d, &modelo.desde(anio, q), &fmt::periodo(anio, q).1);
    let (sumas, encadenadas): (Vec<u32>, &[u32]) = match modelo {
        Trimestral::M303 => (M303::reglas(&Casillas::new()).into_iter().map(|r| r.0).chain([78]).collect(), &[110]),
        Trimestral::M130 => (M130::reglas(&Casillas::new()).into_iter().map(|r| r.0).collect(), &[5, 15]),
    };
    let distinta = |k: u32| (calc.v(k) - pres.v(k)).abs() > DIF;
    let casillas: BTreeSet<u32> = calc
        .keys()
        .chain(pres.keys())
        .copied()
        .filter(|k| !(modelo == Trimestral::M303 && modelo_303::TIPOS.contains(k)))
        .filter(|k| con_datos || encadenadas.contains(k))
        .filter(|&k| distinta(k))
        .collect();
    let mismo_resultado = !con_datos || (modelo.resultado_de(calc) - modelo.resultado_de(pres)).abs() <= DIF;
    let mut c = Conciliacion { con_datos, mismo_resultado, ..Default::default() };
    for k in casillas {
        // La 04 del 130 es el 20 % de la 03: es de origen solo si la 03 cuadra (se puede pagar más del 20 %).
        if sumas.contains(&k) || (modelo == Trimestral::M130 && k == 4 && distinta(3)) {
            c.arrastradas.push(k);
        } else {
            c.diferencias.push(Diferencia { casilla: k, app: calc.v(k), presentado: pres.v(k), causas: Vec::new() });
        }
    }
    for i in 0..c.diferencias.len() {
        let causas = match modelo {
            Trimestral::M303 => causas_303(d, anio, q, &c.diferencias, i, memo),
            Trimestral::M130 => causas_130(d, anio, q, &c.diferencias[i], memo),
        };
        c.diferencias[i].causas = causas;
    }
    c
}

fn igual(a: f64, b: f64) -> bool {
    (a - b).abs() <= DIF
}

/// «la factura 2026-003 (12 oct 2026)», «el gasto «Hosting» (3 jul 2026)».
fn nombre_doc(doc: Doc) -> String {
    match doc {
        Doc::Factura(f) if f.numero.is_empty() => format!("una factura del {}", fmt::fecha_corta(f.devengo())),
        Doc::Factura(f) => format!("la factura {} ({})", f.numero, fmt::fecha_corta(f.devengo())),
        Doc::Gasto(g) => {
            let que = if g.concepto.is_empty() { &g.proveedor } else { &g.concepto };
            let que: String = if que.chars().count() > 40 { que.chars().take(39).chain(['…']).collect() } else { que.clone() };
            format!("el gasto «{que}» ({})", fmt::fecha_corta(&g.fecha))
        }
    }
}

/// «del 2T», «del 4T de 2025».
fn del_trimestre(anio: i32, q: u32, de: i32) -> String {
    if anio == de { format!("del {q}T") } else { format!("del {q}T de {anio}") }
}

fn mayuscula(s: String) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn trimestre_siguiente(anio: i32, q: u32) -> (i32, u32) {
    if q >= 4 { (anio + 1, 1) } else { (anio, q + 1) }
}

/// Por qué no cuadra una casilla del 303.
fn causas_303(d: &Datos, anio: i32, q: u32, difs: &[Diferencia], i: usize, memo: &mut Memo) -> Vec<String> {
    let x = &difs[i];
    let (k, dif) = (x.casilla, x.dif());
    if k == 110 {
        return vec![causa_110(d, anio, q, x, memo)];
    }
    let mut v = Vec::new();
    // El mismo importe en otra casilla.
    for y in difs.iter().filter(|y| y.casilla != k && y.casilla != 110 && igual(y.dif(), -dif)) {
        let (pres, app) = if dif > 0.0 { (k, y.casilla) } else { (y.casilla, k) };
        v.push(if (pres, app) == (60, 120) {
            "Tus facturas a clientes de fuera de la UE: con tus datos van en la 120 («operaciones no sujetas por reglas de localización»: servicios a una empresa de fuera de la UE, art. 69 LIVA); en lo presentado, en la 60, que es para exportaciones de bienes. El resultado no cambia.".to_string()
        } else {
            format!("Es lo mismo que no cuadra en la casilla {}: en lo presentado va en la {} y con tus datos en la {}.", casillas_num(y.casilla), casillas_num(pres), casillas_num(app))
        });
    }
    // Un documento: de este trimestre y que no se declaró, o de otro y que se declaró en este.
    // (sin importes: la base y la cuota de un mismo documento dan la misma causa, y se ve una vez)
    let importe = |a: &Apunte| if a.kb == k { a.base } else { a.cuota };
    let (desde, hasta) = fmt::periodo(anio, q);
    for a in modelo_303::apuntes(d, &desde, &hasta).iter().filter(|a| a.kb == k || a.kc == k) {
        if igual(dif, -importe(a)) {
            v.push(mayuscula(format!("{} cuenta aquí con tus datos y no en lo presentado.", nombre_doc(a.doc))));
        }
    }
    for (y2, q2) in [modelo_303::trimestre_anterior(anio, q), trimestre_siguiente(anio, q)] {
        let (de, ha) = fmt::periodo(y2, q2);
        for a in modelo_303::apuntes(d, &de, &ha).iter().filter(|a| (a.kb == k || a.kc == k) && igual(dif, importe(a))) {
            v.push(mayuscula(format!("{} es {} según su fecha: ¿se declaró en este trimestre?", nombre_doc(a.doc), del_trimestre(y2, q2, anio))));
        }
    }
    // Un gasto cuyo IVA no te deduces entero.
    if (28..=39).contains(&k) {
        for g in d.gastos.iter().filter(|g| fmt::en_rango(&g.fecha, &desde, &hasta)) {
            let gc = gasto_calc(g, d);
            let (kb, kc) = modelo_303::casillas_deducible(&gc);
            if gc.origen == "sin" || gc.cuota <= DIF || gc.iva_ded >= gc.cuota {
                continue;
            }
            let doc = nombre_doc(Doc::Gasto(g));
            if k == kc && igual(dif, gc.cuota - gc.iva_ded) {
                v.push(mayuscula(if gc.iva_ded > DIF {
                    format!("{doc}: con tus datos te deduces el {} de su IVA ({}); en lo presentado, parece que entero ({}).", fmt::pct(gc.p_iva), fmt::eur(gc.iva_ded), fmt::eur(gc.cuota))
                } else {
                    format!("{doc}: con tus datos no te deduces su IVA ({}){}; en lo presentado, sí.", fmt::eur(gc.cuota), if g.iva_no_ded { ", por ser un tique sin tu NIF" } else { "" })
                }));
            } else if k == kb && gc.iva_ded <= DIF && igual(dif, gc.base) {
                v.push(mayuscula(format!("{doc}: como no te deduces su IVA, su base no va en el 303; en lo presentado, sí.")));
            }
        }
    }
    if v.is_empty() {
        let que = match k {
            10 | 11 => "algún gasto de una empresa de la UE",
            12 | 13 => "algún gasto de una empresa de fuera de la UE",
            28..=44 => "algún gasto",
            _ => "alguna factura",
        };
        v.push(if dif > 0.0 {
            format!("En lo presentado hay {} más: ¿falta apuntar {que} de este trimestre?", fmt::eur(dif))
        } else {
            format!("Con tus datos hay {} más: ¿hay {que} de otro trimestre o que no se declaró?", fmt::eur(-dif))
        });
    }
    v
}

/// La 110 (lo pendiente de compensar) sale del 303 anterior: su 87 y, si salió a compensar, su 71.
fn causa_110(d: &Datos, anio: i32, q: u32, x: &Diferencia, memo: &mut Memo) -> String {
    let (py, pq) = modelo_303::trimestre_anterior(anio, q);
    let leido = Modelo::M303.leido(d, py, pq).is_some();
    if !leido && py < d.primer_anio_con_apuntes() {
        return format!("Lo pendiente de compensar de antes de tus apuntes es lo que pusiste en tus ajustes: {}. En lo presentado, {}.", fmt::eur(x.app), fmt::eur(x.presentado));
    }
    let p = Trimestral::M303.cuenta(d, py, pq, memo);
    let a_compensar = if p.v(71) < 0.0 && modelo_303::negativo(d, py, pq) == modelo_303::Negativo::Compensar { -p.v(71) } else { 0.0 };
    let mut t = format!(
        "Tu 303 {}{} deja {} por compensar ({} de la 87 y {} a compensar de la 71); en lo presentado pusiste {}.",
        del_trimestre(py, pq, anio),
        if leido { "" } else { " (calculado con tus datos)" },
        fmt::eur(x.app),
        fmt::eur(p.v(87)),
        fmt::eur(a_compensar),
        fmt::eur(x.presentado)
    );
    if a_compensar > DIF && p.v(87) > DIF && igual(x.presentado, a_compensar) {
        t.push_str(&format!(" Parece que pusiste solo lo que salió a compensar ({}), sin lo que ya venía pendiente de antes ({}).", fmt::eur(a_compensar), fmt::eur(p.v(87))));
    } else if a_compensar > DIF && p.v(87) > DIF && igual(x.presentado, p.v(87)) {
        t.push_str(&format!(" Parece que pusiste solo lo que ya venía pendiente ({}), sin lo que salió a compensar en ese trimestre ({}).", fmt::eur(p.v(87)), fmt::eur(a_compensar)));
    }
    if x.presentado < x.app {
        t.push_str(" Lo que no se compensa no se pierde: se puede aplicar en los 303 siguientes durante cuatro años (art. 99.Cinco LIVA).");
    }
    t
}

/// «07», «110».
fn casillas_num(k: u32) -> String {
    if k < 100 { format!("{k:02}") } else { k.to_string() }
}

/// Por qué no cuadra una casilla del 130 (lo de sus casillas 01, 02 y 06 es lo acumulado desde el 1 de enero).
fn causas_130(d: &Datos, anio: i32, q: u32, x: &Diferencia, memo: &mut Memo) -> Vec<String> {
    let (k, dif) = (x.casilla, x.dif());
    let desde = format!("{anio}-01-01");
    let hasta = fmt::periodo(anio, q).1;
    let (sy, sq) = trimestre_siguiente(anio, q);
    let (sig_ini, sig_fin) = fmt::periodo(sy, sq);
    let mut v = Vec::new();
    match k {
        1 | 6 => {
            let importe = |f: &Factura| {
                let t = factura::totales_factura(f);
                if k == 1 { t.base } else { t.retencion }
            };
            for f in factura::emitidas(d).filter(|f| importe(f).abs() > DIF) {
                if fmt::en_rango(f.devengo(), &desde, &hasta) && igual(dif, -importe(f)) {
                    v.push(mayuscula(format!("{} cuenta con tus datos y no en lo presentado.", nombre_doc(Doc::Factura(f)))));
                } else if fmt::en_rango(f.devengo(), &sig_ini, &sig_fin) && igual(dif, importe(f)) {
                    v.push(mayuscula(format!("{} es {} según su fecha: ¿se declaró ya en este?", nombre_doc(Doc::Factura(f)), del_trimestre(sy, sq, anio))));
                }
            }
            for k2 in 1..q {
                let (de, ha) = fmt::periodo(anio, k2);
                let s = r2(factura::emitidas(d).filter(|f| fmt::en_rango(f.devengo(), &de, &ha)).map(importe).sum());
                if s > DIF && igual(dif, -s) {
                    v.push(format!("En lo presentado faltan los {} del {k2}T ({}): el 130 va con todo lo del año, desde el 1 de enero.", if k == 1 { "ingresos" } else { "retenciones" }, fmt::eur(s)));
                }
            }
        }
        2 => v.extend(causas_gastos_130(d, anio, q, dif)),
        4 if x.presentado > x.app => v.push("En lo presentado la 04 es más del 20 % de la 03: se puede, las instrucciones dejan adelantar más.".into()),
        5 | 15 => {
            let (casilla, que) = if k == 5 { (7, "los positivos de la casilla 07, menos su 16") } else { (19, "los negativos de la casilla 19 aún no restados") };
            let partes: Vec<String> = (1..q)
                .map(|k2| {
                    if !modelo_130::obligado(d, anio, k2) {
                        return format!("el {k2}T no tocaba");
                    }
                    let c = Trimestral::M130.cuenta(d, anio, k2, memo);
                    let de = if Modelo::M130.leido(d, anio, k2).is_some() { "" } else { " (calculado)" };
                    format!("{k2}T {}{de}", fmt::eur(c.v(casilla)))
                })
                .collect();
            let lista = if partes.is_empty() { "no hay ninguno antes".to_string() } else { partes.join(" · ") };
            v.push(format!("Sale de tus 130 anteriores de {anio} ({que}): {lista}. Con ellos sale {}; en lo presentado, {}.", fmt::eur(x.app), fmt::eur(x.presentado)));
        }
        13 => {
            let rn = modelo_130::rn_anterior(d, anio);
            let de = match rn.origen {
                OrigenRn::Escrito => "lo que has escrito",
                OrigenRn::Renta => "casilla 0224 de tu Renta",
                OrigenRn::Alta => "empezaste la actividad ese año: cuenta como 0",
                OrigenRn::Calculado => "calculado con tus datos",
                OrigenRn::NoSe => "no lo sé",
            };
            let mut t = match rn.valor {
                Some(rn) => format!("Con tu rendimiento de {} ({}, {de}) te toca una minoración de {}; en lo presentado, {}.", anio - 1, fmt::eur(rn), fmt::eur(x.app), fmt::eur(x.presentado)),
                None => format!("No sé cuánto ganaste en {} (casilla 0224 de tu Renta): con eso se sabe la minoración.", anio - 1),
            };
            if x.presentado < x.app {
                t.push_str(" Te la puedes aplicar: es para quien ganó 12.000 € o menos el año anterior (art. 110.3.c RIRPF).");
            }
            v.push(t);
        }
        16 => v.push("La app no calcula la deducción por el préstamo de tu vivienda (casilla 16: compras de antes de 2013).".into()),
        _ => {}
    }
    if v.is_empty() {
        v.push(if dif > 0.0 { format!("En lo presentado hay {} más.", fmt::eur(dif)) } else { format!("Con tus datos hay {} más.", fmt::eur(-dif)) });
    }
    v
}

/// Lo que puede explicar la casilla 02 del 130: unos grupos de gastos (el de difícil justificación, el IVA que no te
/// deduces, tus cuotas de autónomo o tus gastos de cada trimestre) cuya suma da justo la diferencia, o un gasto.
fn causas_gastos_130(d: &Datos, anio: i32, q: u32, dif: f64) -> Vec<String> {
    let desde = format!("{anio}-01-01");
    let hasta = fmt::periodo(anio, q).1;
    let a = irpf::irpf_acumulado(d, anio, &hasta);
    let pct = fmt::pct(irpf::gdj_reglas(anio).0);
    // (lo que cambia la diferencia si en lo presentado no está, qué es)
    let mut grupos: Vec<(f64, String)> = Vec::new();
    if a.gdj > DIF {
        grupos.push(if modelo_130::con_gdj(d) {
            (-a.gdj, format!("el {pct} de difícil justificación ({}): la app lo resta y en lo presentado no", fmt::eur(a.gdj)))
        } else {
            (a.gdj, format!("el {pct} de difícil justificación ({}): en lo presentado está restado; la app lo deja para la Renta, aunque las instrucciones de la casilla 02 dicen que va también aquí", fmt::eur(a.gdj)))
        });
    }
    let del_anio = || d.gastos.iter().filter(|g| fmt::en_rango(&g.fecha, &desde, &hasta)).map(|g| (g, gasto_calc(g, d))).filter(|(_, gc)| !gc.amortizable);
    let iva: Vec<f64> = del_anio().filter(|(g, _)| !g.es_recibo()).map(|(_, gc)| r2(gc.coste - r2(gc.base * gc.p_irpf / 100.0))).filter(|x| *x > DIF).collect();
    if !iva.is_empty() {
        let total = r2(iva.iter().sum());
        let n = iva.len();
        grupos.push((-total, format!("el IVA que no te deduces de {n} gasto{} ({}): con tus datos va sumado al gasto, en lo presentado parece que no", if n == 1 { "" } else { "s" }, fmt::eur(total))));
    }
    for k2 in 1..=q {
        let (de, ha) = fmt::periodo(anio, k2);
        let (mut cuotas, mut otros) = (0.0, 0.0);
        for (g, gc) in del_anio().filter(|(g, _)| fmt::en_rango(&g.fecha, &de, &ha)) {
            if g.es_recibo() {
                cuotas = r2(cuotas + gc.coste);
            } else {
                otros = r2(otros + gc.coste);
            }
        }
        if cuotas.abs() > DIF {
            grupos.push((-cuotas, format!("tus cuotas de autónomo del {k2}T ({})", fmt::eur(cuotas))));
        }
        if otros.abs() > DIF && k2 < q {
            grupos.push((-otros, format!("tus gastos del {k2}T ({})", fmt::eur(otros))));
        }
    }
    // La combinación más corta que da justo la diferencia (son pocos grupos: se prueban todas).
    grupos.truncate(12);
    let mejor = (1u32..1 << grupos.len())
        .filter(|m| igual(dif, (0..grupos.len()).filter(|i| m & (1 << i) != 0).map(|i| grupos[i].0).sum()))
        .min_by_key(|m| m.count_ones());
    let mut v = Vec::new();
    if let Some(m) = mejor {
        let partes: Vec<&str> = (0..grupos.len()).filter(|i| m & (1 << i) != 0).map(|i| grupos[i].1.as_str()).collect();
        v.push(format!("Cuadra al céntimo con {}.", y_lista(&partes)));
    }
    let (sy, sq) = trimestre_siguiente(anio, q);
    let (sig_ini, sig_fin) = fmt::periodo(sy, sq);
    for g in d.gastos.iter().filter(|g| !g.es_recibo()) {
        let coste = gasto_calc(g, d).coste;
        if fmt::en_rango(&g.fecha, &desde, &hasta) && igual(dif, -coste) {
            v.push(mayuscula(format!("{} cuenta con tus datos y no en lo presentado.", nombre_doc(Doc::Gasto(g)))));
        } else if fmt::en_rango(&g.fecha, &sig_ini, &sig_fin) && igual(dif, coste) {
            v.push(mayuscula(format!("{} es {} según su fecha: ¿se declaró ya en este?", nombre_doc(Doc::Gasto(g)), del_trimestre(sy, sq, anio))));
        }
    }
    v
}

/// El 390 presentado de un año (si se ha leído su PDF y se conocen sus cuatro 303) frente al que sale de esos 303.
/// Sus casillas son sumas del año: lo que no cuadra suele ser un 303 que no cuadra, el mismo importe en otra
/// casilla o el reparto por tipos.
pub fn cuadre_390(d: &Datos, anio: i32, calc: &modelo_390::Calculado, memo: &mut Memo) -> Option<Conciliacion> {
    let pres = &Modelo::M390.leido(d, anio, 0).filter(|l| !l.casillas.is_empty() && calc.completo)?.casillas;
    let app = &calc.casillas;
    const SUMAS: [u32; 8] = [33, 34, 47, 64, 65, 84, 86, 108];
    // Las parejas que se confunden: (en lo presentado, con tus 303, por qué).
    const PAREJAS: [(u32, u32, &str); 2] = [
        (104, 110, "Lo facturado a clientes de fuera de la UE: con tus 303 va en la 110 («operaciones no sujetas por reglas de localización»); en lo presentado, en la 104 (exportaciones)."),
        (97, 662, "En la 97 va solo lo que salió a compensar en el último 303 del año; lo que salió a compensar en los anteriores y no se aplicó, en la 662 (instrucciones del 390, apartado 9)."),
    ];
    // De la casilla del 390 a la del 303 que la forma, para buscar el trimestre que falta.
    const DEL_303: [(u32, u32); 13] = [(1, 1), (2, 3), (3, 4), (4, 6), (5, 7), (6, 9), (48, 28), (49, 29), (50, 30), (51, 31), (103, 59), (104, 60), (110, 120)];
    let mut c = Conciliacion { con_datos: true, mismo_resultado: igual(app.v(86), pres.v(86)), ..Default::default() };
    let casillas: BTreeSet<u32> = app.keys().chain(pres.keys()).copied().filter(|&k| !igual(app.v(k), pres.v(k))).collect();
    for k in casillas {
        if SUMAS.contains(&k) {
            c.arrastradas.push(k);
        } else {
            c.diferencias.push(Diferencia { casilla: k, app: app.v(k), presentado: pres.v(k), causas: Vec::new() });
        }
    }
    let difs = c.diferencias.clone();
    let por_tipos: BTreeSet<u32> = modelo_390::por_tipos().collect();
    let trimestres: Vec<Casillas> = (1..=4).map(|q| Trimestral::M303.cuenta(d, anio, q, memo)).collect();
    for x in &mut c.diferencias {
        let dif = x.dif();
        for (en_pres, en_app, porque) in PAREJAS.iter().filter(|p| x.casilla == p.0 || x.casilla == p.1) {
            let otra = if x.casilla == *en_pres { *en_app } else { *en_pres };
            if !difs.iter().any(|y| y.casilla == otra && y.dif() * dif < 0.0) {
                continue;
            }
            let resto = r2(pres.v(*en_pres) + pres.v(*en_app) - app.v(*en_pres) - app.v(*en_app));
            let mut t = porque.to_string();
            if resto.abs() > DIF {
                t.push_str(&format!(" Además, entre las dos, el 390 lleva {} {} que tus 303.", fmt::eur(resto.abs()), if resto > 0.0 { "más" } else { "menos" }));
            }
            x.causas.push(t);
        }
        if x.causas.is_empty() && por_tipos.contains(&x.casilla) && !calc.sin_repartir.is_empty() {
            x.causas.push("Sin tus gastos de algún trimestre, la app no sabe repartir su IVA por tipos: lo deja en el total. Repártelo tú con tus facturas.".into());
        }
        if x.causas.is_empty() {
            let mut t = format!("La suma de tus 303 del año da {}; en el 390 presentado, {}.", fmt::eur(x.app), fmt::eur(x.presentado));
            if let Some(&(_, k303)) = DEL_303.iter().find(|p| p.0 == x.casilla)
                && let Some(q) = (1..=4).find(|&q| igual(trimestres[q as usize - 1].v(k303), -dif))
            {
                t.push_str(&format!(" Es justo lo de la casilla {} de tu 303 del {q}T: parece que en el 390 falta ese trimestre.", casillas_num(k303)));
            }
            x.causas.push(t);
        }
    }
    Some(c)
}

/* ================= otras declaraciones del año: 349 y 347 ================= */

/// Si con tus facturas y gastos del año toca el 349 o el 347, en una frase cada uno.
/// - 349 (declaración recapitulativa de operaciones intracomunitarias, art. 78-81 RIVA): si vendes a empresas de
///   la UE o les compras. Trimestral, o mensual si en un trimestre pasas de 50.000 €.
/// - 347 (operaciones con terceras personas, art. 31-35 RGAT): con quien en España pasas de 3.005,06 € en el año,
///   IVA incluido, sin contar lo que lleva retención, lo del 349 ni las exportaciones. En febrero.
pub fn informativas(d: &Datos, anio: i32) -> Vec<String> {
    let (desde, hasta) = (format!("{anio}-01-01"), format!("{anio}-12-31"));
    if !hay_datos(d, &desde, &hasta) {
        return vec![format!("Modelos 349 y 347: no tengo tus facturas ni tus gastos de {anio} para saber si te tocan.")];
    }
    let del_anio = |fecha: &str| fmt::en_rango(fecha, &desde, &hasta);
    let mut v = Vec::new();
    let ventas_ue: Vec<&Factura> = factura::emitidas(d).filter(|f| f.operacion == "ue" && del_anio(f.devengo())).collect();
    let compras_ue: Vec<&gasto::Gasto> = gasto::apuntados(d).filter(|g| g.origen == "ue" && del_anio(&g.fecha)).collect();
    if ventas_ue.is_empty() && compras_ue.is_empty() {
        v.push("Modelo 349 (operaciones con empresas de la UE): no te toca, no tienes ninguna en el año.".into());
    } else {
        let trimestres: BTreeSet<u32> = ventas_ue.iter().map(|f| fmt::trimestre_de(f.devengo())).chain(compras_ue.iter().map(|g| fmt::trimestre_de(&g.fecha))).collect();
        let lista: Vec<String> = trimestres.iter().map(|q| format!("{q}T")).collect();
        let lista: Vec<&str> = lista.iter().map(String::as_str).collect();
        let cuantos = |n: usize, uno: &str, varios: &str| if n == 1 { format!("1 {uno}") } else { format!("{n} {varios}") };
        let partes: Vec<String> = [
            (!ventas_ue.is_empty()).then(|| cuantos(ventas_ue.len(), "factura a una empresa de la UE", "facturas a empresas de la UE")),
            (!compras_ue.is_empty()).then(|| cuantos(compras_ue.len(), "gasto de una empresa de la UE", "gastos de empresas de la UE")),
        ]
        .into_iter()
        .flatten()
        .collect();
        v.push(format!(
            "Modelo 349 (operaciones con empresas de la UE): te toca en el {} por {}. Es trimestral (mensual si en un trimestre pasas de 50.000 €): del 1 al 20 del mes siguiente; el del 4T, hasta el 30 de enero.",
            y_lista(&lista),
            partes.join(" y ")
        ));
    }
    // Por cliente y por proveedor de España, lo del año con IVA (sin las facturas con retención).
    let mut clientes: BTreeMap<String, f64> = BTreeMap::new();
    for f in factura::emitidas(d).filter(|f| f.operacion == "nacional" && del_anio(f.devengo())) {
        let t = factura::totales_factura(f);
        if t.retencion <= DIF {
            let quien = if f.cliente.nif.is_empty() { &f.cliente.nombre } else { &f.cliente.nif };
            *clientes.entry(quien.to_uppercase()).or_default() += t.importe_total;
        }
    }
    let mut proveedores: BTreeMap<String, f64> = BTreeMap::new();
    for g in gasto::apuntados(d).filter(|g| g.origen == "es" && del_anio(&g.fecha)) {
        let quien = if g.nif.is_empty() { &g.proveedor } else { &g.nif };
        *proveedores.entry(quien.to_uppercase()).or_default() += gasto_calc(g, d).pagado;
    }
    let pasan = |m: &BTreeMap<String, f64>| m.iter().filter(|(k, x)| !k.is_empty() && **x > 3005.06).count();
    let (nc, np) = (pasan(&clientes), pasan(&proveedores));
    v.push(if nc + np == 0 {
        "Modelo 347 (operaciones con terceros): no te toca; con nadie de España pasas de 3.005,06 € en el año (IVA incluido, sin contar lo que lleva retención).".into()
    } else {
        let quienes: Vec<String> = [(nc, "cliente", "clientes"), (np, "proveedor", "proveedores")].into_iter().filter(|x| x.0 > 0).map(|(n, uno, varios)| if n == 1 { format!("1 {uno}") } else { format!("{n} {varios}") }).collect();
        format!(
            "Modelo 347 (operaciones con terceros): parece que te toca, en febrero de {}: con {} de España pasas de 3.005,06 € en el año, IVA incluido. No cuenta lo que lleva retención (si le retienes tú, tampoco), lo del 349 ni lo de fuera de España.",
            anio + 1,
            quienes.join(" y ")
        )
    });
    v
}

/// De dónde sale cada casilla de un modelo calculado con tus datos, en pocas palabras («3 facturas», «lo que dejó el
/// 2T», «rendimiento de 2025: 8.000,00 €»). Solo las que tienen algo.
pub fn de_donde(d: &Datos, modelo: Trimestral, anio: i32, q: u32, c: &Casillas) -> BTreeMap<u32, String> {
    let mut v = BTreeMap::new();
    let cuantos = |n: usize, uno: &str, varios: &str| if n == 1 { format!("1 {uno}") } else { format!("{n} {varios}") };
    match modelo {
        Trimestral::M303 => {
            let (desde, hasta) = fmt::periodo(anio, q);
            let mut n: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
            for a in modelo_303::apuntes(d, &desde, &hasta) {
                for k in [a.kb, a.kc].into_iter().filter(|&k| k > 0) {
                    let e = n.entry(k).or_default();
                    match a.doc {
                        Doc::Factura(_) => e.0 += 1,
                        Doc::Gasto(_) => e.1 += 1,
                    }
                }
            }
            for (k, (f, g)) in n {
                let partes: Vec<String> = [(f > 0).then(|| cuantos(f, "factura", "facturas")), (g > 0).then(|| cuantos(g, "gasto", "gastos"))].into_iter().flatten().collect();
                v.insert(k, partes.join(" y "));
            }
            if c.v(110) > DIF {
                let (py, pq) = modelo_303::trimestre_anterior(anio, q);
                v.insert(110, format!("lo que dejó el 303 {}", del_trimestre(py, pq, anio)));
            }
        }
        Trimestral::M130 => {
            let desde = format!("{anio}-01-01");
            let hasta = fmt::periodo(anio, q).1;
            let facturas: Vec<_> = factura::emitidas(d).filter(|f| fmt::en_rango(f.devengo(), &desde, &hasta)).map(factura::totales_factura).collect();
            if !facturas.is_empty() {
                v.insert(1, format!("{} desde el 1 de enero", cuantos(facturas.len(), "factura", "facturas")));
            }
            let con_ret = facturas.iter().filter(|t| t.retencion > DIF).count();
            if con_ret > 0 {
                v.insert(6, format!("de {}", cuantos(con_ret, "factura", "facturas")));
            }
            let del_anio = || d.gastos.iter().filter(|g| fmt::en_rango(&g.fecha, &desde, &hasta));
            let (ng, nc) = (del_anio().filter(|g| !g.es_recibo()).count(), del_anio().filter(|g| g.es_recibo()).count());
            let mut partes: Vec<String> = Vec::new();
            if ng > 0 {
                partes.push(cuantos(ng, "gasto", "gastos"));
            }
            if nc > 0 {
                partes.push(cuantos(nc, "recibo de autónomo", "recibos de autónomo"));
            }
            if modelo_130::con_gdj(d) && irpf::irpf_acumulado(d, anio, &hasta).gdj > DIF {
                partes.push(format!("el {} de difícil justificación", fmt::pct(irpf::gdj_reglas(anio).0)));
            }
            if !partes.is_empty() {
                let partes: Vec<&str> = partes.iter().map(String::as_str).collect();
                v.insert(2, y_lista(&partes));
            }
            if c.v(5) > DIF {
                v.insert(5, "las 07 de los 130 anteriores del año".into());
            }
            let rn = modelo_130::rn_anterior(d, anio);
            if let (Some(x), true) = (rn.valor, c.v(13) > DIF) {
                v.insert(13, if rn.origen == OrigenRn::Alta { format!("empezaste en {anio}") } else { format!("ganaste {} en {}", fmt::eur(x), anio - 1) });
            }
            if c.v(15) > DIF {
                v.insert(15, "las 19 negativas de antes".into());
            }
        }
    }
    v
}

/// «a», «a y b», «a, b y c».
fn y_lista(v: &[&str]) -> String {
    match v {
        [] => String::new(),
        [a] => a.to_string(),
        [resto @ .., ultimo] => format!("{} y {ultimo}", resto.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dominio::hacienda::Tipo;
    use crate::dominio::renta::{Actividad, Renta};
    use crate::dominio::tgss::Recibo;
    use crate::dominio::{FormularioLeido, Gasto, Linea, Num};

    fn factura(numero: &str, fecha: &str, operacion: &str, precio: f64) -> Factura {
        let lineas = vec![Linea { concepto: "Servicio".into(), cantidad: Num::de(1.0), precio: Num::de(precio), iva: Num::de(21.0) }];
        Factura { estado: "emitida".into(), numero: numero.into(), fecha: fecha.into(), operacion: operacion.into(), lineas, ..Default::default() }
    }

    fn gasto(concepto: &str, fecha: &str, base: f64) -> Gasto {
        Gasto { concepto: concepto.into(), categoria: "software".into(), origen: "es".into(), base: Num::de(base), iva_tipo: Num::de(21.0), fecha: fecha.into(), ..Default::default() }
    }

    fn presentado(d: &mut Datos, anio: i32, q: u32, modelo: &str, c: &[(u32, f64)], tipo: Tipo) {
        let l = FormularioLeido { casillas: c.iter().copied().collect(), tipo: Some(tipo), ..Default::default() };
        d.lecturas.formularios.insert(crate::hacienda::clave(anio, q, modelo), l);
    }

    /// El 303 de un trimestre con una factura a un cliente de fuera de la UE: en la 120 con tus datos y en la 60 en
    /// lo presentado (mismo resultado); con otra factura en el trimestre siguiente que se declaró en este.
    #[test]
    fn el_303_con_las_facturas_en_otra_casilla_o_trimestre() {
        let facturas = vec![factura("A-1", "2026-08-10", "extra", 5000.0), factura("A-2", "2026-10-02", "nacional", 100.0)];
        let mut d = Datos { facturas, gastos: vec![gasto("Hosting", "2026-08-01", 100.0)], ..Default::default() };
        presentado(&mut d, 2026, 3, "303", &[(9, 21.0), (7, 100.0), (27, 21.0), (28, 100.0), (29, 21.0), (45, 21.0), (60, 5000.0)], Tipo::Cero);
        let c = cuadre(&d, Trimestral::M303, 2026, 3, &mut Memo::default()).unwrap();
        assert!(c.con_datos && !c.mismo_resultado);
        let casillas: Vec<u32> = c.diferencias.iter().map(|x| x.casilla).collect();
        assert_eq!(casillas, [7, 9, 60, 120]);
        assert!(c.diferencias[0].causas[0].starts_with("La factura A-2 (2 oct 2026) es del 4T"), "{:?}", c.diferencias[0].causas);
        assert!(c.diferencias[2].causas[0].contains("van en la 120"));
        assert!(c.arrastradas.contains(&27) && c.arrastradas.contains(&71));
    }

    /// El 110 que no recoge todo lo que dejó el 303 anterior; sin facturas ni gastos, solo se compara eso.
    #[test]
    fn lo_pendiente_de_compensar() {
        let mut d = Datos::default();
        presentado(&mut d, 2024, 3, "303", &[(110, 100.0), (87, 100.0), (66, -200.0), (69, -200.0), (71, -200.0)], Tipo::Compensar);
        presentado(&mut d, 2024, 4, "303", &[(110, 200.0), (87, 200.0), (66, -50.0), (69, -50.0), (71, -50.0)], Tipo::Compensar);
        let c = cuadre(&d, Trimestral::M303, 2024, 4, &mut Memo::default()).unwrap();
        assert!(!c.con_datos && c.mismo_resultado && c.arrastradas.is_empty());
        assert_eq!((c.diferencias[0].casilla, c.diferencias[0].app, c.diferencias[0].presentado), (110, 300.0, 200.0));
        assert!(c.diferencias[0].causas[0].contains("solo lo que salió a compensar (200,00 €)"), "{:?}", c.diferencias[0].causas);
    }

    /// El 130 en el que lo presentado no resta el IVA que no te deduces ni las cuotas del 1T, ni la minoración.
    #[test]
    fn el_130_con_gastos_agrupados() {
        let mut d = Datos::default();
        d.fiscal.gdj = false;
        // De alta en julio: el 1T y el 2T no tocaba presentarlos (no dejan negativos para la 15).
        d.perfil.fecha_alta = "2026-07-01".into();
        d.facturas = vec![factura("A-1", "2026-08-10", "extra", 10000.0)];
        d.gastos = vec![Gasto { iva_no_ded: true, ..gasto("Tique", "2026-08-01", 10.0) }, gasto("Hosting", "2026-08-02", 100.0)];
        d.tgss.recibos = vec![Recibo { periodo: "2026-02".into(), concepto: "Cuota".into(), importe: 300.0, cargo: "2026-02-27".into(), ..Default::default() }];
        crate::dominio::gasto::con_recibos(&mut d);
        d.lecturas.rentas.insert(2025, Renta { actividad: Some(Actividad { neto: 8000.0, ..Default::default() }), ..Default::default() });
        // Lo presentado: 110 € de gastos (sin el IVA del tique ni la cuota de febrero) y sin minoración.
        presentado(&mut d, 2026, 3, "130", &[(1, 10000.0), (2, 110.0), (3, 9890.0), (4, 1978.0), (7, 1978.0), (12, 1978.0), (14, 1978.0), (17, 1978.0), (19, 1978.0)], Tipo::Ingreso);
        let c = cuadre(&d, Trimestral::M130, 2026, 3, &mut Memo::default()).unwrap();
        let casillas: Vec<u32> = c.diferencias.iter().map(|x| x.casilla).collect();
        assert_eq!(casillas, [2, 13]);
        let t = &c.diferencias[0].causas[0];
        assert!(t.starts_with("Cuadra al céntimo con el IVA que no te deduces de 1 gasto (2,10 €)") && t.contains("tus cuotas de autónomo del 1T (300,00 €)"), "{t}");
        assert!(c.diferencias[1].causas[0].contains("casilla 0224 de tu Renta") && c.diferencias[1].causas[0].contains("Te la puedes aplicar"));
        assert_eq!(c.arrastradas, [3, 4, 7, 12, 14, 17, 19]);
    }

    /// El 349 si hay operaciones con la UE; el 347, con quien en España pasa de 3.005,06 € (sin retención).
    #[test]
    fn el_349_y_el_347() {
        let con_cliente = |f: Factura, nif: &str| Factura { cliente: crate::dominio::Cliente { nif: nif.into(), ..Default::default() }, ..f };
        let mut d = Datos { facturas: vec![con_cliente(factura("A-1", "2026-03-10", "nacional", 2000.0), "B1")], ..Default::default() };
        let t = informativas(&d, 2026);
        assert!(t[0].contains("no te toca") && t[1].contains("no te toca"), "{t:?}");
        // Con 3.630 € con IVA a un mismo cliente, el 347; con una factura a la UE en el 2T, el 349.
        d.facturas.push(con_cliente(factura("A-2", "2026-04-10", "nacional", 1000.0), "B1"));
        d.facturas.push(factura("A-3", "2026-05-10", "ue", 500.0));
        let t = informativas(&d, 2026);
        assert!(t[0].contains("te toca en el 2T por 1 factura a una empresa de la UE"), "{t:?}");
        assert!(t[1].contains("con 1 cliente de España") && t[1].contains("febrero de 2027"), "{t:?}");
        // Con retención no cuenta.
        d.facturas.iter_mut().for_each(|f| f.retencion = Num::de(15.0));
        assert!(informativas(&d, 2026)[1].contains("no te toca"));
    }

    /// Lo presentado frente a lo apuntado en una carpeta de datos, trimestre a trimestre (trimestre, casilla y causa):
    /// `DATOS=mi/carpeta cargo test conciliar_carpeta -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn conciliar_carpeta() {
        let dir = std::path::PathBuf::from(std::env::var("DATOS").expect("DATOS=carpeta"));
        crate::hacienda::lectura::renombrar_antiguos(&dir);
        let mut d = crate::dominio::almacen::cargar(&dir).unwrap();
        d.lecturas = crate::hacienda::lectura::leer_todo(&dir, &mut |_, _| {});
        let mut memo = Memo::default();
        for anio in 2020..=fmt::anio_actual() {
            for q in 1..=4 {
                for m in Trimestral::TODOS {
                    let Some(c) = cuadre(&d, m, anio, q, &mut memo) else { continue };
                    let estado = if c.cuadra() { "cuadra" } else if c.mismo_resultado { "mismo resultado" } else { "NO CUADRA" };
                    println!("{q}T {anio} {} · {} · {estado}", m.modelo(), if c.con_datos { "con datos" } else { "sin datos: solo lo encadenado" });
                    for x in &c.diferencias {
                        println!("   casilla {}: {}", casillas_num(x.casilla), x.causas.join(" | "));
                    }
                    if !c.arrastradas.is_empty() {
                        println!("   y por ellas: {:?}", c.arrastradas);
                    }
                }
            }
            let calc = modelo_390::calcular(&d, anio, &mut memo);
            if let Some(c) = cuadre_390(&d, anio, &calc, &mut memo) {
                let estado = if c.cuadra() { "cuadra" } else if c.mismo_resultado { "mismo resultado" } else { "NO CUADRA" };
                println!("{anio} 390 · {estado} · sin repartir por tipos: {:?}", calc.sin_repartir);
                for x in &c.diferencias {
                    println!("   casilla {}: {}", casillas_num(x.casilla), x.causas.join(" | "));
                }
                if !c.arrastradas.is_empty() {
                    println!("   y por ellas: {:?}", c.arrastradas);
                }
            }
        }
    }
}
