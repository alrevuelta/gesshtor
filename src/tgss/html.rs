//! Leer HTML sin más herramientas: entidades, etiquetas, formularios y URL. Cada función hace lo mismo que la
//! expresión regular que se indica (con sus mismas rarezas). Para no
//! distinguir mayúsculas se busca en una copia en minúsculas ASCII (con las mismas posiciones).

/// `str.find` desde una posición.
pub(crate) fn buscar(t: &str, aguja: &str, desde: usize) -> Option<usize> {
    t.get(desde..)?.find(aguja).map(|i| i + desde)
}

/// Bytes de espacio en blanco desde `i` (`\s*`).
pub(crate) fn espacios(t: &str, i: usize) -> usize {
    t.get(i..).map_or(0, |r| r.len() - r.trim_start().len())
}

/// Cifras ASCII seguidas desde `i`.
pub(crate) fn cifras(t: &str, i: usize) -> usize {
    t.as_bytes().get(i..).map_or(0, |b| b.iter().take_while(|c| c.is_ascii_digit()).count())
}

/// `\w` de Python: letras, cifras y «_».
pub(crate) fn es_palabra(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// ¿Hay frontera de palabra (`\b`) en `i`, viniendo de una letra?
fn frontera(t: &str, i: usize) -> bool {
    t.get(i..).and_then(|r| r.chars().next()).is_none_or(|c| !es_palabra(c))
}

// ---------------------------------------------------------------- entidades (`html.unescape`)

/// Las entidades de Latin-1 (`&nbsp;` … `&yuml;`), en orden desde U+00A0. Valen también sin «;».
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf", "laquo", "not", "shy", "reg", "macr",
    "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34",
    "iquest", "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml", "Igrave",
    "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute",
    "Ucirc", "Uuml", "Yacute", "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil", "egrave",
    "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml",
    "divide", "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
];

/// Las que valen sin «;» además de las de Latin-1.
const SUELTAS: [(&str, char); 10] =
    [("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("AMP", '&'), ("LT", '<'), ("GT", '>'), ("QUOT", '"'), ("COPY", '©'), ("REG", '®')];

/// Otras frecuentes (solo con «;»).
const OTRAS: [(&str, char); 11] = [
    ("apos", '\''),
    ("euro", '€'),
    ("hellip", '…'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bull", '•'),
    ("trade", '™'),
];

/// Los bytes 0x80–0x9F de Windows-1252 (lo que hace `html.unescape` con `&#128;` y similares).
const CP1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—',
    '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Un byte de Windows-1252 como carácter.
pub(crate) fn cp1252(b: u8) -> char {
    if (0x80..0xA0).contains(&b) { CP1252[(b - 0x80) as usize] } else { b as char }
}

fn sin_punto_y_coma(nombre: &str) -> Option<char> {
    if let Some(i) = LATIN1.iter().position(|n| *n == nombre) {
        return char::from_u32(0xA0 + i as u32);
    }
    SUELTAS.iter().find(|(n, _)| *n == nombre).map(|(_, c)| *c)
}

fn con_punto_y_coma(nombre: &str) -> Option<char> {
    sin_punto_y_coma(nombre).or_else(|| OTRAS.iter().find(|(n, _)| *n == nombre).map(|(_, c)| *c))
}

fn numerica(n: u32) -> char {
    match n {
        0x80..=0x9F => cp1252(n as u8),
        0 | 0xD800..=0xDFFF => '\u{FFFD}',
        _ => char::from_u32(n).unwrap_or('\u{FFFD}'),
    }
}

/// `html.unescape`: `&aacute;`, `&#243;`, `&#xF3;`, `&amp` (las antiguas, también sin «;»).
pub fn desescapar(t: &str) -> String {
    if !t.contains('&') {
        return t.to_string();
    }
    let mut out = String::with_capacity(t.len());
    let mut resto = t;
    while let Some(i) = resto.find('&') {
        out.push_str(&resto[..i]);
        let tras = &resto[i + 1..];
        let (texto, usado) = entidad(tras);
        out.push_str(&texto);
        resto = &tras[usado..];
    }
    out.push_str(resto);
    out
}

/// Lo que sigue a un «&»: el texto que sale y los bytes que se consumen.
fn entidad(t: &str) -> (String, usize) {
    let b = t.as_bytes();
    if b.first() == Some(&b'#') {
        let hex = matches!(b.get(1), Some(b'x' | b'X'));
        let ini = if hex { 2 } else { 1 };
        let n = b[ini..].iter().take_while(|c| if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }).count();
        if n == 0 {
            return ("&".into(), 0);
        }
        let digitos = &t[ini..ini + n];
        let valor = u32::from_str_radix(digitos, if hex { 16 } else { 10 }).unwrap_or(u32::MAX);
        let fin = ini + n + usize::from(b.get(ini + n) == Some(&b';'));
        return (numerica(valor).to_string(), fin);
    }
    // `[^\t\n\f <&#;]{1,32};?`
    let nombre: String = t.chars().take_while(|c| !matches!(c, '\t' | '\n' | '\x0c' | ' ' | '<' | '&' | '#' | ';')).take(32).collect();
    if nombre.is_empty() {
        return ("&".into(), 0);
    }
    if t[nombre.len()..].starts_with(';')
        && let Some(c) = con_punto_y_coma(&nombre)
    {
        return (c.to_string(), nombre.len() + 1);
    }
    // Sin «;»: el prefijo más largo que sea una de las antiguas.
    let limites: Vec<usize> = nombre.char_indices().map(|(i, _)| i).skip(2).chain([nombre.len()]).collect();
    for &fin in limites.iter().rev() {
        if let Some(c) = sin_punto_y_coma(&nombre[..fin]) {
            return (c.to_string(), fin);
        }
    }
    ("&".into(), 0)
}

// ---------------------------------------------------------------- texto de una página

/// Quita `<(script|style)[\s\S]*?</\1>` (sin distinguir mayúsculas), cambiándolo por un espacio.
fn sin_scripts(t: &str) -> String {
    let bajo = t.to_ascii_lowercase();
    let mut out = String::with_capacity(t.len());
    let (mut copiado, mut desde) = (0, 0);
    while let Some(i) = buscar(&bajo, "<", desde) {
        let cierre = if bajo[i + 1..].starts_with("script") {
            "</script>"
        } else if bajo[i + 1..].starts_with("style") {
            "</style>"
        } else {
            desde = i + 1;
            continue;
        };
        match buscar(&bajo, cierre, i + 1) {
            Some(f) => {
                out.push_str(&t[copiado..i]);
                out.push(' ');
                copiado = f + cierre.len();
                desde = copiado;
            }
            None => desde = i + 1,
        }
    }
    out.push_str(&t[copiado..]);
    out
}

/// Cambia cada etiqueta (`<[^>]+>`) por un espacio.
fn sin_etiquetas(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut resto = t;
    while let Some(i) = resto.find('<') {
        match resto[i + 1..].find('>') {
            Some(f) if f > 0 => {
                out.push_str(&resto[..i]);
                out.push(' ');
                resto = &resto[i + 1 + f + 1..];
            }
            Some(_) => {
                out.push_str(&resto[..=i]);
                resto = &resto[i + 1..];
            }
            None => break,
        }
    }
    out.push_str(resto);
    out
}

/// El texto que se ve de un trozo de HTML, con los espacios juntos (`_limpio`).
pub fn limpio(t: &str) -> String {
    desescapar(&sin_etiquetas(&sin_scripts(t))).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Los primeros `n` caracteres del texto de una página (`_resumen`), para los mensajes de error.
pub fn resumen(t: &str, n: usize) -> String {
    limpio(t).chars().take(n).collect()
}

/// El `<title>` de una página (`_titulo`).
pub fn titulo(t: &str) -> String {
    let bajo = t.to_ascii_lowercase();
    let Some(i) = buscar(&bajo, "<title>", 0) else { return String::new() };
    let Some(f) = buscar(&bajo, "</title>", i + 7) else { return String::new() };
    desescapar(&t[i + 7..f]).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// El título o, si no tiene, el principio del texto: para decir dónde se ha quedado algo.
pub fn donde(t: &str) -> String {
    let t0 = titulo(t);
    if t0.is_empty() { resumen(t, 160) } else { t0 }
}

// ---------------------------------------------------------------- atributos y formularios

/// `nombre=["']([^"']*)` sin distinguir mayúsculas: el primer atributo así.
pub(crate) fn atributo<'a>(attrs: &'a str, nombre: &str) -> Option<&'a str> {
    let bajo = attrs.to_ascii_lowercase();
    let patron = format!("{nombre}=");
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, &patron, desde) {
        let j = i + patron.len();
        if matches!(attrs.as_bytes().get(j), Some(b'"' | b'\'')) {
            let resto = &attrs[j + 1..];
            return Some(&resto[..resto.find(['"', '\'']).unwrap_or(resto.len())]);
        }
        desde = i + 1;
    }
    None
}

/// `nombre=["']?(\w+)` sin distinguir mayúsculas.
fn atributo_palabra<'a>(attrs: &'a str, nombre: &str) -> Option<&'a str> {
    let bajo = attrs.to_ascii_lowercase();
    let patron = format!("{nombre}=");
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, &patron, desde) {
        let mut j = i + patron.len();
        if matches!(attrs.as_bytes().get(j), Some(b'"' | b'\'')) {
            j += 1;
        }
        let n: usize = attrs[j..].chars().take_while(|c| es_palabra(*c)).map(char::len_utf8).sum();
        if n > 0 {
            return Some(&attrs[j..j + n]);
        }
        desde = i + 1;
    }
    None
}

/// ¿Aparece la palabra suelta (`\bpalabra\b`, sin distinguir mayúsculas)?
fn palabra_suelta(attrs: &str, palabra: &str) -> bool {
    let bajo = attrs.to_ascii_lowercase();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, palabra, desde) {
        let antes = bajo[..i].chars().next_back().is_none_or(|c| !es_palabra(c));
        if antes && frontera(&bajo, i + palabra.len()) {
            return true;
        }
        desde = i + 1;
    }
    false
}

/// Un formulario de una página.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Formulario {
    /// A dónde se manda (absoluta).
    pub accion: String,
    /// «get» | «post».
    pub metodo: String,
    /// Los campos que se mandan (sin los botones; las casillas, solo si están marcadas). Un nombre repetido se
    /// queda en su sitio con el último valor.
    pub campos: Vec<(String, String)>,
    pub botones: Vec<(String, String)>,
}

impl Formulario {
    pub fn tiene(&self, nombre: &str) -> bool {
        self.campos.iter().any(|(k, _)| k == nombre)
    }
}

/// Pone un campo (si ya está, cambia su valor sin moverlo, como un `dict`).
fn poner(campos: &mut Vec<(String, String)>, k: String, v: String) {
    match campos.iter_mut().find(|(n, _)| *n == k) {
        Some(c) => c.1 = v,
        None => campos.push((k, v)),
    }
}

/// `<base href="…">` de la página, hecha absoluta (o la propia URL si no tiene).
pub fn base(url: &str, t: &str) -> String {
    let bajo = t.to_ascii_lowercase();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, "<base", desde) {
        desde = i + 1;
        // `<base[^>]+href=["']([^"']+)`: el último «href=» de la etiqueta que encaje (tras algo de `[^>]+`).
        let Some(primero) = bajo[i + 5..].chars().next().filter(|c| *c != '>') else { continue };
        let lo = i + 5 + primero.len_utf8();
        let mut hasta = buscar(&bajo, ">", i + 5).unwrap_or(bajo.len());
        while let Some(h) = bajo.get(lo..hasta).and_then(|r| r.rfind("href=")).map(|h| h + lo) {
            let q = h + 5;
            if matches!(t.as_bytes().get(q), Some(b'"' | b'\'')) {
                let resto = &t[q + 1..];
                let v = &resto[..resto.find(['"', '\'']).unwrap_or(resto.len())];
                if !v.is_empty() {
                    return unir(url, &desescapar(v));
                }
            }
            hasta = h + 4;
        }
    }
    url.to_string()
}

/// Los formularios de una página (`formularios`).
pub fn formularios(url: &str, t: &str) -> Vec<Formulario> {
    let base = base(url, t);
    let bajo = t.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, "<form", desde) {
        desde = i + 1;
        if !frontera(&bajo, i + 5) {
            continue;
        }
        let Some(gt) = buscar(&bajo, ">", i + 5) else { break };
        let Some(fin) = buscar(&bajo, "</form>", gt + 1) else { continue };
        let (attrs, dentro) = (&t[i + 5..gt], &t[gt + 1..fin]);
        desde = fin + 7;
        let (mut campos, mut botones) = (Vec::new(), Vec::new());
        for (etiqueta, a) in controles(dentro) {
            let Some(n) = atributo(a, "name") else { continue };
            let nombre = desescapar(n);
            let valor = atributo(a, "value").map(desescapar).unwrap_or_default();
            let tipo = atributo_palabra(a, "type").unwrap_or("text").to_lowercase();
            if etiqueta == "button" || tipo == "submit" || tipo == "image" {
                botones.push((nombre, valor));
            } else if (tipo == "checkbox" || tipo == "radio") && !palabra_suelta(a, "checked") {
                continue;
            } else {
                poner(&mut campos, nombre, valor);
            }
        }
        let accion = match atributo(attrs, "action") {
            Some(a) if !a.is_empty() => unir(&base, desescapar(a).split('#').next().unwrap_or("")),
            _ => url.to_string(),
        };
        let metodo = atributo_palabra(attrs, "method").unwrap_or("get").to_lowercase();
        out.push(Formulario { accion, metodo, campos, botones });
    }
    out
}

/// `<(input|button|select|textarea)\b([^>]*)>`: la etiqueta (en minúsculas) y sus atributos.
fn controles(t: &str) -> Vec<(&'static str, &str)> {
    let bajo = t.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, "<", desde) {
        desde = i + 1;
        let Some(e) = ["input", "button", "select", "textarea"].into_iter().find(|e| bajo[i + 1..].starts_with(e)) else { continue };
        let ini = i + 1 + e.len();
        if !frontera(&bajo, ini) {
            continue;
        }
        let Some(gt) = buscar(&bajo, ">", ini) else { break };
        out.push((e, &t[ini..gt]));
        desde = gt + 1;
    }
    out
}

// ---------------------------------------------------------------- URL

/// El servidor de una URL, en minúsculas.
pub fn servidor(url: &str) -> String {
    let resto = url.split_once("://").map_or(url, |(_, r)| r);
    let h = resto.split(['/', '?', '#']).next().unwrap_or("");
    let h = h.rsplit('@').next().unwrap_or(h);
    h.split(':').next().unwrap_or("").to_lowercase()
}

/// `urllib.parse.urljoin`: una dirección relativa a `base`, hecha absoluta.
pub fn unir(base: &str, rel: &str) -> String {
    let rel: String = rel.trim_matches(|c: char| c <= ' ').chars().filter(|c| !matches!(c, '\t' | '\r' | '\n')).collect();
    if rel.is_empty() {
        return base.to_string();
    }
    let con_esquema = rel.find(':').is_some_and(|i| {
        rel.as_bytes()[0].is_ascii_alphabetic() && rel[..i].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    if con_esquema {
        return rel;
    }
    let (sin_frag, _) = base.split_once('#').unwrap_or((base, ""));
    let (sin_consulta, _) = sin_frag.split_once('?').unwrap_or((sin_frag, ""));
    let (esquema, resto) = sin_consulta.split_once("://").unwrap_or(("https", sin_consulta));
    let (host, ruta) = resto.find('/').map_or((resto, ""), |i| resto.split_at(i));
    if let Some(r) = rel.strip_prefix("//") {
        return format!("{esquema}://{r}");
    }
    if rel.starts_with('#') {
        return format!("{sin_frag}{rel}");
    }
    if rel.starts_with('?') {
        return format!("{esquema}://{host}{ruta}{rel}");
    }
    let (ruta_rel, cola) = rel.split_at(rel.find(['?', '#']).unwrap_or(rel.len()));
    format!("{esquema}://{host}{}{cola}", resolver(ruta, ruta_rel))
}

/// La ruta de `ruta_rel` relativa a `ruta_base`, sin «.» ni «..» (como `urljoin`).
fn resolver(ruta_base: &str, ruta_rel: &str) -> String {
    let segmentos: Vec<&str> = if ruta_rel.starts_with('/') {
        ruta_rel.split('/').collect()
    } else {
        let mut s: Vec<&str> = ruta_base.split('/').collect();
        if s.last() != Some(&"") {
            s.pop();
        }
        s.extend(ruta_rel.split('/'));
        // Sin segmentos vacíos por en medio (no dejan «//»).
        let n = s.len();
        if n > 2 {
            let ultimo = s[n - 1];
            s = std::iter::once(s[0]).chain(s[1..n - 1].iter().copied().filter(|x| !x.is_empty())).chain([ultimo]).collect();
        }
        s
    };
    let mut r: Vec<&str> = Vec::new();
    for seg in &segmentos {
        match *seg {
            ".." => {
                r.pop();
            }
            "." => {}
            s => r.push(s),
        }
    }
    if matches!(segmentos.last(), Some(&".") | Some(&"..")) {
        r.push("");
    }
    let j = r.join("/");
    if j.starts_with('/') { j } else { format!("/{j}") }
}

/// Codifica un valor para una URL como `urllib.parse.urlencode` (espacio → «+»).
pub fn codificar(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `urlencode` de varios campos.
pub fn consulta(campos: &[(&str, &str)]) -> String {
    campos.iter().map(|(k, v)| format!("{}={}", codificar(k), codificar(v))).collect::<Vec<_>>().join("&")
}
