//! A dónde va lo que generas en un año, fuente a fuente (sueldo, actividad, alquileres, ahorro): a gastos, a la
//! Seguridad Social, a Hacienda o a ti; y cuánto se lleva el Estado de cada una. Sale de las mismas cifras que el
//! balance del año (`anio::detalle`).
//!
//! El IRPF del año se reparte así:
//! - Con Renta: a cada fuente, su parte de la cuota íntegra (antes de deducciones): la del ahorro, al ahorro; la de la
//!   base general, entre sueldo, actividad y alquileres según lo que aporta cada uno a ella (lo que cuenta en la
//!   Renta). Las deducciones (por invertir en empresas nuevas, donativos…) y los demás ajustes no son de ninguna
//!   fuente: van aparte, en el total.
//! - Sin Renta: lo adelantado; lo retenido en tu nómina, al sueldo, y lo demás (130, retenciones de tus clientes),
//!   a la actividad.
//!
//! El IVA de tu actividad (lo cobrado a tus clientes menos lo deducido de tus compras, según tus 303 leídos) va como
//! un tramo más de la actividad: entra con lo facturado y sale a Hacienda. No cuenta en lo que se lleva el Estado de
//! ti: lo paga tu cliente (si es una empresa, se lo deduce; si es un particular, puede que en parte salga de tu
//! margen, pero eso no se sabe).
use super::anio::{Concepto, Detalle};
use crate::dominio::fmt::{r2, DIF};
use crate::dominio::renta::Renta;
use crate::dominio::{Datos, Valor};
use crate::hacienda::Modelo;

/// Una fuente de dinero del año y a dónde fue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fuente {
    pub nombre: &'static str,
    /// Lo que entró: el sueldo con lo que pagó tu empresa por ti, lo facturado (sin IVA)…
    pub generado: f64,
    /// Los gastos de tu actividad (sin tu cuota de autónomo).
    pub gastos: f64,
    /// El IVA que pasó por tus manos a Hacienda (cobrado menos deducido).
    pub iva: f64,
    pub seguridad_social: f64,
    pub irpf: f64,
    /// Lo que te quedó: lo generado menos todo lo demás (negativo si pagaste más de lo que entró).
    pub queda: f64,
}

impl Fuente {
    /// Lo que se llevó el Estado de ti: la Seguridad Social y el IRPF (el IVA no: lo paga tu cliente).
    pub fn impuestos(&self) -> f64 {
        r2(self.seguridad_social + self.irpf)
    }
    /// Sobre qué se mide lo que se lleva el Estado: lo generado menos los gastos y el IVA.
    pub fn base(&self) -> f64 {
        r2(self.generado - self.gastos - self.iva)
    }
    /// La parte de la base que se llevó el Estado (0,39 = 39 %), si hay base.
    pub fn tipo(&self) -> Option<f64> {
        (self.base() > DIF).then(|| self.impuestos() / self.base())
    }
    fn cerrar(mut self) -> Self {
        self.queda = r2(self.generado - self.gastos - self.iva - self.seguridad_social - self.irpf);
        self
    }
    fn con_algo(&self) -> bool {
        [self.generado, self.gastos, self.iva, self.seguridad_social, self.irpf].iter().any(|x| x.abs() > DIF)
    }
}

/// Lo de un año.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reparto {
    pub y: i32,
    /// Sueldo, actividad, alquileres y ahorro: las que tienen algo, en ese orden.
    pub fuentes: Vec<Fuente>,
    pub total: Fuente,
    /// Del sueldo: (bruto, lo que pagó tu empresa por ti).
    pub sueldo: Option<(f64, f64)>,
    /// Lo cotizado por tu nómina, por grupos de conceptos: (nombre, tu empresa, tú).
    pub ss_grupos: Vec<(&'static str, f64, f64)>,
    /// Lo retenido de IRPF en tus nóminas (con Renta, lo que dice ella).
    pub irpf_retenido: Option<f64>,
    /// De lo que te queda del sueldo, lo que fue en especie.
    pub especie: f64,
    /// De tu Seguridad Social: (tu parte de la nómina, tu cuota de autónomo).
    pub tu_ss: (f64, f64),
    /// Del ahorro: (intereses y dividendos, ganancias).
    pub ahorro: Option<(f64, f64)>,
    /// Con Renta: lo que restaron sus deducciones y lo que cambiaron otros ajustes (incrementos, doble imposición).
    /// Ya van en el IRPF del total; no en el de cada fuente.
    pub deducciones: f64,
    pub ajustes: f64,
    /// El IVA de tu actividad según tus 303 leídos: (cobrado a tus clientes, deducido de tus compras, cuántos 303).
    pub iva: Option<(f64, f64, usize)>,
    pub con_renta: bool,
}

pub const SUELDO: &str = "Sueldo";
pub const ACTIVIDAD: &str = "Como autónomo";
pub const OTRAS: &str = "Alquileres y otras";
pub const AHORRO: &str = "Ahorro";

/// A dónde fue lo de cada fuente en el año de `det` (con su Renta, si la hay).
pub fn reparto(d: &Datos, det: &Detalle, r: Option<&Renta>) -> Reparto {
    let a = &det.anio;
    let y = a.y;
    let empresa = a.empresa;
    let (facturado, gastos) = a.actividad.unwrap_or_default();
    let otras = a.de(Concepto::OtrasRentas);
    let (capital, ganancias) = (a.de(Concepto::Capital), a.de(Concepto::Ganancias));
    let irpf = det.irpf.total;

    // El IRPF de cada fuente.
    let (mut i_sueldo, mut i_actividad, mut i_otras, mut i_ahorro) = (0.0, 0.0, 0.0, 0.0);
    let (mut deducciones, mut ajustes) = (0.0, 0.0);
    match r.filter(|_| det.irpf.con_renta) {
        Some(r) => {
            // Cada base, con su cuota íntegra; si no se leyeron las cuotas, todo a la general.
            let en_general = match &r.cuotas {
                Some(q) => {
                    i_ahorro = r2(q.ahorro);
                    deducciones = r2(q.general + q.ahorro - q.liquida);
                    ajustes = r2(irpf - q.liquida);
                    r2(q.general)
                }
                None => irpf,
            };
            // Lo que aporta cada una a la base general, como cuenta en la Renta (lo negativo no aporta).
            let t = r.trabajo.as_ref().map_or(0.0, |t| t.neto_reducido.or(t.neto).unwrap_or(0.0)).max(0.0);
            let ac = r.actividad.as_ref().map_or(0.0, |x| x.neto_reducido.unwrap_or(x.neto)).max(0.0);
            let pesos = [t, ac, otras.max(0.0)];
            let suma: f64 = pesos.iter().sum();
            if suma > DIF {
                i_sueldo = r2(en_general * pesos[0] / suma);
                i_actividad = r2(en_general * pesos[1] / suma);
                i_otras = r2(en_general - i_sueldo - i_actividad);
            } else {
                i_otras = en_general;
            }
        }
        None => {
            i_sueldo = det.irpf.nomina.unwrap_or(0.0);
            i_actividad = r2(irpf - i_sueldo);
        }
    }

    // El IVA, de los 303 leídos del año: lo que fue a Hacienda (si te devolvieron, no hay tramo).
    let leidos: Vec<_> = (1..=4).filter_map(|q| Modelo::M303.leido(d, y, q)).collect();
    let iva = (!leidos.is_empty()).then(|| {
        let s = |k: u32| r2(leidos.iter().map(|l| l.casillas.v(k)).sum());
        (s(27), s(45), leidos.len())
    });

    let iva_neto = iva.map_or(0.0, |(cobrado, deducido, _)| r2(cobrado - deducido).max(0.0));
    let tu_ss = (a.de(Concepto::TuNomina), a.de(Concepto::Cuota));
    let fuentes: Vec<Fuente> = [
        Fuente { nombre: SUELDO, generado: r2(a.de(Concepto::Sueldo) + empresa), seguridad_social: r2(empresa + tu_ss.0), irpf: i_sueldo, ..Default::default() },
        Fuente { nombre: ACTIVIDAD, generado: r2(facturado + iva_neto), gastos, iva: iva_neto, seguridad_social: tu_ss.1, irpf: i_actividad, ..Default::default() },
        Fuente { nombre: OTRAS, generado: otras, irpf: i_otras, ..Default::default() },
        Fuente { nombre: AHORRO, generado: r2(capital + ganancias), irpf: i_ahorro, ..Default::default() },
    ]
    .into_iter()
    .filter(Fuente::con_algo)
    .map(Fuente::cerrar)
    .collect();
    let suma = |f: fn(&Fuente) -> f64| r2(fuentes.iter().map(f).sum());
    let total = Fuente {
        nombre: "Total",
        generado: suma(|f| f.generado),
        gastos: suma(|f| f.gastos),
        iva: suma(|f| f.iva),
        seguridad_social: suma(|f| f.seguridad_social),
        irpf: r2(suma(|f| f.irpf) - deducciones + ajustes),
        ..Default::default()
    }
    .cerrar();

    Reparto {
        y,
        fuentes,
        total,
        sueldo: (a.de(Concepto::Sueldo).abs() > DIF || empresa.abs() > DIF).then_some((a.de(Concepto::Sueldo), empresa)),
        ss_grupos: det.nomina.as_ref().filter(|n| n.con_nomina > 0).map(|n| n.grupos_con_algo().into_iter().map(|(nombre, _, p)| (nombre, p.empresa, p.trabajador)).collect()).unwrap_or_default(),
        irpf_retenido: det.irpf.nomina.filter(|x| x.abs() > DIF),
        especie: a.especie,
        tu_ss,
        ahorro: (capital.abs() > DIF || ganancias.abs() > DIF).then_some((capital, ganancias)),
        deducciones,
        ajustes,
        iva,
        con_renta: a.con_renta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::informe::anio::{Anio, Irpf};

    fn detalle(importes: [(Concepto, f64); 6], empresa: f64, actividad: Option<(f64, f64)>, irpf: Irpf) -> Detalle {
        let mut a = Anio { y: 2025, empresa, actividad, ..Default::default() };
        for (c, x) in importes {
            a.importes[c as usize] = x;
        }
        Detalle { anio: a, irpf, ..Default::default() }
    }

    #[test]
    fn sin_renta_lo_adelantado_va_a_su_fuente() {
        let det = detalle(
            [(Concepto::Sueldo, 30000.0), (Concepto::Actividad, 14000.0), (Concepto::TuNomina, 1950.0), (Concepto::Cuota, 3500.0), (Concepto::Capital, 0.0), (Concepto::Ganancias, 0.0)],
            7100.0,
            Some((18000.0, 4000.0)),
            Irpf { total: 8300.0, nomina: Some(5400.0), ..Default::default() },
        );
        let r = reparto(&Datos::default(), &det, None);
        assert_eq!(r.fuentes.iter().map(|f| f.nombre).collect::<Vec<_>>(), [SUELDO, ACTIVIDAD]);
        let (s, a) = (&r.fuentes[0], &r.fuentes[1]);
        assert_eq!((s.generado, s.seguridad_social, s.irpf, s.queda), (37100.0, 9050.0, 5400.0, 22650.0));
        assert_eq!((a.generado, a.gastos, a.seguridad_social, a.irpf, a.queda), (18000.0, 4000.0, 3500.0, 2900.0, 7600.0));
        assert_eq!((r.total.generado, r.total.impuestos(), r.total.queda), (55100.0, 20850.0, 30250.0));
        // Sobre el coste del sueldo y sobre lo neto de la actividad.
        assert_eq!(s.tipo().map(|t| (t * 100.0).round()), Some(39.0));
        assert_eq!(a.tipo().map(|t| (t * 100.0).round()), Some(46.0));
        // Lo que te queda en total es lo mismo que en el balance del año: lo que ganas menos lo que pagas.
        assert_eq!(r.total.queda, r2(30000.0 + 14000.0 - 1950.0 - 3500.0 - 8300.0));
        assert!(r.iva.is_none());
    }

    #[test]
    fn el_iva_pasa_por_la_actividad_sin_contar_como_tuyo() {
        use crate::dominio::FormularioLeido;
        let mut d = Datos::default();
        for (q, cobrado, deducido) in [(1, 420.0, 1050.0), (2, 2100.0, 420.0)] {
            let casillas = [(27, cobrado), (45, deducido)].into_iter().collect();
            d.lecturas.formularios.insert(crate::hacienda::clave(2025, q, "303"), FormularioLeido { casillas, ..Default::default() });
        }
        let det = detalle(
            [(Concepto::Sueldo, 0.0), (Concepto::Actividad, 12000.0), (Concepto::TuNomina, 0.0), (Concepto::Cuota, 3000.0), (Concepto::Capital, 0.0), (Concepto::Ganancias, 0.0)],
            0.0,
            Some((12000.0, 0.0)),
            Irpf { total: 1500.0, ..Default::default() },
        );
        let r = reparto(&d, &det, None);
        assert_eq!(r.iva, Some((2520.0, 1470.0, 2)));
        let a = &r.fuentes[0];
        // Entra con lo facturado y sale a Hacienda; lo tuyo y lo que se lleva el Estado de ti, igual que sin IVA.
        assert_eq!((a.generado, a.iva, a.queda, a.base(), a.impuestos()), (13050.0, 1050.0, 7500.0, 12000.0, 4500.0));
    }

    /// Cada año de las personas de prueba (o, con `DATOS=…`, de otra carpeta) cuadra con el balance del año: lo que te
    /// queda, el IRPF y la Seguridad Social (los mensajes, sin cifras: valen también con datos de verdad).
    #[test]
    fn cuadra_con_el_balance_de_cada_anio() {
        use crate::dominio::fmt::anio_actual;
        use crate::informe::{anio, renta_de};
        use crate::tui::prueba::{cargar, caso};
        let dirs = match std::env::var("DATOS") {
            Ok(d) => vec![std::path::PathBuf::from(d)],
            Err(_) => ["a-de-todo", "b-hacienda", "c-sin-gdj"].map(caso).to_vec(),
        };
        for dir in dirs {
            let d = cargar(&dir);
            for y in crate::dominio::PRIMER_ANIO..=anio_actual() {
                let det = anio::detalle(&d, y);
                let r = reparto(&d, &det, renta_de(&d, y));
                let (a, t) = (&det.anio, &r.total);
                let cerca = |x: f64, z: f64| (x - z).abs() < 0.05;
                assert!(cerca(t.queda, a.queda), "{y}: lo que te queda no cuadra");
                assert!(cerca(t.irpf, det.irpf.total), "{y}: el IRPF no cuadra");
                assert!(cerca(t.seguridad_social, a.seguridad_social() + a.empresa), "{y}: la Seguridad Social no cuadra");
                assert!(cerca(t.base(), a.ganas + a.empresa), "{y}: lo generado no cuadra");
                assert!(r.fuentes.iter().all(|f| f.irpf >= -DIF && f.seguridad_social >= -DIF), "{y}: algo negativo");
            }
        }
    }

    #[test]
    fn con_renta_el_irpf_se_reparte_por_bases() {
        use crate::dominio::renta::{Actividad, Cuotas, Trabajo};
        let det = detalle(
            [(Concepto::Sueldo, 30000.0), (Concepto::Actividad, 14000.0), (Concepto::TuNomina, 0.0), (Concepto::Cuota, 0.0), (Concepto::Capital, 600.0), (Concepto::Ganancias, 2000.0)],
            0.0,
            Some((18000.0, 4000.0)),
            Irpf { total: 9000.0, con_renta: true, ..Default::default() },
        );
        let renta = |liquida: f64| Renta {
            trabajo: Some(Trabajo { neto_reducido: Some(24000.0), ..Default::default() }),
            actividad: Some(Actividad { neto: 12000.0, ..Default::default() }),
            cuotas: Some(Cuotas { general: 8400.0, ahorro: 600.0, integra: 9000.0, liquida, ..Default::default() }),
            ..Default::default()
        };
        let r = reparto(&Datos::default(), &det, Some(&renta(9000.0)));
        let irpf: Vec<(&str, f64)> = r.fuentes.iter().map(|f| (f.nombre, f.irpf)).collect();
        // 9.000 € de cuota: 600 por el ahorro; los 8.400 de la general, 2/3 al sueldo y 1/3 a la actividad.
        assert_eq!(irpf, [(SUELDO, 5600.0), (ACTIVIDAD, 2800.0), (AHORRO, 600.0)]);
        assert_eq!((r.total.irpf, r.deducciones, r.ajustes), (9000.0, 0.0, 0.0));
        assert_eq!(r.ahorro, Some((600.0, 2000.0)));
        // Con 2.000 € de deducciones (de nada en concreto): cada fuente, lo mismo; las deducciones, en el total.
        let det = Detalle { irpf: Irpf { total: 7000.0, con_renta: true, ..Default::default() }, ..det };
        let r = reparto(&Datos::default(), &det, Some(&renta(7000.0)));
        assert_eq!(r.fuentes.iter().map(|f| f.irpf).collect::<Vec<_>>(), [5600.0, 2800.0, 600.0]);
        assert_eq!((r.total.irpf, r.deducciones), (7000.0, 2000.0));
        assert_eq!(r.fuentes[2].tipo().map(|t| (t * 1000.0).round()), Some(231.0), "el ahorro, a su escala");
    }
}
