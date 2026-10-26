//! Hacienda: lo presentado cada año (servicio «Comprobación de la presentación electrónica») y los PDF de cada
//! declaración (por su CSV). Solo consulta.
//!
//! Se usa con `Hacienda`: se crea con tu certificado y luego se pide lo presentado de un año (`presentado`) o el PDF
//! de una declaración (`pdf`).

use crate::red::certificado::Certificado;
use crate::red::cliente::{formulario_latin1, Cliente, Cuerpo, Respuesta, DOMINIOS_AEAT};
use crate::dominio::hacienda::{Consulta, Decl, Error, Modelo};
use std::path::{Path, PathBuf};
use super::xml;
use crate::dominio::fmt::ahora;

/// Los modelos que se consultan cada año.
/// Todos los que puede presentar una persona; los que el servicio no da (p. ej. el 721) se descartan solos.
pub const MODELOS: &[&str] = &["100", "714", "720", "721", "303", "130", "131", "390", "349", "347", "111", "115", "123", "180", "190", "193", "840"];
/// Las declaraciones censales (alta, cambios y baja en Hacienda).
pub const CENSALES: &[&str] = &["036", "037"];
/// Los que tiene que mirar una consulta para darla por completa (`fecha` del año).
const COMPLETA: &[&str] = &["303", "130", "349", "390", "347", "100"];

/// Formulario POST con el certificado; responde un XML `<servicioConsultasDirectas>` (consultas.xsd) con hasta
/// 100 `<respuestaCorrecta>` o un `<error><descripcionError>`.
const URL_CONSULTA: &str = "https://www1.agenciatributaria.gob.es/wlpl/SCEJ-MANT/ConsultaExt";
/// El justificante en PDF de cualquier declaración por su CSV (`?COMPLETA=SI&ORIGEN=D&CSV=…`).
const URL_JUSTIFICANTE: &str = "https://www2.agenciatributaria.gob.es/wlpl/inwinvoc/es.aeat.dit.adu.eeca.catalogo.VisualizaSc";
const NO_ACEPTADO: &str = "Hacienda no ha aceptado tu certificado.";
/// Peticiones a la vez a Hacienda (como la web).
const HILOS: usize = 3;

/// Lo presentado de cada año pedido, o por qué no se ha podido saber.
pub type PorAnio = Vec<(i32, Result<Consulta, String>)>;
/// Lo que dijo Hacienda de cada modelo de un año.
type Respuestas<'a> = Vec<(&'a str, Result<Vec<Decl>, Fallo>)>;

/// Por qué no salió la consulta de un modelo.
#[derive(Debug, Clone, PartialEq)]
enum Fallo {
    /// Hacienda no admite el certificado (redirige a `/errores/erro40…`): vale para todo el año.
    Certificado,
    /// No hubo conexión.
    Red(String),
    /// Hacienda contestó con un error o con algo que no se entiende.
    Hacienda(String),
    /// Ese modelo no se puede consultar con este servicio: no se guarda nada de él (ni como error).
    NoDisponible,
}

/// Hacienda con tu certificado. No hay que entrar: el certificado va en cada petición.
pub struct Hacienda {
    cli: Cliente,
    /// Tu NIF y tu nombre («APELLIDOS NOMBRE»), del certificado: los pide la consulta.
    nif: String,
    nombre: String,
}

impl Hacienda {
    pub fn new(cert: &Certificado) -> Hacienda {
        Hacienda { cli: Cliente::new(Some(cert), DOMINIOS_AEAT), nif: cert.nif.clone(), nombre: nombre_fiscal(cert) }
    }

    /// Lo presentado de un año, modelo a modelo. La `fecha` del año solo va si se han mirado todos los modelos del
    /// año (`COMPLETA`); si no, la consulta queda «a medias». «No ha presentado ninguna» no es un error: es un modelo
    /// sin declaraciones. Para guardarlo sobre lo que ya hubiera, `Consulta::mezclar`.
    pub fn presentado(&self, anio: i32, modelos: &[&str]) -> Result<Consulta, String> {
        self.presentado_en(&[anio], modelos, &mut |_, _| {})?.pop().map_or_else(|| Err("Ejercicio no válido.".into()), |(_, r)| r)
    }

    /// Lo presentado en varios años (cada uno, como en `presentado`), en el mismo orden. Todas las consultas, de
    /// todos los años, van seguidas de `HILOS` en `HILOS`; `progreso(hechas, total)` se llama al acabar cada una.
    /// Si Hacienda no acepta el certificado, no se sigue preguntando y es un error de todo.
    pub fn presentado_en(&self, anios: &[i32], modelos: &[&str], progreso: &mut dyn FnMut(usize, usize)) -> Result<PorAnio, String> {
        if self.nif.is_empty() {
            return Err("No encuentro el NIF dentro de tu certificado.".into());
        }
        let mut lista: Vec<&str> = Vec::new();
        for m in modelos.iter().copied().filter(|m| MODELOS.contains(m) || CENSALES.contains(m)) {
            if !lista.contains(&m) {
                lista.push(m);
            }
        }
        let valido = |y: &i32| (1000..=9999).contains(y);
        let pares: Vec<(i32, &str)> = anios.iter().copied().filter(valido).flat_map(|y| lista.iter().map(move |&m| (y, m))).collect();
        let hechas = a_la_vez(&pares, |&(y, m)| consultar_modelo(&self.cli, &self.nif, &self.nombre, m, y), |r| *r == Err(Fallo::Certificado), progreso);
        if hechas.iter().flatten().any(|r| *r == Err(Fallo::Certificado)) {
            return Err(NO_ACEPTADO.into());
        }
        let fecha = ahora();
        let mut por_anio: std::collections::BTreeMap<i32, Respuestas> = Default::default();
        for ((y, m), r) in pares.into_iter().zip(hechas) {
            por_anio.entry(y).or_default().extend(r.map(|r| (m, r)));
        }
        Ok(anios
            .iter()
            .map(|&y| match por_anio.remove(&y) {
                Some(resultados) => (y, armar(&self.nif, &lista, resultados, &fecha)),
                None if valido(&y) => (y, armar(&self.nif, &lista, Vec::new(), &fecha)),
                None => (y, Err("Ejercicio no válido.".into())),
            })
            .collect())
    }

    /// Descarga el PDF de una declaración (por su CSV) a `dir/justificantes/` (con su nombre: `lectura::nombre_pdf`)
    /// si no está ya, y devuelve su ruta.
    pub fn pdf(&self, dir: &Path, modelo: &str, ejercicio: i32, periodo: &str, csv: &str) -> Result<PathBuf, String> {
        let (nombre, csv) = nombre_pdf(modelo, ejercicio, periodo, csv)?;
        let carpeta = dir.join("justificantes");
        let ruta = carpeta.join(&nombre);
        if ruta.is_file() {
            return Ok(ruta);
        }
        let r = self.cli.get(&format!("{URL_JUSTIFICANTE}?COMPLETA=SI&ORIGEN=D&CSV={csv}"), None)?;
        if r.url.contains("/errores/erro40") {
            return Err(NO_ACEPTADO.into());
        }
        if !r.cuerpo.starts_with(b"%PDF") {
            return Err("Hacienda no ha devuelto el PDF. Descárgalo en la Sede con el CSV (Cotejo de documentos).".into());
        }
        std::fs::create_dir_all(&carpeta).map_err(|e| format!("No se puede crear {}: {e}", carpeta.display()))?;
        crate::dominio::almacen::escribir_atomico(&ruta, &r.cuerpo).map_err(|e| format!("No se puede guardar {nombre}: {e}"))?;
        Ok(ruta)
    }

    /// Descarga varios PDF (modelo, ejercicio, periodo, CSV; cada uno como en `pdf`) de `HILOS` en `HILOS`: lo que
    /// ha salido de cada uno, en el mismo orden. Si Hacienda no acepta el certificado, no se piden los demás (None).
    pub fn pdfs(&self, dir: &Path, lista: &[(String, i32, String, String)], progreso: &mut dyn FnMut(usize, usize)) -> Vec<Option<Result<PathBuf, String>>> {
        a_la_vez(lista, |(m, y, p, csv)| self.pdf(dir, m, *y, p, csv), |r| r.as_ref().err().is_some_and(|e| e == NO_ACEPTADO), progreso)
    }
}

/// El nombre del PDF y el CSV, comprobados (nunca se monta una URL ni un nombre raros).
fn nombre_pdf(modelo: &str, ejercicio: i32, periodo: &str, csv: &str) -> Result<(String, String), String> {
    let (periodo, csv) = (periodo.to_uppercase(), csv.to_uppercase());
    let alfanum = |s: &str| s.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase());
    let bien = modelo.len() == 3
        && modelo.bytes().all(|b| b.is_ascii_digit())
        && (1000..=9999).contains(&ejercicio)
        && periodo.len() == 2
        && alfanum(&periodo)
        && (8..=30).contains(&csv.len())
        && alfanum(&csv);
    if !bien {
        return Err("Datos del justificante no válidos.".into());
    }
    Ok((super::lectura::nombre_pdf(ejercicio, &periodo, modelo, &csv), csv))
}

/// Las declaraciones de un modelo y ejercicio que tiene Hacienda (vacío si no hay ninguna).
fn consultar_modelo(cli: &Cliente, nif: &str, anr: &str, modelo: &str, anio: i32) -> Result<Vec<Decl>, Fallo> {
    let cuerpo = formulario(nif, anr, modelo, anio);
    let r = cli.post(URL_CONSULTA, Cuerpo::Bytes(&cuerpo, "application/x-www-form-urlencoded"), None).map_err(Fallo::Red)?;
    interpretar(&r, modelo, anio)
}

/// El formulario de la consulta, en ISO-8859-1 (el periodo y las fechas en blanco: todo el ejercicio).
fn formulario(nif: &str, anr: &str, modelo: &str, anio: i32) -> Vec<u8> {
    let ejercicio = anio.to_string();
    formulario_latin1(&[("NIF", nif), ("ANR", anr), ("MOD", modelo), ("EJF", &ejercicio), ("PER", ""), ("FED", ""), ("FEH", ""), ("HOD", ""), ("HOH", "")])
}

/// «APELLIDOS NOMBRE» en mayúsculas: los apellidos y el nombre de pila del certificado o,
/// si no vienen, su nombre común sin el NIF.
fn nombre_fiscal(cert: &Certificado) -> String {
    let completo = if !cert.apellidos.is_empty() && !cert.nombre_pila.is_empty() {
        format!("{} {}", cert.apellidos, cert.nombre_pila)
    } else {
        let cn = cert.nombre_comun.split(" - ").next().unwrap_or("");
        cn[..sin_nif(cn)].to_string()
    };
    completo.to_uppercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Dónde empieza «NIF …» / «DNI …» (palabra suelta) en un nombre común, o su largo si no lo lleva.
fn sin_nif(s: &str) -> usize {
    let palabra = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    s.char_indices()
        .find(|&(i, _)| {
            let t = s[i..].get(..3).map(str::to_uppercase);
            matches!(t.as_deref(), Some("NIF" | "DNI")) && !palabra(s[..i].chars().next_back()) && !palabra(s[i + 3..].chars().next())
        })
        .map_or(s.len(), |(i, _)| i)
}

/// La respuesta de Hacienda a una consulta: sus declaraciones (por fecha de presentación) o por qué no.
fn interpretar(r: &Respuesta, modelo: &str, anio: i32) -> Result<Vec<Decl>, Fallo> {
    if r.url.contains("/errores/erro40") {
        return Err(Fallo::Certificado);
    }
    let doc = xml::leer(&r.texto());
    let Some(raiz) = doc.todos().into_iter().find(|n| n.nombre == "servicioConsultasDirectas") else {
        return Err(Fallo::Hacienda(format!("Respuesta inesperada de Hacienda (HTTP {}).", r.codigo)));
    };
    let (mut decl, mut error) = (Vec::new(), String::new());
    for n in raiz.todos() {
        match n.nombre.as_str() {
            "respuestaCorrecta" => {
                // Lo de otro modelo no se guarda aquí (el 036 viene como «C36»).
                if !n.hijo("modelo").is_empty() && codigo(n.hijo("modelo")) != modelo {
                    continue;
                }
                decl.push(Decl {
                    modelo: modelo.to_string(),
                    ejercicio: n.hijo("ejercicio").trim().parse().unwrap_or(anio),
                    periodo: n.hijo("periodo").to_string(),
                    csv: n.hijo("csv").to_string(),
                    justificante: n.hijo("justificante").to_string(),
                    anterior: n.hijo("just_anterior").to_string(),
                    fecha: n.hijo("fechaYHoraPresentacion").to_string(),
                });
            }
            "descripcionError" => error = n.texto.trim().to_string(),
            _ => {}
        }
    }
    if !error.is_empty() && decl.is_empty() {
        return if sin_resultados(&error) {
            Ok(Vec::new())
        } else if no_disponible(&error) {
            Err(Fallo::NoDisponible)
        } else {
            Err(Fallo::Hacienda(format!("Hacienda dice: {error}")))
        };
    }
    decl.sort_by(|a, b| a.fecha.cmp(&b.fecha));
    Ok(decl)
}

/// Hacienda devuelve el 036 como «C36» (y el 037 como «C37»).
pub fn codigo(m: &str) -> &str {
    match m {
        "C36" => "036",
        "C37" => "037",
        m => m,
    }
}

/// «No hay nada presentado», dicho de varias maneras («Usted no ha presentado la declaración solicitada»…): no
/// es un fallo. Es la expresión `no (se )?(han? )?(sido )?(encontrad|localizad|
/// exist|hay|const|presentado)|ning[uú]n|sin (datos|resultados|declaraciones)`.
pub fn sin_resultados(texto: &str) -> bool {
    let t = texto.to_lowercase();
    if ["ningun", "ningún", "sin datos", "sin resultados", "sin declaraciones"].iter().any(|x| t.contains(x)) {
        return true;
    }
    t.match_indices("no ").any(|(i, _)| {
        let mut r = &t[i + 3..];
        for opcion in [&["se "][..], &["han ", "ha "], &["sido "]] {
            if let Some(p) = opcion.iter().find(|p| r.starts_with(**p)) {
                r = &r[p.len()..];
            }
        }
        ["encontrad", "localizad", "exist", "hay", "const", "presentado"].iter().any(|x| r.starts_with(x))
    })
}

/// «El modelo y el ejercicio seleccionados no están disponibles para esta consulta»: este servicio no da ese modelo.
pub fn no_disponible(texto: &str) -> bool {
    let t = texto.to_lowercase();
    t.contains("no están disponibles para esta consulta") || t.contains("no estan disponibles para esta consulta")
}

/// Hace `f` con cada uno de la lista, `HILOS` a la vez, y devuelve lo que ha salido de cada uno en el mismo orden.
/// `progreso(hechos, total)` se llama, desde este hilo, al acabar cada uno. En cuanto uno da `parar`, no se empieza
/// ninguno más: los que no se han hecho son None.
fn a_la_vez<T: Sync, R: Send>(lista: &[T], f: impl Fn(&T) -> R + Sync, parar: impl Fn(&R) -> bool + Sync, progreso: &mut dyn FnMut(usize, usize)) -> Vec<Option<R>> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
    let (siguiente, alto) = (AtomicUsize::new(0), AtomicBool::new(false));
    let mut hechos: Vec<Option<R>> = lista.iter().map(|_| None).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..HILOS.min(lista.len()) {
            let (tx, siguiente, alto, f, parar) = (tx.clone(), &siguiente, &alto, &f, &parar);
            s.spawn(move || {
                while !alto.load(Relaxed) {
                    let i = siguiente.fetch_add(1, Relaxed);
                    let Some(x) = lista.get(i) else { break };
                    let r = f(x);
                    if parar(&r) {
                        alto.store(true, Relaxed);
                    }
                    if tx.send((i, r)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (n, (i, r)) in rx.into_iter().enumerate() {
            hechos[i] = Some(r);
            progreso(n + 1, lista.len());
        }
    });
    hechos
}

/// El año a partir de lo que dijo Hacienda de cada modelo.
fn armar(nif: &str, modelos: &[&str], resultados: Respuestas, fecha: &str) -> Result<Consulta, String> {
    if resultados.iter().any(|(_, r)| *r == Err(Fallo::Certificado)) {
        return Err(NO_ACEPTADO.into());
    }
    // Sin conexión con ninguno, no hay nada que guardar: es un fallo de la consulta entera.
    if let Some((_, Err(Fallo::Red(e)))) = resultados.first()
        && resultados.iter().all(|(_, r)| matches!(r, Err(Fallo::Red(_))))
    {
        return Err(e.clone());
    }
    let mut h = Consulta { nif: nif.to_string(), ..Default::default() };
    for (m, r) in resultados {
        match r {
            Ok(decl) => {
                h.modelos.insert(m.into(), Modelo { fecha: fecha.into(), decl });
            }
            Err(Fallo::Red(e) | Fallo::Hacienda(e)) => {
                h.errores.insert(m.into(), Error { fecha: fecha.into(), error: e });
            }
            Err(Fallo::Certificado | Fallo::NoDisponible) => {}
        }
    }
    if COMPLETA.iter().all(|m| modelos.contains(m)) {
        h.fecha = fecha.into();
    }
    Ok(h)
}

#[cfg(test)]
mod pruebas {
    //! Pruebas de la consulta a Hacienda con respuestas de verdad anonimizadas (NIF, CSV, justificantes, expedientes y
    //! fechas inventados).
    use super::*;
    use crate::dominio::hacienda::Consulta;
    use crate::dominio::Datos;
    use crate::hacienda;

    // Respuestas de Hacienda de verdad, con todo cambiado por datos inventados.
    const CENSAL_036: &str = r#"<?xml version="1.0" encoding="UTF-8"?><servicioConsultasDirectas><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>C36</modelo><periodo>0A</periodo><nif>00000000T</nif><csv>PRUEBA036A0A0001</csv><expediente>0000A0000000001A         </expediente><justificante>0360000000001</justificante><fechaYHoraPresentacion>2024-03-01T11:00:00</fechaYHoraPresentacion></respuestaCorrecta></servicioConsultasDirectas>"#;
    const RENTA_Y_COMPLEMENTARIA: &str = r#"<?xml version="1.0" encoding="UTF-8"?><servicioConsultasDirectas><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>100</modelo><periodo>0A</periodo><nif>00000000T</nif><csv>PRUEBA100C0A0002</csv><expediente>2024000000000102</expediente><justificante>1000000000002</justificante><just_anterior>1000000000001</just_anterior><fechaYHoraPresentacion>2025-05-06T12:30:00</fechaYHoraPresentacion></respuestaCorrecta><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>100</modelo><periodo>0A</periodo><nif>00000000T</nif><csv>PRUEBA100A0A0001</csv><expediente>2024000000000101</expediente><justificante>1000000000001</justificante><fechaYHoraPresentacion>2025-04-22T09:15:00</fechaYHoraPresentacion></respuestaCorrecta></servicioConsultasDirectas>"#;
    const TRIMESTRES_303: &str = r#"<?xml version="1.0" encoding="UTF-8"?><servicioConsultasDirectas><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>303</modelo><periodo>2T</periodo><nif>00000000T</nif><csv>PRUEBA303A2T0001</csv><expediente>202400000000002A         </expediente><justificante>3030000000002</justificante><fechaYHoraPresentacion>2024-07-18T10:00:02</fechaYHoraPresentacion></respuestaCorrecta><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>303</modelo><periodo>1T</periodo><nif>00000000T</nif><csv>PRUEBA303A1T0001</csv><expediente>202400000000001A         </expediente><justificante>3030000000001</justificante><fechaYHoraPresentacion>2024-04-19T10:00:01</fechaYHoraPresentacion></respuestaCorrecta><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>303</modelo><periodo>3T</periodo><nif>00000000T</nif><csv>PRUEBA303A3T0001</csv><expediente>202400000000003A         </expediente><justificante>3030000000003</justificante><fechaYHoraPresentacion>2024-10-17T10:00:03</fechaYHoraPresentacion></respuestaCorrecta><respuestaCorrecta><ejercicio>2024</ejercicio><modelo>303</modelo><periodo>4T</periodo><nif>00000000T</nif><csv>PRUEBA303A4T0001</csv><expediente>202400000000004A         </expediente><justificante>3030000000004</justificante><fechaYHoraPresentacion>2025-01-28T10:00:04</fechaYHoraPresentacion></respuestaCorrecta></servicioConsultasDirectas>"#;
    const NO_DISPONIBLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><servicioConsultasDirectas><error><descripcionError>El modelo y el ejercicio seleccionados no están disponibles para esta consulta721, 2024</descripcionError></error></servicioConsultasDirectas>"#;
    const SIN_PRESENTAR: &str = r#"<?xml version="1.0" encoding="UTF-8"?><servicioConsultasDirectas><error><descripcionError>Usted no ha presentado la declaración solicitada</descripcionError></error></servicioConsultasDirectas>"#;

    fn ejemplo(nombre: &str) -> Vec<u8> {
        match nombre {
            "036_censal.xml" => CENSAL_036,
            "100_complementaria.xml" => RENTA_Y_COMPLEMENTARIA,
            "303_trimestres.xml" => TRIMESTRES_303,
            "no_disponible.xml" => NO_DISPONIBLE,
            "sin_presentar.xml" => SIN_PRESENTAR,
            _ => panic!("no hay ejemplo {nombre}"),
        }
        .as_bytes()
        .to_vec()
    }

    fn respuesta(cuerpo: Vec<u8>) -> Respuesta {
        Respuesta { url: URL_CONSULTA.into(), codigo: 200, cabeceras: Vec::new(), cuerpo }
    }

    fn leer(nombre: &str, modelo: &str) -> Result<Vec<Decl>, Fallo> {
        interpretar(&respuesta(ejemplo(nombre)), modelo, 2024)
    }

    fn cert_de_prueba() -> Certificado {
        Certificado::abrir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pruebas/prueba.p12"), "prueba").unwrap()
    }

    /// Lo que dice Hacienda de cada modelo de 2024 en los ejemplos.
    fn resultados_2024() -> Respuestas<'static> {
        vec![
            ("303", leer("303_trimestres.xml", "303")),
            ("130", leer("sin_presentar.xml", "130")),
            ("349", leer("sin_presentar.xml", "349")),
            ("390", Err(Fallo::Red("www1.agenciatributaria.gob.es no contesta (se ha agotado el tiempo).".into()))),
            ("347", leer("sin_presentar.xml", "347")),
            ("100", leer("100_complementaria.xml", "100")),
            ("720", leer("no_disponible.xml", "720")),
        ]
    }

    #[test]
    fn a_la_vez_en_orden() {
        let lista: Vec<u64> = (0..20).collect();
        let mut avisos = Vec::new();
        let r = a_la_vez(&lista, |x| x * 2, |_| false, &mut |h, t| avisos.push((h, t)));
        assert_eq!(r, (0..20).map(|x| Some(x * 2)).collect::<Vec<_>>());
        assert_eq!(avisos, (1..=20).map(|h| (h, 20)).collect::<Vec<_>>());
        // En cuanto uno da `parar`, no se empieza ninguno más: solo acaban los que ya iban.
        let r = a_la_vez(&lista, |&x| {
            if x > 0 {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            x
        }, |&x| x == 0, &mut |_, _| {});
        assert_eq!(r[0], Some(0));
        assert!(r.iter().flatten().count() <= HILOS, "{r:?}");
    }

    #[test]
    fn lee_las_respuestas() {
        // Los cuatro trimestres, por fecha de presentación (Hacienda no los da en orden), con todo limpio.
        let v = leer("303_trimestres.xml", "303").unwrap();
        assert_eq!(v.iter().map(|d| d.periodo.as_str()).collect::<Vec<_>>(), ["1T", "2T", "3T", "4T"]);
        let esperado = Decl {
            modelo: "303".into(),
            ejercicio: 2024,
            periodo: "4T".into(),
            csv: "PRUEBA303A4T0001".into(),
            justificante: "3030000000004".into(),
            anterior: String::new(),
            fecha: "2025-01-28T10:00:04".into(),
        };
        assert_eq!(v[3], esperado);
        // La Renta y su complementaria (que lleva el justificante de la anterior): la última, al final.
        let v = leer("100_complementaria.xml", "100").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].anterior.as_str(), v[1].anterior.as_str()), ("", "1000000000001"));
        assert_eq!(v[1].justificante, "1000000000002");
        // El 036 viene como «C36»: se guarda como «036».
        let v = leer("036_censal.xml", "036").unwrap();
        assert_eq!((v.len(), v[0].modelo.as_str(), v[0].periodo.as_str()), (1, "036", "0A"));
        assert_eq!(leer("036_censal.xml", "037"), Ok(Vec::new()), "lo de otro modelo no se mezcla");
        // «No ha presentado» no es un fallo; «no disponible» tampoco: ese modelo no se guarda.
        assert_eq!(leer("sin_presentar.xml", "349"), Ok(Vec::new()));
        assert_eq!(leer("no_disponible.xml", "721"), Err(Fallo::NoDisponible));
        let h = armar("00000000T", &["303", "721"], vec![("303", Ok(Vec::new())), ("721", Err(Fallo::NoDisponible))], "2026-10-21T10:00:00").unwrap();
        assert!(!h.modelos.contains_key("721") && !h.errores.contains_key("721") && h.modelos.contains_key("303"), "{h:?}");
        // En ISO-8859-1 también se entiende.
        let latin1: Vec<u8> = String::from_utf8(ejemplo("sin_presentar.xml")).unwrap().chars().map(|c| c as u32 as u8).collect();
        assert_eq!(interpretar(&respuesta(latin1), "303", 2024), Ok(Vec::new()));
    }

    #[test]
    fn respuestas_raras() {
        // Una página de error en HTML no es «nada presentado».
        let r = interpretar(&respuesta(b"<html><body>Servicio no disponible</body></html>".to_vec()), "303", 2024);
        assert_eq!(r, Err(Fallo::Hacienda("Respuesta inesperada de Hacienda (HTTP 200).".into())));
        assert!(matches!(interpretar(&respuesta(Vec::new()), "303", 2024), Err(Fallo::Hacienda(_))));
        // Con espacio de nombres, saltos de línea y sin el ejercicio.
        let x = "<?xml version='1.0'?>\n<ns2:servicioConsultasDirectas xmlns:ns2='urn:x'>\n <ns2:respuestaCorrecta>\n  <ns2:modelo>130</ns2:modelo>\n  <ns2:periodo>2T</ns2:periodo>\n  <ns2:csv>ABCDEFGH12345678</ns2:csv>\n </ns2:respuestaCorrecta>\n</ns2:servicioConsultasDirectas>";
        let v = interpretar(&respuesta(x.into()), "130", 2023).unwrap();
        assert_eq!((v[0].ejercicio, v[0].periodo.as_str(), v[0].csv.as_str()), (2023, "2T", "ABCDEFGH12345678"));
        // Un error que no es «nada presentado».
        let x = "<servicioConsultasDirectas><error><descripcionError>Error interno</descripcionError></error></servicioConsultasDirectas>";
        assert_eq!(interpretar(&respuesta(x.into()), "303", 2024), Err(Fallo::Hacienda("Hacienda dice: Error interno".into())));
        // Hacienda no acepta el certificado: redirige a su página de error.
        let r = Respuesta { url: "https://www1.agenciatributaria.gob.es/static_files/common/internet/errores/erro4011.html".into(), ..respuesta(b"<html/>".to_vec()) };
        assert_eq!(interpretar(&r, "303", 2024), Err(Fallo::Certificado));
    }

    #[test]
    fn sin_resultados_de_hacienda() {
        for t in ["Usted no ha presentado la declaración solicitada", "No se han encontrado datos", "NO EXISTEN DECLARACIONES", "Ningún resultado", "Consulta sin resultados", "no ha sido localizada"] {
            assert!(sin_resultados(t), "{t}");
        }
        for t in ["El modelo y el ejercicio seleccionados no están disponibles para esta consulta721, 2024", "Error interno", "No autorizado"] {
            assert!(!sin_resultados(t), "{t}");
        }
    }

    #[test]
    fn arma_el_anio() {
        let fecha = "2026-10-21T10:03:00";
        let h = armar("00000000T", MODELOS, resultados_2024(), fecha).unwrap();
        assert_eq!((h.nif.as_str(), h.fecha.as_str()), ("00000000T", fecha));
        assert_eq!(h.modelos.keys().map(String::as_str).collect::<Vec<_>>(), ["100", "130", "303", "347", "349"]);
        assert_eq!(h.modelos["130"], Modelo { fecha: fecha.into(), decl: Vec::new() });
        assert_eq!(h.modelos["303"].fecha, fecha);
        // El 720 «no disponible» no se guarda ni como error.
        assert_eq!(h.errores.keys().map(String::as_str).collect::<Vec<_>>(), ["390"]);
        assert_eq!(h.errores["390"], Error { fecha: fecha.into(), error: "www1.agenciatributaria.gob.es no contesta (se ha agotado el tiempo).".into() });

        // Lo lee la app (también tras guardarlo y volverlo a cargar).
        let mut d = Datos::default();
        d.hacienda.insert(2024, h);
        let d: Datos = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        let v = hacienda::declaraciones(&d, 2024);
        assert_eq!(v.len(), 6);
        let t3 = hacienda::presentada(&d, 2024, 3, hacienda::Modelo::M303).unwrap();
        assert_eq!((t3.csv.as_str(), t3.ejercicio, t3.clave()), ("PRUEBA303A3T0001", 2024, "2024-3T-303".to_string()));
        let renta = hacienda::presentada(&d, 2024, 0, hacienda::Modelo::M100).unwrap();
        assert_eq!((renta.justificante.as_str(), renta.anterior.as_str()), ("1000000000002", "1000000000001"), "la complementaria es la que vale");
        assert_eq!(hacienda::consultado(&d, 2024).as_deref(), Some(fecha));
        assert!(hacienda::presentada(&d, 2024, 1, hacienda::Modelo::M130).is_none());

        // Solo algunos modelos: la consulta queda a medias (sin `fecha` del año).
        let h = armar("00000000T", &["100"], vec![("100", leer("100_complementaria.xml", "100"))], fecha).unwrap();
        assert!(h.fecha.is_empty());
        let mut d = Datos::default();
        d.hacienda.insert(2024, h);
        assert_eq!(hacienda::declaraciones(&d, 2024).len(), 2);
        assert_eq!(hacienda::consultado(&d, 2024), None);
    }

    #[test]
    fn fallos_de_toda_la_consulta() {
        let mut r = resultados_2024();
        r[2].1 = Err(Fallo::Certificado);
        assert_eq!(armar("00000000T", MODELOS, r, "x"), Err(NO_ACEPTADO.to_string()));
        // Sin conexión con ninguno: un solo error, nada que guardar.
        let sin_red = |m| (m, Err(Fallo::Red("No se puede conectar con www1.agenciatributaria.gob.es".into())));
        assert_eq!(armar("00000000T", &["303", "130"], vec![sin_red("303"), sin_red("130")], "x"), Err("No se puede conectar con www1.agenciatributaria.gob.es".to_string()));
        // Con alguno bien, cada fallo va a su modelo.
        let h = armar("00000000T", &["303", "130"], vec![sin_red("303"), ("130", Ok(Vec::new()))], "x").unwrap();
        assert!(h.errores["303"].error.contains("No se puede conectar"));
        assert!(h.modelos["130"].decl.is_empty());
    }

    #[test]
    fn mezcla() {
        let decl = |csv: &str| Decl { csv: csv.into(), ..Default::default() };
        let modelo = |fecha: &str, decl: Vec<Decl>| Modelo { fecha: fecha.into(), decl };
        let error = |fecha: &str, e: &str| Error { fecha: fecha.into(), error: e.into() };
        let antes = Consulta {
            nif: "00000000T".into(),
            fecha: "a".into(),
            modelos: [("303".to_string(), modelo("a", vec![decl("VIEJO")])), ("130".to_string(), modelo("a", vec![]))].into(),
            errores: [("349".to_string(), error("a", "x"))].into(),
        };
        let nueva = Consulta {
            nif: "00000000T".into(),
            fecha: String::new(),
            modelos: [("349".to_string(), modelo("b", vec![]))].into(),
            errores: [("303".to_string(), error("b", "y"))].into(),
        };
        let h = Consulta::mezclar(Some(&antes), nueva.clone());
        // El 303 falló: se apunta el error y se queda lo que había; el 349 ya salió bien; la fecha del año, la de antes.
        assert_eq!(h.modelos["303"].decl[0].csv, "VIEJO");
        assert_eq!(h.errores["303"].error, "y");
        assert_eq!(h.modelos["349"].fecha, "b");
        assert!(!h.errores.contains_key("349"));
        assert_eq!((h.fecha.as_str(), h.modelos["130"].fecha.as_str()), ("a", "a"));
        // Con fecha del año, se pone.
        let completa = Consulta { fecha: "b".into(), ..nueva.clone() };
        assert_eq!(Consulta::mezclar(Some(&antes), completa).fecha, "b");
        // De otro NIF, o sin nada antes, se empieza de cero.
        let otro = Consulta { nif: "11111111H".into(), ..antes.clone() };
        for a in [Some(&otro), None] {
            assert_eq!(Consulta::mezclar(a, nueva.clone()), nueva);
        }
    }

    #[test]
    fn formulario_y_nombre() {
        assert_eq!(formulario("00000000T", "PEÑA LÓPEZ ANA", "303", 2024), b"NIF=00000000T&ANR=PE%D1A+L%D3PEZ+ANA&MOD=303&EJF=2024&PER=&FED=&FEH=&HOD=&HOH=".to_vec());
        let mut c = cert_de_prueba();
        assert_eq!(nombre_fiscal(&c), "PRUEBA ANA");
        c.apellidos = "Peña  López".into();
        c.nombre_pila = "maría josé".into();
        assert_eq!(nombre_fiscal(&c), "PEÑA LÓPEZ MARÍA JOSÉ");
        // Sin apellidos y nombre de pila, del nombre común sin el NIF.
        c.apellidos.clear();
        for (cn, anr) in [("PEÑA LOPEZ ANA - 00000000T", "PEÑA LOPEZ ANA"), ("PEÑA LOPEZ ANA NIF 00000000T", "PEÑA LOPEZ ANA"), ("Daniela Dnilo dni 00000000T", "DANIELA DNILO")] {
            c.nombre_comun = cn.into();
            assert_eq!(nombre_fiscal(&c), anr);
        }
        assert_eq!(sin_nif("ANA NIF:00000000T"), 4);
        assert_eq!(sin_nif("ANA DNIÉ"), "ANA DNIÉ".len());
    }

    #[test]
    fn comprueba_los_datos_del_pdf() {
        assert_eq!(nombre_pdf("303", 2024, "3t", "prueba303a3t0001"), Ok(("2024_3T_303_PRUEBA303A3T0001.pdf".into(), "PRUEBA303A3T0001".into())));
        assert!(nombre_pdf("036", 2026, "0A", "ABCDEFGH").is_ok());
        for (m, e, p, c) in [
            ("30", 2024, "3T", "ABCDEFGH12345678"),
            ("30a", 2024, "3T", "ABCDEFGH12345678"),
            ("../", 2024, "3T", "ABCDEFGH12345678"),
            ("303", 999, "3T", "ABCDEFGH12345678"),
            ("303", 2024, "3", "ABCDEFGH12345678"),
            ("303", 2024, "3/", "ABCDEFGH12345678"),
            ("303", 2024, "3T", "ABC"),
            ("303", 2024, "3T", "ABCDEFGH&CSV=X"),
            ("303", 2024, "3T", "ABCDEFGH/../../X"),
            ("303", 2024, "3T", "ABCDEFGHÑ1234567"),
            ("303", 2024, "3T", &"A".repeat(31)),
        ] {
            assert!(nombre_pdf(m, e, p, c).is_err(), "{m} {e} {p} {c}");
        }
        // Si ya está, no se descarga (y con datos raros, ni se intenta).
        let dir = std::env::temp_dir().join(format!("gesshtor-aeat-{}", std::process::id()));
        let j = dir.join("justificantes");
        std::fs::create_dir_all(&j).unwrap();
        std::fs::write(j.join("2024_3T_303_PRUEBA303A3T0001.pdf"), b"%PDF-1.4").unwrap();
        let aeat = Hacienda::new(&cert_de_prueba());
        assert_eq!(aeat.pdf(&dir, "303", 2024, "3T", "PRUEBA303A3T0001"), Ok(j.join("2024_3T_303_PRUEBA303A3T0001.pdf")));
        assert_eq!(aeat.pdf(&dir, "303", 2024, "3T", "X&Y"), Err("Datos del justificante no válidos.".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Con internet: `cargo test hacienda::consulta -- --ignored --nocapture`. El certificado de prueba no es de la FNMT:
    /// Hacienda acepta la conexión y contesta que no lo admite. Prueba la ida y vuelta de verdad.
    #[test]
    #[ignore]
    fn hacienda_de_verdad_con_certificado_de_prueba() {
        let aeat = Hacienda::new(&cert_de_prueba());
        assert_eq!(aeat.presentado(2024, MODELOS), Err(NO_ACEPTADO.to_string()));
        let dir = std::env::temp_dir().join(format!("gesshtor-aeat-red-{}", std::process::id()));
        let r = aeat.pdf(&dir, "303", 2024, "3T", "PRUEBA303A3T0001");
        println!("PDF: {r:?}");
        assert!(r.is_err());
        assert!(!dir.join("justificantes").exists());
    }
}
