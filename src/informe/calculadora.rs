//! La calculadora de sueldos: de un sueldo bruto, lo que le cuestas a tu empresa y lo que te queda; y de lo que
//! factura un autónomo, su cuota, su IRPF y lo que le queda. Con las reglas de un año (las cotizaciones, de
//! `tgss::reglas`; el IRPF, de `hacienda::irpf`), sin hijos ni discapacidad y sin otras rentas. Y lo mismo en cada
//! comunidad, para compararlas.
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
/// tipo. Devuelve (empresa, trabajador). Se reparte como las contingencias comunes (art. 19 bis de la LGSS), con el
/// tipo del trabajador redondeado a dos decimales, como en el art. 17 de la Orden de cotización (en 2026, el 0,19, el
/// 0,21 y el 0,24 %; en 2025, el 0,15, el 0,17 y el 0,19 %); el resto, de la empresa.
fn solidaridad(g: &reglas::General, mensual: f64) -> (f64, f64) {
    let max = g.base_maxima;
    let del_trabajador = g.comunes.1 / (g.comunes.0 + g.comunes.1);
    let mut desde = 1.0;
    let (mut empresa, mut trabajador) = (0.0, 0.0);
    for &(hasta, tipo) in g.solidaridad {
        let parte = (mensual.min(max * hasta) - max * desde).max(0.0);
        let t = r2(tipo * del_trabajador);
        trabajador += parte * t / 100.0;
        empresa += parte * (tipo - t) / 100.0;
        desde = hasta;
    }
    (empresa, trabajador)
}

/// Los años que la calculadora tiene enteros (las cotizaciones de asalariados y de autónomos y el IRPF de ese mismo
/// año), del primero de la app al que viene: solo esos se pueden elegir (con las reglas de otro año, el resultado no
/// sería el suyo).
pub fn anios_con_reglas() -> Vec<i32> {
    (crate::dominio::PRIMER_ANIO..=crate::dominio::fmt::anio_actual() + 1)
        .filter(|&y| reglas::general(y).anio == y && reglas::autonomos(y).anio == y && irpf::reglas(y, "").anio == y)
        .collect()
}

/// El año con que se abre la calculadora: el último que tiene entero hasta el de hoy (si no, el primero que tenga).
pub fn anio_por_defecto() -> i32 {
    let v = anios_con_reglas();
    let hoy = crate::dominio::fmt::anio_actual();
    v.iter().rev().find(|&&y| y <= hoy).or(v.first()).copied().unwrap_or(hoy)
}

/// Un sueldo bruto de un año entero (con las pagas extra), con un contrato indefinido o temporal, en una
/// comunidad (su clave; "" si no se sabe).
pub fn asalariado(bruto: f64, temporal: bool, comunidad: &str, anio: i32) -> Asalariado {
    let g = reglas::general(anio);
    let mensual = bruto / 12.0;
    let base = mensual.min(g.base_maxima);
    let anual = base * 12.0;
    let desempleo = if temporal { g.desempleo_temporal } else { g.desempleo_indefinido };
    let (sol_empresa, sol_trabajador) = solidaridad(g, mensual);
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
    // La solidaridad, desde 2025 (antes, por encima de la base máxima no se cotizaba).
    if mensual > g.base_maxima && !g.solidaridad.is_empty() {
        let sol = |mes: f64| Concepto { nombre: "Cotización de solidaridad".into(), tipo: None, importe: r2(mes * 12.0) };
        empresa.push(sol(sol_empresa));
        trabajador.push(sol(sol_trabajador));
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
    /// Los rendimientos al mes con los que se elige el tramo (menos los gastos genéricos), el tramo y sus bases
    /// mínima y máxima (al mes).
    pub mensual_ss: f64,
    pub gastos_genericos: f64,
    /// ¿Con tramos? (desde 2023; antes, la base mínima para todos y sin gastos genéricos).
    pub con_tramos: bool,
    pub tramo: usize,
    pub base_minima: f64,
    pub base_maxima: f64,
    /// La base con que cotizas (la mínima, o la de la cuota que elijas) y el tipo.
    pub base: f64,
    pub tipo: f64,
    /// La cuota al mes, la mínima de tu tramo (sin tarifa plana) y si es la tarifa plana.
    pub cuota_mes: f64,
    pub cuota_minima: f64,
    pub tarifa_plana: bool,
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

/// Cómo cotiza un autónomo: la base que elige (None: la mínima de su tramo; por debajo de ella, la mínima, y por
/// encima de la máxima, la máxima; ver `reglas::Autonomos::bases`) y si tiene la tarifa plana (la de su primer año).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Cotiza {
    pub base: Option<f64>,
    pub tarifa_plana: bool,
}

/// Un autónomo persona física (no societario), con la misma base todo el año: la mínima de su tramo o la que elija;
/// o, con la tarifa plana, la de su primer año.
pub fn autonomo(facturado: f64, gastos: f64, cotiza: Cotiza, comunidad: &str, anio: i32) -> Autonomo {
    let a = reglas::autonomos(anio);
    let antes = r2(facturado - gastos);
    let mensual_ss = r2(antes.max(0.0) * (1.0 - a.gastos_genericos / 100.0) / 12.0);
    let (tramo, base_minima, base_maxima) = a.tramo(mensual_ss);
    // La cuota, como en el recibo: la base por el tipo total, cada mes.
    let al_mes = |base: f64| r2(base * a.tipo / 100.0);
    // La tarifa plana, si ese año la hay.
    let tarifa_plana = a.tarifa_plana.filter(|_| cotiza.tarifa_plana);
    let base = match cotiza.base {
        Some(b) if tarifa_plana.is_none() => b.clamp(base_minima, base_maxima),
        _ => base_minima,
    };
    let (cuota_mes, cuota) = if let Some(t) = tarifa_plana {
        let c = Concepto { nombre: "Tarifa plana".into(), tipo: None, importe: r2(t * 12.0) };
        (t, vec![c])
    } else {
        (al_mes(base), a.desglose.iter().map(|(n, t)| Concepto::new(n, *t, base * 12.0)).collect())
    };
    let ss = r2(cuota_mes * 12.0);
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
        con_tramos: a.tramos.len() > 1,
        tramo,
        base_minima,
        base_maxima,
        base,
        tipo: a.tipo,
        cuota_mes,
        cuota_minima: al_mes(base_minima),
        tarifa_plana: tarifa_plana.is_some(),
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

/// Lo que te queda en una comunidad.
#[derive(Debug, Clone, PartialEq)]
pub struct EnComunidad {
    pub clave: &'static str,
    pub nombre: &'static str,
    pub irpf: f64,
    pub neto: f64,
}

/// Las comunidades de las que la app tiene la escala autonómica (las de territorio común: sin País Vasco ni
/// Navarra, con su IRPF foral, ni Ceuta y Melilla, con su deducción), cada una con lo que te queda: de más a menos.
fn por_comunidad(anio: i32, calcula: impl Fn(&str) -> (f64, f64)) -> Vec<EnComunidad> {
    let mut v: Vec<EnComunidad> = irpf::COMUNIDADES
        .iter()
        .filter(|(c, _)| irpf::reglas(anio, c).autonomica.is_some())
        .map(|&(clave, nombre)| {
            let (irpf, neto) = calcula(clave);
            EnComunidad { clave, nombre, irpf, neto }
        })
        .collect();
    v.sort_by(|a, b| b.neto.total_cmp(&a.neto).then_with(|| a.nombre.cmp(b.nombre)));
    v
}

/// Un sueldo en cada comunidad (ver `asalariado`).
pub fn asalariado_por_comunidad(bruto: f64, temporal: bool, anio: i32) -> Vec<EnComunidad> {
    por_comunidad(anio, |c| {
        let s = asalariado(bruto, temporal, c, anio);
        (s.irpf.total, s.neto)
    })
}

/// Un autónomo en cada comunidad (ver `autonomo`).
pub fn autonomo_por_comunidad(facturado: f64, gastos: f64, cotiza: Cotiza, anio: i32) -> Vec<EnComunidad> {
    por_comunidad(anio, |c| {
        let a = autonomo(facturado, gastos, cotiza, c, anio);
        (a.irpf.total, a.neto)
    })
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
        // Al mes: 510,12 × 1,15 % + 2.040,48 × 1,25 % + 2.348,20 × 1,46 % = 5,87 + 25,51 + 34,28 = 65,66 €. De ello,
        // con los tipos de la Orden PJC/297/2026 (art. 17), el trabajador paga 510,12 × 0,19 % + 2.040,48 × 0,21 %
        // + 2.348,20 × 0,24 % = 10,89 € y la empresa (0,96, 1,04 y 1,22 %), 54,77 €.
        let (e, t) = solidaridad(reglas::general(2026), 10000.0);
        assert!((e + t - 65.66).abs() < 0.01, "{e} + {t}");
        assert_eq!((r2(e), r2(t)), (54.77, 10.89));
        let anual = |v: &[Concepto]| v.iter().find(|c| c.nombre.contains("solidaridad")).unwrap().importe;
        assert_eq!((anual(&s.empresa), anual(&s.trabajador)), (657.19, 130.68));
        // Los tipos del trabajador, tramo a tramo, son los del art. 17 de cada Orden (PJC/178/2025 y PJC/297/2026).
        for (anio, tipos) in [(2025, [0.15, 0.17, 0.19]), (2026, [0.19, 0.21, 0.24])] {
            let g = reglas::general(anio);
            let t = |veces: f64| solidaridad(g, g.base_maxima * veces).1;
            let tramos = [t(1.1), t(1.5) - t(1.1), t(2.0) - t(1.5)];
            let anchos = [0.1, 0.4, 0.5];
            for i in 0..3 {
                assert!((tramos[i] - g.base_maxima * anchos[i] * tipos[i] / 100.0).abs() < 1e-9, "{anio}, tramo {}", i + 1);
            }
        }
        // Las demás, sobre la base máxima: 5.101,20 × 12 × 4,70 % = 2.877,08 €.
        assert_eq!(s.trabajador[0].importe, 2877.08);
        // En 2025 no hay solidaridad por debajo de su base máxima; antes de 2025, tampoco por encima: no se cotiza.
        assert_eq!(asalariado(50000.0, false, "", 2025).trabajador.len(), 4);
        let s = asalariado(120000.0, false, "madrid", 2024);
        assert!(s.tope && s.anio_ss == 2024 && s.trabajador.len() == 4 && s.empresa.len() == 6, "{s:?}");
        assert_eq!(s.trabajador[0].importe, r2(4720.50 * 12.0 * 0.047));
    }

    /// 100.000 € y 30.000 € en Andalucía, indefinido, con las cifras de la Orden PJC/297/2026 (cotización de 2026),
    /// la Ley 35/2006 del IRPF (escala estatal del art. 63.1, mínimo de 5.550 € del art. 57, 2.000 € de otros gastos
    /// del art. 19.2.f) y la Ley andaluza 5/2021 (escala del art. 23 y mínimo de 5.790 € del art. 23 bis), que no
    /// cambian de 2025 a 2026.
    #[test]
    fn en_andalucia_con_las_cifras_oficiales() {
        // 100.000 / 12 = 8.333,33 € al mes: pasa de la base máxima, 5.101,20 € (× 12 = 61.214,40 € al año).
        let s = asalariado(100000.0, false, "andalucia", 2026);
        let importes = |v: &[Concepto]| v.iter().map(|c| c.importe).collect::<Vec<_>>();
        // Trabajador: 61.214,40 × 4,70 % = 2.877,08; × 1,55 % = 948,82; × 0,10 % = 61,21; × 0,15 % (MEI) = 91,82;
        // solidaridad: (510,12 × 0,19 % + 2.040,48 × 0,21 % + 681,53 × 0,24 %) × 12 = 6,89 × 12 = 82,68 €.
        assert_eq!(importes(&s.trabajador), [2877.08, 948.82, 61.21, 91.82, 82.68]);
        assert_eq!(s.ss_trabajador, 4061.61);
        // Empresa: 23,60 % = 14.446,60; 5,50 % = 3.366,79; 0,20 % = 122,43; 0,60 % = 367,29; 0,75 % = 459,11;
        // 1,50 % = 918,22; solidaridad (0,96, 1,04 y 1,22 %): 34,43 × 12 = 413,19 €.
        assert_eq!(importes(&s.empresa), [14446.6, 3366.79, 122.43, 367.29, 459.11, 918.22, 413.19]);
        assert_eq!(s.coste, 120093.63);
        // IRPF: 100.000 − 4.061,61 − 2.000 = 93.938,39 € de base (sin reducción: pasa de 19.747,50 €).
        // Estatal: 8.950,75 + 33.938,39 × 22,5 % = 16.586,89, menos 5.550 × 9,5 % = 527,25 → 16.059,64 €.
        // Andaluza: 8.910 + 33.938,39 × 22,5 % = 16.546,14, menos 5.790 × 9,5 % = 550,05 → 15.996,09 €.
        assert_eq!((s.irpf.base, s.irpf.estatal, s.irpf.autonomica, s.irpf.deduccion), (93938.39, 16059.64, 15996.09, 0.0));
        assert_eq!(s.irpf.total, 32055.73);
        assert_eq!(s.neto, 63882.66);
        // En 2025 (Orden PJC/178/2025): base máxima 4.909,50 €, MEI del 0,13 % y solidaridad del 0,15, 0,17 y 0,19 %:
        // 2.768,96 + 913,17 + 58,91 + 76,59 + 5,92 × 12 = 70,99 → 3.888,62 €.
        assert_eq!(asalariado(100000.0, false, "andalucia", 2025).ss_trabajador, 3888.62);
        // 30.000 €, sin tope: 30.000 × 6,50 % = 1.950 € y 26.050 € de base. Estatal: 2.112,75 + 5.850 × 15 % − 527,25
        // = 2.463 €; andaluza: 2.207 + 4.950 × 15 % − 550,05 = 2.399,45 €.
        let s = asalariado(30000.0, false, "andalucia", 2026);
        assert_eq!((s.ss_trabajador, s.irpf.base, s.irpf.estatal, s.irpf.autonomica, s.irpf.total), (1950.0, 26050.0, 2463.0, 2399.45, 4862.45));
        assert_eq!(s.neto, 23187.55);
    }

    #[test]
    fn en_cada_comunidad() {
        let v = asalariado_por_comunidad(30000.0, false, 2025);
        // Las quince de territorio común, de más a menos; la Seguridad Social es la misma en todas.
        assert_eq!(v.len(), 15);
        assert!(v.windows(2).all(|w| w[0].neto >= w[1].neto));
        assert!(!v.iter().any(|c| ["pais-vasco", "navarra", "ceuta", "melilla"].contains(&c.clave)));
        let m = v.iter().find(|c| c.clave == "madrid").unwrap();
        let s = asalariado(30000.0, false, "madrid", 2025);
        assert_eq!((m.irpf, m.neto), (s.irpf.total, s.neto));
        assert!(v.iter().all(|c| (c.neto + c.irpf - (s.neto + s.irpf.total)).abs() < 0.01), "solo cambia el IRPF");
        // Con 30.000 €, Madrid es de las que menos IRPF cobran: más que en Cataluña te queda.
        let cat = v.iter().find(|c| c.clave == "cataluna").unwrap();
        assert!(m.neto > cat.neto, "{} {}", m.neto, cat.neto);
        let a = autonomo_por_comunidad(40000.0, 5000.0, Cotiza::default(), 2025);
        assert_eq!(a.len(), 15);
        assert!(a.windows(2).all(|w| w[0].neto >= w[1].neto));
    }

    /// Se puede cotizar por más que la mínima: con la base que se elija, entre la mínima y la máxima del tramo (por
    /// debajo de la mínima, la mínima; por encima de la máxima, la máxima). Más cuota, menos IRPF.
    #[test]
    fn una_cuota_mas_alta() {
        let con = |base: Option<f64>| autonomo(40000.0, 5000.0, Cotiza { base, tarifa_plana: false }, "madrid", 2026);
        let min = con(None);
        assert_eq!((min.base, min.cuota_mes), (min.base_minima, min.cuota_minima));
        let a = con(Some(1928.10));
        assert_eq!((a.base, a.cuota_mes), (1928.10, r2(1928.10 * 0.315)));
        assert_eq!(a.cuota_minima, min.cuota_minima);
        assert_eq!(a.ss, r2(a.cuota_mes * 12.0));
        assert!(a.irpf.total < min.irpf.total, "más cuota, menos IRPF");
        assert_eq!(con(Some(10.0)).cuota_mes, min.cuota_mes);
        let mucha = con(Some(1e6));
        assert_eq!(mucha.base, mucha.base_maxima);
    }

    #[test]
    fn un_autonomo() {
        // 40.000 € facturados y 5.000 € de gastos: 35.000 × 0,93 / 12 = 2.712,50 € al mes → tramo 10 (2.330–2.760).
        let a = autonomo(40000.0, 5000.0, Cotiza::default(), "madrid", 2026);
        assert_eq!((a.tramo, a.base), (10, 1356.21));
        assert_eq!(a.ss, r2(r2(1356.21 * 0.315) * 12.0));
        // La cuota por conceptos suma lo mismo (salvo céntimos).
        assert!((suma(&a.cuota) - a.ss).abs() < 0.2, "{} {}", suma(&a.cuota), a.ss);
        assert_eq!(a.rendimiento, r2(35000.0 - a.ss));
        assert_eq!(a.gdj, r2(a.rendimiento * 0.05), "el 5 % (hasta 2.000 €)");
        assert_eq!(autonomo(80000.0, 0.0, Cotiza::default(), "", 2026).gdj, 2000.0, "hasta 2.000 €");
        assert_eq!(a.reduccion, 0.0, "con más de 12.000 € no hay reducción");
        assert_eq!(a.neto, r2(35000.0 - a.ss - a.irpf.total));
        // Con poco, la reducción de 1.620 €.
        let p = autonomo(9000.0, 0.0, Cotiza::default(), "madrid", 2026);
        assert_eq!(p.tramo, 2, "9.000 × 0,93 / 12 = 697,50 € al mes: tabla reducida");
        assert!(p.reduccion > 0.0 && p.irpf.total < 200.0, "{p:?}");
    }
}
