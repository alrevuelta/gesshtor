//! «Inicio»: cómo va el año hasta hoy y qué eres ahora (por cuenta ajena, por tu cuenta o las dos cosas). Lo que te
//! queda va en una banda, con lo generado encima y debajo a dónde fue lo demás (Seguridad Social, IRPF y gastos),
//! con su barra fina. Al lado, el próximo plazo (en su cajita) y lo último presentado. Debajo, de más a menos
//! importante: lo que no se ha podido traer, tu alta, actividad y cuota de autónomo, tu última nómina y tu
//! certificado (y cuándo caduca, con lo traído la última vez), que va antes si caduca pronto. Abajo del todo, la
//! carpeta de tus datos y la versión de la app (con su commit). Sin cajas: se separa con aire. Si no cabe todo, se
//! quita el aire y, si ni así, lo menos importante.
use crate::dominio::fmt::{self, eur, fecha_corta, nombre_mes, DIF};
use crate::informe::inicio::{inicio, Inicio as Datos};
use crate::informe::reparto::{reparto, Reparto};
use crate::tui::texto::{cuando, envolver, frase, partir, ruta_corta};
use crate::tui::widgets::{banda, barra, cajita_plazo, guia, muestra, tema, titulo_bloque, ALTO_BANDA, CAJITA_PLAZO};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

#[derive(Default)]
pub struct Inicio {
    clave: Option<(u64, bool, Option<String>)>,
    i: Datos,
    /// El año en curso hasta hoy: lo generado, los impuestos y cotizaciones y lo que te queda.
    anio: Option<Reparto>,
}

/// «hace 3 años y 7 meses», «dentro de 40 días»…
fn plazo_en_dias(dias: i64) -> String {
    let n = dias.abs();
    let txt = if n >= 365 {
        let (a, m) = (n / 365, (n % 365) / 30);
        let anios = if a == 1 { "1 año".to_string() } else { format!("{a} años") };
        if m > 0 { format!("{anios} y {m} {}", if m == 1 { "mes" } else { "meses" }) } else { anios }
    } else if n >= 60 {
        format!("{} meses", n / 30)
    } else if n == 1 {
        "1 día".into()
    } else {
        format!("{n} días")
    };
    if dias < 0 { format!("hace {txt}") } else { format!("dentro de {txt}") }
}

/// Lo de la fila de abajo en `ancho` columnas: «Tus datos: ~/Impuestos/ana» y «gesshtor 0.1.0 · 1a2b3c». Si no
/// cabe, la ruta se corta por delante (lo último es la carpeta de la persona) y, si ni así, sin la versión.
fn borde(ruta: &str, version: &str, ancho: usize) -> (String, Option<String>) {
    let version = format!("gesshtor {version}");
    let n = |s: &str| s.chars().count();
    // Con la versión, si a la ruta le quedan al menos 30 columnas (y 3 de aire entre las dos).
    let con_version = ancho.saturating_sub(3 + n(&version));
    let (libre, version) = if con_version >= 30 { (con_version, Some(version)) } else { (ancho, None) };
    let entera = format!("Tus datos: {ruta}");
    if n(&entera) <= libre {
        return (entera, version);
    }
    let cabe = libre.saturating_sub(n("Tus datos: …"));
    let cola: String = ruta.chars().skip(n(ruta).saturating_sub(cabe)).collect();
    (format!("Tus datos: …{cola}"), version)
}

/// Lo más ancho de cada columna.
const COLUMNA: u16 = 46;

fn negrita() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// «2026-10-14» → «14 oct» si es de `anio`; si no, con el año: «28 ene 2027».
fn dia_corto(fecha: &str, anio: i32) -> String {
    let c = fecha_corta(fecha);
    match c.rsplit_once(' ') {
        Some((dm, _)) if fmt::anio_de(fecha) == anio => dm.to_string(),
        _ => c,
    }
}

/// «130», «130 y 303», «130, 303 y 390».
fn y_lista(v: &[String]) -> String {
    match v.split_last() {
        Some((u, resto)) if !resto.is_empty() => format!("{} y {u}", resto.join(", ")),
        Some((u, _)) => u.clone(),
        None => String::new(),
    }
}

/// Una fila de un bloque de texto: un texto en trozos, cada uno con su estilo (se parte si no cabe), o un dato con
/// puntos guía («tu cuota de octubre ······ 294,00 €»).
enum Renglon {
    Texto(Vec<(String, Style)>),
    /// Como `Texto`, pero en una sola fila: si no cabe, se corta con «…».
    Corto(Vec<(String, Style)>),
    Dato(String, String),
}

fn texto(t: impl Into<String>, st: Style) -> Renglon {
    Renglon::Texto(vec![(t.into(), st)])
}

/// Los trozos en una fila de `w` columnas (separados por un espacio, como `envolver`); lo que no cabe, fuera, con «…».
fn una_fila(t: &[(String, Style)], w: usize) -> Line<'static> {
    let mut v = Vec::new();
    let mut libre = w;
    for (k, (txt, st)) in t.iter().enumerate() {
        let txt = if k > 0 { format!(" {txt}") } else { txt.clone() };
        let n = txt.chars().count();
        if n <= libre {
            libre -= n;
            v.push(Span::styled(txt, *st));
        } else {
            let corto: String = txt.chars().take(libre.saturating_sub(1)).collect();
            v.push(Span::styled(format!("{}…", corto.trim_end()), *st));
            break;
        }
    }
    Line::from(v)
}

/// Un bloque de texto: su título (en gris y en minúscula) y sus filas.
struct Bloque {
    titulo: &'static str,
    filas: Vec<Renglon>,
    /// Corre prisa (lo que no se ha podido traer, un certificado que caduca): antes que nada, se tiene que ver entero.
    urge: bool,
}

impl Bloque {
    /// Sus líneas en `w` columnas, fila a fila (la primera, el título): lo largo, partido; los datos, con la cifra
    /// acabando en `w` (si no caben en una fila, con la etiqueta partida).
    fn filas(&self, w: u16) -> Vec<Vec<Line<'static>>> {
        let ancho = w as usize;
        let mut v = vec![vec![titulo_bloque(self.titulo)]];
        for r in &self.filas {
            v.push(match r {
                Renglon::Texto(t) => envolver(t, ancho, 0),
                Renglon::Corto(t) => vec![una_fila(t, ancho)],
                Renglon::Dato(e, c) if e.chars().count() + c.chars().count() + 3 <= ancho => vec![guia(e.clone(), c.clone(), Style::new(), w)],
                Renglon::Dato(e, c) => {
                    // La etiqueta, partida para que la cifra quepa en su última línea.
                    let mut v: Vec<Line<'static>> = partir(e, ancho.saturating_sub(c.chars().count() + 3)).into_iter().map(Line::from).collect();
                    let ultima = v.pop().unwrap_or_default();
                    v.push(guia(ultima, c.clone(), Style::new(), w));
                    v
                }
            });
        }
        v
    }

    /// Las líneas que caben en `alto` filas: las filas enteras, de la primera en adelante, mientras quepan (y al
    /// menos el título y una). Vacío si no cabe ni eso.
    fn lineas(&self, w: u16, alto: u16) -> Vec<Line<'static>> {
        let mut v = Vec::new();
        for fila in self.filas(w) {
            if v.len() + fila.len() > alto as usize {
                break;
            }
            v.extend(fila);
        }
        if v.len() < 2 { Vec::new() } else { v }
    }

    fn alto(&self, w: u16) -> u16 {
        self.filas(w).iter().map(|f| f.len() as u16).sum()
    }
}

/// Lo último presentado en Hacienda: «✓ 130 y 303 del 3T 2026 · 14 oct».
fn bloque_ultimo(i: &Datos) -> Option<Bloque> {
    let (modelos, periodo, anio, fecha) = i.ultimo_presentado.as_ref()?;
    let mut t = vec![("✓".to_string(), negrita().fg(tema::BIEN)), (format!("{} del {periodo} {anio}", y_lista(modelos)), Style::new())];
    if !fecha.is_empty() {
        t.push((format!("· {}", dia_corto(fecha, *anio)), tema::apagado()));
    }
    Some(Bloque { titulo: "lo último", filas: vec![Renglon::Texto(t)], urge: false })
}

/// Tu alta y tu baja, tu actividad en la Seguridad Social, tu cuota y tu base de cotización (y la próxima).
fn bloque_autonomo(a: &crate::informe::inicio::Autonomo) -> Bloque {
    let gris = tema::apagado();
    let mut v = Vec::new();
    if !a.alta.is_empty() {
        let mut t = vec![(format!("De alta desde el {}", fecha_corta(&a.alta)), Style::new())];
        if !a.baja.is_empty() {
            t.push((format!(", de baja desde el {}", fecha_corta(&a.baja)), Style::new()));
        }
        let mutua = a.mutua.entidad.trim();
        if !mutua.is_empty() {
            t.push((format!("· mutua {mutua}"), gris));
        }
        v.push(Renglon::Texto(t));
    }
    if let Some((cuota, mes)) = &a.cuota {
        v.push(Renglon::Dato(format!("tu cuota de {mes}"), eur(*cuota)));
    }
    if let Some(b) = a.cotizacion.base {
        v.push(Renglon::Dato("base de cotización".into(), eur(b)));
    }
    if let Some(r) = a.cotizacion.rendimientos_previstos {
        v.push(Renglon::Dato("rendimientos previstos".into(), eur(r)));
    }
    if let Some(b) = a.cotizacion.base_proxima {
        let desde = &a.cotizacion.base_proxima_desde;
        let e = if desde.is_empty() { "tu base pasa a".to_string() } else { format!("desde el {}, tu base", fecha_corta(desde)) };
        v.push(Renglon::Dato(e, eur(b)));
    }
    // Lo que figura como tu actividad, al final: si no cabe todo, es lo que menos falta hace.
    for act in &a.actividades {
        let mut t = vec![(act.tipo.trim().to_string(), Style::new())];
        let desde = act.inicio.trim();
        if !desde.is_empty() {
            t.push((format!("· desde el {}", fecha_corta(desde)), gris));
        }
        v.push(Renglon::Texto(t));
        let cnae = [act.cnae.trim().to_string(), frase(&act.cnae_texto)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        if !cnae.is_empty() {
            v.push(texto(cnae, gris));
        }
    }
    Bloque { titulo: "autónomo", filas: v, urge: false }
}

/// Tu última nómina según la Seguridad Social y hasta cuándo se ha consultado.
fn bloque_empleo(n: &crate::informe::inicio::UltimaNomina) -> Bloque {
    let donde = if n.empresas.is_empty() { String::new() } else { format!(", en {}", n.empresas.join(" y ")) };
    let hasta = if n.consultado_hasta.as_str() > n.mes.as_str() {
        format!("La Seguridad Social está consultada hasta {}.", nombre_mes(&n.consultado_hasta))
    } else {
        format!("Según la Seguridad Social, consultada hasta {}.", nombre_mes(&n.consultado_hasta))
    };
    let filas = vec![texto(format!("Tu última nómina: {}{donde}.", nombre_mes(&n.mes)), Style::new()), texto(hasta, tema::apagado())];
    Bloque { titulo: "empleo", filas, urge: false }
}

/// Tu certificado: de quién es, cuándo caduca, lo traído con él la última vez (o por dónde va) y quién lo emitió. Y si
/// corre prisa: si caduca en menos de 60 días (o ya caducó).
fn bloque_certificado(c: &crate::red::certificado::Certificado, ctx: &Ctx) -> (Bloque, bool) {
    let gris = tema::apagado();
    let dias = c.dias_para_caducar();
    let st = if dias < 0 {
        negrita().fg(tema::MAL)
    } else if dias < 60 {
        negrita().fg(tema::AVISO)
    } else {
        Style::new()
    };
    let verbo = if dias < 0 { "Caducó" } else { "Caduca" };
    let mut caduca = vec![(format!("{verbo} el {} ({})", fecha_corta(&c.caduca), plazo_en_dias(dias)), st)];
    if (0..60).contains(&dias) {
        caduca.push(("· renuévalo en la web de la FNMT".into(), st));
    }
    // De quién es y cuándo caduca; si corre prisa, primero cuándo caduca.
    let quien = texto(format!("{} · {}", c.nombre_completo(), c.nif), Style::new());
    let prisa = dias < 60;
    let mut v = if prisa { vec![Renglon::Texto(caduca), quien] } else { vec![quien, Renglon::Texto(caduca)] };
    v.push(match (&ctx.sincronizando, &ctx.sincronizado) {
        (Some(p), _) => texto(format!("Trayendo lo último… {p}"), Style::new().fg(tema::acento())),
        (None, Some((c, res, _))) => texto(format!("Traído {}: {res}.", cuando(c)), Style::new().fg(tema::BIEN)),
        (None, None) => texto("S: traer lo último de Hacienda y la Seguridad Social.", gris),
    });
    // Quién lo emitió, al final: si no cabe todo, es lo que menos falta hace.
    if !c.emisor.is_empty() {
        v.push(texto(format!("Emitido por {}", c.emisor), gris));
    }
    (Bloque { titulo: "tu certificado", filas: v, urge: prisa }, prisa)
}

/// Lo que no se ha podido traer la última vez (hasta 4 cosas), en MAL. Con poco sitio (`cortos`), cada una en una
/// fila, cortada si no cabe (lo de Hacienda y la Seguridad Social, entero, también está con «!», en los fallos).
fn bloque_errores(ctx: &Ctx, cortos: bool) -> Option<Bloque> {
    let (_, _, errores) = ctx.sincronizado.as_ref().filter(|_| ctx.sincronizando.is_none())?;
    let fila = |e: &String| {
        let t = vec![("✗".into(), negrita().fg(tema::MAL)), (e.clone(), Style::new().fg(tema::MAL))];
        if cortos { Renglon::Corto(t) } else { Renglon::Texto(t) }
    };
    let filas = errores.iter().take(4).map(fila).collect::<Vec<_>>();
    (!filas.is_empty()).then_some(Bloque { titulo: "no se ha podido traer", filas, urge: true })
}

/// Los bloques de debajo, de más a menos importante: lo que no se ha podido traer; tu certificado si caduca pronto;
/// lo de autónomo y tu nómina (lo de ahora primero: si solo trabajas por cuenta ajena, la nómina); y tu certificado
/// si no corre prisa.
fn bloques(i: &Datos, ctx: &Ctx, cortos: bool) -> Vec<Bloque> {
    let cert = ctx.certificado.as_deref().map(|c| bloque_certificado(c, ctx));
    let (urge, cert) = match cert {
        Some((b, true)) => (Some(b), None),
        Some((b, false)) => (None, Some(b)),
        None => (None, None),
    };
    let mut trabajo: Vec<Bloque> = i.autonomo.as_ref().map(bloque_autonomo).into_iter().chain(i.empleo.as_ref().map(bloque_empleo)).collect();
    if i.empleo_ahora && !i.autonomo_ahora {
        trabajo.reverse();
    }
    bloque_errores(ctx, cortos).into_iter().chain(urge).chain(trabajo).chain(cert).collect()
}

/// Una fila de cómo va el año: una línea o la banda de lo que te queda (tres filas, con su fondo).
enum FilaAnio {
    Linea(Line<'static>),
    Banda(Line<'static>, Color),
}

/// Cómo va el año en `w` columnas: lo generado, la barra fina, lo que te queda (en su banda), a dónde fue lo demás
/// (cada uno con la muestra de su tramo) y, si hay, el IVA, que no cuenta. Las cifras acaban en `w`. `holgado`:
/// con aire entre las partes.
fn filas_anio(r: &Reparto, w: u16, holgado: bool) -> Vec<FilaAnio> {
    let t = &r.total;
    let mut v = Vec::new();
    let aire = |v: &mut Vec<FilaAnio>| {
        if holgado {
            v.push(FilaAnio::Linea(Line::default()));
        }
    };
    v.push(FilaAnio::Linea(guia(Span::styled("generaste en el año", tema::apagado()), eur(fmt::r2(t.generado - t.iva)), Style::new(), w)));
    v.push(FilaAnio::Linea(barra(&[(t.queda, tema::BIEN), (t.seguridad_social, tema::AZUL), (t.irpf, tema::AVISO), (t.gastos, tema::MARCO)], w)));
    aire(&mut v);
    // Si te queda menos que nada, en MAL, sobre un fondo suave como los SUAVE_* de la paleta (en torno al 13 %).
    let (c, fondo) = if t.queda < -DIF { (tema::MAL, tema::mezcla(tema::MAL, tema::FONDO, 0.13)) } else { (tema::BIEN, tema::SUAVE_BIEN) };
    let queda = Line::from(vec![muestra(c), Span::styled("te queda", negrita().fg(c))]);
    v.push(FilaAnio::Banda(guia(queda, eur(t.queda), Style::new().fg(c), w), fondo));
    aire(&mut v);
    let irpf = if r.con_renta { "IRPF" } else { "IRPF adelantado" };
    let mut partes = vec![(tema::AZUL, "Seguridad Social", t.seguridad_social), (tema::AVISO, irpf, t.irpf)];
    if t.gastos.abs() > DIF {
        partes.push((tema::MARCO, "gastos del negocio", t.gastos));
    }
    for (c, e, x) in partes {
        v.push(FilaAnio::Linea(guia(Line::from(vec![muestra(c), Span::raw(e)]), eur(x), Style::new(), w)));
    }
    if t.iva > DIF {
        aire(&mut v);
        let nota = format!("el IVA que pasa por tus manos ({}) no cuenta: es de Hacienda.", eur(t.iva));
        v.extend(envolver(&[(nota, tema::apagado())], w as usize, 0).into_iter().map(FilaAnio::Linea));
    }
    v
}

fn alto_anio(v: &[FilaAnio]) -> u16 {
    v.iter().map(|x| if matches!(x, FilaAnio::Banda(..)) { ALTO_BANDA } else { 1 }).sum()
}

/// Pinta cómo va el año desde arriba de `area`. La banda sobresale 2 columnas por la derecha (si hay sitio), para que
/// la cifra no quede pegada al borde de su fondo.
fn pintar_anio(f: &mut Frame, area: Rect, filas: Vec<FilaAnio>, limite: Rect) {
    let mut y = area.y;
    for x in filas {
        match x {
            FilaAnio::Linea(l) => {
                f.render_widget(Paragraph::new(l), Rect { y, height: 1, ..area }.intersection(limite));
                y += 1;
            }
            FilaAnio::Banda(l, fondo) => {
                let r = Rect { y, height: ALTO_BANDA, width: area.width + 2, ..area }.intersection(limite);
                if r.height == ALTO_BANDA {
                    banda(f, r, fondo, l);
                } else {
                    // Sin sitio para el aire: solo la cifra, en la fila que quede.
                    f.render_widget(Paragraph::new(l), Rect { height: r.height.min(1), ..r });
                }
                y += ALTO_BANDA;
            }
        }
    }
}

/// Con la cajita de calendario (y el texto al lado) si caben en `w` columnas.
fn con_cajita(w: u16) -> bool {
    w >= CAJITA_PLAZO.0 + 2 + 22
}

/// El próximo plazo, en texto: cuál, sus fechas («del 1 ene al 1 feb»; sin la cajita, que dice el año, «… al 1 feb
/// 2027») y cuánto falta para que empiece o termine.
fn textos_plazo(q: &str, ini: &str, fin: &str, con_cajita: bool) -> (Line<'static>, Line<'static>, Line<'static>) {
    let hoy = fmt::hoy();
    let anio = fmt::anio_de(fin);
    let hasta = if con_cajita { dia_corto(fin, anio) } else { fecha_corta(fin) };
    let fechas = format!("del {} al {hasta}", dia_corto(ini, anio));
    let ac = negrita().fg(tema::acento());
    let gris = tema::apagado();
    let dias = |n: i64| if n == 1 { "1 día".to_string() } else { format!("{n} días") };
    let falta = if ini > hoy.as_str() {
        Line::from(vec![Span::styled("empieza en ", gris), Span::styled(dias(fmt::dias_entre(&hoy, ini)), ac)])
    } else {
        match fmt::dias_entre(&hoy, fin) {
            0 => Line::styled("hoy es el último día", ac),
            n => Line::from(vec![Span::styled("faltan ", gris), Span::styled(dias(n), ac)]),
        }
    };
    (Line::from(format!("el {q}")), Line::styled(fechas, gris), falta)
}

/// Las filas que ocupa el próximo plazo (con su título) en `w` columnas.
fn alto_plazo(w: u16) -> u16 {
    1 + if con_cajita(w) { CAJITA_PLAZO.1 } else { 3 }
}

/// El próximo plazo desde arriba de `area` (y en su título, en rojo, cuántos modelos hay vencidos): la cajita con el último día y, al lado, cuál es, sus fechas
/// y cuánto falta; si no cabe la cajita, solo el texto.
fn pintar_plazo(f: &mut Frame, area: Rect, (q, ini, fin): &(String, String, String), limite: Rect, vencidos: usize) {
    let fila = |y: u16, x: u16| Rect { x: area.x + x, y: area.y + y, width: area.width.saturating_sub(x), height: 1 }.intersection(limite);
    let mut titulo = titulo_bloque("próximo plazo");
    if vencidos > 0 {
        titulo.push_span(Span::styled(format!(" · {vencidos} vencido{}", if vencidos == 1 { "" } else { "s" }), negrita().fg(tema::MAL)));
    }
    f.render_widget(Paragraph::new(titulo), fila(0, 0));
    let (cual, fechas, falta) = textos_plazo(q, ini, fin, con_cajita(area.width));
    if con_cajita(area.width) {
        let caja = Rect { y: area.y + 1, height: CAJITA_PLAZO.1, ..area }.intersection(limite);
        if caja.height == CAJITA_PLAZO.1 {
            cajita_plazo(f, caja, fin);
        }
        let x = CAJITA_PLAZO.0 + 2;
        // Como en el boceto: cuál y sus fechas, arriba; cuánto falta, a la altura del día.
        for (y, l) in [(1, cual), (2, fechas), (4, falta)] {
            f.render_widget(Paragraph::new(l), fila(y, x));
        }
    } else {
        for (y, l) in [(1, cual), (2, fechas), (3, falta)] {
            f.render_widget(Paragraph::new(l), fila(y, 0));
        }
    }
}

/// Dónde va cada cosa del contenido (menos la fila de abajo, la de tus datos).
#[derive(Default)]
struct Plan {
    /// Arriba: «Así va tu 2026, hasta hoy.» y qué eres ahora.
    arriba: Vec<Line<'static>>,
    /// Lo de en medio: hasta dónde llega (lo que se salga, se recorta).
    medio: Rect,
    anio: Option<(Rect, Vec<FilaAnio>)>,
    plazo: Option<Rect>,
    bloques: Vec<(Rect, Vec<Line<'static>>)>,
    /// ¿Ha cabido todo entero? ¿Y lo que corre prisa?
    entero: bool,
    urgente_entero: bool,
}

/// Las columnas, de la que va más corta (`y`: por qué fila va cada una) a la que más.
fn por_orden(y: &[u16]) -> Vec<usize> {
    let mut v: Vec<usize> = (0..y.len()).collect();
    v.sort_by_key(|&c| (y[c], c));
    v
}

/// Pone un bloque en la primera columna de `orden` en la que cabe entero o, si no cabe en ninguna, lo que quepa de
/// él (sus primeras filas) en la que tenga más sitio. `y`: por qué fila va cada columna.
fn poner(plan: &mut Plan, b: &Bloque, cols: &[Rect], orden: &[usize], y: &mut [u16]) {
    let fin = plan.medio.bottom();
    let libre = |c: usize, y: &[u16]| fin.saturating_sub(y[c]);
    let entero = orden.iter().copied().find(|&c| b.alto(cols[c].width) <= libre(c, y));
    plan.entero &= entero.is_some();
    plan.urgente_entero &= entero.is_some() || !b.urge;
    let Some(c) = entero.or_else(|| orden.iter().copied().max_by_key(|&c| (libre(c, y), std::cmp::Reverse(c)))) else { return };
    let lineas = b.lineas(cols[c].width, libre(c, y));
    if !lineas.is_empty() {
        let alto = lineas.len() as u16;
        plan.bloques.push((Rect { y: y[c], height: alto, ..cols[c] }, lineas));
        y[c] += alto + 1;
    }
}

impl Inicio {
    /// Dónde va cada cosa en `area`. Arriba, «Así va tu 2026, hasta hoy.» y qué eres ahora. En medio, en dos columnas
    /// si caben (en una, si no): a la izquierda, cómo va el año; a la derecha, el próximo plazo y lo último; y debajo,
    /// los demás bloques, de más a menos importante, cada uno en la columna que vaya más corta. `holgado`: con aire
    /// entre las partes (sin él, cabe más); `cortos`: lo que no se ha podido traer, en una fila cada cosa.
    fn planear(&self, area: Rect, ctx: &Ctx, holgado: bool, cortos: bool) -> Plan {
        let i = &self.i;
        let gris = tema::apagado();
        let ahora = match (i.empleo_ahora, i.autonomo_ahora) {
            (true, true) => "Ahora trabajas por cuenta ajena y por tu cuenta (en autónomos).",
            (false, true) => "Ahora trabajas por tu cuenta (de alta en autónomos).",
            (true, false) => "Ahora trabajas por cuenta ajena.",
            (false, false) => "Ahora no constas de alta en autónomos ni con nóminas recientes.",
        };
        let mut arriba = Vec::new();
        if self.anio.is_some() {
            let mut l = Line::from(vec![Span::styled(format!("Así va tu {}", fmt::anio_actual()), negrita()), Span::styled(", hasta hoy.", gris)]);
            // Sin aire, qué eres ahora va en la misma fila, si cabe.
            if !holgado && l.width() + 3 + ahora.chars().count() <= area.width as usize {
                l.push_span(Span::styled(format!("   {ahora}"), gris));
                arriba.push(l);
            } else {
                arriba.push(l);
                arriba.extend(envolver(&[(ahora.to_string(), gris)], area.width as usize, 0));
            }
        } else {
            arriba.extend(envolver(&[(ahora.to_string(), gris)], area.width as usize, 0));
        }
        // Lo vencido va junto al próximo plazo, sin quitarle sitio a nada; si no hay plazo que enseñar, aquí arriba.
        if ctx.vencidos > 0 && i.proximo_plazo.is_none() {
            let que = if ctx.vencidos == 1 { "Tienes 1 modelo vencido sin presentar".to_string() } else { format!("Tienes {} modelos vencidos sin presentar", ctx.vencidos) };
            let aviso = envolver(&[(que, negrita().fg(tema::MAL)), (" · pulsa a y 3".to_string(), gris)], area.width as usize, 0);
            arriba.splice(0..0, aviso);
        }
        let n_arriba = arriba.len() as u16;
        // Debajo, la fila de tus datos (y, con aire, una vacía encima).
        let medio = Rect { y: area.y + n_arriba + 1, height: area.height.saturating_sub(n_arriba + 1 + 1 + u16::from(holgado)), ..area };
        let mut plan = Plan { arriba, medio, entero: true, urgente_entero: true, ..Default::default() };

        // Como en el boceto, de 46 columnas como mucho cada una: más anchas, las cifras quedan lejos de lo que son.
        let dos = area.width >= 72;
        let hueco = if area.width >= 90 { 4 } else { 3 };
        let izq = if dos { COLUMNA.min(area.width - hueco - 33) } else { area.width };
        let mut cols = vec![Rect { width: izq, ..medio }];
        if dos {
            cols.push(Rect { x: medio.x + izq + hueco, width: (area.width - izq - hueco).min(COLUMNA), ..medio });
        }
        let mut y: Vec<u16> = cols.iter().map(|c| c.y).collect();

        if let Some(r) = &self.anio {
            let filas = filas_anio(r, cols[0].width, holgado);
            let alto = alto_anio(&filas);
            plan.entero &= y[0] + alto <= medio.bottom();
            plan.anio = Some((Rect { y: y[0], height: alto, ..cols[0] }, filas));
            y[0] += alto + 1;
        }
        // El plazo y lo último, juntos: a la derecha del año o, sin él, en la columna más corta.
        let lado = if self.anio.is_some() && dos { 1 } else { por_orden(&y)[0] };
        if i.proximo_plazo.is_some() {
            let alto = alto_plazo(cols[lado].width);
            if y[lado] + alto <= medio.bottom() {
                plan.plazo = Some(Rect { y: y[lado], height: alto, ..cols[lado] });
                y[lado] += alto + 1;
            } else {
                plan.entero = false;
            }
        }
        if let Some(b) = bloque_ultimo(i) {
            let mut orden = por_orden(&y);
            orden.sort_by_key(|&c| c != lado);
            poner(&mut plan, &b, &cols, &orden, &mut y);
        }
        for b in bloques(i, ctx, cortos) {
            let orden = por_orden(&y);
            poner(&mut plan, &b, &cols, &orden, &mut y);
        }
        plan
    }
}

impl Pantalla for Inicio {
    fn titulo(&self) -> &'static str {
        "Inicio"
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let clave = (ctx.generacion, ctx.certificado.is_some(), ctx.sincronizando.clone());
        if self.clave.as_ref() != Some(&clave) {
            self.i = inicio(&ctx.datos, ctx.certificado.as_deref());
            let det = crate::informe::anio::detalle(&ctx.datos, fmt::anio_actual());
            let r = reparto(&ctx.datos, &det, None);
            self.anio = (r.total.generado.abs() > 0.005).then_some(r);
            self.clave = Some(clave);
        }
        if area.width == 0 || area.height == 0 {
            return;
        }
        // Con aire si hay sitio; si así no cabe todo entero, sin él; y si ni así cabe entero lo que corre prisa, con lo
        // que no se ha podido traer en una fila cada cosa.
        let mut plan = Plan::default();
        for (holgado, cortos) in [(true, false), (false, false), (false, true)] {
            if holgado && area.height < 28 {
                continue;
            }
            plan = self.planear(area, ctx, holgado, cortos);
            if plan.entero || (!holgado && plan.urgente_entero) {
                break;
            }
        }

        let n_arriba = plan.arriba.len() as u16;
        f.render_widget(Paragraph::new(plan.arriba), Rect { height: n_arriba, ..area }.intersection(area));
        if let Some((r, filas)) = plan.anio {
            pintar_anio(f, r, filas, plan.medio);
        }
        if let (Some(r), Some(p)) = (plan.plazo, &self.i.proximo_plazo) {
            pintar_plazo(f, r, p, plan.medio, ctx.vencidos);
        }
        for (r, lineas) in plan.bloques {
            f.render_widget(Paragraph::new(lineas), r);
        }

        // Abajo del todo, siempre a la vista: dónde están tus datos y qué versión es.
        let ultima = Rect { y: area.bottom() - 1, height: 1, ..area };
        let (datos, version) = borde(&ruta_corta(&ctx.dir), crate::VERSION, area.width as usize);
        f.render_widget(Paragraph::new(Line::styled(datos, tema::apagado())), ultima);
        if let Some(v) = version {
            f.render_widget(Paragraph::new(Line::styled(v, tema::apagado()).right_aligned()), ultima);
        }
    }

    fn tecla(&mut self, _k: KeyEvent, _ctx: &mut Ctx) -> bool {
        false
    }

    fn usa_anio(&self) -> bool {
        false
    }

    fn atajos(&self) -> Vec<(&'static str, &'static str)> {
        vec![("S", "traer lo último"), ("Tab", "más")]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_borde_de_abajo() {
        let (d, v) = borde("~/Impuestos/ana", "0.1.0 · 1a2b3c4d5e6f", 100);
        assert_eq!((d.as_str(), v.as_deref()), ("Tus datos: ~/Impuestos/ana", Some("gesshtor 0.1.0 · 1a2b3c4d5e6f")));
        // Si no cabe, se ve el final de la ruta: la carpeta de la persona.
        let (d, v) = borde("~/Documentos/mis papeles/Impuestos/ana-perez", "0.1.0 · 1a2b3c4d5e6f", 70);
        assert!(d.ends_with("Impuestos/ana-perez") && d.contains('…') && v.is_some(), "{d}");
        assert!(d.chars().count() + 3 + v.unwrap().chars().count() <= 70);
        // Muy estrecho: solo la ruta.
        let (d, v) = borde("~/Impuestos/ana-perez", "0.1.0 · 1a2b3c4d5e6f", 40);
        assert!(v.is_none() && d.ends_with("ana-perez") && d.chars().count() <= 40, "{d}");
    }

    #[test]
    fn listas_y_fechas_cortas() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(y_lista(&v(&["130"])), "130");
        assert_eq!(y_lista(&v(&["130", "303"])), "130 y 303");
        assert_eq!(y_lista(&v(&["130", "303", "390"])), "130, 303 y 390");
        assert_eq!(dia_corto("2026-10-14", 2026), "14 oct");
        assert_eq!(dia_corto("2027-01-28", 2026), "28 ene 2027");
    }

    /// Una persona con todo: alta de autónoma (con su actividad, su cuota y sus bases), nómina, lo presentado en
    /// Hacienda, una nómina de este año y su certificado (que caduca `en` días), con lo traído (y lo que no).
    fn con_todo(en: i64, errores: &[&str]) -> crate::tui::App {
        use crate::dominio::hacienda::{Consulta, Decl, Modelo};
        use crate::dominio::tgss::{Actividad, Alta, Cotizacion, Liquidacion, Mutua};
        let mut a = crate::tui::prueba::app();
        let mut c = crate::red::certificado::Certificado::de_prueba("00000000T", "ANA", "PRUEBA");
        c.caduca = fmt::sumar_dias(&fmt::hoy(), en);
        a.ctx.certificado = Some(std::sync::Arc::new(c));
        let resumen = "Hacienda: 2 años · Seguridad Social: al día · 3 meses de nómina · 5 PDF nuevos";
        a.ctx.sincronizado = Some((fmt::ahora(), resumen.into(), errores.iter().map(|e| e.to_string()).collect()));
        let d = &mut a.ctx.datos;
        let y = fmt::anio_actual();
        d.nominas.push(crate::dominio::Nomina { id: "n1".into(), anio: y, mes: 1, dinerario: 2000.0, ..Default::default() });
        let t = &mut d.tgss;
        t.reta = true;
        t.alta = Alta { fecha: "2023-03-01".into(), ..Default::default() };
        t.mutua = Mutua { entidad: "Mutua de Prueba".into() };
        t.actividades = vec![Actividad { inicio: "2023-03-01".into(), tipo: "Trabajador por cuenta propia".into(), cnae: "6201".into(), cnae_texto: "PROGRAMACION INFORMATICA".into() }];
        t.cotizacion = Cotizacion {
            rendimientos_previstos: Some(18000.0),
            base: Some(1150.0),
            base_proxima: Some(1200.0),
            base_proxima_desde: format!("{}-01-01", y + 1),
            ultima_cuota: Some(294.0),
            ultima_cuota_mes: "octubre".into(),
        };
        for n in 1..=2 {
            t.nomina.meses.insert(fmt::mes_mas(fmt::ym(&fmt::hoy()), -n), vec![Liquidacion { empresa: "Empresa de Prueba SL".into(), ..Default::default() }]);
        }
        let decl = |m: &str| Decl { modelo: m.into(), ejercicio: y - 1, periodo: "4T".into(), justificante: format!("{m}0000000001"), fecha: format!("{y}-01-28T10:00:00"), ..Default::default() };
        let modelos = ["130", "303"].map(|m| (m.to_string(), Modelo { fecha: fmt::ahora(), decl: vec![decl(m)] }));
        d.hacienda.insert(y - 1, Consulta { fecha: fmt::ahora(), modelos: modelos.into_iter().collect(), ..Default::default() });
        crate::tui::prueba::ir(&mut a, "Inicio");
        a
    }

    fn ver(p: &str) {
        if std::env::var("VER").is_ok() {
            println!("{p}");
        }
    }

    /// Con todo y sin prisa, a 120×36 cabe todo entero: el año, el plazo, lo último, lo de autónomo (con su
    /// actividad), la nómina y el certificado (con lo traído y quién lo emitió). Sin el «Hola» repetido.
    #[test]
    fn con_todo_cabe_entero() {
        use crate::tui::prueba::pantalla;
        let mut a = con_todo(400, &[]);
        let p = pantalla(&mut a, 120, 36);
        ver(&p);
        for t in ["Así va tu", "te queda", "próximo plazo", "lo último", "✓ 130 y 303 del 4T", "autónomo", "tu cuota de octubre", "6201 · Programacion", "empleo", "Tu última nómina", "consultada hasta", "tu certificado", "Traído hoy", "Emitido por Pruebas", "Tus datos:"] {
            assert!(p.contains(t), "falta «{t}»:\n{p}");
        }
        assert!(!p.contains("Hola, Ana."), "el saludo, solo arriba a la derecha:\n{p}");
        // A 80×24 no cabe todo, pero sí lo principal.
        let p = pantalla(&mut a, 80, 24);
        ver(&p);
        for t in ["Así va tu", "te queda", "próximo plazo", "lo último", "autónomo", "Tus datos:"] {
            assert!(p.contains(t), "falta «{t}»:\n{p}");
        }
    }

    /// Si el certificado caduca pronto y algo no se ha podido traer, eso va lo primero de lo de debajo y se ve entero,
    /// también a 80×24.
    #[test]
    fn lo_que_falla_y_el_certificado_van_primero() {
        use crate::tui::prueba::pantalla;
        let errores = [
            "Seguridad Social: no se ha podido conectar con el servidor (tiempo agotado)",
            "Hacienda (2026): la sede electrónica no responde, inténtalo más tarde",
            "PDF del 303 3T de 2026: no se ha podido descargar",
            "Seguridad Social (nóminas): la sesión ha caducado",
        ];
        let mut a = con_todo(30, &errores);
        for (w, h) in [(120, 36), (80, 24)] {
            let p = pantalla(&mut a, w, h);
            ver(&p);
            assert!(p.contains("no se ha podido traer"), "{w}x{h}:\n{p}");
            for e in ["✗ Seguridad Social:", "✗ Hacienda (2026):", "✗ PDF del 303", "✗ Seguridad Social (nóminas):"] {
                assert!(p.contains(e), "{w}x{h}: falta «{e}»:\n{p}");
            }
            assert!(p.contains("tu certificado") && p.contains("Caduca el") && p.contains("renuévalo"), "{w}x{h}:\n{p}");
            assert!(p.contains("Así va tu") && p.contains("te queda"), "{w}x{h}:\n{p}");
        }
    }

    /// Sin nada del año (el 1 de enero, por ejemplo): el plazo y lo demás, desde arriba a la izquierda. Y si solo
    /// trabajas por cuenta ajena: sin plazo, y la nómina antes que lo de autónomo.
    #[test]
    fn sin_el_anio_y_solo_por_cuenta_ajena() {
        use crate::tui::prueba::{app_con, cargar, caso, ir, pantalla};
        let dir = caso("a-de-todo");
        let mut d = cargar(&dir);
        d.facturas.clear();
        d.gastos.clear();
        d.nominas.clear();
        let mut a = app_con(d.clone(), &dir);
        ir(&mut a, "Inicio");
        let p = pantalla(&mut a, 120, 36);
        ver(&p);
        let filas: Vec<&str> = p.lines().collect();
        assert!(!p.contains("Así va tu") && filas[4].contains("Ahora trabajas por tu cuenta"), "{p}");
        assert!(filas[6].chars().skip(crate::tui::CONTENIDO as usize).collect::<String>().starts_with("próximo plazo"), "a la izquierda:\n{p}");
        assert!(p.contains("autónomo") && p.contains("De alta desde"), "{p}");

        d.perfil.fecha_alta.clear();
        d.nominas.push(crate::dominio::Nomina { id: "n1".into(), anio: fmt::anio_actual(), mes: 1, dinerario: 2000.0, ..Default::default() });
        d.tgss.nomina.meses.insert(fmt::mes_mas(fmt::ym(&fmt::hoy()), -1), vec![crate::dominio::tgss::Liquidacion { empresa: "Empresa de Prueba SL".into(), ..Default::default() }]);
        let mut a = app_con(d, &dir);
        ir(&mut a, "Inicio");
        for (w, h) in [(120, 36), (80, 24)] {
            let p = pantalla(&mut a, w, h);
            ver(&p);
            assert!(p.contains("Así va tu") && p.contains("Ahora trabajas por cuenta ajena.") && p.contains("Tu última nómina"), "{p}");
            assert!(!p.contains("próximo plazo") && !p.contains("gastos del negocio"), "{p}");
        }
    }
}
