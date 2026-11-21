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
    /// No se ve el devengo (el ejercicio y el periodo) en el impreso.
    SinDevengo(Modelo),
    /// No se ven casillas que salen en todos los PDF de ese modelo (`Formulario::ANCLAS`).
    FaltaCasilla { modelo: Modelo, faltan: Vec<u32>, vistas: Vec<u32> },
    /// La portada no dice nada que se entienda de cómo se presentó (a ingresar, a compensar…).
    SinTipo(Modelo),
    /// En la Renta, un bloque (trabajo, capital, actividad…) tiene sus casillas pero no cuadran entre sí.
    Bloque { bloque: &'static str, regla: String, casillas: Vec<(u32, f64)> },
    /// Lo leído de la Renta no cuadra consigo mismo (las comprobaciones de `modelo_100::comprobar`).
    Incoherente(Vec<String>),
    /// Lo leído no cuadra con las sumas del propio modelo en estas casillas.
    NoCuadra { modelo: Modelo, casillas: Vec<Descuadre> },
    /// El PDF no se puede abrir.
    Pdf(String),
    /// El lector se ha roto con este PDF (un fallo de la app): lo que dijo al romperse.
    Roto(String),
}

/// Una casilla que no cuadra: lo leído y lo que debería valer según las demás.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Descuadre {
    pub casilla: u32,
    pub leido: f64,
    pub debe: f64,
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
            Fallo::SinDevengo(m) => write!(f, "No encuentro en el PDF de qué ejercicio y periodo es (el devengo del {m})."),
            Fallo::FaltaCasilla { modelo, faltan, .. } => {
                let n = modelo.cifras();
                let v: Vec<String> = faltan.iter().map(|c| format!("{c:0n$}")).collect();
                let (s, que) = if v.len() > 1 { ("s", "salen") } else { ("", "sale") };
                write!(f, "No encuentro en el PDF la casilla{s} {}, que {que} en todos los del {modelo}.", v.join(", "))
            }
            Fallo::SinTipo(m) => write!(f, "No entiendo en la portada del {m} cómo se presentó (a ingresar, a compensar, a devolver…)."),
            Fallo::Bloque { bloque, regla, .. } => write!(f, "En la Renta, lo de {bloque} no cuadra: {regla}."),
            Fallo::Incoherente(mal) => write!(f, "Lo que he leído de la Renta no cuadra consigo mismo: {}.", mal.join("; ")),
            Fallo::NoCuadra { modelo, casillas } => {
                let n = modelo.cifras();
                let v: Vec<String> = casillas.iter().map(|c| format!("{:0n$}", c.casilla)).collect();
                write!(f, "Lo que he leído no cuadra (casilla{} {}).", if v.len() > 1 { "s" } else { "" }, v.join(", "))
            }
            Fallo::Pdf(e) => f.write_str(e),
            Fallo::Roto(_) => f.write_str("El lector se ha roto con este PDF."),
        }
    }
}

/// Para el informe de un fallo (`dominio::fallo`): su código, el código que lo lee y qué se esperaba y qué se
/// encontró. Lo que sale del texto del PDF (el fragmento) lo pone `lectura`.
impl Fallo {
    /// Un código estable, con el modelo si se sabe: «AEAT_303_NO_CUADRA».
    pub fn codigo(&self) -> String {
        let (modelo, que) = match self {
            Fallo::Anterior(m) => (Some(*m), "ANTERIOR"),
            Fallo::Periodo { modelo, .. } => (Some(*modelo), "PERIODO"),
            Fallo::SinDiseno { modelo, .. } => (Some(*modelo), "SIN_DISENO"),
            Fallo::OtroModelo { pedido, .. } => (Some(*pedido), "OTRO_MODELO"),
            Fallo::OtroPeriodo { .. } => (None, "OTRO_PERIODO"),
            Fallo::NoEsLaRenta(_) => (Some(Modelo::M100), "NO_ES_LA_RENTA"),
            Fallo::Numeracion(_) => (Some(Modelo::M100), "NUMERACION"),
            Fallo::SinCasillas => (None, "SIN_CASILLAS"),
            Fallo::SinDevengo(m) => (Some(*m), "SIN_DEVENGO"),
            Fallo::FaltaCasilla { modelo, .. } => (Some(*modelo), "FALTA_CASILLA"),
            Fallo::SinTipo(m) => (Some(*m), "SIN_TIPO"),
            Fallo::Bloque { .. } => (Some(Modelo::M100), "BLOQUE_NO_CUADRA"),
            Fallo::Incoherente(_) => (Some(Modelo::M100), "INCOHERENTE"),
            Fallo::NoCuadra { modelo, .. } => (Some(*modelo), "NO_CUADRA"),
            Fallo::Pdf(_) => (None, "PDF_NO_SE_ABRE"),
            Fallo::Roto(_) => (None, "LECTOR_ROTO"),
        };
        match modelo {
            Some(m) => format!("AEAT_{m}_{que}"),
            None => format!("AEAT_{que}"),
        }
    }

    /// El código que lo lee (fichero y función; la prueba `los_sitios_existen` comprueba que existen).
    pub fn donde(&self) -> String {
        let s = match self {
            Fallo::Bloque { bloque, .. } => return format!("src/hacienda/modelo_100.rs · {bloque} (dentro de leer)"),
            Fallo::Anterior(Modelo::M100) | Fallo::NoEsLaRenta(_) => "src/hacienda/modelo_100.rs · leer (es_modelo_100, es_del_anio)",
            Fallo::Numeracion(_) => "src/hacienda/modelo_100.rs · leer (NUMERACIONES)",
            Fallo::NoCuadra { modelo: Modelo::M100, .. } => "src/hacienda/modelo_100.rs · leer",
            Fallo::NoCuadra { .. } => "src/hacienda/formulario.rs · leer → descuadres (las sumas, en `reglas` del modelo_XXX.rs de su modelo)",
            Fallo::Anterior(_) | Fallo::Periodo { .. } | Fallo::SinDiseno { .. } => "src/hacienda/formulario.rs · leer",
            Fallo::OtroModelo { .. } => "src/hacienda/formulario.rs · comprobar (el modelo: src/hacienda/generico.rs · modelo)",
            Fallo::OtroPeriodo { .. } => "src/hacienda/formulario.rs · comprobar (el periodo: devengo)",
            Fallo::SinCasillas => "src/hacienda/formulario.rs · leer (las casillas: src/hacienda/generico.rs · pares_formulario, letra_rellenada)",
            Fallo::SinDevengo(_) => "src/hacienda/formulario.rs · devengo (desde comprobar)",
            Fallo::FaltaCasilla { .. } => "src/hacienda/formulario.rs · leer (las anclas: `ANCLAS` en el modelo_XXX.rs de su modelo; las casillas: src/hacienda/generico.rs · pares_formulario)",
            Fallo::SinTipo(_) => "src/hacienda/formulario.rs · tipo",
            Fallo::Incoherente(_) => "src/hacienda/modelo_100.rs · comprobar (dentro de leer)",
            Fallo::Pdf(_) => "src/hacienda/pdf.rs · desde_bytes",
            Fallo::Roto(_) => "src/hacienda/lectura.rs · leer_pdf (se ha roto dentro del lector de su modelo)",
        };
        s.to_string()
    }

    /// Qué se esperaba encontrar en el PDF.
    pub fn esperado(&self) -> String {
        let de = |(a, p): &(i32, Option<Periodo>)| p.as_ref().map_or(format!("ejercicio {a}"), |p| format!("ejercicio {a}, periodo {p}"));
        match self {
            Fallo::OtroModelo { pedido, .. } => format!("el número del modelo {pedido} en la cabecera (en letra grande) o tras «Modelo»"),
            Fallo::OtroPeriodo { pedido, .. } => format!("el devengo «{}» (lo dice el nombre del PDF)", de(pedido)),
            Fallo::NoEsLaRenta(a) => format!("«Modelo 100» y el ejercicio {a} («Ejercicio {a}», «…Físicas {a}» o la referencia «{a}100…»)"),
            Fallo::Numeracion(_) => "cuota diferencial (0610) = cuota resultante (0595) − pagos a cuenta (0609), y el resultado (0670)".into(),
            Fallo::SinCasillas => "números de casilla con su importe a la derecha, en la misma línea y en la letra de los importes rellenados".into(),
            Fallo::NoCuadra { modelo, casillas } => {
                let n = modelo.cifras();
                casillas.iter().map(|c| format!("{:0n$} = {:.2} (según las demás casillas)", c.casilla, c.debe)).collect::<Vec<_>>().join("; ")
            }
            Fallo::SinDevengo(_) => "«Ejercicio AAAA Período PP» (o «AAAA PP») en el impreso, fuera de la portada".into(),
            Fallo::FaltaCasilla { modelo, faltan, .. } => {
                let n = modelo.cifras();
                format!("las casillas {} con su importe", faltan.iter().map(|c| format!("{c:0n$}")).collect::<Vec<_>>().join(", "))
            }
            Fallo::SinTipo(_) => "en la portada: A INGRESAR / NRC, A COMPENSAR, A DEVOLVER, DOMICILIA…, RECONOCIMIENTO DE DEUDA, NEGATIVA, SIN ACTIVIDAD o RESULTADO CERO".into(),
            Fallo::Bloque { regla, .. } => regla.clone(),
            Fallo::Incoherente(mal) => mal.join("; "),
            Fallo::Pdf(_) => "un PDF que se pueda abrir".into(),
            Fallo::Roto(_) => "que el lector no se rompa".into(),
            Fallo::Anterior(_) | Fallo::Periodo { .. } | Fallo::SinDiseno { .. } => String::new(),
        }
    }

    /// Qué se encontró (lo que se sabe sin el texto del PDF).
    pub fn encontrado(&self) -> String {
        match self {
            Fallo::OtroModelo { es: Some(es), .. } => format!("el modelo {es}"),
            Fallo::OtroModelo { es: None, .. } => "ningún número de modelo".into(),
            Fallo::OtroPeriodo { es: (a, p), .. } => p.as_ref().map_or(format!("ejercicio {a}"), |p| format!("ejercicio {a}, periodo {p}")),
            Fallo::NoCuadra { modelo, casillas } => {
                let n = modelo.cifras();
                casillas.iter().map(|c| format!("{:0n$} = {:.2}", c.casilla, c.leido)).collect::<Vec<_>>().join("; ")
            }
            Fallo::FaltaCasilla { modelo, vistas, .. } => {
                let n = modelo.cifras();
                format!("las casillas que se ven: {}", vistas.iter().map(|c| format!("{c:0n$}")).collect::<Vec<_>>().join(", "))
            }
            Fallo::Bloque { casillas, .. } => casillas.iter().map(|(c, x)| format!("{c:04} = {x:.2}")).collect::<Vec<_>>().join("; "),
            Fallo::Roto(e) => format!("el lector se ha roto: {e}"),
            _ => String::new(),
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
        let d = |casilla| Descuadre { casilla, leido: 1.0, debe: 2.5 };
        let f = Fallo::NoCuadra { modelo: Modelo::M303, casillas: vec![d(3), d(110)] };
        assert_eq!(f.to_string(), "Lo que he leído no cuadra (casillas 03, 110).");
        assert_eq!((f.codigo().as_str(), f.esperado().as_str(), f.encontrado().as_str()), ("AEAT_303_NO_CUADRA", "03 = 2.50 (según las demás casillas); 110 = 2.50 (según las demás casillas)", "03 = 1.00; 110 = 1.00"));
        assert_eq!(Fallo::NoCuadra { modelo: Modelo::M100, casillas: vec![d(685)] }.to_string(), "Lo que he leído no cuadra (casilla 0685).");
        assert_eq!(Fallo::SinCasillas.codigo(), "AEAT_SIN_CASILLAS");
    }

    /// Los ficheros y las funciones que dice `donde` existen (si se cambia el nombre de uno, esto lo dice): cada
    /// «src/….rs» y, en él, la función que va tras «·» y lo que tenga nombre_con_guiones.
    pub(crate) fn existe(donde: &str) {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let (mut fichero, mut tras_punto) = (String::new(), false);
        for p in donde.split([' ', '(', ')', ',', ':']).filter(|p| !p.is_empty()) {
            let nombre = p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if p.starts_with("src/") {
                fichero = std::fs::read_to_string(raiz.join(p)).unwrap_or_else(|_| panic!("no existe {p} ({donde})"));
            } else if nombre && (tras_punto || p.contains('_')) {
                assert!(["fn ", "const ", "struct "].iter().any(|k| fichero.contains(&format!("{k}{p}"))), "no está «{p}» ({donde})");
            }
            tras_punto = p == "·";
        }
    }

    #[test]
    fn los_sitios_existen() {
        let p = Periodo::Anual;
        let todos = [
            Fallo::Anterior(Modelo::M100),
            Fallo::Anterior(Modelo::M303),
            Fallo::Periodo { modelo: Modelo::M303, periodicidad: Periodicidad::Trimestral, periodo: p.clone() },
            Fallo::SinDiseno { modelo: Modelo::M303, ejercicio: 2020, periodo: p.clone() },
            Fallo::OtroModelo { pedido: Modelo::M303, es: None },
            Fallo::OtroPeriodo { pedido: (2020, None), es: (2021, None) },
            Fallo::NoEsLaRenta(2020),
            Fallo::Numeracion(2020),
            Fallo::SinCasillas,
            Fallo::SinDevengo(Modelo::M303),
            Fallo::FaltaCasilla { modelo: Modelo::M303, faltan: vec![], vistas: vec![] },
            Fallo::SinTipo(Modelo::M303),
            Fallo::Bloque { bloque: "trabajo", regla: String::new(), casillas: vec![] },
            Fallo::Bloque { bloque: "capital", regla: String::new(), casillas: vec![] },
            Fallo::Bloque { bloque: "actividad", regla: String::new(), casillas: vec![] },
            Fallo::Bloque { bloque: "ganancias", regla: String::new(), casillas: vec![] },
            Fallo::Bloque { bloque: "pagos", regla: String::new(), casillas: vec![] },
            Fallo::Bloque { bloque: "cuotas", regla: String::new(), casillas: vec![] },
            Fallo::Incoherente(vec![]),
            Fallo::NoCuadra { modelo: Modelo::M100, casillas: vec![] },
            Fallo::NoCuadra { modelo: Modelo::M303, casillas: vec![] },
            Fallo::Pdf(String::new()),
            Fallo::Roto(String::new()),
        ];
        for f in todos {
            existe(&f.donde());
        }
    }
}
