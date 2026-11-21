//! Tus clientes: los que tienes guardados (se guardan solos al emitir una factura, o se crean al hacerla). No son
//! una pestaña: se abren desde Facturas (`C`), con el de la factura elegida. Cada factura lleva su propia copia de
//! los datos del cliente, así que cambiar o borrar uno aquí no toca ninguna factura.
use crate::dominio::factura::{self, OPERACIONES};
use crate::dominio::fmt;
use crate::dominio::{Cliente, Datos, Factura, uid};
use crate::tui::widgets::{coincide, derecha, linea_atajos, normalizar, tema, Confirmar, Entrada, Formulario, Respuesta, Resultado};
use crate::tui::Ctx;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

/* ================= piezas que también usa Facturas ================= */

/// Mayúsculas y sin espacios, guiones ni puntos.
pub fn limpia_nif(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '.').flat_map(|c| c.to_uppercase()).collect()
}

/// ¿Es un NIF, NIE o CIF español válido?
pub fn validar_nif(raw: &str) -> bool {
    let s: Vec<char> = limpia_nif(raw).chars().collect();
    if s.len() != 9 {
        return false;
    }
    const LETRAS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";
    let digitos = |a: usize, b: usize| s[a..b].iter().all(|c| c.is_ascii_digit());
    let numero = |a: usize, b: usize| s[a..b].iter().collect::<String>().parse::<u64>().unwrap_or(0);
    let letra = |n: u64| LETRAS[(n % 23) as usize] as char;
    let ultima = s[8];
    if digitos(0, 8) && ultima.is_ascii_uppercase() {
        return letra(numero(0, 8)) == ultima;
    }
    if let Some(i) = "XYZ".find(s[0]) {
        return digitos(1, 8) && ultima.is_ascii_uppercase() && letra(i as u64 * 10_000_000 + numero(1, 8)) == ultima;
    }
    if "KLM".contains(s[0]) {
        return digitos(1, 8) && ultima.is_ascii_uppercase() && letra(numero(1, 8)) == ultima;
    }
    if "ABCDEFGHJNPQRSUVW".contains(s[0]) && digitos(1, 8) {
        let mut suma = 0;
        for (i, c) in s[1..8].iter().enumerate() {
            let mut n = c.to_digit(10).unwrap_or(0);
            if i % 2 == 0 {
                n *= 2;
                n = n / 10 + n % 10;
            }
            suma += n;
        }
        let c = ((10 - suma % 10) % 10) as usize;
        return ultima == char::from_digit(c as u32, 10).unwrap_or('?') || ultima == "JABCDEFGHI".as_bytes()[c] as char;
    }
    false
}

/// Las operaciones de `OPERACIONES` con un nombre corto, para que quepan en un campo de opciones (el largo va
/// en la ayuda del campo).
pub fn opciones_operacion() -> Vec<(&'static str, &'static str)> {
    OPERACIONES
        .iter()
        .map(|&(k, n)| {
            let corto = match k {
                "nacional" => "Con IVA (España o particular UE)",
                "ue" => "Sin IVA: empresa de la UE",
                "extra" => "Sin IVA: fuera de la UE",
                "exenta" => "Exenta de IVA (art. 20)",
                _ => n,
            };
            (k, corto)
        })
        .collect()
}

/// El formulario de un cliente (nuevo o para editar).
pub fn formulario_cliente(c: &Cliente, titulo: &str) -> Formulario {
    let pais = if c.pais.is_empty() { "ES" } else { c.pais.as_str() };
    let op = if c.operacion.is_empty() { "nacional" } else { c.operacion.as_str() };
    let ayuda_op = OPERACIONES.iter().find(|o| o.0 == op).map(|o| o.1).unwrap_or("");
    Formulario::new(titulo)
        .texto("nombre", "Nombre o razón social", &c.nombre)
        .texto("nif", "NIF / nº de IVA", &c.nif)
        .ayuda("De fuera de España, su número fiscal (VAT, Tax ID…). De la UE, el de VIES (FR…, DE…).")
        .texto("pais", "País", pais)
        .ayuda("Código de 2 letras: ES, FR, DE, US…")
        .texto("direccion", "Dirección", &c.direccion)
        .texto("cp", "C.P.", &c.cp)
        .texto("ciudad", "Ciudad", &c.ciudad)
        .texto("email", "Email", &c.email)
        .opciones("operacion", "Operación habitual", &opciones_operacion(), op)
        .ayuda(&format!("{ayuda_op}. Se pone sola al elegirlo en una factura."))
}

/// Pasa lo escrito en el formulario a `c` (solo si todo está bien; si no, devuelve qué falla).
pub fn leer_cliente(f: &Formulario, c: &mut Cliente) -> Result<(), String> {
    let nombre = f.get("nombre");
    if nombre.is_empty() {
        return Err("Pon el nombre del cliente.".into());
    }
    let pais = match f.get("pais").to_uppercase() {
        p if p.is_empty() => "ES".to_string(),
        p => p,
    };
    if pais.chars().count() != 2 || !pais.chars().all(|c| c.is_ascii_uppercase()) {
        return Err("El país va con su código de 2 letras (ES, FR, US…).".into());
    }
    let email = f.get("email");
    if !email.is_empty() && (!email.contains('@') || email.contains(' ')) {
        return Err("El email no parece válido.".into());
    }
    c.nombre = nombre;
    c.nif = limpia_nif(&f.get("nif"));
    c.pais = pais;
    c.direccion = f.get("direccion");
    c.cp = f.get("cp");
    c.ciudad = f.get("ciudad");
    c.email = email;
    c.operacion = f.get("operacion");
    Ok(())
}

/// ¿Es esta factura de este cliente? (mismo id, mismo NIF o mismo nombre)
pub fn es_del_cliente(f: &Factura, c: &Cliente) -> bool {
    let fc = &f.cliente;
    (!c.id.is_empty() && fc.id == c.id) || (!c.nif.is_empty() && limpia_nif(&fc.nif) == limpia_nif(&c.nif)) || (!c.nombre.is_empty() && fc.nombre == c.nombre)
}

/// Facturas emitidas (de cualquier año) y su base total.
fn facturado(d: &Datos, c: &Cliente) -> (usize, f64) {
    factura::emitidas(d).filter(|f| es_del_cliente(f, c)).fold((0, 0.0), |(n, s), f| (n + 1, fmt::r2(s + factura::totales_factura(f).base)))
}

/* ================= la lista ================= */

/// Columnas de la lista, en su orden (se quitan las menos importantes si no caben).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Col {
    Nombre,
    Nif,
    Pais,
    Ciudad,
    Operacion,
    Facturas,
    Facturado,
}

impl Col {
    fn titulo(self) -> &'static str {
        match self {
            Col::Nombre => "nombre",
            Col::Nif => "NIF",
            Col::Pais => "país",
            Col::Ciudad => "ciudad",
            Col::Operacion => "operación",
            Col::Facturas => "facturas",
            Col::Facturado => "facturado",
        }
    }
}

/// Lo que pasa con una tecla en la lista de clientes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tecla {
    Usada,
    /// No es suya: la usa la app (salir, la ayuda…).
    NoUsada,
    /// Esc: volver a las facturas.
    Cerrar,
}

/// La lista de tus clientes, encima de las facturas.
#[derive(Default)]
pub struct Clientes {
    sel: usize,
    tabla: TableState,
    busca: Entrada,
    buscando: bool,
    /// Formulario abierto y la posición del cliente en `datos.clientes` (None: es nuevo).
    form: Option<(Formulario, Option<usize>)>,
    /// Confirmar el borrado (posición del cliente).
    confirmar: Option<(Confirmar, usize)>,
    /// Filas que caben en la tabla (para PgUp / PgDn).
    alto: usize,
}

impl Clientes {
    /// La lista, con el cliente de esa factura elegido (si está guardado).
    pub fn de(d: &Datos, f: Option<&Factura>) -> Self {
        let mut c = Clientes::default();
        if let Some(p) = f.and_then(|f| c.lista(d).iter().position(|&i| es_del_cliente(f, &d.clientes[i]))) {
            c.sel = p;
        }
        c
    }

    /// Índices de los clientes que se ven, por nombre.
    fn lista(&self, d: &Datos) -> Vec<usize> {
        let mut v: Vec<usize> = (0..d.clientes.len())
            .filter(|&i| {
                let c = &d.clientes[i];
                coincide(&format!("{} {} {} {} {}", c.nombre, c.nif, c.pais, c.ciudad, c.email), &self.busca.valor)
            })
            .collect();
        v.sort_by_cached_key(|&i| normalizar(&d.clientes[i].nombre));
        v
    }

    /// Posición en `d.clientes` del cliente elegido.
    fn elegido(&self, d: &Datos) -> Option<usize> {
        self.lista(d).get(self.sel).copied()
    }

    fn ir_a(&mut self, d: &Datos, id: &str) {
        if let Some(p) = self.lista(d).iter().position(|&i| d.clientes[i].id == id) {
            self.sel = p;
        }
    }

    fn guardar_form(&mut self, ctx: &mut Ctx) {
        let Some((fm, pos)) = &mut self.form else { return };
        let pos = pos.filter(|&i| i < ctx.datos.clientes.len());
        let mut c = pos.map(|i| ctx.datos.clientes[i].clone()).unwrap_or_default();
        if let Err(e) = leer_cliente(fm, &mut c) {
            fm.error = Some(e);
            return;
        }
        if c.id.is_empty() {
            c.id = uid();
        }
        let nombre = c.nombre.clone();
        let id = c.id.clone();
        match pos {
            Some(i) => ctx.datos.clientes[i] = c,
            None => ctx.datos.clientes.push(c),
        }
        self.form = None;
        if ctx.guardar() {
            ctx.ok(format!("Cliente «{nombre}» guardado"));
        }
        self.busca = Entrada::default();
        self.ir_a(&ctx.datos, &id);
    }

    fn pedir_borrar(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.elegido(&ctx.datos) else { return };
        let c = &ctx.datos.clientes[i];
        let n = ctx.datos.facturas.iter().filter(|f| es_del_cliente(f, c)).count();
        let mut conf = Confirmar::new("Borrar cliente", "Borrar").peligro().linea(format!("Se quita «{}» de tu lista de clientes.", c.nombre));
        if n > 0 {
            conf = conf.linea(format!("Tiene {n} factura(s): no cambian, cada una guarda sus datos."));
        }
        self.confirmar = Some((conf, i));
    }

    fn mover(&mut self, d: isize, n: usize) {
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + d).clamp(0, n as isize - 1) as usize;
    }
}

impl Clientes {
    /// Escribiendo (buscar, el formulario) o con la ventana de borrar abierta.
    pub fn capturando(&self) -> bool {
        self.form.is_some() || self.confirmar.is_some() || self.buscando
    }

    /// Tus clientes, sin caja, como la lista de facturas: arriba, el título (en el color de la sección), cuántos hay
    /// y, a la derecha si cabe, la tecla de buscar; debajo, la búsqueda o, si no se busca, que cada factura guarda su
    /// copia; y la tabla, con la cabecera en gris, las cifras a la derecha (sin «€», como en las facturas) y el elegido
    /// en el fondo suave de la sección. Si no caben todas las columnas, se quitan las menos importantes.
    pub fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let d = &ctx.datos;
        let lista = self.lista(d);
        self.sel = self.sel.min(lista.len().saturating_sub(1));
        let gris = tema::apagado();
        let cuantos = if self.busca.valor.is_empty() { format!(" ({})", lista.len()) } else { format!(" ({} con «{}»)", lista.len(), self.busca.valor) };
        let titulo = Line::from(vec![Span::styled("Tus clientes", tema::titulo()), Span::styled(cuantos, gris)]);
        let [a_tit, a_sub, _, cuerpo] = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)]).areas(area);
        let usado = titulo.width();
        f.render_widget(Paragraph::new(titulo), a_tit);
        let teclas = linea_atajos(&[("/", "buscar")]);
        if !self.buscando && usado + 5 + teclas.width() <= area.width as usize {
            f.render_widget(Paragraph::new(teclas.right_aligned()), a_tit);
        }
        if self.buscando || !self.busca.valor.is_empty() {
            let [e, t] = Layout::horizontal([Constraint::Length(7), Constraint::Min(1)]).areas(a_sub);
            f.render_widget(Paragraph::new(Span::styled("buscar", gris)), e);
            self.busca.dibujar(f, t, self.buscando, Style::new().add_modifier(Modifier::BOLD));
        } else {
            f.render_widget(Paragraph::new(Span::styled("Cada factura guarda su copia del cliente: cambiarlo aquí no la toca.", gris)), a_sub);
        }
        if lista.is_empty() {
            let txt = if self.busca.valor.is_empty() { "Aún no tienes clientes. Pulsa n para añadir uno (o se guardan solos al emitir una factura)." } else { "Ningún cliente con ese texto." };
            f.render_widget(Paragraph::new(Line::styled(txt, gris)).wrap(Wrap { trim: true }), cuerpo);
        } else {
            self.dibujar_tabla(f, cuerpo, d, &lista);
        }
        if let Some((fm, _)) = &self.form {
            fm.dibujar_ventana(f, area, 76);
        }
        if let Some((c, _)) = &self.confirmar {
            c.dibujar(f, area);
        }
    }

    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect, d: &Datos, lista: &[usize]) {
        let clientes: Vec<&Cliente> = lista.iter().map(|&i| &d.clientes[i]).collect();
        let hechas: Vec<(usize, f64)> = clientes.iter().map(|c| facturado(d, c)).collect();
        let texto = |col: Col, c: &Cliente, (n, base): (usize, f64)| -> String {
            match col {
                Col::Nombre => c.nombre.clone(),
                Col::Nif => c.nif.clone(),
                Col::Pais => c.pais.clone(),
                Col::Ciudad => c.ciudad.clone(),
                Col::Operacion => factura::nombre_operacion(&c.operacion).to_string(),
                Col::Facturas => n.to_string(),
                Col::Facturado => fmt::num(base),
            }
        };
        // El ancho de cada columna: lo más largo de ella, o su título; el nombre, 16 como poco; la ciudad, 16 como mucho.
        let ancho = |col: Col| -> u16 {
            let largo = clientes.iter().zip(&hechas).map(|(c, h)| texto(col, c, *h).chars().count()).max().unwrap_or(0).max(col.titulo().chars().count()) as u16;
            match col {
                Col::Nombre => 16,
                Col::Ciudad => largo.min(16),
                _ => largo,
            }
        };
        const ESPACIO: u16 = 2;
        let mut cols = vec![Col::Nombre, Col::Facturas, Col::Facturado];
        for c in [Col::Nif, Col::Pais, Col::Operacion, Col::Ciudad] {
            let ocupa: u16 = cols.iter().chain([&c]).map(|&c| ancho(c) + ESPACIO).sum();
            if ocupa <= area.width {
                cols.push(c);
            }
        }
        cols.sort();
        let numero = |c: Col| matches!(c, Col::Facturas | Col::Facturado);
        let cabecera = Row::new(cols.iter().map(|&c| if numero(c) { Cell::from(derecha(c.titulo())) } else { Cell::from(c.titulo()) })).style(tema::cabecera_tabla());
        let filas: Vec<Row> = clientes
            .iter()
            .zip(&hechas)
            .map(|(c, h)| {
                Row::new(cols.iter().map(|&col| {
                    let s = texto(col, c, *h);
                    match col {
                        _ if numero(col) => Cell::from(derecha(s)),
                        Col::Operacion => Cell::from(Span::styled(s, tema::apagado())),
                        _ => Cell::from(s),
                    }
                }))
            })
            .collect();
        let anchos = cols.iter().map(|&c| if c == Col::Nombre { Constraint::Fill(1) } else { Constraint::Length(ancho(c)) });
        let tabla = Table::new(filas, anchos).header(cabecera).column_spacing(ESPACIO).row_highlight_style(tema::seleccion());
        self.tabla.select(Some(self.sel));
        self.alto = area.height.saturating_sub(1).max(1) as usize;
        f.render_stateful_widget(tabla, area, &mut self.tabla);
    }

    pub fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> Tecla {
        if let Some((c, i)) = &self.confirmar {
            match c.tecla(k) {
                Respuesta::Si => {
                    let i = *i;
                    self.confirmar = None;
                    if i < ctx.datos.clientes.len() {
                        let c = ctx.datos.clientes.remove(i);
                        if ctx.guardar() {
                            ctx.ok(format!("Cliente «{}» borrado", c.nombre));
                        }
                    }
                    self.sel = self.sel.min(self.lista(&ctx.datos).len().saturating_sub(1));
                }
                Respuesta::No => self.confirmar = None,
                Respuesta::Sigue => {}
            }
            return Tecla::Usada;
        }
        if let Some((fm, _)) = &mut self.form {
            match fm.tecla(k) {
                Resultado::Guardar => self.guardar_form(ctx),
                Resultado::Cancelar => self.form = None,
                Resultado::Sigue | Resultado::Cambio => {}
            }
            return Tecla::Usada;
        }
        let n = self.lista(&ctx.datos).len();
        if self.buscando {
            match k.code {
                KeyCode::Esc => {
                    self.busca = Entrada::default();
                    self.buscando = false;
                }
                KeyCode::Enter => self.buscando = false,
                KeyCode::Up => self.mover(-1, n),
                KeyCode::Down => self.mover(1, n),
                _ => {
                    if self.busca.tecla(k) {
                        self.sel = 0;
                    }
                }
            }
            return Tecla::Usada;
        }
        let pagina = self.alto.max(1) as isize;
        match k.code {
            KeyCode::Up => self.mover(-1, n),
            KeyCode::Down => self.mover(1, n),
            KeyCode::PageUp => self.mover(-pagina, n),
            KeyCode::PageDown => self.mover(pagina, n),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = n.saturating_sub(1),
            KeyCode::Char('/') => self.buscando = true,
            KeyCode::Esc if !self.busca.valor.is_empty() => {
                self.busca = Entrada::default();
                self.sel = 0;
            }
            KeyCode::Esc => return Tecla::Cerrar,
            KeyCode::Char('n') => {
                let c = Cliente { pais: "ES".into(), operacion: "nacional".into(), ..Default::default() };
                self.form = Some((formulario_cliente(&c, "Nuevo cliente"), None));
            }
            KeyCode::Enter => match self.elegido(&ctx.datos) {
                Some(i) => self.form = Some((formulario_cliente(&ctx.datos.clientes[i], "Editar cliente"), Some(i))),
                None => ctx.info("No hay ningún cliente: pulsa n para añadir uno."),
            },
            KeyCode::Char('d') | KeyCode::Delete => self.pedir_borrar(ctx),
            _ => return Tecla::NoUsada,
        }
        Tecla::Usada
    }

    pub fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.confirmar.is_some() {
            vec![("Enter/s", "borrar"), ("Esc/n", "cancelar")]
        } else if self.form.is_some() {
            vec![("↑↓", "campo"), ("← →", "opción"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")]
        } else if self.buscando {
            vec![("Enter", "aceptar"), ("Esc", "borrar la búsqueda")]
        } else {
            vec![("Esc", "volver"), ("Enter", "editar"), ("n", "nuevo"), ("d", "borrar"), ("/", "buscar")]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::prueba::{app_con, cargar, caso, pantalla, teclas};
    use crate::tui::tests::factura_de_hoy;

    /// Tus clientes se abren desde Facturas (`C`), con el de la factura elegida, sin caja y con la cabecera de las
    /// columnas en minúscula; se cambian, se crean y se borran como antes, sin tocar ninguna factura.
    #[test]
    fn desde_las_facturas() {
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        // La primera factura es de «Zeta Pruebas», el último de tus clientes.
        factura_de_hoy(&mut d);
        let zeta = d.facturas.last().unwrap().cliente.clone();
        d.clientes = vec![Cliente { id: "c-alfa".into(), nombre: "Alfa".into(), ..Default::default() }, Cliente { id: "c-beta".into(), nombre: "Beta".into(), ..Default::default() }, zeta];
        let nombres = |a: &crate::tui::App| a.ctx.datos.clientes.iter().map(|c| c.nombre.clone()).collect::<Vec<_>>();
        let mut a = app_con(d, &dir);
        teclas(&mut a, "a1C");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains("Tus clientes (3)") && !p.contains('│') && !p.contains('┌'), "{p}");
        let cabecera = p.lines().find(|l| l.contains("nombre")).unwrap_or_default();
        assert!(cabecera.contains("facturas") && cabecera.contains("facturado"), "{p}");
        assert!(p.lines().any(|l| l.contains("Zeta Pruebas") && l.trim_end().ends_with(" 100,00")), "{p}");
        // Enter cambia el elegido: el de la factura.
        teclas(&mut a, "<enter><c-u>Zeta S.L.<c-s>");
        assert_eq!(nombres(&a), ["Alfa", "Beta", "Zeta S.L."]);
        assert_eq!(a.ctx.datos.facturas.last().unwrap().cliente.nombre, "Zeta Pruebas", "las facturas no cambian");
        teclas(&mut a, "nTercero<c-s>");
        assert_eq!(nombres(&a), ["Alfa", "Beta", "Zeta S.L.", "Tercero"]);
        // Borra el elegido (el nuevo, que queda elegido).
        teclas(&mut a, "d<enter>");
        assert_eq!(nombres(&a), ["Alfa", "Beta", "Zeta S.L."]);
        teclas(&mut a, "<esc>");
        assert!(!pantalla(&mut a, 120, 36).contains("Tus clientes"));
        assert_eq!(a.titulo_activa(), "Facturas");
    }

    #[test]
    fn nif() {
        assert_eq!(limpia_nif(" b-12.345 678 "), "B12345678");
        assert!(validar_nif("12345678Z"));
        assert!(!validar_nif("12345678A"));
        assert!(validar_nif("X1234567L"));
        assert!(validar_nif("B12345674"));
        assert!(!validar_nif(""));
        assert!(!validar_nif("US123"));
    }
}
