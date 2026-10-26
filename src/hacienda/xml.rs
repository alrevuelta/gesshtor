//! Un lector de XML mínimo y tolerante, lo justo para las respuestas de Hacienda (`<servicioConsultasDirectas>`):
//! elementos (sin el prefijo del espacio de nombres) y su texto, con las entidades resueltas. Se saltan la
//! declaración, los comentarios, el DOCTYPE y los atributos; lo mal cerrado se cierra como se pueda.

/// Un elemento: su nombre (sin prefijo), su texto (sin el de los hijos) y sus hijos.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Nodo {
    pub nombre: String,
    pub texto: String,
    pub hijos: Vec<Nodo>,
}

impl Nodo {
    /// Él y todos los que lleva dentro, en orden de documento (como `iter()` de ElementTree).
    pub fn todos(&self) -> Vec<&Nodo> {
        let mut v = vec![self];
        for h in &self.hijos {
            v.extend(h.todos());
        }
        v
    }

    /// El texto (sin espacios a los lados) del primer hijo con ese nombre, o «».
    pub fn hijo(&self, nombre: &str) -> &str {
        self.hijos.iter().find(|h| h.nombre == nombre).map_or("", |h| h.texto.trim())
    }
}

/// Lee un documento. Devuelve un nodo sin nombre con los elementos de primer nivel como hijos (vacío si no hay
/// nada que parezca XML).
pub fn leer(s: &str) -> Nodo {
    let mut pila = vec![Nodo::default()];
    let mut resto = s;
    while !resto.is_empty() {
        let Some(i) = resto.find('<') else {
            texto(&mut pila, resto);
            break;
        };
        texto(&mut pila, &resto[..i]);
        resto = &resto[i..];
        if let Some(r) = resto.strip_prefix("<![CDATA[") {
            let fin = r.find("]]>").unwrap_or(r.len());
            pila.last_mut().unwrap().texto.push_str(&r[..fin]);
            resto = r.get(fin + 3..).unwrap_or("");
        } else if let Some(r) = resto.strip_prefix("<!--") {
            resto = r.find("-->").map_or("", |f| &r[f + 3..]);
        } else if let Some(r) = resto.strip_prefix("<?") {
            resto = r.find("?>").map_or("", |f| &r[f + 2..]);
        } else if let Some(r) = resto.strip_prefix("<!") {
            // DOCTYPE (con o sin definiciones entre corchetes).
            let corchete = r.find('[').filter(|&c| r.find('>').is_none_or(|f| c < f));
            let fin = if corchete.is_some() { r.find("]>").map(|f| f + 2) } else { r.find('>').map(|f| f + 1) };
            resto = fin.map_or("", |f| &r[f..]);
        } else if let Some(r) = resto.strip_prefix("</") {
            let fin = r.find('>').unwrap_or(r.len());
            cerrar(&mut pila, local(r[..fin].trim()));
            resto = r.get(fin + 1..).unwrap_or("");
        } else {
            let r = &resto[1..];
            let fin = fin_de_etiqueta(r);
            let etiqueta = &r[..fin];
            let nombre = local(etiqueta.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or(""));
            resto = r.get(fin + 1..).unwrap_or("");
            if nombre.is_empty() || !nombre.starts_with(|c: char| c.is_alphabetic() || c == '_') {
                // Un «<» suelto: es texto.
                texto(&mut pila, "<");
                resto = r;
                continue;
            }
            pila.push(Nodo { nombre: nombre.to_string(), ..Nodo::default() });
            if etiqueta.ends_with('/') {
                cerrar(&mut pila, nombre);
            }
        }
    }
    while pila.len() > 1 {
        let n = pila.pop().unwrap();
        pila.last_mut().unwrap().hijos.push(n);
    }
    pila.pop().unwrap()
}

/// Dónde acaba una etiqueta de apertura (el «>», sin contar los que haya entre comillas en los atributos).
fn fin_de_etiqueta(r: &str) -> usize {
    let mut comillas = None;
    for (i, c) in r.char_indices() {
        match (comillas, c) {
            (None, '"' | '\'') => comillas = Some(c),
            (Some(q), _) if q == c => comillas = None,
            (None, '>') => return i,
            _ => {}
        }
    }
    r.len()
}

/// «ns2:modelo» → «modelo»
fn local(nombre: &str) -> &str {
    nombre.rsplit(':').next().unwrap_or(nombre)
}

fn texto(pila: &mut [Nodo], s: &str) {
    if !s.is_empty() {
        pila.last_mut().unwrap().texto.push_str(&entidades(s));
    }
}

/// Cierra el elemento abierto con ese nombre (y los que quedaran abiertos dentro). Si no hay ninguno, nada.
fn cerrar(pila: &mut Vec<Nodo>, nombre: &str) {
    let Some(i) = pila.iter().rposition(|n| n.nombre == nombre).filter(|&i| i > 0) else { return };
    while pila.len() > i {
        let n = pila.pop().unwrap();
        pila.last_mut().unwrap().hijos.push(n);
    }
}

/// Resuelve `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;` y `&#…;`; las desconocidas se quedan tal cual.
fn entidades(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(i) = resto.find('&') {
        out.push_str(&resto[..i]);
        resto = &resto[i..];
        let fin = resto.find(';').filter(|&f| f <= 10);
        let c = fin.and_then(|f| match &resto[1..f] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e => {
                let n = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")).map(|h| u32::from_str_radix(h, 16)).or_else(|| e.strip_prefix('#').map(str::parse));
                n.and_then(Result::ok).and_then(char::from_u32)
            }
        });
        match (c, fin) {
            (Some(c), Some(f)) => {
                out.push(c);
                resto = &resto[f + 1..];
            }
            _ => {
                out.push('&');
                resto = &resto[1..];
            }
        }
    }
    out.push_str(resto);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_lo_justo() {
        let d = leer(r#"<?xml version="1.0" encoding="UTF-8"?><!-- hola --><a><b x="1>2">uno &amp; dos &#225;&#xE9;</b><ns2:c/><d>  tres </d></a>"#);
        assert_eq!(d.hijos.len(), 1);
        let a = &d.hijos[0];
        assert_eq!(a.nombre, "a");
        assert_eq!(a.hijos.iter().map(|h| h.nombre.as_str()).collect::<Vec<_>>(), ["b", "c", "d"]);
        assert_eq!(a.hijo("b"), "uno & dos áé");
        assert_eq!(a.hijo("d"), "tres");
        assert_eq!(a.hijo("c"), "");
        assert_eq!(a.hijo("no"), "");
        assert_eq!(d.todos().len(), 5);
    }

    #[test]
    fn tolera_lo_mal_hecho() {
        // Sin cerrar, cierres de más, «<» y «&» sueltos, CDATA y DOCTYPE.
        let d = leer("<!DOCTYPE a [<!ENTITY x \"y\">]><a><b>1 < 2 & 3 &raro;</c></b><e><![CDATA[<f>]]>");
        let a = &d.hijos[0];
        assert_eq!(a.hijo("b"), "1 < 2 & 3 &raro;");
        assert_eq!(a.hijo("e"), "<f>");
        assert!(leer("").hijos.is_empty());
        assert!(leer("no es xml").hijos.is_empty());
        // Un HTML también se lee, pero no trae lo que se busca.
        assert!(!leer("<html><body><p>Error</p></body></html>").todos().iter().any(|n| n.nombre == "servicioConsultasDirectas"));
    }
}
