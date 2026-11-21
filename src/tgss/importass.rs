//! Leer las páginas de Importass (tus datos de autónomo): el perfil, los recibos y las regularizaciones. Funciones
//! puras: texto → los tipos de `dominio::tgss`, o el fallo (`Error`, con su informe) si la página no es como se
//! espera. La regla (ver `dominio::fallo`): lo que no está solo vale vacío si la página tiene la estructura de
//! siempre (sus anclas: la lista de recibos, la fecha de alta…); y lo que está (una etiqueta, un importe, una fecha)
//! tiene que entenderse, o es un fallo, nunca un 0 ni un vacío.
use super::html::{self, buscar, cifras, desescapar, es_palabra, espacios, limpio};
use crate::donde;
use crate::dominio::fallo::Error;
use crate::dominio::fmt::MESES;
use crate::dominio::tgss::{Actividad, Alta, Cotizacion, Mutua, Recibo, Regularizacion, Resultado};

/// El primer importe del texto, como lo escribe la Seguridad Social: «1.234,56 €», «1234,56», «-56,70», «1.700
/// €/mes», «10000». Un punto con tres cifras detrás es de miles; con una o dos, de decimales («12.34» = 12,34).
/// None si no hay ninguno o no se entiende («1.23.4»).
pub fn importe(t: &str) -> Option<f64> {
    let b = t.as_bytes();
    let i = (0..b.len()).find(|&i| b[i].is_ascii_digit() || (b[i] == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)))?;
    let signo = if b[i] == b'-' { -1.0 } else { 1.0 };
    let ini = i + usize::from(b[i] == b'-');
    let fin = ini + b[ini..].iter().take_while(|c| c.is_ascii_digit() || **c == b'.' || **c == b',').count();
    // Lo que va pegado detrás («300,00.», «1.000,») es de la frase, no del número.
    let n = t[ini..fin].trim_end_matches(['.', ',']);
    // Miles: de 1 a 3 cifras y luego grupos de 3 tras un punto (o todo seguido, sin puntos).
    let entero = |e: &str| -> Option<f64> {
        let g: Vec<&str> = e.split('.').collect();
        let bien = g.iter().all(|x| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit())) && (g.len() == 1 || (g[0].len() <= 3 && g[1..].iter().all(|x| x.len() == 3)));
        if bien { g.concat().parse().ok() } else { None }
    };
    let v = match (n.rsplit_once(','), n.split_once('.')) {
        (Some((e, d)), _) if !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()) => entero(e)? + format!("0.{d}").parse::<f64>().ok()?,
        (Some(_), _) => return None,
        (None, Some((e, d))) if !d.contains('.') && d.len() != 3 => format!("{e}.{d}").parse().ok()?,
        (None, _) => entero(n)?,
    };
    Some(signo * v)
}

/// La primera fecha del texto: «9 de septiembre de 2025» o «30/10/2026» → «2025-09-09». Solo si es una fecha de
/// verdad (sin cifras pegadas delante ni detrás, y que exista: no el 31 de febrero).
pub fn fecha(t: &str) -> Option<String> {
    let t = t.to_lowercase();
    let valida = |a: &str, m: u32, d: u32| -> Option<String> {
        let a: i32 = a.parse().ok()?;
        chrono::NaiveDate::from_ymd_opt(a, m, d).map(|f| f.format("%Y-%m-%d").to_string())
    };
    if let Some((d, mes, a)) = fecha_larga(&t)
        && let Some(m) = MESES.iter().position(|x| *x == mes)
    {
        return valida(a, m as u32 + 1, d);
    }
    // `(?<!\d)(\d{2})/(\d{2})/(\d{4})(?!\d)`
    let b = t.as_bytes();
    let suelta = |i: usize, n: usize| cifras(&t, i) == n;
    (0..b.len()).find_map(|i| {
        let empieza = i == 0 || !b[i - 1].is_ascii_digit();
        (empieza && suelta(i, 2) && b.get(i + 2) == Some(&b'/') && suelta(i + 3, 2) && b.get(i + 5) == Some(&b'/') && suelta(i + 6, 4))
            .then(|| valida(&t[i + 6..i + 10], t[i + 3..i + 5].parse().ok()?, t[i..i + 2].parse().ok()?))
            .flatten()
    })
}

/// `(?<!\d)(\d{1,2})\s+de\s+(\w+)\s+de\s+(\d{4})(?!\d)`: el primero que encaje (día, palabra del mes, año).
fn fecha_larga(t: &str) -> Option<(u32, &str, &str)> {
    let de = |i: usize| -> Option<usize> {
        let k = espacios(t, i);
        if k == 0 || !t[i + k..].starts_with("de") {
            return None;
        }
        let k2 = espacios(t, i + k + 2);
        (k2 > 0).then_some(i + k + 2 + k2)
    };
    for i in t.char_indices().map(|(i, _)| i) {
        let n = cifras(t, i);
        if n == 0 || n > 2 || t[..i].ends_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let Some(p) = de(i + n) else { continue };
        let largo: usize = t[p..].chars().take_while(|c| es_palabra(*c)).map(char::len_utf8).sum();
        if largo == 0 {
            continue;
        }
        let Some(q) = de(p + largo) else { continue };
        if cifras(t, q) == 4 {
            return Some((t[i..i + n].parse().ok()?, &t[p..p + largo], &t[q..q + 4]));
        }
    }
    None
}

/// Lo que se ve de una página, para el informe de un fallo: su título y sus clases (sin datos de nadie).
fn estructura(t: &str) -> String {
    format!("título «{}»; clases de la página: {}", html::titulo(t), html::clases(t, 30).join(", "))
}

/// La página sin lo que va en cada `<dd>` (los valores de las tarjetas: fechas, direcciones, nombres…): para el
/// fragmento de un fallo bastan las etiquetas.
fn sin_valores(t: &str) -> String {
    let bajo = t.to_ascii_lowercase();
    let (mut out, mut desde) = (String::with_capacity(t.len()), 0);
    while let Some(i) = buscar(&bajo, "<dd", desde) {
        let (Some(gt), Some(f)) = (buscar(&bajo, ">", i), buscar(&bajo, "</dd>", i)) else { break };
        if gt > f {
            break;
        }
        out.push_str(&t[desde..=gt]);
        out.push('…');
        desde = f;
    }
    out.push_str(&t[desde..]);
    out
}

/// Un fallo de una página de Importass: el código, la función que la lee, lo que se esperaba y, como fragmento, el
/// principio de lo que se ve en la página (sin los valores de sus tarjetas).
fn mal(codigo: &str, donde: &str, mensaje: &str, t: &str, esperado: &str) -> Error {
    Error::formato(codigo, donde, mensaje)
        .esperado(esperado)
        .encontrado(estructura(t))
        .fragmento(&html::resumen(&sin_valores(t), 600), &[])
        .reproducir("S en la app (o gesshtor --certificado <tu .p12> --sincronizar). Para probar el lector sin red: guarda la página y pásasela a la función de «Código que lo lee» en una prueba (src/tgss/pruebas.rs)")
}

/// Una etiqueta que está pero cuyo valor no se entiende.
fn no_se_entiende(codigo: &str, donde: &str, etiqueta: &str, valor: &str, que: &str, t: &str) -> Error {
    let e = mal(codigo, donde, &format!("No entiendo «{etiqueta}» en la página de la Seguridad Social."), t, que);
    Error { encontrado: format!("«{etiqueta}»: «{valor}»"), ..e }
}

// ---------------------------------------------------------------- búsquedas

/// `literal[^>]*>([\s\S]*?)cierre`: el contenido del primer elemento que empieza por `literal` y tiene cierre.
fn contenido<'a>(t: &'a str, literal: &str, cierre: &str) -> Option<&'a str> {
    let i = buscar(t, literal, 0)?;
    let gt = buscar(t, ">", i + literal.len())?;
    let f = buscar(t, cierre, gt + 1)?;
    Some(&t[gt + 1..f])
}

/// Cada `<dl>` de la página: (aria-label, [(dt, dd)]) (`_tarjetas`).
pub fn tarjetas(t: &str) -> Vec<(String, Vec<(String, String)>)> {
    let bajo = t.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, "<dl", desde) {
        desde = i + 1;
        if bajo[i + 3..].chars().next().is_some_and(es_palabra) {
            continue;
        }
        let Some(gt) = buscar(&bajo, ">", i + 3) else {
            break;
        };
        let Some(fin) = buscar(&bajo, "</dl>", gt + 1) else {
            break;
        };
        let etiqueta = super::html::atributo(&t[i + 3..gt], "aria-label")
            .unwrap_or("")
            .to_string();
        out.push((etiqueta, pares(&t[gt + 1..fin])));
        desde = fin + 5;
    }
    out
}

/// `<dt[^>]*>([\s\S]*?)</dt>\s*<dd[^>]*>([\s\S]*?)</dd>` (sin distinguir mayúsculas), ya limpios.
fn pares(t: &str) -> Vec<(String, String)> {
    let bajo = t.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut desde = 0;
    'dt: while let Some(i) = buscar(&bajo, "<dt", desde) {
        desde = i + 1;
        let Some(gt) = buscar(&bajo, ">", i + 3) else {
            break;
        };
        let mut tras = gt + 1;
        // Si tras un `</dt>` no viene el `<dd>`, el contenido sigue hasta el siguiente `</dt>` (como la expresión).
        while let Some(c) = buscar(&bajo, "</dt>", tras) {
            let k = c + 5 + espacios(t, c + 5);
            if bajo[k..].starts_with("<dd")
                && let Some(gt2) = buscar(&bajo, ">", k + 3)
                && let Some(f) = buscar(&bajo, "</dd>", gt2 + 1)
            {
                out.push((limpio(&t[gt + 1..c]), limpio(&t[gt2 + 1..f])));
                desde = f + 5;
                continue 'dt;
            }
            tras = c + 1;
        }
    }
    out
}

/// El último valor de una etiqueta (como un `dict` de los pares).
fn valor<'a>(pares: &'a [(String, String)], k: &str) -> Option<&'a str> {
    pares
        .iter()
        .rev()
        .find(|(a, _)| a == k)
        .map(|(_, b)| b.as_str())
}

/// Lo de la página «Datos de trabajo autónomo».
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(test, derive(serde::Deserialize), serde(default))]
pub struct Perfil {
    pub alta: Alta,
    pub actividades: Vec<Actividad>,
    pub cotizacion: Cotizacion,
    pub mutua: Mutua,
}

/// La página «Datos de trabajo autónomo» (`leer_perfil`). Cada tarjeta de una actividad completa la última que ha
/// empezado. Su ancla: la fecha de alta como autónomo (quien tiene esta página la tiene). Las fechas y los importes
/// que están tienen que entenderse; uno sin ninguna cifra («No consta», «-») es que no hay.
pub fn leer_perfil(t: &str) -> Result<Perfil, Error> {
    const D: &str = donde!("leer_perfil");
    let mut p = Perfil::default();
    let mut con_alta = false;
    let fecha_de = |k: &str, v: &str| fecha(v).ok_or_else(|| no_se_entiende("TGSS_PERFIL_FECHA", D, k, v, "una fecha («9 de septiembre de 2025», «30/10/2026»)", t));
    let importe_de = |k: &str, v: Option<&str>| match v {
        Some(v) if v.contains(|c: char| c.is_ascii_digit()) => importe(v).map(Some).ok_or_else(|| no_se_entiende("TGSS_PERFIL_IMPORTE", D, k, v, "un importe («1.000,00 €/mes»)", t)),
        _ => Ok(None),
    };
    // Sin fecha: vacía, «-» o «Sin datos» (así sale la de baja mientras sigues de alta).
    let fecha_o_nada = |k: &str, v: Option<&str>| match v.map(str::trim) {
        Some(v) if !v.is_empty() && v != "-" && !v.eq_ignore_ascii_case("sin datos") => fecha_de(k, v),
        _ => Ok(String::new()),
    };
    for (_, pares) in tarjetas(t) {
        let d = |k: &str| valor(&pares, k);
        if let Some(f) = d("Fecha alta como autónomo") {
            con_alta = true;
            p.alta.fecha = fecha_de("Fecha alta como autónomo", f)?;
            p.alta.cotiza_desde = fecha_o_nada("Cotizas desde", d("Cotizas desde"))?;
        }
        if let Some(f) = d("Fecha baja como autónomo") {
            p.alta.baja = fecha_o_nada("Fecha baja como autónomo", Some(f))?;
        }
        if let Some(f) = d("Fecha de inicio de la actividad") {
            p.actividades.push(Actividad { inicio: fecha_de("Fecha de inicio de la actividad", f)?, ..Default::default() });
        }
        if let Some(a) = p.actividades.last_mut() {
            if let Some(v) = d("Actividad de autónomo") {
                a.tipo = v.to_string();
            }
            if let Some(v) = d("Actividad profesional (CNAE)") {
                a.cnae = cnae(v);
                a.cnae_texto = sin_cnae_final(v);
            }
        }
        if d("Base de cotización actual").is_some() || d("Rendimientos netos previstos").is_some() {
            let c = &mut p.cotizacion;
            c.rendimientos_previstos = importe_de("Rendimientos netos previstos", d("Rendimientos netos previstos"))?;
            c.base = importe_de("Base de cotización actual", d("Base de cotización actual"))?;
            let prox = pares.iter().find(|(k, _)| k.starts_with("Base de cotización próxima")).map_or("", |(_, v)| v.as_str());
            if !prox.is_empty() {
                c.base_proxima = importe_de("Base de cotización próxima", Some(prox))?;
                c.base_proxima_desde = fecha(prox).unwrap_or_default();
            }
        }
        if let Some(v) = d("Última cuota emitida") {
            p.cotizacion.ultima_cuota = importe_de("Última cuota emitida", Some(v))?;
        }
        if let Some(v) = d("Entidad aseguradora") {
            p.mutua = Mutua { entidad: v.to_string() };
        }
    }
    if !con_alta {
        let etiquetas: Vec<String> = tarjetas(t).into_iter().flat_map(|(_, p)| p.into_iter().map(|(k, _)| k)).collect();
        let e = mal("TGSS_PERFIL_SIN_ALTA", D, "No encuentro tu fecha de alta como autónomo en la página de la Seguridad Social.", t, "una tarjeta <dl> con «Fecha alta como autónomo»");
        return Err(Error { encontrado: format!("etiquetas <dt> de la página: {}; {}", if etiquetas.is_empty() { "ninguna".into() } else { etiquetas.join(", ") }, e.encontrado), ..e });
    }
    if let Some(mes) = mes_de_la_cuota(&limpio(t)) {
        p.cotizacion.ultima_cuota_mes = mes;
    }
    Ok(p)
}

/// `CNAE\s*(\d+)`: el código.
fn cnae(v: &str) -> String {
    let mut desde = 0;
    while let Some(i) = buscar(v, "CNAE", desde) {
        let k = i + 4 + espacios(v, i + 4);
        let n = cifras(v, k);
        if n > 0 {
            return v[k..k + n].to_string();
        }
        desde = i + 1;
    }
    String::new()
}

/// Quita `\s*CNAE\s*\d+\s*$` del final.
fn sin_cnae_final(v: &str) -> String {
    let s = v.trim_end();
    let sin_cifras = s.trim_end_matches(|c: char| c.is_ascii_digit());
    if sin_cifras.len() == s.len() {
        return v.to_string();
    }
    match sin_cifras.trim_end().strip_suffix("CNAE") {
        Some(antes) => antes.trim_end().to_string(),
        None => v.to_string(),
    }
}

/// `cuota que corresponde al mes de\s*([a-záéíóú]+)` (sin distinguir mayúsculas), en minúsculas.
fn mes_de_la_cuota(t: &str) -> Option<String> {
    const FRASE: &str = "cuota que corresponde al mes de";
    let bajo = t.to_ascii_lowercase();
    let mut desde = 0;
    while let Some(i) = buscar(&bajo, FRASE, desde) {
        let k = i + FRASE.len() + espacios(t, i + FRASE.len());
        let mes: String = t[k..]
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .take_while(|c| c.is_ascii_lowercase() || "áéíóú".contains(*c))
            .collect();
        if !mes.is_empty() {
            return Some(mes);
        }
        desde = i + 1;
    }
    None
}

// ---------------------------------------------------------------- recibos

const ANIO: &str = "class=\"quota-history__year\"";
const MES: &str = "class=\"quota-history__month\"";
const RECIBO: &str = "class=\"quota-history__item\"";
const GRUPO: &str = "class=\"quota-history__group\"";

/// Una página del historial de recibos: los recibos y el enlace a la página siguiente, si hay (`leer_recibos`). Su
/// ancla, la lista (`quota-history`): sin ella es un fallo; con ella y sin recibos, no hay recibos.
pub fn leer_recibos(t: &str) -> Result<(Vec<Recibo>, Option<String>), Error> {
    if !t.contains("quota-history") {
        let esperado = "la lista de recibos (class=\"quota-history\", con quota-history__year, __month y __item)";
        return Err(mal("TGSS_RECIBOS_SIN_LISTA", donde!("leer_recibos"), "No encuentro la lista de tus recibos en la página de la Seguridad Social.", t, esperado));
    }
    let mut out = Vec::new();
    let (mut anio, mut mes): (Option<i64>, Option<String>) = (None, None);
    let mut desde = 0;
    // Recorre en orden las cabeceras de año, los meses y los recibos.
    while let Some(i) = buscar(t, "class=\"quota-history__", desde) {
        desde = i + 1;
        if t[i..].starts_with(ANIO) {
            if let Some((c, fin)) = cabecera(t, i + ANIO.len(), "h2") {
                anio = limpio(c).trim().parse().ok();
                desde = fin;
            }
        } else if t[i..].starts_with(MES) {
            if let Some((c, fin)) = cabecera(t, i + MES.len(), "h3") {
                mes = Some(limpio(c).to_lowercase());
                desde = fin;
            }
        } else if t[i..].starts_with(RECIBO) {
            let ini = i + RECIBO.len();
            let fin = [buscar(t, RECIBO, ini), buscar(t, GRUPO, ini)].into_iter().flatten().filter(|x| *x > 0).min().unwrap_or(t.len());
            out.push(recibo(&t[ini..fin], anio, mes.as_deref())?);
            desde = ini;
        }
    }
    Ok((out, siguiente(t)))
}

/// `[\s\S]*?<hN[^>]*>([\s\S]*?)</hN>` desde `i`: el contenido y dónde acaba.
fn cabecera<'a>(t: &'a str, i: usize, h: &str) -> Option<(&'a str, usize)> {
    let a = buscar(t, &format!("<{h}"), i)?;
    let gt = buscar(t, ">", a + 3)?;
    let cierre = format!("</{h}>");
    let f = buscar(t, &cierre, gt + 1)?;
    Some((&t[gt + 1..f], f + cierre.len()))
}

/// Un recibo: su título, importe y desglose por conceptos (los que no salen, a 0). Tiene que tener su importe y su
/// mes, sus conceptos tienen que entenderse y, si es la cuota de un mes, tiene que llevar las contingencias comunes.
fn recibo(trozo: &str, anio: Option<i64>, mes: Option<&str>) -> Result<Recibo, Error> {
    const D: &str = donde!("recibo (desde leer_recibos)");
    let titulo = limpio(contenido(trozo, "quota-history__item-title\"", "</a>").unwrap_or(""));
    let emision = match (anio, mes.and_then(|m| MESES.iter().position(|x| *x == m))) {
        (Some(a), Some(m)) if a != 0 => format!("{a}-{:02}", m + 1),
        _ => String::new(),
    };
    let cantidad = limpio(contenido(trozo, "quota-history__item-amount\"", "</div>").unwrap_or(""));
    let importe_ = importe(&cantidad).ok_or_else(|| no_se_entiende("TGSS_RECIBO_SIN_IMPORTE", D, &format!("importe de «{titulo}»"), &cantidad, "un importe en quota-history__item-amount («310,00 €»)", trozo))?;
    let cargo = match cargo(trozo) {
        Some(c) => fecha(c).ok_or_else(|| no_se_entiende("TGSS_RECIBO_FECHA", D, "Fecha de cargo en cuenta", c, "una fecha", trozo))?,
        None => String::new(),
    };
    let periodo = periodo(&titulo.to_lowercase()).unwrap_or_else(|| emision.clone());
    if periodo.is_empty() {
        let e = mal("TGSS_RECIBO_SIN_MES", D, "No sé de qué mes es un recibo de la Seguridad Social.", trozo, "el mes en el título («Cuota octubre 2026») o la cabecera del año y el mes (quota-history__year y __month)");
        return Err(Error { encontrado: format!("título «{titulo}», año {anio:?}, mes {mes:?}"), ..e });
    }
    let mut r = Recibo { importe: importe_, cargo, periodo, emision, concepto: titulo, ..Default::default() };
    let mut con_cc = false;
    for (imp, nombre) in desgloses(trozo) {
        let nombre = desescapar(nombre).trim().to_lowercase();
        let k = &mut r.conceptos;
        let concepto = match nombre.as_str() {
            "contingencias comunes" => &mut k.cc,
            "contingencias profesionales" => &mut k.cp,
            "cese de actividad" => &mut k.cese,
            "formación profesional" => &mut k.fp,
            "mei - mecanismo equidad intergeneracional" => &mut k.mei,
            // Otros (reducciones, bonificaciones…): la app no los desglosa; el importe del recibo ya los cuenta.
            _ => continue,
        };
        let x = limpio(imp);
        *concepto = importe(&x).ok_or_else(|| no_se_entiende("TGSS_RECIBO_DESGLOSE", D, &nombre, &x, "un importe en quota-history__breakdown-amount", trozo))?;
        con_cc |= nombre == "contingencias comunes";
    }
    if r.es_cuota() && !con_cc {
        let e = mal("TGSS_RECIBO_SIN_CC", D, "Un recibo de tu cuota de autónomo no trae las contingencias comunes.", trozo, "en el desglose de la cuota: «Contingencias Comunes» (quota-history__breakdown-amount y encabezadoLista)");
        let vistos: Vec<String> = desgloses(trozo).iter().map(|(_, n)| desescapar(n).trim().to_string()).collect();
        return Err(Error { encontrado: format!("título «{}»; conceptos: {}", r.concepto, if vistos.is_empty() { "ninguno".into() } else { vistos.join(", ") }), ..e });
    }
    Ok(r)
}

/// `quota-history__breakdown-amount"[^>]*>([\s\S]*?)</div>\s*<p class="encabezadoLista">([^<]*)</p>`.
fn desgloses(t: &str) -> Vec<(&str, &str)> {
    const LIT: &str = "quota-history__breakdown-amount\"";
    const NOMBRE: &str = "<p class=\"encabezadoLista\">";
    let mut out = Vec::new();
    let mut desde = 0;
    'fuera: while let Some(i) = buscar(t, LIT, desde) {
        desde = i + 1;
        let Some(gt) = buscar(t, ">", i + LIT.len()) else {
            break;
        };
        let mut tras = gt + 1;
        while let Some(c) = buscar(t, "</div>", tras) {
            let k = c + 6 + espacios(t, c + 6);
            if t[k..].starts_with(NOMBRE) {
                let n0 = k + NOMBRE.len();
                let f = buscar(t, "<", n0).unwrap_or(t.len());
                if t[f..].starts_with("</p>") {
                    out.push((&t[gt + 1..c], &t[n0..f]));
                    desde = f + 4;
                    continue 'fuera;
                }
            }
            tras = c + 1;
        }
    }
    out
}

/// `Fecha de cargo en cuenta</p>\s*<p>([^<]*)</p>`.
fn cargo(t: &str) -> Option<&str> {
    const LIT: &str = "Fecha de cargo en cuenta</p>";
    let mut desde = 0;
    while let Some(i) = buscar(t, LIT, desde) {
        let k = i + LIT.len() + espacios(t, i + LIT.len());
        if t[k..].starts_with("<p>") {
            let f = buscar(t, "<", k + 3).unwrap_or(t.len());
            if t[f..].starts_with("</p>") {
                return Some(&t[k + 3..f]);
            }
        }
        desde = i + 1;
    }
    None
}

/// `(enero|…|diciembre)\s+(\d{4})` → «2026-10».
fn periodo(t: &str) -> Option<String> {
    for (i, _) in t.char_indices() {
        for (m, nombre) in MESES.iter().enumerate() {
            if t[i..].starts_with(nombre) {
                let k = i + nombre.len();
                let e = espacios(t, k);
                if e > 0 && cifras(t, k + e) >= 4 {
                    return Some(format!("{}-{:02}", &t[k + e..k + e + 4], m + 1));
                }
            }
        }
    }
    None
}

/// `<a href="([^"]+)"[^>]*class="ss-pagination__next"`: el enlace a la página siguiente (sin «#…»).
fn siguiente(t: &str) -> Option<String> {
    const A: &str = "<a href=\"";
    let mut desde = 0;
    while let Some(i) = buscar(t, A, desde) {
        desde = i + 1;
        let h0 = i + A.len();
        let Some(q) = buscar(t, "\"", h0).filter(|q| *q > h0) else {
            continue;
        };
        let fin = buscar(t, ">", q + 1).unwrap_or(t.len());
        if t[q + 1..fin].contains("class=\"ss-pagination__next\"") {
            let s = desescapar(&t[h0..q]);
            let s = s.split('#').next().unwrap_or("");
            return (!s.is_empty()).then(|| s.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------- regularizaciones

/// El historial de regularizaciones: el año, el resultado y el importe de cada una (`leer_regularizaciones`). Su
/// ancla, que sea la página de las regularizaciones; un año con un resultado que no se entiende es un fallo.
pub fn leer_regularizaciones(html: &str) -> Result<Vec<Regularizacion>, Error> {
    const LIT: &str = "TRABAJADORES AUTÓNOMOS";
    const D: &str = donde!("leer_regularizaciones");
    let t = limpio(html);
    if !t.to_lowercase().contains("regularizaci") {
        return Err(mal("TGSS_REGULARIZACIONES_PAGINA", D, "No encuentro tus regularizaciones en la página de la Seguridad Social.", html, "la página «Regularización de cuotas de años anteriores»"));
    }
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(&t, LIT, desde) {
        desde = i + 1;
        let k = i + LIT.len();
        let e = espacios(&t, k);
        if e == 0 || cifras(&t, k + e) < 4 {
            continue;
        }
        let anio: i32 = t[k + e..k + e + 4].parse().unwrap_or(0);
        let p = k + e + 4;
        let e2 = espacios(&t, p);
        if e2 == 0 {
            continue;
        }
        let r = &t[p + e2..];
        let (resultado, largo) = if r.starts_with("Sin diferencias") {
            (Resultado::SinDiferencias, "Sin diferencias".len())
        } else if let Some(n) = cifra_con_euro(r, "A ingresar:") {
            (Resultado::Ingresar, n)
        } else if let Some(n) = cifra_con_euro(r, "A devolver:") {
            (Resultado::Devolver, n)
        } else {
            let e = mal("TGSS_REGULARIZACION_RESULTADO", D, &format!("No entiendo el resultado de la regularización de {anio}."), html, "tras el año: «Sin diferencias», «A ingresar: 123,45 €» o «A devolver: 123,45 €»");
            return Err(Error { encontrado: format!("«{LIT} {anio} {}»", r.chars().take(60).collect::<String>()), ..e });
        };
        let importe = if resultado == Resultado::SinDiferencias {
            0.0
        } else {
            importe(&r[..largo]).ok_or_else(|| no_se_entiende("TGSS_REGULARIZACION_IMPORTE", D, &format!("regularización de {anio}"), &r[..largo], "un importe («123,45 €»)", html))?
        };
        out.push(Regularizacion {
            anio,
            resultado,
            importe,
            ..Default::default()
        });
        desde = p + e2 + largo;
    }
    Ok(out)
}

/// `prefijo\s*[\d.,]+\s*€` al principio de `t`: lo que ocupa.
fn cifra_con_euro(t: &str, prefijo: &str) -> Option<usize> {
    if !t.starts_with(prefijo) {
        return None;
    }
    let k = prefijo.len() + espacios(t, prefijo.len());
    let n = cifras_y_signos(t, k);
    if n == 0 {
        return None;
    }
    let e = k + n + espacios(t, k + n);
    t[e..].starts_with('€').then_some(e + '€'.len_utf8())
}

/// `[\d.,]*` desde `i`.
fn cifras_y_signos(t: &str, i: usize) -> usize {
    t.as_bytes().get(i..).map_or(0, |b| {
        b.iter()
            .take_while(|c| c.is_ascii_digit() || **c == b'.' || **c == b',')
            .count()
    })
}

/// `prefijo\s*([\d.,]+)` (y, si `euro`, seguido de `\s*€`): la primera cifra que encaje.
fn cifra_tras<'a>(t: &'a str, prefijo: &str, euro: bool) -> Option<&'a str> {
    let mut desde = 0;
    while let Some(i) = buscar(t, prefijo, desde) {
        desde = i + 1;
        let k = i + prefijo.len() + espacios(t, i + prefijo.len());
        let n = cifras_y_signos(t, k);
        if n == 0 {
            continue;
        }
        if euro && !t[k + n + espacios(t, k + n)..].starts_with('€') {
            continue;
        }
        return Some(&t[k..k + n]);
    }
    None
}

/// `Tu base de cotización para el año \d{4} es de:\s*([\d.,]+)`.
fn base_del_anio(t: &str) -> Option<&str> {
    const A: &str = "Tu base de cotización para el año ";
    const B: &str = " es de:";
    let mut desde = 0;
    while let Some(i) = buscar(t, A, desde) {
        desde = i + 1;
        let k = i + A.len();
        if cifras(t, k) >= 4 && t[k + 4..].starts_with(B) {
            let p = k + 4 + B.len();
            let p = p + espacios(t, p);
            let n = cifras_y_signos(t, p);
            if n > 0 {
                return Some(&t[p..p + n]);
            }
        }
    }
    None
}

/// El detalle de la regularización de un año, tal como se ve en su página: lo que no se ve, `None` (o `NoSe`).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    test,
    derive(serde::Deserialize),
    serde(default, rename_all = "camelCase")
)]
pub struct Detalle {
    pub anio: Option<i32>,
    pub base: Option<f64>,
    pub rendimiento_anual: Option<f64>,
    pub rendimiento_mensual: Option<f64>,
    pub promedio_provisional: Option<f64>,
    pub importe: Option<f64>,
    pub pagado: bool,
    pub resultado: Resultado,
}

impl Detalle {
    /// Completa la regularización del historial con lo que se ve en el detalle.
    fn completar(self, r: &mut Regularizacion) {
        r.base = self.base.or(r.base);
        r.rendimiento_anual = self.rendimiento_anual.or(r.rendimiento_anual);
        r.rendimiento_mensual = self.rendimiento_mensual.or(r.rendimiento_mensual);
        r.promedio_provisional = self.promedio_provisional.or(r.promedio_provisional);
        r.importe = self.importe.unwrap_or(r.importe);
        r.pagado = Some(self.pagado);
        if self.resultado != Resultado::NoSe {
            r.resultado = self.resultado;
        }
    }
}

/// El detalle de la regularización de un año: base definitiva, rendimientos y resultado (`leer_regularizacion`). Su
/// ancla, el año; los importes que están tienen que entenderse.
pub fn leer_regularizacion(html: &str) -> Result<Detalle, Error> {
    const D: &str = donde!("leer_regularizacion");
    let t = limpio(html);
    const ANIO: &str = "Regularización de cuotas para el año";
    let mut anio = None;
    let mut desde = 0;
    while let Some(i) = buscar(&t, ANIO, desde) {
        desde = i + 1;
        let k = i + ANIO.len();
        let e = espacios(&t, k);
        if e > 0 && cifras(&t, k + e) >= 4 {
            anio = Some(t[k + e..k + e + 4].parse().unwrap_or(0));
            break;
        }
    }
    let Some(anio) = anio.filter(|a| *a != 0) else {
        return Err(mal("TGSS_REGULARIZACION_SIN_ANIO", D, "No sé de qué año es una regularización de la Seguridad Social.", html, "«Regularización de cuotas para el año AAAA»"));
    };
    let bajo = t.to_lowercase();
    let resultado = if t.contains("debes ingresar") {
        Resultado::Ingresar
    } else if bajo.contains("te devolver") || bajo.contains("a devolver") {
        Resultado::Devolver
    } else if bajo.contains("sin diferencias") {
        Resultado::SinDiferencias
    } else {
        Resultado::NoSe
    };
    // Una cifra tras su etiqueta: si está, tiene que entenderse.
    let cifra = |etiqueta: &str, v: Option<&str>| v.map(|v| importe(v).ok_or_else(|| no_se_entiende("TGSS_REGULARIZACION_IMPORTE", D, etiqueta, v, "un importe («1.234,56 €»)", html))).transpose();
    Ok(Detalle {
        anio: Some(anio),
        base: cifra("Tu base de cotización", base_del_anio(&t))?,
        rendimiento_anual: cifra("Rendimiento neto anual", cifra_tras(&t, "Rendimiento neto anual", false))?,
        rendimiento_mensual: cifra("Rendimiento neto mensual", cifra_tras(&t, "Rendimiento neto mensual", false))?,
        promedio_provisional: cifra("Promedio mensual", cifra_tras(&t, "Promedio mensual de tus bases de cotización provisionales:", false))?,
        importe: cifra("Resultado de la regularización", cifra_tras(&t, "Resultado de la regularización", true))?,
        pagado: t.contains("Ingreso ya realizado"),
        resultado,
    })
}

/// Junta el historial con los detalles de cada año (uno sin su año en el historial, se añade) y ordena del más
/// reciente al más antiguo.
pub fn juntar_regularizaciones(
    mut lista: Vec<Regularizacion>,
    detalles: Vec<Detalle>,
) -> Vec<Regularizacion> {
    for d in detalles {
        let Some(anio) = d.anio else { continue };
        match lista.iter_mut().find(|r| r.anio == anio) {
            Some(r) => d.completar(r),
            None if anio != 0 => {
                let mut r = Regularizacion {
                    anio,
                    ..Default::default()
                };
                d.completar(&mut r);
                lista.push(r);
            }
            None => {}
        }
    }
    lista.sort_by_key(|r| std::cmp::Reverse(r.anio));
    lista
}

// ---------------------------------------------------------------- área personal

/// ¿Es una página de Importass con la sesión iniciada?
pub fn dentro(t: &str) -> bool {
    t.contains("\"authenticated\":\"true\"")
}

/// ¿Estás (o estuviste) en el régimen de autónomos? Lo dice la marca de tu colectivo en tu área personal
/// («ColectivoRETA»; otro colectivo, «ColectivoRGSS»…): sin ninguna marca, la página ha cambiado y no se sabe.
pub fn autonomo(t: &str) -> Result<bool, Error> {
    if t.contains("ColectivoRETA") {
        return Ok(true);
    }
    if t.match_indices("Colectivo").any(|(i, m)| t[i + m.len()..].starts_with(|c: char| c.is_ascii_uppercase())) {
        return Ok(false);
    }
    let esperado = "la marca de tu colectivo en la página («ColectivoRETA» si eres o fuiste autónomo, «ColectivoRGSS»…)";
    Err(mal("TGSS_PERSONAL_SIN_COLECTIVO", donde!("autonomo"), "No sé si eres autónomo: tu área personal de la Seguridad Social no lo dice como antes.", t, esperado))
}

/// Los enlaces del área personal `href="\s*(p0/[^"]*MARCA[^"]*)"` (sin «#…»).
pub fn enlaces(t: &str, marca: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(t, "href=\"", desde) {
        desde = i + 1;
        let k = i + 6 + espacios(t, i + 6);
        if !t[k..].starts_with("p0/") {
            continue;
        }
        let Some(f) = buscar(t, "\"", k) else { break };
        let v = &t[k..f];
        if v.contains(marca) {
            out.push(desescapar(v).split('#').next().unwrap_or("").to_string());
            desde = f + 1;
        }
    }
    out
}
