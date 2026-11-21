//! Cuando algo de fuera no se entiende (un PDF de Hacienda, una página de la Seguridad Social): el `Error`, con lo
//! necesario para arreglarlo. Se guarda con lo leído o traído (`fuente::Fuente::error`) y la app lo enseña hasta
//! que se arregla. Su `informe` es un bloque de texto para copiar y pegárselo a un LLM: qué se esperaba, qué se
//! encontró, qué código lo lee y cómo reproducirlo, con los datos personales que se reconocen tapados (`tapar`): hay
//! que revisarlo antes de pegarlo.
//!
//! La regla de cuándo es un fallo (AVANZADO.md, «Cuando algo falla»): vacío o cero solo vale si la página o el PDF
//! lo dice (su señal de «sin datos») o tiene las anclas de su estructura; si faltan las anclas, o lo que hay no se
//! entiende, es un fallo, nunca un 0 ni un vacío.
use serde::{Deserialize, Serialize};

/// Por qué no se ha podido leer o traer algo. Solo `mensaje` es para la persona; lo demás, para arreglarlo.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Error {
    /// Lo que se le enseña a la persona.
    pub mensaje: String,
    /// Un código estable: el mismo fallo, el mismo código («AEAT_303_NO_CUADRA», «TGSS_RECIBOS_SIN_LISTA»).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub codigo: String,
    /// Qué se estaba leyendo: «PDF del modelo 303 del 1T de 2025», «Importass: https://…/historialRecibosRETA».
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fuente: String,
    /// El código que lo lee: «src/hacienda/formulario.rs · leer».
    #[serde(skip_serializing_if = "String::is_empty")]
    pub donde: String,
    /// La app con la que pasó («0.1.0 · 1a2b3c4d5e6f») y su lector («PDF v4»).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gesshtor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lector: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub esperado: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub encontrado: String,
    /// Un trozo corto del texto de alrededor, ya con los datos personales tapados.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fragmento: String,
    /// Cómo volver a verlo («gesshtor json justificantes/2025_1T_303_‹CSV›.pdf»).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reproducir: String,
}

/// Dónde se ve un fallo: este fichero y la función que se diga («src/tgss/sede.rs · leer_liquidacion»).
#[macro_export]
macro_rules! donde {
    ($funcion:literal) => {
        concat!(file!(), " · ", $funcion)
    };
}

/// Líneas que lleva como mucho un fragmento, y caracteres por línea.
const LINEAS: usize = 8;
const ANCHO: usize = 160;

impl Error {
    /// Un fallo con solo su mensaje (no ha llegado nada, no se puede guardar…).
    pub fn de(mensaje: impl Into<String>) -> Error {
        Error { mensaje: mensaje.into(), gesshtor: crate::VERSION.into(), ..Default::default() }
    }

    /// Lo que ha llegado no es lo que se esperaba: su código y el código que lo lee (`donde!`).
    pub fn formato(codigo: impl Into<String>, donde: &str, mensaje: impl Into<String>) -> Error {
        Error { codigo: codigo.into(), donde: donde.into(), ..Error::de(mensaje) }
    }

    pub fn esperado(self, s: impl Into<String>) -> Self {
        Error { esperado: s.into(), ..self }
    }

    pub fn encontrado(self, s: impl Into<String>) -> Self {
        Error { encontrado: s.into(), ..self }
    }

    /// El trozo de alrededor: como mucho `LINEAS` líneas no vacías (las largas, partidas), y tapado (con esos
    /// nombres además de los que se ven en él).
    pub fn fragmento(self, s: &str, nombres: &[&str]) -> Self {
        let lineas: Vec<String> = s.lines().map(str::trim).filter(|l| !l.is_empty()).flat_map(partir).take(LINEAS).collect();
        Error { fragmento: tapar(&lineas.join("\n"), nombres), ..self }
    }

    /// De qué es (si no lo dice ya).
    pub fn en(self, fuente: impl Into<String>) -> Self {
        if self.fuente.is_empty() { Error { fuente: fuente.into(), ..self } } else { self }
    }

    pub fn lector(self, s: impl Into<String>) -> Self {
        Error { lector: s.into(), ..self }
    }

    pub fn reproducir(self, s: impl Into<String>) -> Self {
        Error { reproducir: s.into(), ..self }
    }

    /// El informe para copiar: todo lo que se sabe del fallo, en texto, con los datos personales tapados (también
    /// estos `nombres`: los de la persona). La versión, el código y el sitio del código, tal cual.
    pub fn informe(&self, nombres: &[&str]) -> String {
        let version = [self.gesshtor.as_str(), self.lector.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · lector ");
        let fragmento: String = self.fragmento.lines().map(|l| format!("\n    {l}")).collect();
        let t = |s: &str| tapar(s, nombres);
        let lineas = [
            ("Código", if self.codigo.is_empty() { "(sin código)".to_string() } else { self.codigo.clone() }),
            ("Qué pasa", t(&self.mensaje)),
            ("Fuente", t(&self.fuente)),
            ("Código que lo lee", self.donde.clone()),
            ("Versión de gesshtor", version),
            ("Se esperaba", t(&self.esperado)),
            ("Se encontró", t(&self.encontrado)),
            ("Fragmento (revísalo antes de pegarlo)", t(&fragmento)),
            ("Para reproducirlo", t(&self.reproducir)),
        ];
        let v: Vec<String> = lineas.iter().filter(|(_, x)| !x.is_empty()).map(|(n, x)| if x.starts_with('\n') { format!("{n}:{x}") } else { format!("{n}: {x}") }).collect();
        v.join("\n")
    }
}

impl From<String> for Error {
    fn from(mensaje: String) -> Self {
        Error::de(mensaje)
    }
}

/// Varios fallos en un solo texto para copiar, con lo que hace falta para entenderlo fuera de la app.
pub fn informes<'a>(fallos: impl IntoIterator<Item = &'a Error>, nombres: &[&str]) -> String {
    let fallos: Vec<&Error> = fallos.into_iter().collect();
    let mut t = format!(
        "Informe de fallos de gesshtor ({} {}). Para arreglarlo, pégaselo a un LLM con el código de gesshtor \
         (https://github.com/alrevuelta/gesshtor): cada fallo dice qué se esperaba, qué se encontró y qué código lo lee. \
         Se tapan los datos personales que se reconocen (NIF, IBAN, nombres…), pero no todos: revísalo antes de pegarlo.\n",
        fallos.len(),
        if fallos.len() == 1 { "fallo" } else { "fallos" }
    );
    for (i, e) in fallos.iter().enumerate() {
        t.push_str(&format!("\n--- {} de {} ---\n{}\n", i + 1, fallos.len(), e.informe(nombres)));
    }
    t
}

/// Una línea en trozos de como mucho `ANCHO` caracteres, por palabras (una palabra más larga, cortada).
fn partir(l: &str) -> Vec<String> {
    let mut v: Vec<String> = vec![String::new()];
    for p in l.split_whitespace() {
        let p: String = if p.chars().count() > ANCHO { p.chars().take(ANCHO - 1).chain(['…']).collect() } else { p.to_string() };
        let ultima = v.last_mut().expect("hay una");
        if ultima.is_empty() {
            *ultima = p;
        } else if ultima.chars().count() + 1 + p.chars().count() <= ANCHO {
            ultima.push(' ');
            ultima.push_str(&p);
        } else {
            v.push(p);
        }
    }
    v
}

/// Una URL sin lo que puede llevar datos o sesiones: la consulta («?…»), el ancla, «;jsessionid=…» y el estado del
/// portal («/!ut/p/…»).
pub fn sin_consulta(url: &str) -> String {
    let corta = url.split(['?', '#', ';']).next().unwrap_or("");
    match corta.find("/!ut/") {
        Some(i) => format!("{}/!ut/…", &corta[..i]),
        None => corta.to_string(),
    }
}

/* ================= tapar los datos personales ================= */

/// Palabras en mayúsculas que no son de un nombre (las etiquetas que van junto a uno).
const NO_SON_NOMBRES: [&str; 18] = ["NIF", "NIE", "CIF", "DNI", "IVA", "IRPF", "AEAT", "CNAE", "IAE", "CCC", "NRC", "CSV", "SL", "SA", "SLU", "S.L.", "S.A.", "Y"];

/// ¿Es una palabra en mayúsculas («GARCÍA», «M.», «PÉREZ,ANA»)? Las de una letra (iniciales) también.
fn mayusculas(p: &str) -> bool {
    let letras: Vec<char> = p.chars().filter(|c| c.is_alphabetic()).collect();
    !letras.is_empty() && letras.iter().all(|c| c.is_uppercase()) && p.chars().all(|c| c.is_alphabetic() || ".,'-".contains(c))
}

/// Las etiquetas que llevan un nombre detrás («Apellidos y nombre», «Razón social:», «NIF Presentador:»…).
const ETIQUETAS: [&str; 14] = [
    "nombre", "social", "apellidos", "denominación", "titular", "declarante", "declarado", "presentador", "cónyuge", "conyuge", "empresa", "relacionarse", "usuario", "afiliado",
];

/// Las palabras de los nombres que se ven en un texto: las de mayúsculas que siguen a un NIF («12345678Z GARCÍA
/// PÉREZ ANA») o a una etiqueta de nombre («Razón social: EMPRESA INVENTADA SL», «Apellidos y nombre ANA…»); si la
/// etiqueta acaba la línea, las del principio de la siguiente (los impresos ponen el nombre debajo); y las que van
/// pegadas a una etiqueta («socialGARCÍA»).
pub fn nombres_en(t: &str) -> Vec<String> {
    let lineas: Vec<Vec<&str>> = t.lines().map(|l| l.split_whitespace().collect()).collect();
    let mut out = Vec::new();
    let mut tomar = |ps: &[&str]| {
        let siguen = ps.iter().skip_while(|p| **p == "/" || **p == ":").take_while(|p| mayusculas(p));
        let n = out.len();
        out.extend(siguen.flat_map(|p| p.split(|c: char| !c.is_alphabetic())).filter(|p| !p.is_empty() && !NO_SON_NOMBRES.contains(p)).map(str::to_string));
        out.len() > n
    };
    for (k, ps) in lineas.iter().enumerate() {
        for (i, p) in ps.iter().enumerate() {
            let letras: String = p.chars().filter(|c| c.is_alphabetic()).collect();
            let minusculas: String = letras.chars().take_while(|c| c.is_lowercase()).collect();
            let etiqueta = ETIQUETAS.contains(&letras.to_lowercase().as_str()) || (ETIQUETAS.contains(&minusculas.as_str()) && letras.len() > minusculas.len());
            if !etiqueta && !es_nif(&p.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()) {
                continue;
            }
            // Pegado a la etiqueta: «socialGARCÍA».
            if letras.len() > minusculas.len() && !minusculas.is_empty() {
                tomar(&[&p[p.char_indices().find(|(_, c)| c.is_uppercase()).map_or(p.len(), |(i, _)| i)..]]);
            }
            // La etiqueta acaba la línea: el nombre va debajo.
            if !tomar(&ps[i + 1..])
                && i + 1 == ps.len()
                && let Some(sig) = lineas.get(k + 1)
            {
                tomar(sig);
            }
        }
    }
    out
}

/// ¿Lleva dentro un NIF, un NIE o un CIF? (8 cifras y una letra; X, Y o Z, 7 cifras y una letra; una letra, 7
/// cifras y una cifra o letra). Basta con que lo lleve dentro: en el «Expediente» va pegado a otras cifras.
fn es_nif(w: &str) -> bool {
    let c: Vec<char> = w.chars().collect();
    let cifras = |i: usize, n: usize| i + n <= c.len() && c[i..i + n].iter().all(char::is_ascii_digit);
    let letra = |i: usize| c.get(i).is_some_and(char::is_ascii_uppercase);
    (0..c.len()).any(|i| {
        (cifras(i, 8) && letra(i + 8))
            || ("XYZ".contains(c[i]) && cifras(i + 1, 7) && letra(i + 8))
            || (letra(i) && cifras(i + 1, 7) && c.get(i + 8).is_some_and(|x| x.is_ascii_digit() || ('A'..='J').contains(x)) && !c.get(i + 9).is_some_and(char::is_ascii_digit))
    })
}

/// Cambia el IBAN («ES12 3456 7890 …», con o sin espacios) por «‹IBAN›».
fn sin_iban(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    while i < c.len() {
        let empieza = i + 4 <= c.len() && c[i].is_ascii_uppercase() && c[i + 1].is_ascii_uppercase() && c[i + 2].is_ascii_digit() && c[i + 3].is_ascii_digit();
        if empieza && (i == 0 || !c[i - 1].is_alphanumeric()) {
            let (mut j, mut cifras) = (i + 2, 0);
            while j < c.len() && (c[j].is_ascii_digit() || (c[j] == ' ' && c.get(j + 1).is_some_and(char::is_ascii_digit))) {
                cifras += usize::from(c[j].is_ascii_digit());
                j += 1;
            }
            if cifras >= 18 {
                out.push_str("‹IBAN›");
                i = j;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// Una tira de letras y cifras: lo que se tapa de ella (o ella misma).
fn tapar_tira(w: &str, nombres: &[String]) -> String {
    let cifras = w.chars().filter(char::is_ascii_digit).count();
    let letras = w.chars().filter(|c| c.is_alphabetic()).count();
    let es_nombre = |w: &str| w.chars().count() >= 3 && nombres.iter().any(|n| n.to_uppercase() == w.to_uppercase());
    // Un nombre pegado detrás de una palabra en minúsculas («socialGARCÍA»).
    let pegado = w.char_indices().find(|(_, c)| c.is_uppercase()).filter(|(i, _)| *i > 0 && w[..*i].chars().all(char::is_lowercase)).map(|(i, _)| i);
    if es_nif(w) {
        "‹NIF›".into()
    } else if cifras == w.len() && cifras >= 8 {
        "‹número›".into()
    } else if w.chars().count() >= 12 && cifras > 0 && letras > 0 {
        "‹código›".into()
    } else if es_nombre(w) {
        "‹nombre›".into()
    } else if let Some(i) = pegado.filter(|&i| es_nombre(&w[i..])) {
        format!("{}‹nombre›", &w[..i])
    } else {
        w.to_string()
    }
}

/// Las etiquetas tras las que se tapa todo, hasta el final de la línea: lo que llevan detrás (un domicilio, una
/// fecha de nacimiento, quien te paga) no se reconoce por su forma.
const HASTA_EL_FINAL: [&str; 4] = ["domicilio", "calle", "nacimiento", "pagador"];

/// Dónde acaba la primera etiqueta de `HASTA_EL_FINAL` de una línea (como palabra entera, con su «:»), si lleva
/// algo detrás.
fn hasta_el_final(linea: &str) -> Option<usize> {
    let mut desde = 0;
    for p in linea.split(' ') {
        let fin = desde + p.len();
        let letras: String = p.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
        if HASTA_EL_FINAL.contains(&letras.as_str()) && !linea[fin..].trim().is_empty() {
            return Some(fin);
        }
        desde = fin + 1;
    }
    None
}

/// Un texto con los datos personales tapados: NIF, NIE y CIF (también con puntos o guion), IBAN, correos, números
/// largos (justificantes, cuentas de cotización, números de afiliación: 8 cifras o más), códigos y fichas (CSV,
/// sesiones), lo que sigue a un domicilio, una fecha de nacimiento o un pagador (`HASTA_EL_FINAL`) y los nombres:
/// estos `nombres` y los que se ven junto a un NIF o una etiqueta de nombre (`nombres_en`). Mejor tapar de más que
/// de menos; aun así, no todo se reconoce (un nombre suelto, una razón social en minúsculas): por eso el informe
/// pide revisarlo, y a él solo llega la estructura de las páginas y los PDF, no su texto.
pub fn tapar(t: &str, nombres: &[&str]) -> String {
    let mut palabras: Vec<String> = nombres.iter().flat_map(|n| n.split_whitespace()).map(str::to_string).chain(nombres_en(t)).collect();
    palabras.retain(|p| p.chars().filter(|c| c.is_alphabetic()).count() >= 3 && !NO_SON_NOMBRES.contains(&p.to_uppercase().as_str()));
    let t = sin_iban(t);
    let mut out = String::with_capacity(t.len());
    for (i, linea) in t.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let corte = hasta_el_final(linea);
        let linea = &linea[..corte.unwrap_or(linea.len())];
        let mut l = String::with_capacity(linea.len());
        for (j, palabra) in linea.split(' ').enumerate() {
            if j > 0 {
                l.push(' ');
            }
            if palabra.contains('@') && palabra.contains('.') {
                l.push_str("‹correo›");
                continue;
            }
            // Con separadores, entera: un NIF «12.345.678-Z» o un número de afiliación «28/12345678/90» (no un
            // importe, que lleva coma, ni una fecha, que tiene 8 cifras).
            let compacto: String = palabra.chars().filter(|c| c.is_alphanumeric()).collect();
            let cifras_y_separadores = palabra.trim_matches(|c: char| !c.is_alphanumeric()).chars().all(|c| c.is_ascii_digit() || "/-.".contains(c));
            if palabra.contains(['-', '.', '/']) && compacto.len() <= 10 && es_nif(&compacto) {
                l.push_str("‹NIF›");
                continue;
            }
            if cifras_y_separadores && compacto.len() >= 9 && !palabra.contains(',') {
                l.push_str("‹número›");
                continue;
            }
            // Cada tira de letras y cifras, por separado (lo de alrededor se queda: «(0670)», «=…», «/»).
            let mut tira = String::new();
            for c in palabra.chars().chain(['\u{0}']) {
                if c.is_alphanumeric() {
                    tira.push(c);
                    continue;
                }
                l.push_str(&tapar_tira(&tira, &palabras));
                tira.clear();
                if c != '\u{0}' {
                    l.push(c);
                }
            }
        }
        // Un nombre entero, una sola vez: «‹nombre› ‹nombre› ‹nombre›» → «‹nombre›».
        while l.contains("‹nombre› ‹nombre›") {
            l = l.replace("‹nombre› ‹nombre›", "‹nombre›");
        }
        if corte.is_some() {
            l.push_str(" ‹tapado›");
        }
        out.push_str(&l);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapa_los_datos_personales() {
        let t = "Expediente: 202330300000000T\nNIF Presentador: 00000000T\nApellidos y Nombre / Razón social: PRUEBA INVENTADA ANA\n\
                 Apellidos y nombre, razón social o denominación del declarado\nOTRO SEÑOR,DE PRUEBA\nModelo 303 denominación socialTERCERO INVENTADO Pág. 1\n\
                 00000000T PRUEBA INVENTADA ANA Ejercicio 2025 Período 1T\nCSV PRUEBA303A1T0001 · justificante 3030000000001\n\
                 IBAN ES12 0000 0000 0000 0000 0000 y ES1200000000000000000000\nCorreo ana@ejemplo.es · CIF B00000000 · NIE X0000000T\n\
                 Total cuota devengada 27 1.234,56 · casilla 0670 · LUIS dice hola\n\
                 DNI 00.000.000-T · NAF 28/00000000/00 · CCC 28000000000 · el 15/04/2026 · 12345678 · 29.747,92";
        let x = tapar(t, &["Luis Ejemplo"]);
        for no in ["00000000T", "PRUEBA", "INVENTADA", "ANA", "3030000000001", "ES12", "ana@", "B00000000", "X0000000T", "LUIS", "303A1T", "OTRO", "SEÑOR", "TERCERO", "00.000.000-T", "28/0000", "28000000000", "12345678"] {
            assert!(!x.contains(no), "«{no}» en:\n{x}");
        }
        for si in ["Ejercicio 2025 Período 1T", "Total cuota devengada 27 1.234,56", "casilla 0670", "Expediente: ‹NIF›", "social: ‹nombre›", "‹IBAN› y ‹IBAN›", "dice hola", "el 15/04/2026", "29.747,92"] {
            assert!(x.contains(si), "falta «{si}» en:\n{x}");
        }
        assert_eq!(tapar("Base de cotización 1.000,00 € · CONTINGENCIAS COMUNES", &[]), "Base de cotización 1.000,00 € · CONTINGENCIAS COMUNES");
        assert_eq!(sin_consulta("https://portal.seg-social.gob.es/wps/myportal/x/personal/!ut/p/z1/04_Sj9CPyk/?a=1#b"), "https://portal.seg-social.gob.es/wps/myportal/x/personal/!ut/…");
        assert_eq!(sin_consulta("https://sp.seg-social.es/ProsaInternet/OnlineAccessUtf8;jsessionid=SES0001?ARQ.SPM.TICKET=T9"), "https://sp.seg-social.es/ProsaInternet/OnlineAccessUtf8");
    }

    /// Un domicilio, una fecha de nacimiento o quien te paga no se reconocen por su forma: se tapa lo que sigue a su
    /// etiqueta, hasta el final de la línea (la etiqueta, como palabra entera).
    #[test]
    fn tapa_lo_que_sigue_a_un_domicilio() {
        let t = "Domicilio: Calle Mayor 12 3ºB 28013 Madrid\nFecha de nacimiento 03/05/1984 Teléfono 600123123\nPagador Talleres Martínez Hermanos\nEn la calle: Alcalá";
        let x = tapar(t, &[]);
        assert_eq!(x, "Domicilio: ‹tapado›\nFecha de nacimiento ‹tapado›\nPagador ‹tapado›\nEn la calle: ‹tapado›");
        // Lo de la app, tal cual: «DOMICILIA…» no es «domicilio».
        let e = "en la portada: A INGRESAR / NRC, A COMPENSAR, DOMICILIACIÓN, DOMICILIA…";
        assert_eq!(tapar(e, &[]), e);
        // Y el informe pide revisarlo: no todo se reconoce.
        let t = informes([&Error::de("x").fragmento("algo", &[])], &[]);
        assert!(t.contains("revísalo antes de pegarlo"), "{t}");
    }

    #[test]
    fn el_informe_se_copia_entero_y_tapado() {
        let e = Error::formato("TGSS_RECIBOS_SIN_LISTA", donde!("leer_recibos"), "No encuentro tus recibos en la página.")
            .en(format!("Importass: {}", sin_consulta("https://portal.seg-social.gob.es/x/historialRecibosRETA?pagina=2")))
            .esperado("la lista de recibos (class=\"quota-history\")")
            .encontrado("ninguna; clases de la página: ss-card, ss-title")
            .fragmento("\n  Historial de recibos de ANA PRUEBA  \n\n00000000T\n", &["Ana Prueba"])
            .reproducir("S en la app (o gesshtor --certificado tu.p12 --sincronizar)");
        assert_eq!(e.donde, "src/dominio/fallo.rs · leer_recibos");
        let t = informes([&e], &[]);
        assert!(t.starts_with("Informe de fallos de gesshtor (1 fallo)."), "{t}");
        let e = Error { gesshtor: "0.1.0 · 1a2b3c4d5e6f".into(), ..e };
        let t = informes([&e], &[]);
        for si in ["Versión de gesshtor: 0.1.0 · 1a2b3c4d5e6f\n", "Código: TGSS_RECIBOS_SIN_LISTA", "Qué pasa: No encuentro tus recibos", "Fuente: Importass: https://portal.seg-social.gob.es/x/historialRecibosRETA\n", "Código que lo lee: src/dominio/fallo.rs · leer_recibos", "Versión de gesshtor: 0.1.0", "Se esperaba: la lista", "    Historial de recibos de ‹nombre›\n    ‹NIF›", "Para reproducirlo: S en la app"] {
            assert!(t.contains(si), "falta «{si}» en:\n{t}");
        }
        assert!(!t.contains("ANA") && !t.contains("pagina=2"), "{t}");
        // Lo de antes (solo el mensaje) se sigue leyendo, y se guarda igual.
        let viejo: Error = serde_json::from_str(r#"{"mensaje":"No contesta."}"#).unwrap();
        assert_eq!(viejo.informe(&[]), "Código: (sin código)\nQué pasa: No contesta.");
        assert_eq!(serde_json::to_string(&Error { gesshtor: String::new(), ..Error::de("x") }).unwrap(), r#"{"mensaje":"x"}"#);
    }
}
