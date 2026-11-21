//! Las casillas de los modelos 303, 130 y 390 que se enseñan, con sus nombres (también las que solo salen en lo
//! presentado). Las más habituales llevan además un nombre corto, para cuando el 303 y el 130 van uno al lado del
//! otro y el oficial no cabe.
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
    /// Un nombre corto (22 columnas como mucho, con el tipo), para cuando el oficial no cabe.
    pub corto: Option<&'static str>,
}

impl Fila {
    /// La misma fila con un nombre corto.
    const fn c(self, corto: &'static str) -> Fila {
        Fila { corto: Some(corto), ..self }
    }
}

pub type Grupo = (&'static str, &'static [Fila]);

const fn f(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Normal, tipo: None, corto: None }
}
const fn t(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Total, tipo: None, corto: None }
}
const fn p(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Pct, tipo: None, corto: None }
}
const fn r(n: u32, nombre: &'static str) -> Fila {
    Fila { n, nombre, clase: Clase::Rara, tipo: None, corto: None }
}
/// Base o cuota de un tipo de IVA (el % está en la casilla `tipo`).
const fn bt(n: u32, nombre: &'static str, tipo: u32, rara: bool) -> Fila {
    Fila { n, nombre, clase: if rara { Clase::Rara } else { Clase::Normal }, tipo: Some(tipo), corto: None }
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
            bt(150, RG, 151, true).c("Base imponible al {t}"),
            bt(152, RG_C, 151, true).c("Cuota al {t}"),
            bt(165, RG, 166, true).c("Base imponible al {t}"),
            bt(167, RG_C, 166, true).c("Cuota al {t}"),
            bt(1, RG, 2, false).c("Base imponible al {t}"),
            bt(3, RG_C, 2, false).c("Cuota al {t}"),
            bt(153, RG, 154, true).c("Base imponible al {t}"),
            bt(155, RG_C, 154, true).c("Cuota al {t}"),
            bt(4, RG, 5, false).c("Base imponible al {t}"),
            bt(6, RG_C, 5, false).c("Cuota al {t}"),
            bt(7, RG, 8, false).c("Base imponible al {t}"),
            bt(9, RG_C, 8, false).c("Cuota al {t}"),
            f(10, "Adquisiciones intracomunitarias de bienes y servicios · base").c("Compras UE · base"),
            f(11, "Adquisiciones intracomunitarias · cuota").c("Compras UE · cuota"),
            f(12, "Otras operaciones con inversión del sujeto pasivo · base").c("Inversión SP · base"),
            f(13, "Otras operaciones con inversión del sujeto pasivo · cuota").c("Inversión SP · cuota"),
            f(14, "Modificación de bases y cuotas · base").c("Modificaciones · base"),
            f(15, "Modificación de bases y cuotas · cuota").c("Modificaciones · cuota"),
            bt(156, RE, 157, true).c("Recargo {t} · base"),
            bt(158, RE_C, 157, true).c("Recargo {t} · cuota"),
            bt(168, RE, 169, true).c("Recargo {t} · base"),
            bt(170, RE_C, 169, true).c("Recargo {t} · cuota"),
            bt(16, RE, 17, true).c("Recargo {t} · base"),
            bt(18, RE_C, 17, true).c("Recargo {t} · cuota"),
            bt(19, RE, 20, true).c("Recargo {t} · base"),
            bt(21, RE_C, 20, true).c("Recargo {t} · cuota"),
            bt(22, RE, 23, true).c("Recargo {t} · base"),
            bt(24, RE_C, 23, true).c("Recargo {t} · cuota"),
            r(25, "Modificación del recargo de equivalencia · base").c("Mod. recargo · base"),
            r(26, "Modificación del recargo de equivalencia · cuota").c("Mod. recargo · cuota"),
            t(27, "Total cuota devengada"),
        ],
    ),
    (
        "IVA deducible · el de tus gastos",
        &[
            f(28, "Operaciones interiores corrientes · base").c("Base de tus gastos"),
            f(29, "Operaciones interiores corrientes · cuota").c("Cuota de tus gastos"),
            f(30, "Operaciones interiores con bienes de inversión · base").c("Bienes de inv. · base"),
            f(31, "Operaciones interiores con bienes de inversión · cuota").c("Bienes de inv. · cuota"),
            r(32, "Importaciones de bienes corrientes · base").c("Importaciones · base"),
            r(33, "Importaciones de bienes corrientes · cuota").c("Importaciones · cuota"),
            r(34, "Importaciones de bienes de inversión · base").c("Import. inv. · base"),
            r(35, "Importaciones de bienes de inversión · cuota").c("Import. inv. · cuota"),
            f(36, "Adquisiciones intracomunitarias de bienes y servicios corrientes · base").c("Compras UE · base"),
            f(37, "Adquisiciones intracomunitarias corrientes · cuota").c("Compras UE · cuota"),
            f(38, "Adquisiciones intracomunitarias de bienes de inversión · base").c("UE inversión · base"),
            f(39, "Adquisiciones intracomunitarias de bienes de inversión · cuota").c("UE inversión · cuota"),
            r(40, "Rectificación de deducciones · base").c("Rectificación · base"),
            r(41, "Rectificación de deducciones · cuota").c("Rectificación · cuota"),
            r(42, "Compensaciones del régimen especial de agricultura, ganadería y pesca").c("Compensaciones REAGP"),
            r(43, "Regularización de bienes de inversión").c("Regul. bienes de inv."),
            r(44, "Regularización por la prorrata definitiva").c("Regul. prorrata"),
            t(45, "Total a deducir"),
        ],
    ),
    (
        "información adicional",
        &[
            f(59, "Entregas intracomunitarias de bienes y servicios (clientes empresa de la UE)").c("Ventas a empresas UE"),
            f(60, "Exportaciones y operaciones asimiladas").c("Exportaciones"),
            r(61, "Operaciones no sujetas o con inversión del sujeto pasivo con derecho a deducción").c("No sujetas con deduc."),
            f(120, "Operaciones no sujetas por reglas de localización (clientes fuera de la UE)").c("Ventas fuera de la UE"),
            r(122, "Operaciones sujetas con inversión del sujeto pasivo").c("Con inversión SP"),
            r(123, "Operaciones no sujetas acogidas a la ventanilla única").c("V. única no sujetas"),
            r(124, "Operaciones sujetas acogidas a la ventanilla única").c("V. única sujetas"),
            r(62, "Criterio de caja · base de lo devengado").c("Caja · base devengada"),
            r(63, "Criterio de caja · cuota de lo devengado").c("Caja · cuota devengada"),
            r(74, "Criterio de caja · base de las adquisiciones").c("Caja · base compras"),
            r(75, "Criterio de caja · cuota de las adquisiciones").c("Caja · cuota compras"),
        ],
    ),
    (
        "volumen de operaciones del año",
        &[
            r(80, "Operaciones en régimen general"),
            r(81, "Operaciones en régimen especial del criterio de caja").c("Criterio de caja"),
            r(93, "Entregas intracomunitarias exentas").c("Entregas UE exentas"),
            r(94, "Exportaciones y otras operaciones exentas con derecho a deducción").c("Exportaciones exentas"),
            r(83, "Operaciones exentas sin derecho a deducción").c("Exentas sin deducción"),
            r(84, "Operaciones no sujetas por reglas de localización").c("No sujetas (localiz.)"),
            r(125, "Operaciones sujetas con inversión del sujeto pasivo").c("Con inversión SP"),
            r(99, "Entregas de bienes de inversión").c("Bienes de inversión"),
            r(88, "Total volumen de operaciones").c("Total volumen"),
        ],
    ),
    (
        "resultado",
        &[
            t(46, "Resultado régimen general (27 − 45)").c("Resultado (27 − 45)"),
            r(76, "Regularización de cuotas (art. 80.cinco.5ª LIVA)").c("Regularización cuotas"),
            f(64, "Suma de resultados"),
            p(65, "% atribuible a la Administración del Estado").c("% para el Estado"),
            f(66, "Atribuible a la Administración del Estado").c("Atribuible al Estado"),
            r(77, "IVA a la importación liquidado por la Aduana pendiente de ingreso").c("IVA importación Aduana"),
            r(67, "Cuotas a compensar de periodos anteriores").c("A compensar de antes"),
            f(110, "Cuotas a compensar pendientes de periodos anteriores").c("Pendiente de compensar"),
            f(78, "Cuotas a compensar de periodos anteriores aplicadas en este periodo").c("Compensas ahora"),
            f(87, "Cuotas a compensar pendientes para periodos posteriores").c("Queda por compensar"),
            r(68, "Regularización anual (tributación conjunta con las Haciendas Forales)").c("Regul. forales"),
            r(108, "Otros ajustes (rectificativa)").c("Otros ajustes"),
            f(69, "Resultado de la autoliquidación").c("Autoliquidación"),
            r(70, "A deducir: resultados de declaraciones anteriores del mismo periodo").c("Ya declarado antes"),
            r(109, "Devoluciones acordadas por Hacienda de este periodo").c("Devoluciones acordadas"),
            r(112, "Pago a cuenta de gasolinas y gasóleos").c("Pago gasolinas"),
            t(71, "Resultado de la liquidación").c("Resultado"),
            r(111, "Rectificativa: importe de la rectificación").c("Importe rectificación"),
        ],
    ),
];

pub const G130: &[Grupo] = &[
    (
        "estimación directa · acumulado desde enero",
        &[
            f(1, "Ingresos computables"),
            f(2, "Gastos fiscalmente deducibles").c("Gastos deducibles"),
            f(3, "Rendimiento neto (01 − 02)").c("Rendimiento neto"),
            f(4, "20 % de la casilla 03 (si es positiva)").c("20 % de la 03"),
            f(5, "A deducir: pagos fraccionados de trimestres anteriores").c("Pagos anteriores"),
            f(6, "Retenciones e ingresos a cuenta").c("Retenciones"),
            t(7, "Pago fraccionado previo del trimestre (04 − 05 − 06)").c("Pago fraccionado"),
        ],
    ),
    (
        "agrícolas, ganaderas, forestales y pesca",
        &[
            r(8, "Volumen de ingresos del trimestre").c("Ingresos del trimestre"),
            r(9, "2 % de la casilla 08").c("2 % de la 08"),
            r(10, "Retenciones e ingresos a cuenta").c("Retenciones"),
            r(11, "Pago fraccionado previo del trimestre").c("Pago fraccionado"),
        ],
    ),
    (
        "total liquidación",
        &[
            f(12, "Suma de pagos fraccionados previos del trimestre").c("Suma de pagos previos"),
            f(13, "Minoración por rendimientos del año anterior (art. 110.3.c RIRPF)").c("Minoración"),
            f(14, "Diferencia (12 − 13)"),
            f(15, "A deducir: resultados negativos de trimestres anteriores").c("Negativos anteriores"),
            f(16, "Deducción por préstamo de vivienda habitual").c("Préstamo de vivienda"),
            f(17, "Total (14 − 15 − 16)"),
            r(18, "A deducir: resultado de la declaración anterior (complementaria)").c("Declaración anterior"),
            t(19, "Resultado de la autoliquidación").c("Resultado"),
        ],
    ),
];

/// El 390 (resumen anual del IVA): lo habitual de un autónomo en régimen general; lo demás, si tiene algo.
pub const G390: &[Grupo] = &[
    (
        "IVA devengado · régimen ordinario",
        &[
            r(700, "0 % · base imponible"),
            r(1, "4 % · base imponible"),
            r(2, "4 % · cuota"),
            r(3, "10 % · base imponible"),
            r(4, "10 % · cuota"),
            f(5, "21 % · base imponible"),
            f(6, "21 % · cuota"),
            r(21, "Adquisiciones intracomunitarias de bienes 4 % · base"),
            r(22, "Adquisiciones intracomunitarias de bienes 4 % · cuota"),
            r(23, "Adquisiciones intracomunitarias de bienes 10 % · base"),
            r(24, "Adquisiciones intracomunitarias de bienes 10 % · cuota"),
            r(25, "Adquisiciones intracomunitarias de bienes 21 % · base"),
            r(26, "Adquisiciones intracomunitarias de bienes 21 % · cuota"),
            r(545, "Adquisiciones intracomunitarias de servicios 4 % · base"),
            r(546, "Adquisiciones intracomunitarias de servicios 4 % · cuota"),
            r(547, "Adquisiciones intracomunitarias de servicios 10 % · base"),
            r(548, "Adquisiciones intracomunitarias de servicios 10 % · cuota"),
            f(551, "Adquisiciones intracomunitarias de servicios 21 % · base"),
            f(552, "Adquisiciones intracomunitarias de servicios 21 % · cuota"),
            f(27, "Otras operaciones con inversión del sujeto pasivo · base"),
            f(28, "Otras operaciones con inversión del sujeto pasivo · cuota"),
            r(29, "Modificación de bases y cuotas · base"),
            r(30, "Modificación de bases y cuotas · cuota"),
            t(33, "Total bases imponibles"),
            t(34, "Total cuotas"),
            t(47, "Total cuotas de IVA y recargo de equivalencia"),
        ],
    ),
    (
        "IVA deducible",
        &[
            r(190, "Interiores corrientes 4 % · base"),
            r(191, "Interiores corrientes 4 % · cuota"),
            r(603, "Interiores corrientes 10 % · base"),
            r(604, "Interiores corrientes 10 % · cuota"),
            f(605, "Interiores corrientes 21 % · base"),
            f(606, "Interiores corrientes 21 % · cuota"),
            f(48, "Total interiores corrientes · base"),
            f(49, "Total interiores corrientes · cuota"),
            r(196, "Interiores de bienes de inversión 4 % · base"),
            r(197, "Interiores de bienes de inversión 4 % · cuota"),
            r(611, "Interiores de bienes de inversión 10 % · base"),
            r(612, "Interiores de bienes de inversión 10 % · cuota"),
            r(613, "Interiores de bienes de inversión 21 % · base"),
            r(614, "Interiores de bienes de inversión 21 % · cuota"),
            r(50, "Total interiores de bienes de inversión · base"),
            r(51, "Total interiores de bienes de inversión · cuota"),
            r(214, "Intracomunitarias de bienes corrientes 4 % · base"),
            r(215, "Intracomunitarias de bienes corrientes 4 % · cuota"),
            r(627, "Intracomunitarias de bienes corrientes 10 % · base"),
            r(628, "Intracomunitarias de bienes corrientes 10 % · cuota"),
            r(629, "Intracomunitarias de bienes corrientes 21 % · base"),
            r(630, "Intracomunitarias de bienes corrientes 21 % · cuota"),
            r(56, "Total intracomunitarias de bienes corrientes · base"),
            r(57, "Total intracomunitarias de bienes corrientes · cuota"),
            r(220, "Intracomunitarias de bienes de inversión 4 % · base"),
            r(221, "Intracomunitarias de bienes de inversión 4 % · cuota"),
            r(631, "Intracomunitarias de bienes de inversión 10 % · base"),
            r(632, "Intracomunitarias de bienes de inversión 10 % · cuota"),
            r(633, "Intracomunitarias de bienes de inversión 21 % · base"),
            r(634, "Intracomunitarias de bienes de inversión 21 % · cuota"),
            r(58, "Total intracomunitarias de bienes de inversión · base"),
            r(59, "Total intracomunitarias de bienes de inversión · cuota"),
            r(587, "Intracomunitarias de servicios 4 % · base"),
            r(588, "Intracomunitarias de servicios 4 % · cuota"),
            r(635, "Intracomunitarias de servicios 10 % · base"),
            r(636, "Intracomunitarias de servicios 10 % · cuota"),
            r(637, "Intracomunitarias de servicios 21 % · base"),
            r(638, "Intracomunitarias de servicios 21 % · cuota"),
            r(597, "Total intracomunitarias de servicios · base"),
            r(598, "Total intracomunitarias de servicios · cuota"),
            t(64, "Suma de deducciones"),
            t(65, "Resultado régimen general (47 − 64)"),
        ],
    ),
    (
        "liquidación anual",
        &[
            f(84, "Suma de resultados"),
            f(85, "Compensación de cuotas del ejercicio anterior"),
            t(86, "Resultado de la liquidación (84 − 85)"),
        ],
    ),
    (
        "resultado de las liquidaciones del año",
        &[
            f(95, "Total a ingresar en tus 303 del año"),
            f(97, "A compensar: el resultado del último 303"),
            f(98, "A devolver: el resultado del último 303"),
            f(662, "A compensar de los demás 303 y aún sin aplicar"),
        ],
    ),
    (
        "volumen de operaciones",
        &[
            f(99, "Operaciones en régimen general"),
            f(103, "Entregas y servicios intracomunitarios (a empresas de la UE)"),
            f(104, "Exportaciones y otras exentas con derecho a deducción"),
            f(105, "Operaciones exentas sin derecho a deducción"),
            f(110, "No sujetas por reglas de localización (clientes fuera de la UE)"),
            r(125, "Sujetas con inversión del sujeto pasivo"),
            t(108, "Total volumen de operaciones"),
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
    con_tipo(fila.nombre, fila, columnas)
}

/// El nombre corto de la fila, si lo tiene (si no, el oficial): para cuando el oficial no cabe.
pub fn nombre_corto(fila: &Fila, columnas: &[&Casillas]) -> String {
    con_tipo(fila.corto.unwrap_or(fila.nombre), fila, columnas)
}

/// `texto` con el tipo de IVA de la fila en su «{t}» (sin él, si no se sabe).
fn con_tipo(texto: &str, fila: &Fila, columnas: &[&Casillas]) -> String {
    let Some(k) = fila.tipo else { return texto.to_string() };
    let pct = columnas
        .iter()
        .map(|c| c.v(k))
        .find(|x| x.abs() > DIF)
        .or_else(|| TIPO_FIJO.iter().find(|(n, _)| *n == k).map(|(_, x)| *x));
    match pct {
        Some(x) => texto.replace("{t}", &fmt::pct(x)),
        None => texto.replace(" al {t}", "").replace(" {t}", ""),
    }
}

/// «07», «110» (como en los formularios de Hacienda).
pub fn numero(n: u32) -> String {
    if n < 100 { format!("{n:02}") } else { n.to_string() }
}

/// El nombre de una casilla de unos grupos («Régimen general 21 % · cuota»; «Casilla 72» si no está en la lista).
pub fn nombre_de(grupos: &[Grupo], n: u32, columnas: &[&Casillas]) -> String {
    match grupos.iter().flat_map(|(_, filas)| filas.iter()).find(|x| x.n == n) {
        Some(fila) => nombre(fila, columnas),
        None => format!("Casilla {}", numero(n)),
    }
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
        // El corto, con su tipo; sin corto, el oficial.
        let fila = G303[0].1.iter().find(|x| x.n == 7).unwrap();
        assert_eq!(nombre_corto(fila, &[&c]), "Base imponible al 21 %");
        let fila = G130[0].1.iter().find(|x| x.n == 3).unwrap();
        assert_eq!(nombre_corto(fila, &[&c]), "Rendimiento neto");
        let fila = G130[0].1.iter().find(|x| x.n == 1).unwrap();
        assert_eq!(nombre_corto(fila, &[&c]), "Ingresos computables");
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
    fn nombres_de_casillas() {
        let c = Casillas::new();
        assert_eq!(nombre_de(G303, 120, &[&c]), "Operaciones no sujetas por reglas de localización (clientes fuera de la UE)");
        assert_eq!(nombre_de(G303, 72, &[&c]), "Casilla 72");
        assert_eq!(nombre_de(G130, 2, &[&c]), "Gastos fiscalmente deducibles");
        assert_eq!(nombre_de(G390, 662, &[&c]), "A compensar de los demás 303 y aún sin aplicar");
    }
}
