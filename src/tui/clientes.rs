//! Clientes: los que tienes guardados (se guardan solos al emitir una factura). Cada factura lleva su
//! propia copia de los datos del cliente, así que cambiar o borrar uno aquí no toca ninguna factura.
use crate::dominio::factura::{self, OPERACIONES};
use crate::dominio::fmt;
use crate::dominio::{Cliente, Datos, Factura, uid};
use crate::tui::widgets::{bloque, coincide, derecha, normalizar, tema, Confirmar, Entrada, Formulario, Respuesta, Resultado};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
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

/* ================= la pantalla ================= */

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

impl Pantalla for Clientes {
    fn titulo(&self) -> &'static str {
        "Clientes"
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        let n = self.lista(&ctx.datos).len();
        self.sel = self.sel.min(n.saturating_sub(1));
    }

    fn usa_anio(&self) -> bool {
        false
    }

    fn capturando(&self) -> bool {
        self.form.is_some() || self.confirmar.is_some() || self.buscando
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let d = &ctx.datos;
        let lista = self.lista(d);
        self.sel = self.sel.min(lista.len().saturating_sub(1));
        let titulo = if self.busca.valor.is_empty() { format!("Clientes ({})", lista.len()) } else { format!("Clientes ({} con «{}»)", lista.len(), self.busca.valor) };
        let b = bloque(&titulo);
        let dentro = b.inner(area);
        f.render_widget(b, area);
        let [barra, cuerpo] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(dentro);
        // Barra de búsqueda
        if self.buscando || !self.busca.valor.is_empty() {
            let [etq, txt] = Layout::horizontal([Constraint::Length(9), Constraint::Min(1)]).areas(barra);
            f.render_widget(Paragraph::new(Span::styled(" Buscar: ", tema::apagado())), etq);
            self.busca.dibujar(f, txt, self.buscando, Style::new().add_modifier(Modifier::BOLD));
        } else {
            f.render_widget(Paragraph::new(Span::styled(" Cada factura guarda su copia del cliente: cambiarlo aquí no la toca.", tema::apagado())), barra);
        }
        if lista.is_empty() {
            let txt = if self.busca.valor.is_empty() { "Aún no tienes clientes. Pulsa n para añadir uno (o se guardan solos al emitir una factura)." } else { "Ningún cliente con ese texto." };
            f.render_widget(Paragraph::new(Line::styled(format!(" {txt}"), tema::apagado())).wrap(ratatui::widgets::Wrap { trim: true }), cuerpo);
        } else {
            let ancho = cuerpo.width;
            let con_ciudad = ancho >= 100;
            let mut cab = vec!["Nombre", "NIF", "País"];
            if con_ciudad {
                cab.push("Ciudad");
            }
            cab.extend(["Operación", "Facturas", "Facturado"]);
            let mut anchos = vec![Constraint::Fill(1), Constraint::Length(13), Constraint::Length(4)];
            if con_ciudad {
                anchos.push(Constraint::Length(16));
            }
            anchos.extend([Constraint::Length(14), Constraint::Length(8), Constraint::Length(14)]);
            let filas: Vec<Row> = lista
                .iter()
                .map(|&i| {
                    let c = &d.clientes[i];
                    let (n, base) = facturado(d, c);
                    let mut v = vec![Cell::from(c.nombre.clone()), Cell::from(c.nif.clone()), Cell::from(c.pais.clone())];
                    if con_ciudad {
                        v.push(Cell::from(c.ciudad.clone()));
                    }
                    v.push(Cell::from(factura::nombre_operacion(&c.operacion)));
                    v.push(Cell::from(derecha(n.to_string())));
                    v.push(Cell::from(derecha(fmt::eur(base))));
                    Row::new(v)
                })
                .collect();
            let cabecera = Row::new(cab.into_iter().enumerate().map(|(i, t)| if i >= 4 + con_ciudad as usize { Cell::from(derecha(t)) } else { Cell::from(t) }))
                .style(tema::cabecera_tabla());
            let tabla = Table::new(filas, anchos).header(cabecera).column_spacing(1).row_highlight_style(tema::seleccion());
            self.tabla.select(Some(self.sel));
            self.alto = cuerpo.height.saturating_sub(1).max(1) as usize;
            f.render_stateful_widget(tabla, cuerpo, &mut self.tabla);
        }
        if let Some((fm, _)) = &self.form {
            fm.dibujar_ventana(f, area, 76);
        }
        if let Some((c, _)) = &self.confirmar {
            c.dibujar(f, area);
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
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
                    self.al_entrar(ctx);
                }
                Respuesta::No => self.confirmar = None,
                Respuesta::Sigue => {}
            }
            return true;
        }
        if let Some((fm, _)) = &mut self.form {
            match fm.tecla(k) {
                Resultado::Guardar => self.guardar_form(ctx),
                Resultado::Cancelar => self.form = None,
                Resultado::Sigue | Resultado::Cambio => {}
            }
            return true;
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
            return true;
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
            KeyCode::Char('n') => {
                let c = Cliente { pais: "ES".into(), operacion: "nacional".into(), ..Default::default() };
                self.form = Some((formulario_cliente(&c, "Nuevo cliente"), None));
            }
            KeyCode::Enter | KeyCode::Char('e') => match self.elegido(&ctx.datos) {
                Some(i) => self.form = Some((formulario_cliente(&ctx.datos.clientes[i], "Editar cliente"), Some(i))),
                None => ctx.info("No hay ningún cliente: pulsa n para añadir uno."),
            },
            KeyCode::Char('d') | KeyCode::Delete => self.pedir_borrar(ctx),
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        if self.confirmar.is_some() {
            vec![("Enter/s", "borrar"), ("Esc/n", "cancelar")]
        } else if self.form.is_some() {
            vec![("↑↓", "campo"), ("← →", "opción"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")]
        } else if self.buscando {
            vec![("Enter", "aceptar"), ("Esc", "borrar la búsqueda")]
        } else {
            vec![("↑↓", "mover"), ("n", "nuevo"), ("Enter", "editar"), ("d", "borrar"), ("/", "buscar")]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
