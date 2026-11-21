//! Lo que no se ha podido leer o traer la última vez (`Datos::fallos`), entero, para copiarlo y pegárselo a un LLM
//! (el informe de cada fallo, ver `dominio::fallo`). Se abre con `!` desde cualquier pantalla; mientras haya
//! fallos, la barra de abajo lo recuerda. `c` lo copia al portapapeles (OSC 52, si la terminal lo deja) y lo guarda
//! en `informe-de-fallos.txt`, en la carpeta de la persona.
use super::widgets::{self, tema};
use super::Ctx;
use crate::dominio::{almacen, fallo};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::io::Write;

/// El fichero donde se guarda el informe, en la carpeta de la persona.
pub const FICHERO: &str = "informe-de-fallos.txt";

/// La ventana de los fallos: el informe entero, por dónde va y lo que ha pasado al copiarlo.
pub struct Fallos {
    texto: String,
    desde: u16,
    hecho: Option<String>,
}

/// Cuántas cosas no se han podido leer o traer.
pub fn cuantos(ctx: &Ctx) -> usize {
    ctx.datos.fallos().count()
}

/// El aviso de la barra de abajo, si hay fallos.
pub fn aviso(ctx: &Ctx) -> Option<String> {
    match cuantos(ctx) {
        0 => None,
        1 => Some("Una cosa no se ha podido leer o traer · ! para verla y copiar el informe".into()),
        n => Some(format!("{n} cosas no se han podido leer o traer · ! para verlas y copiar el informe")),
    }
}

/// El informe de todos los fallos, con los nombres de la persona tapados (los de su perfil y su certificado).
pub fn informe(ctx: &Ctx) -> String {
    let p = &ctx.datos.perfil;
    let mut nombres = vec![p.nombre.clone(), p.nombre_pila.clone(), p.apellidos.clone()];
    if let Some(c) = ctx.certificado.as_deref() {
        nombres.push(c.nombre_completo());
    }
    let nombres: Vec<&str> = nombres.iter().map(String::as_str).collect();
    fallo::informes(ctx.datos.fallos(), &nombres)
}

impl Fallos {
    pub fn new(ctx: &Ctx) -> Self {
        let texto = if cuantos(ctx) == 0 { "Nada ha fallado: todo se ha leído y traído bien.".to_string() } else { informe(ctx) };
        Fallos { texto, desde: 0, hecho: None }
    }

    /// Atiende una tecla; true si hay que cerrar.
    pub fn tecla(&mut self, k: KeyEvent, ctx: &Ctx) -> bool {
        let lineas = self.texto.lines().count() as u16;
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('!') => return true,
            KeyCode::Down => self.desde = (self.desde + 1).min(lineas.saturating_sub(1)),
            KeyCode::Up => self.desde = self.desde.saturating_sub(1),
            KeyCode::PageDown => self.desde = (self.desde + 10).min(lineas.saturating_sub(1)),
            KeyCode::PageUp => self.desde = self.desde.saturating_sub(10),
            KeyCode::Home => self.desde = 0,
            KeyCode::Char('c') if cuantos(ctx) > 0 => self.hecho = Some(copiar(&self.texto, ctx)),
            _ => {}
        }
        false
    }

    /// La ventana, encima de todo salvo la barra de abajo: lo que ha pasado al copiarlo y el informe tal cual (se
    /// copia lo mismo que se ve), solo con colores: la presentación y los nombres de cada dato en gris, lo que dice
    /// cada fallo en TINTA (el código y qué pasa, en negrita) y los fragmentos en gris. Lo que no cabe en una fila
    /// sigue debajo con sangría.
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        let area = Rect { height: area.height.saturating_sub(2), ..widgets::centrado(area, area.width.saturating_sub(4), area.height) };
        let dentro = widgets::ventana(f, area, "Lo que no se ha podido leer o traer");
        let w = dentro.width as usize;
        let mut v: Vec<Line> = Vec::new();
        if let Some(h) = &self.hecho {
            let color = if h.contains("No se ha podido") { tema::AVISO } else { tema::BIEN };
            v.extend(partir(&[(h.clone(), Style::new().fg(color).add_modifier(Modifier::BOLD))], w, 0));
            v.push(Line::from(""));
        }
        // Lo de antes del primer «--- 1 de N ---» es la presentación.
        let mut en_fallo = false;
        for (i, l) in self.texto.lines().enumerate() {
            en_fallo |= es_separador(l);
            if i >= self.desde as usize {
                let (trozos, sangria) = estilo(l, en_fallo);
                v.extend(partir(&trozos, w, sangria));
            }
        }
        f.render_widget(Paragraph::new(v), dentro);
        widgets::pie_ventana(f, area, widgets::linea_atajos(&[("c", "copiar y guardar"), ("↑ ↓ PgUp PgDn", "moverse"), ("Esc", "cerrar")]));
    }
}

/// «--- 1 de 2 ---»: donde empieza cada fallo.
fn es_separador(l: &str) -> bool {
    l.starts_with("--- ") && l.ends_with(" ---")
}

/// Los trozos de una línea del informe, cada uno con su estilo, y la sangría de lo que no quepa en una fila. No
/// cambia ni un carácter: solo los colores.
fn estilo(l: &str, en_fallo: bool) -> (Vec<(String, Style)>, usize) {
    let gris = tema::apagado();
    let tinta = Style::new().fg(tema::TINTA);
    if !en_fallo {
        return (vec![(l.to_string(), gris)], 0);
    }
    if es_separador(l) {
        return (vec![(l.to_string(), gris)], 0);
    }
    // Los fragmentos (y lo que va en varias líneas) van sangrados: en gris.
    if l.starts_with("    ") {
        return (vec![(l.to_string(), gris)], 4);
    }
    if l.ends_with(':') && !l.contains(": ") {
        return (vec![(l.to_string(), gris)], 0);
    }
    match l.split_once(": ") {
        // «Código: …»: el nombre, corto, en gris; lo que dice, en TINTA (el código y qué pasa, en negrita).
        Some((nombre, valor)) if nombre.chars().count() <= 40 && !nombre.contains("//") => {
            let fuerte = nombre == "Código" || nombre == "Qué pasa";
            let st = if fuerte { tinta.add_modifier(Modifier::BOLD) } else { tinta };
            (vec![(format!("{nombre}: "), gris), (valor.to_string(), st)], 2)
        }
        _ => (vec![(l.to_string(), tinta)], 0),
    }
}

/// Parte unos trozos (cada uno con su estilo) en filas de `w` columnas por los espacios, con `sangria` en las de
/// después de la primera; una palabra más larga que la fila, cortada donde llegue. Sin quitar nada más que el
/// espacio donde se parte. También para la ventana de los documentos.
pub(super) fn partir(trozos: &[(String, Style)], w: usize, sangria: usize) -> Vec<Line<'static>> {
    let w = w.max(sangria + 8);
    let mut filas: Vec<Line<'static>> = Vec::new();
    let mut fila: Vec<Span<'static>> = Vec::new();
    let mut largo = 0;
    // El primero de todos va sin espacio delante; después, cada palabra lleva el suyo (salvo al empezar fila).
    let mut primera = true;
    for (texto, st) in trozos {
        for (j, palabra) in texto.split(' ').enumerate() {
            // Entre palabras del mismo trozo hay un espacio; entre trozos, no (lo lleva el trozo).
            let espacio = j > 0 && !primera;
            let n = palabra.chars().count();
            if espacio && largo > sangria && largo + 1 + n > w {
                filas.push(Line::from(std::mem::take(&mut fila)));
                fila.push(Span::raw(" ".repeat(sangria)));
                largo = sangria;
            } else if espacio {
                fila.push(Span::styled(" ", *st));
                largo += 1;
            }
            // Una palabra que no cabe ni sola: a trozos.
            let mut resto: Vec<char> = palabra.chars().collect();
            while largo + resto.len() > w && !resto.is_empty() {
                let cabe = w.saturating_sub(largo).max(1);
                let (a, b) = resto.split_at(cabe.min(resto.len()));
                fila.push(Span::styled(a.iter().collect::<String>(), *st));
                filas.push(Line::from(std::mem::take(&mut fila)));
                fila.push(Span::raw(" ".repeat(sangria)));
                largo = sangria;
                resto = b.to_vec();
            }
            if !resto.is_empty() {
                largo += resto.len();
                fila.push(Span::styled(resto.into_iter().collect::<String>(), *st));
            }
            primera = false;
        }
    }
    filas.push(Line::from(fila));
    filas
}

/// Copia el informe al portapapeles (con OSC 52: lo entienden iTerm2, kitty, WezTerm, Alacritty, Windows
/// Terminal…; la Terminal del Mac, no) y lo guarda en la carpeta de la persona. Dice qué ha hecho.
fn copiar(texto: &str, ctx: &Ctx) -> String {
    let ruta = ctx.dir.join(FICHERO);
    if ctx.sin_disco {
        return format!("(prueba) Copiaría el informe y lo guardaría en {}.", ruta.display());
    }
    let mut salida = std::io::stdout();
    let _ = write!(salida, "\x1b]52;c;{}\x07", crate::dominio::fmt::base64(texto.as_bytes())).and_then(|_| salida.flush());
    match almacen::escribir_atomico(&ruta, texto.as_bytes()) {
        Ok(()) => format!("Copiado al portapapeles (si tu terminal lo deja) y guardado en {}. Revísalo antes de pegarlo.", ruta.display()),
        Err(e) => format!("Copiado al portapapeles (si tu terminal lo deja). No se ha podido guardar en {}: {e}. Revísalo antes de pegarlo.", ruta.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::prueba::{app_con, cargar, caso, pantalla, teclas};
    use crate::dominio::fuente::Error;


    /// Con fallos, la barra de abajo lo dice siempre; `!` los enseña enteros, con los datos de la persona tapados, y
    /// `c` los copia.
    #[test]
    fn los_fallos_se_ven_y_se_copian() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        let e = Error::formato("TGSS_RECIBOS_SIN_LISTA", "src/tgss/importass.rs · leer_recibos", "No encuentro tus recibos en la página.")
            .en("Importass: https://portal.seg-social.gob.es/x/historialRecibosRETA")
            .fragmento(&format!("Hola, {}", d.perfil.nombre), &[]);
        d.fallos.insert("tgss.json".into(), e);
        let mut a = app_con(d, &dir);
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Una cosa no se ha podido leer o traer · ! para verla"), "{p}");
        teclas(&mut a, "a2");
        assert!(pantalla(&mut a, 120, 36).contains("! para verla"), "en cualquier pantalla");
        teclas(&mut a, "!");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Código: TGSS_RECIBOS_SIN_LISTA") && p.contains("src/tgss/importass.rs · leer_recibos") && p.contains("Hola, ‹nombre›"), "{p}");
        assert!(!p.contains(&a.ctx.datos.perfil.nombre), "{p}");
        teclas(&mut a, "c");
        assert!(pantalla(&mut a, 120, 36).contains("informe-de-fallos.txt"));
        // Las letras de navegar no navegan con la ventana abierta; Esc la cierra.
        teclas(&mut a, "r");
        assert_eq!(a.titulo_activa(), "Gastos");
        teclas(&mut a, "<esc>");
        assert!(!pantalla(&mut a, 120, 36).contains("Código: TGSS"));
        // Sin fallos, ni aviso.
        let mut a = app_con(cargar(&dir), &dir);
        assert!(!pantalla(&mut a, 120, 36).contains("no se ha podido leer o traer"));
    }
}
