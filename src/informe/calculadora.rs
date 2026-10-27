//! La calculadora de sueldos: de un sueldo bruto, lo que le cuestas a tu empresa y lo que te queda; y de lo que
//! factura un autónomo, su cuota, su IRPF y lo que le queda. Con las reglas de un año (las cotizaciones, de
//! `tgss::reglas`; el IRPF, de `hacienda::irpf`), sin hijos ni discapacidad y sin otras rentas.
use crate::dominio::fmt::r2;
use crate::hacienda::irpf::{self, Cuota};
use crate::tgss::reglas::{self, AT_EP_OFICINA};

/// Una línea de lo cotizado: nombre, tipo (%) e importe al año.
#[derive(Debug, Clone, PartialEq)]
pub struct Concepto {
    pub nombre: String,
    pub tipo: Option<f64>,
    pub importe: f64,
}

impl Concepto {
    fn new(nombre: &str, tipo: f64, base: f64) -> Concepto {
        Concepto { nombre: nombre.into(), tipo: Some(tipo), importe: r2(base * tipo / 100.0) }
    }
}

/// Lo que se suma de unas líneas.
fn suma(v: &[Concepto]) -> f64 {
    r2(v.iter().map(|c| c.importe).sum())
}

/// Un sueldo, de lo que paga la empresa a lo que te queda.
#[derive(Debug, Clone, PartialEq)]
pub struct Asalariado {
    /// Los años de las reglas: de la Seguridad Social y del IRPF (si la app no tiene las del año pedido, las del
    /// más cercano).
    pub anio_ss: i32,
    pub anio_irpf: i32,
    pub norma: &'static str,
    pub bruto: f64,
    /// Lo que paga la empresa a la Seguridad Social, por conceptos.
    pub empresa: Vec<Concepto>,
    pub ss_empresa: f64,
    /// Lo que le cuestas: tu bruto más lo de la empresa.
    pub coste: f64,
    /// Tu parte de la Seguridad Social, por conceptos.
    pub trabajador: Vec<Concepto>,
    pub ss_trabajador: f64,
    pub irpf: Cuota,
    pub neto: f64,
    /// La base de cotización al mes y si el sueldo pasa de la máxima.
    pub base_mensual: f64,
    pub tope: bool,
    /// Sin reglas de tu comunidad: la escala estatal hace de autonómica.
    pub sin_comunidad: bool,
}

impl Asalariado {
    /// Todo lo que se llevan impuestos y cotizaciones (de la empresa y tuyas).
    pub fn impuestos(&self) -> f64 {
        r2(self.coste - self.neto)
    }
}

/// La cotización de solidaridad de un mes: por lo que el sueldo del mes pasa de la base máxima, a cada tramo su
/// tipo (total; luego se reparte).
fn solidaridad(g: &reglas::General, mensual: f64) -> f64 {
    let max = g.base_maxima;
    let mut desde = 1.0;
    let mut total = 0.0;
    for &(hasta, tipo) in g.solidaridad {
        let parte = (mensual.min(max * hasta) - max * desde).max(0.0);
        total += parte * tipo / 100.0;
        desde = hasta;
    }
    total
}

/// Un sueldo bruto de un año entero (con las pagas extra), con un contrato indefinido o temporal, en una
/// comunidad (su clave; "" si no se sabe).
pub fn asalariado(bruto: f64, temporal: bool, comunidad: &str, anio: i32) -> Asalariado {
    let g = reglas::general(anio);
    let mensual = bruto / 12.0;
    let base = mensual.min(g.base_maxima);
    let anual = base * 12.0;
    let desempleo = if temporal { g.desempleo_temporal } else { g.desempleo_indefinido };
    // La de solidaridad se reparte como las contingencias comunes.
    let sol = solidaridad(g, mensual) * 12.0;
    let parte_empresa = g.comunes.0 / (g.comunes.0 + g.comunes.1);
    let mut empresa = vec![
        Concepto::new("Contingencias comunes (pensiones, bajas…)", g.comunes.0, anual),
        Concepto::new("Desempleo", desempleo.0, anual),
        Concepto::new("Fondo de Garantía Salarial (FOGASA)", g.fogasa, anual),
        Concepto::new("Formación profesional", g.formacion.0, anual),
        Concepto::new("Equidad intergeneracional (MEI)", g.mei.0, anual),
        Concepto::new("Accidentes de trabajo (oficina)", AT_EP_OFICINA, anual),
    ];
    let mut trabajador = vec![
        Concepto::new("Contingencias comunes (pensiones, bajas…)", g.comunes.1, anual),
        Concepto::new("Desempleo", desempleo.1, anual),
        Concepto::new("Formación profesional", g.formacion.1, anual),
        Concepto::new("Equidad intergeneracional (MEI)", g.mei.1, anual),
    ];
    if sol > 0.005 {
        let e = r2(sol * parte_empresa);
        empresa.push(Concepto { nombre: "Cotización de solidaridad".into(), tipo: None, importe: e });
        trabajador.push(Concepto { nombre: "Cotización de solidaridad".into(), tipo: None, importe: r2(r2(sol) - e) });
    }
    let (ss_empresa, ss_trabajador) = (suma(&empresa), suma(&trabajador));
    let r = irpf::reglas(anio, comunidad);
    let cuota = irpf::cuota_sueldo(&r, bruto, ss_trabajador);
    Asalariado {
        anio_ss: g.anio,
        anio_irpf: r.anio,
        norma: g.norma,
        bruto,
        coste: r2(bruto + ss_empresa),
        empresa,
        ss_empresa,
        trabajador,
        ss_trabajador,
        neto: r2(bruto - ss_trabajador - cuota.total),
        irpf: cuota,
        base_mensual: r2(base),
        tope: mensual > g.base_maxima,
        sin_comunidad: r.autonomica.is_none(),
    }
}

/// Un autónomo de un año entero: lo que factura y gasta, su cuota y su IRPF.
#[derive(Debug, Clone, PartialEq)]
pub struct Autonomo {
    pub anio_ss: i32,
    pub anio_irpf: i32,
    pub norma: &'static str,
    /// Lo facturado sin IVA, y los gastos de la actividad sin la cuota.
    pub facturado: f64,
    pub gastos: f64,
    /// Los rendimientos al mes con los que se elige el tramo (menos los gastos genéricos), el tramo y su base
    /// mínima (al mes).
    pub mensual_ss: f64,
    pub gastos_genericos: f64,
    pub tramo: usize,
    pub base: f64,
    pub tipo: f64,
    /// La cuota del año, por conceptos.
    pub cuota: Vec<Concepto>,
    pub ss: f64,
    /// El rendimiento neto (con la cuota), la difícil justificación y la reducción por rentas bajas.
    pub rendimiento: f64,
    pub gdj: f64,
    pub reduccion: f64,
    pub irpf: Cuota,
    pub neto: f64,
    pub sin_comunidad: bool,
}

impl Autonomo {
    /// Lo que ganas antes de cuota e IRPF: lo facturado menos los gastos.
    pub fn antes(&self) -> f64 {
        r2(self.facturado - self.gastos)
    }
    pub fn impuestos(&self) -> f64 {
        r2(self.ss + self.irpf.total)
    }
}

/// Un autónomo persona física (no societario), con la base mínima de su tramo todo el año.
pub fn autonomo(facturado: f64, gastos: f64, comunidad: &str, anio: i32) -> Autonomo {
    let a = reglas::autonomos(anio);
    let antes = r2(facturado - gastos);
    let mensual_ss = r2(antes.max(0.0) * (1.0 - a.gastos_genericos / 100.0) / 12.0);
    let (tramo, base) = a.tramo(mensual_ss);
    let cuota: Vec<Concepto> = a.desglose.iter().map(|(n, t)| Concepto::new(n, *t, base * 12.0)).collect();
    // La cuota, como en el recibo: la base por el tipo total, cada mes.
    let ss = r2(r2(base * a.tipo / 100.0) * 12.0);
    let r = irpf::reglas(anio, comunidad);
    let rendimiento = r2(antes - ss);
    let (gdj, reduccion, _, cuota_irpf) = irpf::cuota_actividad(&r, rendimiento);
    Autonomo {
        anio_ss: a.anio,
        anio_irpf: r.anio,
        norma: a.norma,
        facturado,
        gastos,
        mensual_ss,
        gastos_genericos: a.gastos_genericos,
        tramo,
        base,
        tipo: a.tipo,
        cuota,
        ss,
        rendimiento,
        gdj,
        reduccion,
        neto: r2(antes - ss - cuota_irpf.total),
        irpf: cuota_irpf,
        sin_comunidad: r.autonomica.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_sueldo_de_30000_en_2026() {
        let s = asalariado(30000.0, false, "madrid", 2026);
        // Empresa: 23,60 + 5,50 + 0,20 + 0,60 + 0,75 + 1,50 = 32,15 %.
        assert_eq!(s.ss_empresa, 9645.0);
        assert_eq!(s.coste, 39645.0);
        // Trabajador: 4,70 + 1,55 + 0,10 + 0,15 = 6,50 %.
        assert_eq!(s.ss_trabajador, 1950.0);
        assert!(!s.tope && s.trabajador.len() == 4, "sin solidaridad");
        assert_eq!(s.neto, r2(30000.0 - 1950.0 - s.irpf.total));
        // IRPF: 30.000 − 1.950 − 2.000 de otros gastos = 26.050 € de base (sin reducción: pasa de 19.747,50 €).
        // Estatal: 2.112,75 + (26.050 − 20.200) × 15 % − 527,25 del mínimo = 2.463 €; con la de Madrid, unos 4.600 €.
        assert_eq!(s.irpf.base, 26050.0);
        assert_eq!(s.irpf.estatal, 2463.0);
        assert!(s.irpf.total > 4400.0 && s.irpf.total < 4800.0, "{}", s.irpf.total);
        assert_eq!(s.impuestos(), r2(s.coste - s.neto));
    }

    #[test]
    fn por_encima_de_la_base_maxima_hay_solidaridad() {
        // 120.000 € al año: 10.000 € al mes; la base máxima de 2026, 5.101,20 €.
        let s = asalariado(120000.0, false, "madrid", 2026);
        assert!(s.tope);
        assert_eq!(s.base_mensual, 5101.20);
        // Al mes: 510,12 × 1,15 % + 2.040,48 × 1,25 % + 2.348,20 × 1,46 % = 5,87 + 25,51 + 34,28 = 65,66 €.
        let sol = solidaridad(reglas::general(2026), 10000.0);
        assert!((sol - 65.66).abs() < 0.01, "{sol}");
        let e = s.empresa.iter().find(|c| c.nombre.contains("solidaridad")).unwrap().importe;
        let t = s.trabajador.iter().find(|c| c.nombre.contains("solidaridad")).unwrap().importe;
        assert!((e + t - sol * 12.0).abs() < 0.02, "{e} + {t}");
        assert!((t / (e + t) - 4.70 / 28.30).abs() < 0.001, "se reparte como las contingencias comunes");
        // Las demás, sobre la base máxima: 5.101,20 × 12 × 4,70 % = 2.877,08 €.
        assert_eq!(s.trabajador[0].importe, 2877.08);
        // En 2025 no hay solidaridad por debajo de su base máxima, y la de 2024 no la tiene la app: usa la de 2025.
        assert_eq!(asalariado(50000.0, false, "", 2025).trabajador.len(), 4);
        assert_eq!(asalariado(50000.0, false, "", 2024).anio_ss, 2025);
    }

    #[test]
    fn un_autonomo() {
        // 40.000 € facturados y 5.000 € de gastos: 35.000 × 0,93 / 12 = 2.712,50 € al mes → tramo 10 (2.330–2.760).
        let a = autonomo(40000.0, 5000.0, "madrid", 2026);
        assert_eq!((a.tramo, a.base), (10, 1356.21));
        assert_eq!(a.ss, r2(r2(1356.21 * 0.315) * 12.0));
        // La cuota por conceptos suma lo mismo (salvo céntimos).
        assert!((suma(&a.cuota) - a.ss).abs() < 0.2, "{} {}", suma(&a.cuota), a.ss);
        assert_eq!(a.rendimiento, r2(35000.0 - a.ss));
        assert_eq!(a.gdj, r2(a.rendimiento * 0.05), "el 5 % (hasta 2.000 €)");
        assert_eq!(autonomo(80000.0, 0.0, "", 2026).gdj, 2000.0, "hasta 2.000 €");
        assert_eq!(a.reduccion, 0.0, "con más de 12.000 € no hay reducción");
        assert_eq!(a.neto, r2(35000.0 - a.ss - a.irpf.total));
        // Con poco, la reducción de 1.620 €.
        let p = autonomo(9000.0, 0.0, "madrid", 2026);
        assert_eq!(p.tramo, 2, "9.000 × 0,93 / 12 = 697,50 € al mes: tabla reducida");
        assert!(p.reduccion > 0.0 && p.irpf.total < 200.0, "{p:?}");
    }
}
