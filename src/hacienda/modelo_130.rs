//! Modelo 130 · el pago fraccionado del IRPF de la actividad, cada trimestre. Todo lo que la app sabe de él:
//!
//! 1. **Qué casillas tiene** (`DISENOS`): las del diseño de registro oficial de la AEAT (Orden HAP/258/2015,
//!    `DR130e15v12.xls`, en sede.agenciatributaria.gob.es → Ayuda → Diseños de registro → Modelos 100 al 199),
//!    el mismo desde 2019.
//! 2. **Cómo se lee** de su PDF: como cualquier formulario (`formulario::leer`), y lo leído tiene que cuadrar con
//!    sus sumas (`reglas`).
//! 3. **Cuándo se presenta**: con el 303, cada trimestre (`hacienda::plazos_trimestres`). Quien factura casi todo
//!    con retención se libra (`exento`).
//! 4. **Cómo se calcula** con tus facturas y gastos (`calcular`): con lo acumulado desde el 1 de enero, y los
//!    trimestres se encadenan (casillas 05 y 15, con los 130 anteriores del año).
use super::irpf::irpf_acumulado;
use super::formulario::{Diseno, Formulario, QueCasillas};
use super::modelo::{Memo, Modelo, Periodicidad, Trimestral};
use crate::dominio::factura::{emitidas, totales_factura};
use crate::dominio::fmt::{anio_de, en_rango, periodo, r2, DIF};
use crate::dominio::hacienda::Tipo;
use crate::dominio::{Casillas, Datos, Valor};

/// El modelo 130, para leerlo (`formulario::leer::<M130>`).
pub struct M130;

impl Formulario for M130 {
    const MODELO: Modelo = Modelo::M130;
    const PERIODICIDAD: Periodicidad = Periodicidad::Trimestral;
    const CASILLAS: QueCasillas = QueCasillas::Disenos(DISENOS);

    /// La 04 no: se puede pagar más del 20 %.
    fn reglas(c: &Casillas) -> Vec<(u32, f64)> {
        let v = |n| c.v(n);
        vec![
            (3, v(1) - v(2)),
            (7, v(4) - v(5) - v(6)),
            (11, v(9) - v(10)),
            (12, (v(7) + v(11)).max(0.0)),
            (14, v(12) - v(13)),
            (17, v(14) - v(15) - v(16)),
            (19, v(17) - v(18)),
        ]
    }

    /// Negativa si la 19 lo es; si no, a cero.
    fn sin_resultado(c: &Casillas) -> Tipo {
        if c.v(19) < 0.0 { Tipo::Negativa } else { Tipo::Cero }
    }
}

/// La casilla del resultado: lo que sale a ingresar.
pub const RESULTADO: u32 = 19;

/// Lo que sale por el trimestre entero: en una complementaria (con algo en la 18, lo ya ingresado), la 17, que
/// suma lo de antes y lo de ahora; si no, la del resultado (19).
pub fn resultado_del_trimestre(c: &Casillas) -> f64 {
    if c.v(18).abs() > DIF && c.contains_key(&17) { r2(c.v(17)) } else { r2(c.v(RESULTADO)) }
}

/* ================= 1. las casillas del diseño de registro ================= */

const DISENOS: &[Diseno] = &[Diseno { id: "e19", desde: 201901, casillas: &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19] }];

/* ================= 4. calcularlo con tus facturas y gastos ================= */

/// Minoración de la casilla 13 según el rendimiento neto del año anterior.
pub fn minoracion(rn: Option<f64>) -> f64 {
    match rn {
        None => 0.0,
        Some(x) if x <= 9000.0 => 100.0,
        Some(x) if x <= 10000.0 => 75.0,
        Some(x) if x <= 11000.0 => 50.0,
        Some(x) if x <= 12000.0 => 25.0,
        Some(_) => 0.0,
    }
}

/// ¿El de difícil justificación se resta también en los 130? (por defecto no: solo en la Renta)
pub fn con_gdj(d: &Datos) -> bool {
    d.fiscal.gdj && d.fiscal.gdj130
}

/// El 130 calculado con tus facturas y gastos.
pub fn calcular(d: &Datos, anio: i32, q: u32, memo: &mut Memo) -> Casillas {
    let hasta = periodo(anio, q).1;
    let a = irpf_acumulado(d, anio, &hasta);
    let c1 = a.ingresos;
    let c2 = if con_gdj(d) { a.gastos } else { r2(a.gastos - a.gdj) };
    let c3 = r2(c1 - c2);
    let c4 = if c3 > 0.0 { r2(c3 * 0.20) } else { 0.0 };
    // 05: positivos de la casilla 07 de trimestres anteriores (menos sus casillas 16).
    // 15: negativos de la casilla 19 anteriores aún no deducidos.
    let (mut c5, mut pendiente) = (0.0, 0.0);
    for k in 1..q {
        let p = Trimestral::M130.cuenta(d, anio, k, memo);
        if p.v(7) > 0.0 {
            c5 = r2(c5 + p.v(7) - p.v(16));
        }
        if p.v(19) < 0.0 {
            pendiente = r2(pendiente - p.v(19));
        }
        pendiente = r2(pendiente - p.v(15));
    }
    let c6 = a.retenciones;
    let c7 = r2(c4 - c5 - c6);
    let (c8, c9, c10, c11) = (0.0, 0.0, 0.0, 0.0);
    let c12 = r2(c7 + c11).max(0.0);
    let c13 = minoracion(rn_anterior(d, anio).valor);
    let c14 = r2(c12 - c13);
    let c15 = if c14 > 0.0 { r2(c14.min(pendiente.max(0.0))) } else { 0.0 };
    let c16 = 0.0;
    let c17 = r2(c14 - c15 - c16);
    let c18 = 0.0;
    let c19 = r2(c17 - c18);
    [c1, c2, c3, c4, c5, c6, c7, c8, c9, c10, c11, c12, c13, c14, c15, c16, c17, c18, c19].into_iter().zip(1..).map(|(v, k)| (k, v)).collect()
}

/// El rendimiento neto del año anterior (para la casilla 13) y de dónde sale.
#[derive(Debug, Clone, Default)]
pub struct RnAnterior {
    pub valor: Option<f64>,
    pub origen: OrigenRn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OrigenRn {
    /// Lo has escrito a mano.
    Escrito,
    /// Empezaste la actividad ese año: el anterior cuenta como 0.
    Alta,
    /// Calculado con tus datos del año anterior.
    Calculado,
    /// No se sabe.
    #[default]
    NoSe,
}

/// ¿Empezó la actividad ese año (o después)? Con la fecha de tu perfil (`Datos::fechas_actividad`).
fn primer_anio_de_actividad(d: &Datos, anio: i32) -> bool {
    let (alta, _) = d.fechas_actividad();
    !alta.is_empty() && anio_de(alta) >= anio
}

/// El rendimiento neto del año anterior: 1º lo escrito a mano; 2º si empezaste este año, 0; 3º lo que salga con tus
/// datos del año pasado.
pub fn rn_anterior(d: &Datos, anio: i32) -> RnAnterior {
    if let Some(&valor) = d.fiscal.rn_anterior.get(&anio) {
        return RnAnterior { valor: Some(valor), origen: OrigenRn::Escrito };
    }
    if primer_anio_de_actividad(d, anio) {
        return RnAnterior { valor: Some(0.0), origen: OrigenRn::Alta };
    }
    let ant = irpf_acumulado(d, anio - 1, &format!("{}-12-31", anio - 1));
    if ant.ingresos > 0.0 {
        return RnAnterior { valor: Some(ant.rn), origen: OrigenRn::Calculado };
    }
    RnAnterior { valor: None, origen: OrigenRn::NoSe }
}

/// Si se libra del 130: con el 70 % o más de los ingresos del año anterior con retención.
#[derive(Debug, Clone, Default)]
pub struct Exento {
    pub exento: bool,
    /// % de ingresos con retención.
    pub pct: f64,
    /// Con qué ingresos se ha mirado.
    pub por_que: PorQue,
    /// Sin datos del año anterior: lo que saldría con el trimestre.
    pub sugerido: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PorQue {
    /// Con los del año anterior.
    AnioAnterior,
    /// Con los del trimestre, por ser el primer año de actividad.
    Trimestre,
    /// Con los del trimestre, porque no hay datos del año anterior (y no es el primero).
    TrimestreSinAnterior,
    /// No hay ingresos con los que mirarlo.
    #[default]
    SinDatos,
}

/// ¿Exento del 130? (70 % o más de los ingresos del año anterior con retención). En el primer año de actividad
/// se mira el propio trimestre (`q` = 0: sin trimestre).
pub fn exento(d: &Datos, anio: i32, q: u32) -> Exento {
    let primer = primer_anio_de_actividad(d, anio);
    let ant = irpf_acumulado(d, anio - 1, &format!("{}-12-31", anio - 1));
    let (mut pct, mut por_que) = (None, PorQue::SinDatos);
    if !primer && ant.ingresos > 0.0 {
        pct = Some(ant.ingresos_con_ret / ant.ingresos * 100.0);
        por_que = PorQue::AnioAnterior;
    } else if q > 0 {
        let (desde, hasta) = periodo(anio, q);
        let (mut ing, mut con) = (0.0, 0.0);
        for f in emitidas(d) {
            if !en_rango(f.devengo(), &desde, &hasta) {
                continue;
            }
            let t = totales_factura(f);
            ing += t.base;
            if t.retencion > 0.0 {
                con += t.base;
            }
        }
        if ing > 0.0 {
            pct = Some(con / ing * 100.0);
            por_que = if primer { PorQue::Trimestre } else { PorQue::TrimestreSinAnterior };
        }
    }
    match pct {
        None => Exento { exento: false, pct: 0.0, por_que: PorQue::SinDatos, sugerido: None },
        // Sin datos del año anterior (y no es el primer año) no se da por exento: mejor presentarlo de más.
        Some(pct) if por_que == PorQue::TrimestreSinAnterior => Exento { exento: false, pct, por_que, sugerido: Some(pct >= 70.0) },
        Some(pct) => Exento { exento: pct >= 70.0, pct, por_que, sugerido: None },
    }
}

/// ¿Se libra del 130 este trimestre? Respeta lo elegido a mano (`fiscal.exento130`).
pub fn se_libra(d: &Datos, anio: i32, q: u32) -> bool {
    match d.fiscal.exento130.as_str() {
        "si" => true,
        "no" => false,
        _ => exento(d, anio, q).exento,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hacienda::formulario::descuadres;

    #[test]
    fn sumas_del_modelo() {
        let c = |v: &[(u32, f64)]| v.iter().copied().collect::<Casillas>();
        assert!(descuadres::<M130>(&c(&[(1, 100.0), (2, 40.0), (3, 60.0), (4, 12.0), (7, 12.0), (12, 12.0), (14, 12.0), (17, 12.0), (19, 12.0)])).is_empty());
        assert_eq!(descuadres::<M130>(&c(&[(1, 100.0), (2, 40.0), (3, 70.0)])), [3]);
        // El 04 puede ser más del 20 % del 03 (se puede adelantar más): no se comprueba.
        assert!(descuadres::<M130>(&c(&[(1, 100.0), (2, 40.0), (3, 60.0), (4, 30.0), (7, 30.0), (12, 30.0), (14, 30.0), (17, 30.0), (19, 30.0)])).is_empty());
        assert_eq!(M130::sin_resultado(&c(&[(19, -5.0)])), Tipo::Negativa);
        assert_eq!(M130::sin_resultado(&c(&[])), Tipo::Cero);
    }

    #[test]
    fn minoracion_por_rendimiento() {
        assert_eq!(minoracion(None), 0.0);
        assert_eq!(minoracion(Some(-5.0)), 100.0);
        assert_eq!(minoracion(Some(10000.0)), 75.0);
        assert_eq!(minoracion(Some(12000.01)), 0.0);
    }
}
