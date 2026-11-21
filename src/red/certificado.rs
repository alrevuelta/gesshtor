//! El certificado digital de la persona (`.p12` / `.pfx`, el que da la FNMT): quién es, cuándo caduca y lo
//! necesario para identificarse en Hacienda y en la Seguridad Social. La contraseña solo se usa al abrirlo y
//! no se guarda; la clave privada se queda en memoria mientras la app está abierta.
use std::path::{Path, PathBuf};

/// La variable del entorno de la que se puede tomar la contraseña del certificado. La app la quita de su entorno nada
/// más leerla (`main::abrir_certificado`), y los programas que lanza no la reciben.
pub const CLAVE_EN_EL_ENTORNO: &str = "PANEL_CLAVE_CERTIFICADO";

#[derive(Clone)]
pub struct Certificado {
    pub ruta: PathBuf,
    /// Nombre de pila y apellidos, como vienen (en mayúsculas).
    pub nombre_pila: String,
    pub apellidos: String,
    /// El nombre común completo («APELLIDO … - 12345678Z»).
    pub nombre_comun: String,
    pub nif: String,
    /// Fecha de caducidad (YYYY-MM-DD).
    pub caduca: String,
    /// Quién lo emitió («AC FNMT Usuarios»…).
    pub emisor: String,
    /// La cadena (el certificado primero) y la clave privada, en DER (PKCS#8).
    cadena: Vec<Vec<u8>>,
    clave: Vec<u8>,
}

impl std::fmt::Debug for Certificado {
    /// Sin la clave privada.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Certificado")
            .field("ruta", &self.ruta)
            .field("nif", &self.nif)
            .field("caduca", &self.caduca)
            .finish_non_exhaustive()
    }
}

impl Certificado {
    /// Abre un `.p12` / `.pfx` con su contraseña. Sirven los modernos (AES) y los antiguos (3DES, RC2).
    pub fn abrir(ruta: &Path, clave: &str) -> Result<Certificado, String> {
        let bytes =
            std::fs::read(ruta).map_err(|e| format!("No se puede leer {}: {e}", ruta.display()))?;
        Self::desde_bytes(&bytes, clave, ruta)
    }

    pub fn desde_bytes(bytes: &[u8], clave: &str, ruta: &Path) -> Result<Certificado, String> {
        use p12_keystore::error::Error;
        use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
        let ks =
            KeyStore::from_pkcs12(bytes, clave, Pkcs12ImportPolicy::Relaxed).map_err(
                |e| match e {
                    Error::MacError(_) | Error::UnpadError | Error::Pkcs5Error(_) => {
                        "La contraseña del certificado no es correcta.".to_string()
                    }
                    Error::UnsupportedEncryptionScheme | Error::UnsupportedMacAlgorithm => {
                        format!("Este certificado usa un cifrado que la app no conoce ({e}).")
                    }
                    _ => format!("No parece un certificado .p12 / .pfx ({e})."),
                },
            )?;
        let (_, cadena) = ks.private_key_chain().ok_or(
            "El fichero no tiene la clave privada: exporta el certificado «con la clave privada».",
        )?;
        let certs: Vec<Vec<u8>> = cadena.certs().iter().map(|c| c.as_der().to_vec()).collect();
        let primero = certs.first().ok_or("El certificado viene vacío.")?;
        let (_, x) = x509_parser::parse_x509_certificate(primero)
            .map_err(|e| format!("No se puede leer el certificado: {e}"))?;
        let campo = |oid: &x509_parser::oid_registry::Oid| -> String {
            x.subject()
                .iter_by_oid(oid)
                .next()
                .and_then(|a| a.as_str().ok())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        use x509_parser::oid_registry::{
            OID_X509_COMMON_NAME, OID_X509_GIVEN_NAME, OID_X509_SERIALNUMBER, OID_X509_SURNAME,
        };
        let nombre_comun = campo(&OID_X509_COMMON_NAME);
        let nif = nif_de(&campo(&OID_X509_SERIALNUMBER), &nombre_comun);
        let caduca = x.validity().not_after.to_datetime();
        let caduca = format!(
            "{:04}-{:02}-{:02}",
            caduca.year(),
            u8::from(caduca.month()),
            caduca.day()
        );
        let emisor = x
            .issuer()
            .iter_by_oid(&OID_X509_COMMON_NAME)
            .next()
            .and_then(|a| a.as_str().ok())
            .unwrap_or("")
            .to_string();
        Ok(Certificado {
            ruta: ruta.to_path_buf(),
            nombre_pila: campo(&OID_X509_GIVEN_NAME),
            apellidos: campo(&OID_X509_SURNAME),
            nombre_comun,
            nif,
            caduca,
            emisor,
            cadena: certs,
            clave: cadena.key().as_der().to_vec(),
        })
    }

    /// Para la conexión TLS: la cadena y la clave privada.
    pub fn para_tls(&self) -> ureq::tls::ClientCert {
        use ureq::tls::{Certificate, ClientCert, PrivateKey};
        let cadena: Vec<Certificate<'static>> = self
            .cadena
            .iter()
            .map(|c| Certificate::from_der(c).to_owned())
            .collect();
        // ureq no deja decir el tipo de una clave en DER: se le da en PEM (PKCS#8), que sí reconoce.
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            en_lineas_de_64(&self.clave)
        );
        let clave =
            PrivateKey::from_pem(pem.as_bytes()).expect("una clave PKCS#8 en PEM siempre se lee");
        ClientCert::new_with_certs(&cadena, clave)
    }

    /// «Ana» (el nombre de pila, con mayúscula inicial), o lo que se pueda.
    pub fn saludo(&self) -> String {
        let pila = if self.nombre_pila.is_empty() {
            self.nombre_comun.split_whitespace().last().unwrap_or("")
        } else {
            self.nombre_pila.split_whitespace().next().unwrap_or("")
        };
        titulo(pila)
    }

    /// «Ana García López»
    pub fn nombre_completo(&self) -> String {
        if self.nombre_pila.is_empty() {
            return titulo(
                self.nombre_comun
                    .split(" - ")
                    .next()
                    .unwrap_or(&self.nombre_comun),
            );
        }
        format!("{} {}", titulo(&self.nombre_pila), titulo(&self.apellidos))
    }

    /// Para las pruebas: uno sin fichero ni clave (no sirve para conectarse), con ese NIF y ese nombre (en
    /// mayúsculas, como vienen).
    #[cfg(test)]
    pub fn de_prueba(nif: &str, nombre_pila: &str, apellidos: &str) -> Certificado {
        Certificado {
            ruta: PathBuf::new(),
            nombre_pila: nombre_pila.into(),
            apellidos: apellidos.into(),
            nombre_comun: format!("{apellidos} {nombre_pila} - {nif}"),
            nif: nif.into(),
            caduca: "2099-12-31".into(),
            emisor: "Pruebas".into(),
            cadena: Vec::new(),
            clave: Vec::new(),
        }
    }

    /// Días que le quedan (negativo si ya caducó).
    pub fn dias_para_caducar(&self) -> i64 {
        crate::dominio::fmt::dias_entre(&crate::dominio::fmt::hoy(), &self.caduca)
    }
}

/// Base64 en líneas de 64 (para el PEM).
fn en_lineas_de_64(b: &[u8]) -> String {
    let b64 = crate::dominio::fmt::base64(b);
    b64.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).expect("base64 es ASCII")).collect::<Vec<_>>().join("\n")
}

/// El NIF del certificado: del «serialNumber» («IDCES-12345678Z») o, si no, del final del nombre común.
fn nif_de(serie: &str, nombre_comun: &str) -> String {
    let s = serie.rsplit('-').next().unwrap_or(serie).trim();
    if s.len() >= 8 && s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return s.to_uppercase();
    }
    nombre_comun
        .rsplit(" - ")
        .next()
        .map(|x| x.trim().to_uppercase())
        .filter(|x| x.len() >= 8 && x.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_default()
}

/// «MARÍA JOSÉ» → «María José»
pub fn titulo(s: &str) -> String {
    s.split_whitespace()
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nifs_y_nombres() {
        assert_eq!(nif_de("IDCES-12345678Z", ""), "12345678Z");
        assert_eq!(nif_de("", "PEREZ GOMEZ ANA - 12345678Z"), "12345678Z");
        assert_eq!(titulo("MARÍA JOSÉ"), "María José");
        assert_eq!(titulo("ÁNGEL"), "Ángel");
        let pem = en_lineas_de_64(&[0; 100]);
        assert_eq!(pem.lines().map(str::len).collect::<Vec<_>>(), [64, 64, 8]);
    }

    #[test]
    fn saludo_y_nombre() {
        let c = Certificado::de_prueba("00000000T", "ANA", "PRUEBA");
        assert_eq!((c.saludo().as_str(), c.nombre_completo().as_str()), ("Ana", "Ana Prueba"));
        // Sin nombre de pila ni apellidos, del nombre común.
        let c = Certificado { nombre_pila: String::new(), apellidos: String::new(), ..c };
        assert_eq!((c.saludo().as_str(), c.nombre_completo().as_str()), ("00000000t", "Prueba Ana"));
        assert!(Certificado::desde_bytes(b"esto no es un certificado", "x", Path::new("x.p12")).unwrap_err().contains(".p12"));
    }
}
