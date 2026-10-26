//! El empleo por cuenta ajena de un año: sueldo, IRPF retenido, tu Seguridad Social y la de la empresa por
//! conceptos, mes a mes y por empresa, y las nóminas apuntadas.
use crate::dominio::fmt::DIF;
use crate::dominio::tgss::Liquidacion;
use crate::dominio::{sin_repetir, Datos, Nomina};
use crate::dominio::fmt::r2;
use std::collections::BTreeMap;


/// Grupos de conceptos de la Seguridad Social, en el orden en que se enseñan (`T.GRUPOS_SS`):
/// (clave, nombre corto, explicación).
pub const GRUPOS_SS: [(&str, &str, &str); 6] = [
    ("cc", "Pensiones, bajas y demás", "Contingencias comunes: jubilación, incapacidad, bajas por enfermedad…"),
    ("otras", "Desempleo, FOGASA y formación", "El paro, el fondo de garantía salarial y la formación profesional"),
    ("at", "Accidentes de trabajo", "Incapacidad temporal e invalidez por accidente o enfermedad profesional: solo paga la empresa"),
    ("mei", "Equidad intergeneracional (MEI)", "Va al fondo de reserva de las pensiones"),
    ("sol", "Cotización de solidaridad", "Por la parte del sueldo que pasa de la base máxima (desde 2025)"),
    ("otros", "Otros", "Ajustes de la Seguridad Social que no van en ningún concepto"),
];

/// El grupo de un concepto, por su nombre (como lo escribe la Seguridad Social).
pub fn grupo_de(concepto: &str) -> &'static str {
    let c = concepto.trim().to_uppercase();
    if c == "CONTINGENCIAS COMUNES" {
        "cc"
    } else if c == "OTRAS COTIZACIONES" {
        "otras"
    } else if c.contains("ACCIDENTES DE TRABAJO") {
        "at"
    } else if c.contains("EQUIDAD INTERGENERACIONAL") {
        "mei"
    } else if c.contains("SOLIDARIDAD") {
        "sol"
    } else {
        "otros"
    }
}

/// Lo que pagáis tu empresa y tú a la Seguridad Social (de un mes, una empresa, un grupo de conceptos…).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Parte {
    pub empresa: f64,
    pub trabajador: f64,
}

impl Parte {
    fn sumar(&mut self, empresa: f64, trabajador: f64) {
        self.empresa += empresa;
        self.trabajador += trabajador;
    }
    fn redondear(&mut self) {
        self.empresa = r2(self.empresa);
        self.trabajador = r2(self.trabajador);
    }
    /// ¿Hay algo (de tu empresa o tuyo)?
    pub fn algo(&self) -> bool {
        self.empresa.abs() > DIF || self.trabajador.abs() > DIF
    }
}

/// Un mes de cotizaciones de tu nómina.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MesSs {
    /// «2025-12»
    pub ym: String,
    /// Las empresas que liquidaron ese mes, sin repetir.
    pub empresas: Vec<String>,
    pub liquidaciones: usize,
    /// Las que no son la normal (L00): atrasos y demás, liquidadas después.
    pub complementarias: usize,
    pub cuota: Parte,
    /// Sueldo por el que se cotiza (base de contingencias comunes más lo que pasa de la máxima).
    pub sueldo: f64,
}

/// Lo de una empresa en el año.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmpresaAnio {
    pub cuota: Parte,
    pub sueldo: f64,
    /// Meses distintos con liquidación.
    pub meses: usize,
}

/// Las cotizaciones por cuenta ajena de un año, según la Seguridad Social (`T.nominaAnio`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NominaAnio {
    /// Los meses consultados, con o sin nómina.
    pub meses: Vec<MesSs>,
    /// Lo que pagó tu empresa y lo que te descontaron a ti.
    pub cuota: Parte,
    /// El sueldo por el que se cotizó (aproximación del bruto).
    pub sueldo: f64,
    /// Por grupo de conceptos (claves de `GRUPOS_SS`).
    pub grupos: BTreeMap<&'static str, Parte>,
    /// Por empresa (nombre como lo escribe la Seguridad Social).
    pub empresas: BTreeMap<String, EmpresaAnio>,
    /// Meses consultados y meses con nómina.
    pub mirados: usize,
    pub con_nomina: usize,
}

impl NominaAnio {
    /// Lo cotizado por un grupo (clave de `GRUPOS_SS`).
    pub fn grupo(&self, clave: &str) -> Parte {
        self.grupos.get(clave).copied().unwrap_or_default()
    }
    /// Los grupos con algo, en el orden de `GRUPOS_SS`: (nombre, explicación, lo cotizado).
    pub fn grupos_con_algo(&self) -> Vec<(&'static str, &'static str, Parte)> {
        GRUPOS_SS.iter().map(|(k, nombre, expl)| (*nombre, *expl, self.grupo(k))).filter(|g| g.2.algo()).collect()
    }
    /// «1 mes», «7 meses»: los meses con nómina.
    pub fn texto_meses(&self) -> String {
        if self.con_nomina == 1 { "1 mes".into() } else { format!("{} meses", self.con_nomina) }
    }
}

/// Lo de un año, por el mes al que corresponde cada liquidación. None si la Seguridad Social no tiene meses de
/// ese año (no se consultó).
pub fn nomina_anio(d: &Datos, anio: i32) -> Option<NominaAnio> {
    let pref = format!("{anio}-");
    let meses: Vec<(&String, &Vec<Liquidacion>)> = d.tgss.nomina.meses.iter().filter(|(ym, _)| ym.starts_with(&pref)).collect();
    if meses.is_empty() {
        return None;
    }
    let mut out = NominaAnio { mirados: meses.len(), ..Default::default() };
    for (k, _, _) in GRUPOS_SS {
        out.grupos.insert(k, Parte::default());
    }
    let mut meses_empresa: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for (ym, lista) in meses {
        let mut m = MesSs {
            ym: ym.clone(),
            empresas: sin_repetir(lista.iter().map(|l| l.empresa.as_str())),
            liquidaciones: lista.len(),
            complementarias: lista.iter().filter(|l| l.complementaria()).count(),
            ..Default::default()
        };
        for l in lista {
            m.cuota.sumar(l.cuota_empresa, l.cuota_trabajador);
            let s = l.sueldo();
            m.sueldo += s;
            let e = out.empresas.entry(l.empresa.clone()).or_default();
            e.cuota.sumar(l.cuota_empresa, l.cuota_trabajador);
            e.sueldo += s;
            meses_empresa.entry(l.empresa.clone()).or_default().insert(ym.clone());
            let mut en_conceptos = Parte::default();
            for c in l.conceptos.iter().filter(|c| !c.es_total()) {
                out.grupos.entry(grupo_de(&c.concepto)).or_default().sumar(c.empresa, c.trabajador);
                en_conceptos.sumar(c.empresa, c.trabajador);
            }
            // Si la Seguridad Social trae algún ajuste que no está en los conceptos, cuadra en «otros».
            out.grupos.entry("otros").or_default().sumar(l.cuota_empresa - en_conceptos.empresa, l.cuota_trabajador - en_conceptos.trabajador);
        }
        m.cuota.redondear();
        m.sueldo = r2(m.sueldo);
        out.cuota.sumar(m.cuota.empresa, m.cuota.trabajador);
        out.sueldo += m.sueldo;
        out.meses.push(m);
    }
    out.grupos.values_mut().for_each(Parte::redondear);
    for (n, e) in out.empresas.iter_mut() {
        e.cuota.redondear();
        e.sueldo = r2(e.sueldo);
        e.meses = meses_empresa.get(n).map_or(0, |s| s.len());
    }
    out.cuota.redondear();
    out.sueldo = r2(out.sueldo);
    out.con_nomina = out.meses.iter().filter(|m| m.liquidaciones > 0).count();
    Some(out)
}

/* ================= el año, con lo que haya (Renta, nóminas, Seguridad Social) ================= */

/// De dónde sale una cifra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fuente {
    /// La Renta de ese año.
    Renta(i32),
    /// Las nóminas apuntadas a mano.
    Nominas,
    /// Lo que liquidó la Seguridad Social.
    SeguridadSocial,
    /// El sueldo por el que se cotizó: se parece al bruto, pero no es el bruto.
    Cotizado,
}

impl Fuente {
    /// «tu Renta de 2024», «tus nóminas apuntadas», «la Seguridad Social», «la Seguridad Social (aprox.)».
    pub fn texto(&self) -> String {
        match self {
            Fuente::Renta(y) => format!("tu Renta de {y}"),
            Fuente::Nominas => "tus nóminas apuntadas".into(),
            Fuente::SeguridadSocial => "la Seguridad Social".into(),
            Fuente::Cotizado => "la Seguridad Social (aprox.)".into(),
        }
    }
}

/// Un importe y de dónde sale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cifra {
    pub v: f64,
    pub de: Fuente,
}

/// Tu empleo por cuenta ajena de un año, con lo que haya: de la Renta si está
/// presentada; si no, de tus nóminas apuntadas y de la Seguridad Social.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmpleoAnio {
    pub anio: i32,
    /// Sueldo bruto (dinero y especie).
    pub bruto: Option<Cifra>,
    /// IRPF que te retuvieron.
    pub irpf: Option<Cifra>,
    /// Tu Seguridad Social (lo que te descontaron).
    pub tu_ss: Option<Cifra>,
    /// Lo que pagó tu empresa a la Seguridad Social por ti (solo lo sabe la Seguridad Social).
    pub empresa: Option<Cifra>,
    /// Dónde trabajaste: según la Seguridad Social o, si no lo tiene, tus nóminas apuntadas.
    pub empresas: Vec<String>,
    /// Lo cobrado en dinero (sin la especie), para saber lo que te llegó a la cuenta.
    pub dinero: Option<f64>,
    /// En especie: seguro, acciones, coche…
    pub especie: f64,
    /// Lo que dice la Seguridad Social de tus nóminas de ese año (None si no se consultó).
    pub ss: Option<NominaAnio>,
    /// Nóminas apuntadas ese año.
    pub nominas: usize,
    /// ¿Hay Renta de ese año? Y, si tiene trabajo, el sueldo que dice.
    pub con_renta: bool,
    pub bruto_renta: Option<f64>,
}

impl EmpleoAnio {
    /// ¿Hay algo que enseñar de ese año?
    pub fn hay(&self) -> bool {
        self.bruto.is_some() || self.irpf.is_some() || self.tu_ss.is_some() || self.empresa.is_some()
    }
    /// Lo de la Seguridad Social, si liquidó alguna nómina tuya ese año.
    pub fn con_nomina(&self) -> Option<&NominaAnio> {
        self.ss.as_ref().filter(|n| n.con_nomina > 0)
    }
}

/// De lo que pagó tu empresa por ti a lo que te llegó, paso a paso (como una nómina, pero del año entero).
#[derive(Debug, Clone, PartialEq)]
pub struct Camino {
    /// Lo que pagó tu empresa por ti: tu bruto más su Seguridad Social (None si la Seguridad Social no lo tiene).
    pub coste: Option<f64>,
    pub ss_empresa: Option<f64>,
    /// Tu sueldo bruto: en dinero y en especie.
    pub bruto: f64,
    pub tu_ss: f64,
    /// IRPF retenido de tu sueldo en dinero.
    pub irpf_sueldo: f64,
    /// IRPF de lo cobrado en especie (los ingresos a cuenta que hace tu empresa); 0 si no se sabe.
    pub irpf_especie: f64,
    /// Ese IRPF te lo descontaron de la nómina (repercutido); si no, lo pagó tu empresa además de tu sueldo.
    pub especie_repercutida: bool,
    /// Lo que te llegó en dinero.
    pub dinero: f64,
    /// Lo que vale lo que recibiste en especie.
    pub especie: f64,
    pub de: Fuente,
    /// Con la Renta: lo que salió al hacerla (> 0 a pagar, < 0 a devolver), con todo lo del año (no solo el sueldo).
    pub resultado_renta: Option<f64>,
    /// Con la Renta: tipo medio estatal y autonómico de la base general (%).
    pub tipo_medio: Option<(f64, f64)>,
}

impl Camino {
    pub fn irpf(&self) -> f64 {
        r2(self.irpf_sueldo + self.irpf_especie)
    }
    /// La base de los porcentajes: lo que pagó tu empresa si se sabe; si no, tu bruto.
    pub fn total(&self) -> f64 {
        self.coste.unwrap_or(self.bruto)
    }
    /// `x` en % de `total()`.
    pub fn pct(&self, x: f64) -> f64 {
        let t = self.total();
        if t.abs() > DIF { x / t * 100.0 } else { 0.0 }
    }
    /// De cada 100 € de `total()`: (impuestos y cotizaciones, en dinero, en especie), redondeados y sumando 100.
    pub fn de_cada_100(&self) -> (i64, i64, i64) {
        let dinero = self.pct(self.dinero).round() as i64;
        let especie = self.pct(self.especie).round() as i64;
        (100 - dinero - especie, dinero, especie)
    }
    /// Lo retenido, en % del bruto.
    pub fn retenido_pct(&self) -> f64 {
        if self.bruto.abs() > DIF { self.irpf() / self.bruto * 100.0 } else { 0.0 }
    }
}

/// El camino del dinero de un año: de la Renta (con el IRPF de la especie aparte) o, sin Renta, de las nóminas
/// apuntadas. Lo de la empresa, de la Seguridad Social. None si no se sabe el sueldo y el IRPF.
pub fn camino(d: &Datos, a: &EmpleoAnio) -> Option<Camino> {
    let y = a.anio;
    let ss_empresa = a.empresa.map(|c| c.v);
    let coste = |bruto: f64| ss_empresa.map(|e| r2(bruto + e));
    if let Some(r) = super::renta_de(d, y)
        && let Some(t) = &r.trabajo {
            let num = crate::hacienda::modelo_100::numeracion(&r.numeracion);
            let n = num.trabajo;
            let ic = r.casilla(n.ingresos_cuenta).unwrap_or(0.0);
            let rep = r.casilla(n.repercutidos).unwrap_or(0.0);
            let especie = r.casilla(n.valoracion_especie).unwrap_or_else(|| r2(t.especie - ic + rep));
            let irpf_sueldo = r2(t.retenciones.unwrap_or(0.0) - ic);
            let tipo_medio = match (r.casilla(num.tipo_medio[0]), r.casilla(num.tipo_medio[1])) {
                (Some(e), Some(c)) => Some((e, c)),
                _ => None,
            };
            return Some(Camino {
                coste: coste(t.integros),
                ss_empresa,
                bruto: t.integros,
                tu_ss: t.ss,
                irpf_sueldo,
                irpf_especie: ic,
                especie_repercutida: rep > DIF,
                // Lo cobrado en dinero, menos lo que te descontaron: tu Seguridad Social, el IRPF y, si te lo
                // repercutieron, el de la especie.
                dinero: r2(t.dinerario - t.ss - irpf_sueldo - rep),
                especie,
                de: Fuente::Renta(y),
                resultado_renta: Some(r.resultado),
                tipo_medio,
            });
        }
    let l = nominas_anio(d, y);
    if l.is_empty() {
        return None;
    }
    let suma = |f: fn(&Nomina) -> f64| r2(l.iter().map(|n| f(n)).sum());
    let (dinerario, especie, ret, ss) = (suma(|n| n.dinerario), suma(|n| n.especie), suma(|n| n.retenciones), suma(|n| n.ss));
    let bruto = r2(dinerario + especie);
    Some(Camino {
        coste: coste(bruto),
        ss_empresa,
        bruto,
        tu_ss: ss,
        irpf_sueldo: ret,
        irpf_especie: 0.0,
        especie_repercutida: false,
        dinero: r2(dinerario - ss - ret),
        especie,
        de: Fuente::Nominas,
        resultado_renta: None,
        tipo_medio: None,
    })
}

/// Las nóminas apuntadas de un año, por meses (las de un mismo mes, en el orden en que se apuntaron).
pub fn nominas_anio(d: &Datos, anio: i32) -> Vec<&Nomina> {
    let mut v: Vec<&Nomina> = d.nominas.iter().filter(|n| n.anio == anio).collect();
    v.sort_by_key(|n| n.mes);
    v
}

/// El empleo de un año (ver `EmpleoAnio`).
pub fn empleo_anio(d: &Datos, anio: i32) -> EmpleoAnio {
    let r = super::renta_de(d, anio);
    let t = r.and_then(|r| r.trabajo.as_ref());
    let n = nomina_anio(d, anio);
    let l = nominas_anio(d, anio);
    let suma = |f: fn(&Nomina) -> f64| r2(l.iter().map(|n| f(n)).sum());
    let hay_l = !l.is_empty();
    let ss = n.as_ref().filter(|n| n.con_nomina > 0);
    let cifra = |v: f64, de: Fuente| Cifra { v, de };

    let bruto = t
        .map(|t| cifra(t.integros, Fuente::Renta(anio)))
        .or_else(|| hay_l.then(|| cifra(r2(suma(|n| n.dinerario) + suma(|n| n.especie)), Fuente::Nominas)))
        .or_else(|| ss.map(|n| cifra(n.sueldo, Fuente::Cotizado)));
    let irpf = t
        .and_then(|t| t.retenciones)
        .map(|v| cifra(v, Fuente::Renta(anio)))
        .or_else(|| hay_l.then(|| cifra(suma(|n| n.retenciones), Fuente::Nominas)));
    let tu_ss = ss
        .map(|n| cifra(n.cuota.trabajador, Fuente::SeguridadSocial))
        .or_else(|| t.filter(|t| t.ss.abs() > DIF).map(|t| cifra(t.ss, Fuente::Renta(anio))))
        .or_else(|| Some(suma(|n| n.ss)).filter(|v| hay_l && v.abs() > DIF).map(|v| cifra(v, Fuente::Nominas)));
    let empresa = ss.map(|n| cifra(n.cuota.empresa, Fuente::SeguridadSocial));
    let empresas = match ss {
        Some(n) => n.empresas.keys().cloned().collect(),
        None => sin_repetir(l.iter().map(|n| n.empresa.as_str())),
    };
    let dinero = t.map(|t| t.dinerario).or_else(|| hay_l.then(|| suma(|n| n.dinerario)));
    let especie = t.map(|t| t.especie).unwrap_or_else(|| if hay_l { suma(|n| n.especie) } else { 0.0 });

    EmpleoAnio {
        anio,
        bruto,
        irpf,
        tu_ss,
        empresa,
        empresas,
        dinero,
        especie,
        nominas: l.len(),
        con_renta: r.is_some(),
        bruto_renta: t.map(|t| t.integros),
        ss: n,
    }
}

/* ================= mes a mes ================= */

/// `x` en % de `de` (0 si `de` es cero).
fn pct(x: f64, de: f64) -> f64 {
    if de.abs() > DIF { x / de * 100.0 } else { 0.0 }
}

/// Lo apuntado en las nóminas de un mes (sumado, si hay más de una).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Apuntado {
    /// Bruto: en dinero y en especie.
    pub bruto: f64,
    pub especie: f64,
    pub irpf: f64,
    pub ss: f64,
    pub empresas: Vec<String>,
}

impl Apuntado {
    /// Lo retenido, en % del bruto.
    pub fn retenido_pct(&self) -> f64 {
        pct(self.irpf, self.bruto)
    }
}

/// Un mes del año: lo apuntado de tus nóminas y lo que liquidó la Seguridad Social.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MesSueldo {
    /// «2026-03»
    pub ym: String,
    pub nomina: Option<Apuntado>,
    /// Lo de la Seguridad Social, si liquidó alguna nómina tuya ese mes.
    pub ss: Option<MesSs>,
}

impl MesSueldo {
    /// El bruto: el de tu nómina o, si no está apuntada, el sueldo cotizado (aproximado: true).
    pub fn bruto(&self) -> Option<(f64, bool)> {
        self.nomina.as_ref().map(|n| (n.bruto, false)).or_else(|| self.ss.as_ref().map(|m| (m.sueldo, true)))
    }
    /// Tu Seguridad Social: la de tu nómina o, si no está apuntada, la liquidada.
    pub fn tu_ss(&self) -> Option<f64> {
        self.nomina.as_ref().map(|n| n.ss).or_else(|| self.ss.as_ref().map(|m| m.cuota.trabajador))
    }
    /// Dónde: según tus nóminas o la Seguridad Social.
    pub fn empresas(&self) -> &[String] {
        match (&self.nomina, &self.ss) {
            (Some(n), _) if !n.empresas.is_empty() => &n.empresas,
            (_, Some(m)) => &m.empresas,
            _ => &[],
        }
    }
    pub fn vacio(&self) -> bool {
        self.nomina.is_none() && self.ss.is_none()
    }
    /// El mes (1–12).
    pub fn mes(&self) -> u32 {
        crate::dominio::fmt::mes_de(&self.ym)
    }
}

/// Los meses de un año, de enero al último que toca: diciembre en los años pasados; en el año en curso, el de hoy
/// (o el último con algo, si va por delante).
pub fn meses_sueldo(d: &Datos, a: &EmpleoAnio) -> Vec<MesSueldo> {
    let y = a.anio;
    let l = nominas_anio(d, y);
    let hoy = crate::dominio::fmt::hoy();
    let ultimo = if y < crate::dominio::fmt::anio_de(&hoy) { 12 } else { crate::dominio::fmt::mes_de(&hoy) };
    let ultimo = l.iter().map(|n| n.mes).chain([ultimo]).max().unwrap_or(12).clamp(1, 12);
    (1..=ultimo)
        .map(|m| {
            let ym = format!("{y}-{m:02}");
            let del_mes: Vec<&&Nomina> = l.iter().filter(|n| n.mes == m).collect();
            let mut nomina: Option<Apuntado> = None;
            for n in &del_mes {
                let x = nomina.get_or_insert_with(Apuntado::default);
                x.bruto = r2(x.bruto + n.bruto());
                x.especie = r2(x.especie + n.especie);
                x.irpf = r2(x.irpf + n.retenciones);
                x.ss = r2(x.ss + n.ss);
            }
            if let Some(x) = &mut nomina {
                x.empresas = sin_repetir(del_mes.iter().map(|n| n.empresa.as_str()));
            }
            let ss = a.ss.iter().flat_map(|n| &n.meses).find(|c| c.ym == ym && c.liquidaciones > 0).cloned();
            MesSueldo { ym, nomina, ss }
        })
        .collect()
}

/* ================= ¿te retienen bien? ================= */

/// Tu comunidad, la de tu Renta más reciente hasta ese año (o, si no hay, la primera de después): (clave, año).
pub fn comunidad(d: &Datos, anio: i32) -> Option<(&str, i32)> {
    let r = &d.lecturas.rentas;
    r.range(..=anio).rev().chain(r.range(anio + 1..)).find(|(_, r)| !r.comunidad.is_empty()).map(|(y, r)| (r.comunidad.as_str(), *y))
}

/// Lo que dicen tus nóminas apuntadas de un año, comparado con lo que te tocaría pagar en la Renta por ese sueldo
/// (sin hijos ni discapacidad y sin otras rentas: ver `hacienda::irpf`).
#[derive(Debug, Clone, PartialEq)]
pub struct Retencion {
    /// Meses con nómina apuntada, y los que se cuentan del año: en el año en curso, desde el primero en que
    /// cobraste (enero si la Seguridad Social no está consultada) hasta diciembre; en los pasados, los que
    /// cobraste (según tus nóminas y la Seguridad Social).
    pub meses: usize,
    pub meses_anio: usize,
    /// Lo apuntado: bruto (con la especie), IRPF retenido y tu Seguridad Social.
    pub bruto: f64,
    pub retenido: f64,
    pub ss: f64,
    /// El sueldo de esos meses del año, si cobras lo mismo cada mes.
    pub anual: f64,
    /// El IRPF que toca por `anual`.
    pub toca: f64,
    /// Tu comunidad (clave) y la Renta de la que sale; None si no hay Renta que lo diga.
    pub comunidad: Option<(String, i32)>,
    /// El año de las reglas que se usan.
    pub reglas: i32,
}

/// Cómo vas de retención.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredicto {
    Bien,
    Poco,
    Mucho,
}

impl Retencion {
    pub fn retenido_pct(&self) -> f64 {
        pct(self.retenido, self.bruto)
    }
    /// El IRPF que toca, en % del sueldo.
    pub fn toca_pct(&self) -> f64 {
        pct(self.toca, self.anual)
    }
    /// ¿Se ha supuesto el resto del año? (no están todos los meses que se cuentan)
    pub fn extrapolado(&self) -> bool {
        self.meses < self.meses_anio
    }
    /// Lo que saldría en la Renta si sigue así (> 0 a pagar, < 0 a devolver), redondeado a la decena.
    pub fn renta(&self) -> f64 {
        let retenido = self.retenido * self.meses_anio as f64 / self.meses.max(1) as f64;
        ((self.toca - retenido) / 10.0).round() * 10.0
    }
    /// Con qué se ha calculado: «Calculado para unos 30.000 € al año (si cobras lo mismo hasta diciembre), sin hijos
    /// ni discapacidad y sin otras rentas, en Madrid (tu Renta de 2025).»
    pub fn supuestos(&self, anio: i32) -> String {
        let sueldo = if self.extrapolado() {
            format!("unos {} al año (si cobras lo mismo hasta diciembre)", crate::dominio::fmt::eur0((self.anual / 100.0).round() * 100.0))
        } else {
            format!("tu sueldo de {}", crate::dominio::fmt::eur0(self.anual))
        };
        let donde = match &self.comunidad {
            Some((c, ry)) => format!(", en {} (tu Renta de {ry})", crate::hacienda::irpf::nombre_comunidad(c)),
            None => ", con la escala estatal también de autonómica (no hay Renta tuya que diga tu comunidad)".into(),
        };
        let reglas = if self.reglas != anio { format!(", con las reglas de {}", self.reglas) } else { String::new() };
        format!("Calculado para {sueldo}, sin hijos ni discapacidad y sin otras rentas{donde}{reglas}.")
    }
    /// Bien si la diferencia es menos de un punto o de 100 €.
    pub fn veredicto(&self) -> Veredicto {
        let d = self.toca_pct() - self.retenido_pct();
        if d.abs() < 1.0 || self.renta().abs() < 100.0 {
            Veredicto::Bien
        } else if d > 0.0 {
            Veredicto::Poco
        } else {
            Veredicto::Mucho
        }
    }
}

/// ¿Te retienen bien? Con las nóminas apuntadas de un año; None si no hay ninguna con bruto.
pub fn retencion(d: &Datos, meses: &[MesSueldo], anio: i32) -> Option<Retencion> {
    let con: Vec<&Apuntado> = meses.iter().filter_map(|m| m.nomina.as_ref()).filter(|n| n.bruto > DIF).collect();
    if con.is_empty() {
        return None;
    }
    let suma = |f: fn(&Apuntado) -> f64| r2(con.iter().map(|n| f(n)).sum());
    let (bruto, retenido, ss) = (suma(|n| n.bruto), suma(|n| n.irpf), suma(|n| n.ss));
    let n = con.len();
    // Los meses que cobraste: los que tienen nómina apuntada o liquidación en la Seguridad Social. Si la Seguridad
    // Social no está consultada ese año, no se sabe: en el año en curso se cuenta desde enero.
    let cobrado = |m: &&MesSueldo| m.nomina.as_ref().is_some_and(|n| n.bruto > DIF) || m.ss.is_some();
    let consultado = d.tgss.nomina.meses.keys().any(|ym| crate::dominio::fmt::anio_de(ym) == anio);
    let meses_anio = if anio < crate::dominio::fmt::anio_actual() {
        meses.iter().filter(cobrado).count()
    } else if consultado {
        meses.iter().find(cobrado).map_or(12, |m| 13 - m.mes() as usize)
    } else {
        12
    }
    .max(n);
    let k = meses_anio as f64 / n as f64;
    let (anual, ss_anual) = (r2(bruto * k), r2(ss * k));
    let c = comunidad(d, anio);
    let reglas = crate::hacienda::irpf::reglas(anio, c.map_or("", |c| c.0));
    let toca = crate::hacienda::irpf::cuota_sueldo(&reglas, anual, ss_anual).total;
    Some(Retencion { meses: n, meses_anio, bruto, retenido, ss, anual, toca, comunidad: c.map(|(c, y)| (c.to_string(), y)), reglas: reglas.anio })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nomina(anio: i32, empresa: &str, mes: u32, v: [f64; 4]) -> Nomina {
        Nomina { id: format!("{empresa}{mes}"), anio, empresa: empresa.into(), mes, dinerario: v[0], especie: v[1], retenciones: v[2], ss: v[3] }
    }

    #[test]
    fn validar() {
        assert_eq!(Nomina::validar(&nomina(2026, "", 9, [1000.0, 0.0, 150.0, 63.5])), Ok(()));
        assert_eq!(Nomina::validar(&nomina(2026, "", 9, [0.0, 200.0, 0.0, 0.0])), Ok(()), "solo en especie también vale");
        assert!(Nomina::validar(&nomina(2026, "", 9, [0.0, 0.0, 0.0, 0.0])).unwrap_err().contains("bruto"));
        assert!(Nomina::validar(&nomina(2026, "", 9, [100.0, 0.0, 80.0, 30.0])).unwrap_err().contains("sumar más"));
        assert!(Nomina::validar(&nomina(2026, "", 0, [100.0, 0.0, 0.0, 0.0])).is_err());
        assert!(Nomina::validar(&nomina(2026, "", 13, [100.0, 0.0, 0.0, 0.0])).is_err());
    }

    #[test]
    fn conceptos_y_meses() {
        let concepto = |c: &str| crate::dominio::tgss::ConceptoSs { concepto: c.into(), ..Default::default() };
        assert!(concepto("LIQUIDO DE TOTALES").es_total() && !concepto("CONTINGENCIAS COMUNES").es_total());
        assert_eq!(crate::dominio::fmt::nombre_mes("2025-12"), "diciembre de 2025");
        assert_eq!(crate::dominio::fmt::nombre_mes("2025-01"), "enero de 2025");
        assert_eq!(crate::dominio::fmt::nombre_mes("2025"), "2025");
    }
}
