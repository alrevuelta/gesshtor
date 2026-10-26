//! Traer lo último de Hacienda y la Seguridad Social con el certificado: lo presentado cada año, los PDF en
//! `justificantes/` y lo de la Seguridad Social (con las cotizaciones de las nóminas mes a mes). Corre en otro hilo
//! (`sincronizar`) y lo traído se aplica en el de la app (`aplicar`), que es el único que escribe `datos.json`.
use crate::hacienda::consulta::{self, Hacienda};
use crate::hacienda::lectura;
use crate::red::certificado::{self, Certificado};
use crate::tgss::{self, SeguridadSocial};
use crate::dominio::hacienda::Consulta;
use crate::dominio::tgss::{Meses, Tgss};
use crate::dominio::Datos;
use crate::dominio::fmt::anio_actual;
use crate::dominio::PRIMER_ANIO;
use std::path::Path;

/// Meses de nómina por petición a la Seguridad Social.
const MESES_POR_VEZ: usize = 12;

/// Lo traído en una sincronización.
#[derive(Debug, Default)]
pub struct Resultado {
    /// Lo presentado de cada año consultado.
    pub hacienda: Vec<(i32, Consulta)>,
    /// Los datos de autónomo de la Seguridad Social (sin las nóminas).
    pub tgss: Option<Tgss>,
    /// Las cotizaciones de las nóminas: mes → liquidaciones; y los meses que aún no da.
    pub nomina: Meses,
    pub nomina_aun_no: Vec<String>,
    /// PDF nuevos descargados.
    pub pdfs: usize,
    pub errores: Vec<String>,
    /// Cuándo (ISO, para `consultado`).
    pub cuando: String,
}

impl Resultado {
    /// «Hacienda: 6 años · Seguridad Social: sí · 4 PDF nuevos»
    pub fn resumen(&self) -> String {
        let mut v = Vec::new();
        if !self.hacienda.is_empty() {
            v.push(format!("Hacienda: {} {}", self.hacienda.len(), if self.hacienda.len() == 1 { "año" } else { "años" }));
        }
        if self.tgss.is_some() {
            v.push("Seguridad Social: al día".into());
        }
        if !self.nomina.is_empty() {
            v.push(format!("{} meses de nómina", self.nomina.len()));
        }
        if self.pdfs > 0 {
            v.push(format!("{} PDF nuevos", self.pdfs));
        }
        if v.is_empty() {
            v.push("nada nuevo".into());
        }
        v.join(" · ")
    }
}

/// Los años que se consultan en Hacienda: el actual y el anterior siempre (pueden cambiar); los demás, desde
/// `PRIMER_ANIO`, solo si nunca se consultaron (o si `todo`).
pub fn anios_hacienda(d: &Datos, todo: bool) -> Vec<i32> {
    let hoy = anio_actual();
    (PRIMER_ANIO..=hoy).rev().filter(|&y| todo || y >= hoy - 1 || !d.hacienda.contains_key(&y)).collect()
}

/// Trae lo de Hacienda y la Seguridad Social, las dos a la vez (son servidores distintos y no dependen la una de
/// la otra). `paso` dice por dónde va cada una («Hacienda: … · Seguridad Social: …»). Nunca falla del todo: lo que
/// no se puede traer va a `errores` y se sigue con lo demás.
pub fn sincronizar(dir: &Path, d: &Datos, cert: &Certificado, todo: bool, paso: &mut dyn FnMut(String)) -> Resultado {
    let cuando = crate::dominio::fmt::ahora();
    let (tx, rx) = std::sync::mpsc::channel::<(usize, String)>();
    let (h, ss) = std::thread::scope(|s| {
        // Cada una dice por dónde va con su número; al acabar, con un paso vacío.
        let avisa = |i: usize| {
            let tx = tx.clone();
            move |p: String| {
                let _ = tx.send((i, p));
            }
        };
        let (mut ph, mut ps) = (avisa(0), avisa(1));
        let h = s.spawn(move || {
            let r = hacienda(dir, d, cert, todo, &mut ph);
            ph(String::new());
            r
        });
        let ss = s.spawn(move || {
            let r = seguridad_social(d, cert, &mut ps);
            ps(String::new());
            r
        });
        drop(tx);
        let mut ahora = [String::new(), String::new()];
        for (i, p) in rx {
            ahora[i] = p;
            let v: Vec<&str> = ahora.iter().map(String::as_str).filter(|p| !p.is_empty()).collect();
            if !v.is_empty() {
                paso(v.join(" · "));
            }
        }
        let cortado = |quien: &str| Resultado { errores: vec![format!("{quien}: se ha cortado a medias.")], ..Default::default() };
        (h.join().unwrap_or_else(|_| cortado("Hacienda")), ss.join().unwrap_or_else(|_| cortado("Seguridad Social")))
    });
    let mut errores = h.errores;
    errores.extend(ss.errores);
    Resultado { hacienda: h.hacienda, pdfs: h.pdfs, tgss: ss.tgss, nomina: ss.nomina, nomina_aun_no: ss.nomina_aun_no, errores, cuando }
}

/// Hacienda: lo presentado y los PDF que falten.
fn hacienda(dir: &Path, d: &Datos, cert: &Certificado, todo: bool, paso: &mut dyn FnMut(String)) -> Resultado {
    let mut r = Resultado::default();
    let aeat = Hacienda::new(cert);
    let modelos: Vec<&str> = consulta::MODELOS.iter().chain(consulta::CENSALES).copied().collect();
    let anios = anios_hacienda(d, todo);
    paso("Hacienda: lo presentado…".into());
    let anios = match aeat.presentado_en(&anios, &modelos, &mut |h, t| paso(format!("Hacienda: lo presentado ({h} de {t} consultas)…"))) {
        Ok(v) => v,
        Err(e) => {
            r.errores.push(format!("Hacienda: {e}"));
            return r;
        }
    };
    let mut faltan = Vec::new();
    for (y, consulta) in anios {
        match consulta {
            Ok(v) => {
                for (modelo, periodo, csv) in declaraciones(&v) {
                    if !dir.join("justificantes").join(lectura::nombre_pdf(y, &periodo, &modelo, &csv)).is_file() {
                        faltan.push((modelo, y, periodo, csv));
                    }
                }
                r.hacienda.push((y, v));
            }
            Err(e) => r.errores.push(format!("Hacienda ({y}): {e}")),
        }
    }
    let bajados = aeat.pdfs(dir, &faltan, &mut |h, t| paso(format!("Hacienda: los PDF nuevos ({h} de {t})…")));
    for ((modelo, y, periodo, _), b) in faltan.iter().zip(bajados) {
        match b {
            Some(Ok(_)) => r.pdfs += 1,
            Some(Err(e)) => r.errores.push(format!("PDF del {modelo} {periodo} de {y}: {e}")),
            None => {}
        }
    }
    r
}

/// La Seguridad Social: los datos de autónomo y las nóminas. Todo en una sola sesión y en orden, como el navegador:
/// la Sede recuerda en qué paso va cada servicio.
fn seguridad_social(d: &Datos, cert: &Certificado, paso: &mut dyn FnMut(String)) -> Resultado {
    let mut r = Resultado::default();
    paso("Seguridad Social: entrando con tu certificado…".into());
    let ss = match SeguridadSocial::entrar(cert) {
        Ok(ss) => ss,
        Err(e) => {
            r.errores.push(format!("Seguridad Social: {e}"));
            return r;
        }
    };
    paso("Seguridad Social: tus datos de autónomo…".into());
    match ss.autonomo() {
        Ok(v) => r.tgss = Some(v),
        Err(e) => r.errores.push(format!("Seguridad Social: {e}")),
    }
    let meses = tgss::meses_a_consultar(d, &tgss::ahora());
    for (i, lote) in meses.chunks(MESES_POR_VEZ).enumerate() {
        paso(format!("Seguridad Social: tus nóminas ({} de {} meses)…", (i * MESES_POR_VEZ + lote.len()), meses.len()));
        match ss.nominas(lote) {
            Ok((m, aun_no)) => {
                r.nomina.extend(m);
                r.nomina_aun_no.extend(aun_no);
            }
            Err(e) => {
                r.errores.push(format!("Seguridad Social (nóminas): {e}"));
                break;
            }
        }
    }
    r
}

/// (modelo, periodo, csv) de cada declaración de un año traído de Hacienda.
fn declaraciones(h: &Consulta) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (m, e) in &h.modelos {
        for x in &e.decl {
            if !x.csv.is_empty() && !x.periodo.is_empty() && x.csv.chars().all(|c| c.is_ascii_alphanumeric()) {
                out.push((m.clone(), x.periodo.clone(), x.csv.clone()));
            }
        }
    }
    out
}

/// Guarda en los datos lo traído (en el hilo de la app; después hay que guardar `datos.json`).
pub fn aplicar(d: &mut Datos, r: &Resultado) {
    // Lo nuevo sobre lo que había (un modelo que falla conserva lo de antes, con su error).
    for (y, h) in &r.hacienda {
        let mezcla = Consulta::mezclar(d.hacienda.get(y), h.clone());
        d.hacienda.insert(*y, mezcla);
    }
    if let Some(nuevo) = &r.tgss {
        let nomina = std::mem::take(&mut d.tgss.nomina);
        d.tgss = Tgss { nomina, ..nuevo.clone() };
    }
    let ahora = tgss::ahora();
    let n = &mut d.tgss.nomina;
    for (ym, l) in &r.nomina {
        n.meses.insert(ym.clone(), l.clone());
        n.consultado.insert(ym.clone(), ahora.clone());
    }
    // Los que la Seguridad Social aún no da: se vuelven a mirar más adelante.
    for ym in &r.nomina_aun_no {
        n.consultado.insert(ym.clone(), ahora.clone());
    }
}

/// Pone en el perfil lo que dice el certificado si falta (NIF, nombre y apellidos).
pub fn perfil_desde(d: &mut Datos, cert: &Certificado) -> bool {
    let mut cambio = false;
    if d.perfil.nif.trim().is_empty() && !cert.nif.is_empty() {
        d.perfil.nif = cert.nif.clone();
        cambio = true;
    }
    let pila = certificado::titulo(&cert.nombre_pila);
    let apellidos = certificado::titulo(&cert.apellidos);
    if !pila.is_empty() && d.perfil.nombre_pila.trim().is_empty() {
        d.perfil.nombre_pila = pila.clone();
        cambio = true;
    }
    if !apellidos.is_empty() && d.perfil.apellidos.trim().is_empty() {
        d.perfil.apellidos = apellidos.clone();
        cambio = true;
    }
    if d.perfil.nombre.chars().count() <= 2 && !pila.is_empty() {
        d.perfil.nombre = format!("{pila} {apellidos}").trim().to_string();
        cambio = true;
    }
    cambio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anios_que_se_consultan() {
        let mut d = Datos::default();
        let hoy = anio_actual();
        assert_eq!(anios_hacienda(&d, false).len() as i32, hoy - PRIMER_ANIO + 1, "sin nada consultado: todos");
        for y in PRIMER_ANIO..=hoy {
            d.hacienda.insert(y, Consulta::default());
        }
        assert_eq!(anios_hacienda(&d, false), vec![hoy, hoy - 1], "ya consultados: solo los dos últimos");
        assert_eq!(anios_hacienda(&d, true).len() as i32, hoy - PRIMER_ANIO + 1);
    }

    #[test]
    fn aplica_lo_traido_sin_perder_lo_que_habia() {
        use crate::dominio::hacienda::{Decl, Modelo};
        let mut d = Datos::default();
        d.tgss.reta = true;
        d.tgss.nomina.meses.insert("2025-01".into(), Vec::new());
        d.tgss.nomina.consultado.insert("2025-01".into(), "x".into());
        let decl = Decl { modelo: "303".into(), ejercicio: 2025, periodo: "1T".into(), csv: "ABC123".into(), ..Default::default() };
        let h = Consulta { nif: "00000000T".into(), fecha: "2026-10-21T10:00:00".into(), modelos: [("303".to_string(), Modelo { fecha: String::new(), decl: vec![decl] })].into(), ..Default::default() };
        let mut ss = Tgss { reta: true, ..Default::default() };
        ss.recibos.push(crate::dominio::tgss::Recibo { periodo: "2026-10".into(), importe: 315.0, ..Default::default() });
        let r = Resultado {
            hacienda: vec![(2025, h)],
            tgss: Some(ss),
            nomina: [("2025-02".to_string(), Vec::new())].into(),
            nomina_aun_no: vec!["2026-09".into()],
            cuando: "2026-10-21T10:00:00".into(),
            ..Default::default()
        };
        assert_eq!(declaraciones(&r.hacienda[0].1), vec![("303".to_string(), "1T".to_string(), "ABC123".to_string())]);
        aplicar(&mut d, &r);
        assert_eq!(crate::hacienda::declaraciones(&d, 2025).len(), 1);
        assert_eq!(d.tgss.recibos.len(), 1);
        let n = &d.tgss.nomina;
        assert!(n.meses.contains_key("2025-01") && n.meses.contains_key("2025-02") && !n.meses.contains_key("2026-09"));
        assert!(n.consultado["2026-09"].starts_with("20"));
        assert!(r.resumen().contains("Hacienda: 1 año"));
    }

    #[test]
    fn perfil_del_certificado() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pruebas/prueba.p12");
        let cert = Certificado::abrir(&p, "prueba").unwrap();
        let mut d = Datos::default();
        assert!(perfil_desde(&mut d, &cert));
        assert_eq!((d.perfil.nif.as_str(), d.perfil.nombre.as_str()), ("00000000T", "Ana Prueba"));
        assert!(!perfil_desde(&mut d, &cert), "la segunda vez no cambia nada");
    }
}
