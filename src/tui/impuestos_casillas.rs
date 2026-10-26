//! Las casillas de los modelos 303 y 130 que se enseñan, con sus nombres (también las que solo salen en lo
//! presentado).
use crate::dominio::fmt::DIF;
use crate::dominio::Casillas;
use crate::hacienda::Trimestral;
use crate::dominio::fmt;
use crate::dominio::Valor;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Clase {
    /// Se ve si tiene algo (o con «todas»).
    Normal,
    /// Siempre se ve, resaltada.
    Total,
    /// Un porcentaje: siempre se ve.
    Pct,
    /// Poco habitual: solo se ve si tiene algo, aunque se pidan todas.
    Rara,
}

#[derive(Debug, Clone, Copy)]
pub struct Fila {
    pub n: u32,
    /// «{t}» se cambia por el tipo de IVA («21 %»).
    pub nombre: &'static str,
    pub clase: Clase,
    /// Casilla con el tipo de IVA de esta base o cuota (cambia de un año a otro).
    pub tipo: Option<u32>,
}

pub type Grupo = (&'static str, &'static [Fila]);

const fn f(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Normal, tipo: None }
}
const fn t(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Total, tipo: None }
}
const fn p(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Pct, tipo: None }
}
const fn r(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Rara, tipo: None }
}
/// Base o cuota de un tipo de IVA (el % está en la casilla `tipo`).
const fn bt(n: u32, nombre: &'static str, tipo: u32, rara: bool) -> Fila {
    Fila { n, nombre, clase: if rara { Clase::Rara } else { Clase::Normal }, tipo: Some(tipo) }
}

/// El tipo de las casillas de tipo que no vienen en lo calculado.
const TIPO_FIJO: [(u32, f64); 3] = [(2, 4.0), (5, 10.0), (8, 21.0)];

const RG: &str = "Régimen general {t} · base imponible";
const RG_C: &str = "Régimen general {t} · cuota";
const RE: &str = "Recargo de equivalencia {t} · base";
const RE_C: &str = "Recargo de equivalencia {t} · cuota";

pub const G303: &[Grupo] = &[
    (
        "IVA devengado · el que has cobrado",
        &[
            bt(150, RG, 151, true),
            bt(152, RG_C, 151, true),
            bt(165, RG, 166, true),
            bt(167, RG_C, 166, true),
            bt(1, RG, 2, false),
            bt(3, RG_C, 2, false),
            bt(153, RG, 154, true),
            bt(155, RG_C, 154, true),
            bt(4, RG, 5, false),
            bt(6, RG_C, 5, false),
            bt(7, RG, 8, false),
            bt(9, RG_C, 8, false),
            f(10, "Adquisiciones intracomunitarias de bienes y servicios · base"),
            f(11, "Adquisiciones intracomunitarias · cuota"),
            f(12, "Otras operaciones con inversión del sujeto pasivo · base"),
            f(13, "Otras operaciones con inversión del sujeto pasivo · cuota"),
            f(14, "Modificación de bases y cuotas · base"),
            f(15, "Modificación de bases y cuotas · cuota"),
            bt(156, RE, 157, true),
            bt(158, RE_C, 157, true),
            bt(168, RE, 169, true),
            bt(170, RE_C, 169, true),
            bt(16, RE, 17, true),
            bt(18, RE_C, 17, true),
            bt(19, RE, 20, true),
            bt(21, RE_C, 20, true),
            bt(22, RE, 23, true),
            bt(24, RE_C, 23, true),
            r(25, "Modificación del recargo de equivalencia · base"),
            r(26, "Modificación del recargo de equivalencia · cuota"),
            t(27, "Total cuota devengada"),
        ],
    ),
    (
        "IVA deducible · el de tus gastos",
        &[
            f(28, "Operaciones interiores corrientes · base"),
            f(29, "Operaciones interiores corrientes · cuota"),
            f(30, "Operaciones interiores con bienes de inversión · base"),
            f(31, "Operaciones interiores con bienes de inversión · cuota"),
            r(32, "Importaciones de bienes corrientes · base"),
            r(33, "Importaciones de bienes corrientes · cuota"),
            r(34, "Importaciones de bienes de inversión · base"),
            r(35, "Importaciones de bienes de inversión · cuota"),
            f(36, "Adquisiciones intracomunitarias de bienes y servicios corrientes · base"),
            f(37, "Adquisiciones intracomunitarias corrientes · cuota"),
            f(38, "Adquisiciones intracomunitarias de bienes de inversión · base"),
            f(39, "Adquisiciones intracomunitarias de bienes de inversión · cuota"),
            r(40, "Rectificación de deducciones · base"),
            r(41, "Rectificación de deducciones · cuota"),
            r(42, "Compensaciones del régimen especial de agricultura, ganadería y pesca"),
            r(43, "Regularización de bienes de inversión"),
            r(44, "Regularización por la prorrata definitiva"),
            t(45, "Total a deducir"),
        ],
    ),
    (
        "Información adicional",
        &[
            f(59, "Entregas intracomunitarias de bienes y servicios (clientes empresa de la UE)"),
            f(60, "Exportaciones y operaciones asimiladas"),
            r(61, "Operaciones no sujetas o con inversión del sujeto pasivo con derecho a deducción"),
            f(120, "Operaciones no sujetas por reglas de localización (clientes fuera de la UE)"),
            r(122, "Operaciones sujetas con inversión del sujeto pasivo"),
            r(123, "Operaciones no sujetas acogidas a la ventanilla única"),
            r(124, "Operaciones sujetas acogidas a la ventanilla única"),
            r(62, "Criterio de caja · base de lo devengado"),
            r(63, "Criterio de caja · cuota de lo devengado"),
            r(74, "Criterio de caja · base de las adquisiciones"),
            r(75, "Criterio de caja · cuota de las adquisiciones"),
        ],
    ),
    (
        "Volumen de operaciones del año",
        &[
            r(80, "Operaciones en régimen general"),
            r(81, "Operaciones en régimen especial del criterio de caja"),
            r(93, "Entregas intracomunitarias exentas"),
            r(94, "Exportaciones y otras operaciones exentas con derecho a deducción"),
            r(83, "Operaciones exentas sin derecho a deducción"),
            r(84, "Operaciones no sujetas por reglas de localización"),
            r(125, "Operaciones sujetas con inversión del sujeto pasivo"),
            r(99, "Entregas de bienes de inversión"),
            r(88, "Total volumen de operaciones"),
        ],
    ),
    (
        "Resultado",
        &[
            t(46, "Resultado régimen general (27 − 45)"),
            r(76, "Regularización de cuotas (art. 80.cinco.5ª LIVA)"),
            f(64, "Suma de resultados"),
            p(65, "% atribuible a la Administración del Estado"),
            f(66, "Atribuible a la Administración del Estado"),
            r(77, "IVA a la importación liquidado por la Aduana pendiente de ingreso"),
            r(67, "Cuotas a compensar de periodos anteriores"),
            f(110, "Cuotas a compensar pendientes de periodos anteriores"),
            f(78, "Cuotas a compensar de periodos anteriores aplicadas en este periodo"),
            f(87, "Cuotas a compensar pendientes para periodos posteriores"),
            r(68, "Regularización anual (tributación conjunta con las Haciendas Forales)"),
            r(108, "Otros ajustes (rectificativa)"),
            f(69, "Resultado de la autoliquidación"),
            r(70, "A deducir: resultados de declaraciones anteriores del mismo periodo"),
            r(109, "Devoluciones acordadas por Hacienda de este periodo"),
            r(112, "Pago a cuenta de gasolinas y gasóleos"),
            t(71, "Resultado de la liquidación"),
            r(111, "Rectificativa: importe de la rectificación"),
        ],
    ),
];

pub const G130: &[Grupo] = &[
    (
        "I. Estimación directa · acumulado desde el 1 de enero",
        &[
            f(1, "Ingresos computables"),
            f(2, "Gastos fiscalmente deducibles"),
            f(3, "Rendimiento neto (01 − 02)"),
            f(4, "20 % de la casilla 03 (si es positiva)"),
            f(5, "A deducir: pagos fraccionados de trimestres anteriores"),
            f(6, "Retenciones e ingresos a cuenta"),
            t(7, "Pago fraccionado previo del trimestre (04 − 05 − 06)"),
        ],
    ),
    (
        "II. Actividades agrícolas, ganaderas, forestales y pesqueras",
        &[
            r(8, "Volumen de ingresos del trimestre"),
            r(9, "2 % de la casilla 08"),
            r(10, "Retenciones e ingresos a cuenta"),
            r(11, "Pago fraccionado previo del trimestre"),
        ],
    ),
    (
        "III. Total liquidación",
        &[
            f(12, "Suma de pagos fraccionados previos del trimestre"),
            f(13, "Minoración por rendimientos del año anterior (art. 110.3.c RIRPF)"),
            f(14, "Diferencia (12 − 13)"),
            f(15, "A deducir: resultados negativos de trimestres anteriores"),
            f(16, "Deducción por préstamo de vivienda habitual"),
            f(17, "Total (14 − 15 − 16)"),
            r(18, "A deducir: resultado de la declaración anterior (complementaria)"),
            t(19, "Resultado de la autoliquidación"),
        ],
    ),
];

/// Las casillas que se enseñan de un modelo.
pub fn grupos(modelo: Trimestral) -> &'static [Grupo] {
    match modelo {
        Trimestral::M303 => G303,
        Trimestral::M130 => G130,
    }
}

/// ¿Se enseña esta fila? Con varias columnas (calculado y presentado), basta con que una tenga algo.
pub fn visible(fila: &Fila, todas: bool, columnas: &[&Casillas]) -> bool {
    let tiene = columnas.iter().any(|c| c.v(fila.n).abs() > DIF);
    match fila.clase {
        Clase::Total | Clase::Pct => true,
        Clase::Normal => todas || tiene,
        Clase::Rara => tiene,
    }
}

/// El nombre de la fila, con el tipo de IVA si lo lleva («Régimen general 21 % · cuota»).
pub fn nombre(fila: &Fila, columnas: &[&Casillas]) -> String {
    let Some(k) = fila.tipo else { return fila.nombre.to_string() };
    let pct = columnas
        .iter()
        .map(|c| c.v(k))
        .find(|x| x.abs() > DIF)
        .or_else(|| TIPO_FIJO.iter().find(|(n, _)| *n == k).map(|(_, x)| *x));
    match pct {
        Some(x) => fila.nombre.replace("{t}", &fmt::pct(x)),
        None => fila.nombre.replace(" {t}", ""),
    }
}

/// «07», «110» (como en los formularios de Hacienda).
pub fn numero(n: u32) -> String {
    if n < 100 { format!("{n:02}") } else { n.to_string() }
}

/// Las filas en las que lo calculado y lo presentado no coinciden.
pub fn distintas(modelo: Trimestral, calc: &Casillas, pres: &Casillas) -> usize {
    grupos(modelo).iter().flat_map(|(_, filas)| filas.iter()).filter(|x| (calc.v(x.n) - pres.v(x.n)).abs() > DIF).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_con_tipo() {
        let c = Casillas::new();
        let fila = G303[0].1.iter().find(|x| x.n == 9).unwrap();
        assert_eq!(nombre(fila, &[&c]), "Régimen general 21 % · cuota");
        // El tipo que trae lo presentado manda.
        let mut p = Casillas::new();
        p.insert(151, 5.0);
        let fila = G303[0].1.iter().find(|x| x.n == 150).unwrap();
        assert_eq!(nombre(fila, &[&c, &p]), "Régimen general 5 % · base imponible");
        assert_eq!(nombre(fila, &[&c]), "Régimen general · base imponible");
        assert_eq!(numero(7), "07");
        assert_eq!(numero(110), "110");
    }

    #[test]
    fn que_filas_se_ven() {
        let mut c = Casillas::new();
        c.insert(28, 270.0);
        let todas = |m: Trimestral, t: bool, c: &Casillas| -> Vec<u32> {
            grupos(m).iter().flat_map(|(_, f)| f.iter()).filter(|x| visible(x, t, &[c])).map(|x| x.n).collect()
        };
        let v = todas(Trimestral::M303, false, &c);
        assert!(v.contains(&28) && v.contains(&27) && v.contains(&71) && v.contains(&65));
        assert!(!v.contains(&29));
        // Con «todas» salen las normales a cero, pero no las raras.
        let v = todas(Trimestral::M303, true, &c);
        assert!(v.contains(&29) && !v.contains(&32));
        // Sin nada (lo que devuelve el esbozo de calc) salen solo los totales.
        let vacio = Casillas::new();
        assert_eq!(todas(Trimestral::M130, false, &vacio), vec![7, 19]);
    }

    #[test]
    fn diferencias() {
        let mut a = Casillas::new();
        let mut b = Casillas::new();
        a.insert(28, 270.0);
        b.insert(28, 270.004);
        assert_eq!(distintas(Trimestral::M303, &a, &b), 0);
        b.insert(71, -56.7);
        assert_eq!(distintas(Trimestral::M303, &a, &b), 1);
        // Las casillas que no están en la lista (los tipos, p. ej.) no cuentan.
        b.insert(8, 21.0);
        assert_eq!(distintas(Trimestral::M303, &a, &b), 1);
    }
}
