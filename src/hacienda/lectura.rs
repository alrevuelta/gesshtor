//! Leer los PDF de Hacienda que hay en la carpeta de datos (`justificantes/`): las Rentas y las declaraciones
//! con casillas (303, 130, 390…). No habla con Hacienda: lee lo que ya está descargado.
//!
//! Lo leído se guarda en `cache.json` con la huella de cada PDF y la versión del lector: al arrancar solo se lee
//! lo nuevo o lo que se leyó con un lector anterior. Así arrancar es instantáneo.

use super::modelo::{Fallo, Modelo, Periodo};
use super::pdf::Documento;
use super::{cache, formulario, generico, modelo_100, pdf};
use super::{modelo_130::M130, modelo_303::M303, modelo_390::M390};
use crate::dominio::{FormularioLeido, Lecturas};
use std::path::{Path, PathBuf};

/// Súbelo cuando cambie lo que sale de algún lector: se releen solos todos los PDF.
pub const VERSION_LECTOR: u32 = 3;

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

/// Cambia una vez los nombres de antes («303_2026_3T_CSV.pdf») a los de ahora («2026_3T_303_CSV.pdf»), también en
/// `cache.json`, para no releerlos. Si ya hay uno con el nombre nuevo, no lo toca. Si no hay nombres de antes, no
/// hace nada: se llama al abrir la carpeta.
pub fn renombrar_antiguos(dir: &Path) {
    let carpeta = dir.join("justificantes");
    let Ok(it) = std::fs::read_dir(&carpeta) else { return };
    let mut cambiados = Vec::new();
    for ruta in it.filter_map(|e| e.ok().map(|e| e.path())) {
        let Some(viejo) = ruta.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
        let Some(nuevo) = nombre_de_antes(&viejo) else { continue };
        let destino = carpeta.join(&nuevo);
        if !destino.exists() && std::fs::rename(&ruta, &destino).is_ok() {
            cambiados.push((viejo, nuevo));
        }
    }
    let mut c = cache::cargar(dir);
    let mut cambio = false;
    for (viejo, nuevo) in cambiados {
        if let Some(mut e) = c.pdfs.remove(&viejo) {
            if let cache::Leido::Formulario(f) = &mut e.leido {
                f.pdf = nuevo.clone();
            }
            c.pdfs.insert(nuevo, e);
            cambio = true;
        }
    }
    if cambio {
        let _ = cache::guardar(dir, &c);
    }
}

/// Lee todo lo que haga falta de la carpeta y lo devuelve. `progreso(hechos, total)` se llama por cada PDF que
/// de verdad hay que leer (los de la caché no cuentan). Se puede llamar desde otro hilo.
///
/// Solo se lee lo nuevo, lo que ha cambiado de tamaño o lo que se leyó con otra `VERSION_LECTOR`; lo demás sale
/// de `cache.json`, que se reescribe solo si algo cambia. Los PDF se leen en paralelo.
pub fn leer_todo(dir: &Path, progreso: &mut dyn FnMut(usize, usize)) -> Lecturas {
    let js: Vec<Justificante> = justificantes(dir).into_iter().filter(|j| j.leible().is_some()).collect();
    let mut c = cache::cargar(dir);
    let antes = c.pdfs.len();
    // Lo que ya no está en la carpeta se olvida.
    let nombres: std::collections::HashSet<String> = js.iter().map(Justificante::nombre).collect();
    c.pdfs.retain(|n, _| nombres.contains(n));
    let mut cambio = c.pdfs.len() != antes;
    let por_leer: Vec<(&Justificante, Modelo, u64)> = js
        .iter()
        .filter_map(|j| Some((j, j.leible()?, std::fs::metadata(&j.ruta).map(|m| m.len()).unwrap_or(0))))
        .filter(|(j, _, tam)| c.pdfs.get(&j.nombre()).is_none_or(|e| e.tamano != *tam))
        .collect();
    if !por_leer.is_empty() {
        c.pdfs.extend(leer_varios(&por_leer, progreso));
        cambio = true;
    }
    if cambio {
        let _ = cache::guardar(dir, &c);
    }
    lecturas(&js, &c)
}

/// Lee los PDF en varios hilos; `progreso` se llama desde este, cada vez que acaba uno.
fn leer_varios(lista: &[(&Justificante, Modelo, u64)], progreso: &mut dyn FnMut(usize, usize)) -> Vec<(String, cache::Entrada)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let total = lista.len();
    progreso(0, total);
    let hilos = std::thread::available_parallelism().map_or(4, |n| n.get()).min(total);
    let siguiente = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut hechos = Vec::with_capacity(total);
    std::thread::scope(|s| {
        for _ in 0..hilos {
            let (tx, siguiente) = (tx.clone(), &siguiente);
            s.spawn(move || {
                while let Some((j, modelo, tamano)) = lista.get(siguiente.fetch_add(1, Ordering::Relaxed)) {
                    let (presentada, leido) = leer_uno(j, *modelo);
                    if tx.send((j.nombre(), cache::Entrada { tamano: *tamano, presentada, leido })).is_err() {
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

/// Lee un PDF: (cuándo se presentó, lo leído o por qué no se ha podido).
fn leer_uno(j: &Justificante, modelo: Modelo) -> (String, cache::Leido) {
    use cache::Leido;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let doc = pdf::abrir(&j.ruta).map_err(|e| (String::new(), Fallo::Pdf(e)))?;
        let fecha = generico::presentada(&doc).unwrap_or_default();
        leer_pdf(&doc, j, modelo).map(|l| (fecha.clone(), l)).map_err(|e| (fecha, e))
    }));
    match r {
        Ok(Ok(x)) => x,
        Ok(Err((fecha, e))) => (fecha, Leido::Fallo(e.to_string())),
        Err(_) => (String::new(), Leido::Fallo(Fallo::Roto.to_string())),
    }
}

/// El lector de cada modelo: aquí se elige.
fn leer_pdf(doc: &Documento, j: &Justificante, modelo: Modelo) -> Result<cache::Leido, Fallo> {
    use cache::Leido;
    let periodo = Periodo::de(&j.periodo);
    let con_pdf = |f: FormularioLeido| Leido::Formulario(FormularioLeido { pdf: j.nombre(), ..f });
    match modelo {
        Modelo::M100 => modelo_100::leer(doc, j.ejercicio).map(|r| Leido::Renta(Box::new(r))),
        Modelo::M130 => formulario::leer::<M130>(doc, j.ejercicio, &periodo).map(con_pdf),
        Modelo::M303 => formulario::leer::<M303>(doc, j.ejercicio, &periodo).map(con_pdf),
        Modelo::M390 => formulario::leer::<M390>(doc, j.ejercicio, &periodo).map(con_pdf),
    }
}

/// Lo leído de cada declaración: si hay varios PDF de la misma (una complementaria…), el último presentado.
fn lecturas(js: &[Justificante], c: &cache::Cache) -> Lecturas {
    use std::collections::BTreeMap;
    let mut ultimo: BTreeMap<String, (&Justificante, &cache::Entrada)> = BTreeMap::new();
    for j in js {
        let Some(e) = c.pdfs.get(&j.nombre()) else { continue };
        let k = j.clave();
        if ultimo.get(&k).is_none_or(|(oj, oe)| (oe.presentada.as_str(), oj.nombre()) < (e.presentada.as_str(), j.nombre())) {
            ultimo.insert(k, (j, e));
        }
    }
    let mut l = Lecturas::default();
    for (k, (j, e)) in ultimo {
        match &e.leido {
            cache::Leido::Renta(r) => {
                l.rentas.insert(j.ejercicio, (**r).clone());
            }
            cache::Leido::Formulario(f) => {
                l.formularios.insert(k, f.clone());
            }
            cache::Leido::Fallo(m) => {
                l.fallos.insert(k, m.clone());
            }
        }
    }
    l
}


#[cfg(test)]
mod pruebas {
    //! `leer_todo` y `cache.json`.
    use super::*;
    use serde_json::Value;

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
    }

    impl Drop for Carpeta {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn la_cache_guarda_tambien_los_fallos() {
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
        assert!(c.0.join("cache.json").is_file());
        // Al volver a arrancar no se intenta otra vez, pero el fallo sigue ahí.
        let (l2, p2) = c.leer();
        assert!(p2.is_empty(), "{p2:?}");
        assert_eq!(l2.fallos, l.fallos);
        // Si el PDF cambia, se relee solo ese.
        std::fs::write(c.0.join("justificantes/2025_1T_303_ROTO.pdf"), b"otro contenido, otro tamano").unwrap();
        let (_, p3) = c.leer();
        assert_eq!(p3.last(), Some(&(1, 1)));
        // Uno que se borra sale de la caché (y de los fallos).
        std::fs::remove_file(c.0.join("justificantes/2024_0A_390_ROTO.pdf")).unwrap();
        let (l4, p4) = c.leer();
        assert!(p4.is_empty());
        assert_eq!(l4.fallos.keys().collect::<Vec<_>>(), ["2025-1T-303"]);
        let cache: Value = serde_json::from_slice(&std::fs::read(c.0.join("cache.json")).unwrap()).unwrap();
        assert_eq!(cache["pdfs"].as_object().unwrap().len(), 1);
        // Con otra versión del lector se relee todo.
        let mut vieja = cache.clone();
        vieja["version"] = Value::from(VERSION_LECTOR + 1);
        std::fs::write(c.0.join("cache.json"), serde_json::to_vec(&vieja).unwrap()).unwrap();
        let (_, p5) = c.leer();
        assert_eq!(p5.last(), Some(&(1, 1)));
        // Sin carpeta de justificantes no pasa nada (ni se escribe la caché).
        let vacia = Carpeta::nueva("vacia", &[]);
        std::fs::remove_dir(vacia.0.join("justificantes")).unwrap();
        let (l, p) = vacia.leer();
        assert!(p.is_empty() && l.fallos.is_empty() && !vacia.0.join("cache.json").exists());
    }

    /// Los PDF de `pruebas/pdf/`, hechos por el servicio de Hacienda que valida e imprime declaraciones con datos
    /// inventados (ver `generar.py`), se leen enteros: del nombre del fichero a las casillas, y salen las que se le
    /// mandaron (`casos.json`).
    #[test]
    fn lee_los_pdf_de_hacienda_de_prueba() {
        use crate::dominio::{Casillas, Valor};
        // Los % que imprime el 303 aunque no se le manden: los tipos de IVA y de recargo.
        const PORCENTAJES_303: [u32; 11] = [2, 5, 8, 17, 20, 23, 151, 154, 157, 166, 169];
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("pruebas/pdf");
        let casos: Vec<Value> = serde_json::from_slice(&std::fs::read(dir.join("casos.json")).unwrap()).unwrap();
        let pdfs: Vec<(&str, Vec<u8>)> = casos.iter().map(|c| c["pdf"].as_str().unwrap()).map(|n| (n, std::fs::read(dir.join(n)).unwrap())).collect();
        let c = Carpeta::nueva("hacienda", &pdfs);
        let (l, _) = c.leer();
        assert!(l.fallos.is_empty(), "{:?}", l.fallos);
        assert_eq!(l.formularios.len(), casos.len());
        for caso in &casos {
            let nombre = caso["pdf"].as_str().unwrap();
            let j = de_nombre(Path::new(nombre)).unwrap();
            let f = &l.formularios[&j.clave()];
            assert_eq!(f.pdf, nombre);
            let mandado: Casillas = caso["casillas"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), v.as_f64().unwrap())).collect();
            for (k, v) in &mandado {
                assert!((f.casillas.v(*k) - v).abs() < 0.005, "{nombre}, casilla {k}: leído {}, mandado {v}", f.casillas.v(*k));
            }
            let de_mas: Vec<_> = f.casillas.iter().filter(|(k, v)| v.abs() >= 0.005 && !mandado.contains_key(k) && !(j.modelo == "303" && PORCENTAJES_303.contains(k))).collect();
            assert!(de_mas.is_empty(), "{nombre}: leído sin haberlo mandado {de_mas:?}");
            // No se han presentado: no llevan la página de la presentación, que es la que dice cómo se presentó.
            assert_eq!(f.tipo, None, "{nombre}");
        }
    }

    #[test]
    fn nombres() {
        assert_eq!(nombre_pdf(2026, "3T", "303", "ABC123"), "2026_3T_303_ABC123.pdf");
        let j = de_nombre(Path::new("justificantes/2026_3T_303_ABC123.pdf")).unwrap();
        assert_eq!((j.ejercicio, j.periodo.as_str(), j.modelo.as_str(), j.csv.as_str()), (2026, "3T", "303", "ABC123"));
        assert_eq!(j.clave(), "2026-3T-303");
        assert_eq!(de_nombre(Path::new("2025_0A_100.PDF")).map(|j| j.csv), Some(String::new()));
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
        // Leídos con el nombre de antes (así estaba la caché): se cambian en la caché también.
        let mut cache = cache::cargar(&c.0);
        let formulario = |pdf: &str| cache::Leido::Formulario(FormularioLeido { pdf: pdf.into(), ..Default::default() });
        cache.pdfs.insert("303_2025_1T_ROTO.pdf".into(), cache::Entrada { tamano: 17, presentada: String::new(), leido: formulario("303_2025_1T_ROTO.pdf") });
        cache::guardar(&c.0, &cache).unwrap();
        // Uno que ya está con el nombre nuevo no se pisa.
        std::fs::write(c.0.join("justificantes/2024_0A_390_ROTO.pdf"), b"el de ahora").unwrap();

        renombrar_antiguos(&c.0);
        let mut hay: Vec<String> = std::fs::read_dir(c.0.join("justificantes")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
        hay.sort();
        assert_eq!(hay, ["2024_0A_390_ROTO.pdf", "2025_1T_303_ROTO.pdf", "2026-3T_gasto01.pdf", "390_2024_0A_ROTO.pdf"]);
        assert_eq!(std::fs::read(c.0.join("justificantes/2024_0A_390_ROTO.pdf")).unwrap(), b"el de ahora");
        let cache = cache::cargar(&c.0);
        assert_eq!(cache.pdfs.keys().collect::<Vec<_>>(), ["2025_1T_303_ROTO.pdf"]);
        assert!(matches!(&cache.pdfs["2025_1T_303_ROTO.pdf"].leido, cache::Leido::Formulario(f) if f.pdf == "2025_1T_303_ROTO.pdf"));
        // Otra vez, nada más que hacer.
        renombrar_antiguos(&c.0);
        assert_eq!(std::fs::read_dir(c.0.join("justificantes")).unwrap().count(), 4);
    }
}
