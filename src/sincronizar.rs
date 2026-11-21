//! Traer lo último de Hacienda y la Seguridad Social con el certificado: lo presentado cada año, los PDF en
//! `justificantes/` y lo de la Seguridad Social (con las cotizaciones de las nóminas mes a mes). Corre en otro hilo
//! (`sincronizar`) y lo traído se aplica en el de la app (`aplicar`), que es el único que lo escribe (en `fuentes/`).
//! Y cuál es la carpeta de la persona del certificado (`carpeta_de`).
use crate::hacienda::consulta::{self, Hacienda};
use crate::hacienda::lectura;
use crate::hacienda::modelo_720::Bloque;
use crate::red::certificado::{self, Certificado};
use crate::tgss::{self, SeguridadSocial};
use crate::dominio::fuente::Error;
use crate::dominio::hacienda::Consulta;
use crate::dominio::tgss::{Meses, Tgss};
use crate::dominio::{almacen, Datos};
use crate::dominio::fmt::anio_actual;
use crate::dominio::PRIMER_ANIO;
use std::path::{Path, PathBuf};

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
    /// Los años que no se han podido consultar en Hacienda, con el motivo.
    pub error_hacienda: Vec<(i32, String)>,
    /// Por qué no se ha podido traer lo de la Seguridad Social (todo o una parte), con su informe.
    pub error_tgss: Option<Error>,
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

/// Los años de antes de `PRIMER_ANIO` en los que se mira solo el 720, para saber si ya lo presentaste (y entonces
/// solo vuelve a tocar si sube o dejas de tener algo): desde 2012, el primero que se presentó («será de aplicación,
/// por primera vez, para la presentación de la declaración informativa correspondiente al ejercicio 2012», Orden
/// HAP/72/2013, disposición final única, BOE-A-2013-954). El 721 empieza en 2023 (`Bloque::desde`). Una vez, como
/// los demás años (o con `todo`).
pub fn anios_720(d: &Datos, todo: bool) -> Vec<i32> {
    (Bloque::Valores.desde()..PRIMER_ANIO).rev().filter(|y| todo || !d.hacienda.contains_key(y)).collect()
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
    let (hacienda, error_hacienda, pdfs) = (h.hacienda, h.error_hacienda, h.pdfs);
    Resultado { hacienda, error_hacienda, pdfs, tgss: ss.tgss, nomina: ss.nomina, nomina_aun_no: ss.nomina_aun_no, error_tgss: ss.error_tgss, errores, cuando }
}

/// Hacienda: lo presentado y los PDF que falten.
fn hacienda(dir: &Path, d: &Datos, cert: &Certificado, todo: bool, paso: &mut dyn FnMut(String)) -> Resultado {
    let mut r = Resultado::default();
    let aeat = Hacienda::new(cert);
    let modelos: Vec<&str> = consulta::MODELOS.iter().chain(consulta::CENSALES).copied().collect();
    let anios = anios_hacienda(d, todo);
    paso("Hacienda: lo presentado…".into());
    let mut anios = match aeat.presentado_en(&anios, &modelos, &mut |h, t| paso(format!("Hacienda: lo presentado ({h} de {t} consultas)…"))) {
        Ok(v) => v,
        Err(e) => {
            r.error_hacienda = anios.iter().map(|&y| (y, e.clone())).collect();
            r.errores.push(format!("Hacienda: {e}"));
            return r;
        }
    };
    let viejos = anios_720(d, todo);
    if !viejos.is_empty() {
        match aeat.presentado_en(&viejos, &["720"], &mut |h, t| paso(format!("Hacienda: tus 720 de antes de {PRIMER_ANIO} ({h} de {t})…"))) {
            Ok(v) => anios.extend(v),
            Err(e) => {
                r.errores.push(format!("Hacienda (el 720 de antes de {PRIMER_ANIO}): {e}"));
                r.error_hacienda.extend(viejos.iter().map(|&y| (y, e.clone())));
            }
        }
    }
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
            Err(e) => {
                r.errores.push(format!("Hacienda ({y}): {e}"));
                r.error_hacienda.push((y, e));
            }
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
    let ss = match SeguridadSocial::entrar(cert).map_err(sin_respuesta) {
        Ok(ss) => ss,
        Err(e) => {
            r.errores.push(format!("Seguridad Social: {}", e.mensaje));
            r.error_tgss = Some(e);
            return r;
        }
    };
    paso("Seguridad Social: tus datos de autónomo…".into());
    // Quien fue autónomo lo sigue siendo para la Seguridad Social (lo dice su área personal): si ahora no, es que la
    // página ha cambiado, y no se borra lo que se sabe de su cuota.
    let autonomo = ss.autonomo().map_err(sin_respuesta).and_then(|v| if d.tgss.reta && !v.reta { Err(ya_no_autonomo()) } else { Ok(v) });
    match autonomo {
        Ok(v) => r.tgss = Some(v),
        Err(e) => {
            r.errores.push(format!("Seguridad Social: {}", e.mensaje));
            r.error_tgss = Some(e);
        }
    }
    let meses = tgss::meses_a_consultar(d, &tgss::ahora());
    for (i, lote) in meses.chunks(MESES_POR_VEZ).enumerate() {
        paso(format!("Seguridad Social: tus nóminas ({} de {} meses)…", (i * MESES_POR_VEZ + lote.len()), meses.len()));
        match ss.nominas(lote).map_err(sin_respuesta) {
            Ok((m, aun_no)) => {
                r.nomina.extend(m);
                r.nomina_aun_no.extend(aun_no);
            }
            Err(e) => {
                r.errores.push(format!("Seguridad Social (nóminas): {}", e.mensaje));
                // Un solo fallo por fuente: si ya había uno, este se dice en él.
                r.error_tgss = Some(match r.error_tgss.take() {
                    Some(antes) => Error { mensaje: format!("{} (Y tus nóminas: {})", antes.mensaje, e.mensaje), ..antes },
                    None => Error { mensaje: format!("Tus nóminas: {}", e.mensaje), ..e },
                });
                break;
            }
        }
    }
    r
}

/// Un fallo sin código es que la Seguridad Social no ha contestado (o no ha aceptado tu certificado): no es de
/// formato, pero también se ve y se puede copiar.
fn sin_respuesta(e: Error) -> Error {
    if !e.codigo.is_empty() {
        return e;
    }
    Error { codigo: "TGSS_CONEXION".into(), donde: "src/tgss/mod.rs · pedir (red/cliente.rs)".into(), ..e.en("la Seguridad Social") }
}

/// El fallo de quien era autónomo y ahora la Seguridad Social no lo dice.
fn ya_no_autonomo() -> Error {
    Error::formato("TGSS_YA_NO_AUTONOMO", crate::donde!("seguridad_social"), "La Seguridad Social ya no dice que seas o hayas sido autónomo, y antes sí: no lo cambio, por si su página ha cambiado.")
        .en("Importass: tu área personal")
        .esperado("la marca «ColectivoRETA» en tu área personal, como la última vez (ver tgss::importass::autonomo)")
        .encontrado("la marca de otro colectivo")
        .reproducir("S en la app (o gesshtor --certificado <tu .p12> --sincronizar)")
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

/// Guarda lo traído (en el hilo de la app): en los datos y cada cosa en su fichero de `fuentes/`. Lo que ha fallado
/// se apunta y no pisa lo que había. Si no se puede guardar algo, se dice en `r.errores`.
pub fn aplicar(dir: &Path, d: &mut Datos, r: &mut Resultado) {
    let mut guardado = Vec::new();
    // Lo nuevo sobre lo que había (un modelo que falla conserva lo de antes, con su error).
    for (y, h) in &r.hacienda {
        let mezcla = Consulta::mezclar(d.hacienda.get(y), h.clone());
        guardado.push(almacen::guardar_consulta(dir, *y, &r.cuando, Some(&mezcla), None));
        d.hacienda.insert(*y, mezcla);
        d.fallos.remove(&almacen::consulta(*y));
    }
    for (y, e) in &r.error_hacienda {
        let e = Error::formato("AEAT_CONSULTA", "src/hacienda/consulta.rs · presentado_en", e.as_str()).en(format!("Hacienda: lo presentado en {y}"));
        guardado.push(almacen::guardar_consulta(dir, *y, &r.cuando, None, Some(&e)));
        d.fallos.insert(almacen::consulta(*y), e);
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
    let traido = r.tgss.is_some() || !r.nomina.is_empty() || !r.nomina_aun_no.is_empty();
    if traido || r.error_tgss.is_some() {
        guardado.push(almacen::guardar_tgss(dir, &r.cuando, traido.then_some(&d.tgss), r.error_tgss.as_ref()));
        match &r.error_tgss {
            Some(e) => d.fallos.insert(almacen::TGSS.into(), e.clone()),
            None => d.fallos.remove(almacen::TGSS),
        };
    }
    if let Some(e) = guardado.into_iter().find_map(Result::err) {
        r.errores.push(format!("No se ha podido guardar lo traído en {}: {e}", almacen::ruta_fuentes(dir).display()));
    }
    // Los recibos de autónomo, entre los gastos.
    crate::dominio::gasto::con_recibos(d);
}

/// La carpeta de la persona del certificado: la que tenga su NIF; si no hay ninguna, una nueva (en una carpeta
/// de varias personas) o la carpeta dada si aún no es de nadie. Si alguna no se puede abrir, podría ser la suya: no
/// se sigue (y nunca se crea una encima de un `datos.json` que ya está).
pub fn carpeta_de(dir: &Path, cert: &Certificado) -> Result<PathBuf, String> {
    let nif_de = |p: &Path| {
        almacen::cargar(p).map(|d| d.perfil.nif.trim().to_uppercase()).map_err(|e| {
            format!("No puedo abrir {}: {e}. Si es de una versión más nueva de gesshtor, actualiza la app; si está estropeado, en su carpeta copias/ están las copias de antes.", almacen::ruta_datos(p).display())
        })
    };
    let mut fallo = None;
    for p in almacen::perfiles(dir) {
        match nif_de(&p) {
            Ok(n) if n == cert.nif => return Ok(p),
            Ok(n) if n.is_empty() && p == dir => return Ok(p),
            Ok(_) => {}
            Err(e) => fallo = fallo.or(Some(e)),
        }
    }
    if let Some(e) = fallo {
        return Err(e);
    }
    if almacen::ruta_datos(dir).is_file() {
        return Err(format!("{} es de otra persona (otro NIF). Usa la carpeta de esta persona, o una carpeta nueva.", dir.display()));
    }
    // Persona nueva: su carpeta, con su nombre.
    let nombre: String = cert.nombre_completo().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let nueva = dir.join(nombre.trim_matches('-'));
    if almacen::ruta_datos(&nueva).exists() {
        return Err(format!("{} ya tiene los datos de otra persona (otro NIF, o ninguno): no los piso. Mira de quién son, o abre la app con una carpeta para esta persona.", nueva.display()));
    }
    std::fs::create_dir_all(&nueva).map_err(|e| format!("No se puede crear {}: {e}", nueva.display()))?;
    let mut d = Datos::default();
    perfil_desde(&mut d, cert);
    almacen::guardar(&nueva, &d, &mut false).map_err(|e| format!("No se puede crear {}: {e}", nueva.display()))?;
    eprintln!("Carpeta nueva para {}: {}", cert.nombre_completo(), nueva.display());
    Ok(nueva)
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

    /// Una carpeta de varias personas en la que la de Ana no se puede abrir (de una versión que la app no conoce): no
    /// se toma por una persona nueva ni se escribe encima. Y nunca se crea encima de un `datos.json` que ya está.
    #[test]
    fn la_carpeta_de_alguien_no_se_pisa() {
        let raiz = std::env::temp_dir().join(format!("gesshtor-carpeta-de-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        let ana = raiz.join("ana-prueba");
        std::fs::create_dir_all(&ana).unwrap();
        let cert = Certificado::de_prueba("00000000T", "ANA", "PRUEBA");
        let leer = || std::fs::read(ana.join("datos.json")).unwrap();
        // De otra versión: no se abre, y se dice cuál.
        std::fs::write(ana.join("datos.json"), r#"{"version": 4, "perfil": {"nif": "00000000T"}, "facturas": [{"numero": "2026-001"}]}"#).unwrap();
        let antes = leer();
        let e = carpeta_de(&raiz, &cert).unwrap_err();
        assert!(e.contains("ana-prueba") && e.contains("otra versión (4)"), "{e}");
        assert_eq!(leer(), antes, "sin tocar");
        // Se abre bien pero es de otra persona (u otro NIF): tampoco se escribe encima.
        std::fs::write(ana.join("datos.json"), r#"{"version": 3, "perfil": {"nif": "11111111H"}}"#).unwrap();
        let antes = leer();
        assert!(carpeta_de(&raiz, &cert).unwrap_err().contains("ana-prueba"));
        assert_eq!(leer(), antes, "sin tocar");
        // La de Ana, con su NIF: esa.
        std::fs::write(ana.join("datos.json"), r#"{"version": 3, "perfil": {"nif": "00000000T"}}"#).unwrap();
        assert_eq!(carpeta_de(&raiz, &cert).unwrap(), ana);
        // Persona nueva: su carpeta, con su nombre.
        let luis = Certificado::de_prueba("22222222J", "LUIS", "EJEMPLO");
        let nueva = carpeta_de(&raiz, &luis).unwrap();
        assert_eq!(nueva, raiz.join("luis-ejemplo"));
        assert_eq!(almacen::cargar(&nueva).unwrap().perfil.nif, "22222222J");
        std::fs::remove_dir_all(&raiz).unwrap();
    }

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

    /// El 720 se presenta desde 2012: de antes de `PRIMER_ANIO` se mira solo ese, una vez (o con `todo`).
    #[test]
    fn el_720_desde_2012() {
        let mut d = Datos::default();
        assert_eq!(anios_720(&d, false), (2012..PRIMER_ANIO).rev().collect::<Vec<_>>());
        d.hacienda.insert(2015, Consulta::default());
        assert!(!anios_720(&d, false).contains(&2015) && anios_720(&d, true).contains(&2015));
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
        let mut r = Resultado {
            hacienda: vec![(2025, h)],
            tgss: Some(ss),
            nomina: [("2025-02".to_string(), Vec::new())].into(),
            nomina_aun_no: vec!["2026-09".into()],
            cuando: "2026-10-21T10:00:00".into(),
            ..Default::default()
        };
        assert_eq!(declaraciones(&r.hacienda[0].1), vec![("303".to_string(), "1T".to_string(), "ABC123".to_string())]);
        let dir = std::env::temp_dir().join(format!("gesshtor-aplicar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        aplicar(&dir, &mut d, &mut r);
        assert!(r.errores.is_empty(), "{:?}", r.errores);
        assert_eq!(crate::hacienda::declaraciones(&d, 2025).len(), 1);
        assert_eq!(d.tgss.recibos.len(), 1);
        let n = &d.tgss.nomina;
        assert!(n.meses.contains_key("2025-01") && n.meses.contains_key("2025-02") && !n.meses.contains_key("2026-09"));
        assert!(n.consultado["2026-09"].starts_with("20"));
        assert!(r.resumen().contains("Hacienda: 1 año"));
        // Y está en fuentes/: es lo que se lee al abrir la carpeta.
        almacen::guardar(&dir, &Datos::default(), &mut false).unwrap();
        let leido = almacen::cargar(&dir).unwrap();
        assert_eq!((&leido.hacienda, &leido.tgss), (&d.hacienda, &d.tgss));

        // Falla todo: se apunta, y no se pierde nada de lo que había.
        let mut mal = Resultado { error_hacienda: vec![(2025, "No contesta.".into())], error_tgss: Some(Error::de("No contesta.")), cuando: "2026-10-22T10:00:00".into(), ..Default::default() };
        let antes = d.clone();
        aplicar(&dir, &mut d, &mut mal);
        let leido = almacen::cargar(&dir).unwrap();
        assert_eq!((&d.hacienda, &d.tgss, &leido.hacienda, &leido.tgss), (&antes.hacienda, &antes.tgss, &antes.hacienda, &antes.tgss));
        let f: crate::dominio::fuente::Fuente<Tgss> = almacen::leer_fuente(&dir, "tgss.json").unwrap();
        assert_eq!((f.cuando.as_str(), f.error.map(|e| e.mensaje)), ("2026-10-22T10:00:00", Some("No contesta.".to_string())));
        // Y los dos fallos, a la vista (también al volver a abrir la carpeta).
        assert_eq!(d.fallos.keys().collect::<Vec<_>>(), ["aeat-2025-consulta.json", "tgss.json"]);
        assert_eq!(almacen::cargar(&dir).unwrap().fallos.keys().collect::<Vec<_>>(), ["aeat-2025-consulta.json", "tgss.json"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn perfil_del_certificado() {
        let cert = Certificado::de_prueba("00000000T", "ANA", "PRUEBA");
        let mut d = Datos::default();
        assert!(perfil_desde(&mut d, &cert));
        assert_eq!((d.perfil.nif.as_str(), d.perfil.nombre.as_str()), ("00000000T", "Ana Prueba"));
        assert!(!perfil_desde(&mut d, &cert), "la segunda vez no cambia nada");
    }
}
