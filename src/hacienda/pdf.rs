//! Texto con posiciones de un PDF: lo único que la app necesita de la librería de PDF. Si un día hay que
//! cambiar de librería, solo cambia este fichero.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Un PDF ya extraído: sus páginas, con las palabras y las líneas en coordenadas de página (puntos, y hacia abajo).
/// Se puede guardar en JSON (sin el PDF) para las pruebas.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Documento {
    pub paginas: Vec<Pagina>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Pagina {
    /// Desde 1.
    pub numero: usize,
    pub ancho: f64,
    pub alto: f64,
    /// Las líneas de arriba abajo, cada una con sus palabras de izquierda a derecha.
    pub lineas: Vec<Linea>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Linea {
    pub top: f64,
    pub bottom: f64,
    pub x0: f64,
    pub x1: f64,
    pub palabras: Vec<Palabra>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Palabra {
    pub texto: String,
    pub x0: f64,
    pub x1: f64,
    pub top: f64,
    pub bottom: f64,
    /// Fuente y tamaño del primer carácter (los números de casilla suelen ir en otra fuente o tamaño).
    pub fuente: String,
    pub tamano: f64,
}

impl Linea {
    /// El texto de la línea: sus palabras separadas por un espacio.
    pub fn texto(&self) -> String {
        self.palabras.iter().map(|p| p.texto.as_str()).collect::<Vec<_>>().join(" ")
    }
}

impl Pagina {
    pub fn texto(&self) -> String {
        self.lineas.iter().map(Linea::texto).collect::<Vec<_>>().join("\n")
    }
    /// Todas las palabras de la página, en orden de lectura.
    pub fn palabras(&self) -> impl Iterator<Item = &Palabra> {
        self.lineas.iter().flat_map(|l| l.palabras.iter())
    }
}

impl Documento {
    pub fn texto(&self) -> String {
        self.paginas.iter().map(Pagina::texto).collect::<Vec<_>>().join("\n\n")
    }
}

/// Extrae un PDF del disco.
pub fn abrir(ruta: &Path) -> Result<Documento, String> {
    let bytes = std::fs::read(ruta).map_err(|e| format!("No se puede leer {}: {e}", ruta.display()))?;
    desde_bytes(&bytes)
}

/// ¿Está escrita en diagonal? (ni en horizontal ni en vertical: su matriz gira y no en un ángulo recto)
fn en_diagonal(c: &pdfplumber::Char) -> bool {
    let (a, b) = (c.ctm[0], c.ctm[1]);
    let m = a.hypot(b);
    m > 0.0 && a.abs() > 0.05 * m && b.abs() > 0.05 * m
}

pub fn desde_bytes(bytes: &[u8]) -> Result<Documento, String> {
    use pdfplumber::{PageObject, Pdf, TextOptions};
    let pdf = Pdf::open_bytes(bytes, None).map_err(|e| format!("El PDF no se puede abrir: {e}"))?;
    let mut paginas = Vec::new();
    for (i, p) in pdf.pages_iter().enumerate() {
        let p = p.map_err(|e| format!("La página {} no se puede leer: {e}", i + 1))?;
        // Lo escrito en diagonal es una marca de agua («BORRADOR», en los PDF de prueba de Hacienda): no se lee.
        let opciones = TextOptions::default();
        let lineas = if p.chars().iter().any(en_diagonal) {
            p.filter(|o| !matches!(o, PageObject::Char(c) if en_diagonal(c))).extract_text_lines(&opciones)
        } else {
            p.extract_text_lines(&opciones)
        };
        let lineas = lineas
            .into_iter()
            .map(|l| Linea {
                top: l.bbox.top,
                bottom: l.bbox.bottom,
                x0: l.bbox.x0,
                x1: l.bbox.x1,
                palabras: l
                    .words
                    .iter()
                    .map(|w| Palabra {
                        texto: w.text.clone(),
                        x0: w.bbox.x0,
                        x1: w.bbox.x1,
                        top: w.bbox.top,
                        bottom: w.bbox.bottom,
                        fuente: w.chars.first().map(|c| c.fontname.clone()).unwrap_or_default(),
                        tamano: w.chars.first().map(|c| c.size).unwrap_or(0.0),
                    })
                    .collect(),
            })
            .collect();
        paginas.push(Pagina { numero: i + 1, ancho: p.width(), alto: p.height(), lineas });
    }
    Ok(Documento { paginas })
}
