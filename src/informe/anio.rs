//! El año en cifras: lo que ganas, lo que pagas y lo que te queda, con de dónde sale cada cifra, y la serie año a
//! año.
//!
//! - Con la Renta del año, casi todo sale de ella; de la Seguridad Social, tu parte de la nómina, la de tu empresa
//!   y tu cuota de autónomo.
//! - Sin Renta (el año en curso): tus nóminas apuntadas, tus facturas y gastos, tus 130 y la Seguridad Social.
//!
//! Lo que ganas es antes de impuestos y cotizaciones. De la actividad cuenta lo facturado menos los gastos, sin la
//! cuota de autónomo (va en lo que pagas) ni los de difícil justificación; los alquileres, sin su reducción (solo
//! es fiscal) y sin la renta imputada de los inmuebles vacíos (no es dinero que cobres).
use crate::dominio::fmt::DIF;
use super::empleo::{self, NominaAnio};
use super::renta_de;
use crate::dominio::PRIMER_ANIO;
use crate::dominio::hacienda::Tipo;
use crate::dominio::renta::Renta;
use crate::dominio::{gasto, Datos};
use crate::hacienda::modelo_100::numeracion;
use crate::hacienda::{irpf, Modelo};
use crate::dominio::fmt::{self, eur, r2};
use crate::hacienda;


/* ================= conceptos ================= */

/// Lo que se cuenta, en el orden en que se enseña (y se apila en el gráfico).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Concepto {
    Sueldo,
    Actividad,
    Capital,
    Ganancias,
    OtrasRentas,
    Irpf,
    TuNomina,
    Cuota,
}

impl Concepto {
    pub const GANAS: [Concepto; 5] = [Concepto::Sueldo, Concepto::Actividad, Concepto::Capital, Concepto::Ganancias, Concepto::OtrasRentas];
    pub const PAGAS: [Concepto; 3] = [Concepto::Irpf, Concepto::TuNomina, Concepto::Cuota];

    pub fn nombre(self) -> &'static str {
        match self {
            Concepto::Sueldo => "Sueldo",
            Concepto::Actividad => "Como autónomo",
            Concepto::Capital => "Intereses y dividendos",
            Concepto::Ganancias => "Ganancias patrimoniales",
            Concepto::OtrasRentas => "Alquileres y otras rentas",
            Concepto::Irpf => "Hacienda: IRPF",
            Concepto::TuNomina => "Seguridad Social: tu parte de la nómina",
            Concepto::Cuota => "Seguridad Social: tu cuota de autónomo",
        }
    }

    /// Para leyendas y columnas estrechas.
    pub fn corto(self) -> &'static str {
        match self {
            Concepto::Sueldo => "Sueldo",
            Concepto::Actividad => "Autónomo",
            Concepto::Capital => "Intereses",
            Concepto::Ganancias => "Ganancias",
            Concepto::OtrasRentas => "Alquileres",
            Concepto::Irpf => "IRPF",
            Concepto::TuNomina => "SS nómina",
            Concepto::Cuota => "Cuota autónomo",
        }
    }
}

/// Lo que pagó tu empresa por ti, por grupos.
pub const EMPRESA: [&str; 4] = ["Pensiones y bajas", "Desempleo, FOGASA y formación", "Accidentes de trabajo y MEI", "Cotización de solidaridad"];

/* ================= de dónde sale cada cifra ================= */

#[derive(Debug, Clone, PartialEq)]
pub enum Fuente {
    /// Casillas de tu Renta de ese año.
    Renta(i32, Vec<u32>),
    /// En palabras: «tus nóminas apuntadas», «130 3T · 312,00 €»…
    Texto(String),
}

impl Fuente {
    /// «Renta 2023 · 0180 0223 0235»
    pub fn texto(&self) -> String {
        match self {
            Fuente::Renta(y, c) if c.is_empty() => format!("Renta {y}"),
            Fuente::Renta(y, c) => format!("Renta {y} · {}", c.iter().map(|n| format!("{n:04}")).collect::<Vec<_>>().join(" ")),
            Fuente::Texto(t) => t.clone(),
        }
    }
}

/// De dónde sale una cifra: si consta (la Renta, Hacienda o la Seguridad Social), si la has apuntado tú o si es
/// una estimación.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origen {
    #[default]
    Consta,
    Apuntado,
    Estimado,
}

/// Una cifra explicada: qué es, cuánto, una nota corta y de dónde sale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fila {
    pub nombre: String,
    /// None: no se sabe.
    pub importe: Option<f64>,
    pub nota: String,
    pub fuentes: Vec<Fuente>,
    /// Se enseña, pero no suma (la renta imputada de tus inmuebles).
    pub aparte: bool,
    /// El concepto al que suma (para su color), si es uno.
    pub concepto: Option<Concepto>,
    pub origen: Origen,
}

fn fila(nombre: impl Into<String>, importe: Option<f64>, nota: impl Into<String>, fuentes: Vec<Fuente>) -> Fila {
    Fila { nombre: nombre.into(), importe, nota: nota.into(), fuentes, ..Default::default() }
}

impl Fila {
    fn de(mut self, c: Concepto) -> Fila {
        self.concepto = Some(c);
        self
    }
    fn origen(mut self, o: Origen) -> Fila {
        self.origen = o;
        self
    }
}

fn texto(t: impl Into<String>) -> Vec<Fuente> {
    vec![Fuente::Texto(t.into())]
}

/// «70,93 %» (con dos decimales como mucho), o «—» si no hay base.
pub fn pct(a: f64, b: f64) -> String {
    if b > 0.0 { fmt::pct(r2(a / b * 100.0)) } else { "—".into() }
}

/// Con el signo menos de verdad: «−456,78 €».
pub fn euro(x: f64) -> String {
    if x < -DIF { format!("−{}", eur(-x)) } else { eur(x) }
}

/* ================= casillas de la Renta ================= */

/// De dónde sale una cifra: estas casillas de la Renta de ese año (las que se citan al lado de cada cifra).
fn de_renta(y: i32, casillas: &[u32]) -> Vec<Fuente> {
    vec![Fuente::Renta(y, casillas.to_vec())]
}

/* ================= IRPF ================= */

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Irpf {
    /// El IRPF del año: la cuota de la Renta o, sin ella, lo adelantado.
    pub total: f64,
    /// Lo retenido en tu nómina (None: no se sabe).
    pub nomina: Option<f64>,
    /// Cómo se pagó (con Renta) o lo adelantado hasta ahora (sin ella).
    pub filas: Vec<Fila>,
    /// El IRPF que te tocó, por partes (solo con la Renta de la numeración actual).
    pub toco: Vec<Fila>,
    /// De dónde sale el total.
    pub fuente: Vec<Fuente>,
    pub con_renta: bool,
    /// Tiene nómina según la Seguridad Social, pero no hay nóminas apuntadas: falta lo retenido.
    pub falta_nomina: bool,
}

/// Los 130 presentados del año: (trimestre, lo que salió por el trimestre entero; None si no se sabe con qué
/// casillas).
pub fn presentados130(d: &Datos, y: i32) -> Vec<(u32, Option<f64>)> {
    (1..=4)
        .filter(|&q| hacienda::consta(d, y, q, Modelo::M130))
        .map(|q| {
            let c = Modelo::M130.leido(d, y, q).map(|l| &l.casillas);
            let importe = c.map(crate::hacienda::modelo_130::resultado_del_trimestre);
            (q, importe)
        })
        .collect()
}

fn fuentes130(v: &[(u32, Option<f64>)]) -> Vec<Fuente> {
    v.iter().map(|(q, i)| Fuente::Texto(match i {
        Some(x) => format!("130 {q}T · {}", euro(*x)),
        None => format!("130 {q}T"),
    }))
    .collect()
}

/// Las nóminas apuntadas de un año.
fn nominas(d: &Datos, y: i32) -> Vec<&crate::dominio::Nomina> {
    d.nominas.iter().filter(|n| n.anio == y).collect()
}

/// «hasta la nómina de octubre»
fn hasta_nomina(l: &[&crate::dominio::Nomina]) -> String {
    let m = l.iter().map(|n| if n.mes == 0 { 12 } else { n.mes }).max().unwrap_or(12).clamp(1, 12);
    format!("hasta la nómina de {}", fmt::MESES[m as usize - 1])
}

/// El IRPF de un año (`irpfAnio`): con la Renta, su cuota y cómo se pagó; sin ella, lo adelantado (lo retenido
/// en tus nóminas apuntadas, tus 130 y lo que te retienen tus clientes en las facturas).
pub fn irpf_anio(d: &Datos, y: i32, n: Option<&NominaAnio>) -> Irpf {
    if let Some(r) = renta_de(d, y) {
        return irpf_renta(d, r, y);
    }
    let p130 = presentados130(d, y);
    let fracc = r2(p130.iter().filter_map(|x| x.1).filter(|x| *x > DIF).sum());
    let hay = !p130.is_empty() || (1..=4).any(|q| hacienda::consta(d, y, q, Modelo::M303));
    let ret = irpf::irpf_acumulado(d, y, &format!("{y}-12-31")).retenciones;
    let l = nominas(d, y);
    let con_nomina = n.is_some_and(|n| n.con_nomina > 0);
    let mut filas = Vec::new();
    let mut nomina = None;
    if !l.is_empty() {
        let x = r2(l.iter().map(|n| n.retenciones).sum());
        nomina = Some(x);
        filas.push(fila("Retenido en tu nómina", Some(x), hasta_nomina(&l), texto("tus nóminas apuntadas")).origen(Origen::Apuntado));
    } else if con_nomina {
        filas.push(fila("Retenido en tu nómina", None, "solo lo dicen tus nóminas: Hacienda no lo tiene hasta que tu empresa lo declara, en enero", vec![]).origen(Origen::Apuntado));
    }
    if fracc.abs() > DIF || hay {
        filas.push(fila("Pagos fraccionados (modelo 130)", Some(fracc), "", fuentes130(&p130)));
    }
    if ret.abs() > DIF {
        filas.push(fila("Retenido por tus clientes", Some(ret), "en tus facturas", texto("tus facturas")).origen(Origen::Apuntado));
    }
    Irpf {
        total: r2(nomina.unwrap_or(0.0) + fracc + ret),
        nomina,
        filas,
        toco: Vec::new(),
        fuente: Vec::new(),
        con_renta: false,
        falta_nomina: l.is_empty() && con_nomina,
    }
}

fn irpf_renta(d: &Datos, r: &Renta, y: i32) -> Irpf {
    let cas = numeracion(&r.numeracion);
    let de = |k: &[u32]| de_renta(y, k);
    let nomina = match (&r.trabajo, &r.pagos) {
        (Some(t), _) => r2(t.retenciones.unwrap_or(0.0)),
        (None, Some(p)) => p.trabajo,
        _ => 0.0,
    };
    let con130 = |mut v: Vec<Fuente>| {
        v.extend(fuentes130(&presentados130(d, y)));
        v
    };
    let mut filas = Vec::new();
    if let Some(p) = &r.pagos {
        if p.trabajo.abs() > DIF {
            filas.push(fila("Retenido en tu nómina", Some(p.trabajo), "", de(&[cas.ret_trabajo])));
        }
        if p.capital.abs() > DIF {
            filas.push(fila("Retenido por tu banco", Some(p.capital), "en tus intereses y dividendos", de(&[cas.ret_capital])));
        }
        if p.actividades.abs() > DIF {
            filas.push(fila("Retenido por tus clientes", Some(p.actividades), "en tus facturas", de(&[cas.ret_actividades])));
        }
        if p.fraccionados.abs() > DIF {
            filas.push(fila("Pagos fraccionados (modelo 130)", Some(p.fraccionados), "", con130(de(&[cas.fraccionados]))));
        }
        let otros = r2(p.total - p.trabajo - p.capital - p.actividades - p.fraccionados);
        if otros.abs() > DIF {
            filas.push(fila("Otros pagos a cuenta", Some(otros), "", de(&[cas.pagos])));
        }
    } else {
        // Leída con un lector anterior: lo que hay.
        let fracc = r2(r.fraccionados.unwrap_or(0.0));
        let otras = r2(r.pagos_cuenta - nomina - fracc);
        if nomina.abs() > DIF {
            filas.push(fila("Retenido en tu nómina", Some(nomina), "", de(&[cas.ret_trabajo])));
        }
        if fracc.abs() > DIF {
            filas.push(fila("Pagos fraccionados (modelo 130)", Some(fracc), "", con130(de(&[cas.fraccionados]))));
        }
        if otras.abs() > DIF {
            filas.push(fila("Otras retenciones y pagos a cuenta", Some(otras), "de tu banco, tus clientes…", de(&[cas.pagos])));
        }
    }
    let nota = match r.plazos.first() {
        Some(p) if r.forma == Some(Tipo::Domiciliado) && !p.fecha.is_empty() => format!("domiciliado, cargado el {}", fmt::dmy(&p.fecha)),
        _ => String::new(),
    };
    let que = if r.resultado < -DIF { "Te devolvieron al presentar la Renta" } else { "Pagado al presentar la Renta" };
    filas.push(fila(que, Some(r.resultado), nota, de(&[cas.resultado])));

    // El IRPF que te tocó, por la base general y por el ahorro.
    let mut toco = Vec::new();
    if let Some(q) = &r.cuotas {
        let cas = |c: &[u32]| vec![Fuente::Renta(y, c.to_vec())];
        let (ded, ajuste) = (r2(q.integra - q.liquida), r2(r.cuota - q.liquida));
        let nota = format!("base general {} · tipo medio {}", eur(q.liquidable_general), pct(q.general, q.liquidable_general));
        toco.push(fila("Por tu sueldo, tu actividad y demás", Some(q.general), nota, cas(&[505, 532, 533])));
        if q.ahorro.abs() > DIF || q.liquidable_ahorro.abs() > DIF {
            let nota = format!("base del ahorro {} · tipo medio {}", eur(q.liquidable_ahorro), pct(q.ahorro, q.liquidable_ahorro));
            toco.push(fila("Por tu ahorro", Some(q.ahorro), nota, cas(&[510, 540, 541])));
        }
        if ded > DIF {
            toco.push(fila("Deducciones", Some(-ded), "", cas(&[545, 546, 570, 571])));
        }
        if ajuste.abs() > DIF {
            toco.push(fila("Otros ajustes", Some(ajuste), "incrementos o deducción por doble imposición", cas(&[587, 595])));
        }
    }
    Irpf { total: r2(r.cuota), nomina: Some(nomina), filas, toco, fuente: de(&[cas.cuota]), con_renta: true, falta_nomina: false }
}

/* ================= lo que ganas ================= */

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ganado {
    pub total: f64,
    /// Por concepto (solo los que hay).
    pub partes: Vec<(Concepto, f64)>,
    /// Del sueldo, lo que fue en especie.
    pub especie: f64,
    /// El sueldo, aproximado: lo cotizado según la Seguridad Social (no hay nóminas apuntadas).
    pub aprox: bool,
    /// Sin Renta: sin intereses, dividendos ni ganancias.
    pub sin_ahorro: bool,
    /// De la actividad: (facturado, gastos sin la cuota de autónomo ni los de difícil justificación).
    pub actividad: Option<(f64, f64)>,
    /// Lo de arriba explicado, fila a fila, con sus fuentes.
    pub filas: Vec<Fila>,
}

/// «facturado 23.500,00 € − 11.011,26 € de gastos»
pub fn bruto_actividad(facturado: f64, gastos: f64) -> String {
    if gastos > DIF { format!("facturado {} − {} de gastos", eur(facturado), eur(gastos)) } else { format!("facturado {}", eur(facturado)) }
}

/// Lo que ganaste en el año antes de impuestos y cotizaciones, sin lo que paga tu empresa por ti (`ganadoAnio`).
/// Con la Renta sale de ella; si no, de tus facturas y gastos y de tu nómina (apuntada o, aproximada, lo cotizado).
pub fn ganado_anio(d: &Datos, y: i32, n: Option<&NominaAnio>, r: Option<&Renta>) -> Ganado {
    match r {
        Some(r) => ganado_renta(r, y),
        None => ganado_sin_renta(d, y, n),
    }
}

fn ganado_renta(r: &Renta, y: i32) -> Ganado {
    let cas = numeracion(&r.numeracion);
    let de = |k: &[u32]| de_renta(y, k);
    let mut g = Ganado::default();
    let t = r.trabajo.as_ref();
    let a = r.actividad.as_ref();
    let i = r.inmuebles.as_ref();
    let t_cuenta = t.map_or(0.0, |t| t.neto_reducido.or(t.neto).unwrap_or(0.0));
    let a_cuenta = a.map_or(0.0, |a| a.neto_reducido.unwrap_or(a.neto));
    if let Some(t) = t {
        g.partes.push((Concepto::Sueldo, t.integros));
        g.especie = t.especie;
        let especie = if t.especie.abs() > DIF { format!("{} en especie · ", eur(t.especie)) } else { String::new() };
        let nota = format!("{especie}cuentan {}, tras tu Seguridad Social y los gastos fijos", eur(t_cuenta));
        g.filas.push(fila("Sueldo", Some(t.integros), nota, de(&[cas.trabajo.integros, cas.trabajo.reducido])).de(Concepto::Sueldo));
    }
    if let Some(a) = a {
        let gastos = r2(a.gastos - a.ss_titular.unwrap_or(0.0) - a.gdj.unwrap_or(0.0));
        let neto = r2(a.ingresos - gastos);
        g.actividad = Some((a.ingresos, gastos));
        g.partes.push((Concepto::Actividad, neto));
        let gdj = if a.gdj.is_some_and(|x| x.abs() > DIF) { " y los gastos de difícil justificación" } else { "" };
        let reduccion = match a.reduccion {
            Some(x) if x.abs() > DIF => " y la reducción por inicio de actividad",
            _ => "",
        };
        let nota = format!("{} · en la Renta cuentan {}, descontadas también tu cuota de autónomo{gdj}{reduccion}", bruto_actividad(a.ingresos, gastos), eur(a_cuenta));
        g.filas.push(fila("Como autónomo", Some(neto), nota, de(&[cas.actividad.ingresos, cas.actividad.gastos[0], cas.actividad.reducido])).de(Concepto::Actividad));
    }
    // Alquileres y demás: lo que queda de la base general; los alquileres, sin su reducción y sin la renta imputada.
    let general = r.cuotas.as_ref().map(|q| q.base_general).or(r.bases.as_ref().map(|b| b.general));
    if let Some(general) = general {
        let otras = r2(general - t_cuenta - a_cuenta - i.map_or(0.0, |i| i.imputaciones - i.reduccion));
        // Con pérdidas (un alquiler con más gastos que ingresos), resta.
        if otras.abs() > 1.0 {
            g.partes.push((Concepto::OtrasRentas, otras));
        }
        if let Some(i) = i
            && i.neto.abs() > DIF
        {
            let cobrado = if i.ingresos > DIF { format!("cobrado {}, menos sus gastos y su amortización", eur(i.ingresos)) } else { "lo cobrado menos sus gastos y su amortización".into() };
            let reducida = if i.reduccion > DIF { format!(" · en la Renta cuentan {}, tras la reducción por alquiler de vivienda", euro(i.reducidos)) } else { String::new() };
            g.filas.push(fila("Alquileres", Some(i.neto), format!("{cobrado}{reducida}"), de(&[cas.inmuebles.ingresos, cas.inmuebles.neto, cas.inmuebles.suma])).de(Concepto::OtrasRentas));
        }
        let resto = r2(general - t_cuenta - a_cuenta - i.map_or(0.0, |i| i.reducidos + i.imputaciones));
        if resto.abs() > 1.0 {
            let que = if i.is_some() { "lo que no es sueldo, actividad ni alquileres" } else { "alquileres, imputación de inmuebles y demás" };
            let conyuge = if r.conjunta { ", y lo de tu cónyuge" } else { "" };
            g.filas.push(fila("Otras rentas", Some(resto), format!("{que}{conyuge}"), de(&[cas.cuotas.base_general])).de(Concepto::OtrasRentas));
        }
    }
    if let Some(i) = i
        && i.imputaciones > DIF
    {
        let nota = "de los que no tienes alquilados: Hacienda la suma a tu base, pero no es dinero que cobres, así que no cuenta en lo que ganas";
        g.filas.push(Fila { aparte: true, ..fila("Renta imputada de tus inmuebles", Some(i.imputaciones), nota, de(&[cas.inmuebles.imputaciones])) });
    }
    if let Some(k) = &r.capital {
        if k.neto.abs() > DIF {
            g.partes.push((Concepto::Capital, k.neto));
        }
        if k.solo_total {
            g.filas.push(fila("Intereses y dividendos", Some(k.neto), "el total de la declaración", de(&[cas.capital.ahorro])).de(Concepto::Capital));
        } else if k.neto.abs() > DIF || k.integros.abs() > DIF {
            let mut v = Vec::new();
            for (nombre, x) in [("intereses", k.intereses), ("dividendos", k.dividendos), ("otros", k.otros)] {
                if x.abs() > DIF {
                    v.push(format!("{nombre} {}", eur(x)));
                }
            }
            if k.gastos > DIF {
                v.push(format!("gastos −{}", eur(k.gastos)));
            }
            g.filas.push(fila("Intereses y dividendos", Some(k.neto), v.join(" · "), de(&[cas.capital.intereses, cas.capital.dividendos, cas.capital.integros, cas.capital.gastos, cas.capital.ahorro])).de(Concepto::Capital));
        }
    }
    if let Some(gp) = &r.ganancias {
        if gp.neto.abs() > DIF {
            g.partes.push((Concepto::Ganancias, gp.neto));
        }
        if gp.ganancias.abs() > DIF || gp.perdidas.abs() > DIF {
            // Por tipo de bien, lo neto y, si hubo pérdidas, lo ganado y lo perdido.
            let parte = |nombre: &str, gan: f64, per: f64| -> Option<String> {
                if gan.abs() < DIF && per.abs() < DIF {
                    return None;
                }
                let detalle = if per > DIF && gan > DIF { format!(" ({} ganado, {} perdido)", eur(gan), eur(per)) } else { String::new() };
                Some(format!("{nombre} {}{detalle}", euro(r2(gan - per))))
            };
            let par = |x: &Option<crate::dominio::renta::GananciasPerdidas>| x.as_ref().map_or((0.0, 0.0), |x| (x.ganancias, x.perdidas));
            let (c, o) = (par(&gp.cripto), par(&gp.otros));
            let mut v: Vec<String> = [
                parte("criptomonedas", c.0, c.1),
                parte("fondos, acciones y demás", r2(gp.ganancias - c.0 - o.0), r2(gp.perdidas - c.1 - o.1)),
                parte("otros bienes", o.0, o.1),
            ]
            .into_iter()
            .flatten()
            .collect();
            let compensado = r2(gp.ganancias - gp.perdidas - gp.neto);
            if compensado > DIF {
                v.push(format!("menos {} compensados", eur(compensado)));
            } else if gp.ganancias - gp.perdidas < -DIF {
                v.push("las pérdidas se compensan en los años siguientes".into());
            }
            g.filas.push(fila("Ganancias patrimoniales", Some(gp.neto), v.join(" · "), de(&[cas.ganancias.ganancias, cas.ganancias.perdidas, cas.ganancias.neto])).de(Concepto::Ganancias));
        }
    }
    // En este orden: sueldo, actividad, otras rentas, capital, ganancias.
    g.total = r2(g.partes.iter().map(|p| p.1).sum());
    g
}

fn ganado_sin_renta(d: &Datos, y: i32, n: Option<&NominaAnio>) -> Ganado {
    let mut g = Ganado { sin_ahorro: true, ..Default::default() };
    let hoy = fmt::hoy();
    let fin = format!("{y}-12-31");
    let hasta = if fin < hoy { fin } else { hoy };
    let l = nominas(d, y);
    if !l.is_empty() {
        let sueldo = r2(l.iter().map(|n| n.bruto()).sum());
        g.especie = r2(l.iter().map(|n| n.especie).sum());
        g.partes.push((Concepto::Sueldo, sueldo));
        let especie = if g.especie.abs() > DIF { format!(" · {} en especie", eur(g.especie)) } else { String::new() };
        g.filas.push(fila("Sueldo", Some(sueldo), format!("{}{especie}", hasta_nomina(&l)), texto("tus nóminas apuntadas")).de(Concepto::Sueldo).origen(Origen::Apuntado));
    } else if let Some(n) = n.filter(|n| n.con_nomina > 0) {
        g.partes.push((Concepto::Sueldo, n.sueldo));
        g.aprox = true;
        let meses = n.texto_meses();
        g.filas.push(fila("Sueldo", Some(n.sueldo), "aproximado: lo cotizado según la Seguridad Social", texto(format!("Seguridad Social · {meses}"))).de(Concepto::Sueldo).origen(Origen::Estimado));
    }
    let a = irpf::irpf_acumulado(d, y, &hasta);
    let (facturado, gastos) = (a.ingresos, r2(a.gastos - a.gdj - a.lineas.get("ss_titular").copied().unwrap_or(0.0)));
    if facturado.abs() > DIF || gastos.abs() > DIF {
        let neto = r2(facturado - gastos);
        g.actividad = Some((facturado, gastos));
        g.partes.push((Concepto::Actividad, neto));
        let nota = format!("{} (sin tu cuota de autónomo ni los de difícil justificación)", bruto_actividad(facturado, gastos));
        g.filas.push(fila("Como autónomo", Some(neto), nota, texto("tus facturas y gastos")).de(Concepto::Actividad).origen(Origen::Apuntado));
    }
    g.total = r2(g.partes.iter().map(|p| p.1).sum());
    g
}

/* ================= cuota de autónomo ================= */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigenCuota {
    /// Los recibos de la Seguridad Social, por la fecha de cargo.
    Recibos,
    /// La Seguridad Social del titular de tu Renta.
    Renta,
    /// Tus gastos de cuota de autónomos.
    Gastos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CuotaAutonomo {
    pub total: f64,
    pub origen: OrigenCuota,
    /// Cuántos recibos (si sale de ellos).
    pub recibos: usize,
    /// De dónde sale, en palabras.
    pub de: String,
}

/// Tu cuota de autónomo del año (`cuotaAutonomo`): los recibos de la Seguridad Social; si no, tu Renta o tus
/// gastos. La Seguridad Social solo da los recibos de los últimos años: si tu alta es anterior, el primer año que
/// da sale a medias y manda la Renta, si la hay.
pub fn cuota_autonomo(d: &Datos, y: i32, r: Option<&Renta>) -> Option<CuotaAutonomo> {
    let ss = &d.tgss;
    let consultado = !ss.consultado.is_empty();
    let del_anio: Vec<_> = if consultado {
        ss.recibos.iter().filter(|x| fmt::anio_de(x.fecha_cargo()) == y).collect()
    } else {
        Vec::new()
    };
    let desde = if consultado { ss.recibos.iter().map(|x| x.emision.as_str()).filter(|e| !e.is_empty()).min().unwrap_or("") } else { "" };
    let alta = if consultado { ss.alta.fecha.get(..7).unwrap_or(&ss.alta.fecha) } else { "" };
    let de_renta = r.and_then(|r| r.actividad.as_ref()).and_then(|a| a.ss_titular).filter(|x| x.abs() > DIF);
    let a_medias = !alta.is_empty() && alta < desde && desde.starts_with(&format!("{y}-")) && desde > format!("{y}-01").as_str() && de_renta.is_some();
    if !del_anio.is_empty() && !a_medias {
        let n = del_anio.len();
        let total = r2(del_anio.iter().map(|x| x.importe).sum());
        let de = if n == 1 { format!("1 recibo cobrado en {y}") } else { format!("{n} recibos cobrados en {y}") };
        return Some(CuotaAutonomo { total, origen: OrigenCuota::Recibos, recibos: n, de });
    }
    if let Some(total) = de_renta {
        let de = if a_medias {
            format!("tu Renta de {y}: los recibos de la Seguridad Social empiezan en {}", fmt::MESES[(fmt::mes_de(&format!("{desde}-01")) as usize).clamp(1, 12) - 1])
        } else {
            format!("tu Renta de {y}")
        };
        return Some(CuotaAutonomo { total, origen: OrigenCuota::Renta, recibos: 0, de });
    }
    let g = r2(d.gastos.iter().filter(|g| g.categoria == "ss" && fmt::anio_de(&g.fecha) == y).map(|g| gasto::gasto_calc(g, d).coste).sum());
    (g.abs() > DIF).then(|| CuotaAutonomo { total: g, origen: OrigenCuota::Gastos, recibos: 0, de: "tus gastos de cuota de autónomos".into() })
}

/* ================= el año ================= */

/// Las cifras de un año, con las mismas cuentas y partes que la pantalla (`datosAnio`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Anio {
    pub y: i32,
    pub ganas: f64,
    pub pagas: f64,
    pub queda: f64,
    /// Lo que pagó tu empresa por ti (su parte de la Seguridad Social).
    pub empresa: f64,
    /// Por concepto (índice: `Concepto as usize`).
    pub importes: [f64; 8],
    /// Lo de tu empresa por grupos (`EMPRESA`).
    pub empresa_grupos: [f64; 4],
    /// De tu parte de la nómina, lo de la cotización de solidaridad.
    pub tu_solidaridad: f64,
    /// Lo que va en especie (del sueldo).
    pub especie: f64,
    /// De la actividad: (facturado, gastos).
    pub actividad: Option<(f64, f64)>,
    /// Declaración conjunta: con lo de tu cónyuge.
    pub conjunta: bool,
    /// Falta el IRPF de tu nómina (no está apuntada).
    pub falta_nomina: bool,
    pub con_renta: bool,
    /// El sueldo, aproximado (lo cotizado).
    pub sueldo_aprox: bool,
    /// Sin intereses, dividendos ni ganancias (no hay Renta).
    pub sin_ahorro: bool,
}

impl Anio {
    pub fn de(&self, c: Concepto) -> f64 {
        self.importes[c as usize]
    }
    /// Lo que pagaste a la Seguridad Social: tu parte de la nómina y tu cuota de autónomo.
    pub fn seguridad_social(&self) -> f64 {
        r2(self.de(Concepto::TuNomina) + self.de(Concepto::Cuota))
    }
    /// Sin nada que contar.
    pub fn vacio(&self) -> bool {
        self.ganas.abs() <= DIF && self.pagas.abs() <= DIF && self.empresa.abs() <= DIF
    }
}

/// Todo lo de un año para la pantalla: las cifras y cómo salen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detalle {
    pub anio: Anio,
    pub ganado: Ganado,
    pub irpf: Irpf,
    pub cuota: Option<CuotaAutonomo>,
    /// Lo que dice la Seguridad Social de tus nóminas de ese año.
    pub nomina: Option<NominaAnio>,
    /// Lo que conviene saber (conjunta, lo que falta…), en frases cortas.
    pub notas: Vec<String>,
}

/// Todo lo de un año.
pub fn detalle(d: &Datos, y: i32) -> Detalle {
    if y < PRIMER_ANIO {
        let notas = vec![format!("La app enseña los años desde {PRIMER_ANIO}.")];
        return Detalle { anio: Anio { y, ..Default::default() }, notas, ..Default::default() };
    }
    let hoy_y = fmt::anio_actual();
    let n = empleo::nomina_anio(d, y);
    let r = renta_de(d, y);
    let irpf = irpf_anio(d, y, n.as_ref());
    let cuota = cuota_autonomo(d, y, r);
    let ganado = ganado_anio(d, y, n.as_ref(), r);
    let con_nomina = n.as_ref().is_some_and(|n| n.con_nomina > 0);

    let parte = |c: Concepto| r2(ganado.partes.iter().filter(|p| p.0 == c).map(|p| p.1).sum());
    let tu_nom = n.as_ref().map_or(0.0, |n| n.cuota.trabajador);
    let c = cuota.as_ref().map_or(0.0, |c| c.total);
    let pagas = r2(irpf.total + tu_nom + c);
    let mut importes = [0.0; 8];
    for k in Concepto::GANAS {
        importes[k as usize] = parte(k);
    }
    importes[Concepto::Irpf as usize] = irpf.total;
    importes[Concepto::TuNomina as usize] = tu_nom;
    importes[Concepto::Cuota as usize] = c;
    let g = |k: &str| n.as_ref().filter(|_| con_nomina).map(|n| n.grupo(k)).unwrap_or_default();
    let anio = Anio {
        y,
        ganas: ganado.total,
        pagas,
        queda: r2(ganado.total - pagas),
        empresa: n.as_ref().map_or(0.0, |n| n.cuota.empresa),
        importes,
        empresa_grupos: [g("cc").empresa, g("otras").empresa, r2(g("at").empresa + g("mei").empresa + g("otros").empresa), g("sol").empresa],
        tu_solidaridad: g("sol").trabajador,
        especie: ganado.especie,
        actividad: ganado.actividad,
        conjunta: r.is_some_and(|r| r.conjunta),
        falta_nomina: irpf.falta_nomina,
        con_renta: r.is_some(),
        sueldo_aprox: ganado.aprox,
        sin_ahorro: ganado.sin_ahorro,
    };


    let mut notas = Vec::new();
    if anio.conjunta {
        notas.push("Declaración conjunta: las cifras de tu Renta son de la familia, con lo de tu cónyuge.".into());
    }
    if anio.falta_nomina {
        notas.push("Falta el IRPF de tu nómina: solo lo dicen tus nóminas (Hacienda no lo tiene hasta enero). Apúntalas en Empleo.".into());
    }
    if anio.sueldo_aprox {
        notas.push("El sueldo es aproximado: lo cotizado según la Seguridad Social. Apunta tus nóminas para afinarlo.".into());
    }
    if r.is_none() {
        // Año cerrado sin Renta en la app (la del año pasado se presenta hasta el fin de su plazo).
        let cerrado = y < hoy_y - 1 || (y == hoy_y - 1 && fmt::hoy() > hacienda::plazo_renta(y).1);
        if cerrado {
            notas.push(format!("No hay Renta de {y} en la app: sin su IRPF, intereses, dividendos ni ganancias."));
        } else if !anio.vacio() {
            notas.push(format!("Sin intereses, dividendos ni ganancias: salen en la Renta de {y}."));
        }
    }
    if let Some(t) = r.and_then(|r| r.trabajo.as_ref()).filter(|t| t.integros > DIF)
        && !con_nomina
    {
        let tu = if t.ss > DIF { format!(" (tu Renta dice {})", eur(t.ss)) } else { String::new() };
        notas.push(format!("La Seguridad Social no tiene el cálculo de tu nómina de {y}: tu parte{tu} y la de tu empresa no se suman."));
    }
    if anio.vacio() {
        notas.push(format!("No hay datos de {y}."));
    }

    Detalle { anio, ganado, irpf, cuota, nomina: n, notas }
}

/// Las cifras de un año (para la serie).
pub fn anio(d: &Datos, y: i32) -> Anio {
    detalle(d, y).anio
}

/// Los años con algo, del primero (desde `PRIMER_ANIO`) al actual; los de en medio también, aunque estén vacíos
/// (es una línea de tiempo).
pub fn serie(d: &Datos) -> Vec<Anio> {
    let desde = super::primer_anio(d).max(PRIMER_ANIO);
    let mut v: Vec<Anio> = (desde..=fmt::anio_actual()).map(|y| anio(d, y)).collect();
    match v.iter().position(|a| !a.vacio()) {
        Some(i) => v.split_off(i),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textos() {
        assert_eq!(Fuente::Renta(2023, vec![180, 223]).texto(), "Renta 2023 · 0180 0223");
        assert_eq!(euro(-456.78), "−456,78 €");
        assert_eq!(pct(1.0, 4.0), "25 %");
    }
}
