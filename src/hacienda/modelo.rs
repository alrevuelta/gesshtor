//! Los modelos que la app sabe leer de su PDF, y lo que tienen en común. Cada uno tiene su fichero con todo lo
//! suyo (de dónde sale su especificación, qué casillas tiene, cómo se lee, cómo se comprueba y, si la app lo
//! calcula, cómo se calcula): `modelo_100` (la Renta), `modelo_130`, `modelo_303` y `modelo_390`.
//!
//! Aquí:
//! - `Modelo`: cuál es. Al añadir uno, el compilador señala cada `match` en el que falta.
//! - `Trimestral`: los que se presentan cada trimestre y la app calcula con tus facturas y gastos (303 y 130).
//! - `Periodo` y `Periodicidad`: de qué periodo es una declaración y cada cuánto se presenta un modelo.
//! - `Fallo`: por qué no se ha podido leer un PDF.
//!
//! Para añadir un modelo con casillas en formulario, ver `formulario`.
use super::{modelo_130, modelo_303};
use crate::dominio::{Casillas, Datos, FormularioLeido};
use crate::dominio::PRIMER_ANIO;
use std::collections::HashMap;
use std::fmt;

/// Un modelo que la app sabe leer de su PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modelo {
    /// La Renta: el IRPF del año.
    M100,
    /// El pago fraccionado del IRPF de la actividad, cada trimestre.
    M130,
    /// El IVA, cada trimestre.
    M303,
    /// El resumen anual del IVA.
    M390,
}

impl Modelo {
    pub const TODOS: [Modelo; 4] = [Modelo::M100, Modelo::M130, Modelo::M303, Modelo::M390];

    /// Su número, como lo escribe Hacienda («303»).
    pub const fn codigo(self) -> &'static str {
        match self {
            Modelo::M100 => "100",
            Modelo::M130 => "130",
            Modelo::M303 => "303",
            Modelo::M390 => "390",
        }
    }

    /// El modelo de un número («303»), si es uno de los que lee la app.
    pub fn de_codigo(codigo: &str) -> Option<Modelo> {
        Self::TODOS.into_iter().find(|m| m.codigo() == codigo)
    }

    /// Con cuántas cifras se escriben sus casillas: la Renta, con 4 («0670»); los demás, con 2 o más («07», «110»).
    pub const fn cifras(self) -> usize {
        match self {
            Modelo::M100 => 4,
            Modelo::M130 | Modelo::M303 | Modelo::M390 => 2,
        }
    }

    /// Lo leído del PDF presentado de un trimestre (`q` 1–4) o de un año (`q` = 0), si lo hay. La Renta no está
    /// aquí, sino en `lecturas.rentas`.
    pub fn leido(self, d: &Datos, anio: i32, q: u32) -> Option<&FormularioLeido> {
        d.lecturas.formularios.get(&super::clave(anio, q, self.codigo()))
    }
}

impl fmt::Display for Modelo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

/// Los modelos que se presentan cada trimestre y que la app calcula con tus facturas y gastos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trimestral {
    M303,
    M130,
}

impl Trimestral {
    pub const TODOS: [Trimestral; 2] = [Trimestral::M303, Trimestral::M130];

    pub const fn modelo(self) -> Modelo {
        match self {
            Trimestral::M303 => Modelo::M303,
            Trimestral::M130 => Modelo::M130,
        }
    }

    /// La casilla del resultado: lo que sale a ingresar (o, si es negativo, a compensar o a devolver).
    pub const fn resultado(self) -> u32 {
        match self {
            Trimestral::M303 => modelo_303::RESULTADO,
            Trimestral::M130 => modelo_130::RESULTADO,
        }
    }

    /// Lo que sale por el trimestre entero de unas casillas del modelo (también si son de una complementaria).
    pub fn resultado_de(self, c: &Casillas) -> f64 {
        match self {
            Trimestral::M303 => modelo_303::resultado_del_periodo(c),
            Trimestral::M130 => modelo_130::resultado_del_trimestre(c),
        }
    }

    /// Desde cuándo cuentan tus facturas y gastos: en el 303, desde que empieza el trimestre; en el 130, desde el 1
    /// de enero (se calcula con lo acumulado del año).
    pub fn desde(self, anio: i32, q: u32) -> String {
        match self {
            Trimestral::M303 => crate::dominio::fmt::periodo(anio, q).0,
            Trimestral::M130 => format!("{anio}-01-01"),
        }
    }

    /// Calculado con tus facturas y gastos.
    pub fn calcular(self, d: &Datos, anio: i32, q: u32, memo: &mut Memo) -> Casillas {
        match self {
            Trimestral::M303 => modelo_303::calcular(d, anio, q, memo),
            Trimestral::M130 => modelo_130::calcular(d, anio, q, memo),
        }
    }

    /// El que cuenta: el presentado si se ha leído su PDF; si no, el calculado.
    pub fn cuenta(self, d: &Datos, anio: i32, q: u32, memo: &mut Memo) -> Casillas {
        if let Some(c) = memo.0.get(&(self, anio, q)) {
            return c.clone();
        }
        let c = match self.modelo().leido(d, anio, q) {
            Some(l) => l.casillas.clone(),
            None => self.calcular(d, anio, q, memo),
        };
        memo.0.insert((self, anio, q), c.clone());
        c
    }
}

/// Lo que ya se sabe que cuenta de cada trimestre, para no repetir la cadena de trimestres: cada uno arrastra lo
/// de los anteriores (lo pendiente de compensar del 303, los pagos anteriores del 130).
#[derive(Debug, Default)]
pub struct Memo(HashMap<(Trimestral, i32, u32), Casillas>);

/// Cada cuánto se presenta un modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Periodicidad {
    Trimestral,
    Anual,
}

/// Un trimestre: del 1 al 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trimestre(u32);

impl Trimestre {
    pub fn new(q: u32) -> Option<Trimestre> {
        (1..=4).contains(&q).then_some(Trimestre(q))
    }
    pub fn numero(self) -> u32 {
        self.0
    }
    /// El último mes del trimestre (3, 6, 9 o 12).
    pub fn ultimo_mes(self) -> u32 {
        self.0 * 3
    }
}

/// De qué periodo es una declaración, como lo escribe Hacienda: «1T»…«4T», «0A» (el año) o «01»…«12» (un mes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Periodo {
    Trimestre(Trimestre),
    Anual,
    Mes(u32),
    /// Uno que la app no conoce, tal cual.
    Otro(String),
}

impl Periodo {
    /// El periodo de un texto («3T»); vacío es el año.
    pub fn de(s: &str) -> Periodo {
        let trimestre = s.strip_suffix('T').filter(|q| q.len() == 1).and_then(|q| q.parse().ok()).and_then(Trimestre::new);
        let mes = Some(s).filter(|s| s.len() == 2).and_then(|s| s.parse().ok()).filter(|m| (1..=12).contains(m));
        match (s, trimestre, mes) {
            (_, Some(t), _) => Periodo::Trimestre(t),
            ("0A" | "", _, _) => Periodo::Anual,
            (_, _, Some(m)) => Periodo::Mes(m),
            _ => Periodo::Otro(s.to_string()),
        }
    }
}

impl fmt::Display for Periodo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Periodo::Trimestre(t) => write!(f, "{}T", t.numero()),
            Periodo::Anual => f.write_str("0A"),
            Periodo::Mes(m) => write!(f, "{m:02}"),
            Periodo::Otro(s) => f.write_str(s),
        }
    }
}

/// Por qué no se ha podido leer el PDF de una declaración. Lo que se enseña es su texto (`Display`).
#[derive(Debug, Clone, PartialEq)]
pub enum Fallo {
    /// Es de antes de `PRIMER_ANIO`.
    Anterior(Modelo),
    /// El modelo no tiene ese periodo (un 303 anual, un 390 trimestral…).
    Periodo { modelo: Modelo, periodicidad: Periodicidad, periodo: Periodo },
    /// No hay diseño de registro de ese periodo.
    SinDiseno { modelo: Modelo, ejercicio: i32, periodo: Periodo },
    /// El PDF es de otro modelo (`es`: el que dice, si dice alguno).
    OtroModelo { pedido: Modelo, es: Option<String> },
    /// El PDF es de otro ejercicio o periodo (sin periodo: el ejercicio entero).
    OtroPeriodo { pedido: (i32, Option<Periodo>), es: (i32, Option<Periodo>) },
    /// No es la Renta de ese año.
    NoEsLaRenta(i32),
    /// En la Renta, no cuadra ninguna de las numeraciones de casillas que conoce la app.
    Numeracion(i32),
    /// No hay ninguna casilla con importe.
    SinCasillas,
    /// Lo leído no cuadra con las sumas del propio modelo en estas casillas.
    NoCuadra { modelo: Modelo, casillas: Vec<u32> },
    /// El PDF no se puede abrir.
    Pdf(String),
    /// El lector se ha roto con este PDF (un fallo de la app).
    Roto,
}

impl fmt::Display for Fallo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let de = |(a, p): &(i32, Option<Periodo>)| match p {
            Some(p) => format!("del {p} de {a}"),
            None => format!("de {a}"),
        };
        match self {
            Fallo::Anterior(Modelo::M100) => write!(f, "La app lee las Rentas desde {PRIMER_ANIO}."),
            Fallo::Anterior(_) => write!(f, "La app lee declaraciones desde {PRIMER_ANIO}."),
            Fallo::Periodo { modelo, periodicidad: Periodicidad::Trimestral, periodo } => {
                write!(f, "La app lee el {modelo} trimestral (1T…4T), no el del periodo «{periodo}».")
            }
            Fallo::Periodo { modelo, periodicidad: Periodicidad::Anual, periodo } => write!(f, "El {modelo} es anual: no tiene periodo «{periodo}»."),
            Fallo::SinDiseno { modelo, ejercicio, periodo } => write!(f, "No hay diseño del {modelo} para {periodo} de {ejercicio}."),
            Fallo::OtroModelo { pedido, es: Some(es) } => write!(f, "El PDF es de un modelo {es}, no de un {pedido}."),
            Fallo::OtroModelo { pedido, es: None } => write!(f, "El PDF no es de un modelo {pedido}."),
            Fallo::OtroPeriodo { pedido, es } => write!(f, "El PDF es {}, no {}.", de(es), de(pedido)),
            Fallo::NoEsLaRenta(a) => write!(f, "El PDF no es la Renta de {a}."),
            Fallo::Numeracion(a) => write!(f, "No reconozco la numeración de las casillas de la Renta de {a}."),
            Fallo::SinCasillas => f.write_str("No encuentro las casillas en el PDF."),
            Fallo::NoCuadra { modelo, casillas } => {
                let n = modelo.cifras();
                let v: Vec<String> = casillas.iter().map(|c| format!("{c:0n$}")).collect();
                write!(f, "Lo que he leído no cuadra (casilla{} {}).", if v.len() > 1 { "s" } else { "" }, v.join(", "))
            }
            Fallo::Pdf(e) => f.write_str(e),
            Fallo::Roto => f.write_str("El lector se ha roto con este PDF."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modelos_y_periodos() {
        assert!(Modelo::TODOS.iter().all(|&m| Modelo::de_codigo(m.codigo()) == Some(m)));
        assert_eq!(Modelo::de_codigo("347"), None);
        for s in ["1T", "4T", "0A", "01", "12", "1P", "13", "5T"] {
            assert_eq!(Periodo::de(s).to_string(), s);
        }
        assert_eq!(Periodo::de("3T"), Periodo::Trimestre(Trimestre::new(3).unwrap()));
        assert_eq!(Periodo::de(""), Periodo::Anual);
        assert_eq!(Periodo::de("07"), Periodo::Mes(7));
        assert_eq!(Periodo::de("5T"), Periodo::Otro("5T".into()));
        assert_eq!(Trimestre::new(0), None);
        assert_eq!(Fallo::NoCuadra { modelo: Modelo::M303, casillas: vec![3, 110] }.to_string(), "Lo que he leído no cuadra (casillas 03, 110).");
        assert_eq!(Fallo::NoCuadra { modelo: Modelo::M100, casillas: vec![685] }.to_string(), "Lo que he leído no cuadra (casilla 0685).");
    }
}

#[cfg(test)]
mod como_la_web {
    //! Los cálculos (IRPF acumulado, IVA de un periodo, 303 y 130) comparados con lo que calculaba la app web de la
    //! que salió la app, con los mismos datos: `pruebas/<caso>/web.json` es su resultado, guardado; aquí se compara
    //! cada número (tolerancia 0,005). Los trimestres se encadenan: casillas 05/15 del 130 con los 130 anteriores
    //! del año; casilla 110 del 303 con las 87/71 del trimestre anterior.
    use super::{Memo, Modelo, Trimestral};
    use crate::dominio::fmt::{periodo, r2};
    use crate::dominio::Casillas;
    use crate::hacienda::irpf::{irpf_acumulado, Acumulado};
    use crate::hacienda::modelo_130::{self, OrigenRn, PorQue};
    use crate::hacienda::modelo_303::{self, Negativo};
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    fn dir_crate() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Lo que calculó la web para un caso.
    fn referencia(caso: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(caso.join("web.json")).unwrap()).unwrap()
    }

    /// Cómo lo escribía la web.
    fn origen_rn(o: OrigenRn) -> &'static str {
        match o {
            OrigenRn::Escrito => "manual",
            OrigenRn::Alta => "alta",
            OrigenRn::Calculado => "app",
            OrigenRn::NoSe => "desconocido",
        }
    }

    fn por_que(p: PorQue) -> &'static str {
        match p {
            PorQue::AnioAnterior => "anterior",
            PorQue::Trimestre => "trimestre",
            PorQue::TrimestreSinAnterior => "trimestre-sin-datos",
            PorQue::SinDatos => "sin-datos",
        }
    }

    #[derive(Default)]
    struct Cmp {
        n: usize,
        fallos: Vec<String>,
    }

    impl Cmp {
        fn num(&mut self, ruta: &str, js: &Value, rs: f64) {
            self.n += 1;
            if !js.as_f64().is_some_and(|j| (j - rs).abs() < 0.005) {
                self.fallos.push(format!("{ruta}: web={js} rust={rs}"));
            }
        }
        fn igual<T: PartialEq + std::fmt::Debug>(&mut self, ruta: &str, js: T, rs: T) {
            self.n += 1;
            if js != rs {
                self.fallos.push(format!("{ruta}: web={js:?} rust={rs:?}"));
            }
        }
        fn casillas(&mut self, ruta: &str, js: &Value, rs: &Casillas) {
            let m = js.as_object().unwrap_or_else(|| panic!("{ruta}: sin casillas"));
            let claves_js: Vec<u32> = m.keys().map(|k| k.parse().unwrap()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            let claves_rs: Vec<u32> = rs.keys().copied().collect();
            self.igual(&format!("{ruta} (casillas)"), claves_js, claves_rs);
            for (k, v) in m {
                self.num(&format!("{ruta}[{k}]"), v, rs.get(&k.parse::<u32>().unwrap()).copied().unwrap_or(f64::NAN));
            }
        }
        fn acumulado(&mut self, ruta: &str, js: &Value, a: &Acumulado) {
            for (k, v) in [
                ("ingresos", a.ingresos),
                ("retenciones", a.retenciones),
                ("ingresosConRet", a.ingresos_con_ret),
                ("gastos", a.gastos),
                ("gdj", a.gdj),
                ("rn", a.rn),
            ] {
                self.num(&format!("{ruta}.{k}"), &js[k], v);
            }
            let l = js["lineas"].as_object().unwrap();
            self.igual(&format!("{ruta}.lineas (claves)"), l.len(), a.lineas.len());
            for (k, v) in l {
                self.num(&format!("{ruta}.lineas.{k}"), v, a.lineas.get(k.as_str()).copied().unwrap_or(f64::NAN));
            }
        }
    }

    /// Compara todo lo que da la web para un caso con Rust. Devuelve cuántos valores se han comparado.
    fn comparar(caso: &Path) -> usize {
        let datos = caso.display();
        let web = referencia(caso);
        let mut d = crate::dominio::almacen::cargar(caso).unwrap();
        // Lo leído de los PDF (estos datos de prueba no tienen PDF: va aparte).
        if let Ok(t) = std::fs::read_to_string(caso.join("lecturas.json")) {
            d.lecturas = serde_json::from_str(&t).unwrap();
        }
        let mut c = Cmp::default();
        c.num("primerAnio", &web["primerAnio"], d.primer_anio_con_apuntes() as f64);
        // Con un memo compartido por todo (como una pantalla que lo enseña todo): tiene que dar lo mismo.
        let mut memo = Memo::default();
        let (m303, m130) = (Trimestral::M303, Trimestral::M130);
        for (y, w) in web["anios"].as_object().unwrap() {
            let y: i32 = y.parse().unwrap();
            for (f, a) in w["irpf"].as_object().unwrap() {
                c.acumulado(&format!("{y} irpf {f}"), a, &irpf_acumulado(&d, y, f));
            }
            let rn = modelo_130::rn_anterior(&d, y);
            c.igual(&format!("{y} rnAnterior.fuente"), w["rnAnterior"]["fuente"].as_str().unwrap(), origen_rn(rn.origen));
            c.igual(&format!("{y} rnAnterior.valor"), w["rnAnterior"]["valor"].as_f64().map(r2), rn.valor.map(r2));
            c.igual(&format!("{y} gdjEn130"), w["gdjEn130"].as_bool().unwrap(), modelo_130::con_gdj(&d));
            let (ini, fin) = (format!("{y}-01-01"), format!("{y}-12-31"));
            c.casillas(&format!("{y} iva año"), &w["iva"]["anio"], &modelo_303::iva_periodo(&d, &ini, &fin));
            for q in 0..=4u32 {
                let (wj, e) = (&w["exento130"][q.to_string()], modelo_130::exento(&d, y, q));
                c.igual(&format!("{y}-{q}T exento130.exento"), wj["exento"].as_bool().unwrap(), e.exento);
                c.igual(&format!("{y}-{q}T exento130.fuente"), wj["fuente"].as_str().unwrap(), por_que(e.por_que));
                c.num(&format!("{y}-{q}T exento130.pct"), &wj["pct"], e.pct);
                c.igual(&format!("{y}-{q}T exento130.sugerido"), wj["sugerido"].as_bool(), e.sugerido);
            }
            for q in 1..=4u32 {
                let k = q.to_string();
                let r = |m: &str| format!("{y}-{q}T {m}");
                let (ini, fin) = periodo(y, q);
                c.casillas(&r("iva"), &w["iva"][&k], &modelo_303::iva_periodo(&d, &ini, &fin));
                c.casillas(&r("calc303"), &w["calc303"][&k], &m303.calcular(&d, y, q, &mut Memo::default()));
                c.casillas(&r("get303"), &w["get303"][&k], &m303.cuenta(&d, y, q, &mut Memo::default()));
                c.casillas(&r("calc130"), &w["calc130"][&k], &m130.calcular(&d, y, q, &mut Memo::default()));
                c.casillas(&r("get130"), &w["get130"][&k], &m130.cuenta(&d, y, q, &mut Memo::default()));
                c.casillas(&r("get303 (memo compartido)"), &w["get303"][&k], &m303.cuenta(&d, y, q, &mut memo));
                c.casillas(&r("get130 (memo compartido)"), &w["get130"][&k], &m130.cuenta(&d, y, q, &mut memo));
                c.casillas(&r("calc303 (memo compartido)"), &w["calc303"][&k], &m303.calcular(&d, y, q, &mut memo));
                c.casillas(&r("calc130 (memo compartido)"), &w["calc130"][&k], &m130.calcular(&d, y, q, &mut memo));
                c.num(&r("pendienteCompensar"), &w["pendiente"][&k], modelo_303::pendiente_compensar(&d, y, q, &mut Memo::default()));
                let negativo = match modelo_303::negativo(&d, y, q) {
                    Negativo::Devolver => "devolver",
                    Negativo::Compensar => "compensar",
                };
                c.igual(&r("decision303"), w["decision303"][&k].as_str().unwrap_or(""), negativo);
                c.igual(&r("no130"), w["no130"][&k].as_bool().unwrap(), modelo_130::se_libra(&d, y, q));
                for (m, wl) in [(Modelo::M303, &w["leido303"][&k]), (Modelo::M130, &w["leido130"][&k])] {
                    let l = m.leido(&d, y, q);
                    c.igual(&r(&format!("leido{m}")), !wl.is_null(), l.is_some());
                    if let Some(l) = l.filter(|_| !wl.is_null()) {
                        c.casillas(&r(&format!("leido{m}.casillas")), &wl["casillas"], &l.casillas);
                        c.igual(&r(&format!("leido{m}.tipo")), wl["tipo"].as_str().unwrap(), l.tipo.map_or("", |t| t.letra()));
                    }
                }
            }
        }
        assert!(c.fallos.is_empty(), "{datos}: {} de {} valores distintos de la web:\n{}", c.fallos.len(), c.n, c.fallos.join("\n"));
        eprintln!("{datos}: {} valores iguales que la web", c.n);
        c.n
    }

    #[test]
    fn como_la_web() {
        let mut v: Vec<_> = std::fs::read_dir(dir_crate().join("pruebas")).unwrap().map(|e| e.unwrap().path()).filter(|p| p.join("web.json").is_file()).collect();
        v.sort();
        assert_eq!(v.len(), 3);
        let n: usize = v.iter().map(|p| comparar(p)).sum();
        assert!(n > 18_000, "solo {n} valores");
    }
}
