//! Piezas comunes: colores, recuadros, formularios, un selector con búsqueda y la ventana de confirmar.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

pub mod tema {
    use ratatui::style::{Color, Modifier, Style};
    pub const ACENTO: Color = Color::Cyan;
    pub const BIEN: Color = Color::Green;
    pub const MAL: Color = Color::Red;
    pub const AVISO: Color = Color::Yellow;
    pub const APAGADO: Color = Color::DarkGray;
    pub fn apagado() -> Style {
        Style::new().fg(APAGADO)
    }
    pub fn titulo() -> Style {
        Style::new().fg(ACENTO).add_modifier(Modifier::BOLD)
    }
    /// La fila elegida de una tabla o lista.
    pub fn seleccion() -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }
    pub fn pestana_activa() -> Style {
        Style::new().fg(Color::Black).bg(ACENTO).add_modifier(Modifier::BOLD)
    }
    pub fn cabecera_tabla() -> Style {
        Style::new().fg(ACENTO).add_modifier(Modifier::BOLD)
    }
    /// Importe en verde si es a tu favor, rojo si es a pagar.
    pub fn importe(x: f64) -> Style {
        if x > 0.005 { Style::new().fg(MAL) } else if x < -0.005 { Style::new().fg(BIEN) } else { Style::new() }
    }
}

/// Recuadro con título.
pub fn bloque(titulo: &str) -> Block<'_> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(tema::apagado())
        .title(Span::styled(format!(" {titulo} "), tema::titulo()))
}

/// Un rectángulo de `w` × `h` en el centro de `area` (recortado si no cabe).
pub fn centrado(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

/// «n nueva · Enter ver · q salir»
pub fn linea_atajos(atajos: &[(&str, &str)]) -> Line<'static> {
    let mut v = vec![Span::raw(" ")];
    for (i, (k, d)) in atajos.iter().enumerate() {
        if i > 0 {
            v.push(Span::styled(" · ", tema::apagado()));
        }
        v.push(Span::styled(k.to_string(), Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD)));
        v.push(Span::styled(format!(" {d}"), tema::apagado()));
    }
    Line::from(v)
}

/// Una cifra destacada: etiqueta arriba, valor grande, nota abajo.
pub fn kpi(f: &mut Frame, area: Rect, etiqueta: &str, valor: &str, nota: &str, estilo: Style) {
    let b = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(tema::apagado());
    let t = vec![
        Line::styled(etiqueta.to_string(), tema::apagado()),
        Line::styled(valor.to_string(), estilo.add_modifier(Modifier::BOLD)),
        Line::styled(nota.to_string(), tema::apagado()),
    ];
    f.render_widget(Paragraph::new(t).block(b), area);
}

/// Texto alineado a la derecha (importes en tablas).
pub fn derecha<'a>(s: impl Into<String>) -> Line<'a> {
    Line::from(s.into()).right_aligned()
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

    /// Pinta el formulario dentro de `area` (sin recuadro: lo pone quien lo llama, con `bloque`).
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        let [cuerpo, abajo] = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).areas(area);
        let w_etq = self.campos.iter().map(|c| c.etiqueta.chars().count()).max().unwrap_or(10).min(28) as u16 + 3;
        // Si no caben todos, se desplaza para que se vea el del foco.
        let alto = cuerpo.height as usize;
        let ini = if self.foco >= alto { self.foco + 1 - alto } else { 0 };
        for (i, c) in self.campos.iter().enumerate().skip(ini).take(alto) {
            let y = cuerpo.y + (i - ini) as u16;
            let fila = Rect { x: cuerpo.x, y, width: cuerpo.width, height: 1 };
            let [etq, val] = Layout::horizontal([Constraint::Length(w_etq), Constraint::Min(1)]).areas(fila);
            let foco = i == self.foco;
            let est_etq = if foco { Style::new().fg(tema::ACENTO).add_modifier(Modifier::BOLD) } else { tema::apagado() };
            let marca = if foco { "› " } else { "  " };
            let nombre: String = c.etiqueta.chars().take(w_etq as usize - 2).collect();
            f.render_widget(Paragraph::new(Span::styled(format!("{marca}{nombre}"), est_etq)), etq);
            let est_val = if c.solo_lectura { tema::apagado() } else { Style::new() };
            match &c.tipo {
                Tipo::Opciones(v) => {
                    let txt = v.get(c.elegida).map(|x| x.1.as_str()).unwrap_or("");
                    let s = if foco { format!("‹ {txt} ›") } else { txt.to_string() };
                    f.render_widget(Paragraph::new(Span::styled(s, if foco { est_val.add_modifier(Modifier::BOLD) } else { est_val })), val);
                }
                Tipo::Si => {
                    let s = if c.si { "[x] sí" } else { "[ ] no" };
                    f.render_widget(Paragraph::new(Span::styled(s, est_val)), val);
                }
                _ => {
                    let st = if foco { est_val.add_modifier(Modifier::UNDERLINED) } else { est_val };
                    let vacio = c.entrada.valor.is_empty();
                    if vacio && !foco {
                        f.render_widget(Paragraph::new(Span::styled("—", tema::apagado())), val);
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
                t.push(Line::styled(c.ayuda.clone(), tema::apagado().italic()));
            }
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }), abajo);
    }

    /// Pinta el formulario como ventana en el centro de `area`, con su título.
    pub fn dibujar_ventana(&self, f: &mut Frame, area: Rect, ancho: u16) {
        let r = centrado(area, ancho, self.alto() + 2);
        f.render_widget(Clear, r);
        let b = bloque(&self.titulo);
        let dentro = b.inner(r);
        f.render_widget(b, r);
        self.dibujar(f, dentro);
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
    pub fn dibujar(&mut self, f: &mut Frame, area: Rect) {
        let r = centrado(area, 72.min(area.width), 20.min(area.height));
        f.render_widget(Clear, r);
        let b = bloque(&self.titulo);
        let dentro = b.inner(r);
        f.render_widget(b, r);
        let [busca, lista, pie] = Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(dentro);
        let [lupa, txt] = Layout::horizontal([Constraint::Length(9), Constraint::Min(1)]).areas(busca);
        f.render_widget(Paragraph::new(Span::styled("Buscar: ", tema::apagado())), lupa);
        self.filtro.dibujar(f, txt, true, Style::new());
        let vis = self.visibles();
        let items: Vec<ListItem> = vis
            .iter()
            .map(|&i| {
                let (_, t, d) = &self.elementos[i];
                ListItem::new(Line::from(vec![Span::raw(t.clone()), Span::styled(if d.is_empty() { String::new() } else { format!("  {d}") }, tema::apagado())]))
            })
            .collect();
        let vacia = items.is_empty();
        f.render_stateful_widget(List::new(items).highlight_style(tema::seleccion()).highlight_symbol("› "), lista, &mut self.estado);
        if vacia {
            f.render_widget(Paragraph::new(Span::styled("  (nada con ese texto)", tema::apagado())), lista);
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
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        let ancho = self.lineas.iter().map(|l| l.chars().count()).max().unwrap_or(30).clamp(36, 76) as u16 + 4;
        let alto = self.lineas.len() as u16 + 5;
        let r = centrado(area, ancho, alto);
        f.render_widget(Clear, r);
        let b = bloque(&self.titulo);
        let dentro = b.inner(r);
        f.render_widget(b, r);
        let mut t: Vec<Line> = self.lineas.iter().map(|l| Line::from(l.clone())).collect();
        t.push(Line::from(""));
        let est = if self.peligro { Style::new().fg(Color::White).bg(tema::MAL).bold() } else { tema::pestana_activa() };
        t.push(Line::from(vec![
            Span::styled(format!(" Enter/s: {} ", self.si), est),
            Span::raw("   "),
            Span::styled(" Esc/n: cancelar ", tema::apagado()),
        ]));
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), dentro);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
