//! Leer los PDF de Hacienda que hay en la carpeta de datos (`justificantes/`): las Rentas y las declaraciones
//! con casillas (303, 130, 390…). No habla con Hacienda: lee lo que ya está descargado.
//!
//! Lo leído de cada PDF se guarda en su fichero de `fuentes/` («2025_1T_303_CSV.pdf» →
//! «aeat-2025-1T-303-CSV.json», ver `dominio::fuente`), con la huella del PDF y la versión del lector: al
//! arrancar solo se lee lo nuevo, lo que ha cambiado o lo que se leyó con otro lector. Así arrancar es instantáneo.
//! Si al releer uno falla, se queda lo último que se leyó bien.

use super::modelo::{Fallo, Modelo, Periodo};
use super::pdf::Documento;
use super::{formulario, generico, modelo_100, pdf};
use super::{modelo_130::M130, modelo_303::M303, modelo_390::M390};
use crate::dominio::almacen;
use crate::dominio::fuente::{Error, Fuente, Huella, Leido};
use crate::dominio::Lecturas;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Súbelo cuando cambie lo que sale de algún lector: se releen solos todos los PDF. (4: lo que antes salía vacío o
/// a 0 sin decir nada ahora es un fallo, ver `dominio::fallo`; 5: el informe de un fallo ya no lleva el texto del PDF,
/// solo su estructura, y los fallos guardados se rehacen.)
pub const VERSION_LECTOR: u32 = 5;

/// El nombre del PDF de una declaración en `justificantes/`: «2026_3T_303_CSV.pdf». Año, periodo y modelo, en ese
/// orden: así la carpeta sale en orden de calendario (lo anual de cada año y luego cada trimestre, con su 130 y su
/// 303 juntos). El CSV distingue una complementaria de la original.
pub fn nombre_pdf(ejercicio: i32, periodo: &str, modelo: &str, csv: &str) -> String {
    format!("{}{csv}.pdf", prefijo_pdf(ejercicio, periodo, modelo))
}

/// El principio del nombre de los PDF de un año, periodo y modelo (todo menos el CSV).
pub fn prefijo_pdf(ejercicio: i32, periodo: &str, modelo: &str) -> String {
    format!("{ejercicio}_{periodo}_{modelo}_")
}

/// Un PDF de declaración de `justificantes/`, por su nombre (ver `nombre_pdf`).
#[derive(Debug, Clone, PartialEq)]
pub struct Justificante {
    pub ruta: PathBuf,
    pub modelo: String,
    pub ejercicio: i32,
    /// «1T»…«4T», «0A» (anual), «01»…«12»
    pub periodo: String,
    pub csv: String,
}

impl Justificante {
    pub fn trimestre(&self) -> u32 {
        self.periodo.strip_suffix('T').and_then(|q| q.parse().ok()).unwrap_or(0)
    }
    /// Su clave: «2026-3T-303», «2025-100».
    pub fn clave(&self) -> String {
        crate::hacienda::clave(self.ejercicio, self.trimestre(), &self.modelo)
    }
    pub fn nombre(&self) -> String {
        self.ruta.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }
    /// Su modelo, si la app lo lee: la Renta y los de casillas en formulario (303, 130, 390), desde `PRIMER_ANIO`.
    pub fn leible(&self) -> Option<Modelo> {
        Modelo::de_codigo(&self.modelo).filter(|_| self.ejercicio >= crate::dominio::PRIMER_ANIO)
    }
    /// Su fichero en `fuentes/`: «aeat-2025-1T-303-CSV.json» (sin CSV, «aeat-2025-0A-100.json»).
    pub fn fuente(&self) -> String {
        let csv = if self.csv.is_empty() { String::new() } else { format!("-{}", self.csv) };
        format!("aeat-{}-{}-{}{csv}.json", self.ejercicio, self.periodo, self.modelo)
    }
    /// De dónde sale lo leído: «justificantes/2025_1T_303_CSV.pdf».
    fn origen(&self) -> String {
        format!("justificantes/{}", self.nombre())
    }
    /// Qué es, para el informe de un fallo (sin el CSV ni la carpeta): «PDF del modelo 303 del 1T de 2025».
    fn descripcion(&self) -> String {
        match (self.modelo.as_str(), Periodo::de(&self.periodo)) {
            ("100", _) => format!("PDF de la Renta (modelo 100) de {}", self.ejercicio),
            (m, Periodo::Anual) => format!("PDF del modelo {m} de {}", self.ejercicio),
            (m, p) => format!("PDF del modelo {m} del {p} de {}", self.ejercicio),
        }
    }
    /// Cómo volver a leerlo, sin el CSV (que da acceso a la declaración en la sede de Hacienda).
    fn reproducir(&self) -> String {
        let csv = if self.csv.is_empty() { String::new() } else { "‹CSV›".into() };
        format!(
            "gesshtor json justificantes/{} (desde la carpeta de la persona: sale lo leído y este fallo; el PDF tiene sus datos, no lo compartas)",
            nombre_pdf(self.ejercicio, &self.periodo, &self.modelo, &csv)
        )
    }
}

/// Los PDF de declaraciones de la carpeta, por nombre (los que no siguen el patrón se ignoran).
pub fn justificantes(dir: &Path) -> Vec<Justificante> {
    let mut v: Vec<Justificante> = std::fs::read_dir(dir.join("justificantes"))
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).filter_map(|p| de_nombre(&p)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| (a.ejercicio, a.modelo.as_str(), a.periodo.as_str(), a.csv.as_str()).cmp(&(b.ejercicio, b.modelo.as_str(), b.periodo.as_str(), b.csv.as_str())));
    v
}

/// ¿Son `n` cifras?
fn cifras(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// «2026_3T_303_CSV.pdf» → año, periodo, modelo y CSV (que puede faltar). None si no es un nombre así.
fn partes(n: &str) -> Option<(i32, &str, &str, &str)> {
    let base = n.strip_suffix(".pdf").or_else(|| n.strip_suffix(".PDF"))?;
    let p: Vec<&str> = base.splitn(4, '_').collect();
    if p.len() < 3 || !cifras(p[0], 4) || !cifras(p[2], 3) {
        return None;
    }
    Some((p[0].parse().ok()?, p[1], p[2], p.get(3).copied().unwrap_or("")))
}

fn de_nombre(p: &Path) -> Option<Justificante> {
    let (ejercicio, periodo, modelo, csv) = partes(p.file_name()?.to_str()?)?;
    Some(Justificante { ruta: p.to_path_buf(), modelo: modelo.into(), ejercicio, periodo: periodo.into(), csv: csv.into() })
}

/// El nombre de ahora de un PDF con el nombre de antes («303_2026_3T_CSV.pdf», el modelo delante).
fn nombre_de_antes(n: &str) -> Option<String> {
    let base = n.strip_suffix(".pdf").or_else(|| n.strip_suffix(".PDF"))?;
    let p: Vec<&str> = base.splitn(4, '_').collect();
    if p.len() < 3 || !cifras(p[0], 3) || !cifras(p[1], 4) {
        return None;
    }
    let (modelo, ejercicio, periodo) = (p[0], p[1].parse().ok()?, p[2]);
    Some(match p.get(3) {
        Some(csv) => nombre_pdf(ejercicio, periodo, modelo, csv),
        None => format!("{ejercicio}_{periodo}_{modelo}.pdf"),
    })
}

/// Cambia una vez los nombres de antes («303_2026_3T_CSV.pdf») a los de ahora («2026_3T_303_CSV.pdf»). Si ya hay
/// uno con el nombre nuevo, no lo toca. Si no hay nombres de antes, no hace nada: se llama al abrir la carpeta.
pub fn renombrar_antiguos(dir: &Path) {
    let carpeta = dir.join("justificantes");
    let Ok(it) = std::fs::read_dir(&carpeta) else { return };
    for ruta in it.filter_map(|e| e.ok().map(|e| e.path())) {
        let Some(nuevo) = ruta.file_name().and_then(|n| n.to_str()).and_then(nombre_de_antes) else { continue };
        let destino = carpeta.join(&nuevo);
        if !destino.exists() {
            let _ = std::fs::rename(&ruta, &destino);
        }
    }
}

/// Lee todo lo que haga falta de la carpeta y lo devuelve. `progreso(hechos, total)` se llama por cada PDF que
/// de verdad hay que leer (los ya leídos no cuentan). Se puede llamar desde otro hilo.
///
/// Solo se lee lo que no tiene su fichero en `fuentes/`, o lo tiene de otro PDF (otro nombre u otra huella) o de
/// otra `VERSION_LECTOR`; y solo se escribe lo que se lee. Los PDF se leen en paralelo. Los ficheros de PDF que
/// ya no están se borran.
pub fn leer_todo(dir: &Path, progreso: &mut dyn FnMut(usize, usize)) -> Lecturas {
    let js: Vec<Justificante> = justificantes(dir).into_iter().filter(|j| j.leible().is_some()).collect();
    let mut hechos = Vec::new();
    let mut por_leer = Vec::new();
    for j in &js {
        let bytes = std::fs::read(&j.ruta);
        let huella = bytes.as_deref().ok().map(Huella::de);
        let antes: Option<Fuente<Leido>> = almacen::leer_fuente(dir, &j.fuente());
        match antes {
            Some(f) if f.lector == VERSION_LECTOR && f.huella.is_some() && f.huella == huella && f.origen == j.origen() => hechos.push((j, f)),
            antes => por_leer.push((j, bytes, huella, antes)),
        }
    }
    borrar_sin_pdf(dir, &js);
    let mut sin_guardar = Vec::new();
    for (j, f) in leer_varios(&por_leer, progreso) {
        // Si no se puede guardar, se vuelve a leer la próxima vez (y se dice).
        if let Err(e) = almacen::guardar_fuente(dir, &j.fuente(), &f) {
            sin_guardar.push((j.fuente(), e.to_string()));
        }
        hechos.push((j, f));
    }
    let mut l = lecturas(&hechos);
    for (n, e) in sin_guardar {
        let msg = format!("No se ha podido guardar lo leído en fuentes/{n}: se vuelve a leer cada vez que se abre la app.");
        l.fallos.insert(format!("fuentes/{n}"), Error::formato("FUENTE_NO_SE_GUARDA", crate::donde!("leer_todo"), msg).en(format!("fuentes/{n}")).encontrado(e));
    }
    l
}

/// Borra de `fuentes/` lo leído de PDF que ya no están en `justificantes/` (solo si se ha podido mirar qué hay).
fn borrar_sin_pdf(dir: &Path, js: &[Justificante]) {
    let (Ok(_), Ok(it)) = (std::fs::read_dir(dir.join("justificantes")), std::fs::read_dir(almacen::ruta_fuentes(dir))) else { return };
    let hay: std::collections::HashSet<String> = js.iter().map(Justificante::fuente).collect();
    for n in it.filter_map(|e| e.ok()?.file_name().into_string().ok()) {
        let de_pdf = n.strip_prefix("aeat-").and_then(|r| r.strip_suffix(".json")).is_some_and(|r| partes(&format!("{}.pdf", r.replace('-', "_"))).is_some());
        if de_pdf && !hay.contains(&n) {
            let _ = std::fs::remove_file(almacen::ruta_fuentes(dir).join(n));
        }
    }
}

/// Lo que se hace con cada PDF por leer: el PDF, sus bytes (o por qué no se ha podido abrir), su huella y lo de antes.
type PorLeer<'a> = (&'a Justificante, std::io::Result<Vec<u8>>, Option<Huella>, Option<Fuente<Leido>>);

/// Lee los PDF en varios hilos; `progreso` se llama desde este, cada vez que acaba uno.
fn leer_varios<'a>(lista: &[PorLeer<'a>], progreso: &mut dyn FnMut(usize, usize)) -> Vec<(&'a Justificante, Fuente<Leido>)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let total = lista.len();
    if total == 0 {
        return Vec::new();
    }
    progreso(0, total);
    let hilos = std::thread::available_parallelism().map_or(4, |n| n.get()).min(total);
    let siguiente = AtomicUsize::new(0);
    let cuando = crate::dominio::fmt::ahora();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut hechos = Vec::with_capacity(total);
    std::thread::scope(|s| {
        for _ in 0..hilos {
            let (tx, siguiente, cuando) = (tx.clone(), &siguiente, &cuando);
            s.spawn(move || {
                while let Some((j, bytes, huella, antes)) = lista.get(siguiente.fetch_add(1, Ordering::Relaxed)) {
                    let f = Fuente { huella: huella.clone(), ..leer_bytes(j, bytes.as_deref().map_err(|e| e.to_string()), cuando) };
                    if tx.send((*j, f.encima(antes.clone()))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for x in rx {
            hechos.push(x);
            progreso(hechos.len(), total);
        }
    });
    hechos
}

/// Lee un PDF por la ruta (sin mirar ni tocar nada de su carpeta): lo que se guardaría en `fuentes/` (con la ruta
/// como `origen`). De qué declaración es lo dice su nombre, como los de `justificantes/` («2025_1T_303_CSV.pdf», o
/// el de antes, «303_2025_1T_CSV.pdf»), o, si se llama de otra forma, `como` («2025_0A_100»).
pub fn leer_fichero(ruta: &Path, como: Option<&str>) -> Result<Fuente<Leido>, String> {
    let nombre = ruta.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let nuevo = match como {
        Some(c) => format!("{c}.pdf"),
        None => nombre_de_antes(nombre).unwrap_or_else(|| nombre.to_string()),
    };
    let j = de_nombre(&ruta.with_file_name(&nuevo)).ok_or_else(|| {
        format!("No sé de qué declaración es «{}»: dilo con su año, periodo y modelo, como en justificantes/ (gesshtor json {nombre} 2025_0A_100).", como.unwrap_or(nombre))
    })?;
    if j.leible().is_none() {
        return Err(format!("La app no lee el modelo {} de {} (lee la Renta, el 303, el 130 y el 390, desde {}).", j.modelo, j.ejercicio, crate::dominio::PRIMER_ANIO));
    }
    let j = Justificante { ruta: ruta.to_path_buf(), ..j };
    let bytes = std::fs::read(ruta);
    let huella = bytes.as_deref().ok().map(Huella::de);
    let f = leer_bytes(&j, bytes.as_deref().map_err(|e| e.to_string()), &crate::dominio::fmt::ahora());
    Ok(Fuente { origen: ruta.display().to_string(), huella, ..f })
}

/// Lee un PDF ya cargado en memoria (o por qué no se ha podido): lo leído o por qué no, y cuándo se presentó.
fn leer_bytes(j: &Justificante, bytes: Result<&[u8], String>, cuando: &str) -> Fuente<Leido> {
    let Some(modelo) = j.leible() else { return Fuente::default() };
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bytes = bytes.map_err(|e| (String::new(), informe(&Fallo::Pdf(format!("No se puede leer {}: {e}", j.origen())), None, j)))?;
        let doc = pdf::desde_bytes(bytes).map_err(|e| (String::new(), informe(&Fallo::Pdf(e), None, j)))?;
        // Sin fecha (algunas portadas antiguas la ponen en otra columna y no se entiende), para elegir entre varias
        // del mismo periodo vale el nombre del PDF.
        let fecha = generico::presentada(&doc).unwrap_or_default();
        leer_pdf(&doc, j, modelo).map(|l| (fecha.clone(), l)).map_err(|e| (fecha, informe(&e, Some(&doc), j)))
    }));
    let (presentada, dato, error) = match r {
        Ok(Ok((fecha, l))) => (fecha, Some(l), None),
        Ok(Err((fecha, e))) => (fecha, None, Some(e)),
        Err(p) => {
            let que = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
            (String::new(), None, Some(informe(&Fallo::Roto(que), None, j)))
        }
    };
    Fuente { lector: VERSION_LECTOR, presentada, ..Fuente::nueva(j.origen(), cuando, dato, error) }
}

/// El informe de un fallo al leer un PDF: lo que dice el fallo y, del PDF, el trozo de donde sale (`fragmento`), con
/// los nombres que se ven en él tapados.
fn informe(f: &Fallo, doc: Option<&Documento>, j: &Justificante) -> Error {
    let lector = match j.leible() {
        Some(Modelo::M100) => format!("de PDF v{VERSION_LECTOR} (Renta v{})", modelo_100::VERSION),
        _ => format!("de PDF v{VERSION_LECTOR}"),
    };
    let e = Error::formato(f.codigo(), &f.donde(), f.to_string()).en(j.descripcion()).lector(lector).esperado(f.esperado()).encontrado(f.encontrado()).reproducir(j.reproducir());
    let Some(doc) = doc else { return e };
    let texto = doc.texto();
    let nombres = crate::dominio::fallo::nombres_en(&texto);
    let nombres: Vec<&str> = nombres.iter().map(String::as_str).collect();
    let e = if e.encontrado.is_empty() { e.encontrado(estructura(doc)) } else { e };
    e.fragmento(&fragmento(doc, f), &nombres)
}

/// Lo que se ve del PDF, para cuando no se sabe qué casilla falla: páginas, palabras, importes y su letra.
fn estructura(doc: &Documento) -> String {
    let palabras: Vec<&pdf::Palabra> = doc.paginas.iter().flat_map(|p| p.palabras()).collect();
    let importes = palabras.iter().filter(|w| generico::importe(&w.texto).is_some()).count();
    format!("{} páginas, {} palabras, {importes} importes («1.234,56»); letra de los importes rellenados: «{}»", doc.paginas.len(), palabras.len(), generico::letra_rellenada(doc))
}

/// El trozo del PDF de donde sale un fallo: las líneas de las casillas que no cuadran o, si no, el principio del
/// impreso (la primera página que no es la de la presentación).
fn fragmento(doc: &Documento, f: &Fallo) -> String {
    let lineas = || doc.paginas.iter().flat_map(|p| p.lineas.iter());
    let con = |numeros: &[String]| lineas().filter(|l| l.palabras.iter().any(|w| numeros.contains(&w.texto))).map(|l| l.texto()).collect::<Vec<_>>().join("\n");
    match f {
        Fallo::NoCuadra { modelo, casillas } => {
            let n = modelo.cifras();
            let v: Vec<String> = casillas.iter().map(|c| format!("{:0n$}", c.casilla)).collect();
            let ingreso = casillas.iter().any(|c| c.casilla == 700);
            let t = con(&v);
            if ingreso { format!("{t}\n{}", lineas().filter(|l| l.texto().contains("Resultado a ingresar")).map(|l| l.texto()).collect::<Vec<_>>().join("\n")) } else { t }
        }
        Fallo::Numeracion(_) => con(&["0595", "0609", "0610", "0670"].map(String::from)),
        Fallo::Bloque { casillas, .. } => con(&casillas.iter().map(|(c, _)| format!("{c:04}")).collect::<Vec<_>>()),
        // De la portada, solo sus etiquetas («Número de justificante: …»): lo demás es de la persona.
        Fallo::SinTipo(_) => generico::portada(doc).map(etiquetas).unwrap_or_default(),
        // Del impreso, sin lo que rellenó Hacienda (nombre, NIF, domicilio, importes…): su estructura.
        _ => {
            let portada = generico::portada(doc).map(|p| p.numero);
            doc.paginas.iter().find(|p| Some(p.numero) != portada).or(doc.paginas.first()).map(|p| generico::sin_lo_rellenado(doc, p)).unwrap_or_default()
        }
    }
}

/// Las etiquetas de una página (lo de antes de cada «:», con «…» detrás), sin sus valores. Los «:» de una dirección
/// («https://…») no son de una etiqueta.
fn etiquetas(p: &pdf::Pagina) -> String {
    let etiqueta = |t: String| t.split_once(':').filter(|(_, resto)| !resto.starts_with("//")).map(|(e, _)| format!("{}: …", e.trim()));
    let v: Vec<String> = p.lineas.iter().filter_map(|l| etiqueta(l.texto())).collect();
    v.join("\n")
}

/// El lector de cada modelo: aquí se elige.
fn leer_pdf(doc: &Documento, j: &Justificante, modelo: Modelo) -> Result<Leido, Fallo> {
    let periodo = Periodo::de(&j.periodo);
    match modelo {
        Modelo::M100 => modelo_100::leer(doc, j.ejercicio).map(|r| Leido::Renta(Box::new(r))),
        Modelo::M130 => formulario::leer::<M130>(doc, j.ejercicio, &periodo).map(Leido::Formulario),
        Modelo::M303 => formulario::leer::<M303>(doc, j.ejercicio, &periodo).map(Leido::Formulario),
        Modelo::M390 => formulario::leer::<M390>(doc, j.ejercicio, &periodo).map(Leido::Formulario),
    }
}

/// Lo leído de cada declaración: si hay varios PDF de la misma (una complementaria…), el último presentado. Si su
/// último intento falló pero antes se leyó bien, vale lo que se leyó bien (y el fallo se apunta igual).
fn lecturas(hechos: &[(&Justificante, Fuente<Leido>)]) -> Lecturas {
    let mut ultimo: BTreeMap<String, &(&Justificante, Fuente<Leido>)> = BTreeMap::new();
    for x in hechos {
        let (j, f) = x;
        if ultimo.get(&j.clave()).is_none_or(|(oj, of)| (of.presentada.as_str(), oj.nombre()) < (f.presentada.as_str(), j.nombre())) {
            ultimo.insert(j.clave(), x);
        }
    }
    let mut l = Lecturas::default();
    // Lo que falló de un PDF que no es el que vale (uno anterior del mismo periodo), también a la vista.
    for (j, f) in hechos {
        if f.error.is_some() && ultimo.get(&j.clave()).is_none_or(|(oj, _)| oj.ruta != j.ruta) {
            l.fallos.insert(format!("{} · {}", j.clave(), j.nombre()), f.error.clone().unwrap_or_default());
        }
    }
    for (k, (j, f)) in ultimo {
        // Un fallo se apunta aunque se quede lo que se leyó bien antes: lo de ahora no se entiende.
        if let Some(e) = &f.error {
            l.fallos.insert(k.clone(), e.clone());
        }
        match &f.dato {
            Some(Leido::Renta(r)) => {
                l.rentas.insert(j.ejercicio, (**r).clone());
            }
            Some(Leido::Formulario(x)) => {
                l.formularios.insert(k, crate::dominio::FormularioLeido { pdf: j.nombre(), ..x.clone() });
            }
            None if f.error.is_none() => {
                l.fallos.insert(k, Error::de("No se ha leído nada de este PDF."));
            }
            None => {}
        }
    }
    l
}

#[cfg(test)]
mod pruebas {
    //! `leer_todo` y lo leído en `fuentes/`.
    use super::*;
    use crate::dominio::FormularioLeido;

    /// El informe de un PDF que no se entiende no lleva lo que rellenó Hacienda (nombre, domicilio, importes): solo
    /// el impreso. Y de la portada, solo sus etiquetas.
    #[test]
    fn el_informe_no_lleva_lo_rellenado() {
        let linea = |top: f64, ps: &[(&str, &str)]| pdf::Linea {
            top,
            bottom: top + 8.0,
            x0: 0.0,
            x1: 500.0,
            palabras: ps.iter().map(|(t, f)| pdf::Palabra { texto: t.to_string(), top, bottom: top + 8.0, fuente: f.to_string(), tamano: 8.0, ..Default::default() }).collect(),
        };
        let (impreso, rellenado) = ("ArialMT", "ABCDEF+NimbusSanL-Regu");
        let portada = pdf::Pagina {
            numero: 1,
            lineas: vec![
                linea(10.0, &[("Información", rellenado), ("de", rellenado), ("la", rellenado), ("presentación", rellenado)]),
                linea(20.0, &[("Apellidos", rellenado), ("y", rellenado), ("Nombre:", rellenado), ("Pérez", rellenado), ("Inventada", rellenado)]),
            ],
            ..Default::default()
        };
        let hoja = pdf::Pagina {
            numero: 2,
            lineas: vec![
                linea(10.0, &[("Dirección", impreso), ("Avenida", rellenado), ("Mayor", rellenado), ("12", rellenado), ("Madrid", rellenado)]),
                linea(20.0, &[("Apellidos", impreso), ("INVENTADA", rellenado), ("PRUEBA", rellenado)]),
                linea(30.0, &[("Total", impreso), ("27", impreso), ("1.234,56", rellenado)]),
            ],
            ..Default::default()
        };
        let doc = pdf::Documento { paginas: vec![portada, hoja] };
        let j = de_nombre(Path::new("2025_1T_303_PRUEBA303A1T0001.pdf")).unwrap();
        let e = informe(&Fallo::SinDevengo(Modelo::M303), Some(&doc), &j);
        assert!(e.fragmento.contains("Dirección …") && e.fragmento.contains("Total 27 …"), "{}", e.fragmento);
        for no in ["Mayor", "Madrid", "INVENTADA", "1.234,56"] {
            assert!(!e.fragmento.contains(no), "«{no}» en: {}", e.fragmento);
        }
        let e = informe(&Fallo::SinTipo(Modelo::M303), Some(&doc), &j);
        assert_eq!(e.fragmento, "Apellidos y Nombre: …");
        // Sin importes no se sabe cuál es la letra de lo rellenado: entonces, nada del texto (la estructura ya va en
        // «Se encontró»). Y una dirección («en https:») no es una etiqueta.
        let hoja = pdf::Pagina { numero: 1, lineas: vec![linea(10.0, &[("Pérez", impreso), ("Inventada", impreso)]), linea(20.0, &[("CSV", impreso), ("PRUEBACSV", impreso), ("en", impreso), ("https://sede.agenciatributaria.gob.es", impreso)])], ..Default::default() };
        let doc = pdf::Documento { paginas: vec![hoja.clone()] };
        let e = informe(&Fallo::SinCasillas, Some(&doc), &j);
        assert!(e.fragmento.is_empty() && e.encontrado.contains("palabras"), "{e:?}");
        assert_eq!(etiquetas(&hoja), "");
    }

    /// Una carpeta de datos temporal con estos PDF (nombre → bytes).
    struct Carpeta(PathBuf);

    impl Carpeta {
        fn nueva(nombre: &str, pdfs: &[(&str, Vec<u8>)]) -> Self {
            let d = std::env::temp_dir().join(format!("panel-lectura-{nombre}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("justificantes")).unwrap();
            for (n, b) in pdfs {
                std::fs::write(d.join("justificantes").join(n), b).unwrap();
            }
            Carpeta(d)
        }
        /// Lee todo y devuelve lo leído y las veces que se ha llamado a `progreso` (con el total).
        fn leer(&self) -> (Lecturas, Vec<(usize, usize)>) {
            let mut p = Vec::new();
            let l = leer_todo(&self.0, &mut |h, t| p.push((h, t)));
            (l, p)
        }
        /// Los ficheros de `fuentes/`.
        fn fuentes(&self) -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(self.0.join("fuentes")).map(|it| it.map(|e| e.unwrap().file_name().into_string().unwrap()).collect()).unwrap_or_default();
            v.sort();
            v
        }
    }

    impl Drop for Carpeta {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn tambien_se_guardan_los_fallos() {
        let c = Carpeta::nueva("fallos", &[
            ("2025_1T_303_ROTO.pdf", b"esto no es un PDF".to_vec()),
            ("2024_0A_390_ROTO.pdf", b"%PDF-1.4 tampoco".to_vec()),
            ("2019_1T_303_VIEJO.pdf", b"anterior a 2020: ni se mira".to_vec()),
            ("2025_0A_720_OTRO.pdf", b"modelo que no se lee".to_vec()),
            ("2026-3T_gasto01.pdf", b"no es un justificante".to_vec()),
        ]);
        let (l, p) = c.leer();
        assert_eq!(p.last(), Some(&(2, 2)), "solo cuentan los que se leen: {p:?}");
        assert_eq!(l.fallos.keys().collect::<Vec<_>>(), ["2024-390", "2025-1T-303"]);
        assert!(l.formularios.is_empty() && l.rentas.is_empty());
        assert_eq!(c.fuentes(), ["aeat-2024-0A-390-ROTO.json", "aeat-2025-1T-303-ROTO.json"]);
        let f: Fuente<Leido> = almacen::leer_fuente(&c.0, "aeat-2025-1T-303-ROTO.json").unwrap();
        assert_eq!((f.origen.as_str(), f.lector, f.huella.as_ref().map(|h| h.tamano)), ("justificantes/2025_1T_303_ROTO.pdf", VERSION_LECTOR, Some(17)));
        assert!(f.dato.is_none() && f.error.as_ref() == Some(&l.fallos["2025-1T-303"]), "{f:?}");
        let e = &l.fallos["2025-1T-303"];
        assert_eq!((e.codigo.as_str(), e.fuente.as_str()), ("AEAT_PDF_NO_SE_ABRE", "PDF del modelo 303 del 1T de 2025"));
        assert!(e.reproducir.starts_with("gesshtor json justificantes/2025_1T_303_‹CSV›.pdf"), "{e:?}");
        // Al volver a arrancar no se intenta otra vez, pero el fallo sigue ahí.
        let (l2, p2) = c.leer();
        assert!(p2.is_empty(), "{p2:?}");
        assert_eq!(l2.fallos, l.fallos);
        // Si el PDF cambia (aunque tenga el mismo tamaño), se relee solo ese.
        std::fs::write(c.0.join("justificantes/2025_1T_303_ROTO.pdf"), b"esto no es un PDX").unwrap();
        let (_, p3) = c.leer();
        assert_eq!(p3.last(), Some(&(1, 1)));
        // Uno que se borra sale de las fuentes (y de los fallos).
        std::fs::remove_file(c.0.join("justificantes/2024_0A_390_ROTO.pdf")).unwrap();
        let (l4, p4) = c.leer();
        assert!(p4.is_empty());
        assert_eq!(l4.fallos.keys().collect::<Vec<_>>(), ["2025-1T-303"]);
        assert_eq!(c.fuentes(), ["aeat-2025-1T-303-ROTO.json"]);
        // Con otra versión del lector se relee.
        let vieja = Fuente { lector: VERSION_LECTOR + 1, ..f };
        almacen::guardar_fuente(&c.0, "aeat-2025-1T-303-ROTO.json", &vieja).unwrap();
        let (_, p5) = c.leer();
        assert_eq!(p5.last(), Some(&(1, 1)));
        // Sin carpeta de justificantes no pasa nada (ni se escribe ni se borra nada).
        std::fs::remove_dir_all(c.0.join("justificantes")).unwrap();
        let (l, p) = c.leer();
        assert!(p.is_empty() && l.fallos.is_empty());
        assert_eq!(c.fuentes(), ["aeat-2025-1T-303-ROTO.json"]);
    }

    #[test]
    fn un_fallo_al_releer_no_pisa_lo_leido() {
        let c = Carpeta::nueva("releer", &[("2025_1T_303_X.pdf", b"ahora no se puede leer".to_vec())]);
        let bueno = FormularioLeido { casillas: [(71, 10.0)].into(), justificante: "3030000000001".into(), ..Default::default() };
        // Leído bien con un lector anterior.
        let antes = Fuente { lector: VERSION_LECTOR - 1, presentada: "2025-04-15T10:00:00".into(), ..Fuente::nueva("justificantes/2025_1T_303_X.pdf", "2025-05-01T00:00:00", Some(Leido::Formulario(bueno.clone())), None) };
        almacen::guardar_fuente(&c.0, "aeat-2025-1T-303-X.json", &antes).unwrap();
        let (l, p) = c.leer();
        assert_eq!(p.last(), Some(&(1, 1)), "se relee");
        assert_eq!(l.formularios.get("2025-1T-303"), Some(&FormularioLeido { pdf: "2025_1T_303_X.pdf".into(), ..bueno.clone() }), "con lo de antes");
        assert_eq!(l.fallos.keys().collect::<Vec<_>>(), ["2025-1T-303"], "y el fallo de ahora, a la vista");
        let f: Fuente<Leido> = almacen::leer_fuente(&c.0, "aeat-2025-1T-303-X.json").unwrap();
        assert_eq!((f.lector, f.dato, f.error.is_some()), (VERSION_LECTOR, Some(Leido::Formulario(bueno)), true), "el error apuntado, y el dato de antes");
        assert_eq!(f.presentada, "2025-04-15T10:00:00", "con su fecha de presentación");
        assert!(c.leer().1.is_empty(), "y no se vuelve a intentar");
    }

    /// Nada falla sin que se vea: tampoco un PDF que no es el que vale de su periodo, ni no poder guardar lo leído.
    #[test]
    fn todos_los_fallos_a_la_vista() {
        let c = Carpeta::nueva("todos", &[("2025_1T_303_A.pdf", b"roto".to_vec()), ("2025_1T_303_B.pdf", b"roto".to_vec())]);
        std::fs::write(c.0.join("fuentes"), b"no es una carpeta").unwrap();
        let (l, _) = c.leer();
        let mut k: Vec<&str> = l.fallos.keys().map(String::as_str).collect();
        k.sort();
        assert_eq!(k, ["2025-1T-303", "2025-1T-303 · 2025_1T_303_A.pdf", "fuentes/aeat-2025-1T-303-A.json", "fuentes/aeat-2025-1T-303-B.json"]);
        assert_eq!(l.fallos["fuentes/aeat-2025-1T-303-A.json"].codigo, "FUENTE_NO_SE_GUARDA");
    }

    #[test]
    fn leer_un_fichero_suelto() {
        let c = Carpeta::nueva("suelto", &[("2025_1T_303_X.pdf", b"no es un PDF".to_vec()), ("303_2025_2T_Y.pdf", b"tampoco".to_vec())]);
        let ruta = c.0.join("justificantes/2025_1T_303_X.pdf");
        let f = leer_fichero(&ruta, None).unwrap();
        assert_eq!((f.origen, f.lector, f.dato.is_none(), f.huella.map(|h| h.tamano)), (ruta.display().to_string(), VERSION_LECTOR, true, Some(12)));
        assert!(f.error.unwrap().mensaje.contains("no se puede abrir"));
        assert!(leer_fichero(&c.0.join("justificantes/303_2025_2T_Y.pdf"), None).unwrap().error.is_some(), "con el nombre de antes, también");
        assert!(leer_fichero(Path::new("factura.pdf"), None).unwrap_err().contains("gesshtor json factura.pdf 2025_0A_100"));
        assert!(leer_fichero(Path::new("2025_0A_720_X.pdf"), None).unwrap_err().contains("no lee el modelo 720"));
        // Con otro nombre, diciendo de qué es.
        let f = leer_fichero(&c.0.join("justificantes/2025_1T_303_X.pdf"), Some("2024_0A_100")).unwrap();
        assert!(f.error.is_some() && f.origen.ends_with("2025_1T_303_X.pdf"));
        assert!(leer_fichero(Path::new("renta.pdf"), Some("2024")).unwrap_err().contains("«2024»"));
        assert!(!c.0.join("fuentes").exists(), "no escribe nada");
    }

    #[test]
    fn nombres() {
        assert_eq!(nombre_pdf(2026, "3T", "303", "ABC123"), "2026_3T_303_ABC123.pdf");
        let j = de_nombre(Path::new("justificantes/2026_3T_303_ABC123.pdf")).unwrap();
        assert_eq!((j.ejercicio, j.periodo.as_str(), j.modelo.as_str(), j.csv.as_str()), (2026, "3T", "303", "ABC123"));
        assert_eq!((j.clave().as_str(), j.fuente().as_str()), ("2026-3T-303", "aeat-2026-3T-303-ABC123.json"));
        assert_eq!(de_nombre(Path::new("2025_0A_100.PDF")).map(|j| (j.fuente(), j.csv)), Some(("aeat-2025-0A-100.json".into(), String::new())));
        // El nombre de antes (el modelo delante) ya no se lee: se cambia al abrir la carpeta.
        for n in ["303_2026_3T_ABC123.pdf", "2026-3T_gasto01.pdf", "2026_3T_30_X.pdf", "26_3T_303_X.pdf", "2026_3T_303_X.txt"] {
            assert!(de_nombre(Path::new(n)).is_none(), "{n}");
        }
        assert_eq!(nombre_de_antes("303_2026_3T_ABC123.pdf").as_deref(), Some("2026_3T_303_ABC123.pdf"));
        assert_eq!(nombre_de_antes("100_2025_0A.PDF").as_deref(), Some("2025_0A_100.pdf"));
        assert_eq!(nombre_de_antes("2026_3T_303_ABC123.pdf"), None);
    }

    #[test]
    fn los_nombres_de_antes_se_cambian_una_vez() {
        let c = Carpeta::nueva("antes", &[
            ("303_2025_1T_ROTO.pdf", b"esto no es un PDF".to_vec()),
            ("390_2024_0A_ROTO.pdf", b"%PDF-1.4 tampoco".to_vec()),
            ("2026-3T_gasto01.pdf", b"no es un justificante".to_vec()),
        ]);
        // Uno que ya está con el nombre nuevo no se pisa.
        std::fs::write(c.0.join("justificantes/2024_0A_390_ROTO.pdf"), b"el de ahora").unwrap();

        renombrar_antiguos(&c.0);
        let mut hay: Vec<String> = std::fs::read_dir(c.0.join("justificantes")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
        hay.sort();
        assert_eq!(hay, ["2024_0A_390_ROTO.pdf", "2025_1T_303_ROTO.pdf", "2026-3T_gasto01.pdf", "390_2024_0A_ROTO.pdf"]);
        assert_eq!(std::fs::read(c.0.join("justificantes/2024_0A_390_ROTO.pdf")).unwrap(), b"el de ahora");
        // Otra vez, nada más que hacer.
        renombrar_antiguos(&c.0);
        assert_eq!(std::fs::read_dir(c.0.join("justificantes")).unwrap().count(), 4);
    }
}
