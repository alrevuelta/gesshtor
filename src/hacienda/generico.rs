//! Lectura genérica de cualquier declaración de Hacienda (nivel 1): las casillas tal como se ven impresas, con
//! su número, el texto que llevan al lado y su importe. Sirve para cualquier modelo y año, aunque la app no lo
//! conozca: no da significado a las casillas, solo las enseña.
//!
//! Hay dos formas de imprimirlas:
//! - **Formulario** (303, 130, 390…): el número de la casilla en su recuadro y, a su derecha y en la misma línea,
//!   el importe. Los importes que rellena Hacienda van en otra letra que el impreso: así no se confunden con los
//!   que forman parte del impreso («máximo: 660,14 euros», los tipos de IVA del 390…).
//! - **Lista** (Renta): «Ingresos de explotación 23.500,00 0171», el número a la derecha del importe.
//!
//! En cada página vale la forma con la que salen más casillas.
//!
//! Aquí también está lo que dice la portada («Información de la presentación»): modelo, justificante y fecha.
use super::pdf::{Documento, Pagina, Palabra};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CasillaImpresa {
    pub pagina: usize,
    /// Tal como sale («0171», «27»).
    pub numero: String,
    /// El texto que la acompaña en el impreso.
    pub etiqueta: String,
    pub importe: f64,
}

pub fn casillas(doc: &Documento) -> Vec<CasillaImpresa> {
    let mut v = Vec::new();
    let letra = letra_rellenada(doc);
    for p in &doc.paginas {
        let h = Hoja::de(p, &letra);
        let form = h.pares_formulario(&|_| true);
        let lista = h.pares_lista();
        let (pares, ancla_importe) = if lista.len() > form.len() { (lista, true) } else { (form, false) };
        let paradas = h.paradas(&pares);
        for par in &pares {
            // La etiqueta va a la izquierda del número (formulario) o del importe (lista).
            let ancla = if ancla_importe { par.imp } else { par.num };
            v.push(CasillaImpresa { pagina: p.numero, numero: par.numero.clone(), etiqueta: h.etiqueta(ancla, &paradas), importe: par.importe });
        }
    }
    v
}

/* ================= piezas comunes (también para `formulario`) ================= */

/// Un importe tal como se imprime: «1.234,56», «-56,70», «0,00». None si no lo es.
pub fn importe(s: &str) -> Option<f64> {
    let (neg, d) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (ent, dec) = d.split_once(',')?;
    if dec.len() != 2 || !dec.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !ent.ends_with(|c: char| c.is_ascii_digit()) || !ent.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let limpio: String = ent.chars().filter(|&c| c != '.').collect();
    let v: f64 = format!("{limpio}.{dec}").parse().ok()?;
    Some(if neg { -v } else { v })
}

/// ¿Es un número de casilla («07», «110», «0171»)? Entre `min` y `max` cifras.
fn es_numero(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

fn centro(w: &Palabra) -> f64 {
    (w.top + w.bottom) / 2.0
}

/// La familia de una letra, sin el prefijo del subconjunto ni el estilo: «ABCDEF+NewsGothicStd-Bold» → «NewsGothicStd».
fn familia(f: &str) -> &str {
    let f = match f.split_once('+') {
        Some((pre, resto)) if pre.len() == 6 && pre.bytes().all(|b| b.is_ascii_uppercase()) => resto,
        _ => f,
    };
    let f = f.split(['-', ',']).next().unwrap_or(f).trim_end_matches(|c: char| c.is_ascii_digit());
    f.strip_suffix("MT").unwrap_or(f)
}

/// Casilla e importe pegados a los puntos de una línea de puntos, como el «% atribuible» del 303:
/// «......6.5......1..0..0...,.0..0.....%» → (65, 100,00). Puede haber más de una forma de partirlo: van todas,
/// la de número más corto primero.
fn punteada(s: &str) -> Vec<(String, f64)> {
    if !s.contains("...") || !s.bytes().any(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let t: String = s.chars().filter(|&c| c != '.').collect();
    let t = t.strip_suffix('%').unwrap_or(&t);
    if !t.bytes().all(|b| b.is_ascii_digit() || b == b',') {
        return Vec::new();
    }
    (2..=3)
        .filter_map(|n| {
            let (num, resto) = (t.get(..n)?, t.get(n..)?);
            // Sin ceros de más delante: «00,00» no es un importe impreso.
            let ent = resto.split(',').next()?;
            if ent.len() > 1 && ent.starts_with('0') {
                return None;
            }
            Some((num.to_string(), importe(resto)?))
        })
        .collect()
}

/// La letra con la que Hacienda rellena los importes (la familia que más importes tiene): los del impreso
/// («máximo: 660,14 euros», los tipos del 390…) van en la del impreso y no cuentan.
pub(crate) fn letra_rellenada(doc: &Documento) -> String {
    let mut cuenta: Vec<(&str, usize)> = Vec::new();
    for w in doc.paginas.iter().flat_map(Pagina::palabras).filter(|w| importe(&w.texto).is_some()) {
        let f = familia(&w.fuente);
        match cuenta.iter_mut().find(|x| x.0 == f) {
            Some(x) => x.1 += 1,
            None => cuenta.push((f, 1)),
        }
    }
    cuenta.iter().rev().max_by_key(|x| x.1).map(|x| x.0.to_string()).unwrap_or_default()
}

/// Un número de casilla y su importe, emparejados por su posición en la página.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Par {
    pub pagina: usize,
    pub numero: String,
    pub importe: f64,
    /// Las palabras (índices en `Hoja::palabras`) del número y del importe; la misma si van pegados.
    num: usize,
    imp: usize,
}

/// Las palabras de una página, en orden de lectura, y la letra de los importes rellenados (`letra_rellenada`).
pub(crate) struct Hoja<'a> {
    numero: usize,
    palabras: Vec<&'a Palabra>,
    letra: &'a str,
}

impl<'a> Hoja<'a> {
    pub fn de(p: &'a Pagina, letra: &'a str) -> Self {
        Hoja { numero: p.numero, palabras: p.palabras().collect(), letra }
    }

    /// El importe de una palabra, si es un importe rellenado (no del impreso).
    fn importe(&self, w: &Palabra) -> Option<f64> {
        importe(&w.texto).filter(|_| familia(&w.fuente) == self.letra)
    }

    /// Formulario: cada importe con el número de casilla más cercano a su izquierda en la misma línea (si es de
    /// otra letra que el importe). `vale` elige entre las formas de partir una casilla pegada a los puntos.
    pub fn pares_formulario(&self, vale: &dyn Fn(&str) -> bool) -> Vec<Par> {
        let ps = &self.palabras;
        let mut v = Vec::new();
        for (i, a) in ps.iter().enumerate() {
            let Some(x) = self.importe(a) else {
                let opciones = punteada(&a.texto);
                if let Some((n, x)) = opciones.iter().find(|o| vale(&o.0)).or(opciones.first()) {
                    v.push(Par { pagina: self.numero, numero: n.clone(), importe: *x, num: i, imp: i });
                }
                continue;
            };
            let cy = centro(a);
            let izq = ps
                .iter()
                .enumerate()
                .filter(|(_, n)| es_numero(&n.texto, 2, 3) && (centro(n) - cy).abs() <= 4.0 && n.x1 <= a.x0 + 1.0 && a.x0 - n.x0 < 250.0)
                .max_by(|p, q| p.1.x0.total_cmp(&q.1.x0));
            if let Some((j, n)) = izq
                && familia(&n.fuente) != familia(&a.fuente)
            {
                v.push(Par { pagina: self.numero, numero: n.texto.clone(), importe: x, num: j, imp: i });
            }
        }
        v
    }

    /// Lista: cada importe con el número que tiene justo a su derecha en la misma línea.
    pub fn pares_lista(&self) -> Vec<Par> {
        let ps = &self.palabras;
        let mut v = Vec::new();
        for (i, a) in ps.iter().enumerate() {
            let Some(x) = self.importe(a) else { continue };
            let cy = centro(a);
            let der = ps
                .iter()
                .enumerate()
                .filter(|(_, n)| es_numero(&n.texto, 2, 4) && (centro(n) - cy).abs() <= 3.0 && n.x0 >= a.x1 - 1.0 && n.x0 - a.x1 < 60.0)
                .min_by(|p, q| p.1.x0.total_cmp(&q.1.x0));
            if let Some((j, n)) = der {
                v.push(Par { pagina: self.numero, numero: n.texto.clone(), importe: x, num: j, imp: i });
            }
        }
        v
    }

    /// Las palabras donde se corta una etiqueta: las de las casillas emparejadas y cualquier importe.
    fn paradas(&self, pares: &[Par]) -> Vec<bool> {
        let mut v: Vec<bool> = self.palabras.iter().map(|w| importe(&w.texto).is_some()).collect();
        for p in pares {
            v[p.num] = true;
            v[p.imp] = true;
        }
        v
    }

    /// El texto que acompaña a la palabra `k`: el que tiene a su izquierda en la misma línea; si no hay, el del
    /// principio de la línea; si tampoco, el de la línea de texto que tenga encima. Si empieza en minúscula, sigue
    /// de la línea de encima («Total bases imponibles… / interiores de bienes y servicios corrientes»).
    fn etiqueta(&self, k: usize, paradas: &[bool]) -> String {
        let ps = &self.palabras;
        let a = ps[k];
        let fila = self.fila(centro(a), |i| i != k && ps[i].x1 <= a.x0 + 0.5);
        // Lo que tiene justo a la izquierda, hasta la casilla o el importe anterior; si no, el principio de la línea.
        let n = fila.iter().rev().take_while(|&&i| !paradas[i]).count();
        let pegado = limpiar(&fila[fila.len() - n..].iter().map(|&i| ps[i]).collect::<Vec<_>>());
        let inicio = || limpiar(&fila.iter().take_while(|&&i| !paradas[i]).map(|&i| ps[i]).collect::<Vec<_>>());
        let (t, y) = match (pegado, centro(a)) {
            (t, y) if !t.is_empty() => (t, y),
            (_, y) => match inicio() {
                t if !t.is_empty() => (t, y),
                _ => match self.encima(y, a.x0, paradas, 60.0) {
                    Some(x) => x,
                    None => return String::new(),
                },
            },
        };
        if t.starts_with(|c: char| c.is_lowercase())
            && let Some((antes, _)) = self.encima(y, a.x0, paradas, 14.0)
        {
            return format!("{antes} {t}");
        }
        t
    }

    /// Las palabras de la línea con centro en `y` que cumplen `f`, de izquierda a derecha.
    fn fila(&self, y: f64, f: impl Fn(usize) -> bool) -> Vec<usize> {
        let ps = &self.palabras;
        let mut v: Vec<usize> = (0..ps.len()).filter(|&i| (centro(ps[i]) - y).abs() <= 3.0 && f(i)).collect();
        v.sort_by(|&i, &j| ps[i].x0.total_cmp(&ps[j].x0));
        v
    }

    /// La primera línea con texto por encima de `y` (hasta `hasta` puntos) que empiece a la izquierda de `x`:
    /// su texto y su centro.
    fn encima(&self, y: f64, x: f64, paradas: &[bool], hasta: f64) -> Option<(String, f64)> {
        let ps = &self.palabras;
        let mut ys: Vec<f64> = (0..ps.len()).map(|i| centro(ps[i])).filter(|&c| c < y - 3.0 && c >= y - hasta).collect();
        ys.sort_by(|a, b| b.total_cmp(a));
        ys.dedup_by(|a, b| (*a - *b).abs() <= 3.0);
        ys.into_iter().find_map(|cy| {
            let fila = self.fila(cy, |i| !paradas[i]);
            let primera = fila.first()?;
            if ps[*primera].x0 >= x {
                return None;
            }
            let t = limpiar(&fila.iter().map(|&i| ps[i]).collect::<Vec<_>>());
            (!t.is_empty()).then_some((t, cy))
        })
    }
}

/// Las palabras de una etiqueta, sin las líneas de puntos, las llaves ni los números de casilla sueltos de los
/// extremos; vacía si no tiene letras.
fn limpiar(ps: &[&Palabra]) -> String {
    let suelto = |s: &str| s.bytes().all(|b| b.is_ascii_digit()) || s == "{" || s == "}";
    let mut v: Vec<&str> = ps.iter().map(|p| p.texto.trim_end_matches('.')).filter(|s| !s.is_empty()).collect();
    while v.last().is_some_and(|s| suelto(s)) {
        v.pop();
    }
    let desde = v.iter().take_while(|s| suelto(s)).count();
    let t = v[desde..].iter().filter(|s| **s != "{" && **s != "}").copied().collect::<Vec<_>>().join(" ");
    let t = t.trim_matches(|c: char| c == ' ' || c == ':');
    if t.chars().filter(|c| c.is_alphabetic()).count() < 2 { String::new() } else { t.to_string() }
}

/* ================= la portada ================= */

/// La portada («Información de la presentación de la declaración»), si la tiene.
pub fn portada(doc: &Documento) -> Option<&Pagina> {
    doc.paginas.iter().take(2).find(|p| p.lineas.iter().take(4).any(|l| l.texto().to_uppercase().contains("INFORMACIÓN DE LA PRESENTACIÓN")))
}

/// El modelo que dice el PDF. En la cabecera del formulario va su número en letra grande: si la primera página lo
/// tiene (los PDF sin la página de la presentación), ese. Si no, el del primer «Modelo» con su número detrás
/// («Modelo 303», en la página de la presentación) o una palabra más allá.
pub fn modelo(doc: &Documento) -> Option<String> {
    let es_codigo = |w: &Palabra| w.texto.len() == 3 && w.texto.bytes().all(|b| b.is_ascii_alphanumeric());
    let numero = |w: &Palabra| es_codigo(w) && w.texto.bytes().all(|b| b.is_ascii_digit());
    let cabecera = doc.paginas.first().and_then(|p| p.palabras().filter(|w| w.tamano >= 16.0 && numero(w)).max_by(|a, b| a.tamano.total_cmp(&b.tamano)));
    if let Some(w) = cabecera {
        return Some(w.texto.clone());
    }
    let ps: Vec<&Palabra> = doc.paginas.iter().flat_map(Pagina::palabras).collect();
    let modelo = |w: &Palabra| w.texto.eq_ignore_ascii_case("modelo");
    (0..ps.len()).filter(|&i| modelo(ps[i])).find_map(|i| match (ps.get(i + 1), ps.get(i + 2)) {
        (Some(a), _) if es_codigo(a) => Some(a.texto.clone()),
        (_, Some(b)) if numero(b) => Some(b.texto.clone()),
        _ => None,
    })
}

/// El número de justificante (13 cifras): el que va tras «justificante:» o, si no, el primero que empieza por el
/// número del modelo (los justificantes empiezan así: 303…, 130…).
pub fn justificante(doc: &Documento, modelo: &str) -> String {
    let ps: Vec<&Palabra> = doc.paginas.iter().flat_map(Pagina::palabras).collect();
    let es = |w: &Palabra| es_numero(&w.texto, 13, 13);
    ps.windows(2)
        .find(|w| w[0].texto.to_lowercase().ends_with("justificante:") && es(w[1]))
        .map(|w| w[1].texto.clone())
        .or_else(|| ps.iter().find(|w| es(w) && !modelo.is_empty() && w.texto.starts_with(modelo)).map(|w| w.texto.clone()))
        .unwrap_or_default()
}

/// Cuándo se presentó, según la portada: «Presentación realizada el: 15-10-2026 a las 09:05:30» → «2026-10-15T09:05:30».
pub fn presentada(doc: &Documento) -> Option<String> {
    let p = portada(doc)?;
    let ps: Vec<&Palabra> = p.palabras().collect();
    let i = ps.iter().position(|w| w.texto.starts_with("realizada"))?;
    let fecha = ps[i + 1..].iter().take(3).find(|w| w.texto.len() == 10 && w.texto.as_bytes()[2] == b'-')?;
    let (d, m, a) = (fecha.texto.get(..2)?, fecha.texto.get(3..5)?, fecha.texto.get(6..)?);
    let hora = ps[i + 1..].iter().take(6).find(|w| w.texto.len() == 8 && w.texto.as_bytes()[2] == b':').map(|w| w.texto.as_str()).unwrap_or("00:00:00");
    Some(format!("{a}-{m}-{d}T{hora}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palabra(texto: &str, x0: f64, top: f64, fuente: &str, tamano: f64) -> Palabra {
        Palabra { texto: texto.into(), x0, x1: x0 + texto.len() as f64 * tamano * 0.5, top, bottom: top + tamano, fuente: fuente.into(), tamano }
    }

    fn doc(ps: Vec<Palabra>) -> Documento {
        let lineas = ps.into_iter().map(|p| super::super::pdf::Linea { top: p.top, bottom: p.bottom, x0: p.x0, x1: p.x1, palabras: vec![p] }).collect();
        Documento { paginas: vec![Pagina { numero: 1, ancho: 595.0, alto: 842.0, lineas }] }
    }

    #[test]
    fn importes_impresos() {
        assert_eq!(importe("1.234,56"), Some(1234.56));
        assert_eq!(importe("-56,70"), Some(-56.70));
        assert_eq!(importe("0,00"), Some(0.0));
        assert_eq!(importe("12345,67"), Some(12345.67));
        for no in ["", "12", "1,5", "1,555", ",50", "-", "1.234", "12,3a", "a1,00"] {
            assert_eq!(importe(no), None, "{no}");
        }
        assert_eq!(familia("RGCQZA+NewsGothicStd"), "NewsGothicStd");
        assert_eq!(familia("POJDGH+NewsGothicStd-Bold"), "NewsGothicStd");
        assert_eq!(familia("NimbusSanL-Regu"), "NimbusSanL");
        assert_eq!(familia("ArialMT2"), "Arial");
        assert_eq!(familia("Arial,Bold2"), "Arial");
    }

    #[test]
    fn casilla_pegada_a_los_puntos() {
        assert_eq!(punteada("......6.5......1..0..0...,.0..0.....%......"), vec![("65".to_string(), 100.0)]);
        assert!(punteada("..................").is_empty());
        assert!(punteada("1.234,56").is_empty());
    }

    #[test]
    fn formulario_y_lista() {
        // Formulario: «Base 28 270,00 29 56,70»; el importe del impreso («máximo 660,14») no cuenta.
        let f = "ABCDEF+NewsGothicStd";
        let d = doc(vec![
            palabra("Por", 40.0, 100.0, f, 8.0),
            palabra("cuotas", 60.0, 100.0, f, 8.0),
            palabra("28", 360.0, 101.0, f, 6.5),
            palabra("270,00", 428.0, 100.0, "NimbusSanL-Regu", 9.0),
            palabra("29", 460.0, 101.0, f, 6.5),
            palabra("56,70", 533.0, 100.0, "NimbusSanL-Regu", 9.0),
            palabra("de", 40.0, 140.0, f, 8.0),
            palabra("03", 60.0, 140.0, f, 8.0),
            palabra("(máximo:", 75.0, 140.0, f, 8.0),
            palabra("660,14", 115.0, 140.0, f, 8.0),
        ]);
        let c = casillas(&d);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!((c[0].numero.as_str(), c[0].importe, c[0].etiqueta.as_str()), ("28", 270.0, "Por cuotas"));
        assert_eq!((c[1].numero.as_str(), c[1].importe, c[1].etiqueta.as_str()), ("29", 56.7, "Por cuotas"));
        // Lista: «Ingresos de explotación 23.500,00 0171».
        let r = "NimbusSanL-Regu";
        let d = doc(vec![
            palabra("Ingresos", 40.0, 100.0, r, 9.0),
            palabra("de", 80.0, 100.0, r, 9.0),
            palabra("explotación.", 95.0, 100.0, r, 9.0),
            palabra("23.500,00", 460.0, 100.0, r, 9.0),
            palabra("0171", 510.0, 100.0, r, 8.0),
            palabra("Gastos", 40.0, 112.0, r, 9.0),
            palabra("1.000,00", 460.0, 112.0, r, 9.0),
            palabra("0186", 510.0, 112.0, r, 8.0),
        ]);
        let c = casillas(&d);
        assert_eq!(c.iter().map(|x| (x.numero.as_str(), x.importe, x.etiqueta.as_str())).collect::<Vec<_>>(), [("0171", 23500.0, "Ingresos de explotación"), ("0186", 1000.0, "Gastos")]);
    }

    #[test]
    fn el_303_de_prueba() {
        use crate::hacienda::modelo_303::pruebas::{con_portada, pdf_de_prueba};
        let doc = pdf_de_prueba();
        let c = casillas(&doc);
        let de = |n: &str| c.iter().find(|x| x.numero == n).unwrap_or_else(|| panic!("falta la {n}: {c:?}"));
        assert_eq!(de("28").importe, 5000.0);
        assert!(de("28").etiqueta.starts_with("Por cuotas soportadas en operaciones interiores corrientes"), "{:?}", de("28"));
        assert!(de("46").etiqueta.starts_with("Resultado régimen general"), "{:?}", de("46"));
        assert_eq!(de("65").importe, 100.0);
        // El modelo, del número grande de la cabecera; sin la página de la presentación no hay más.
        assert_eq!(modelo(&doc).as_deref(), Some("303"));
        assert!(portada(&doc).is_none() && presentada(&doc).is_none() && justificante(&doc, "303").is_empty());
        // Con ella, de ahí salen el modelo, el justificante y cuándo se presentó, pero ninguna casilla.
        let doc = con_portada("A COMPENSAR");
        assert_eq!(portada(&doc).map(|p| p.numero), Some(1));
        assert_eq!(casillas(&doc), c.iter().map(|x| CasillaImpresa { pagina: x.pagina + 1, ..x.clone() }).collect::<Vec<_>>());
        assert_eq!(modelo(&doc).as_deref(), Some("303"));
        assert_eq!(justificante(&doc, "303"), "3039999999999");
        assert_eq!(presentada(&doc).as_deref(), Some("2025-04-15T09:05:30"));
    }
}
