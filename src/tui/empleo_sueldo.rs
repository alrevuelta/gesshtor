//! «Sueldo»: tu sueldo del año que miras, en una pantalla.
//!
//! - Arriba, el año en una línea (bruto, IRPF, tu Seguridad Social) y si te retienen bien: con la Renta, lo que
//!   salió; sin ella, lo que te tocaría por tus nóminas apuntadas (`informe::empleo::retencion`).
//! - Debajo, mes a mes: de tus nóminas apuntadas o, si no, de la Seguridad Social (≈, lo cotizado).
//!
//! Enter abre un mes (tu nómina y lo liquidado a la Seguridad Social, concepto a concepto); «c», el camino del
//! dinero, de lo que pagó tu empresa a lo que te llegó; «n» apunta (o cambia) la nómina del mes.
use crate::dominio::fmt::DIF;
use super::empleo_nominas::{formulario, guardar, mes, mes_nuevo};
use super::{empleo_camino as ventana_camino, empleo_mes as ventana_mes};
use super::empleo::{col, col_der, con_importes, eur_duro, parrafo, pct1, pintar_filas, pintar_linea, tabla, Cache, Col, Fila, FilaT, Tabla, RESTO};
use crate::dominio::fmt;
use crate::dominio::fmt::nombre_mes;
use crate::informe::empleo::{camino, meses_sueldo, retencion, Camino, Fuente, MesSueldo, Retencion, Veredicto};
use crate::tui::widgets::{bloque, tema, Formulario, Resultado};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding};
use ratatui::Frame;

/// Ancho de las columnas de importes.
const COL: u16 = 13;

#[derive(Default)]
pub struct Sueldo {
    cache: Cache,
    meses: Vec<MesSueldo>,
    retencion: Option<Retencion>,
    camino: Option<Camino>,
    /// El mes elegido (índice en `meses`) y la primera fila que se ve.
    sel: usize,
    desde: usize,
    modo: Modo,
}

/// Lo que hay abierto encima de la tabla.
#[derive(Default)]
enum Modo {
    #[default]
    Tabla,
    /// Un mes (índice en `meses`).
    Mes(usize, Ventana),
    Camino(Ventana),
    /// El formulario de la nómina, con el id de la que se cambia (None: nueva).
    Form(Box<Formulario>, Option<String>),
}

/// Una ventana de solo leer, que se desplaza.
#[derive(Default)]
struct Ventana {
    desplaza: u16,
    alto: u16,
    /// Las líneas ya hechas, y con qué datos y ancho se hicieron.
    hechas: Option<(u64, u16)>,
    lineas: Vec<Fila>,
}

impl Ventana {
    /// Pinta la ventana a todo `area`, con las líneas que da `hacer(ancho)` (solo si ha cambiado algo).
    fn dibujar(&mut self, f: &mut Frame, area: Rect, b: Block, generacion: u64, hacer: impl FnOnce(usize) -> Vec<Fila>) {
        f.render_widget(Clear, area);
        let b = b.padding(Padding::horizontal(1));
        let dentro = b.inner(area);
        if self.hechas != Some((generacion, dentro.width)) {
            self.hechas = Some((generacion, dentro.width));
            self.lineas = hacer(dentro.width as usize);
        }
        self.alto = dentro.height;
        self.desplaza = self.desplaza.min((self.lineas.len() as u16).saturating_sub(dentro.height));
        pintar_filas(f, area, b, &self.lineas, self.desplaza as usize);
    }

    /// Las teclas de desplazar. Devuelve false si no es una de ellas.
    fn tecla(&mut self, k: KeyEvent) -> bool {
        let pagina = (self.alto / 2).max(1);
        match k.code {
            KeyCode::Up => self.desplaza = self.desplaza.saturating_sub(1),
            KeyCode::Down => self.desplaza = self.desplaza.saturating_add(1),
            KeyCode::PageUp => self.desplaza = self.desplaza.saturating_sub(pagina),
            KeyCode::PageDown => self.desplaza = self.desplaza.saturating_add(pagina),
            KeyCode::Home => self.desplaza = 0,
            _ => return false,
        }
        true
    }
}

/// Cómo se dice lo que salió en la Renta (> 0 a pagar, < 0 a devolver), con el veredicto y su color.
fn veredicto_renta(r: f64, irpf: f64) -> (&'static str, ratatui::style::Color) {
    if r.abs() <= irpf * 0.05 {
        ("✓ casi lo justo", tema::BIEN)
    } else if r > 0.0 {
        ("te retuvieron poco", tema::AVISO)
    } else {
        ("te retuvieron de más", tema::AVISO)
    }
}

/// «¿Te retienen bien?» y su veredicto, en una línea.
fn pregunta(pregunta: &str, veredicto: Option<(&str, ratatui::style::Color)>) -> Line<'static> {
    let mut v = vec![Span::styled(pregunta.to_string(), tema::titulo())];
    if let Some((t, c)) = veredicto {
        v.push(Span::raw("  "));
        v.push(Span::styled(t.to_string(), Style::new().fg(c).add_modifier(Modifier::BOLD)));
    }
    Line::from(v)
}

/// «¿Te retienen bien?» con tus nóminas apuntadas: lo que te retienen y lo que te tocaría.
fn lineas_estimacion(r: &Retencion, y: i32, w: usize) -> Vec<Line<'static>> {
    let gris = tema::apagado();
    let duro = |x: f64| pct1(x).replace(' ', "\u{a0}");
    let (etiqueta, color) = match r.veredicto() {
        Veredicto::Bien => ("✓ vas bien", tema::BIEN),
        Veredicto::Poco => ("te retienen poco", tema::AVISO),
        Veredicto::Mucho => ("te retienen de más", tema::AVISO),
    };
    let mut v = vec![pregunta("¿Te retienen bien?", Some((etiqueta, color)))];
    let mut s = format!("Te retienen el {} y por tu sueldo te tocaría en torno al {}.", duro(r.retenido_pct()), duro(r.toca_pct()));
    let x = r.renta();
    match r.veredicto() {
        Veredicto::Bien => {}
        Veredicto::Poco => s += &format!(" Si sigue así, en la Renta saldrá a pagar unos {}.", eur_duro0(x)),
        Veredicto::Mucho => s += &format!(" Si sigue así, en la Renta te devolverán unos {}.", eur_duro0(-x)),
    }
    v.extend(con_importes(&s, w, Style::new()));
    v.extend(parrafo(&r.supuestos(y), w, gris));
    v
}

/// «12.345 €» con espacio duro (para `con_importes`).
fn eur_duro0(x: f64) -> String {
    fmt::eur0(x).replace(' ', "\u{a0}")
}

impl Sueldo {
    /// Recalcula si han cambiado los datos o el año; con otro año se elige el último mes y se cierra lo abierto.
    fn al_dia(&mut self, ctx: &Ctx) {
        let antes = self.cache.clave.map(|c| c.1);
        if !self.cache.al_dia(ctx) {
            return;
        }
        let a = &self.cache.anio;
        self.meses = meses_sueldo(&ctx.datos, a);
        self.retencion = retencion(&ctx.datos, &self.meses, a.anio);
        self.camino = camino(&ctx.datos, a);
        if antes != Some(ctx.anio) {
            self.sel = self.meses.len().saturating_sub(1);
            self.desde = 0;
            if !matches!(self.modo, Modo::Form(..)) {
                self.modo = Modo::Tabla;
            }
        }
        self.sel = self.sel.min(self.meses.len().saturating_sub(1));
        if matches!(self.modo, Modo::Mes(i, _) if i >= self.meses.len()) {
            self.modo = Modo::Tabla;
        }
    }

    /// ¿Hay algo que enseñar mes a mes?
    fn hay_meses(&self) -> bool {
        self.meses.iter().any(|m| !m.vacio())
    }

    /// Lo de arriba de la tabla: el año en una línea, de dónde sale y si te retienen bien.
    fn arriba(&self, w: usize) -> Vec<Line<'static>> {
        let a = &self.cache.anio;
        let y = a.anio;
        let gris = tema::apagado();
        let mut v = Vec::new();
        if !a.hay() {
            v.extend(parrafo(&format!("No hay sueldo por cuenta ajena en {y}: ni en tu Renta, ni en tus nóminas apuntadas, ni en la Seguridad Social."), w, Style::new()));
            v.push(Line::from(""));
            v.push(Line::styled("Pulsa n para apuntar una nómina.", Style::new().fg(tema::ACENTO)));
            return v;
        }
        let mut partes = Vec::new();
        if let Some(b) = a.bruto {
            partes.push(format!("Bruto {}{}", if b.de == Fuente::Cotizado { "≈" } else { "" }, eur_duro(b.v)));
        }
        if let Some(i) = a.irpf {
            partes.push(format!("IRPF {}", eur_duro(i.v)));
        }
        if let Some(s) = a.tu_ss {
            partes.push(format!("Seguridad Social {}", eur_duro(s.v)));
        }
        if let Some(c) = &self.camino {
            partes.push(format!("te llegaron {}", eur_duro(c.dinero)));
        }
        v.extend(con_importes(&partes.join(" · "), w, Style::new()));
        let fuente = match a.bruto.map(|b| b.de) {
            Some(Fuente::Nominas) => {
                let n = self.meses.iter().filter(|m| m.nomina.is_some()).count();
                format!("tus nóminas apuntadas ({n} {})", if n == 1 { "mes" } else { "meses" })
            }
            Some(Fuente::Cotizado) => "la Seguridad Social: el bruto es lo cotizado, aproximado".into(),
            Some(de) => format!("{}{}", de.texto(), if a.empresa.is_some() { " y la Seguridad Social" } else { "" }),
            None => "la Seguridad Social".into(),
        };
        let donde = match a.empresas.as_slice() {
            [] => String::new(),
            [e] => format!("En {e}, según "),
            [e1, e2] => format!("En {e1} y {e2}, según "),
            v => format!("En {} empresas, según ", v.len()),
        };
        let fuente = if donde.is_empty() { format!("Según {fuente}.") } else { format!("{donde}{fuente}.") };
        v.extend(parrafo(&fuente, w, gris));
        v.push(Line::from(""));
        // ¿Te retienen bien?
        match (&self.camino, &self.retencion) {
            (Some(c), _) if c.resultado_renta.is_some() => {
                let r = c.resultado_renta.unwrap_or(0.0);
                v.push(pregunta("¿Te retuvieron bien?", Some(veredicto_renta(r, c.irpf()))));
                let duro = |x: f64| fmt::pct(x).replace(' ', "\u{a0}");
                let mut s = format!("Te retuvieron el {} de tu sueldo", duro(c.retenido_pct()));
                if let Some((e, a)) = c.tipo_medio {
                    s += &format!(" y tu tipo medio en la Renta fue el {} ({} estatal y {} autonómico)", duro(e + a), duro(e), duro(a));
                }
                let que = if r > DIF { format!("salió a pagar {}", eur_duro(r)) } else if r < -DIF { format!("salió a devolver {}", eur_duro(-r)) } else { "salió a cero".into() };
                s += &format!(". Al hacer la Renta, con todo lo del año (no solo el sueldo), {que}.");
                v.extend(con_importes(&s, w, Style::new()));
            }
            (_, Some(r)) => v.extend(lineas_estimacion(r, y, w)),
            _ => {
                v.push(pregunta("¿Te retienen bien?", None));
                v.extend(parrafo("Para saberlo, apunta tus nóminas (n): el IRPF que te retienen solo viene en ellas.", w, gris));
            }
        }
        if !self.hay_meses() {
            v.push(Line::from(""));
            let s = if a.ss.is_none() {
                format!("Mes a mes: la Seguridad Social no está consultada para {y} y no has apuntado nóminas.")
            } else {
                format!("Mes a mes: la Seguridad Social no tiene el detalle de tus nóminas de {y} (solo lo da de las liquidadas con el sistema actual) y no has apuntado ninguna.")
            };
            v.extend(parrafo(&s, w, gris));
        }
        v
    }

    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect) {
        let w = area.width;
        let con_empresa = w >= 84;
        let aprox = self.meses.iter().any(|m| m.nomina.is_none() && m.ss.is_some());
        let gris = tema::apagado();
        let raya = || Line::styled("—", gris);
        let mut filas: Vec<FilaT> = self
            .meses
            .iter()
            .map(|m| {
                let mut c = vec![Line::from(mes(m.mes()))];
                if con_empresa {
                    c.push(if m.vacio() { Line::styled("sin nómina", gris) } else { Line::from(m.empresas().join(", ")) });
                }
                c.push(match m.bruto() {
                    Some((b, true)) => Line::styled(format!("≈{}", fmt::eur(b)), gris),
                    Some((b, false)) => Line::from(fmt::eur(b)),
                    None => raya(),
                });
                match &m.nomina {
                    Some(n) => c.extend([Line::from(fmt::eur(n.irpf)), Line::styled(pct1(n.retenido_pct()), gris)]),
                    None => c.extend([raya(), raya()]),
                }
                c.push(m.tu_ss().map_or_else(raya, |x| Line::from(fmt::eur(x))));
                FilaT::new(c)
            })
            .collect();
        // El total del año, al final.
        let bruto: f64 = self.meses.iter().filter_map(|m| m.bruto()).map(|b| b.0).sum();
        let con: Vec<_> = self.meses.iter().filter_map(|m| m.nomina.as_ref()).collect();
        let irpf: f64 = con.iter().map(|n| n.irpf).sum();
        let bruto_nominas: f64 = con.iter().map(|n| n.bruto).sum();
        let ss: f64 = self.meses.iter().filter_map(|m| m.tu_ss()).sum();
        let mut c = vec![Line::from("Total")];
        if con_empresa {
            c.push(Line::from(""));
        }
        c.push(Line::from(format!("{}{}", if aprox { "≈" } else { "" }, fmt::eur(fmt::r2(bruto)))));
        if con.is_empty() {
            c.extend([raya(), raya()]);
        } else {
            c.extend([Line::from(fmt::eur(fmt::r2(irpf))), Line::from(pct1(irpf / bruto_nominas.max(DIF) * 100.0))]);
        }
        c.push(Line::from(fmt::eur(fmt::r2(ss))));
        filas.push(FilaT::new(c).estilo(Style::new().add_modifier(Modifier::BOLD)));

        let mut cols: Vec<Col> = vec![col(11)];
        let mut cab = vec!["Mes"];
        if con_empresa {
            cols.push(RESTO);
            cab.push("Empresa");
        }
        cols.extend([col_der(COL + 1), col_der(COL - 1), col_der(7), col_der(COL)]);
        cab.extend(["Bruto", "IRPF", "%", "Seg. Social"]);
        if !con_empresa {
            cols[0] = RESTO;
        }
        tabla(f, area, Block::new(), &Tabla { cols: &cols, cab: &cab, filas }, Some(self.sel), &mut self.desde);
    }

    /// El formulario de la nómina de un mes: la que haya (si hay una) o una nueva.
    fn apuntar(&mut self, ctx: &mut Ctx, i: Option<usize>) {
        let y = ctx.anio;
        let m = i.and_then(|i| self.meses.get(i)).map_or_else(|| mes_nuevo(&ctx.datos, y), |m| m.mes());
        let del_mes: Vec<_> = ctx.datos.nominas.iter().filter(|n| n.anio == y && n.mes == m).collect();
        let (fm, id) = match del_mes.as_slice() {
            [n] => (formulario(&ctx.datos, y, m, Some(n)), Some(n.id.clone())),
            _ => (formulario(&ctx.datos, y, m, None), None),
        };
        self.modo = Modo::Form(Box::new(fm), id);
    }

    /// La nómina del mes elegido: ¿hay una que cambiar?
    fn hay_nomina(&self) -> bool {
        self.meses.get(self.sel).is_some_and(|m| m.nomina.is_some())
    }
}

impl Pantalla for Sueldo {
    fn titulo(&self) -> &'static str {
        "Sueldo"
    }

    fn capturando(&self) -> bool {
        !matches!(self.modo, Modo::Tabla)
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.al_dia(ctx);
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.al_dia(ctx);
        let y = ctx.anio;
        let aprox = self.meses.iter().any(|m| m.nomina.is_none() && m.ss.is_some());
        let titulo = format!("Tu sueldo en {y}");
        let mut b = bloque(&titulo);
        if aprox {
            b = b.title_bottom(Line::styled(" ≈ lo cotizado: apunta la nómina para saber el bruto y el IRPF ", tema::apagado()));
        }
        let dentro = b.inner(area);
        f.render_widget(b, area);
        let arriba = self.arriba(dentro.width.saturating_sub(2) as usize);
        let tabla = self.hay_meses();
        let alto = if tabla { (arriba.len() as u16 + 1).min(dentro.height.saturating_sub(5)) } else { dentro.height };
        let [za, zt] = Layout::vertical([Constraint::Length(alto), Constraint::Min(0)]).areas(dentro);
        for (i, l) in arriba.iter().take(za.height as usize).enumerate() {
            pintar_linea(f, Rect { x: za.x + 1, y: za.y + i as u16, width: za.width.saturating_sub(2), height: 1 }, l);
        }
        if tabla {
            self.dibujar_tabla(f, Rect { x: zt.x + 1, width: zt.width.saturating_sub(2), ..zt });
        }

        // Lo que haya abierto encima.
        let generacion = ctx.generacion;
        match &mut self.modo {
            Modo::Tabla => {}
            Modo::Mes(i, ven) => {
                let Some(m) = self.meses.get(*i) else { return };
                let t = nombre_mes(&m.ym);
                let t = format!("{}{}", t[..1].to_uppercase(), &t[1..]);
                let b = bloque(&t).title_bottom(Line::styled(" ← → otro mes · n nómina · Esc volver ", tema::apagado()).right_aligned());
                ven.dibujar(f, area, b, generacion, |w| ventana_mes::lineas(&ctx.datos, m, w));
            }
            Modo::Camino(ven) => {
                let Some(c) = &self.camino else { return };
                let t = format!("{} · {y}", ventana_camino::titulo(c));
                let b = bloque(&t).title_bottom(Line::styled(" Esc volver ", tema::apagado()).right_aligned());
                let a = &self.cache.anio;
                ven.dibujar(f, area, b, generacion, |w| ventana_camino::lineas(c, a, w));
            }
            Modo::Form(fm, _) => fm.dibujar_ventana(f, area, 72),
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        self.al_dia(ctx);
        let n = self.meses.len();
        match &mut self.modo {
            Modo::Form(fm, id) => {
                match fm.tecla(k) {
                    Resultado::Guardar => {
                        let id = id.clone();
                        if let Some(nuevo) = guardar(fm, id.as_deref(), ctx.anio, ctx) {
                            self.modo = Modo::Tabla;
                            self.al_dia(ctx);
                            let m = ctx.datos.nominas.iter().find(|x| x.id == nuevo).map_or(0, |x| x.mes);
                            if let Some(i) = self.meses.iter().position(|x| x.mes() == m) {
                                self.sel = i;
                            }
                        }
                    }
                    Resultado::Cancelar => self.modo = Modo::Tabla,
                    Resultado::Sigue | Resultado::Cambio => {}
                }
                return true;
            }
            Modo::Mes(i, ven) => {
                if !ven.tecla(k) {
                    match k.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace => self.modo = Modo::Tabla,
                        KeyCode::Left if *i > 0 => {
                            self.sel = *i - 1;
                            self.modo = Modo::Mes(self.sel, Ventana::default());
                        }
                        KeyCode::Right if *i + 1 < n => {
                            self.sel = *i + 1;
                            self.modo = Modo::Mes(self.sel, Ventana::default());
                        }
                        KeyCode::Char('n') => {
                            let i = *i;
                            self.apuntar(ctx, Some(i));
                        }
                        _ => {}
                    }
                }
                return true;
            }
            Modo::Camino(ven) => {
                if !ven.tecla(k) && matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace | KeyCode::Char('c')) {
                    self.modo = Modo::Tabla;
                }
                return true;
            }
            Modo::Tabla => {}
        }
        let hay = self.hay_meses();
        match k.code {
            KeyCode::Up if hay => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down if hay => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::Home if hay => self.sel = 0,
            KeyCode::End if hay => self.sel = n.saturating_sub(1),
            KeyCode::Enter if hay => match self.meses.get(self.sel) {
                Some(m) if m.vacio() => self.apuntar(ctx, Some(self.sel)),
                Some(_) => self.modo = Modo::Mes(self.sel, Ventana::default()),
                None => {}
            },
            KeyCode::Char('n') => self.apuntar(ctx, if hay { Some(self.sel) } else { None }),
            KeyCode::Char('c') => match self.camino {
                Some(_) => self.modo = Modo::Camino(Ventana::default()),
                None => ctx.info(format!("Para ver de lo que pagó tu empresa a lo que te llegó hace falta tu Renta de {} o tus nóminas.", ctx.anio)),
            },
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        match &self.modo {
            Modo::Form(..) => vec![("Tab ↓", "siguiente"), ("← →", "mes"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")],
            Modo::Mes(..) => vec![("← →", "otro mes"), ("n", "nómina del mes"), ("↑↓", "desplazar"), ("Esc", "volver")],
            Modo::Camino(..) => vec![("↑↓ PgUp PgDn", "desplazar"), ("Esc", "volver")],
            Modo::Tabla if !self.hay_meses() => vec![("n", "apuntar nómina"), ("c", "de tu empresa a ti")],
            Modo::Tabla => {
                let vacio = self.meses.get(self.sel).is_none_or(|m| m.vacio());
                let n = if self.hay_nomina() { "cambiar nómina" } else { "apuntar nómina" };
                vec![("↑↓", "mes"), ("Enter", if vacio { "apuntar nómina" } else { "ver el mes" }), ("n", n), ("c", "de tu empresa a ti")]
            }
        }
    }
}
