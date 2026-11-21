//! La Seguridad Social: tus datos de autónomo (Importass: alta, actividades, cotización, recibos,
//! regularizaciones) y las cotizaciones de tus nóminas mes a mes (la sede). Solo consulta: nunca envía trámites.
//!
//! Se usa con `SeguridadSocial`: se entra con el certificado y luego se pide cada cosa (`perfil`, `recibos`,
//! `regularizaciones`, `nominas`, o `autonomo` para todo lo de autónomo junto). Los lectores de las páginas son
//! funciones puras que dan los tipos de `dominio::tgss` o, si la página no es como se espera, el fallo (`Error`, con
//! su informe: ver `dominio::fallo`), nunca algo vacío o a 0.
//!
//! - `html`: leer HTML (entidades, etiquetas, formularios, URL).
//! - `importass`: leer las páginas de Importass.
//! - `sede`: la consulta de cuotas de la Sede (Prosa).
//! - `reglas`: los tipos y bases de cotización de cada año (para la calculadora de sueldos).
mod html;
mod importass;
#[cfg(test)]
mod pruebas;
pub mod reglas;
mod sede;

use crate::donde;
use crate::dominio::fallo::{sin_consulta, Error};
use crate::red::certificado::Certificado;
use crate::red::cliente::{Cliente, Cuerpo, Respuesta, DOMINIOS_TGSS};
use crate::dominio::tgss::{Meses, Recibo, Regularizacion, Tgss};
use crate::dominio::Datos;
use importass::Perfil;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

const IMPORTASS: &str = "https://portal.seg-social.gob.es/wps/myportal/importass/importass/personal/";
const RECIBOS: &str = "https://portal.seg-social.gob.es/wps/myportal/importass/importass/tramites/historialRecibosRETA";
const REGULARIZACIONES: &str = "https://portal.seg-social.gob.es/wps/myportal/importass/importass/tramites/regularizacionCuotasAnyosAnteriores";
/// La opción «DNIe o certificado» de la pasarela (IPCE pide el certificado en la conexión).
const IPCE: &str = "https://idp.seg-social.es/PGIS/Login?seleccion=IPCE";
const NO_ACEPTA: &str = "No se ha podido entrar en la Seguridad Social: no ha aceptado tu certificado.";

/// Meses de nómina que se piden como mucho de una vez (unos 0,3 s por mes sin nómina y 0,8 s con ella).
const MAX_MESES: usize = 24;
/// Las complementarias (p. ej. por atrasos) llegan meses después: estos últimos meses se vuelven a mirar.
const RECIENTES: i32 = 7;
/// … si se miraron hace más de estas horas (también los que la Seguridad Social aún no daba).
const CADUCA_HORAS: i64 = 12;

/// La Seguridad Social con tu certificado, ya dentro. Solo existe si se ha podido entrar; si la sesión caduca a
/// mitad, cada consulta vuelve a entrar sola.
pub struct SeguridadSocial {
    red: Cliente,
    /// Tu área personal de Importass: si eres autónomo y los enlaces a tu perfil y a tus regularizaciones.
    personal: Pagina,
}

impl SeguridadSocial {
    /// Entra con tu certificado (la opción «DNIe o certificado»). Error si no lo acepta. Al entrar se suele llegar
    /// ya a tu área personal; si no, se pide.
    pub fn entrar(cert: &Certificado) -> Result<Self, Error> {
        let red = Cliente::new(Some(cert), DOMINIOS_TGSS);
        let p = iniciar_sesion(&red)?;
        let personal = if p.url == IMPORTASS { p } else { pagina(&red, IMPORTASS)? };
        Ok(SeguridadSocial { red, personal })
    }

    /// ¿Estás (o estuviste) en el régimen de autónomos? Sale de tu área personal: no pregunta nada. Si la página no
    /// lo dice como siempre, es un fallo (no un «no»: eso borraría lo que se sabe de tu cuota).
    pub fn es_autonomo(&self) -> Result<bool, Error> {
        importass::autonomo(&self.personal.texto).map_err(|e| e.en(fuente(&self.personal.url)))
    }

    /// Los enlaces de tu área personal con esa marca, ya completos.
    fn enlaces(&self, marca: &str) -> Vec<String> {
        let base = html::base(&self.personal.url, &self.personal.texto);
        importass::enlaces(&self.personal.texto, marca).iter().map(|e| html::unir(&base, e)).collect()
    }

    /// Tu alta, actividades, cotización y mutua. Si no eres autónomo, vacío (sin preguntar).
    pub fn perfil(&self) -> Result<Perfil, Error> {
        if !self.es_autonomo()? {
            return Ok(Perfil::default());
        }
        let Some(url) = self.enlaces("ACC_PERFIL_AUTONOMO").into_iter().next() else {
            let t = &self.personal.texto;
            let marcas: Vec<String> = t.match_indices("ACC_").map(|(i, _)| t[i..].chars().take_while(|c| c.is_ascii_uppercase() || *c == '_').collect()).collect();
            return Err(Error::formato("TGSS_PERSONAL_SIN_ENLACE", donde!("perfil"), "Eres autónomo, pero no encuentro en tu área personal el enlace a tus datos de autónomo.")
                .en(fuente(&self.personal.url))
                .esperado("un enlace «p0/…ACC_PERFIL_AUTONOMO…» (los busca importass::enlaces)")
                .encontrado(format!("las marcas ACC_ de la página: {}", if marcas.is_empty() { "ninguna".into() } else { marcas.join(", ") })));
        };
        let p = pagina(&self.red, &url)?;
        importass::leer_perfil(&p.texto).map_err(|e| e.en(fuente(&p.url)))
    }

    /// Tus recibos de autónomo, de todas las páginas del historial. Si no eres autónomo, ninguno (sin preguntar).
    pub fn recibos(&self) -> Result<Vec<Recibo>, Error> {
        let mut recibos = Vec::new();
        if !self.es_autonomo()? {
            return Ok(recibos);
        }
        let mut siguiente = RECIBOS.to_string();
        for _ in 0..20 {
            let p = pagina(&self.red, &siguiente)?;
            let (estos, sig) = importass::leer_recibos(&p.texto).map_err(|e| e.en(fuente(&p.url)))?;
            recibos.extend(estos);
            let Some(sig) = sig else { break };
            siguiente = html::unir(&html::base(&p.url, &p.texto), &sig);
        }
        Ok(recibos)
    }

    /// Tus regularizaciones (el historial, con el detalle de cada año que lo tenga), de la más reciente a la más
    /// antigua. Si no eres autónomo, ninguna (sin preguntar).
    pub fn regularizaciones(&self) -> Result<Vec<Regularizacion>, Error> {
        if !self.es_autonomo()? {
            return Ok(Vec::new());
        }
        let p = pagina(&self.red, REGULARIZACIONES)?;
        let lista = importass::leer_regularizaciones(&p.texto).map_err(|e| e.en(fuente(&p.url)))?;
        let mut detalles = Vec::new();
        for url in self.enlaces("ACC_REGULARIZACION_AUTONOMO") {
            let p = pagina(&self.red, &url)?;
            detalles.push(importass::leer_regularizacion(&p.texto).map_err(|e| e.en(fuente(&p.url)))?);
        }
        Ok(importass::juntar_regularizaciones(lista, detalles))
    }

    /// Todo lo de autónomo junto (sin las nóminas), como se guarda en `fuentes/tgss.json`.
    pub fn autonomo(&self) -> Result<Tgss, Error> {
        if !self.es_autonomo()? {
            return Ok(Tgss { reta: false, consultado: ahora(), ..Default::default() });
        }
        let perfil = self.perfil()?;
        let recibos = self.recibos()?;
        let regularizaciones = self.regularizaciones()?;
        Ok(Tgss {
            reta: true,
            alta: perfil.alta,
            actividades: perfil.actividades,
            mutua: perfil.mutua,
            cotizacion: perfil.cotizacion,
            recibos,
            regularizaciones,
            consultado: ahora(),
            nomina: Default::default(),
        })
    }

    /// Lo cotizado por tus nóminas en esos meses («2025-12»): mes → liquidaciones, y los meses que la Seguridad
    /// Social aún no da (se vuelven a mirar más adelante). Quien la llama apunta `nomina.consultado[mes] = ahora()`
    /// de todos ellos.
    pub fn nominas(&self, meses: &[String]) -> Result<(Meses, Vec<String>), Error> {
        sede::nominas(&self.red, meses)
    }
}

/// De dónde es una página de Importass, para el informe de un fallo (sin la consulta ni el estado del portal).
fn fuente(url: &str) -> String {
    format!("Importass: {}", sin_consulta(url))
}

// ---------------------------------------------------------------- la red

/// Una página: a dónde se llegó, el código y el texto.
#[derive(Debug, Clone)]
struct Pagina {
    pub url: String,
    pub codigo: u16,
    pub texto: String,
}

/// Pide una página (GET, o POST con `campos`, siguiendo las redirecciones dentro de la Seguridad Social). Los fallos,
/// en cristiano.
fn pedir(red: &Cliente, url: &str, campos: Option<&[(&str, &str)]>, referer: Option<&str>) -> Result<Pagina, String> {
    let r = match campos {
        Some(c) => red.post(url, Cuerpo::Formulario(c), referer),
        None => red.get(url, referer),
    }
    .map_err(fallo_de_red)?;
    if r.cuerpo.starts_with(&[0x1f, 0x8b]) {
        return Err("La Seguridad Social ha mandado una página comprimida que no se puede leer.".into());
    }
    if r.codigo >= 500 {
        return Err(format!("La Seguridad Social no está disponible ahora mismo ({} contesta {}): prueba más tarde.", html::servidor(&r.url), r.codigo));
    }
    Ok(Pagina { texto: texto(&r), url: r.url, codigo: r.codigo })
}

/// Si la conexión se corta en IPCE (el que pide el certificado), es que no lo ha aceptado.
fn fallo_de_red(e: String) -> String {
    let en_ipce = e.contains("ipce.seg-social.es");
    if en_ipce && !e.starts_with("No se encuentra") && !e.contains("no contesta") { format!("{NO_ACEPTA} ({e})") } else { e }
}

/// El cuerpo como texto, con el juego de caracteres que diga la cabecera (UTF-8 si no dice).
fn texto(r: &Respuesta) -> String {
    let tipo = r.cabecera("content-type").unwrap_or("").to_ascii_lowercase();
    let juego = tipo.split("charset=").nth(1).unwrap_or("").trim_matches(|c: char| c == '"' || c.is_whitespace());
    if ["iso-8859-1", "iso8859-1", "latin1", "latin-1", "windows-1252", "cp1252"].iter().any(|j| juego.starts_with(j)) {
        r.cuerpo.iter().map(|&b| html::cp1252(b)).collect()
    } else {
        String::from_utf8_lossy(&r.cuerpo).into_owned()
    }
}

/// Manda un formulario (con la página de la que viene como referer).
fn enviar(red: &Cliente, accion: &str, campos: &[(String, String)], desde: &str) -> Result<Pagina, String> {
    let pares: Vec<(&str, &str)> = campos.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pedir(red, accion, Some(&pares), Some(desde))
}

/// Un formulario por mandar: a dónde y sus campos.
type Envio = (String, Vec<(String, String)>);

/// Lo siguiente al iniciar sesión desde esta página: el formulario que hay que mandar (a dónde y qué), o nada si la
/// página no es del inicio de sesión. Si IPCE contesta 403, no ha aceptado el certificado.
fn paso(p: &Pagina, con_login: bool) -> Result<Option<Envio>, String> {
    let host = html::servidor(&p.url);
    if host == "ipce.seg-social.es" && p.codigo == 403 {
        return Err(NO_ACEPTA.into());
    }
    let fs = html::formularios(&p.url, &p.texto);
    let saml = fs.iter().find(|f| f.tiene("SAMLRequest") || f.tiene("SAMLResponse"));
    if con_login && let Some(f) = fs.iter().find(|f| f.tiene("loginFormSubmit")) {
        return Ok(Some((f.accion.clone(), f.campos.clone())));
    }
    if host == "idp.seg-social.es" && p.texto.contains("seleccion=IPCE") && saml.is_none() {
        // La pantalla para elegir cómo identificarte: «DNIe o certificado».
        return Ok(Some((IPCE.into(), fs.last().map(|f| f.campos.clone()).unwrap_or_default())));
    }
    Ok(saml.map(|f| (f.accion.clone(), f.campos.clone())))
}

// ---------------------------------------------------------------- Importass

/// Inicia sesión: Importass → pasarela de la Seguridad Social (PGIS) → «DNIe o certificado» (IPCE, que pide el
/// certificado en la conexión) → vuelta a Importass, mandando los formularios que se mandan solos. Da la página a la
/// que se llega ya dentro.
fn iniciar_sesion(red: &Cliente) -> Result<Pagina, Error> {
    const D: &str = donde!("iniciar_sesion");
    let mut p = pedir(red, IMPORTASS, None, None)?;
    let esperado = "en cada paso, un formulario SAML (SAMLRequest o SAMLResponse), el de «loginFormSubmit» o la elección «DNIe o certificado» (seleccion=IPCE), hasta llegar a Importass con \"authenticated\":\"true\" (ver paso)";
    let mal = |p: &Pagina, msg: String| {
        Error::formato("TGSS_ENTRAR", D, msg)
            .en(format!("el inicio de sesión de la Seguridad Social: {}", sin_consulta(&p.url)))
            .esperado(esperado)
            .encontrado(format!("título «{}»; clases de la página: {}", html::titulo(&p.texto), html::clases(&p.texto, 30).join(", ")))
    };
    for _ in 0..14 {
        if importass::dentro(&p.texto) {
            return Ok(p);
        }
        let Some((accion, campos)) = paso(&p, true)? else {
            let msg = format!("No se ha podido entrar en la Seguridad Social: no se sabe seguir desde {} ({}).", html::servidor(&p.url), html::donde(&p.texto));
            return Err(mal(&p, msg));
        };
        p = enviar(red, &accion, &campos, &p.url)?;
    }
    Err(mal(&p, "No se ha podido entrar en la Seguridad Social: el inicio de sesión no termina.".into()))
}

/// Una página de Importass; si la sesión ha caducado, vuelve a entrar con el certificado.
fn pagina(red: &Cliente, url: &str) -> Result<Pagina, Error> {
    let p = pedir(red, url, None, None)?;
    if importass::dentro(&p.texto) {
        return Ok(p);
    }
    iniciar_sesion(red)?;
    let p = pedir(red, url, None, None)?;
    if importass::dentro(&p.texto) {
        return Ok(p);
    }
    Err(Error::formato("TGSS_SESION", donde!("pagina"), "No se ha podido abrir tu área personal de Importass: la sesión se cierra sola.")
        .en(fuente(&p.url))
        .esperado("una página de Importass con la sesión iniciada (\"authenticated\":\"true\", ver importass::dentro)")
        .encontrado(format!("título «{}»", html::titulo(&p.texto))))
}

// ---------------------------------------------------------------- qué meses mirar

/// Ahora (`fmt::ahora`): cuándo se consultó.
pub use crate::dominio::fmt::ahora;

/// Un instante en UTC («…Z»), sin zona (hora local) o solo la fecha (a las
/// 00:00 UTC).
fn instante(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc));
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, f) {
            return Local.from_local_datetime(&t).earliest().map(|t| t.with_timezone(&Utc));
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map(|t| t.and_utc())
}

/// Qué meses hay que consultar ahora, mes a mes desde `dominio::PRIMER_ANIO`: los que no se han mirado nunca
/// (`consultado`) y los de los últimos 7 meses mirados hace más de 12 horas (también los que la Seguridad Social aún
/// no daba). El mes en curso no: la empresa lo liquida al mes siguiente. Del más reciente al más antiguo y como mucho
/// `MAX_MESES`: lo que no cabe se pide la vez siguiente (por eso se decide mes a mes, no por años: un año que se
/// cortó a medias no se queda con huecos). `hoy` es ahora (`ahora()`); si solo es la fecha («2026-10-21»), se toma
/// su 00:00 UTC.
pub fn meses_a_consultar(d: &Datos, hoy: &str) -> Vec<String> {
    let Some(ahora) = instante(hoy) else { return Vec::new() };
    let mes_hoy = hoy.get(..7).unwrap_or("");
    let Some(anio_hoy) = mes_hoy.get(..4).and_then(|a| a.parse::<i32>().ok()) else { return Vec::new() };
    let consultado = &d.tgss.nomina.consultado;
    let recientes = crate::dominio::fmt::mes_mas(mes_hoy, -RECIENTES);
    let mut out = Vec::new();
    for anio in (crate::dominio::PRIMER_ANIO..=anio_hoy).rev() {
        for m in (1..=12).rev() {
            let ym = format!("{anio}-{m:02}");
            if ym.as_str() >= mes_hoy {
                continue;
            }
            let cuando = consultado.get(&ym).filter(|v| !v.is_empty());
            // Lo que no se entiende no cuenta como viejo.
            let viejo = cuando.and_then(|c| instante(c)).is_some_and(|c| ahora - c > chrono::Duration::hours(CADUCA_HORAS));
            if cuando.is_none() || (ym >= recientes && viejo) {
                out.push(ym);
            }
        }
    }
    out.truncate(MAX_MESES);
    out
}
