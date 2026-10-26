//! Textos para las pantallas: partirlos en líneas y escribir como frase lo que viene en mayúsculas.
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Parte un texto en líneas de `ancho` caracteres como mucho (al menos 10), por palabras; un espacio duro
/// (`\u{a0}`) no parte. Sin texto, ninguna línea.
pub fn partir(s: &str, ancho: usize) -> Vec<String> {
    let ancho = ancho.max(10);
    let mut v = Vec::new();
    let mut l = String::new();
    for p in s.split(' ').filter(|p| !p.is_empty()) {
        if !l.is_empty() && l.chars().count() + 1 + p.chars().count() > ancho {
            v.push(std::mem::take(&mut l));
        }
        if !l.is_empty() {
            l.push(' ');
        }
        l.push_str(p);
    }
    if !l.is_empty() {
        v.push(l);
    }
    v
}

/// Como `partir`, pero siempre al menos una línea (vacía si no hay texto).
pub fn lineas(s: &str, ancho: usize) -> Vec<String> {
    let v = partir(s, ancho);
    if v.is_empty() { vec![String::new()] } else { v }
}

/// Parte en líneas de `w` columnas como mucho, por palabras y con sangría; cada trozo con su estilo.
pub(super) fn envolver(trozos: &[(String, Style)], w: usize, sangria: usize) -> Vec<Line<'static>> {
    let w = w.max(sangria + 8);
    let mut lineas = Vec::new();
    let nueva = || vec![Span::raw(" ".repeat(sangria))];
    let mut actual = nueva();
    let mut largo = sangria;
    for (t, st) in trozos {
        // El «€» y el «%» van con su número: «1.234,56 €» no se parte.
        let mut palabras: Vec<String> = Vec::new();
        for p in t.split_whitespace() {
            match palabras.last_mut() {
                Some(u) if p.starts_with('€') || p.starts_with('%') => {
                    u.push(' ');
                    u.push_str(p);
                }
                _ => palabras.push(p.to_string()),
            }
        }
        for mut p in palabras {
            let n = p.chars().count();
            if largo > sangria && largo + 1 + n > w {
                lineas.push(Line::from(std::mem::replace(&mut actual, nueva())));
                largo = sangria;
            }
            if n > w - sangria {
                p = p.chars().take(w - sangria - 1).collect::<String>() + "…";
            }
            if largo > sangria {
                actual.push(Span::styled(" ", *st));
                largo += 1;
            }
            largo += p.chars().count();
            actual.push(Span::styled(p, *st));
        }
    }
    if largo > sangria {
        lineas.push(Line::from(actual));
    }
    lineas
}

/// «Nombre             1.234,56 €» en `w` columnas: el nombre se corta con «…» si no cabe; el importe, nunca.
pub(super) fn fila(nombre: &str, valor: &str, w: usize, st_nombre: Style, st_valor: Style) -> Line<'static> {
    let nv = valor.chars().count();
    let libre = w.saturating_sub(nv + 1);
    let mut et: String = nombre.to_string();
    if et.chars().count() > libre {
        et = et.chars().take(libre.saturating_sub(1)).collect::<String>() + "…";
    }
    let hueco = w.saturating_sub(et.chars().count() + nv).max(1);
    Line::from(vec![Span::styled(et, st_nombre), Span::raw(" ".repeat(hueco)), Span::styled(valor.to_string(), st_valor)])
}

/// Dos columnas de `w` de ancho, una al lado de la otra y separadas por 3 espacios, en las mismas líneas.
pub fn dos_columnas(izq: Vec<Line<'static>>, der: Vec<Line<'static>>, w: usize) -> Vec<Line<'static>> {
    (0..izq.len().max(der.len()))
        .map(|i| {
            let mut v: Vec<Span> = izq.get(i).map(|l| l.spans.clone()).unwrap_or_default();
            let ancho: usize = v.iter().map(|s| s.width()).sum();
            v.push(Span::raw(" ".repeat(w + 3 - ancho.min(w))));
            v.extend(der.get(i).map(|l| l.spans.clone()).unwrap_or_default());
            Line::from(v)
        })
        .collect()
}

/// Un texto en mayúsculas de la Seguridad Social escrito como frase: «IT DE ACCIDENTES DE TRABAJO» → «IT de
/// accidentes de trabajo», «PROGRAMACIÓN INFORMÁTICA» → «Programación informática» (la primera en mayúscula y el
/// resto en minúscula, salvo las siglas IT e IMS).
pub fn frase(t: &str) -> String {
    let mut s = String::new();
    let mut cs = t.trim().chars();
    if let Some(c) = cs.next() {
        s.extend(c.to_uppercase());
    }
    s.extend(cs.flat_map(char::to_lowercase));
    // Las siglas, como palabras sueltas.
    let es_letra = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(s.len());
    let mut palabra = String::new();
    let soltar = |palabra: &mut String, out: &mut String| {
        if palabra.eq_ignore_ascii_case("it") || palabra.eq_ignore_ascii_case("ims") {
            out.push_str(&palabra.to_uppercase());
        } else {
            out.push_str(palabra);
        }
        palabra.clear();
    };
    for c in s.chars() {
        if es_letra(c) {
            palabra.push(c);
        } else {
            soltar(&mut palabra, &mut out);
            out.push(c);
        }
    }
    soltar(&mut palabra, &mut out);
    out
}

/// La hora de una fecha con hora: «2026-10-15T09:05:30» → «09:05».
pub fn hora(s: &str) -> Option<&str> {
    s.get(11..16).filter(|h| h.as_bytes().get(2) == Some(&b':'))
}

/// «2026-10-15T09:05:30» → «hoy a las 09:05», «ayer a las…», «el 15 oct 2026 a las 09:05».
pub fn cuando(s: &str) -> String {
    let dia = s.get(..10).unwrap_or(s);
    if crate::dominio::fmt::fecha(dia).is_none() {
        return s.to_string();
    }
    let d = match crate::dominio::fmt::dias_entre(dia, &crate::dominio::fmt::hoy()) {
        0 => "hoy".to_string(),
        1 => "ayer".to_string(),
        _ => format!("el {}", crate::dominio::fmt::fecha_corta(dia)),
    };
    match hora(s) {
        Some(h) => format!("{d} a las {h}"),
        None => d,
    }
}

/// Una cifra en corto, para barras, ejes y columnas estrechas: 1234,5 → «1,2k»; 850 → «850»; 24.190 → «24k»;
/// 1.500.000 → «1,5M».
pub fn compacto(x: f64) -> String {
    fn decimal(x: f64) -> String {
        format!("{x:.1}").replace('.', ",").trim_end_matches(",0").to_string()
    }
    let a = x.abs();
    let s = if a < 999.5 {
        format!("{}", a.round() as i64)
    } else if a < 9_950.0 {
        format!("{}k", decimal(a / 1e3))
    } else if a < 999_500.0 {
        format!("{}k", (a / 1e3).round() as i64)
    } else if a < 9_950_000.0 {
        format!("{}M", decimal(a / 1e6))
    } else {
        format!("{}M", (a / 1e6).round() as i64)
    };
    if x < 0.0 && s != "0" { format!("-{s}") } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeros_compactos() {
        assert_eq!(compacto(0.0), "0");
        assert_eq!(compacto(0.4), "0");
        assert_eq!(compacto(-0.4), "0");
        assert_eq!(compacto(850.0), "850");
        assert_eq!(compacto(999.4), "999");
        assert_eq!(compacto(999.6), "1k");
        assert_eq!(compacto(1000.0), "1k");
        assert_eq!(compacto(1234.5), "1,2k");
        assert_eq!(compacto(9_949.0), "9,9k");
        assert_eq!(compacto(9_960.0), "10k");
        assert_eq!(compacto(24_190.48), "24k");
        assert_eq!(compacto(-1500.0), "-1,5k");
        assert_eq!(compacto(1_500_000.0), "1,5M");
        assert_eq!(compacto(25_000_000.0), "25M");
    }

    #[test]
    fn frases() {
        assert_eq!(frase("CONTINGENCIAS COMUNES"), "Contingencias comunes");
        assert_eq!(frase("IT DE ACCIDENTES DE TRABAJO"), "IT de accidentes de trabajo");
        assert_eq!(frase("IMS DE ACCIDENTES DE TRABAJO"), "IMS de accidentes de trabajo");
        assert_eq!(frase("1º TRAMO COTIZACION ADIC. SOLIDARIDAD"), "1º tramo cotizacion adic. solidaridad");
        assert_eq!(frase("CREDITO (LIMITE)"), "Credito (limite)");
        assert_eq!(frase("  programación INFORMÁTICA "), "Programación informática");
        assert_eq!(frase(""), "");
    }

    #[test]
    fn parte_por_palabras() {
        assert_eq!(partir("uno dos tres cuatro", 10), ["uno dos", "tres", "cuatro"]);
        assert_eq!(partir("uno  dos", 3), ["uno dos"], "al menos 10 de ancho; los espacios de más no cuentan");
        assert_eq!(partir("pagos anteriores 0,00\u{a0}€ (05)", 22), ["pagos anteriores", "0,00\u{a0}€ (05)"]);
        assert!(partir("", 10).is_empty());
        assert_eq!(lineas("", 10), [""]);
    }

    #[test]
    fn envuelve_y_corta() {
        let l = envolver(&[("uno dos tres cuatro cinco".into(), Style::new())], 12, 2);
        let t: Vec<String> = l.iter().map(|l| l.to_string()).collect();
        assert_eq!(t, ["  uno dos", "  tres", "  cuatro", "  cinco"]);
        let t: Vec<String> = envolver(&[("cuentan 1.234,56 € y 21 % más".into(), Style::new())], 16, 0).iter().map(|l| l.to_string()).collect();
        assert_eq!(t, ["cuentan", "1.234,56 € y", "21 % más"]);
        assert!(l.iter().all(|l| l.width() <= 12));
        let f = fila("Un nombre muy largo que no cabe", "1.234,56 €", 20, Style::new(), Style::new());
        assert_eq!(f.width(), 20);
        assert!(f.to_string().ends_with("1.234,56 €") && f.to_string().contains('…'));
    }
}
