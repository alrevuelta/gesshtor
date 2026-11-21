//! «Sueldo»: tu sueldo del año que miras, en una pantalla (estilo C2, sin cajas).
//!
//! - Arriba, de dónde sale; lo que te llegó en dinero, en una banda (la idea principal), y el bruto, el IRPF y tu
//!   Seguridad Social con puntos guía. Al lado, si te retienen bien: con la Renta, lo que salió; sin ella, lo que te
//!   tocaría por tus nóminas apuntadas (`informe::empleo::retencion`).
//! - Debajo, mes a mes: de tus nóminas apuntadas o, si no, de la Seguridad Social (≈, lo cotizado).
//!
//! Enter abre un mes (tu nómina y lo liquidado a la Seguridad Social, concepto a concepto); «n» apunta (o cambia)
//! la nómina del mes. De lo que pagó tu empresa a lo que te llegó, paso a paso, está en «Años», al abrir el año.
use crate::dominio::fmt::DIF;
use super::empleo_nominas::{formulario, guardar, mes, mes_nuevo};
use super::empleo_mes as ventana_mes;
use super::empleo::{col, col_der, con_importes, en_banda, eur_duro, parrafo, pct1, pintar_filas, tabla, Cache, Col, Fila, FilaT, Tabla, RESTO};
use crate::dominio::fmt;
use crate::dominio::fmt::nombre_mes;
use crate::informe::empleo::{camino, meses_sueldo, retencion, Camino, Fuente, MesSueldo, Retencion, Veredicto};
use crate::tui::widgets::{banda, filas_partidas, guia, linea_atajos, pie_ventana, sello, tema, titulo_bloque, ventana, Formulario, Resultado, ALTO_BANDA};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

/// Lo más ancho de la columna de la izquierda (la banda y las guías) y de la de «¿te retienen bien?».
const IZQUIERDA: u16 = 46;
const DERECHA: u16 = 56;
/// Hueco entre las dos columnas de arriba.
const HUECO: u16 = 4;
/// Lo más ancho de la tabla de los meses: lo que ocupan las dos columnas de arriba.
const ANCHO_TABLA: u16 = IZQUIERDA + HUECO + DERECHA;

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
    /// Pinta en `area` (lo de dentro de la ventana) las líneas que da `hacer(ancho)` (solo si ha cambiado algo).
    fn dibujar(&mut self, f: &mut Frame, area: Rect, generacion: u64, hacer: impl FnOnce(usize) -> Vec<Fila>) {
        if self.hechas != Some((generacion, area.width)) {
            self.hechas = Some((generacion, area.width));
            self.lineas = hacer(area.width as usize);
        }
        self.alto = area.height;
        self.desplaza = self.desplaza.min((self.lineas.len() as u16).saturating_sub(area.height));
        pintar_filas(f, area, &self.lineas, self.desplaza as usize);
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

/// Lo que queda de `r` quitando sus `n` primeras filas.
fn debajo(r: Rect, n: u16) -> Rect {
    let n = n.min(r.height);
    Rect { y: r.y + n, height: r.height - n, ..r }
}

/// Cómo se dice lo que salió en la Renta (> 0 a pagar, < 0 a devolver), con el veredicto y su color.
fn veredicto_renta(r: f64, irpf: f64) -> (&'static str, Color) {
    if r.abs() <= irpf * 0.05 {
        ("✓ casi lo justo", tema::BIEN)
    } else if r > 0.0 {
        ("te retuvieron poco", tema::AVISO)
    } else {
        ("te retuvieron de más", tema::AVISO)
    }
}

/// El veredicto en un sello: «▕ TE RETIENEN POCO ▏» (sin el ✓: ya lo dice el color).
fn sello_veredicto(texto: &str, color: Color) -> Line<'static> {
    let fondo = if color == tema::BIEN { tema::SUAVE_BIEN } else { tema::SUAVE_AVISO };
    sello(&texto.trim_start_matches('✓').trim().to_uppercase(), color, fondo)
}

/// «¿Te retienen bien?» con tus nóminas apuntadas: el veredicto, lo que te retienen y lo que te tocaría (con puntos
/// guía acabando en `fin`) y lo que saldría en la Renta. Aparte, con qué se ha calculado. `compacto`: sin filas de
/// aire.
fn lineas_estimacion(r: &Retencion, y: i32, w: usize, fin: u16, compacto: bool) -> (Vec<Line<'static>>, String) {
    let (etiqueta, color) = match r.veredicto() {
        Veredicto::Bien => ("✓ vas bien", tema::BIEN),
        Veredicto::Poco => ("te retienen poco", tema::AVISO),
        Veredicto::Mucho => ("te retienen de más", tema::AVISO),
    };
    let mut v = vec![titulo_bloque("¿te retienen bien?"), sello_veredicto(etiqueta, color)];
    if !compacto {
        v.push(Line::from(""));
    }
    v.push(guia("te retienen el", pct1(r.retenido_pct()), Style::new(), fin));
    v.push(guia("te tocaría, en torno al", pct1(r.toca_pct()), Style::new(), fin));
    let x = r.renta();
    let s = match r.veredicto() {
        Veredicto::Bien => String::new(),
        Veredicto::Poco => format!("Si sigue así, en la Renta saldrá a pagar unos {}.", eur_duro0(x)),
        Veredicto::Mucho => format!("Si sigue así, en la Renta te devolverán unos {}.", eur_duro0(-x)),
    };
    if !s.is_empty() {
        v.extend(con_importes(&s, w, Style::new()));
    }
    (v, r.supuestos(y))
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

    /// De dónde sale lo del año: «en Empresa Inventada SL, según tus nóminas apuntadas (9 meses)».
    fn fuente(&self) -> String {
        let a = &self.cache.anio;
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
            [e] => format!("en {e}, según "),
            [e1, e2] => format!("en {e1} y {e2}, según "),
            v => format!("en {} empresas, según ", v.len()),
        };
        if donde.is_empty() { format!("según {fuente}") } else { format!("{donde}{fuente}") }
    }

    /// La columna de la izquierda: lo que te llegó en dinero, en una banda (si se sabe), y debajo el bruto, el IRPF
    /// y tu Seguridad Social con puntos guía, con las cifras acabando donde la de la banda. Devuelve las filas
    /// que ocupa. `compacto`: sin la fila de aire entre la banda y las guías.
    fn pintar_anio(&self, f: &mut Frame, r: Rect, compacto: bool) -> u16 {
        let a = &self.cache.anio;
        let fin = r.width.saturating_sub(1);
        let mut y = 0;
        if let Some(c) = &self.camino
            && r.height >= ALTO_BANDA
        {
            let cifra = fmt::eur(c.dinero);
            // Con aire entre la etiqueta y la cifra; si no, la etiqueta corta.
            let etiqueta = if "te llegaron en dinero".len() + cifra.chars().count() + 8 <= fin as usize { "te llegaron en dinero" } else { "te llegaron" };
            banda(f, Rect { height: ALTO_BANDA, ..r }, tema::SUAVE_BIEN, en_banda(etiqueta, cifra, tema::BIEN, fin));
            y = ALTO_BANDA + if compacto { 0 } else { 1 };
        }
        let mut v = Vec::new();
        if let Some(b) = a.bruto {
            v.push(guia("bruto", format!("{}{}", if b.de == Fuente::Cotizado { "≈" } else { "" }, fmt::eur(b.v)), Style::new(), fin));
        }
        if let Some(i) = a.irpf {
            v.push(guia("IRPF", fmt::eur(i.v), Style::new(), fin));
        }
        if let Some(s) = a.tu_ss {
            v.push(guia("Seguridad Social", fmt::eur(s.v), Style::new(), fin));
        }
        let n = v.len() as u16;
        f.render_widget(Paragraph::new(v), debajo(r, y));
        (y + n).min(r.height)
    }

    /// ¿Te retienen bien? Con la Renta, lo que salió; sin ella, lo que te tocaría por tus nóminas (y, aparte, con
    /// qué se ha calculado). Las cifras, con puntos guía acabando en `w`. `compacto`: sin filas de aire.
    fn retenido(&self, w: usize, compacto: bool) -> (Vec<Line<'static>>, String) {
        let fin = w as u16;
        match (&self.camino, &self.retencion) {
            (Some(c), _) if c.resultado_renta.is_some() => {
                let r = c.resultado_renta.unwrap_or(0.0);
                let (t, color) = veredicto_renta(r, c.irpf());
                let mut v = vec![titulo_bloque("¿te retuvieron bien?"), sello_veredicto(t, color)];
                if !compacto {
                    v.push(Line::from(""));
                }
                v.push(guia("te retuvieron el", fmt::pct(c.retenido_pct()), Style::new(), fin));
                if let Some((e, a)) = c.tipo_medio {
                    v.push(guia("tu tipo medio en la Renta", fmt::pct(e + a), Style::new(), fin));
                    v.push(Line::styled(format!("  {} estatal y {} autonómico", fmt::pct(e), fmt::pct(a)), tema::apagado()));
                }
                let que = if r > DIF { format!("salió a pagar {}", eur_duro(r)) } else if r < -DIF { format!("salió a devolver {}", eur_duro(-r)) } else { "salió a cero".into() };
                v.extend(con_importes(&format!("Al hacer la Renta, con todo lo del año (no solo el sueldo), {que}."), w, Style::new()));
                (v, String::new())
            }
            (_, Some(r)) => lineas_estimacion(r, self.cache.anio.anio, w, fin, compacto),
            _ => {
                let mut v = vec![titulo_bloque("¿te retienen bien?")];
                v.extend(parrafo("Para saberlo, apunta tus nóminas (n): el IRPF que te retienen solo viene en ellas.", w, tema::apagado()));
                (v, String::new())
            }
        }
    }

    /// Lo del año, arriba de la tabla: de dónde sale, la banda y las guías y, al lado (o debajo, si no cabe), si te
    /// retienen bien. Devuelve las filas que ocupa. Con poco alto (`compacto`), sin algunas filas de aire.
    fn pintar_arriba(&self, f: &mut Frame, area: Rect, compacto: bool) -> u16 {
        let gris = tema::apagado();
        let w = area.width;
        let y = self.cache.anio.anio;
        // De dónde sale, con el título delante si cabe en una fila o hay sitio de sobra (si no, sin él: el año ya
        // está arriba).
        let fuente = self.fuente();
        let titulo = format!("Tu sueldo en {y}");
        let junto = format!("{titulo} · {fuente}");
        let filas = if !compacto && (junto.chars().count() <= w as usize || w >= 90) {
            let l = Line::from(vec![Span::styled(titulo, Style::new().add_modifier(Modifier::BOLD)), Span::styled(format!(" · {fuente}"), gris)]);
            let filas = filas_partidas(&junto, w as usize);
            f.render_widget(Paragraph::new(l).wrap(Wrap { trim: true }), Rect { height: filas.min(area.height), ..area });
            filas
        } else {
            let mut s = fuente;
            if let Some(c) = s.get(..1) {
                s = c.to_uppercase() + &s[1..];
            }
            let v = parrafo(&s, w as usize, gris);
            let filas = v.len() as u16;
            f.render_widget(Paragraph::new(v), area);
            filas
        };
        let mut alto = filas + if compacto { 0 } else { 1 };
        // Dos columnas si caben (36 como poco cada una); si no, una debajo de otra.
        let (izq, der) = if w >= 2 * 36 + HUECO {
            let izq = ((w - HUECO) / 2).min(IZQUIERDA);
            (izq, Some((izq + HUECO, (w - izq - HUECO).min(DERECHA))))
        } else {
            (w.min(IZQUIERDA), None)
        };
        let resto = debajo(area, alto);
        let h_izq = self.pintar_anio(f, Rect { width: izq, ..resto }, compacto);
        let (lineas, supuestos) = self.retenido(der.map_or(izq, |d| d.1) as usize, compacto);
        let h_der = lineas.len() as u16;
        match der {
            Some((x, ancho)) => {
                f.render_widget(Paragraph::new(lineas), Rect { x: resto.x + x, width: ancho, ..resto });
                alto += h_izq.max(h_der);
            }
            None => {
                let r = debajo(resto, h_izq + 1);
                f.render_widget(Paragraph::new(lineas), r);
                alto += h_izq + 1 + h_der;
            }
        }
        // Con qué se ha calculado, debajo y a lo ancho.
        if !supuestos.is_empty() {
            let v = parrafo(&supuestos, w as usize, gris);
            let n = v.len() as u16;
            f.render_widget(Paragraph::new(v), debajo(area, alto + 1));
            alto += 1 + n;
        }
        alto.min(area.height)
    }

    /// Mes a mes, sin caja: la cabecera en gris, las cifras a la derecha (sin «€»), el total del año al final y el
    /// mes elegido en el fondo suave de la sección. Sin nómina apuntada, lo cotizado (≈) y lo liquidado.
    fn dibujar_tabla(&mut self, f: &mut Frame, area: Rect) {
        let gris = tema::apagado();
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        let raya = || Line::styled("—", gris);
        let aprox = self.meses.iter().any(|m| m.nomina.is_none() && m.ss.is_some());
        // El total del año, al final.
        let bruto: f64 = self.meses.iter().filter_map(|m| m.bruto()).map(|b| b.0).sum();
        let con: Vec<_> = self.meses.iter().filter_map(|m| m.nomina.as_ref()).collect();
        let irpf: f64 = con.iter().map(|n| n.irpf).sum();
        let bruto_nominas: f64 = con.iter().map(|n| n.bruto).sum();
        let ss: f64 = self.meses.iter().filter_map(|m| m.tu_ss()).sum();
        let total_bruto = format!("{}{}", if aprox { "≈" } else { "" }, fmt::num(fmt::r2(bruto)));
        let total_pct = pct1(irpf / bruto_nominas.max(DIF) * 100.0);
        // El ancho de cada columna: lo más largo de ella o su título.
        let largo = |v: &mut dyn Iterator<Item = String>, titulo: &str| v.map(|s| s.chars().count()).chain([titulo.chars().count()]).max().unwrap_or(0) as u16;
        let bruto_mes = |m: &MesSueldo| m.bruto().map(|(b, a)| format!("{}{}", if a { "≈" } else { "" }, fmt::num(b)));
        let w_bruto = largo(&mut self.meses.iter().filter_map(bruto_mes).chain([total_bruto.clone()]), "bruto");
        let w_irpf = largo(&mut con.iter().map(|n| fmt::num(n.irpf)).chain([fmt::num(fmt::r2(irpf))]), "IRPF");
        let w_pct = largo(&mut con.iter().map(|n| pct1(n.retenido_pct())).chain([total_pct.clone()]), "%");
        let w_ss = largo(&mut [fmt::num(fmt::r2(ss))].into_iter(), "seg. social");
        // La empresa, si le quedan 14 como poco.
        let fijo = 10 + w_bruto + w_irpf + w_pct + w_ss + 5 * 2;
        let con_empresa = fijo + 14 <= area.width;
        let mut filas: Vec<FilaT> = self
            .meses
            .iter()
            .map(|m| {
                let mut c = vec![Line::from(mes(m.mes()))];
                if con_empresa {
                    c.push(if m.vacio() { Line::styled("sin nómina", gris) } else { Line::from(m.empresas().join(", ")) });
                }
                c.push(match m.bruto() {
                    Some((b, true)) => Line::styled(format!("≈{}", fmt::num(b)), gris),
                    Some((b, false)) => Line::from(fmt::num(b)),
                    None => raya(),
                });
                match &m.nomina {
                    Some(n) => c.extend([Line::from(fmt::num(n.irpf)), Line::styled(pct1(n.retenido_pct()), gris)]),
                    None => c.extend([raya(), raya()]),
                }
                c.push(m.tu_ss().map_or_else(raya, |x| Line::from(fmt::num(x))));
                FilaT::new(c)
            })
            .collect();
        let mut c = vec![Line::from("total")];
        if con_empresa {
            c.push(Line::from(""));
        }
        c.push(Line::from(total_bruto));
        if con.is_empty() {
            c.extend([raya(), raya()]);
        } else {
            c.extend([Line::from(fmt::num(fmt::r2(irpf))), Line::from(total_pct)]);
        }
        c.push(Line::from(fmt::num(fmt::r2(ss))));
        filas.push(FilaT::new(c).estilo(negrita));

        let mut cols: Vec<Col> = vec![if con_empresa { col(10) } else { RESTO }];
        let mut cab = vec!["mes"];
        if con_empresa {
            cols.push(RESTO);
            cab.push("empresa");
        }
        cols.extend([col_der(w_bruto), col_der(w_irpf), col_der(w_pct), col_der(w_ss)]);
        cab.extend(["bruto", "IRPF", "%", "seg. social"]);
        // Lo que dice el ≈, debajo de la tabla y con una fila de aire (la tabla se desplaza si no cabe entera).
        let n = filas.len() as u16 + 1;
        let (alto, nota) = if aprox && area.height >= 4 { (n.min(area.height - 2), true) } else { (n.min(area.height), false) };
        tabla(f, Rect { height: alto, ..area }, &Tabla { cols: &cols, cab: &cab, filas }, Some(self.sel), &mut self.desde);
        if nota {
            let r = Rect { y: area.y + alto + 1, height: 1, ..area };
            f.render_widget(Paragraph::new(Line::styled("≈ lo cotizado: apunta la nómina para saber el bruto y el IRPF", gris)), r);
        }
    }

    /// Todo lo de la pantalla, debajo de lo que haya abierto encima.
    fn dibujar_sueldo(&mut self, f: &mut Frame, area: Rect) {
        let a = &self.cache.anio;
        let y = a.anio;
        let gris = tema::apagado();
        if !a.hay() {
            let mut v = parrafo(&format!("No hay sueldo por cuenta ajena en {y}: ni en tu Renta, ni en tus nóminas apuntadas, ni en la Seguridad Social."), area.width as usize, Style::new());
            v.push(Line::from(""));
            v.push(Line::styled("Pulsa n para apuntar una nómina.", Style::new().fg(tema::acento())));
            f.render_widget(Paragraph::new(v).wrap(Wrap { trim: false }), area);
            return;
        }
        // Con poco alto, sin algunas filas de aire: que se vean unos cuantos meses.
        let compacto = area.height < 24;
        let alto = self.pintar_arriba(f, area, compacto);
        let resto = debajo(area, alto + 1);
        if self.hay_meses() {
            // No más ancha que lo de arriba.
            self.dibujar_tabla(f, Rect { width: resto.width.min(ANCHO_TABLA), ..resto });
        } else {
            let s = if a.ss.is_none() {
                format!("Mes a mes: la Seguridad Social no está consultada para {y} y no has apuntado nóminas.")
            } else {
                format!("Mes a mes: la Seguridad Social no tiene el detalle de tus nóminas de {y} (solo lo da de las liquidadas con el sistema actual) y no has apuntado ninguna.")
            };
            f.render_widget(Paragraph::new(parrafo(&s, area.width as usize, gris)), resto);
        }
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
        self.dibujar_sueldo(f, area);

        // Lo que haya abierto encima.
        let generacion = ctx.generacion;
        match &mut self.modo {
            Modo::Tabla => {}
            Modo::Mes(i, ven) => {
                let Some(m) = self.meses.get(*i) else { return };
                let t = nombre_mes(&m.ym);
                let t = format!("{}{}", t[..1].to_uppercase(), &t[1..]);
                // Encima de todo, con sus teclas en el borde de abajo.
                let dentro = ventana(f, area, &t);
                pie_ventana(f, area, linea_atajos(&[("← →", "otro mes"), ("n", "nómina"), ("Esc", "volver")]));
                ven.dibujar(f, dentro, generacion, |w| ventana_mes::lineas(&ctx.datos, m, w));
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
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        match &self.modo {
            Modo::Form(..) => vec![("Tab ↓", "siguiente"), ("← →", "mes"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")],
            Modo::Mes(..) => vec![("← →", "otro mes"), ("n", "nómina del mes"), ("↑↓", "desplazar"), ("Esc", "volver")],
            Modo::Tabla if !self.hay_meses() => vec![("n", "apuntar nómina")],
            Modo::Tabla => {
                let vacio = self.meses.get(self.sel).is_none_or(|m| m.vacio());
                let n = if self.hay_nomina() { "cambiar nómina" } else { "apuntar nómina" };
                vec![("↑↓", "mes"), ("Enter", if vacio { "apuntar nómina" } else { "ver el mes" }), ("n", n)]
            }
        }
    }
}
