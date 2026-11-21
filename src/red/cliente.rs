//! Un navegador mínimo para hablar con Hacienda y la Seguridad Social (y, solo si activas su conector, con tu
//! bróker): identifica con el certificado en la conexión TLS, guarda las cookies de la sesión y sigue las
//! redirecciones a mano, solo dentro de los dominios permitidos (nunca se manda nada a otro sitio). Hay otro,
//! Anthropic (`DOMINIOS_ANTHROPIC`), solo si la persona activa que Claude clasifique los documentos que arrastra
//! (ver `clasificar`); si no, no se usa.
use super::certificado::Certificado;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// Redirecciones seguidas como mucho en una petición.
const SALTOS: usize = 15;
/// Lo más grande que se descarga (un PDF de la Renta ronda los 300 KB).
const MAX_CUERPO: u64 = 30 * 1024 * 1024;

/// Hacienda.
pub const DOMINIOS_AEAT: &[&str] = &["agenciatributaria.gob.es", "agenciatributaria.es", "aeat.es"];
/// La Seguridad Social.
pub const DOMINIOS_TGSS: &[&str] = &["seg-social.es", "seg-social.gob.es"];
/// La API de Claude: solo con la clasificación de documentos activada (`PANEL_CLASIFICAR=api`).
pub const DOMINIOS_ANTHROPIC: &[&str] = &["api.anthropic.com"];
/// DEGIRO: solo su web (la de su plataforma, la misma que usa tu navegador). Solo lo usa su conector
/// (`brokers::degiro`), y solo si lo has activado (`brokers::traer`).
pub const DOMINIOS_DEGIRO: &[&str] = &["trader.degiro.nl"];

/// Una respuesta: la URL a la que se llegó (tras las redirecciones), el código, las cabeceras y el cuerpo.
#[derive(Debug, Clone, Default)]
pub struct Respuesta {
    pub url: String,
    pub codigo: u16,
    pub cabeceras: Vec<(String, String)>,
    pub cuerpo: Vec<u8>,
}

impl Respuesta {
    /// El cuerpo como texto (UTF-8; si no lo es, ISO-8859-1, que es lo que usa Hacienda).
    pub fn texto(&self) -> String {
        match std::str::from_utf8(&self.cuerpo) {
            Ok(s) => s.to_string(),
            Err(_) => self.cuerpo.iter().map(|&b| b as char).collect(),
        }
    }
    pub fn cabecera(&self, nombre: &str) -> Option<&str> {
        self.cabeceras.iter().find(|(k, _)| k.eq_ignore_ascii_case(nombre)).map(|(_, v)| v.as_str())
    }
}

/// Lo que se manda en un POST.
pub enum Cuerpo<'a> {
    /// Formulario (`application/x-www-form-urlencoded`), codificado en UTF-8.
    Formulario(&'a [(&'a str, &'a str)]),
    /// Ya codificado, con su tipo.
    Bytes(&'a [u8], &'a str),
}

/// Una sesión: cookies propias y, si se da, el certificado.
pub struct Cliente {
    agente: ureq::Agent,
    dominios: &'static [&'static str],
    /// Cabeceras que van en todas las peticiones, además de las de siempre (la clave de la API de Claude y su
    /// versión; el `Origin` de DEGIRO).
    cabeceras: Vec<(&'static str, String)>,
}

impl Cliente {
    /// Una sesión nueva con esos dominios permitidos (`DOMINIOS_AEAT`, `DOMINIOS_TGSS`).
    pub fn new(cert: Option<&Certificado>, dominios: &'static [&'static str]) -> Cliente {
        let tls = ureq::tls::TlsConfig::builder().client_cert(cert.map(Certificado::para_tls)).build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(90)))
            .user_agent(UA)
            .build();
        Cliente { agente: config.into(), dominios, cabeceras: Vec::new() }
    }

    /// La misma sesión, mandando además estas cabeceras en cada petición.
    pub fn con_cabeceras(mut self, cabeceras: Vec<(&'static str, String)>) -> Cliente {
        self.cabeceras.extend(cabeceras);
        self
    }

    /// La misma sesión, con una cabecera más en todas las peticiones (`Origin`, como las manda un navegador).
    pub fn con_cabecera(mut self, nombre: &'static str, valor: &'static str) -> Cliente {
        self.cabeceras.push((nombre, valor.to_string()));
        self
    }

    /// Pone una cookie de la sesión para un sitio permitido (la que identifica la sesión, si la web la da en el
    /// cuerpo de la respuesta y no en una cookie).
    pub fn cookie(&self, url: &str, nombre: &str, valor: &str) -> Result<(), String> {
        let mal = |e: &dyn std::fmt::Display| format!("No se puede guardar la cookie de la sesión: {e}");
        if !self.permitida(url) {
            return Err(mal(&"no es un sitio permitido"));
        }
        let uri: ureq::http::Uri = url.parse().map_err(|e| mal(&e))?;
        let c = ureq::Cookie::parse(format!("{nombre}={valor}; Path=/"), &uri).map_err(|e| mal(&e))?;
        self.agente.cookie_jar_lock().insert(c, &uri).map_err(|e| mal(&e))
    }

    /// PUT sin cuerpo, sin seguir redirecciones (cerrar la sesión).
    pub fn put(&self, url: &str) -> Result<Respuesta, String> {
        if !self.permitida(url) {
            return Err("No es un sitio permitido: no se manda nada.".into());
        }
        let mut p = self.agente.put(url);
        for (k, v) in &self.cabeceras {
            p = p.header(*k, v.as_str());
        }
        let r = p.send_empty().map_err(|e| traducir(&e, url))?;
        Ok(Respuesta { url: url.to_string(), codigo: r.status().as_u16(), ..Default::default() })
    }

    /// ¿Es de un dominio permitido (y https)?
    pub fn permitida(&self, url: &str) -> bool {
        let Some(resto) = url.strip_prefix("https://") else { return false };
        let host = resto.split(['/', '?', '#', ':']).next().unwrap_or("").to_lowercase();
        self.dominios.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }

    pub fn get(&self, url: &str, referer: Option<&str>) -> Result<Respuesta, String> {
        self.pedir(url, None, referer)
    }

    pub fn post(&self, url: &str, cuerpo: Cuerpo, referer: Option<&str>) -> Result<Respuesta, String> {
        self.pedir(url, Some(cuerpo), referer)
    }

    /// GET (o POST con `cuerpo`), siguiendo las redirecciones dentro de los dominios permitidos. Tras una
    /// redirección se sigue con GET (como un navegador con 302/303).
    pub fn pedir(&self, url: &str, cuerpo: Option<Cuerpo>, referer: Option<&str>) -> Result<Respuesta, String> {
        let mut url = url.to_string();
        let mut cuerpo = cuerpo;
        let mut referer = referer.map(str::to_string);
        for _ in 0..SALTOS {
            if !self.permitida(&url) {
                let host = url.split('/').nth(2).unwrap_or("?");
                return Err(format!("La web ha redirigido fuera de su dominio ({host}): no se sigue."));
            }
            let r = self.una(&url, cuerpo.take(), referer.as_deref())?;
            if (300..400).contains(&r.codigo)
                && let Some(destino) = r.cabecera("location") {
                    let siguiente = absoluta(&url, destino);
                    referer = Some(url);
                    url = siguiente;
                    continue;
                }
            return Ok(r);
        }
        Err("Demasiadas redirecciones.".into())
    }

    fn una(&self, url: &str, cuerpo: Option<Cuerpo>, referer: Option<&str>) -> Result<Respuesta, String> {
        let error = |e: ureq::Error| traducir(&e, url);
        let r = match cuerpo {
            None => {
                let mut p = self.agente.get(url).header("Accept-Language", "es-ES,es;q=0.9").header("Accept", "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8");
                if let Some(rf) = referer {
                    p = p.header("Referer", rf);
                }
                for (k, v) in &self.cabeceras {
                    p = p.header(*k, v.as_str());
                }
                p.call().map_err(error)?
            }
            Some(c) => {
                let mut p = self.agente.post(url).header("Accept-Language", "es-ES,es;q=0.9");
                if let Some(rf) = referer {
                    p = p.header("Referer", rf);
                }
                for (k, v) in &self.cabeceras {
                    p = p.header(*k, v.as_str());
                }
                match c {
                    Cuerpo::Formulario(campos) => p.send_form(campos.iter().copied()).map_err(error)?,
                    Cuerpo::Bytes(b, tipo) => p.header("Content-Type", tipo).send(b).map_err(error)?,
                }
            }
        };
        let codigo = r.status().as_u16();
        let cabeceras = r.headers().iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())).collect();
        let cuerpo = r.into_body().with_config().limit(MAX_CUERPO).read_to_vec().map_err(|e| traducir(&e, url))?;
        Ok(Respuesta { url: url.to_string(), codigo, cabeceras, cuerpo })
    }
}

/// Los errores de red, en cristiano.
fn traducir(e: &ureq::Error, url: &str) -> String {
    let host = url.split('/').nth(2).unwrap_or(url);
    match e {
        ureq::Error::Timeout(_) => format!("{host} no contesta (se ha agotado el tiempo)."),
        ureq::Error::HostNotFound => format!("No se encuentra {host}: ¿hay conexión a internet?"),
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => format!("No se puede conectar con {host}: ¿hay conexión a internet? ({e})"),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => format!("Fallo en la conexión segura con {host} ({e})."),
        _ => format!("Error hablando con {host}: {e}"),
    }
}

/// Una URL de redirección (absoluta, relativa a la raíz o relativa a la ruta) hecha absoluta.
pub fn absoluta(base: &str, destino: &str) -> String {
    if destino.starts_with("https://") || destino.starts_with("http://") {
        return destino.to_string();
    }
    let origen: String = base.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    if destino.starts_with("//") {
        return format!("https:{destino}");
    }
    if destino.starts_with('/') {
        return format!("{origen}{destino}");
    }
    let sin_consulta = base.split(['?', '#']).next().unwrap_or(base);
    match sin_consulta.rfind('/') {
        Some(i) if i >= origen.len() => format!("{}{destino}", &sin_consulta[..=i]),
        _ => format!("{origen}/{destino}"),
    }
}

/// Codifica un formulario en ISO-8859-1 (como lo espera Hacienda en sus servicios antiguos).
pub fn formulario_latin1(campos: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, (k, v)) in campos.iter().enumerate() {
        if i > 0 {
            out.push(b'&');
        }
        codificar_latin1(k, &mut out);
        out.push(b'=');
        codificar_latin1(v, &mut out);
    }
    out
}

fn codificar_latin1(s: &str, out: &mut Vec<u8>) {
    for c in s.chars() {
        let b = if (c as u32) < 256 { c as u32 as u8 } else { b'?' };
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => out.push(b),
            b' ' => out.push(b'+'),
            _ => out.extend(format!("%{b:02X}").bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirecciones_y_dominios() {
        assert_eq!(absoluta("https://a.es/x/y?z=1", "/b"), "https://a.es/b");
        assert_eq!(absoluta("https://a.es/x/y?z=1", "c"), "https://a.es/x/c");
        assert_eq!(absoluta("https://a.es", "c"), "https://a.es/c");
        assert_eq!(absoluta("https://a.es/x", "https://b.es/"), "https://b.es/");
        let c = Cliente::new(None, DOMINIOS_TGSS);
        assert!(c.permitida("https://idp.seg-social.es/PGIS/Login"));
        assert!(c.permitida("https://portal.seg-social.gob.es/wps"));
        assert!(!c.permitida("http://portal.seg-social.gob.es/wps"));
        assert!(!c.permitida("https://seg-social.es.malo.com/"));
        assert!(!c.permitida("https://www1.agenciatributaria.gob.es/"));
        let c = Cliente::new(None, DOMINIOS_ANTHROPIC);
        assert!(c.permitida("https://api.anthropic.com/v1/messages"));
        assert!(!c.permitida("https://anthropic.com.malo.es/v1/messages"));
        // DEGIRO: solo su plataforma, ni otro subdominio suyo.
        let d = Cliente::new(None, DOMINIOS_DEGIRO);
        assert!(d.permitida("https://trader.degiro.nl/login/secure/login"));
        assert!(!d.permitida("https://www.degiro.nl/") && !d.permitida("https://charting.vwdservices.com/") && !d.permitida("https://trader.degiro.nl.malo.com/"));
        assert!(d.cookie("https://trader.degiro.nl/", "JSESSIONID", "x").is_ok() && d.cookie("https://malo.com/", "JSESSIONID", "x").is_err());
    }

    #[test]
    fn formulario_en_latin1() {
        assert_eq!(formulario_latin1(&[("ANR", "LÓPEZ PEÑA"), ("MOD", "303")]), b"ANR=L%D3PEZ+PE%D1A&MOD=303".to_vec());
    }
}
