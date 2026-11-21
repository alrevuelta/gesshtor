//! Lo que cotizáis tú y tu empresa por tu nómina, mes a mes: la «Consulta de cálculos de cuotas para trabajador»
//! de la Sede (sp.seg-social.es, aplicación Prosa). Por cada mes, las liquidaciones que presentó tu empresa (la
//! normal y las complementarias, p. ej. por atrasos) y, de cada una, la base y la cuota de la empresa y la tuya por
//! conceptos. Solo consulta.
//!
//! Lo que no se entiende es un fallo (`Error`, con su informe): una página sin lo que tiene que tener (el mes, las
//! liquidaciones de la lista, los conceptos de una liquidación con sus cuotas) o con un importe o una fecha que no
//! se entienden. Un mes sin liquidaciones solo lo es si la Sede lo dice (su aviso 7101).
use super::html::{buscar, cifras, consulta, desescapar, donde, espacios};
use super::importass::{fecha, importe};
use super::{MAX_MESES, enviar, paso, pedir};
use crate::dominio::fallo::{sin_consulta, Error};
use crate::red::cliente::Cliente;
use crate::dominio::tgss::{ConceptoSs, Liquidacion, Meses};

pub const SEDE_CUOTAS: &str = "https://sp.seg-social.es/ProsaInternet/OnlineAccess?ARQ.SPM.ACTION=LOGIN&ARQ.SPM.APPTYPE=SERVICE\
                               &ARQ.IDAPP=XV21YD00&PAUC.NIVEL=1&PAUC.TIPO_IDENTIFICACION=0";
pub const FRONTEND: &str = "https://sp.seg-social.es/ProsaInternet/OnlineAccessUtf8";
/// «No existe liquidación presentada … para el periodo solicitado».
const SIN_LIQUIDACION: &str = "7101";
/// «Periodo liquidación no válido»: la Sede aún no da ese mes (el anterior, mientras dura su plazo).
const AUN_NO: &str = "7008";

/// Una página de la Sede: su XML de datos, el ticket del servicio y los mensajes de error.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prosa {
    pub xml: String,
    pub ticket: String,
    pub sesion: String,
    pub errores: Vec<String>,
}

impl Prosa {
    pub fn de(t: &str) -> Result<Prosa, Error> {
        let raro = || {
            Error::formato("TGSS_SEDE_RESPUESTA", crate::donde!("Prosa::de"), format!("La Sede de la Seguridad Social no ha contestado como se esperaba ({}).", donde(t)))
                .esperado("una página de Prosa con su XML de datos (<script id=\"xml\">) y su ticket (<SPM.TICKET id=\"SPM.TICKET\">)")
                .encontrado(format!("título «{}»; clases de la página: {}", super::html::titulo(t), super::html::clases(t, 30).join(", ")))
        };
        if !t.contains("SPM.TICKET") {
            return Err(raro());
        }
        let xml = entre(t, "<script id=\"xml\" type=\"text/plain\">", "</script>").unwrap_or(t);
        let ticket = tras(xml, "<SPM.TICKET id=\"SPM.TICKET\">", true).ok_or_else(raro)?;
        let sesion = tras(xml, "<SPM.IDSESSION id=\"SPM.IDSESSION\">", false).unwrap_or("");
        Ok(Prosa { xml: xml.to_string(), ticket: ticket.to_string(), sesion: sesion.to_string(), errores: errores(xml) })
    }

    /// El valor de la primera etiqueta `<nombre>` del XML.
    pub fn dato(&self, nombre: &str) -> String {
        dato_en(&self.xml, nombre)
    }

    pub fn bloques(&self, nombre: &str) -> Vec<&str> {
        bloques(&self.xml, nombre)
    }
}

/// `a([\s\S]*?)b`
fn entre<'a>(t: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = buscar(t, a, 0)? + a.len();
    Some(&t[i..buscar(t, b, i)?])
}

/// `literal([^<]*)` (o `[^<]+` si `lleno`).
fn tras<'a>(t: &'a str, literal: &str, lleno: bool) -> Option<&'a str> {
    let mut desde = 0;
    while let Some(i) = buscar(t, literal, desde) {
        let k = i + literal.len();
        let v = &t[k..buscar(t, "<", k).unwrap_or(t.len())];
        if !(lleno && v.is_empty()) {
            return Some(v);
        }
        desde = i + 1;
    }
    None
}

/// `<TIPO>ERROR</TIPO>\s*<TEXTO><![CDATA[…]]>`: los mensajes de error, con los espacios juntos.
fn errores(xml: &str) -> Vec<String> {
    const TIPO: &str = "<TIPO>ERROR</TIPO>";
    const TEXTO: &str = "<TEXTO><![CDATA[";
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(xml, TIPO, desde) {
        desde = i + 1;
        let k = i + TIPO.len() + espacios(xml, i + TIPO.len());
        if !xml[k..].starts_with(TEXTO) {
            continue;
        }
        let Some(f) = buscar(xml, "]]>", k + TEXTO.len()) else { break };
        out.push(xml[k + TEXTO.len()..f].split_whitespace().collect::<Vec<_>>().join(" "));
        desde = f + 3;
    }
    out
}

/// `<nombre(?:\s[^>]*)?>([^<]*)</nombre>` en `xml`: el primero, sin entidades ni espacios alrededor («» si no está).
pub fn dato_en(xml: &str, nombre: &str) -> String {
    dato(xml, nombre).unwrap_or_default()
}

/// Lo mismo, distinguiendo «no está» (None) de «está vacío».
pub fn dato(xml: &str, nombre: &str) -> Option<String> {
    let abre = format!("<{nombre}");
    let cierra = format!("</{nombre}>");
    let mut desde = 0;
    while let Some(i) = buscar(xml, &abre, desde) {
        desde = i + 1;
        let j = i + abre.len();
        let ini = match xml[j..].chars().next() {
            Some('>') => j + 1,
            Some(c) if c.is_whitespace() => match buscar(xml, ">", j) {
                Some(g) => g + 1,
                None => break,
            },
            _ => continue,
        };
        let fin = buscar(xml, "<", ini).unwrap_or(xml.len());
        if xml[fin..].starts_with(&cierra) {
            return Some(desescapar(&xml[ini..fin]).trim().to_string());
        }
    }
    None
}

/// Las etiquetas de un XML, sin repetir y en orden (como mucho `n`): para el informe de un fallo, su estructura sin
/// los datos.
fn etiquetas(xml: &str, n: usize) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for (i, _) in xml.match_indices('<') {
        let e: String = xml[i + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_').collect();
        if !e.is_empty() && v.len() < n && !v.contains(&e) {
            v.push(e);
        }
    }
    v
}

/// Las etiquetas cuyo valor se deja ver en el fragmento de un fallo: las de la estructura y las cifras. Las demás
/// (la empresa, su cuenta de cotización, el ticket, la sesión, tu nombre o tu número de afiliación…) salen como «…».
const SE_VEN: [&str; 17] = [
    "tituloPantalla", "desDatCal", "baseCalValor", "cuotaCalValPatronal", "cuotaCalValObrera", "cuotaCalVaTotal", "tipoLiq", "descTipoLiq", "posicion",
    "perSolicitud", "periodoControl", "fechHoraCalc", "MESLIQUIDACION", "ANNIOLIQUIDACION", "mesLiquidacion", "annioLiquidacion", "TIPO",
];

/// El XML con el valor de cada etiqueta que no está en `SE_VEN` cambiado por «…».
fn sin_valores(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut resto = xml;
    while let Some(i) = resto.find('>') {
        out.push_str(&resto[..=i]);
        resto = &resto[i + 1..];
        let abierta = out.rfind('<').map(|a| &out[a + 1..out.len() - 1]).unwrap_or("");
        let nombre = abierta.split([' ', '/']).next().unwrap_or("");
        let fin = resto.find('<').unwrap_or(resto.len());
        let valor = &resto[..fin];
        if !valor.trim().is_empty() && !abierta.starts_with('/') && !SE_VEN.contains(&nombre) && !resto[fin..].starts_with("<![CDATA[") {
            out.push('…');
            resto = &resto[fin..];
        }
    }
    out.push_str(resto);
    out
}

/// Un fallo de una página de la Sede: lo que se esperaba y, como fragmento, el trozo del XML donde estaba (sin los
/// valores que no son de su estructura: `sin_valores`).
fn mal(codigo: &str, donde: &str, mensaje: &str, xml: &str, esperado: &str) -> Error {
    Error::formato(codigo, donde, mensaje)
        .esperado(esperado)
        .encontrado(format!("etiquetas del XML: {}", etiquetas(xml, 40).join(", ")))
        .fragmento(&sin_valores(xml), &[])
        .reproducir("S en la app (o gesshtor --certificado <tu .p12> --sincronizar). Para probar el lector sin red: guarda la página y pásasela a sede::Prosa::de y a la función de «Código que lo lee» en una prueba (src/tgss/pruebas.rs)")
}

/// `<nombre>([\s\S]*?)</nombre>`: el contenido de cada uno.
pub fn bloques<'a>(xml: &'a str, nombre: &str) -> Vec<&'a str> {
    let abre = format!("<{nombre}>");
    let cierra = format!("</{nombre}>");
    let mut out = Vec::new();
    let mut desde = 0;
    while let Some(i) = buscar(xml, &abre, desde) {
        let ini = i + abre.len();
        let Some(f) = buscar(xml, &cierra, ini) else { break };
        out.push(&xml[ini..f]);
        desde = f + cierra.len();
    }
    out
}

/// «202602» → «2026-02» («» si no es un mes).
pub fn ym(t: &str) -> String {
    if t.len() == 6 && cifras(t, 0) == 6 { format!("{}-{}", &t[..4], &t[4..]) } else { String::new() }
}

/// ¿Es un mes que se puede pedir («2025-12»)? (`20\d\d-(0[1-9]|1[0-2])`)
pub fn mes_valido(m: &str) -> bool {
    let b = m.as_bytes();
    b.len() == 7 && m.starts_with("20") && cifras(m, 0) == 4 && b[4] == b'-' && cifras(m, 5) == 2 && (1..=12).contains(&m[5..].parse::<u32>().unwrap_or(0))
}

/// El mes al que corresponde una página de la consulta («AAAA-MM»), para comprobar que es el pedido.
pub fn mes_de(p: &Prosa) -> String {
    let titulo = p.dato("tituloPantalla");
    let mut desde = 0;
    // `PARA\s+(\d{1,2})/(\d{4})`
    while let Some(i) = buscar(&titulo, "PARA", desde) {
        desde = i + 1;
        let k = i + 4 + espacios(&titulo, i + 4);
        let n = cifras(&titulo, k);
        if k > i + 4 && (1..=2).contains(&n) && titulo[k + n..].starts_with('/') && cifras(&titulo, k + n + 1) >= 4 {
            let mes: u32 = titulo[k..k + n].parse().unwrap_or(0);
            return format!("{}-{mes:02}", &titulo[k + n + 1..k + n + 5]);
        }
    }
    let o = |a: &str, b: &str| {
        let v = p.dato(a);
        if v.is_empty() { p.dato(b) } else { v }
    };
    let (a, n) = (o("annioLiquidacion", "ANNIOLIQUIDACION"), o("mesLiquidacion", "MESLIQUIDACION"));
    if !a.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
        format!("{a}-{:02}", n.parse::<u64>().unwrap_or(0))
    } else {
        String::new()
    }
}

/// El detalle de una liquidación (`leer_liquidacion`), sin el mes en que se presentó (lo pone quien la pide). Las
/// cuotas, las de «LIQUIDO DE TOTALES»; si no está, la suma de los conceptos que no son un «LIQUIDO…». Tiene que
/// tener su tipo, su empresa y sus conceptos, cada uno con sus cuatro cifras (vacía vale 0: la Sede deja así la
/// base de las líneas de totales); una cifra o una fecha que no se entienden son un fallo.
pub fn leer_liquidacion(p: &Prosa) -> Result<Liquidacion, Error> {
    const D: &str = crate::donde!("leer_liquidacion");
    let falta = |que: &str, bloque: &str| mal("TGSS_SEDE_LIQUIDACION", D, "Una liquidación de tu nómina en la Sede de la Seguridad Social no es como siempre.", bloque, que);
    let bloques = p.bloques("CrtrEnCalcAcumTrabajador");
    if bloques.is_empty() {
        return Err(falta("los conceptos de la liquidación (<CrtrEnCalcAcumTrabajador>)", &p.xml));
    }
    let mut conceptos = Vec::new();
    for b in bloques {
        let cifra = |n: &str| -> Result<f64, Error> {
            match dato(b, n) {
                None => Err(falta(&format!("<{n}> en cada concepto"), b)),
                Some(v) if v.is_empty() => Ok(0.0),
                Some(v) => importe(&v).ok_or_else(|| falta(&format!("un importe en <{n}> («2.500,00»), no «{v}»"), b)),
            }
        };
        let concepto = dato(b, "desDatCal").filter(|c| !c.is_empty()).ok_or_else(|| falta("el nombre del concepto (<desDatCal>)", b))?;
        conceptos.push(ConceptoSs { concepto, base: cifra("baseCalValor")?, empresa: cifra("cuotaCalValPatronal")?, trabajador: cifra("cuotaCalValObrera")?, total: cifra("cuotaCalVaTotal")? });
    }
    let total = conceptos.iter().find(|c| c.concepto.to_uppercase() == "LIQUIDO DE TOTALES");
    let suma = |f: fn(&ConceptoSs) -> f64| {
        let s: f64 = conceptos.iter().filter(|c| !c.es_total()).map(f).sum();
        crate::dominio::fmt::r2(s)
    };
    let cuota_empresa = total.map_or_else(|| suma(|c| c.empresa), |t| t.empresa);
    let cuota_trabajador = total.map_or_else(|| suma(|c| c.trabajador), |t| t.trabajador);
    let tipo = dato(&p.xml, "tipoLiq").filter(|t| !t.is_empty()).ok_or_else(|| falta("el tipo de liquidación (<tipoLiq>: L00, L03…)", &p.xml))?;
    let empresa = dato(&p.xml, "razonSocialEmp").ok_or_else(|| falta("la empresa (<razonSocialEmp>)", &p.xml))?;
    let calculada = match p.dato("fechHoraCalc") {
        c if c.is_empty() => String::new(),
        c => fecha(&c).ok_or_else(|| falta(&format!("una fecha en <fechHoraCalc> («15/04/2026 10:20:30»), no «{c}»"), &p.xml))?,
    };
    Ok(Liquidacion {
        empresa,
        ccc: p.dato("ccc"),
        tipo,
        descripcion: capitalizar(&p.dato("descTipoLiq")),
        calculada,
        presentada: String::new(),
        conceptos,
        cuota_empresa,
        cuota_trabajador,
    })
}

/// «NORMAL» → «Normal» (`str.capitalize`).
fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c.as_str().to_lowercase().chars()).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------- la consulta

/// Una página de la Sede; si hace falta, entra con el certificado (solo los formularios SAML y la elección
/// «DNIe o certificado»).
fn sede(red: &Cliente, url: &str, campos: Option<&[(&str, &str)]>) -> Result<(String, String), Error> {
    let mut p = pedir(red, url, campos, None)?;
    for _ in 0..10 {
        match paso(&p, false)? {
            Some((accion, campos)) => p = enviar(red, &accion, &campos, &p.url)?,
            None => return Ok((p.url, p.texto)),
        }
    }
    Err(Error::de("No se ha podido entrar en la Sede de la Seguridad Social: el inicio de sesión no termina."))
}

/// La página de la Sede como `Prosa`; si no lo es, el fallo dice cuál era (sin su consulta).
fn prosa(r: (String, String)) -> Result<Prosa, Error> {
    let (url, texto) = r;
    Prosa::de(&texto).map_err(|e| e.en(format!("la Sede (Prosa): {}", sin_consulta(&url))))
}

fn abrir(red: &Cliente) -> Result<Prosa, Error> {
    prosa(sede(red, SEDE_CUOTAS, None)?)
}

fn pedir_mes(red: &Cliente, p: &Prosa, ym: &str) -> Result<Prosa, Error> {
    let campos = [
        ("ARQ.SPM.TICKET", p.ticket.as_str()),
        ("SPM.CONTEXT", "internet"),
        ("ARQ.SPM.OUT", "XML_STYLESHEET"),
        ("ES_FW4", "1"),
        ("SPM.ISPOPUP", "0"),
        ("ARQ.SPM.IDIOMA", "ES"),
        ("SPM.HAYJS", "1"),
        ("ARQ.SPM.APPTYPE", "SERVICE"),
        ("MESLIQUIDACION", &ym[5..7]),
        ("ANNIOLIQUIDACION", &ym[..4]),
        ("SPM.ACC.ACEPTAR", "ACEPTAR"),
    ];
    prosa(sede(red, &format!("{FRONTEND}{}", p.sesion), Some(&campos))?)
}

fn pedir_detalle(red: &Cliente, p: &Prosa, posicion: &str) -> Result<Prosa, Error> {
    let q = consulta(&[
        ("SPM.CONTEXT", "internet"),
        ("ARQ.SPM.OUT", "XML_STYLESHEET"),
        ("ES_FW4", "1"),
        ("ARQ.SPM.IDIOMA", "ES"),
        ("SPM.HAYJS", "1"),
        ("ARQ.SPM.TICKET", &p.ticket),
        ("SPM.ISPOPUP", "0"),
        ("SPM.ACC.CONTINUAR", "CONTINUAR"),
        ("LIQUIDACIONSEL", posicion),
    ]);
    prosa(sede(red, &format!("{FRONTEND}{}?{q}", p.sesion), None)?)
}

/// Cierra el servicio en la Sede, como hace el navegador al salir (no cambia nada; si falla, da igual).
fn cerrar(red: &Cliente, p: &Prosa) {
    let _ = pedir(red, &format!("{FRONTEND}{}?ARQ.SPM.TICKET={}&ARQ.SPM.ACTION=CANCELAPP&ARQ.SPM.CAUSA=CANC_AUTM_FW4", p.sesion, p.ticket), None, None);
}

/// El servicio abierto: su formulario y si ya ha enseñado un cálculo (desde ahí no acepta otro mes).
struct Servicio {
    p: Prosa,
    usado: bool,
}

impl Servicio {
    /// El formulario para pedir un mes: si el servicio ya enseñó un cálculo, se cierra y se abre otro (en la misma
    /// sesión, sin volver a identificarse).
    fn formulario(&mut self, red: &Cliente) -> Result<&Prosa, Error> {
        if self.usado {
            cerrar(red, &self.p);
            self.p = abrir(red)?;
        }
        self.usado = false;
        Ok(&self.p)
    }
}

/// Lo cotizado por tu nómina en cada mes («AAAA-MM»): mes → liquidaciones (lista vacía si no hubo) y los meses que
/// la Sede aún no da. Como el navegador al volver atrás: una sola sesión; cada mes se pide desde el formulario y, en
/// cuanto la Sede ha enseñado un cálculo, se cierra ese servicio y se abre otro. Al acabar (también si algo falla)
/// se cierra el último. Solo los meses válidos y como mucho `MAX_MESES`; sin meses no se pide nada.
pub fn nominas(red: &Cliente, meses: &[String]) -> Result<(Meses, Vec<String>), Error> {
    let meses: Vec<&str> = meses.iter().map(String::as_str).filter(|m| mes_valido(m)).take(MAX_MESES).collect();
    if meses.is_empty() {
        return Ok(Default::default());
    }
    let mut s = Servicio { p: abrir(red)?, usado: false };
    let r = recorrer(red, &mut s, &meses);
    cerrar(red, &s.p);
    r
}

fn recorrer(red: &Cliente, s: &mut Servicio, meses: &[&str]) -> Result<(Meses, Vec<String>), Error> {
    let mut out = Meses::new();
    let mut aun_no = Vec::new();
    for &mes in meses {
        let de_quien = || format!("la Sede (Prosa): tus cotizaciones de {mes}, {}", sin_consulta(FRONTEND));
        match un_mes(red, s, mes).map_err(|e| e.en(de_quien()))? {
            Some(liqs) => {
                out.insert(mes.to_string(), liqs);
            }
            None => aun_no.push(mes.to_string()),
        }
    }
    Ok((out, aun_no))
}

/// Lo cotizado en un mes: sus liquidaciones (ninguna si la Sede dice que no hay) o None si aún no lo da.
fn un_mes(red: &Cliente, s: &mut Servicio, mes: &str) -> Result<Option<Vec<Liquidacion>>, Error> {
    const D: &str = crate::donde!("un_mes (desde nominas)");
    let dice = |e: &str| Error::formato("TGSS_SEDE_AVISO", D, format!("La Seguridad Social dice: {e}")).esperado("una liquidación, o los avisos 7101 (no hay) o 7008 (aún no)");
    let mut lista = pedir_mes(red, s.formulario(red)?, mes)?;
    let m = mes_de(&lista);
    if m != mes {
        let msg = format!("La Sede de la Seguridad Social ha devuelto otro mes ({}) al pedir {mes}.", if m.is_empty() { "?" } else { &m });
        return Err(mal("TGSS_SEDE_OTRO_MES", D, &msg, &lista.xml, "el mes pedido en <tituloPantalla> («… PARA 3/2026») o en annioLiquidacion y mesLiquidacion (ver mes_de)"));
    }
    if lista.errores.iter().any(|e| e.contains(SIN_LIQUIDACION)) {
        return Ok(Some(Vec::new()));
    }
    if lista.errores.iter().any(|e| e.contains(AUN_NO)) {
        return Ok(None);
    }
    if let Some(e) = lista.errores.first() {
        return Err(dice(e));
    }
    s.usado = true;
    if lista.xml.contains("CrtrEnSalidaNCBN2211") {
        // Una sola liquidación: la Sede la enseña directamente.
        let fpr = lista.dato("fprAutorizada");
        let fpr = if fpr.is_empty() { lista.dato("fprCalculada") } else { fpr };
        return Ok(Some(vec![Liquidacion { presentada: ym(&fpr), ..leer_liquidacion(&lista)? }]));
    }
    // Cada liquidación del mes: su posición en la lista, su tipo y cuándo se presentó.
    let resumen: Vec<[String; 3]> = lista
        .bloques("CrtrEnLiquidacionesTrabajador")
        .into_iter()
        .map(|b| [dato_en(b, "posicion"), dato_en(b, "tipoLiq"), ym(&dato_en(b, "perSolicitud"))])
        .collect();
    // Sin el aviso de «no hay liquidaciones» (7101), la lista no puede estar vacía.
    if resumen.is_empty() || resumen.iter().any(|[posicion, tipo, _]| posicion.is_empty() || tipo.is_empty()) {
        let esperado = "una liquidación sola (<CrtrEnSalidaNCBN2211>) o la lista (<CrtrEnLiquidacionesTrabajador>, cada una con <posicion> y <tipoLiq>), o el aviso 7101 de que no hay";
        return Err(mal("TGSS_SEDE_LISTA", D, &format!("No entiendo la lista de liquidaciones de {mes} de la Sede de la Seguridad Social."), &lista.xml, esperado));
    }
    let mut liqs = Vec::new();
    for (i, [posicion, tipo, presentada]) in resumen.iter().enumerate() {
        if i > 0 {
            lista = pedir_mes(red, s.formulario(red)?, mes)?;
            s.usado = true;
        }
        let det = pedir_detalle(red, &lista, posicion)?;
        if let Some(e) = det.errores.first() {
            return Err(dice(e));
        }
        if mes_de(&det) != mes || det.dato("tipoLiq") != *tipo {
            let msg = format!("La Sede de la Seguridad Social ha devuelto otra liquidación al pedir {mes}.");
            return Err(mal("TGSS_SEDE_OTRO_MES", D, &msg, &det.xml, &format!("el detalle de {mes} con <tipoLiq>{tipo}</tipoLiq>")));
        }
        liqs.push(Liquidacion { presentada: presentada.clone(), ..leer_liquidacion(&det)? });
    }
    Ok(Some(liqs))
}
