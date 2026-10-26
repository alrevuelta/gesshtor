//! La actividad como autónomo de un año: las cuotas (recibos de la Seguridad Social por conceptos, meses sin
//! recibo, regularización) y lo facturado y gastado: de la app si hay facturas o gastos de ese año; si no, de la
//! Renta.
use crate::dominio::fmt::DIF;
use super::{editable, renta_de};
use crate::dominio::Datos;
use crate::dominio::tgss::{Conceptos, Recibo, Regularizacion, Resultado};
use crate::dominio::factura;
use crate::hacienda::irpf;
use crate::dominio::fmt::{self, r2};


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
    factura::emitidas(d).any(|f| fmt::anio_de(f.devengo()) == y) || d.gastos.iter().any(|g| fmt::anio_de(&g.fecha) == y)
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
