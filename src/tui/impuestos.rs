//! Impuestos: los modelos 303 (IVA) y 130 (IRPF) de un trimestre, casilla a casilla, con lo calculado y lo
//! presentado lado a lado. Solo se mira: no cambia nada.
use crate::dominio::fmt::DIF;

use crate::dominio::{Casillas, Datos};
use crate::hacienda::modelo::Memo;
use crate::hacienda::modelo_130::{self, Exento, OrigenRn, PorQue, RnAnterior};
use crate::hacienda::Trimestral;
use crate::dominio::fmt;
use crate::informe::autonomo;
use crate::tui::widgets::{bloque, tema};
use crate::tui::texto::lineas;
use crate::tui::{Ctx, Pantalla};
use casillas::{Clase, Grupo};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::PathBuf;
use trimestre::Estado;
use super::{impuestos_casillas as casillas, impuestos_trimestre as trimestre};
use crate::dominio::Valor;

/// Ancho de cada columna de importes.
const VW: usize = 13;
/// A partir de este ancho, el 303 y el 130 van uno al lado del otro.
const ANCHO_DOBLE: u16 = 150;

#[derive(Default)]
pub struct Impuestos {
    /// Trimestre que se ve (1–4; 0 = aún sin elegir).
    q: u32,
    /// Año para el que se eligió el trimestre.
    anio: i32,
    /// También las casillas a cero.
    todas: bool,
    /// Los cuatro trimestres del año en vez de uno.
    anual: bool,
    scroll: u16,
    /// Los PDF del trimestre que se ve (para la barra de atajos).
    pdf303: bool,
    pdf130: bool,
    /// Lo calculado (las cadenas de 303 y 130, los estados, los PDF): no se rehace en cada dibujo.
    cab: Option<Cabecera>,
    trim: Option<DelTrimestre>,
    del_anio: Option<Anual>,
    /// Cuántas veces se ha calculado algo (para las pruebas).
    #[cfg(test)]
    pub(crate) calculos: usize,
}

/// La clave de lo calculado: (generación de los datos, año, trimestre, hoy).
type Clave = (u64, i32, u32, String);

fn clave(ctx: &Ctx, q: u32) -> Clave {
    (ctx.generacion, ctx.anio, q, fmt::hoy())
}

/// Lo de la cabecera: el estado de cada modelo y cuántas facturas y gastos hay en cada trimestre.
struct Cabecera {
    clave: Clave,
    /// [trimestre][303, 130]
    estados: [[Estado; 2]; 4],
    cuantos: [(usize, usize); 4],
}

impl Cabecera {
    fn nueva(ctx: &Ctx, hoy: &str, clave: Clave) -> Self {
        let d = &ctx.datos;
        let y = ctx.anio;
        let estados = std::array::from_fn(|i| Trimestral::TODOS.map(|m| trimestre::estado_modelo(d, hoy, y, i as u32 + 1, m)));
        let cuantos = std::array::from_fn(|i| {
            let (ini, fin) = fmt::periodo(y, i as u32 + 1);
            let nf = crate::dominio::factura::emitidas(d).filter(|x| fmt::en_rango(x.devengo(), &ini, &fin)).count();
            let ng = d.gastos.iter().filter(|g| fmt::en_rango(&g.fecha, &ini, &fin)).count();
            (nf, ng)
        });
        Cabecera { clave, estados, cuantos }
    }
}

/// El 303 y el 130 del trimestre que se ve.
struct DelTrimestre {
    clave: Clave,
    m303: Modelo,
    m130: Modelo,
}

/// De dónde sale lo que se ve de un modelo en la vista del año.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Origen {
    /// Las casillas con las que se presentó.
    Presentado,
    /// Lo calculado con tus facturas y gastos.
    Calculado,
    /// Presentado, pero sin sus casillas (no está su PDF).
    SinPdf,
    /// Nada: un año cerrado sin datos en la app ni nada presentado.
    Nada,
}

impl Origen {
    fn texto(self, corto: bool) -> &'static str {
        match self {
            Origen::Presentado if corto => "present.",
            Origen::Presentado => "presentado",
            Origen::Calculado => "calculado",
            Origen::SinPdf => "sin PDF",
            Origen::Nada => "—",
        }
    }
    fn cuenta(self) -> bool {
        matches!(self, Origen::Presentado | Origen::Calculado)
    }
}

/// Los cuatro trimestres del año.
struct Anual {
    clave: Clave,
    c303: Vec<Casillas>,
    c130: Vec<Casillas>,
    e303: Vec<Estado>,
    e130: Vec<Estado>,
    o303: Vec<Origen>,
    o130: Vec<Origen>,
    cerrado: bool,
}

impl Anual {
    fn nuevo(ctx: &Ctx, hoy: &str, clave: Clave) -> Self {
        let d = &ctx.datos;
        let y = ctx.anio;
        let cerrado = autonomo::cerrado(d, y);
        let mut memo = Memo::default();
        let origen = |q: u32, m: Trimestral| {
            let fin = fmt::periodo(y, q).1;
            if trimestre::presentadas(d, y, q, m.modelo()).is_some() {
                Origen::Presentado
            } else if !cerrado || trimestre::hay_actividad(d, &m.desde(y, q), &fin) {
                Origen::Calculado
            } else if trimestre::presentado(d, y, q, m.modelo()).is_some() {
                Origen::SinPdf
            } else {
                Origen::Nada
            }
        };
        Anual {
            c303: (1..=4).map(|q| Trimestral::M303.cuenta(d, y, q, &mut memo)).collect(),
            c130: (1..=4).map(|q| Trimestral::M130.cuenta(d, y, q, &mut memo)).collect(),
            e303: (1..=4).map(|q| trimestre::estado_modelo(d, hoy, y, q, Trimestral::M303)).collect(),
            e130: (1..=4).map(|q| trimestre::estado_modelo(d, hoy, y, q, Trimestral::M130)).collect(),
            o303: (1..=4).map(|q| origen(q, Trimestral::M303)).collect(),
            o130: (1..=4).map(|q| origen(q, Trimestral::M130)).collect(),
            cerrado,
            clave,
        }
    }
}

/// El trimestre que se ve al entrar: el que toca presentar ahora o, si no, el último que ha empezado
/// (el 4T en los años pasados, el 1T en los que vienen).
pub fn trimestre_por_defecto(anio: i32, hoy: &str) -> u32 {
    let ahora = fmt::anio_de(hoy);
    if anio < ahora {
        return 4;
    }
    if anio > ahora {
        return 1;
    }
    match crate::hacienda::plazos_trimestres(anio).iter().find(|p| fmt::en_rango(hoy, &p.ini, &p.fin)) {
        Some(p) => p.q,
        None => fmt::trimestre_de(hoy).clamp(1, 4),
    }
}

/// Un modelo del trimestre que se ve, ya calculado.
struct Modelo {
    modelo: Trimestral,
    anio: i32,
    q: u32,
    calc: Casillas,
    pres: Option<(Casillas, &'static str)>,
    estado: Estado,
    /// Hay facturas o gastos con los que comparar (en el 130, desde el 1 de enero).
    actividad: bool,
    pdf: Option<PathBuf>,
    /// Fecha y hora de la presentación según Hacienda.
    hacienda: Option<String>,
    /// El año está cerrado: sin datos en la app, solo se enseña lo presentado.
    cerrado: bool,
    /// Si el 303 negativo se pidió devolver.
    devolver: bool,
    /// Del 130: si hay que presentarlo y el rendimiento del año anterior.
    exento: Exento,
    no130: bool,
    rn: RnAnterior,
    /// La Renta de ese año no tiene pagos fraccionados (no se presentó el 130).
    renta_sin130: bool,
}

impl Modelo {
    fn nuevo(ctx: &Ctx, hoy: &str, anio: i32, q: u32, modelo: Trimestral, memo: &mut Memo) -> Self {
        let d = &ctx.datos;
        let fin = fmt::periodo(anio, q).1;
        let es130 = modelo == Trimestral::M130;
        Modelo {
            modelo,
            anio,
            q,
            calc: modelo.calcular(d, anio, q, memo),
            pres: trimestre::presentadas(d, anio, q, modelo.modelo()),
            estado: trimestre::estado_modelo(d, hoy, anio, q, modelo),
            actividad: trimestre::hay_actividad(d, &modelo.desde(anio, q), &fin),
            pdf: trimestre::pdf(&ctx.dir, d, anio, q, modelo.modelo()),
            hacienda: trimestre::fecha_hacienda(d, anio, q, modelo.modelo()),
            cerrado: autonomo::cerrado(d, anio),
            devolver: trimestre::devolver303(d, anio, q),
            exento: if es130 { modelo_130::exento(d, anio, q) } else { Exento::default() },
            no130: es130 && modelo_130::se_libra(d, anio, q),
            rn: if es130 { modelo_130::rn_anterior(d, anio) } else { RnAnterior::default() },
            renta_sin130: crate::informe::renta_de(d, anio).and_then(|r| r.pagos.as_ref()).is_some_and(|p| p.fraccionados.abs() <= DIF),
        }
    }
    /// Solo se puede enseñar lo presentado (un año cerrado sin facturas ni gastos de este periodo).
    fn solo_presentado(&self) -> bool {
        self.cerrado && !self.actividad
    }
    /// Lo que cuenta: lo presentado si se conoce; si no, lo calculado.
    fn cuenta(&self) -> &Casillas {
        self.pres.as_ref().map(|p| &p.0).unwrap_or(&self.calc)
    }
    /// La casilla del resultado.
    fn k(&self) -> u32 {
        self.modelo.resultado()
    }
    fn texto(&self, v: f64) -> String {
        match self.modelo {
            Trimestral::M303 => trimestre::texto303(v, self.devolver, false),
            Trimestral::M130 => trimestre::texto130(v, false),
        }
    }
}

/* ================= piezas de texto ================= */

/// Rellena con espacios o corta con «…» hasta `n` caracteres.
fn ajustar(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n {
        format!("{s}{}", " ".repeat(n - c))
    } else if n == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

fn nota(v: &mut Vec<Line<'static>>, texto: &str, ancho: usize, estilo: Style) {
    // El «€» va con su número.
    v.extend(lineas(&texto.replace(" €", "\u{a0}€"), ancho).into_iter().map(|l| Line::styled(l, estilo)));
}

fn importe_casilla(v: f64, clase: Clase) -> String {
    if clase == Clase::Pct { format!("{} %", fmt::num(v)) } else { fmt::num(v) }
}

/// Las casillas de un modelo en columnas («calculado», «presentado»), marcando con ≠ las que no coinciden.
fn tabla(grupos: &[Grupo], cols: &[(&str, &Casillas)], todas: bool, ancho: usize, apagar: bool) -> Vec<Line<'static>> {
    let valores: Vec<&Casillas> = cols.iter().map(|c| c.1).collect();
    let compara = cols.len() > 1;
    // Sin pasarse de ancho: con mucho sitio, los importes quedan lejos de su nombre.
    let lw = ancho.saturating_sub(4 + VW * cols.len() + if compara { 2 } else { 0 }).clamp(8, 72);
    let mut v = Vec::new();
    let mut cab = " ".repeat(4 + lw);
    for (nombre, _) in cols {
        cab.push_str(&format!("{nombre:>VW$}"));
    }
    v.push(Line::styled(cab, tema::apagado()));
    for (titulo, filas) in grupos {
        let vis: Vec<_> = filas.iter().filter(|x| casillas::visible(x, todas, &valores)).collect();
        if vis.is_empty() {
            continue;
        }
        v.push(Line::styled(titulo.to_string(), if apagar { tema::apagado() } else { tema::cabecera_tabla() }));
        for x in vis {
            let total = x.clase == Clase::Total;
            let distinta = compara && valores.windows(2).any(|w| (w[0].v(x.n) - w[1].v(x.n)).abs() > DIF);
            let base = if apagar {
                tema::apagado()
            } else if total {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            let mut spans = vec![
                Span::styled(format!("{:>3} ", casillas::numero(x.n)), tema::apagado()),
                Span::styled(ajustar(&casillas::nombre(x, &valores), lw), base),
            ];
            for c in &valores {
                let n = c.v(x.n);
                let st = if distinta && !apagar {
                    Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)
                } else if n.abs() <= DIF && x.clase != Clase::Pct {
                    tema::apagado()
                } else {
                    base
                };
                spans.push(Span::styled(format!("{:>VW$}", importe_casilla(n, x.clase)), st));
            }
            if compara {
                spans.push(Span::styled(if distinta { " ≠" } else { "  " }, Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)));
            }
            v.push(Line::from(spans));
        }
    }
    v
}

/// El estado de un modelo en una frase: «Presentado el 15 oct 2026 a las 09:05», «En plazo hasta el 20/10 · quedan 3 días».
fn frase_estado(m: &Modelo) -> String {
    let p = trimestre::plazo(m.anio, m.q);
    match &m.estado {
        Estado::Presentado(f) => {
            let hora = m.hacienda.as_deref().and_then(|h| h.get(11..16)).map(|h| format!(" a las {h}")).unwrap_or_default();
            if f.is_empty() { "Presentado".into() } else { format!("Presentado el {}{hora}", fmt::fecha_corta(f)) }
        }
        Estado::NoToca("exento") => "No tienes que presentarlo".into(),
        Estado::NoToca(motivo) => format!("No toca: {motivo}"),
        Estado::Futuro => format!("Aún no: se presenta {}", trimestre::rango(&p.ini, &p.fin)),
        Estado::EnCurso => format!("En curso: se presenta {}", trimestre::rango(&p.ini, &p.fin)),
        Estado::Proximo => format!("Se presenta {}", trimestre::rango(&p.ini, &p.fin)),
        Estado::EnPlazo(n) => format!("En plazo hasta el {} · {}", fmt::fecha_corta(&p.fin), trimestre::dias_txt(*n)),
        Estado::Vencido => format!("Plazo terminado el {}", fmt::fecha_corta(&p.fin)),
        Estado::SinDatos => format!("Plazo terminado el {} · sin datos en la app (¿lo presentaste fuera?)", fmt::fecha_corta(&p.fin)),
    }
}

/// Todo lo de un modelo: estado, de dónde salen las casillas, la tabla y el resultado.
fn doc(m: &Modelo, d: &Datos, ancho: usize, todas: bool) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    let (titulo, tecla) = match m.modelo {
        Trimestral::M303 => ("Modelo 303 · IVA", "o"),
        Trimestral::M130 => ("Modelo 130 · IRPF (pago fraccionado)", "O"),
    };
    v.push(Line::from(vec![Span::styled(titulo, tema::titulo()), Span::raw("   "), Span::styled(frase_estado(m), m.estado.estilo())]));
    match &m.pdf {
        Some(p) => {
            let nombre = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            v.push(Line::from(vec![
                Span::styled("PDF: ", tema::apagado()),
                Span::raw(nombre),
                Span::styled(format!(" · {tecla} para abrirlo"), tema::apagado()),
            ]));
        }
        None if matches!(m.estado, Estado::Presentado(_)) => v.push(Line::styled("Sin PDF en justificantes/", tema::apagado())),
        None => {}
    }
    if matches!(m.estado, Estado::Futuro) {
        nota(&mut v, "Este trimestre todavía no ha empezado.", ancho, tema::apagado());
    }

    // Un año cerrado sin facturas ni gastos de este periodo: solo lo presentado (si se tienen sus casillas).
    if m.solo_presentado() && m.pres.is_none() {
        let t = if m.modelo == Trimestral::M130 && m.renta_sin130 && !matches!(m.estado, Estado::Presentado(_)) {
            format!("Tu Renta de {} no tiene pagos fraccionados: no presentaste el 130.", m.anio)
        } else if !matches!(m.estado, Estado::Presentado(_)) {
            format!("{} está cerrado y en la app no hay facturas ni gastos de este periodo: no hay nada que calcular.", m.anio)
        } else if m.pdf.is_some() {
            format!("Presentado. Sus casillas aún no se han leído del PDF: ábrelo con {tecla}.")
        } else {
            "Presentado, pero sin su PDF en justificantes/ no sé con qué casillas.".to_string()
        };
        nota(&mut v, &t, ancho, tema::apagado());
        return v;
    }

    // El 130: ¿hay que presentarlo?
    let mut apagar = false;
    if m.modelo == Trimestral::M130 && !matches!(m.estado, Estado::Presentado(_)) {
        let ex = &m.exento;
        let modo = d.fiscal.exento130.as_str();
        if m.no130 {
            apagar = true;
            let porque = if modo == "si" {
                "Lo has indicado tú: al menos el 70 % de tus ingresos del año pasado llevaron retención.".to_string()
            } else {
                let de = if ex.por_que == PorQue::AnioAnterior { format!("de {}", m.anio - 1) } else { "del trimestre (primer año de actividad)".into() };
                format!("El {} de tus ingresos {de} lleva retención de IRPF: tus clientes ya adelantan el impuesto por ti.", fmt::pct(ex.pct.round()))
            };
            nota(&mut v, &format!("No estás obligado a presentar el 130 este trimestre. {porque}"), ancho, Style::new().fg(tema::BIEN));
        } else if ex.sugerido == Some(true) && (modo.is_empty() || modo == "auto") {
            let t = format!(
                "Este trimestre el {} de tus ingresos lleva retención. Si en {} también fue así (70 % o más), no tienes que presentar el 130. No tengo tus datos de {} para comprobarlo.",
                fmt::pct(ex.pct.round()),
                m.anio - 1,
                m.anio - 1
            );
            nota(&mut v, &t, ancho, Style::new().fg(tema::ACENTO));
        }
    }

    // De dónde salen las casillas y si coinciden con las de la app.
    let k = m.k();
    let cols: Vec<(&str, &Casillas)> = match &m.pres {
        Some((c, origen)) => {
            nota(&mut v, origen, ancho, Style::new().fg(tema::BIEN));
            if m.actividad {
                let (vc, vp) = (m.calc.v(k), c.v(k));
                let n = casillas::distintas(m.modelo, &m.calc, c);
                if (vc - vp).abs() > DIF {
                    let t = format!("Con tus datos de hoy saldría {}, no {}. Mira las casillas marcadas con ≠.", m.texto(vc), m.texto(vp));
                    nota(&mut v, &t, ancho, Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD));
                } else if n > 0 {
                    let t = format!("El resultado coincide con tus datos, pero {} (≠).", if n == 1 { "hay una casilla distinta".into() } else { format!("hay {n} casillas distintas") });
                    nota(&mut v, &t, ancho, Style::new().fg(tema::ACENTO));
                } else {
                    nota(&mut v, "Coincide con lo que calcula la app con tus datos de hoy.", ancho, Style::new().fg(tema::BIEN));
                }
                vec![("calculado", &m.calc), ("presentado", c)]
            } else {
                nota(&mut v, "En la app no hay facturas ni gastos de este periodo: solo se ve lo presentado.", ancho, tema::apagado());
                vec![("presentado", c)]
            }
        }
        None => {
            if matches!(m.estado, Estado::Presentado(_)) {
                nota(&mut v, "No tengo las casillas con las que se presentó: se ve lo calculado con tus datos.", ancho, tema::apagado());
            }
            vec![("calculado", &m.calc)]
        }
    };
    v.push(Line::from(""));
    v.extend(tabla(casillas::grupos(m.modelo), &cols, todas, ancho, apagar));
    v.push(Line::from(""));

    // El resultado y lo que sale de él.
    let c = m.cuenta();
    let r = c.v(k);
    let res_estilo = if apagar { tema::apagado() } else { trimestre::estilo_resultado(r).add_modifier(Modifier::BOLD) };
    let de = if m.pres.is_some() { " (presentado)" } else { " (calculado)" };
    v.push(Line::from(vec![Span::styled(format!("Resultado ({k}): "), tema::apagado()), Span::styled(m.texto(r), res_estilo), Span::styled(de, tema::apagado())]));
    if m.modelo == Trimestral::M303 {
        let (c110, c78, c87) = (c.v(110), c.v(78), c.v(87));
        let compensa = r < -DIF && !m.devolver;
        if c110.abs() > DIF || c78.abs() > DIF || c87.abs() > DIF || compensa {
            let mut t = format!(
                "Compensación: pendiente de antes {} (110) → aplicado ahora {} (78) → queda {} (87).",
                fmt::eur(c110),
                fmt::eur(c78),
                fmt::eur(c87)
            );
            if compensa {
                t.push_str(&format!(" El trimestre siguiente podrás compensar {}.", fmt::eur(fmt::r2(c87 - r))));
            }
            nota(&mut v, &t, ancho, tema::apagado());
        }
    } else {
        let t = format!(
            "Rendimiento neto acumulado {} (03) · 20 %: {} (04) · retenciones {} (06) · pagos anteriores {} (05).",
            fmt::eur(c.v(3)),
            fmt::eur(c.v(4)),
            fmt::eur(c.v(6)),
            fmt::eur(c.v(5))
        );
        nota(&mut v, &t, ancho, tema::apagado());
        let rn = &m.rn;
        let ant = m.anio - 1;
        let origen = match rn.origen {
            OrigenRn::Escrito => format!("has indicado un rendimiento de {} en {ant}.", fmt::eur(rn.valor.unwrap_or(0.0))),
            OrigenRn::Alta => format!("empezaste la actividad en {}: el rendimiento del año anterior cuenta como 0.", m.anio),
            OrigenRn::Calculado => format!("calculado con tus datos de {ant}: {}.", fmt::eur(rn.valor.unwrap_or(0.0))),
            OrigenRn::NoSe => format!("no sé cuánto ganaste en {ant} (casilla 0224 de tu Renta)."),
        };
        let t = format!("Casilla 13 (minoración): hasta 100 € por trimestre si en {ant} ganaste 12.000 € o menos. Para la app, {origen}");
        nota(&mut v, &t, ancho, tema::apagado());
    }
    v
}

/// Los cuatro trimestres del año de un vistazo (lo que cuenta: lo presentado si se conoce; si no, lo calculado).
fn doc_anual(an: &Anual, y: i32, ancho: usize) -> Vec<Line<'static>> {
    let (c303, c130, e303, e130) = (&an.c303, &an.c130, &an.e303, &an.e130);
    // Los trimestres que no han empezado o no tocan (antes del alta, exento…) no llevan importes ni suman; en un año
    // cerrado, tampoco los que no tienen ni casillas presentadas ni datos en la app.
    let cuenta = |e: &Estado, o: Origen| !matches!(e, Estado::Futuro | Estado::NoToca(_)) && o.cuenta();

    let cw = if ancho >= 5 * 12 + 30 { 12 } else { 10 };
    let corto = cw < 12;
    let lw = ancho.saturating_sub(5 * cw).clamp(10, 36);
    let n = |x: f64| if corto { fmt::num(x.round()).trim_end_matches(",00").to_string() } else { fmt::num(x) };
    let mut v = Vec::new();
    let intro = if an.cerrado {
        format!("{y} está cerrado: lo presentado en cada trimestre (y lo de la app, si tienes facturas o gastos de ese año).")
    } else {
        format!("Año {y}: lo que cuenta de cada trimestre (lo presentado si se conoce; si no, lo calculado).")
    };
    nota(&mut v, &intro, ancho, tema::apagado());
    v.push(Line::from(""));
    let mut cab = ajustar("", lw);
    for t in ["1T", "2T", "3T", "4T", "Año"] {
        cab.push_str(&format!("{t:>cw$}"));
    }
    v.push(Line::styled(cab, tema::cabecera_tabla()));

    let fila = |nombre: &str, celdas: [(String, Style); 4], total: String| -> Line<'static> {
        let mut s = vec![Span::raw(ajustar(nombre, lw))];
        for (x, st) in celdas {
            s.push(Span::styled(format!("{x:>cw$}"), st));
        }
        s.push(Span::styled(format!("{total:>cw$}"), Style::new().add_modifier(Modifier::BOLD)));
        Line::from(s)
    };
    // Una casilla de cada trimestre (en color si es un resultado); «—» si ese trimestre no cuenta.
    let por = |c: &[Casillas], e: &[Estado], o: &[Origen], k: u32, color: bool| -> [(String, Style); 4] {
        std::array::from_fn(|i| match &e[i] {
            Estado::NoToca("exento") => ("exento".into(), tema::apagado()),
            x if !cuenta(x, o[i]) && o[i] == Origen::SinPdf => ("sin PDF".into(), tema::apagado()),
            x if !cuenta(x, o[i]) => ("—".into(), tema::apagado()),
            _ => (n(c[i].v(k)), if color { trimestre::estilo_resultado(c[i].v(k)) } else { Style::new() }),
        })
    };
    // La suma del año («—» si no cuenta ningún trimestre); de los resultados, solo lo que salió a ingresar.
    let sumar = |c: &[Casillas], e: &[Estado], o: &[Origen], k: u32, solo_pagar: bool| -> String {
        let v: Vec<f64> = (0..4).filter(|&i| cuenta(&e[i], o[i])).map(|i| if solo_pagar { c[i].v(k).max(0.0) } else { c[i].v(k) }).collect();
        if v.is_empty() { "—".into() } else { n(fmt::r2(v.iter().sum())) }
    };
    let suma = |c: &[Casillas], e: &[Estado], o: &[Origen], k: u32| sumar(c, e, o, k, false);
    let pagar = |c: &[Casillas], e: &[Estado], o: &[Origen], k: u32| sumar(c, e, o, k, true);
    let estados = |e: &[Estado]| -> [(String, Style); 4] { std::array::from_fn(|i| (e[i].corto(), e[i].estilo())) };
    let origenes = |e: &[Estado], o: &[Origen]| -> [(String, Style); 4] {
        std::array::from_fn(|i| {
            let t = if matches!(e[i], Estado::Futuro | Estado::NoToca(_)) { "—" } else { o[i].texto(corto) };
            let st = if o[i] == Origen::Presentado { Style::new().fg(tema::BIEN) } else { tema::apagado() };
            (t.to_string(), st)
        })
    };
    // Lo del 130 es acumulado: el del año es el del último trimestre que cuenta.
    let ultimo = (0..4).rev().find(|&i| cuenta(&e130[i], an.o130[i]));
    let acumulado = |k: u32| ultimo.map(|i| n(c130[i].v(k))).unwrap_or_else(|| "—".into());
    let (o3, o1) = (&an.o303, &an.o130);

    v.push(Line::styled("303 · IVA", tema::titulo()));
    v.push(fila("IVA devengado (27)", por(c303, e303, o3, 27, false), suma(c303, e303, o3, 27)));
    v.push(fila("IVA deducible (45)", por(c303, e303, o3, 45, false), suma(c303, e303, o3, 45)));
    v.push(fila("Resultado (71)", por(c303, e303, o3, 71, true), pagar(c303, e303, o3, 71)));
    v.push(fila("Estado", estados(e303), String::new()));
    v.push(fila("De dónde", origenes(e303, o3), String::new()));
    v.push(Line::from(""));
    v.push(Line::styled("130 · IRPF", tema::titulo()));
    v.push(fila("Rendimiento neto (03)", por(c130, e130, o1, 3, false), acumulado(3)));
    v.push(fila("Retenciones (06)", por(c130, e130, o1, 6, false), acumulado(6)));
    v.push(fila("Resultado (19)", por(c130, e130, o1, 19, true), pagar(c130, e130, o1, 19)));
    v.push(fila("Estado", estados(e130), String::new()));
    v.push(fila("De dónde", origenes(e130, o1), String::new()));
    v.push(Line::from(""));
    nota(
        &mut v,
        "En «Año», los resultados son lo que salió a ingresar; el rendimiento y las retenciones del 130 son acumulados desde el 1 de enero (los del último trimestre).",
        ancho,
        tema::apagado(),
    );
    if o3.iter().chain(o1.iter()).any(|o| *o == Origen::SinPdf) {
        nota(&mut v, "«sin PDF»: presentado, pero sin su PDF en justificantes/ no sé sus casillas.", ancho, tema::apagado());
    }
    v
}

impl Impuestos {
    fn elegir_trimestre(&mut self, ctx: &Ctx) {
        if self.q == 0 || self.anio != ctx.anio {
            self.q = trimestre_por_defecto(ctx.anio, &fmt::hoy());
            self.anio = ctx.anio;
            self.scroll = 0;
        }
    }

    /// Rehace lo calculado que haya cambiado (datos, año, trimestre o día).
    fn preparar(&mut self, ctx: &Ctx) {
        self.elegir_trimestre(ctx);
        let hoy = fmt::hoy();
        let k = clave(ctx, 0);
        if self.cab.as_ref().is_none_or(|c| c.clave != k) {
            self.cab = Some(Cabecera::nueva(ctx, &hoy, k.clone()));
            self.contar();
        }
        if self.anual {
            if self.del_anio.as_ref().is_none_or(|c| c.clave != k) {
                self.del_anio = Some(Anual::nuevo(ctx, &hoy, k));
                self.contar();
            }
        } else {
            let k = clave(ctx, self.q);
            if self.trim.as_ref().is_none_or(|c| c.clave != k) {
                let mut memo = Memo::default();
                let m303 = Modelo::nuevo(ctx, &hoy, ctx.anio, self.q, Trimestral::M303, &mut memo);
                let m130 = Modelo::nuevo(ctx, &hoy, ctx.anio, self.q, Trimestral::M130, &mut memo);
                self.trim = Some(DelTrimestre { clave: k, m303, m130 });
                self.contar();
            }
        }
    }

    /// Lleva la cuenta de lo calculado (para las pruebas).
    fn contar(&mut self) {
        #[cfg(test)]
        {
            self.calculos += 1;
        }
    }

    /// Las dos líneas de arriba: los trimestres, el periodo, el plazo y cómo va cada modelo.
    fn cabecera(&self, f: &mut Frame, area: Rect, cab: &Cabecera, y: i32) {
        let mut l1 = vec![Span::raw(" ")];
        for q in 1..=4 {
            let hecho = &cab.estados[q as usize - 1];
            let marca = if hecho.iter().all(Estado::hecho) && hecho.iter().any(|e| matches!(e, Estado::Presentado(_))) { "✓" } else { " " };
            let t = format!(" {q}T{marca}");
            let elegido = q == self.q && !self.anual;
            l1.push(Span::styled(t, if elegido { tema::pestana_activa() } else { tema::apagado() }));
            l1.push(Span::raw(" "));
        }
        l1.push(Span::styled(" Año ", if self.anual { tema::pestana_activa() } else { tema::apagado() }));
        if !self.anual {
            let (ini, fin) = fmt::periodo(y, self.q);
            let (nf, ng) = cab.cuantos[self.q as usize - 1];
            l1.push(Span::styled(
                format!("   {} · {nf} factura{} · {ng} gasto{}", trimestre::rango(&ini, &fin), if nf == 1 { "" } else { "s" }, if ng == 1 { "" } else { "s" }),
                tema::apagado(),
            ));
        }
        let mut l2 = vec![Span::raw(" ")];
        if self.anual {
            l2.push(Span::styled(format!("Los cuatro trimestres de {y} · a vuelve al trimestre"), tema::apagado()));
        } else {
            let p = trimestre::plazo(y, self.q);
            l2.push(Span::styled("Plazo ", tema::apagado()));
            l2.push(Span::raw(trimestre::rango(&p.ini, &p.fin)));
            if area.width >= 110 {
                l2.push(Span::styled(format!(" (domiciliar hasta el {})", trimestre::dia_mes(&p.domiciliar)), tema::apagado()));
            }
            for (i, m) in Trimestral::TODOS.into_iter().enumerate() {
                let e = &cab.estados[self.q as usize - 1][i];
                l2.push(Span::styled(format!(" · {} ", m.modelo()), tema::apagado()));
                let mut t = e.texto(&p.ini, &p.fin);
                if let Estado::EnPlazo(n) = e {
                    t.push_str(&format!(" ({})", trimestre::dias_txt(*n)));
                }
                l2.push(Span::styled(t, e.estilo()));
            }
        }
        f.render_widget(Paragraph::new(vec![Line::from(l1), Line::from(l2)]), area);
    }

    /// Pinta unas líneas en un recuadro con desplazamiento; devuelve cuánto se puede bajar como mucho.
    fn pintar_doc(&self, f: &mut Frame, area: Rect, titulo: &str, lineas: Vec<Line<'static>>) -> u16 {
        let b = bloque(titulo);
        let dentro = b.inner(area);
        let total = lineas.len() as u16;
        let max = total.saturating_sub(dentro.height);
        let scroll = self.scroll.min(max);
        let b = if scroll < max {
            b.title_bottom(Line::styled(" ↓ más ", tema::apagado()).right_aligned())
        } else if scroll > 0 {
            b.title_bottom(Line::styled(" ↑ ", tema::apagado()).right_aligned())
        } else {
            b
        };
        f.render_widget(Paragraph::new(lineas).scroll((scroll, 0)).block(b), area);
        max
    }
}

impl Pantalla for Impuestos {
    fn titulo(&self) -> &'static str {
        "Impuestos"
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.elegir_trimestre(ctx);
        // Al entrar se recalcula siempre (por si los datos cambiaron sin pasar por `guardar`).
        self.cab = None;
        self.trim = None;
        self.del_anio = None;
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.preparar(ctx);
        let [a_cab, cuerpo] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
        if let Some(cab) = self.cab.take() {
            self.cabecera(f, a_cab, &cab, ctx.anio);
            self.cab = Some(cab);
        }
        let y = ctx.anio;
        if self.anual {
            self.pdf303 = false;
            self.pdf130 = false;
            let ancho = cuerpo.width.saturating_sub(2) as usize;
            let lineas = self.del_anio.as_ref().map(|an| doc_anual(an, y, ancho)).unwrap_or_default();
            let max = self.pintar_doc(f, cuerpo, &format!("{y} · 303 y 130 por trimestre"), lineas);
            self.scroll = self.scroll.min(max);
            return;
        }
        let Some(t) = self.trim.take() else { return };
        let (m303, m130) = (&t.m303, &t.m130);
        self.pdf303 = m303.pdf.is_some();
        self.pdf130 = m130.pdf.is_some();
        let d = &ctx.datos;
        let max = if cuerpo.width >= ANCHO_DOBLE {
            let [a, b] = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(cuerpo);
            let t303 = format!("303 · {}T {y}", self.q);
            let t130 = format!("130 · {}T {y}", self.q);
            let m1 = self.pintar_doc(f, a, &t303, doc(m303, d, a.width.saturating_sub(2) as usize, self.todas));
            let m2 = self.pintar_doc(f, b, &t130, doc(m130, d, b.width.saturating_sub(2) as usize, self.todas));
            m1.max(m2)
        } else {
            let ancho = cuerpo.width.saturating_sub(2) as usize;
            let mut l = doc(m303, d, ancho, self.todas);
            l.push(Line::styled("─".repeat(ancho), tema::apagado()));
            l.extend(doc(m130, d, ancho, self.todas));
            self.pintar_doc(f, cuerpo, &format!("{}T {y} · 303 y 130", self.q), l)
        };
        self.trim = Some(t);
        self.scroll = self.scroll.min(max);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        self.elegir_trimestre(ctx);
        match k.code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.anual = false;
                self.q = self.q.saturating_sub(1).max(1);
                self.scroll = 0;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.anual = false;
                self.q = (self.q + 1).min(4);
                self.scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Char('t') => {
                self.todas = !self.todas;
                ctx.info(if self.todas { "Se ven también las casillas a cero." } else { "Solo las casillas con algo." });
            }
            KeyCode::Char('a') => {
                self.anual = !self.anual;
                self.scroll = 0;
            }
            KeyCode::Char(c @ ('o' | 'O')) => {
                let modelo = if c == 'o' { Trimestral::M303 } else { Trimestral::M130 };
                if self.anual {
                    ctx.elegir_pdf(ctx.anio);
                    return true;
                }
                match trimestre::pdf(&ctx.dir, &ctx.datos, ctx.anio, self.q, modelo.modelo()) {
                    Some(p) => ctx.abrir_ruta(&p),
                    // Sin PDF de ese trimestre (aún no presentado): los del año, para elegir.
                    None => ctx.elegir_pdf(ctx.anio),
                }
            }
            _ => return false,
        }
        true
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = vec![("← →", "trimestre"), ("↑↓", "bajar"), ("t", if self.todas { "solo con valor" } else { "todas" }), ("a", if self.anual { "trimestre" } else { "año" })];
        if self.pdf303 && !self.anual {
            v.push(("o", "PDF 303"));
        }
        if self.pdf130 && !self.anual {
            v.push(("O", "PDF 130"));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trimestre_al_entrar() {
        assert_eq!(trimestre_por_defecto(2026, "2026-10-19"), 3); // plazo del 3T
        assert_eq!(trimestre_por_defecto(2026, "2026-10-25"), 4); // pasado el plazo: el que está en curso
        assert_eq!(trimestre_por_defecto(2026, "2026-02-10"), 1);
        assert_eq!(trimestre_por_defecto(2025, "2026-01-15"), 4);
        assert_eq!(trimestre_por_defecto(2027, "2026-10-19"), 1);
    }

    #[test]
    fn textos() {
        assert_eq!(ajustar("hola", 6), "hola  ");
        assert_eq!(ajustar("operaciones", 5), "oper…");
        assert_eq!(ajustar("x", 0), "");
    }
}
