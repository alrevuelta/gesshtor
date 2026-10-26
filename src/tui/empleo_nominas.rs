//! «Nóminas»: donde se apuntan, una por mes, en cualquier año desde 2020. Con ellas, Sueldo sabe tu bruto y tu IRPF
//! mes a mes y si te retienen bien. El formulario es el mismo que abre Sueldo con «n».
use crate::dominio::fmt::DIF;
use super::empleo::{col, col_der, parrafo, pct1, tabla, Col, FilaT, Tabla, RESTO};
use crate::dominio::{uid, Datos, Nomina};
use crate::dominio::fmt::{self, r2};
use crate::informe::empleo::nominas_anio;
use crate::tui::widgets::{bloque, tema, Confirmar, Formulario, Respuesta, Resultado};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

/// Ancho de las columnas de importes.
const COL: u16 = 12;

#[derive(Default)]
pub struct Nominas {
    /// Las nóminas del año que se ve, por meses (copia, para pintar sin buscar cada vez), y con qué se hicieron.
    lista: Vec<Nomina>,
    clave: Option<(u64, i32)>,
    sel: usize,
    /// La primera fila que se ve.
    desde: usize,
    modo: Modo,
}

#[derive(Default)]
enum Modo {
    #[default]
    Lista,
    /// El formulario, con el id de la nómina que se cambia (None: nueva).
    Form(Box<Formulario>, Option<String>),
    /// Confirmar el borrado de la nómina con ese id.
    Borrar(Confirmar, String),
}

/// «marzo» (diciembre si el mes no vale).
pub(super) fn mes(n: u32) -> &'static str {
    fmt::MESES.get((n as usize).wrapping_sub(1)).copied().unwrap_or("diciembre")
}

/// El mes de una nómina nueva: el siguiente al último apuntado (sin pasar del de hoy en el año en curso); sin
/// ninguna, el anterior al de hoy en el año en curso (la última cobrada) y enero en los demás.
pub(super) fn mes_nuevo(d: &Datos, anio: i32) -> u32 {
    let hoy = fmt::hoy();
    let tope = if anio == fmt::anio_de(&hoy) { fmt::mes_de(&hoy) } else { 12 };
    match d.nominas.iter().filter(|n| n.anio == anio).map(|n| n.mes).max() {
        Some(m) => (m + 1).min(tope).max(1),
        None if tope < 12 => tope.saturating_sub(1).max(1),
        None => 1,
    }
}

/// La empresa de la última nómina apuntada (para no tener que escribirla cada mes).
fn ultima_empresa(d: &Datos, anio: i32) -> String {
    let mut v: Vec<&Nomina> = d.nominas.iter().filter(|n| !n.empresa.trim().is_empty()).collect();
    v.sort_by_key(|n| (n.anio <= anio, n.anio, n.mes));
    v.last().map(|n| n.empresa.trim().to_string()).unwrap_or_default()
}

/// El formulario de una nómina: la que se cambia o una nueva de ese mes (con la empresa de la última apuntada).
pub(super) fn formulario(d: &Datos, anio: i32, mes_por_defecto: u32, n: Option<&Nomina>) -> Formulario {
    let m = n.map(|n| n.mes).filter(|m| (1..=12).contains(m)).unwrap_or(mes_por_defecto.clamp(1, 12));
    let claves: Vec<String> = (1..=12).map(|m: u32| m.to_string()).collect();
    let meses: Vec<(&str, &str)> = claves.iter().map(String::as_str).zip(fmt::MESES).collect();
    // Un cero se deja vacío.
    let v = |x: Option<&f64>| x.copied().filter(|x| x.abs() > DIF);
    let empresa = n.map_or_else(|| ultima_empresa(d, anio), |n| n.empresa.clone());
    let titulo = if n.is_some() { format!("Cambiar nómina · {anio}") } else { format!("Apuntar nómina · {anio}") };
    let mut fm = Formulario::new(titulo)
        .opciones("mes", "Mes", &meses, &m.to_string())
        .ayuda("← → para cambiarlo.")
        .texto("empresa", "Empresa", &empresa)
        .ayuda("Si cobras de dos empresas, apunta una nómina de cada.")
        .numero("dinerario", "Bruto", v(n.map(|n| &n.dinerario)))
        .ayuda("El total devengado, sin lo de en especie.")
        .numero("especie", "En especie", v(n.map(|n| &n.especie)))
        .ayuda("Seguro médico, acciones, coche… si tu nómina lo trae.")
        .numero("retenciones", "IRPF", v(n.map(|n| &n.retenciones)))
        .ayuda("La retención de IRPF que te descuentan.")
        .numero("ss", "Seguridad Social", v(n.map(|n| &n.ss)))
        .ayuda("Todo lo que te descuentan de Seguridad Social, sumado.");
    // Se empieza por lo primero que falta.
    fm.foco = if n.is_some() { 0 } else if empresa.is_empty() { 1 } else { 2 };
    fm
}

/// Guarda lo del formulario si está bien (si no, deja el error en el formulario). Devuelve el id guardado.
pub(super) fn guardar(fm: &mut Formulario, id: Option<&str>, anio: i32, ctx: &mut Ctx) -> Option<String> {
    for (k, nombre) in [("dinerario", "el bruto"), ("especie", "lo de en especie"), ("retenciones", "el IRPF"), ("ss", "la Seguridad Social")] {
        let t = fm.get(k);
        if !fmt::es_num(&t) {
            fm.error = Some(format!("Revisa {nombre}: «{t}» no es un importe."));
            return None;
        }
    }
    let v = |k: &str| r2(fm.num(k).unwrap_or(0.0).max(0.0));
    let mut n = match id.and_then(|id| ctx.datos.nominas.iter().find(|x| x.id == id)) {
        Some(x) => x.clone(),
        None => Nomina { id: uid(), anio, ..Default::default() },
    };
    n.mes = fm.get("mes").parse().unwrap_or(0);
    n.empresa = fm.get("empresa").trim().to_string();
    n.dinerario = v("dinerario");
    n.especie = v("especie");
    n.retenciones = v("retenciones");
    n.ss = v("ss");
    if let Err(e) = n.validar() {
        fm.error = Some(e.to_string());
        return None;
    }
    let guardado = n.id.clone();
    let texto = format!("Nómina de {} guardada", mes(n.mes));
    match ctx.datos.nominas.iter_mut().find(|x| x.id == n.id) {
        Some(x) => *x = n,
        None => ctx.datos.nominas.push(n),
    }
    if ctx.guardar() {
        ctx.ok(texto);
    }
    Some(guardado)
}

impl Nominas {
    fn al_dia(&mut self, ctx: &Ctx) {
        let clave = (ctx.generacion, ctx.anio);
        if self.clave == Some(clave) {
            return;
        }
        if self.clave.map(|c| c.1) != Some(ctx.anio) {
            self.sel = 0;
            self.desde = 0;
        }
        self.clave = Some(clave);
        self.lista = nominas_anio(&ctx.datos, ctx.anio).into_iter().cloned().collect();
        self.sel = self.sel.min(self.lista.len().saturating_sub(1));
    }

    fn elegida(&self) -> Option<&Nomina> {
        self.lista.get(self.sel)
    }

    fn nueva(&mut self, ctx: &Ctx) {
        let fm = formulario(&ctx.datos, ctx.anio, mes_nuevo(&ctx.datos, ctx.anio), None);
        self.modo = Modo::Form(Box::new(fm), None);
    }

    fn cambiar(&mut self, ctx: &Ctx) {
        let Some(n) = self.elegida().cloned() else { return };
        self.modo = Modo::Form(Box::new(formulario(&ctx.datos, n.anio, n.mes, Some(&n))), Some(n.id));
    }

    fn preguntar_borrar(&mut self) {
        let Some(n) = self.elegida().cloned() else { return };
        let empresa = if n.empresa.is_empty() { String::new() } else { format!(" · {}", n.empresa) };
        let c = Confirmar::new("Borrar la nómina", "Borrar").linea(format!("{}{empresa} · {}", mes(n.mes), fmt::eur(n.bruto()))).peligro();
        self.modo = Modo::Borrar(c, n.id);
    }

    fn borrar(&mut self, ctx: &mut Ctx, id: &str) {
        let antes = ctx.datos.nominas.len();
        ctx.datos.nominas.retain(|n| n.id != id);
        if ctx.datos.nominas.len() != antes && ctx.guardar() {
            ctx.ok("Nómina borrada");
        }
        self.al_dia(ctx);
    }

    fn dibujar_lista(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let y = ctx.anio;
        let n = self.lista.len();
        let titulo = format!("Tus nóminas de {y}");
        let b = bloque(&titulo);
        if n == 0 {
            let w = area.width.saturating_sub(2) as usize;
            let mut t = vec![Line::from(format!("Todavía no has apuntado nóminas de {y}.")), Line::from("")];
            t.extend(parrafo("Con ellas ves tu sueldo mes a mes y si te retienen bien. Copia de cada nómina el bruto, el IRPF y la Seguridad Social.", w, tema::apagado()));
            t.push(Line::from(""));
            t.push(Line::styled("Pulsa n para apuntar una.", Style::new().fg(tema::ACENTO)));
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: true }).block(b), area);
            return;
        }
        let ancho = b.inner(area).width;
        let con_especie = ancho >= 84 && self.lista.iter().any(|n| n.especie.abs() > DIF);
        let con_empresa = ancho >= 64;
        let importe = |x: f64| Line::from(fmt::eur(x));
        let celdas = |mes: Line<'static>, empresa: &str, bruto: f64, especie: f64, irpf: f64, ss: f64| {
            let mut c = vec![mes];
            if con_empresa {
                c.push(Line::from(empresa.to_string()));
            }
            c.push(importe(bruto));
            if con_especie {
                c.push(if especie.abs() > DIF { importe(especie) } else { Line::styled("—", tema::apagado()) });
            }
            let pct = if bruto.abs() > DIF { pct1(irpf / bruto * 100.0) } else { "—".into() };
            c.extend([importe(irpf), Line::styled(pct, tema::apagado()), importe(ss)]);
            FilaT::new(c)
        };
        let mut filas: Vec<FilaT> = self.lista.iter().map(|n| celdas(Line::from(mes(n.mes)), &n.empresa, r2(n.bruto()), n.especie, n.retenciones, n.ss)).collect();
        if n > 1 {
            let suma = |f: fn(&Nomina) -> f64| r2(self.lista.iter().map(f).sum());
            let total = celdas(Line::from("Total"), "", suma(|n| n.bruto()), suma(|n| n.especie), suma(|n| n.retenciones), suma(|n| n.ss));
            filas.push(total.estilo(Style::new().add_modifier(Modifier::BOLD)));
        }
        let mut cols: Vec<Col> = vec![col(11)];
        let mut cab = vec!["Mes"];
        if con_empresa {
            cols.push(RESTO);
            cab.push("Empresa");
        }
        cols.push(col_der(COL + 1));
        cab.push("Bruto");
        if con_especie {
            cols.push(col_der(COL));
            cab.push("En especie");
        }
        cols.extend([col_der(COL), col_der(7), col_der(COL)]);
        cab.extend(["IRPF", "%", "Seg. Social"]);
        // Sin la columna de empresa, la del mes se lleva lo que sobre.
        if !con_empresa {
            cols[0] = RESTO;
        }
        tabla(f, area, b, &Tabla { cols: &cols, cab: &cab, filas }, Some(self.sel), &mut self.desde);
    }
}

impl Pantalla for Nominas {
    fn titulo(&self) -> &'static str {
        "Nóminas"
    }

    fn capturando(&self) -> bool {
        matches!(self.modo, Modo::Form(..) | Modo::Borrar(..))
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.al_dia(ctx);
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.al_dia(ctx);
        self.dibujar_lista(f, area, ctx);
        match &self.modo {
            Modo::Form(fm, _) => fm.dibujar_ventana(f, area, 72),
            Modo::Borrar(c, _) => c.dibujar(f, area),
            Modo::Lista => {}
        }
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        self.al_dia(ctx);
        match &mut self.modo {
            Modo::Form(fm, id) => {
                match fm.tecla(k) {
                    Resultado::Guardar => {
                        let id = id.clone();
                        if let Some(nuevo) = guardar(fm, id.as_deref(), ctx.anio, ctx) {
                            self.modo = Modo::Lista;
                            self.al_dia(ctx);
                            if let Some(i) = self.lista.iter().position(|x| x.id == nuevo) {
                                self.sel = i;
                            }
                        }
                    }
                    Resultado::Cancelar => self.modo = Modo::Lista,
                    Resultado::Sigue | Resultado::Cambio => {}
                }
                return true;
            }
            Modo::Borrar(c, id) => {
                match c.tecla(k) {
                    Respuesta::Si => {
                        let id = id.clone();
                        self.modo = Modo::Lista;
                        self.borrar(ctx, &id);
                    }
                    Respuesta::No => self.modo = Modo::Lista,
                    Respuesta::Sigue => {}
                }
                return true;
            }
            Modo::Lista => {}
        }
        let n = self.lista.len();
        match k.code {
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = n.saturating_sub(1),
            KeyCode::Char('n') => self.nueva(ctx),
            KeyCode::Enter | KeyCode::Char('e') if n > 0 => self.cambiar(ctx),
            KeyCode::Char('d') if n > 0 => self.preguntar_borrar(),
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        match &self.modo {
            Modo::Form(..) => vec![("Tab ↓", "siguiente"), ("← →", "mes"), ("Ctrl+S", "guardar"), ("Esc", "cancelar")],
            Modo::Borrar(..) => vec![("Enter/s", "sí"), ("Esc/n", "no")],
            Modo::Lista if self.lista.is_empty() => vec![("n", "nueva")],
            Modo::Lista => vec![("n", "nueva"), ("Enter", "cambiar"), ("d", "borrar")],
        }
    }
}
