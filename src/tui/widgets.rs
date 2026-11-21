//! Piezas comunes: los colores (`tema`), las del estilo C2 (puntos guía, banda, sello, barra fina, la cajita del
//! próximo plazo, las ventanas encima), formularios, un selector con búsqueda y la ventana de confirmar.
//!
//! Lo básico del estilo C2, para cualquier pantalla:
//! - Sin cajas: se separa con aire. Los títulos de bloque, con `titulo_bloque` (gris y en minúscula).
//! - Las cifras, en su columna y a la derecha; las que importan, con `guia` (puntos guía) y la principal en una
//!   `banda`.
//! - El color de la sección en la que estás: `tema::acento()`. La fila elegida de una lista: `tema::seleccion()`.
//! - Las ventanas que salen encima, con `ventana` (nunca `Clear` a secas: deja agujeros sin el fondo).
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;

/// Los colores. La app pinta el fondo de toda la pantalla (FONDO, con la letra en TINTA): se ve igual en cualquier
/// terminal.
pub mod tema {
    use ratatui::style::{Color, Modifier, Style};
    use std::cell::Cell;

    /// El fondo de toda la pantalla.
    pub const FONDO: Color = Color::Rgb(0x15, 0x13, 0x1c);
    /// El texto normal.
    pub const TINTA: Color = Color::Rgb(0xec, 0xe8, 0xf4);
    /// Etiquetas y lo secundario.
    pub const GRIS: Color = Color::Rgb(0x9a, 0x94, 0xaa);
    /// Los puntos guía y lo que es solo decoración (no para texto que haya que leer).
    pub const PUNTOS: Color = Color::Rgb(0x5a, 0x54, 0x6b);
    /// Las líneas finas: el borde de las ventanas, la cajita del plazo, un tramo neutro de una barra.
    pub const MARCO: Color = Color::Rgb(0x6f, 0x69, 0x87);

    /// A tu favor, cuadra, ✓.
    pub const BIEN: Color = Color::Rgb(0x7e, 0xe0, 0xa1);
    /// De más, pendiente.
    pub const AVISO: Color = Color::Rgb(0xff, 0xb8, 0x6b);
    /// Un error.
    pub const MAL: Color = Color::Rgb(0xff, 0x8a, 0x80);
    /// Azul de apoyo (un tramo de una barra, lo adelantado).
    pub const AZUL: Color = Color::Rgb(0x8f, 0xb8, 0xff);

    /// Fondos suaves para destacar (bandas y sellos): lo que es tuyo…
    pub const SUAVE_BIEN: Color = Color::Rgb(0x1d, 0x2f, 0x28);
    /// …lo que pagas (en Autónomo)…
    pub const SUAVE_NARANJA: Color = Color::Rgb(0x33, 0x24, 0x1a);
    /// …y el sello «de más».
    pub const SUAVE_AVISO: Color = Color::Rgb(0x33, 0x28, 0x1a);
    /// El fondo de la barra de las teclas, abajo: un poco más claro que FONDO.
    pub const BARRA: Color = Color::Rgb(0x28, 0x24, 0x36);

    /// El color de cada sección: la letra del menú, la pestaña activa y lo destacado de esa sección.
    pub const RESUMEN: Color = Color::Rgb(0xb6, 0x9c, 0xff);
    pub const EMPLEO: Color = Color::Rgb(0x5f, 0xd4, 0xc4);
    pub const AUTONOMO: Color = Color::Rgb(0xff, 0x9f, 0x5a);
    pub const PATRIMONIO: Color = Color::Rgb(0x9b, 0xd7, 0x7a);
    pub const UTILIDADES: Color = Color::Rgb(0xa3, 0xb1, 0xc6);

    /// El color de una sección, por su letra del menú (otra letra: el de Resumen).
    pub fn seccion(tecla: char) -> Color {
        match tecla {
            'e' => EMPLEO,
            'a' => AUTONOMO,
            'p' => PATRIMONIO,
            'u' => UTILIDADES,
            _ => RESUMEN,
        }
    }

    thread_local! {
        /// La sección que se está pintando: la pone `App::dibujar` (y `App::entrar`) antes de pintar.
        static ACTIVA: Cell<char> = const { Cell::new('r') };
    }

    /// Dice qué sección se va a pintar (su letra): desde ahí, `acento()` es su color. Solo lo llama la app.
    pub fn poner_seccion(tecla: char) {
        ACTIVA.with(|s| s.set(tecla));
    }

    /// El color de la sección en la que estás: la pestaña activa, lo destacado de esa sección, la fila elegida…
    /// Sirve en cualquier pantalla y en cualquier pieza sin pasarlo de mano en mano.
    pub fn acento() -> Color {
        seccion(ACTIVA.with(Cell::get))
    }

    /// `a` mezclado con `b`: `t` de `a` (de 0 a 1) y el resto de `b`. Si alguno no es RGB, `a`.
    pub fn mezcla(a: Color, b: Color, t: f32) -> Color {
        match (a, b) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let m = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
                Color::Rgb(m(r1, r2), m(g1, g2), m(b1, b2))
            }
            _ => a,
        }
    }

    /// El fondo suave de un color (el 28 % del color con el fondo): el de la fila elegida, con el de la sección.
    pub fn suave(c: Color) -> Color {
        mezcla(c, FONDO, 0.28)
    }

    /// Lo secundario: en GRIS.
    pub fn apagado() -> Style {
        Style::new().fg(GRIS)
    }
    /// Un título destacado: en negrita y del color de la sección («Tu Renta de 2025», «Por qué»).
    pub fn titulo() -> Style {
        Style::new().fg(acento()).add_modifier(Modifier::BOLD)
    }
    /// La fila elegida de una tabla o una lista: fondo suave del color de la sección y el texto en TINTA y negrita.
    /// Para `Table::row_highlight_style`, `List::highlight_style` o a mano.
    pub fn seleccion() -> Style {
        Style::new().fg(TINTA).bg(suave(acento())).add_modifier(Modifier::BOLD)
    }
    /// Lo elegido de unas pocas opciones en fila (« 3T », la sección activa): en FONDO y negrita sobre el color de
    /// la sección.
    pub fn pestana_activa() -> Style {
        Style::new().fg(FONDO).bg(acento()).add_modifier(Modifier::BOLD)
    }
    /// La cabecera de las columnas de una tabla: en gris y sin fondo.
    pub fn cabecera_tabla() -> Style {
        Style::new().fg(GRIS)
    }
    /// Importe en verde si es a tu favor, rojo si es a pagar.
    pub fn importe(x: f64) -> Style {
        if x > 0.005 { Style::new().fg(MAL) } else if x < -0.005 { Style::new().fg(BIEN) } else { Style::new() }
    }
}

/// Recuadro con título (borde fino en MARCO, el título en el color de la sección y el fondo repintado). En el
/// estilo C2 el contenido va sin cajas: para lo que sale encima, mejor `ventana`.
pub fn bloque(titulo: &str) -> Block<'_> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(tema::MARCO))
        .style(Style::new().bg(tema::FONDO))
        .title(Span::styled(format!(" {titulo} "), tema::titulo()))
}

/// Un rectángulo de `w` × `h` en el centro de `area` (recortado si no cabe).
pub fn centrado(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

/// «n nueva   Enter ver   q salir»: cada tecla en negrita y su descripción en gris, separadas por tres espacios.
/// Sin botoncitos.
pub fn linea_atajos(atajos: &[(&str, &str)]) -> Line<'static> {
    let mut v = Vec::new();
    for (i, (k, d)) in atajos.iter().enumerate() {
        if i > 0 {
            v.push(Span::raw("   "));
        }
        v.extend(atajo(k, d));
    }
    Line::from(v)
}

/// Una tecla y lo que hace («n nueva»): la tecla en negrita y TINTA; lo que hace, en gris.
pub fn atajo(tecla: &str, que: &str) -> [Span<'static>; 2] {
    [Span::styled(tecla.to_string(), Style::new().fg(tema::TINTA).add_modifier(Modifier::BOLD)), Span::styled(format!(" {que}"), tema::apagado())]
}

/// Una cifra destacada: la etiqueta en el borde de arriba y, dentro, el valor (en negrita) y una nota. Es del
/// diseño de antes (con caja): en C2, mejor `guia` o `banda`.
pub fn kpi(f: &mut Frame, area: Rect, etiqueta: &str, valor: &str, nota: &str, estilo: Style) {
    // La etiqueta, con un espacio a cada lado si cabe; si no, recortada.
    let sitio = area.width.saturating_sub(2) as usize;
    let n = etiqueta.chars().count();
    let titulo = if n + 2 <= sitio {
        format!(" {etiqueta} ")
    } else if n <= sitio {
        etiqueta.to_string()
    } else {
        etiqueta.chars().take(sitio.saturating_sub(1)).chain(['…']).collect()
    };
    let b = Block::new().borders(Borders::ALL).border_style(Style::new().fg(tema::MARCO)).title(Span::styled(titulo, tema::apagado()));
    let dentro = b.inner(area);
    // Un espacio de margen a la izquierda, si cabe el valor.
    let m = if (dentro.width as usize) > valor.chars().count() { " " } else { "" };
    let t = vec![Line::styled(format!("{m}{valor}"), estilo.add_modifier(Modifier::BOLD)), Line::styled(format!("{m}{nota}"), tema::apagado())];
    f.render_widget(b, area);
    f.render_widget(Paragraph::new(t), dentro);
}

/// Lo que se ha quedado sin color (lo de debajo de un `Clear`, un `Style::reset()`), al fondo y la tinta de la app:
/// así no quedan agujeros con los colores del terminal. Lo hace la app al acabar de pintar.
pub fn rellenar_fondo(buf: &mut ratatui::buffer::Buffer) {
    for c in buf.content.iter_mut() {
        if c.bg == Color::Reset {
            c.bg = tema::FONDO;
        }
        if c.fg == Color::Reset {
            c.fg = tema::TINTA;
        }
    }
}

/// Texto alineado a la derecha (importes en tablas).
pub fn derecha<'a>(s: impl Into<String>) -> Line<'a> {
    Line::from(s.into()).right_aligned()
}

/* ================= piezas del estilo C2 ================= */

/// El título de un bloque del contenido («próximo plazo», «lo último», «líneas»): en gris y sin recuadro.
/// Escríbelo en minúscula (salvo siglas: «IVA · modelo 303»).
pub fn titulo_bloque(t: impl Into<String>) -> Line<'static> {
    Line::styled(t.into(), tema::apagado())
}

/// Puntos guía: «te queda ·········· 17.796,00 €». La etiqueta (un texto, un `Span` o una `Line` de varios
/// trozos), un espacio, los puntos (en PUNTOS), otro espacio y la cifra en negrita con `estilo` (`Style::new()`
/// para TINTA), que acaba justo antes de la columna `fin` contada desde el principio de la línea: así, varias
/// guías con el mismo `fin` alinean sus cifras. Si no cabe, va al menos un punto. Detrás se pueden añadir más
/// trozos (`l.push_span(…)`, el «66 %»).
/// `guia(Span::styled("te queda", verde), fmt::eur(x), Style::new().fg(tema::BIEN), 39)`
pub fn guia<'a>(etiqueta: impl Into<Line<'a>>, cifra: impl Into<String>, estilo: Style, fin: u16) -> Line<'a> {
    let mut l: Line<'a> = etiqueta.into();
    let cifra = cifra.into();
    let ocupado = l.width() + cifra.chars().count() + 2;
    let puntos = (fin as usize).saturating_sub(ocupado).max(1);
    l.push_span(Span::raw(" "));
    l.push_span(Span::styled("·".repeat(puntos), Style::new().fg(tema::PUNTOS)));
    l.push_span(Span::raw(" "));
    l.push_span(Span::styled(cifra, estilo.add_modifier(Modifier::BOLD)));
    l
}

/// Filas de una banda: la de en medio y una de aire encima y otra debajo.
pub const ALTO_BANDA: u16 = 3;

/// Banda: la idea principal de una pantalla. Pinta un fondo suave en las tres primeras filas de `area` y `linea`
/// en la del medio (con aire encima y debajo; sin líneas). La cifra, en negrita y en color: BIEN si es tuya o a
/// tu favor; el de la sección o AVISO si la pagas. `fondo`: `tema::SUAVE_BIEN`, `tema::SUAVE_NARANJA`…
/// `banda(f, Rect { height: ALTO_BANDA, ..zona }, tema::SUAVE_BIEN, guia(…))`
pub fn banda(f: &mut Frame, area: Rect, fondo: Color, linea: Line) {
    let r = Rect { height: area.height.min(ALTO_BANDA), ..area };
    f.buffer_mut().set_style(r, Style::new().bg(fondo));
    let medio = Rect { y: r.y + r.height / 2, height: r.height.min(1), ..r };
    f.render_widget(Paragraph::new(linea), medio);
}

/// Sello: «▕ CUADRA ▏». Los ▕ ▏ en `color` y el texto, en negrita y en `color`, sobre `fondo`. Detrás se puede
/// seguir la línea (`l.push_span(…)`).
/// `sello("CUADRA", tema::BIEN, tema::SUAVE_BIEN)` · `sello("2,48 € DE MÁS", tema::AVISO, tema::SUAVE_AVISO)`
pub fn sello(texto: &str, color: Color, fondo: Color) -> Line<'static> {
    let c = Style::new().fg(color);
    Line::from(vec![Span::styled("▕", c), Span::styled(format!(" {texto} "), c.bg(fondo).add_modifier(Modifier::BOLD)), Span::styled("▏", c)])
}

/// Barra fina: una fila de «▄» de `ancho` columnas, repartida entre los tramos (cantidad, color) en proporción. Lo
/// que no es positivo no sale; un tramo pequeño ocupa al menos una columna (si caben todos).
/// `barra(&[(queda, tema::BIEN), (ss, tema::AZUL), (irpf, tema::AVISO), (gastos, tema::MARCO)], 46)`
pub fn barra(tramos: &[(f64, Color)], ancho: u16) -> Line<'static> {
    let total: f64 = tramos.iter().map(|t| t.0.max(0.0)).sum();
    let ancho = ancho as usize;
    if total <= 0.0 || ancho == 0 {
        return Line::default();
    }
    // Lo que toca a cada uno, redondeando por los restos más grandes para que sumen `ancho`.
    let exacto: Vec<f64> = tramos.iter().map(|t| t.0.max(0.0) / total * ancho as f64).collect();
    let mut n: Vec<usize> = exacto.iter().map(|x| x.floor() as usize).collect();
    let mut orden: Vec<usize> = (0..n.len()).collect();
    orden.sort_by(|&a, &b| (exacto[b] - n[b] as f64).total_cmp(&(exacto[a] - n[a] as f64)));
    for &i in orden.iter().take(ancho - n.iter().sum::<usize>()) {
        n[i] += 1;
    }
    // Los pequeños, al menos una: se la quitan al más grande.
    let positivos = exacto.iter().filter(|x| **x > 0.0).count();
    if positivos <= ancho {
        for i in 0..n.len() {
            if exacto[i] > 0.0 && n[i] == 0 {
                let mayor = (0..n.len()).max_by_key(|&j| n[j]).unwrap_or(i);
                n[mayor] -= 1;
                n[i] = 1;
            }
        }
    }
    Line::from(tramos.iter().zip(n).filter(|(_, k)| *k > 0).map(|((_, c), k)| Span::styled("▄".repeat(k), Style::new().fg(*c))).collect::<Vec<_>>())
}

/// «█ »: la muestra del color de un tramo de la barra, delante de su línea del desglose.
pub fn muestra(color: Color) -> Span<'static> {
    Span::styled("█ ", Style::new().fg(color))
}

/// Los días de la semana, del lunes al domingo.
pub const DIAS_SEMANA: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];

/// «2026-11-19» → «jueves» (vacío si no es una fecha).
pub fn dia_semana(fecha: &str) -> &'static str {
    use chrono::Datelike;
    crate::dominio::fmt::fecha(fecha).map_or("", |d| DIAS_SEMANA[d.weekday().num_days_from_monday() as usize])
}

/// «2026-11-19» → «jueves, 19 de noviembre» (tal cual si no es una fecha).
pub fn fecha_larga(fecha: &str) -> String {
    use chrono::Datelike;
    match crate::dominio::fmt::fecha(fecha) {
        Some(d) => format!("{}, {} de {}", dia_semana(fecha), d.day(), crate::dominio::fmt::MESES[d.month0() as usize]),
        None => fecha.to_string(),
    }
}

/// Ancho y alto de la cajita del próximo plazo.
pub const CAJITA_PLAZO: (u16, u16) = (20, 6);

/// La cajita de calendario del próximo plazo, arriba a la izquierda de `area` (`CAJITA_PLAZO` de ancho y alto):
/// marco fino en MARCO; el mes y el año en negrita, en FONDO sobre el color de la sección; el día en negrita y el
/// día de la semana en gris. `fecha`: «2027-02-01» (si no es una fecha, no pinta nada).
pub fn cajita_plazo(f: &mut Frame, area: Rect, fecha: &str) {
    use chrono::Datelike;
    let Some(d) = crate::dominio::fmt::fecha(fecha) else { return };
    let (w, h) = CAJITA_PLAZO;
    let r = Rect { width: w.min(area.width), height: h.min(area.height), ..area };
    let b = Block::new().borders(Borders::ALL).border_style(Style::new().fg(tema::MARCO)).style(Style::new().bg(tema::FONDO));
    let dentro = b.inner(r);
    f.render_widget(b, r);
    // Centrado como en los bocetos: lo que sobra, con la mitad (por abajo) a la izquierda.
    let centrar = |s: String, st: Style| Line::styled(format!("{}{s}", " ".repeat((dentro.width as usize).saturating_sub(s.chars().count()) / 2)), st);
    let mes = format!("{} {}", crate::dominio::fmt::MESES[d.month0() as usize].to_uppercase(), d.year());
    let t = vec![
        centrar(mes, tema::pestana_activa()),
        Line::from(""),
        centrar(d.day().to_string(), Style::new().add_modifier(Modifier::BOLD)),
        centrar(dia_semana(fecha).to_string(), tema::apagado()),
    ];
    // La fila del mes, entera del color de la sección.
    f.buffer_mut().set_style(Rect { height: dentro.height.min(1), ..dentro }, tema::pestana_activa());
    f.render_widget(Paragraph::new(t), dentro);
}

/// Una ventana encima de la pantalla (ayuda, editores, selector, fallos…) en `r` (normalmente
/// `centrado(area, w, h)`): borra lo de debajo, repinta el fondo (`Clear` solo dejaría agujeros), pone un borde
/// fino en MARCO con el título en el color de la sección y devuelve el sitio de dentro, con un espacio de margen
/// a cada lado.
/// `let dentro = ventana(f, centrado(area, 72, 20), "Elegir cliente");`
pub fn ventana(f: &mut Frame, r: Rect, titulo: &str) -> Rect {
    f.render_widget(Clear, r);
    let b = bloque(titulo).style(Style::new().fg(tema::TINTA).bg(tema::FONDO)).padding(Padding::horizontal(1));
    let dentro = b.inner(r);
    f.render_widget(b, r);
    dentro
}

/// Una línea en el borde de abajo de una ventana (`r`, el mismo de `ventana`), a la derecha: las teclas, la
/// versión…
pub fn pie_ventana(f: &mut Frame, r: Rect, linea: Line) {
    if r.height < 2 || r.width < 4 {
        return;
    }
    let fila = Rect { x: r.x + 1, y: r.bottom() - 1, width: r.width - 2, height: 1 };
    let mut l = linea;
    l.spans.insert(0, Span::raw(" "));
    l.push_span(Span::raw(" "));
    f.render_widget(Paragraph::new(l.right_aligned()), fila);
}

/// Las filas que ocupa `texto` partido por las palabras en `ancho` columnas (como `Wrap`), para dimensionar una
/// ventana.
pub fn filas_partidas(texto: &str, ancho: usize) -> u16 {
    let ancho = ancho.max(1);
    let (mut filas, mut largo) = (1u16, 0usize);
    for p in texto.split(' ') {
        let w = p.chars().count();
        if largo == 0 {
            largo = w;
        } else if largo + 1 + w <= ancho {
            largo += 1 + w;
        } else {
            filas += 1;
            largo = w;
        }
        while largo > ancho {
            filas += 1;
            largo -= ancho;
        }
    }
    filas
}

/// Para buscar sin que importen mayúsculas ni tildes.
pub fn normalizar(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            c => c,
        })
        .collect()
}

/// ¿`texto` contiene todas las palabras de `busca`?
pub fn coincide(texto: &str, busca: &str) -> bool {
    let t = normalizar(texto);
    normalizar(busca).split_whitespace().all(|p| t.contains(p))
}

/// ¿Es Ctrl+tecla? En Windows, AltGr (la @, el €…) llega como Ctrl+Alt: eso no cuenta como Ctrl.
pub fn es_ctrl(k: &KeyEvent) -> bool {
    k.modifiers.contains(KeyModifiers::CONTROL) && !k.modifiers.contains(KeyModifiers::ALT)
}

/* ================= campo de texto de una línea ================= */

/// Texto editable con cursor (cuenta en caracteres, no en bytes: las tildes ocupan dos).
#[derive(Debug, Clone, Default)]
pub struct Entrada {
    pub valor: String,
    pub cursor: usize,
}

impl Entrada {
    pub fn new(v: impl Into<String>) -> Self {
        let valor = v.into();
        let cursor = valor.chars().count();
        Entrada { valor, cursor }
    }
    fn byte(&self, i: usize) -> usize {
        self.valor.char_indices().nth(i).map(|(b, _)| b).unwrap_or(self.valor.len())
    }
    /// Devuelve true si ha cambiado el texto.
    pub fn tecla(&mut self, k: KeyEvent) -> bool {
        let n = self.valor.chars().count();
        let ctrl = es_ctrl(&k);
        match k.code {
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(n),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = n,
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = n,
            KeyCode::Char('u') if ctrl => {
                self.valor.clear();
                self.cursor = 0;
                return true;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let b = self.byte(self.cursor - 1);
                self.valor.remove(b);
                self.cursor -= 1;
                return true;
            }
            KeyCode::Delete if self.cursor < n => {
                let b = self.byte(self.cursor);
                self.valor.remove(b);
                return true;
            }
            KeyCode::Char(c) if !ctrl => {
                let b = self.byte(self.cursor);
                self.valor.insert(b, c);
                self.cursor += 1;
                return true;
            }
            _ => {}
        }
        false
    }
    /// Pinta el texto en `area` (una línea) y, si tiene el foco, pone ahí el cursor.
    pub fn dibujar(&self, f: &mut Frame, area: Rect, foco: bool, estilo: Style) {
        let ancho = area.width.saturating_sub(1) as usize;
        let ini = self.cursor.saturating_sub(ancho);
        let visible: String = self.valor.chars().skip(ini).take(ancho + 1).collect();
        f.render_widget(Paragraph::new(Span::styled(visible, estilo)), area);
        if foco {
            f.set_cursor_position((area.x + (self.cursor - ini) as u16, area.y));
        }
    }
}

/* ================= formularios ================= */

#[derive(Debug, Clone)]
pub enum Tipo {
    Texto,
    /// Importe o %: «1.234,56».
    Numero,
    /// «2026-10-19» o «19/10/2026».
    Fecha,
    /// (clave, nombre): se elige con ← →.
    Opciones(Vec<(String, String)>),
    /// Sí / no, con espacio.
    Si,
    /// Texto que no se ve al escribirlo (una contraseña): se pinta con «•».
    Secreto,
}

#[derive(Debug, Clone)]
pub struct Campo {
    pub clave: String,
    pub etiqueta: String,
    pub tipo: Tipo,
    pub entrada: Entrada,
    /// Para Opciones: la elegida.
    pub elegida: usize,
    pub si: bool,
    pub ayuda: String,
    /// Se ve, pero no se puede cambiar.
    pub solo_lectura: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resultado {
    /// Nada que hacer (se movió el foco o el cursor).
    Sigue,
    /// Ha cambiado el valor de un campo (por si hay que recalcular algo).
    Cambio,
    /// Ctrl+S, o Enter en el último campo.
    Guardar,
    Cancelar,
}

/// Formulario de una columna: etiqueta y valor por línea. Se construye encadenando campos:
/// `Formulario::new("Nuevo gasto").fecha("fecha", "Fecha", &hoy).texto("proveedor", "Proveedor", "")…`
#[derive(Debug, Clone, Default)]
pub struct Formulario {
    pub titulo: String,
    pub campos: Vec<Campo>,
    pub foco: usize,
    pub error: Option<String>,
}

impl Formulario {
    pub fn new(titulo: impl Into<String>) -> Self {
        Formulario { titulo: titulo.into(), ..Default::default() }
    }
    fn campo(mut self, clave: &str, etiqueta: &str, tipo: Tipo, valor: &str) -> Self {
        self.campos.push(Campo {
            clave: clave.into(),
            etiqueta: etiqueta.into(),
            tipo,
            entrada: Entrada::new(valor),
            elegida: 0,
            si: false,
            ayuda: String::new(),
            solo_lectura: false,
        });
        self
    }
    pub fn texto(self, clave: &str, etiqueta: &str, valor: &str) -> Self {
        self.campo(clave, etiqueta, Tipo::Texto, valor)
    }
    /// Un número: `None` deja el campo vacío.
    pub fn numero(self, clave: &str, etiqueta: &str, valor: Option<f64>) -> Self {
        let v = valor.map(|x| crate::dominio::fmt::num(x).replace('.', "").trim_end_matches(",00").to_string()).unwrap_or_default();
        self.campo(clave, etiqueta, Tipo::Numero, &v)
    }
    /// Una contraseña: no se ve al escribirla.
    pub fn secreto(self, clave: &str, etiqueta: &str) -> Self {
        self.campo(clave, etiqueta, Tipo::Secreto, "")
    }
    pub fn fecha(self, clave: &str, etiqueta: &str, valor: &str) -> Self {
        self.campo(clave, etiqueta, Tipo::Fecha, valor)
    }
    /// Opciones (clave, nombre); `valor` es la clave elegida.
    pub fn opciones(mut self, clave: &str, etiqueta: &str, opciones: &[(&str, &str)], valor: &str) -> Self {
        let v: Vec<(String, String)> = opciones.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let i = v.iter().position(|(k, _)| k == valor).unwrap_or(0);
        self = self.campo(clave, etiqueta, Tipo::Opciones(v), "");
        self.campos.last_mut().unwrap().elegida = i;
        self
    }
    pub fn si(mut self, clave: &str, etiqueta: &str, valor: bool) -> Self {
        self = self.campo(clave, etiqueta, Tipo::Si, "");
        self.campos.last_mut().unwrap().si = valor;
        self
    }
    /// Texto de ayuda del último campo añadido (se ve abajo cuando tiene el foco).
    pub fn ayuda(mut self, txt: &str) -> Self {
        if let Some(c) = self.campos.last_mut() {
            c.ayuda = txt.into();
        }
        self
    }
    fn buscar(&self, clave: &str) -> Option<&Campo> {
        self.campos.iter().find(|c| c.clave == clave)
    }
    fn buscar_mut(&mut self, clave: &str) -> Option<&mut Campo> {
        self.campos.iter_mut().find(|c| c.clave == clave)
    }
    /// Lo escrito (o, en las opciones, la clave elegida; en sí/no, "1" o "").
    pub fn get(&self, clave: &str) -> String {
        match self.buscar(clave) {
            Some(c) => match &c.tipo {
                Tipo::Opciones(v) => v.get(c.elegida).map(|x| x.0.clone()).unwrap_or_default(),
                Tipo::Si => if c.si { "1".into() } else { String::new() },
                _ => c.entrada.valor.trim().to_string(),
            },
            None => String::new(),
        }
    }
    /// Número escrito; vacío → None.
    pub fn num(&self, clave: &str) -> Option<f64> {
        crate::dominio::fmt::num_opcional(&self.get(clave))
    }
    /// Fecha escrita, en ISO; None si no es una fecha válida.
    pub fn fecha_iso(&self, clave: &str) -> Option<String> {
        crate::dominio::fmt::leer_fecha(&self.get(clave))
    }
    /// Cambia el valor de un campo (en las opciones, por clave; en sí/no, "1"/"").
    pub fn set(&mut self, clave: &str, valor: &str) {
        if let Some(c) = self.buscar_mut(clave) {
            match &c.tipo {
                Tipo::Opciones(v) => {
                    if let Some(i) = v.iter().position(|(k, _)| k == valor) {
                        c.elegida = i;
                    }
                }
                Tipo::Si => c.si = !valor.is_empty() && valor != "0",
                _ => c.entrada = Entrada::new(valor),
            }
        }
    }
    /// Clave del campo con el foco.
    pub fn foco_clave(&self) -> &str {
        self.campos.get(self.foco).map(|c| c.clave.as_str()).unwrap_or("")
    }

    fn mover(&mut self, d: isize) {
        let n = self.campos.len() as isize;
        if n == 0 {
            return;
        }
        let mut i = self.foco as isize;
        for _ in 0..n {
            i = (i + d).rem_euclid(n);
            if !self.campos[i as usize].solo_lectura {
                break;
            }
        }
        self.foco = i as usize;
    }

    pub fn tecla(&mut self, k: KeyEvent) -> Resultado {
        let ctrl = es_ctrl(&k);
        match k.code {
            KeyCode::Esc => return Resultado::Cancelar,
            KeyCode::Char('s') if ctrl => return Resultado::Guardar,
            KeyCode::Tab | KeyCode::Down => {
                self.mover(1);
                return Resultado::Sigue;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.mover(-1);
                return Resultado::Sigue;
            }
            KeyCode::Enter => {
                let ultimo = (self.foco + 1..self.campos.len()).all(|i| self.campos[i].solo_lectura);
                if ultimo {
                    return Resultado::Guardar;
                }
                self.mover(1);
                return Resultado::Sigue;
            }
            _ => {}
        }
        let Some(c) = self.campos.get_mut(self.foco) else { return Resultado::Sigue };
        if c.solo_lectura {
            return Resultado::Sigue;
        }
        self.error = None;
        match &c.tipo {
            Tipo::Opciones(v) => {
                let n = v.len().max(1);
                match k.code {
                    KeyCode::Left => c.elegida = (c.elegida + n - 1) % n,
                    KeyCode::Right | KeyCode::Char(' ') => c.elegida = (c.elegida + 1) % n,
                    KeyCode::Char(ch) => {
                        // Salta a la siguiente opción que empieza por esa letra.
                        let ch = normalizar(&ch.to_string());
                        let pos = (1..=n).map(|d| (c.elegida + d) % n).find(|&i| normalizar(&v[i].1).starts_with(&ch));
                        match pos {
                            Some(i) => c.elegida = i,
                            None => return Resultado::Sigue,
                        }
                    }
                    _ => return Resultado::Sigue,
                }
                Resultado::Cambio
            }
            Tipo::Si => match k.code {
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                    c.si = !c.si;
                    Resultado::Cambio
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    c.si = true;
                    Resultado::Cambio
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    c.si = false;
                    Resultado::Cambio
                }
                _ => Resultado::Sigue,
            },
            _ => {
                if c.entrada.tecla(k) { Resultado::Cambio } else { Resultado::Sigue }
            }
        }
    }

    /// Líneas que necesita (campos + ayuda + error), para dimensionar la ventana.
    pub fn alto(&self) -> u16 {
        self.campos.len() as u16 + 4
    }

    /// Pinta el formulario dentro de `area`, sin recuadro (encima de la pantalla, con `dibujar_ventana`). Estilo C2:
    /// las etiquetas en gris; el campo con el foco, con «›» y la etiqueta del color de la sección, y su valor sobre
    /// el fondo suave de ese color (sin un recuadro por campo). Debajo, el error (en MAL) o la ayuda del campo.
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        let [cuerpo, abajo] = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).areas(area);
        let w_etq = self.campos.iter().map(|c| c.etiqueta.chars().count()).max().unwrap_or(10).min(28) as u16 + 4;
        let acento = Style::new().fg(tema::acento()).add_modifier(Modifier::BOLD);
        // Si no caben todos, se desplaza para que se vea el del foco (sin foco, desde el primero).
        let alto = cuerpo.height as usize;
        let ini = if self.foco < self.campos.len() && self.foco >= alto { self.foco + 1 - alto } else { 0 };
        for (i, c) in self.campos.iter().enumerate().skip(ini).take(alto) {
            let y = cuerpo.y + (i - ini) as u16;
            let fila = Rect { x: cuerpo.x, y, width: cuerpo.width, height: 1 };
            let [etq, val] = Layout::horizontal([Constraint::Length(w_etq), Constraint::Min(1)]).areas(fila);
            let foco = i == self.foco;
            let nombre: String = c.etiqueta.chars().take(w_etq as usize - 4).collect();
            let etiqueta = if foco { vec![Span::styled("› ", acento), Span::styled(nombre, acento)] } else { vec![Span::raw("  "), Span::styled(nombre, tema::apagado())] };
            f.render_widget(Paragraph::new(Line::from(etiqueta)), etq);
            let est_val = if c.solo_lectura { tema::apagado() } else { Style::new() };
            // El campo con el foco, sobre el fondo suave de la sección (de lo que ocupe, con un mínimo).
            let val = if foco && !c.solo_lectura {
                let largo = match &c.tipo {
                    Tipo::Opciones(v) => v.iter().map(|x| x.1.chars().count() + 4).max().unwrap_or(0),
                    _ => c.entrada.valor.chars().count() + 2,
                };
                let r = Rect { width: val.width.min(largo.max(24) as u16), ..val };
                f.buffer_mut().set_style(r, Style::new().bg(tema::suave(tema::acento())));
                Rect { x: r.x + 1, width: r.width.saturating_sub(1), ..r }
            } else {
                Rect { x: val.x + 1, width: val.width.saturating_sub(1), ..val }
            };
            match &c.tipo {
                Tipo::Opciones(v) => {
                    let txt = v.get(c.elegida).map(|x| x.1.as_str()).unwrap_or("");
                    let l = if foco {
                        Line::from(vec![Span::styled("‹ ", acento), Span::styled(txt.to_string(), est_val.add_modifier(Modifier::BOLD)), Span::styled(" ›", acento)])
                    } else {
                        Line::styled(txt.to_string(), est_val)
                    };
                    f.render_widget(Paragraph::new(l), val);
                }
                Tipo::Si => {
                    let s = if c.si { "[x] sí" } else { "[ ] no" };
                    f.render_widget(Paragraph::new(Span::styled(s, if foco { est_val.add_modifier(Modifier::BOLD) } else { est_val })), val);
                }
                _ => {
                    let st = if foco { est_val.add_modifier(Modifier::BOLD) } else { est_val };
                    let vacio = c.entrada.valor.is_empty();
                    if vacio && !foco {
                        f.render_widget(Paragraph::new(Span::styled("—", Style::new().fg(tema::PUNTOS))), val);
                    } else if matches!(c.tipo, Tipo::Secreto) {
                        let oculta = Entrada { valor: "•".repeat(c.entrada.valor.chars().count()), cursor: c.entrada.cursor };
                        oculta.dibujar(f, val, foco, st);
                    } else {
                        c.entrada.dibujar(f, val, foco, st);
                    }
                }
            }
        }
        let mut t: Vec<Line> = Vec::new();
        if let Some(e) = &self.error {
            t.push(Line::styled(e.clone(), Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD)));
        }
        if let Some(c) = self.campos.get(self.foco)
            && !c.ayuda.is_empty() {
                t.push(Line::styled(c.ayuda.clone(), tema::apagado()));
            }
        let abajo = Rect { x: abajo.x + 2, width: abajo.width.saturating_sub(2), ..abajo };
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), abajo);
    }

    /// Pinta el formulario como ventana encima, en el centro de `area` y de `ancho` columnas, con su título.
    pub fn dibujar_ventana(&self, f: &mut Frame, area: Rect, ancho: u16) {
        let r = centrado(area, ancho, self.alto() + 2);
        let dentro = ventana(f, r, &self.titulo);
        // Una fila de aire bajo el título.
        self.dibujar(f, Rect { y: dentro.y + 1, height: dentro.height.saturating_sub(1), ..dentro });
    }
}

/* ================= selector con búsqueda ================= */

#[derive(Debug, Clone, PartialEq)]
pub enum Eleccion {
    Sigue,
    /// La clave del elemento elegido.
    Elegido(String),
    Cancelado,
}

/// Lista para elegir (un cliente, una categoría…): se escribe para filtrar, ↑ ↓ y Enter.
#[derive(Debug, Clone, Default)]
pub struct Selector {
    pub titulo: String,
    /// (clave, texto, detalle)
    pub elementos: Vec<(String, String, String)>,
    pub filtro: Entrada,
    pub estado: ListState,
}

impl Selector {
    pub fn new(titulo: impl Into<String>, elementos: Vec<(String, String, String)>) -> Self {
        let mut s = Selector { titulo: titulo.into(), elementos, filtro: Entrada::default(), estado: ListState::default() };
        s.estado.select(Some(0));
        s
    }
    /// Pone la selección en el elemento con esa clave.
    pub fn con_elegido(mut self, clave: &str) -> Self {
        if let Some(i) = self.visibles().iter().position(|&i| self.elementos[i].0 == clave) {
            self.estado.select(Some(i));
        }
        self
    }
    fn visibles(&self) -> Vec<usize> {
        (0..self.elementos.len())
            .filter(|&i| {
                let (_, t, d) = &self.elementos[i];
                coincide(&format!("{t} {d}"), &self.filtro.valor)
            })
            .collect()
    }
    pub fn tecla(&mut self, k: KeyEvent) -> Eleccion {
        let vis = self.visibles();
        let sel = self.estado.selected().unwrap_or(0);
        match k.code {
            KeyCode::Esc => return Eleccion::Cancelado,
            KeyCode::Enter => {
                return match vis.get(sel) {
                    Some(&i) => Eleccion::Elegido(self.elementos[i].0.clone()),
                    None => Eleccion::Sigue,
                };
            }
            KeyCode::Down => self.estado.select(Some((sel + 1).min(vis.len().saturating_sub(1)))),
            KeyCode::Up => self.estado.select(Some(sel.saturating_sub(1))),
            KeyCode::PageDown => self.estado.select(Some((sel + 10).min(vis.len().saturating_sub(1)))),
            KeyCode::PageUp => self.estado.select(Some(sel.saturating_sub(10))),
            _ => {
                if self.filtro.tecla(k) {
                    self.estado.select(Some(0));
                }
            }
        }
        Eleccion::Sigue
    }
    /// La ventana del selector, en el centro de `area`: arriba lo que escribes para buscar; la lista, con la fila
    /// elegida en el fondo suave de la sección; y abajo, las teclas.
    pub fn dibujar(&mut self, f: &mut Frame, area: Rect) {
        let r = centrado(area, 72.min(area.width), 20.min(area.height));
        let dentro = ventana(f, r, &self.titulo);
        let [_, busca, _, lista, pie] = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(dentro);
        let [lupa, txt] = Layout::horizontal([Constraint::Length(7), Constraint::Min(1)]).areas(busca);
        f.render_widget(Paragraph::new(Span::styled("buscar", tema::apagado())), lupa);
        self.filtro.dibujar(f, txt, true, Style::new().add_modifier(Modifier::BOLD));
        let vis = self.visibles();
        let items: Vec<ListItem> = vis
            .iter()
            .map(|&i| {
                let (_, t, d) = &self.elementos[i];
                ListItem::new(Line::from(vec![Span::raw(t.clone()), Span::styled(if d.is_empty() { String::new() } else { format!("  {d}") }, tema::apagado())]))
            })
            .collect();
        let vacia = items.is_empty();
        f.render_stateful_widget(List::new(items).highlight_style(tema::seleccion()), lista, &mut self.estado);
        if vacia {
            f.render_widget(Paragraph::new(Span::styled("nada con ese texto", tema::apagado())), lista);
        }
        f.render_widget(Paragraph::new(linea_atajos(&[("↑↓", "elegir"), ("Enter", "aceptar"), ("Esc", "cancelar")])), pie);
    }
}

/* ================= confirmar ================= */

#[derive(Debug, Clone, Default)]
pub struct Confirmar {
    pub titulo: String,
    pub lineas: Vec<String>,
    /// El texto del botón de sí («Emitir», «Borrar»…).
    pub si: String,
    /// Es algo que no se deshace (en rojo).
    pub peligro: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Respuesta {
    Sigue,
    Si,
    No,
}

impl Confirmar {
    pub fn new(titulo: impl Into<String>, si: impl Into<String>) -> Self {
        Confirmar { titulo: titulo.into(), si: si.into(), ..Default::default() }
    }
    pub fn linea(mut self, s: impl Into<String>) -> Self {
        self.lineas.push(s.into());
        self
    }
    pub fn peligro(mut self) -> Self {
        self.peligro = true;
        self
    }
    pub fn tecla(&self, k: KeyEvent) -> Respuesta {
        match k.code {
            KeyCode::Enter | KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Char('y') => Respuesta::Si,
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => Respuesta::No,
            _ => Respuesta::Sigue,
        }
    }
    /// La ventana de confirmar, en el centro de `area`: las líneas y, debajo, qué tecla dice que sí (en el color de
    /// la sección, o en MAL si no se deshace) y cuál cancela.
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        // Dentro, dos columnas más que la línea más larga (hasta 78) y las filas que ocupen las líneas partidas, con
        // aire arriba, una en blanco y la de las teclas.
        let ancho = self.lineas.iter().map(|l| l.chars().count()).max().unwrap_or(30).clamp(36, 76) as u16 + 6;
        let util = centrado(area, ancho, 1).width.saturating_sub(4) as usize;
        let alto = self.lineas.iter().map(|l| filas_partidas(l, util)).sum::<u16>() + 5;
        let dentro = ventana(f, centrado(area, ancho, alto), &self.titulo);
        // Las teclas, siempre en la última fila (aunque alguna línea se parta y no quepa entera).
        let [arriba, abajo] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(dentro);
        let mut t: Vec<Line> = vec![Line::from("")];
        t.extend(self.lineas.iter().map(|l| Line::from(l.clone())));
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), arriba);
        let si = Style::new().fg(if self.peligro { tema::MAL } else { tema::acento() }).add_modifier(Modifier::BOLD);
        let [k_no, d_no] = atajo("Esc/n", "cancelar");
        let teclas = Line::from(vec![Span::styled("Enter/s", Style::new().add_modifier(Modifier::BOLD)), Span::styled(format!(" {}", self.si), si), Span::raw("   "), k_no, d_no]);
        f.render_widget(Paragraph::new(teclas), abajo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Pinta con `pintar` en un terminal de `w` × `h` y devuelve su buffer.
    fn pintado(w: u16, h: u16, pintar: impl FnOnce(&mut Frame)) -> ratatui::buffer::Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(pintar).unwrap();
        t.backend().buffer().clone()
    }

    fn texto(b: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..b.area.width).map(|x| b[(x, y)].symbol()).collect::<String>().trim_end().to_string()
    }

    /// El color de la sección es el de la que se ha puesto; la selección, su fondo suave con TINTA en negrita.
    #[test]
    fn el_acento_es_el_de_la_seccion() {
        tema::poner_seccion('a');
        assert_eq!(tema::acento(), tema::seccion('a'));
        let s = tema::seleccion();
        assert_eq!((s.fg, s.bg), (Some(tema::TINTA), Some(tema::suave(tema::seccion('a')))));
        assert!(s.add_modifier.contains(Modifier::BOLD) && !s.add_modifier.contains(Modifier::REVERSED));
        tema::poner_seccion('r');
        assert_eq!(tema::acento(), tema::seccion('r'));
        assert_eq!(tema::mezcla(tema::TINTA, tema::FONDO, 1.0), tema::TINTA);
    }

    /// Con el mismo `fin`, las cifras de varias guías acaban en la misma columna; si no cabe, un punto.
    #[test]
    fn las_guias_alinean_sus_cifras() {
        let a = guia("te queda", "17.796,00 €", Style::new(), 40);
        let b = guia(Span::styled("IRPF adelantado", tema::apagado()), "948,40 €", Style::new(), 40);
        assert_eq!((a.width(), b.width()), (40, 40));
        assert_eq!(a.to_string(), format!("te queda {} 17.796,00 €", "·".repeat(19)));
        assert!(a.spans.last().unwrap().style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(guia("una etiqueta muy larga", "1,00 €", Style::new(), 10).to_string(), "una etiqueta muy larga · 1,00 €");
    }

    /// La barra ocupa justo su ancho, en proporción; lo pequeño, al menos una columna; sin nada, vacía.
    #[test]
    fn la_barra_reparte_su_ancho() {
        let l = barra(&[(66.0, tema::BIEN), (13.0, tema::AZUL), (11.0, tema::AVISO), (10.0, tema::MARCO)], 46);
        let n: Vec<usize> = l.spans.iter().map(|s| s.content.chars().count()).collect();
        assert_eq!(n.iter().sum::<usize>(), 46);
        assert_eq!(n, [30, 6, 5, 5]);
        let l = barra(&[(1000.0, tema::BIEN), (1.0, tema::MAL), (-5.0, tema::AZUL)], 10);
        assert_eq!(l.spans.iter().map(|s| s.content.chars().count()).collect::<Vec<_>>(), [9, 1]);
        assert_eq!(barra(&[(0.0, tema::BIEN)], 10).width(), 0);
    }

    #[test]
    fn el_sello_y_las_fechas() {
        assert_eq!(sello("CUADRA", tema::BIEN, tema::SUAVE_BIEN).to_string(), "▕ CUADRA ▏");
        assert_eq!(fecha_larga("2026-11-19"), "jueves, 19 de noviembre");
        assert_eq!(dia_semana("2027-02-01"), "lunes");
    }

    /// La ventana repinta el fondo ella sola (sin ella, `Clear` deja los colores del terminal), con el borde en MARCO
    /// y el título en el color de la sección; devuelve el sitio de dentro, con un espacio a cada lado.
    #[test]
    fn la_ventana_repinta_el_fondo() {
        tema::poner_seccion('e');
        let mut dentro = Rect::default();
        let r = Rect::new(2, 1, 30, 8);
        let b = pintado(40, 10, |f| dentro = ventana(f, r, "Ayuda"));
        assert_eq!(dentro, Rect::new(4, 2, 26, 6));
        for y in r.top()..r.bottom() {
            for x in r.left()..r.right() {
                assert_eq!(b[(x, y)].bg, tema::FONDO, "({x}, {y})");
            }
        }
        assert_eq!(b[(2, 1)].fg, tema::MARCO);
        assert!(texto(&b, 1).contains("┌ Ayuda ─"));
        assert_eq!(b[(4, 1)].fg, tema::seccion('e'));
        tema::poner_seccion('r');
    }

    /// La cajita del plazo: el mes y el año en FONDO sobre el color de la sección, el día y el día de la semana.
    #[test]
    fn la_cajita_del_plazo() {
        let b = pintado(24, 8, |f| cajita_plazo(f, Rect::new(1, 1, 22, 7), "2027-02-01"));
        let filas: Vec<String> = (0..8).map(|y| texto(&b, y)).collect();
        assert_eq!(filas[1], " ┌──────────────────┐");
        assert_eq!(filas[2], " │   FEBRERO 2027   │");
        assert_eq!(filas[4], " │        1         │");
        assert_eq!(filas[5], " │      lunes       │");
        assert_eq!(filas[6], " └──────────────────┘");
        assert_eq!((b[(2, 2)].fg, b[(2, 2)].bg), (tema::FONDO, tema::acento()));
    }

    /// Las teclas, sin botoncitos: la tecla en negrita y lo que hace en gris, con tres espacios entre una y otra.
    #[test]
    fn la_linea_de_teclas() {
        let l = linea_atajos(&[("n", "nueva"), ("Enter", "ver")]);
        assert_eq!(l.to_string(), "n nueva   Enter ver");
        assert!(l.spans.iter().all(|s| s.style.bg.is_none()));
        assert_eq!(l.spans[1].style.fg, Some(tema::GRIS));
    }

    #[test]
    fn altgr_de_windows_escribe() {
        // AltGr+2 en un teclado español de Windows: «@» con Ctrl+Alt.
        let mut e = Entrada::new("yo");
        assert!(e.tecla(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL | KeyModifiers::ALT)));
        assert_eq!(e.valor, "yo@");
        // Ctrl+U de verdad sigue borrando.
        assert!(e.tecla(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)));
        assert_eq!(e.valor, "");
    }
}
