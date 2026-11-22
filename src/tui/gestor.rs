//! Tu gestor: vive debajo del menú, en el carril, con su mesa para soltar documentos; con `G` se elige otro. Son
//! cuatro gestorías de toda la vida, en broma. Cada una tiene su dibujo (20 × 13 celdas, 26 filas de píxeles en
//! medios bloques), su mote, su frase, su habilidad, su debilidad y cuatro atributos con su broma.
//!
//! La mesa enseña cómo van los documentos que arrastras al terminal (`documentos`): esperando, recién recibidos,
//! mientras Claude los lee y ya ordenados mientras los revisas. Después vuelve sola a esperar.
//!
//! Los dibujos salen de los bocetos (`bocetos/gen_int.py --rust`): una letra por píxel y su paleta; «.» es el
//! fondo.
use super::fallos::partir;
use super::widgets::{centrado, linea_atajos, tema, ventana};
use crate::dominio::Datos;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// La tecla para elegir gestor. Ninguna pantalla la usa (salvo escribiendo): lo vigila un test de `mod.rs`.
pub const TECLA: char = 'G';
/// Columnas del dibujo y de su mesa.
pub const ANCHO: u16 = 20;
/// Filas de píxeles de cada dibujo: dos por celda.
const ALTO_PX: usize = 26;
/// Filas (celdas) del dibujo.
pub const ALTO_DIBUJO: u16 = ALTO_PX as u16 / 2;
/// Filas del faldón de la mesa, donde va lo que dice.
const FALDON_FILAS: u16 = 3;
/// La mesa: el tablero, el faldón y las patas.
pub const ALTO_MESA: u16 = 1 + FALDON_FILAS + 1;
/// El bloque entero del carril: el nombre, el dibujo y la mesa.
pub const ALTO_BLOQUE: u16 = 1 + ALTO_DIBUJO + ALTO_MESA;
/// El que va si no has elegido ninguno: Mari Carmen.
const POR_DEFECTO: usize = 1;

/// La madera de la mesa: el tablero, su canto con luz, el faldón y las patas; y la letra del faldón.
const TABLERO: Color = Color::Rgb(0x8c, 0x73, 0x5f);
const CANTO: Color = Color::Rgb(0xc4, 0xa3, 0x83);
const FALDON: Color = Color::Rgb(0x4b, 0x3a, 0x30);
const PATA: Color = Color::Rgb(0x3a, 0x2d, 0x26);
const LETRA_FALDON: Color = Color::Rgb(0xd9, 0xcf, 0xc4);
/// Lo que falta de una barra de atributos.
const APAGADO: Color = Color::Rgb(0x2e, 0x2a, 0x3c);
/// Columnas de la izquierda de la rejilla de la ventana (el mote, la habilidad, el nombre del atributo).
const COLUMNA: usize = 26;
/// Rayas de las barras de atributos: diez y, si se pasa, hasta dos más (en AVISO).
const RAYAS: usize = 12;

pub struct Gestor {
    /// Lo que se guarda en `Datos::gestor`.
    pub clave: &'static str,
    pub nombre: &'static str,
    pub mote: &'static str,
    /// El suyo: su nombre y sus barras.
    pub color: Color,
    pub frase: &'static str,
    /// Lo que le contesta el otro (los Martínez); si nada, vacío.
    pub replica: &'static str,
    /// Su habilidad especial y su debilidad: (cuál, en qué consiste).
    pub habilidad: (&'static str, &'static str),
    pub debilidad: (&'static str, &'static str),
    /// Sus cuatro atributos más graciosos: (cuál, cuánto de 10 —o más—, la broma).
    pub atributos: [(&'static str, usize, &'static str); 4],
    /// Lo que dice en la mesa al recibir documentos.
    pub coletilla: &'static str,
    /// El aviso de abajo al contratarlo.
    pub contratado: &'static str,
    dibujo: [&'static str; ALTO_PX],
    paleta: &'static [(char, Color)],
}

impl Gestor {
    /// El color de un píxel del dibujo; `None`, el fondo.
    fn pixel(&self, c: char) -> Option<Color> {
        self.paleta.iter().find(|(k, _)| *k == c).map(|(_, color)| *color)
    }
}

pub const GESTORES: [Gestor; 4] = [
    Gestor {
        clave: "don-antonio",
        nombre: "Don Antonio",
        mote: "el del fax",
        color: Color::Rgb(0xec, 0xb8, 0x5e),
        frase: "«Mándamelo por fax, chaval, que el correo ese se pierde.»",
        replica: "",
        habilidad: ("Letra de notario", "Pasa tus 400 facturas a mano, a pluma y sin un borrón."),
        debilidad: ("¿Un pe-de-efe?", "Si le mandas un PDF, te pide que lo imprimas y se lo mandes por fax."),
        atributos: [("Fax", 10, "su gran amor"), ("Uso del ordenador", 1, "tapado con un paño"), ("Paciencia", 9, "lo explica tres veces"), ("Excusas", 5, "se ha cortado la línea")],
        coletilla: "mejor por fax",
        contratado: "Contratado: Don Antonio, el del fax.",
        dibujo: DIBUJO_ANTONIO,
        paleta: PALETA_ANTONIO,
    },
    Gestor {
        clave: "mari-carmen",
        nombre: "Mari Carmen",
        mote: "la de las dos",
        color: Color::Rgb(0xff, 0x8f, 0xa3),
        frase: "«Uy, que son menos cinco. Eso, mañana, ¿vale, cariño?»",
        replica: "",
        habilidad: ("Sprint de las 13:30", "De 13:30 a 13:59 tramita más que en toda la mañana."),
        debilidad: ("Persiana a las dos", "A las 14:00 en punto se va, aunque tu IVA esté a medio sumar."),
        atributos: [("Puntualidad", 10, "para irse"), ("Cafés al día", 9, "el de las 11, una hora"), ("Uso del ordenador", 4, "al solitario, un crac"), ("Paciencia", 3, "desde las 13:45, cero")],
        coletilla: "antes de las dos",
        contratado: "Contratada: Mari Carmen, la de las dos.",
        dibujo: DIBUJO_MARI_CARMEN,
        paleta: PALETA_MARI_CARMEN,
    },
    Gestor {
        clave: "hermanos-martinez",
        nombre: "Hermanos Martínez",
        mote: "eso lo lleva mi hermano",
        color: Color::Rgb(0x9b, 0xd7, 0x7a),
        frase: "«¿Lo tuyo? Eso lo lleva mi hermano.»",
        replica: "—¡Que no, que lo llevas tú!",
        habilidad: ("Dos por uno", "Dos gestores por el precio de uno, y un cuñado que «de esto sabe»."),
        debilidad: ("Eso lo lleva él", "Cada uno cree que tu trimestre lo lleva el otro. No lo lleva nadie."),
        atributos: [("Excusas", 10, "una por hermano"), ("Rapidez", 3, "3 + 3, nunca a la vez"), ("Uso del ordenador", 4, "lo instaló el sobrino"), ("Cafés al día", 8, "en el bar de abajo")],
        coletilla: "mi hermano lo ve",
        contratado: "Contratados: los Hermanos Martínez (eso lo lleva su hermano).",
        dibujo: DIBUJO_HERMANOS,
        paleta: PALETA_HERMANOS,
    },
    Gestor {
        clave: "dona-remedios",
        nombre: "Doña Remedios",
        mote: "la que lo imprime todo",
        color: Color::Rgb(0x5f, 0xd4, 0xc4),
        frase: "«Lo tenía aquí mismo… ¿Me lo vuelves a traer? Por triplicado.»",
        replica: "",
        habilidad: ("Copia de seguridad", "Imprime cada correo tres veces, por si acaso, y lo grapa."),
        debilidad: ("¿Qué papel?", "Tu factura está en la torre. En qué piso, ni ella lo sabe."),
        atributos: [("Papel impreso", 12, "no cabe en la barra"), ("Uso del ordenador", 2, "solo para imprimir"), ("Rapidez", 1, "primero lo busca"), ("Puntualidad", 6, "llega; las llaves, no")],
        coletilla: "y lo imprimo",
        contratado: "Contratada: Doña Remedios, la que lo imprime todo.",
        dibujo: DIBUJO_REMEDIOS,
        paleta: PALETA_REMEDIOS,
    },
];

/// El que has elegido (su índice en `GESTORES`): por su clave en tus datos; si no hay o no se conoce, Mari Carmen.
pub fn elegido(d: &Datos) -> usize {
    GESTORES.iter().position(|g| g.clave == d.gestor).unwrap_or(POR_DEFECTO)
}

/// Una celda de medio bloque en (x, y): «▀» con el píxel de arriba de tinta y el de abajo de fondo (si son iguales,
/// un espacio con ese fondo). Fuera de `zona`, nada.
fn medio(buf: &mut Buffer, zona: Rect, x: u16, y: u16, arriba: Color, abajo: Color) {
    if x < zona.x || x >= zona.right() || y < zona.y || y >= zona.bottom() {
        return;
    }
    if let Some(c) = buf.cell_mut((x, y)) {
        let simbolo = if arriba == abajo { " " } else { "▀" };
        c.set_symbol(simbolo).set_fg(arriba).set_bg(abajo);
    }
}

/// Pinta el dibujo de `g` con su esquina de arriba a la izquierda en (x, y), sin salirse de `zona`. Donde no hay
/// píxel, `fondo`.
pub fn dibujar(buf: &mut Buffer, zona: Rect, x: u16, y: u16, g: &Gestor, fondo: Color) {
    for (fila, par) in g.dibujo.chunks(2).enumerate() {
        for (col, (a, b)) in par[0].chars().zip(par[1].chars()).enumerate() {
            let (a, b) = (g.pixel(a).unwrap_or(fondo), g.pixel(b).unwrap_or(fondo));
            medio(buf, zona, x + col as u16, y + fila as u16, a, b);
        }
    }
}

/// Cómo está la mesa: lo que pasa con los documentos que arrastras (lo dice `Documentos::mesa`).
#[derive(Debug, Clone, PartialEq)]
pub enum Mesa {
    /// Esperando: «⇣ arrastra aquí» y lo que se puede soltar.
    Reposo,
    /// Recién soltados, un momento: el faldón se enciende con «✓ ¡recibido! N» y caen papeles en el tablero.
    Recibido(usize),
    /// Claude los está leyendo: «leyendo 2 de 3» y una barra fina.
    Leyendo { leidos: usize, total: usize },
    /// Leídos todos, mientras los revisas: «✓ ordenado» y una carpeta por documento entendido, del color de donde va
    /// (Autónomo o Empleo). `por_revisar`: los que quedan.
    Ordenado { carpetas: Vec<Color>, por_revisar: usize },
}

/// Lo que hay encima del tablero, píxel a píxel (`ANCHO`): papeles al soltar y mientras se leen (los que faltan,
/// tres como mucho) y las carpetas al acabar.
fn encima(mesa: &Mesa) -> Vec<Option<Color>> {
    let mut v = vec![None; ANCHO as usize];
    let papeles = |v: &mut Vec<Option<Color>>, n: usize| {
        for (desde, largo) in [(2, 3), (7, 4), (13, 3)].into_iter().take(n) {
            v[desde..desde + largo].fill(Some(tema::TINTA));
        }
    };
    match mesa {
        Mesa::Reposo => {}
        Mesa::Recibido(n) => papeles(&mut v, *n),
        Mesa::Leyendo { leidos, total } => papeles(&mut v, total.saturating_sub(*leidos)),
        Mesa::Ordenado { carpetas, .. } => {
            for (k, c) in carpetas.iter().take(6).enumerate() {
                v[2 + 3 * k..4 + 3 * k].fill(Some(*c));
            }
        }
    }
    v
}

/// Lo que dice el faldón (sus `FALDON_FILAS` filas, centradas) y su fondo. Esperando, qué se puede soltar; luego,
/// cómo va, en las dos primeras.
fn faldon(g: &Gestor, mesa: &Mesa) -> (Color, Vec<Line<'static>>) {
    let negrita = Style::new().fg(tema::TINTA).add_modifier(Modifier::BOLD);
    let tinta = Style::new().fg(tema::TINTA);
    let suave = Style::new().fg(LETRA_FALDON);
    let dos = |a: Line<'static>, b: Line<'static>| vec![a, b, Line::default()];
    match mesa {
        Mesa::Reposo => (FALDON, vec![Line::styled("⇣ arrastra aquí", negrita), Line::styled("facturas, gastos y", suave), Line::styled("nóminas (PDF, foto)", suave)]),
        Mesa::Recibido(n) => (tema::mezcla(tema::acento(), tema::FONDO, 0.30), dos(Line::styled(format!("✓ ¡recibido! {n}"), negrita), Line::styled(g.coletilla, tinta))),
        Mesa::Leyendo { leidos, total } => {
            // La barra fina: lo leído, del color de la sección; lo que falta, en MARCO.
            let largo = 14;
            let hecho = (largo * leidos).checked_div(*total).unwrap_or(0);
            let barra = Line::from(vec![Span::styled("▀".repeat(hecho), Style::new().fg(tema::acento())), Span::styled("▀".repeat(largo - hecho), Style::new().fg(tema::MARCO))]);
            (FALDON, dos(Line::styled(format!("leyendo {} de {total}", (leidos + 1).min(*total)), negrita), barra))
        }
        Mesa::Ordenado { por_revisar, .. } => (tema::mezcla(tema::BIEN, tema::FONDO, 0.24), dos(Line::styled("✓ ordenado", negrita), Line::styled(format!("{por_revisar} por revisar"), tinta))),
    }
}

/// La mesa de `g` desde (x, y), sin salirse de `zona`: el tablero con lo que haya encima y su canto, el faldón con
/// lo que dice y las patas (la tecla para cambiar de gestor está en la barra de abajo, con las demás).
fn dibujar_mesa(f: &mut Frame, zona: Rect, x: u16, y: u16, g: &Gestor, mesa: &Mesa) {
    let buf = f.buffer_mut();
    for (col, c) in encima(mesa).into_iter().enumerate() {
        medio(buf, zona, x + col as u16, y, c.unwrap_or(TABLERO), CANTO);
    }
    let (fondo, lineas) = faldon(g, mesa);
    for (k, l) in lineas.into_iter().enumerate() {
        let r = Rect::new(x, y + 1 + k as u16, ANCHO, 1).intersection(zona);
        f.buffer_mut().set_style(r, Style::new().bg(fondo));
        f.render_widget(Paragraph::new(l.centered()), r);
    }
    let patas = y + 1 + FALDON_FILAS;
    for col in [1, 2, ANCHO - 3, ANCHO - 2] {
        medio(f.buffer_mut(), zona, x + col, patas, PATA, PATA);
    }
}

/// El bloque del carril en `zona` (desde su fila de arriba): el nombre de `g`, en negrita y de su color; su dibujo
/// y su mesa. Si no cabe entero, sin el dibujo; si tampoco, nada.
pub fn dibujar_bloque(f: &mut Frame, zona: Rect, g: &Gestor, mesa: &Mesa) {
    if zona.width < ANCHO {
        return;
    }
    let con_dibujo = zona.height >= ALTO_BLOQUE;
    if !con_dibujo && zona.height < 1 + ALTO_MESA {
        return;
    }
    let (x, y) = (zona.x, zona.y);
    let nombre = Line::styled(g.nombre, Style::new().fg(g.color).add_modifier(Modifier::BOLD));
    f.render_widget(Paragraph::new(nombre), Rect::new(x, y, ANCHO, 1));
    let mut y = y + 1;
    if con_dibujo {
        dibujar(f.buffer_mut(), zona, x, y, g, tema::FONDO);
        y += ALTO_DIBUJO;
    }
    dibujar_mesa(f, zona, x, y, g, mesa);
}

/// La ventana para elegir gestor (con `G`): arriba, los cuatro en fila con su nombre y el elegido resaltado; debajo,
/// solo lo del elegido, en una rejilla: mote y frase; habilidad y debilidad; y sus cuatro atributos con su barra.
/// En un terminal pequeño, sin los dibujos.
pub struct Elegir {
    /// El que está resaltado (su índice en `GESTORES`).
    pub i: usize,
}

/// Lo que pasa con una tecla en la ventana de elegir.
#[derive(Debug, PartialEq)]
pub enum Respuesta {
    Sigue,
    /// Enter: contratar a este (su índice en `GESTORES`).
    Contratado(usize),
    /// Esc: se queda el que estaba.
    Cancelado,
}

impl Elegir {
    pub fn tecla(&mut self, k: KeyEvent) -> Respuesta {
        let n = GESTORES.len();
        match k.code {
            KeyCode::Left => self.i = (self.i + n - 1) % n,
            KeyCode::Right => self.i = (self.i + 1) % n,
            KeyCode::Enter => return Respuesta::Contratado(self.i),
            KeyCode::Esc => return Respuesta::Cancelado,
            _ => {}
        }
        Respuesta::Sigue
    }

    /// Lo del elegido, en la rejilla de `ancho` columnas: a la izquierda (`COLUMNA`) qué es; a la derecha, lo que
    /// dice (partido si no cabe, con sangría).
    fn rejilla(&self, ancho: usize) -> Vec<Line<'static>> {
        let g = &GESTORES[self.i];
        let negrita = Style::new().fg(tema::TINTA).add_modifier(Modifier::BOLD);
        let gris = tema::apagado();
        let derecha = ancho.saturating_sub(COLUMNA);
        // Una fila: la etiqueta, rellena hasta `COLUMNA`, y lo de la derecha partido.
        let fila = |etiqueta: Vec<Span<'static>>, texto: Vec<(String, Style)>| {
            let relleno = COLUMNA.saturating_sub(Line::from(etiqueta.clone()).width());
            let mut v = Vec::new();
            for (k, l) in partir(&texto, derecha, 0).into_iter().enumerate() {
                let mut spans = if k == 0 { [etiqueta.clone(), vec![Span::raw(" ".repeat(relleno))]].concat() } else { vec![Span::raw(" ".repeat(COLUMNA))] };
                spans.extend(l.spans);
                v.push(Line::from(spans));
            }
            v
        };
        let mut t = Vec::new();
        let mut frase = vec![(g.frase.to_string(), negrita)];
        if !g.replica.is_empty() {
            frase.push((format!(" {}", g.replica), gris));
        }
        t.extend(fila(vec![Span::styled(g.mote, Style::new().fg(g.color))], frase));
        t.push(Line::default());
        for (marca, color, (que, como)) in [("★ ", tema::BIEN, g.habilidad), ("✗ ", tema::AVISO, g.debilidad)] {
            t.extend(fila(vec![Span::styled(marca, Style::new().fg(color).add_modifier(Modifier::BOLD)), Span::styled(que, negrita)], vec![(como.to_string(), gris)]));
        }
        t.push(Line::default());
        for (que, cuanto, broma) in g.atributos {
            // Diez rayas: las que tiene, de su color; las que no, apagadas; si se pasa, las de más en AVISO.
            let mut barra: Vec<Span<'static>> = (0..RAYAS)
                .map(|k| {
                    let color = if k >= 10 && k < cuanto { tema::AVISO } else if k < cuanto { g.color } else if k < 10 { APAGADO } else { tema::FONDO };
                    Span::styled("▌", Style::new().fg(color))
                })
                .collect();
            let cifra = Style::new().fg(if cuanto > 10 { tema::AVISO } else { tema::TINTA }).add_modifier(Modifier::BOLD);
            barra.extend([Span::styled(format!(" {cuanto:>2}"), cifra), Span::raw("   "), Span::styled(broma, gris)]);
            let mut l = vec![Span::styled(que, gris), Span::raw(" ".repeat(COLUMNA.saturating_sub(que.chars().count())))];
            l.extend(barra);
            t.push(Line::from(l));
        }
        t
    }

    /// La ventana, en el centro de `area`. Con sitio, los cuatro dibujos en fila: el elegido, sobre el fondo suave de
    /// su color y con su nombre en negrita y de su color. Si no, solo los nombres, con una raya bajo el elegido.
    pub fn dibujar(&self, f: &mut Frame, area: Rect) {
        let ancho = area.width.saturating_sub(4).min(102);
        let util = ancho.saturating_sub(4);
        // Cuatro columnas repartidas: de `ANCHO` (los dibujos, a cuatro de distancia al menos y a seis como mucho)
        // o, sin dibujos, de lo que quepa.
        let a_lo_ancho = util >= 4 * ANCHO + 3 * 4;
        let columna = if a_lo_ancho { ANCHO } else { (util / 4).min(ANCHO) };
        let hueco = (util.saturating_sub(4 * columna) / 3).min(6);
        let margen = util.saturating_sub(4 * columna + 3 * hueco) / 2;
        let rejilla = self.rejilla(util.saturating_sub(margen) as usize);
        // Dentro: aire, los dibujos y los nombres (o los nombres y la raya), aire, la rejilla, aire y las teclas.
        let arriba = |dibujos: bool| if dibujos { 1 + ALTO_DIBUJO + 1 + 2 } else { 1 + 2 + 1 };
        let alto = |dibujos: bool| arriba(dibujos) + rejilla.len() as u16 + 2 + 2;
        let caben = a_lo_ancho && area.height >= alto(true);
        let r = centrado(area, ancho, alto(caben));
        let dentro = ventana(f, r, "Elige a tu gestor");
        if dentro.height < 2 {
            return;
        }
        let x0 = dentro.x + margen;
        let y = dentro.y + 1;
        for (k, g) in GESTORES.iter().enumerate() {
            let x = x0 + k as u16 * (columna + hueco);
            let elegido = k == self.i;
            let mut fila_nombre = y;
            if caben {
                let tinte = tema::mezcla(g.color, tema::FONDO, 0.16);
                if elegido {
                    // La tarjeta del elegido: su fondo suave, con aire alrededor.
                    let tarjeta = Rect::new(x.saturating_sub(2), y - 1, ANCHO + 4, ALTO_DIBUJO + 3).intersection(dentro);
                    f.buffer_mut().set_style(tarjeta, Style::new().bg(tinte));
                }
                dibujar(f.buffer_mut(), dentro, x, y, g, if elegido { tinte } else { tema::FONDO });
                fila_nombre += ALTO_DIBUJO;
            }
            let estilo = if elegido { Style::new().fg(g.color).add_modifier(Modifier::BOLD) } else { tema::apagado() };
            let r = Rect::new(x, fila_nombre, columna, 1).intersection(dentro);
            f.render_widget(Paragraph::new(Line::styled(g.nombre, estilo).centered()), r);
            if !caben && elegido {
                let largo = (g.nombre.chars().count() as u16).min(columna);
                let raya = Rect::new(x + (columna - largo) / 2, fila_nombre + 1, largo, 1).intersection(dentro);
                f.render_widget(Paragraph::new(Line::styled("▀".repeat(largo as usize), Style::new().fg(g.color))), raya);
            }
        }
        let y = dentro.y + arriba(caben);
        let abajo = dentro.bottom() - 1;
        let zona = Rect::new(x0, y, dentro.right().saturating_sub(x0), abajo.saturating_sub(y)).intersection(dentro);
        f.render_widget(Paragraph::new(rejilla), zona);
        let teclas = linea_atajos(&[("← →", "elegir"), ("Enter", "contratar"), ("Esc", "cancelar")]);
        f.render_widget(Paragraph::new(teclas), Rect::new(x0, abajo, dentro.right().saturating_sub(x0), 1).intersection(dentro));
    }
}

// Los dibujos: 26 filas de 20 píxeles, una letra por píxel, y su paleta. Salen de `bocetos/gen_int.py --rust`.
const DIBUJO_ANTONIO: [&str; ALTO_PX] = [
    "....................",
    "......FGGGGGGg......",
    ".....GFGGGGGGggf....",
    "....GGfGGGGGGggf....",
    "....GfGGGGGGgggff...",
    "...GGGGGGG2234fff...",
    "...GGG111122334ff...",
    "rY.gggGGGg2gGGGgF...",
    ".Yygg1fff222fff34F..",
    "...Yemmmmm2nmmmmmF..",
    "...3n2wE2mmm2wE2m3..",
    "..42nmmmmm2mmmmmm4..",
    "..432222223222234...",
    "...42+2222322+334...",
    "....222222+433334...",
    "....3222223333344...",
    ".....222MWWWM334....",
    ".....3323MMM445.....",
    "......33333445......",
    ".......444454.......",
    ".....jCCc4444xccK...",
    "...jJjjCCc44xccKkKk.",
    ".JjjJjjCCttccKkKkKkq",
    ".JjjJjjCCTtccKkKkKkq",
    "jJjJJjjjCTtcKKkKkKkq",
    "jJjJJjjjCTtcKKkKkKkq",
];
const PALETA_ANTONIO: &[(char, Color)] = &[
    ('+', Color::Rgb(0xe3, 0x90, 0x7f)),
    ('1', Color::Rgb(0xf8, 0xdc, 0xc4)),
    ('2', Color::Rgb(0xec, 0xbc, 0x9c)),
    ('3', Color::Rgb(0xd6, 0x9f, 0x7e)),
    ('4', Color::Rgb(0xb5, 0x7a, 0x62)),
    ('5', Color::Rgb(0x8a, 0x56, 0x50)),
    ('C', Color::Rgb(0xf8, 0xf5, 0xfc)),
    ('E', Color::Rgb(0x1b, 0x13, 0x18)),
    ('F', Color::Rgb(0x6a, 0x64, 0x82)),
    ('G', Color::Rgb(0xf6, 0xf4, 0xfb)),
    ('J', Color::Rgb(0xf2, 0xc8, 0x70)),
    ('K', Color::Rgb(0xb0, 0x7c, 0x34)),
    ('M', Color::Rgb(0x7c, 0x3a, 0x3c)),
    ('T', Color::Rgb(0xc0, 0x50, 0x5f)),
    ('W', Color::Rgb(0xff, 0xff, 0xff)),
    ('Y', Color::Rgb(0xff, 0xd1, 0x66)),
    ('c', Color::Rgb(0xdb, 0xd4, 0xe8)),
    ('e', Color::Rgb(0x3a, 0x2c, 0x2a)),
    ('f', Color::Rgb(0x9c, 0x96, 0xb2)),
    ('g', Color::Rgb(0xcf, 0xca, 0xde)),
    ('j', Color::Rgb(0xd9, 0xa4, 0x47)),
    ('k', Color::Rgb(0x7d, 0x54, 0x24)),
    ('m', Color::Rgb(0x4a, 0x2a, 0x22)),
    ('n', Color::Rgb(0x9a, 0x5c, 0x38)),
    ('q', Color::Rgb(0x4e, 0x34, 0x16)),
    ('r', Color::Rgb(0xff, 0x8a, 0x80)),
    ('t', Color::Rgb(0x7e, 0x2e, 0x40)),
    ('w', Color::Rgb(0xee, 0xe9, 0xf1)),
    ('x', Color::Rgb(0xad, 0xa4, 0xc3)),
    ('y', Color::Rgb(0xd9, 0x9a, 0x2e)),
];
const DIBUJO_MARI_CARMEN: [&str; ALTO_PX] = [
    "......edeeddnd......",
    "....eedeeddnddnb....",
    "...deeddeddnnbnnb...",
    "..dendddnnnbdnnbnb..",
    ".eednnnbbbbnnnbnnbb.",
    "deddbbbbeedbbnbdnnba",
    "dddneedeeednedndnnbb",
    "ddndedddddndeddnndnb",
    ".dndddnndnndddnbedd.",
    "ddnbnnnbbbb2nnnbeddn",
    "dnnb2b1112223b4dddnb",
    "nnba21bbb222bbbnnnnb",
    ".dd22b112b2b334beedb",
    "ednb2zzzz22zzzzdeedn",
    "ddnb2wwEE22wwEEdddnn",
    "nnbb22222223224nnnnb",
    ".ed2+2222223334+bdba",
    "eedn22222243344eedn.",
    "ddnna222lLLLl44dddnb",
    "nnnba3223lLl44annnb.",
    "bbba..3333344...bba.",
    ".......44445........",
    ".......44444........",
    "....CCC44444cccq....",
    "...RrCCP233pccqqqqq.",
    "..rrrCCCPPpcccqqqqqQ",
];
const PALETA_MARI_CARMEN: &[(char, Color)] = &[
    ('+', Color::Rgb(0xe3, 0x90, 0x7f)),
    ('1', Color::Rgb(0xf8, 0xdc, 0xc4)),
    ('2', Color::Rgb(0xec, 0xbc, 0x9c)),
    ('3', Color::Rgb(0xd6, 0x9f, 0x7e)),
    ('4', Color::Rgb(0xb5, 0x7a, 0x62)),
    ('5', Color::Rgb(0x8a, 0x56, 0x50)),
    ('C', Color::Rgb(0xf8, 0xf5, 0xfc)),
    ('E', Color::Rgb(0x1b, 0x13, 0x18)),
    ('L', Color::Rgb(0xe8, 0x56, 0x6c)),
    ('P', Color::Rgb(0xfb, 0xf6, 0xff)),
    ('Q', Color::Rgb(0x8f, 0x3d, 0x5e)),
    ('R', Color::Rgb(0xff, 0xb1, 0xc0)),
    ('a', Color::Rgb(0x3e, 0x18, 0x24)),
    ('b', Color::Rgb(0x6e, 0x29, 0x34)),
    ('c', Color::Rgb(0xdb, 0xd4, 0xe8)),
    ('d', Color::Rgb(0xc8, 0x63, 0x4c)),
    ('e', Color::Rgb(0xec, 0x91, 0x68)),
    ('l', Color::Rgb(0xa8, 0x34, 0x4c)),
    ('n', Color::Rgb(0x9c, 0x3f, 0x3e)),
    ('p', Color::Rgb(0xbf, 0xb6, 0xcf)),
    ('q', Color::Rgb(0xc9, 0x5c, 0x7a)),
    ('r', Color::Rgb(0xf2, 0x84, 0x9b)),
    ('w', Color::Rgb(0xee, 0xe9, 0xf1)),
    ('z', Color::Rgb(0x86, 0xae, 0xe8)),
];
const DIBUJO_HERMANOS: [&str; ALTO_PX] = [
    "....................",
    "....................",
    "....GGGg............",
    "...GGGGgg...........",
    "..GgGGfggf..........",
    ".GGGGggfgf..........",
    ".GGGGgggfg..........",
    ".gG11223fg...hIHh...",
    ".g2112234g.hHiIiHIh.",
    ".f2ff2ff4fhhiHHhiiHh",
    ".f2wE2wE4fhiHhiiiHIh",
    "3222222334hihi122Hih",
    "4222243344hh111223Hh",
    "4322224334hHhh22hh3h",
    ".32gGGGg3.3mmm2mmm44",
    "..2fgggf3.mEw2mEw3m5",
    "..3222M34.2mmm2mmm4.",
    "...32234..+2222334+.",
    "....3345..322MM344..",
    "....4445...333344...",
    "...CCtcq....3344....",
    ".sQQCccqqq..4444....",
    "sQQQCtcqqXXCC4YcQ...",
    "QQQQQtqqXSSsCCccQQQ.",
    "QQQQQtqXSSssCCycQQQQ",
    "QQQQQtqXSsssCCycQQQQ",
];
const PALETA_HERMANOS: &[(char, Color)] = &[
    ('+', Color::Rgb(0xe3, 0x90, 0x7f)),
    ('1', Color::Rgb(0xf8, 0xdc, 0xc4)),
    ('2', Color::Rgb(0xec, 0xbc, 0x9c)),
    ('3', Color::Rgb(0xd6, 0x9f, 0x7e)),
    ('4', Color::Rgb(0xb5, 0x7a, 0x62)),
    ('5', Color::Rgb(0x8a, 0x56, 0x50)),
    ('C', Color::Rgb(0xf8, 0xf5, 0xfc)),
    ('E', Color::Rgb(0x1b, 0x13, 0x18)),
    ('G', Color::Rgb(0xdc, 0xda, 0xe6)),
    ('H', Color::Rgb(0x4f, 0x3e, 0x4c)),
    ('I', Color::Rgb(0x90, 0x7c, 0x8a)),
    ('M', Color::Rgb(0x7c, 0x3a, 0x3c)),
    ('Q', Color::Rgb(0x55, 0x8a, 0x46)),
    ('S', Color::Rgb(0xa3, 0xd4, 0x7f)),
    ('X', Color::Rgb(0x17, 0x24, 0x1b)),
    ('Y', Color::Rgb(0xf0, 0xc0, 0x60)),
    ('c', Color::Rgb(0xdb, 0xd4, 0xe8)),
    ('f', Color::Rgb(0x7f, 0x7a, 0x94)),
    ('g', Color::Rgb(0xae, 0xaa, 0xbf)),
    ('h', Color::Rgb(0x38, 0x2b, 0x3b)),
    ('i', Color::Rgb(0x6a, 0x56, 0x64)),
    ('m', Color::Rgb(0x4a, 0x30, 0x26)),
    ('q', Color::Rgb(0x3a, 0x62, 0x34)),
    ('s', Color::Rgb(0x78, 0xb2, 0x5a)),
    ('t', Color::Rgb(0x43, 0x5f, 0x9c)),
    ('w', Color::Rgb(0xee, 0xe9, 0xf1)),
    ('y', Color::Rgb(0xb5, 0x87, 0x3a)),
];
const DIBUJO_REMEDIOS: [&str; ALTO_PX] = [
    "..............r.....",
    "........GGGgfK......",
    "........GfGgK.......",
    ".......gGGfKf.......",
    "........ggKff.......",
    "........ffffF.......",
    ".......GGGFGgg......",
    "......GgGgGGfgf.....",
    ".....GGGGGGGggf.....",
    ".....G21112233ff....",
    ".....G1ff222ff4F....",
    ".....g2Ew222Ew4f....",
    ".....2m33mmm33m4....",
    ".....22mm242mm45....",
    ".....2222223334.....",
    ".....222222MM34.....",
    ".....3222233344.....",
    "......222233345.....",
    ".......3333345......",
    ".......533445.......",
    "PPPPo..444444.......",
    "pppposCC4444ccQQq...",
    "8888dssCCC44ccQQQqq.",
    "6666asssCCcccQQQQqqq",
    "9999DsssCCccQQQQQqqq",
    "PPPPosssCCccQQQQQqqq",
];
const PALETA_REMEDIOS: &[(char, Color)] = &[
    ('1', Color::Rgb(0xf8, 0xdc, 0xc4)),
    ('2', Color::Rgb(0xec, 0xbc, 0x9c)),
    ('3', Color::Rgb(0xd6, 0x9f, 0x7e)),
    ('4', Color::Rgb(0xb5, 0x7a, 0x62)),
    ('5', Color::Rgb(0x8a, 0x56, 0x50)),
    ('6', Color::Rgb(0xb6, 0x9c, 0xff)),
    ('8', Color::Rgb(0xff, 0x9f, 0x5a)),
    ('9', Color::Rgb(0x9b, 0xd7, 0x7a)),
    ('C', Color::Rgb(0xf8, 0xf5, 0xfc)),
    ('D', Color::Rgb(0x6e, 0x9e, 0x54)),
    ('E', Color::Rgb(0x1b, 0x13, 0x18)),
    ('F', Color::Rgb(0x6c, 0x62, 0x90)),
    ('G', Color::Rgb(0xe6, 0xe0, 0xf2)),
    ('K', Color::Rgb(0xff, 0xd1, 0x66)),
    ('M', Color::Rgb(0x7c, 0x3a, 0x3c)),
    ('P', Color::Rgb(0xfb, 0xf9, 0xff)),
    ('Q', Color::Rgb(0x35, 0x8f, 0x87)),
    ('a', Color::Rgb(0x7d, 0x66, 0xc4)),
    ('c', Color::Rgb(0xdb, 0xd4, 0xe8)),
    ('d', Color::Rgb(0xc4, 0x72, 0x3c)),
    ('f', Color::Rgb(0x99, 0x8e, 0xb8)),
    ('g', Color::Rgb(0xc2, 0xb8, 0xda)),
    ('m', Color::Rgb(0x8c, 0x33, 0x50)),
    ('o', Color::Rgb(0xa3, 0x9c, 0xb8)),
    ('p', Color::Rgb(0xd6, 0xd0, 0xe4)),
    ('q', Color::Rgb(0x25, 0x64, 0x62)),
    ('r', Color::Rgb(0xff, 0x8a, 0x80)),
    ('s', Color::Rgb(0x4f, 0xb8, 0xaa)),
    ('w', Color::Rgb(0xee, 0xe9, 0xf1)),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada dibujo: exactamente 26 filas de 20 píxeles, y cada píxel, fondo («.») o un color de su paleta (que no
    /// repite letras).
    #[test]
    fn cada_dibujo_tiene_su_tamano_y_su_paleta() {
        for g in &GESTORES {
            assert_eq!(g.dibujo.len(), ALTO_PX, "{}", g.nombre);
            for (k, fila) in g.dibujo.iter().enumerate() {
                assert_eq!(fila.chars().count(), ANCHO as usize, "{}, fila {k}", g.nombre);
                for c in fila.chars() {
                    assert!(c == '.' || g.pixel(c).is_some(), "{}, fila {k}: «{c}» no está en su paleta", g.nombre);
                }
            }
            let mut letras: Vec<char> = g.paleta.iter().map(|(c, _)| *c).collect();
            letras.sort();
            letras.dedup();
            assert_eq!(letras.len(), g.paleta.len(), "{}: letras repetidas en la paleta", g.nombre);
            assert!(!letras.contains(&'.'), "{}: «.» es el fondo", g.nombre);
        }
    }

    /// Los textos caben donde van: el nombre en el carril; la coletilla en el faldón; el mote y la habilidad en la
    /// columna de la izquierda de la ventana; los atributos, en su barra (diez y dos de más).
    #[test]
    fn los_textos_caben() {
        let mut claves: Vec<&str> = GESTORES.iter().map(|g| g.clave).collect();
        claves.sort();
        claves.dedup();
        assert_eq!(claves.len(), GESTORES.len());
        for g in &GESTORES {
            assert!(g.nombre.chars().count() <= ANCHO as usize, "{}", g.nombre);
            assert!(g.coletilla.chars().count() <= ANCHO as usize - 2, "{}", g.coletilla);
            assert!(g.mote.chars().count() < COLUMNA, "{}", g.mote);
            for (que, _) in [g.habilidad, g.debilidad] {
                assert!(2 + que.chars().count() < COLUMNA, "{que}");
            }
            for (que, cuanto, _) in g.atributos {
                assert!(que.chars().count() < COLUMNA && cuanto <= RAYAS, "{que} {cuanto}");
            }
        }
        for (_, lineas) in [Mesa::Reposo, Mesa::Recibido(99), Mesa::Leyendo { leidos: 9, total: 99 }, Mesa::Ordenado { carpetas: vec![], por_revisar: 99 }].iter().map(|m| faldon(&GESTORES[0], m)) {
            assert_eq!(lineas.len(), FALDON_FILAS as usize);
            assert!(lineas.iter().all(|l| l.width() < ANCHO as usize), "{lineas:?}");
        }
    }

    /// Sin elegir (los datos de antes, sin el campo), o con uno que ya no existe, Mari Carmen. El elegido se guarda
    /// por su clave; sin elegir, no se escribe nada.
    #[test]
    fn por_defecto_mari_carmen() {
        let viejos: Datos = serde_json::from_str(r#"{"version": 3, "perfil": {"nombre": "Ana"}}"#).unwrap();
        assert_eq!(viejos.gestor, "");
        assert_eq!(GESTORES[elegido(&viejos)].nombre, "Mari Carmen");
        assert!(!serde_json::to_string(&viejos).unwrap().contains("gestor"));
        let mut d = Datos { gestor: "dona-remedios".into(), ..Default::default() };
        assert_eq!(GESTORES[elegido(&d)].nombre, "Doña Remedios");
        let leidos: Datos = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(leidos.gestor, "dona-remedios");
        d.gestor = "el-que-ya-no-esta".into();
        assert_eq!(GESTORES[elegido(&d)].nombre, "Mari Carmen");
    }

    /// ← → dan la vuelta; Enter contrata al que se ve; Esc, a ninguno; lo demás no hace nada.
    #[test]
    fn las_teclas_de_la_ventana() {
        use ratatui::crossterm::event::KeyModifiers;
        let k = |c| KeyEvent::new(c, KeyModifiers::NONE);
        let mut v = Elegir { i: 0 };
        assert_eq!(v.tecla(k(KeyCode::Left)), Respuesta::Sigue);
        assert_eq!(v.i, 3);
        v.tecla(k(KeyCode::Right));
        v.tecla(k(KeyCode::Right));
        assert_eq!(v.tecla(k(KeyCode::Char('x'))), Respuesta::Sigue);
        assert_eq!(v.tecla(k(KeyCode::Enter)), Respuesta::Contratado(1));
        assert_eq!(v.tecla(k(KeyCode::Esc)), Respuesta::Cancelado);
    }
}
