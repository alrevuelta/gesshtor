//! Impuestos: en la pestaña «Año», cómo va la actividad (lo facturado y lo gastado, los avisos, los trimestres y
//! los meses: `impuestos_anio`); en cada trimestre, los modelos 303 (IVA) y 130 (IRPF) casilla a casilla: lo que
//! hay que poner y, si se presentó, si cuadra con tus datos y por qué no; en «390», el resumen anual del IVA y si
//! toca el 349 o el 347. Se pasa de una a otra con ← →. Solo se mira: no cambia nada.
//!
//! Sin cajas (estilo C2): arriba, las pestañas y el periodo; en un trimestre, el 303 y el 130 uno al lado del otro
//! (o uno debajo del otro, si no caben), cada uno con su resultado en una banda, el sello de si cuadra y sus
//! casillas; debajo, lo que conviene saber y la fila del año. Lo que no cabe se ve bajando (↑ ↓).
use crate::dominio::fmt::DIF;

use crate::dominio::{Casillas, Datos};
use crate::hacienda::{modelo_303, modelo_390};
use crate::informe::autonomo::Conciliacion;
use std::collections::{BTreeMap, BTreeSet};
use crate::hacienda::modelo::Memo;
use crate::hacienda::modelo_130::{self, Exento, OrigenRn, PorQue, RnAnterior};
use crate::hacienda::Trimestral;
use crate::dominio::fmt;
use crate::informe::autonomo;
use crate::tui::widgets::{atajo, sello, tema, titulo_bloque};
use crate::tui::texto::lineas;
use crate::tui::{Ctx, Pantalla};
use casillas::{Clase, Fila, Grupo};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::PathBuf;
use trimestre::Estado;
use super::{impuestos_anio, impuestos_casillas as casillas, impuestos_trimestre as trimestre};
use crate::dominio::Valor;

/// Desde este ancho del contenido, el 303 y el 130 van uno al lado del otro (a 120 columnas, el contenido tiene 96).
const ANCHO_DOBLE: u16 = 96;
/// Columnas entre el 303 y el 130.
const HUECO: usize = 4;
/// Lo más ancho del nombre de una casilla: con más sitio, los importes quedarían lejos de su nombre.
const NOMBRE_MAX: usize = 60;
/// Lo que ocupa «↓ más», con aire delante.
const MAS: usize = 8;

#[derive(Default)]
pub struct Impuestos {
    /// Trimestre que se ve (1–4), si no se ve el año.
    q: u32,
    /// Año para el que se eligió la pestaña (0 = aún sin elegir).
    anio: i32,
    /// También las casillas a cero.
    todas: bool,
    /// La pestaña «Año» en vez de un trimestre.
    anual: bool,
    /// La pestaña «390» (el resumen anual del IVA y las demás declaraciones del año).
    v390: bool,
    scroll: u16,
    /// Los PDF del trimestre que se ve (para la barra de atajos).
    pdf303: bool,
    pdf130: bool,
    /// Lo calculado (las cadenas de 303 y 130, los estados, los PDF): no se rehace en cada dibujo.
    cab: Option<Cabecera>,
    trim: Option<DelTrimestre>,
    del390: Option<Del390>,
    /// La pestaña «Año».
    resumen: impuestos_anio::Anio,
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
    /// Algún modelo presentado del trimestre no cuadra con tus datos.
    no_cuadra: [bool; 4],
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
        let mut memo = Memo::default();
        let no_cuadra = std::array::from_fn(|i| Trimestral::TODOS.iter().any(|&m| autonomo::cuadre(d, m, y, i as u32 + 1, &mut memo).is_some_and(|c| !c.cuadra())));
        Cabecera { clave, estados, cuantos, no_cuadra }
    }
}

/// El 303 y el 130 del trimestre que se ve.
struct DelTrimestre {
    clave: Clave,
    m303: Modelo,
    m130: Modelo,
}

/// El 390 del año que se ve, y si toca el 349 o el 347.
struct Del390 {
    clave: Clave,
    calc: modelo_390::Calculado,
    pres: Option<Casillas>,
    conc: Option<Conciliacion>,
    informativas: Vec<String>,
    pdf: Option<PathBuf>,
    /// Fecha de presentación, si consta (vacía si solo se tiene el PDF).
    presentado: Option<String>,
}

impl Del390 {
    fn nuevo(ctx: &Ctx, clave: Clave) -> Self {
        let (d, anio) = (&ctx.datos, ctx.anio);
        let mut memo = Memo::default();
        let calc = modelo_390::calcular(d, anio, &mut memo);
        let conc = autonomo::cuadre_390(d, anio, &calc, &mut memo);
        let pres = crate::hacienda::Modelo::M390.leido(d, anio, 0).map(|l| l.casillas.clone()).filter(|c| !c.is_empty());
        let pdf = trimestre::pdf(&ctx.dir, d, anio, 0, crate::hacienda::Modelo::M390);
        let presentado = trimestre::presentado(d, anio, 0, crate::hacienda::Modelo::M390);
        Del390 { clave, calc, pres, conc, informativas: autonomo::informativas(d, anio), pdf, presentado }
    }
}


/// La pestaña «390»: si cuadra con la suma de tus 303 (en un sello), lo que hay que poner o lo presentado, y el 349
/// y el 347. Si se presentó o cuándo se presenta va arriba, en la línea del periodo.
fn doc390(t: &Del390, anio: i32, ancho: usize, todas: bool) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    let vacio = BTreeSet::new();
    let (cols, marcadas): (Vec<(&str, &Casillas)>, BTreeSet<u32>) = match (&t.pres, &t.conc) {
        (Some(pres), Some(conc)) => {
            if conc.cuadra() {
                sello_y(&mut v, "CUADRA", tema::BIEN, tema::SUAVE_BIEN, "con la suma de tus 303 del año", ancho);
            } else {
                let n = conc.distintas();
                let (color, fondo) = if conc.mismo_resultado { (tema::acento(), tema::suave(tema::acento())) } else { (tema::AVISO, tema::SUAVE_AVISO) };
                let que = format!("con la suma de tus 303 ({n} casilla{}) · abajo, por qué", if n == 1 { "" } else { "s" });
                sello_y(&mut v, "NO CUADRA DEL TODO", color, fondo, &que, ancho);
            }
            (vec![("tus 303", &t.calc.casillas), ("presentó", pres)], conc.diferencias.iter().map(|x| x.casilla).chain(conc.arrastradas.iter().copied()).collect())
        }
        (Some(pres), None) => {
            nota(&mut v, "Faltan 303 de este año (sin su PDF ni tus facturas y gastos): solo se ve lo presentado.", ancho, tema::apagado());
            (vec![("presentó", pres)], vacio)
        }
        _ => {
            let que = if t.calc.completo { "Esto es lo que tienes que poner: la suma de tus 303 del año." } else { "Faltan 303 de este año (sin su PDF ni tus facturas y gastos): esto sale a medias." };
            nota(&mut v, que, ancho, Style::new().add_modifier(Modifier::BOLD));
            (vec![("a poner", &t.calc.casillas)], vacio)
        }
    };
    if let Some(p) = &t.pdf {
        v.push(linea_pdf("390", p, "o"));
    }
    v.push(Line::from(""));
    v.extend(tabla(casillas::G390, &cols, &marcadas, &BTreeMap::new(), todas, ancho, false).0);
    v.push(Line::from(""));
    if !t.calc.sin_repartir.is_empty() {
        let lista: Vec<String> = t.calc.sin_repartir.iter().map(|&k| casillas::numero(k)).collect();
        nota(&mut v, &format!("Sin tus gastos de algún trimestre, la app no sabe repartir su IVA por tipos: va en el total ({}). Repártelo tú con tus facturas.", lista.join(", ")), ancho, Style::new().fg(tema::acento()));
    }
    nota(&mut v, "No lo presentan quienes llevan los libros del IVA en la sede de la AEAT (SII) ni quienes solo tienen el régimen simplificado o alquileres.", ancho, tema::apagado());
    if let Some(conc) = &t.conc {
        let cols: Vec<&Casillas> = cols.iter().map(|c| c.1).collect();
        por_que(&mut v, conc, casillas::G390, &cols, ancho, "por qué no cuadra");
    }
    v.push(Line::from(""));
    v.push(titulo_bloque(format!("otras declaraciones de {anio}")));
    for i in &t.informativas {
        sangrada(&mut v, "· ", i, ancho, Style::new());
    }
    v
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
    /// Lo presentado frente a tus datos.
    conc: Option<Conciliacion>,
    /// De dónde sale cada casilla calculada, en pocas palabras.
    de_donde: BTreeMap<u32, String>,
    /// En el 130: el de difícil justificación que la app deja para la Renta (0 si lo resta aquí o no se aplica).
    gdj_fuera: f64,
}

impl Modelo {
    fn nuevo(ctx: &Ctx, hoy: &str, anio: i32, q: u32, modelo: Trimestral, memo: &mut Memo) -> Self {
        let d = &ctx.datos;
        let fin = fmt::periodo(anio, q).1;
        let es130 = modelo == Trimestral::M130;
        let calc = modelo.calcular(d, anio, q, memo);
        let pres = trimestre::presentadas(d, anio, q, modelo.modelo());
        let conc = pres.as_ref().map(|p| autonomo::conciliar(d, modelo, anio, q, &calc, &p.0, memo));
        let gdj_fuera = if es130 && d.fiscal.gdj && !modelo_130::con_gdj(d) { crate::hacienda::irpf::irpf_acumulado(d, anio, &fin).gdj } else { 0.0 };
        Modelo {
            modelo,
            anio,
            q,
            de_donde: autonomo::de_donde(d, modelo, anio, q, &calc),
            calc,
            pres,
            conc,
            gdj_fuera,
            estado: trimestre::estado_modelo(d, hoy, anio, q, modelo),
            actividad: autonomo::hay_datos(d, &modelo.desde(anio, q), &fin),
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

pub(super) fn nota(v: &mut Vec<Line<'static>>, texto: &str, ancho: usize, estilo: Style) {
    // El «€» va con su número.
    v.extend(lineas(&texto.replace(" €", "\u{a0}€"), ancho).into_iter().map(|l| Line::styled(l, estilo)));
}

/// Una nota que empieza por `prefijo` y sigue sangrada en las líneas de debajo.
pub(super) fn sangrada(v: &mut Vec<Line<'static>>, prefijo: &str, texto: &str, ancho: usize, estilo: Style) {
    let n = prefijo.chars().count();
    for (i, l) in lineas(&texto.replace(" €", "\u{a0}€"), ancho.saturating_sub(n).max(10)).into_iter().enumerate() {
        let p = if i == 0 { prefijo.to_string() } else { " ".repeat(n) };
        v.push(Line::from(vec![Span::styled(p, tema::apagado()), Span::styled(l, estilo)]));
    }
}

fn importe_casilla(v: f64, clase: Clase) -> String {
    if clase == Clase::Pct { format!("{} %", fmt::num(v)) } else { fmt::num(v) }
}

/// Las columnas que ocupa un texto.
fn largo(s: &str) -> usize {
    s.chars().count()
}

/// La línea en `ancho` columnas justas: se corta (con «…») o se rellena con espacios.
fn a_medida(l: Line<'static>, ancho: usize) -> Line<'static> {
    let estilo = l.style;
    let mut v: Vec<Span<'static>> = Vec::new();
    let mut usado = 0;
    for s in l.spans {
        let n = largo(&s.content);
        if usado + n <= ancho {
            usado += n;
            v.push(s);
            continue;
        }
        let cabe = ancho - usado;
        if cabe > 0 {
            let t: String = s.content.chars().take(cabe - 1).chain(['…']).collect();
            v.push(Span::styled(t, s.style));
            usado = ancho;
        }
        break;
    }
    v.push(Span::raw(" ".repeat(ancho - usado)));
    Line::from(v).style(estilo)
}

/// Los trozos de una línea con el estilo de la línea dentro de cada uno (para juntar dos líneas en una).
fn trozos(l: Line<'static>) -> Vec<Span<'static>> {
    let base = l.style;
    l.spans.into_iter().map(|s| Span::styled(s.content, base.patch(s.style))).collect()
}

/// Dos columnas de `ancho`, una al lado de la otra y con `HUECO` entre medias; la más corta, con filas en blanco.
fn juntar(v: &mut Vec<Line<'static>>, izq: Vec<Line<'static>>, der: Vec<Line<'static>>, ancho: usize) {
    let n = izq.len().max(der.len());
    let (mut i, mut d) = (izq.into_iter(), der.into_iter());
    for _ in 0..n {
        let mut s = trozos(a_medida(i.next().unwrap_or_default(), ancho));
        s.push(Span::raw(" ".repeat(HUECO)));
        s.extend(trozos(a_medida(d.next().unwrap_or_default(), ancho)));
        v.push(Line::from(s));
    }
}

/// Las dos tablas con las mismas filas: a la más corta se le meten filas en blanco antes de la última. Así el
/// resultado de las dos queda en la misma fila, como en el boceto.
fn igualar(mut a: Vec<Line<'static>>, mut b: Vec<Line<'static>>) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let n = a.len().max(b.len());
    for t in [&mut a, &mut b] {
        if let Some(ultima) = t.pop() {
            t.resize(n - 1, Line::default());
            t.push(ultima);
        }
    }
    (a, b)
}

/// Un sello («▕ CUADRA ▏») y, detrás, una frase en gris (debajo, si no cabe en `ancho`).
fn sello_y(v: &mut Vec<Line<'static>>, texto: &str, color: Color, fondo: Color, detras: &str, ancho: usize) {
    let mut l = sello(texto, color, fondo);
    if detras.is_empty() {
        v.push(l);
    } else if l.width() + 1 + largo(detras) <= ancho {
        l.push_span(Span::styled(format!(" {detras}"), tema::apagado()));
        v.push(l);
    } else {
        v.push(l);
        nota(v, detras, ancho, tema::apagado());
    }
}

/// La banda del resultado, en `ancho` columnas: tres filas de fondo suave con, en la de en medio, qué es («a
/// ingresar») y la cifra, con su último dígito en la columna `fin` (la de la primera columna de la tabla) y el «€»
/// detrás, si cabe. Va con las demás líneas y se desplaza con ellas (`widgets::banda` pinta en un sitio fijo).
fn banda(que: &str, cifra: String, estilo: Style, detras: &str, fin: usize, ancho: usize, fondo: Color) -> Vec<Line<'static>> {
    let fin = if cifra.ends_with(" €") { (fin + 2).min(ancho) } else { fin };
    let hueco = fin.saturating_sub(2 + largo(que) + largo(&cifra)).max(1);
    let mut medio = vec![Span::raw(format!("  {que}")), Span::raw(" ".repeat(hueco)), Span::styled(cifra, estilo.add_modifier(Modifier::BOLD))];
    if !detras.is_empty() {
        medio.push(Span::styled(format!(" {detras}"), tema::apagado()));
    }
    [Line::default(), Line::from(medio), Line::default()]
        .into_iter()
        .map(|l| {
            let mut l = a_medida(l, ancho);
            l.spans.iter_mut().for_each(|s| s.style = s.style.bg(fondo));
            l
        })
        .collect()
}

/// Lo que es el resultado de un modelo para la banda: qué es («a ingresar», «a compensar»…), el importe, su color,
/// lo que va detrás y el fondo (naranja si se paga; verde si sale a devolver o a compensar; neutro si nada).
fn que_sale(m: &Modelo, v: f64) -> (&'static str, String, Style, &'static str, Color) {
    let neutro = tema::suave(tema::MARCO);
    match m.modelo {
        _ if v > DIF => ("a ingresar", fmt::eur(v), Style::new().fg(tema::acento()), "", tema::SUAVE_NARANJA),
        Trimestral::M303 if v < -DIF => (if m.devolver { "a devolver" } else { "a compensar" }, fmt::eur(-v), Style::new().fg(tema::BIEN), "", tema::SUAVE_BIEN),
        Trimestral::M130 if v < -DIF => ("a ingresar", fmt::eur(0.0), tema::apagado(), "(negativa)", neutro),
        _ => ("resultado", fmt::eur(0.0), tema::apagado(), "", neutro),
    }
}

/// Las casillas de un modelo en columnas («a poner», o «tú pones» y «presentó»), sin caja: la cabecera en gris,
/// los grupos con su título en gris y los importes a la derecha. Lo presentado que es igual que lo tuyo va como
/// «✓»; lo que no cuadra (`marcadas`), con el número de la casilla y lo presentado en AVISO. En gris tras el
/// nombre, si cabe entero, de dónde sale cada una (`de_donde`). Cada línea mide `ancho`. Devuelve también la columna en
/// la que acaba la primera de importes (para alinear con ella la cifra de la banda) y lo de `de_donde` que no ha
/// cabido (para decirlo debajo, en una nota).
pub(super) fn tabla(grupos: &[Grupo], cols: &[(&str, &Casillas)], marcadas: &BTreeSet<u32>, de_donde: &BTreeMap<u32, String>, todas: bool, ancho: usize, apagar: bool) -> (Vec<Line<'static>>, usize, Vec<(u32, String)>) {
    let valores: Vec<&Casillas> = cols.iter().map(|c| c.1).collect();
    let compara = cols.len() > 1;
    let vis: Vec<(&str, Vec<&Fila>)> = grupos
        .iter()
        .map(|(t, filas)| (*t, filas.iter().filter(|x| casillas::visible(x, todas, &valores)).collect::<Vec<_>>()))
        .filter(|(_, filas)| !filas.is_empty())
        .collect();
    // Cada columna de importes, tan ancha como lo más largo de todas (o su título, con dos espacios) y uno delante.
    let vw = cols
        .iter()
        .flat_map(|(nombre, c)| vis.iter().flat_map(|(_, filas)| filas.iter()).map(|x| largo(&importe_casilla(c.v(x.n), x.clase))).chain([largo(nombre) + 1]))
        .max()
        .unwrap_or(0)
        + 1;
    // El nombre, con lo que quede (sin pasarse: con mucho sitio, los importes quedarían lejos de su nombre).
    let lw = ancho.saturating_sub(4 + vw * cols.len()).clamp(6, NOMBRE_MAX);
    // Si algún nombre oficial no cabe, todos con su nombre corto (los que lo tienen): así no se mezclan.
    let cortos = vis.iter().flat_map(|(_, filas)| filas.iter()).any(|x| x.corto.is_some() && largo(&casillas::nombre(x, &valores)) > lw);
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let aviso = Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD);
    let (mut v, mut sin_sitio) = (Vec::new(), Vec::new());
    let mut cab = format!("    {}", ajustar("casilla", lw));
    for (nombre, _) in cols {
        cab.push_str(&format!("{nombre:>vw$}"));
    }
    v.push(a_medida(Line::styled(cab, tema::cabecera_tabla()), ancho));
    for (titulo, filas) in vis {
        v.push(a_medida(Line::styled(titulo.to_string(), gris), ancho));
        for x in filas {
            let distinta = marcadas.contains(&x.n) && !apagar;
            let base = if apagar {
                gris
            } else if x.clase == Clase::Total {
                negrita
            } else {
                Style::new()
            };
            let mut s = vec![Span::styled(format!("{:<3} ", casillas::numero(x.n)), if distinta { aviso } else { gris })];
            let nombre = if cortos { casillas::nombre_corto(x, &valores) } else { casillas::nombre(x, &valores) };
            let n = largo(&nombre);
            // De dónde sale, solo si cabe entero.
            match de_donde.get(&x.n).filter(|de| n + 3 + largo(de) <= lw) {
                Some(de) => {
                    s.push(Span::styled(nombre, base));
                    s.push(Span::styled(ajustar(&format!(" · {de}"), lw - n), gris));
                }
                None => {
                    if let Some(de) = de_donde.get(&x.n) {
                        sin_sitio.push((x.n, de.clone()));
                    }
                    s.push(Span::styled(ajustar(&nombre, lw), base));
                }
            }
            for (i, c) in valores.iter().enumerate() {
                let val = c.v(x.n);
                let igual = compara && i > 0 && !distinta && (val - valores[0].v(x.n)).abs() <= DIF;
                let (txt, st) = if igual {
                    ("✓".to_string(), if apagar { gris } else { Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD) })
                } else if distinta {
                    // Lo tuyo en negrita; lo presentado, en AVISO.
                    (importe_casilla(val, x.clase), if compara && i == 0 { negrita } else { aviso })
                } else if val.abs() <= DIF && x.clase != Clase::Pct {
                    (importe_casilla(val, x.clase), gris)
                } else {
                    (importe_casilla(val, x.clase), base)
                };
                s.push(Span::styled(format!("{txt:>vw$}"), st));
            }
            v.push(a_medida(Line::from(s), ancho));
        }
    }
    (v, (4 + lw + vw).min(ancho), sin_sitio)
}

/// El estado de un modelo en pocas palabras: «presentado el 15 oct a las 09:05», «en plazo hasta el 20 oct ·
/// quedan 3 días», «plazo terminado el 20 jul».
fn frase_estado(m: &Modelo) -> String {
    let p = trimestre::plazo(m.anio, m.q);
    let dia = |f: &str| if fmt::anio_de(f) == m.anio { trimestre::dia_mes(f) } else { fmt::fecha_corta(f) };
    let plazo = trimestre::del_al(&p.ini, &p.fin, m.anio);
    match &m.estado {
        Estado::Presentado(f) => {
            let hora = m.hacienda.as_deref().and_then(|h| h.get(11..16)).map(|h| format!(" a las {h}")).unwrap_or_default();
            if f.is_empty() { "presentado".into() } else { format!("presentado el {}{hora}", dia(f)) }
        }
        Estado::NoToca("exento") => "no tienes que presentarlo".into(),
        Estado::NoToca(motivo) => format!("no toca: {motivo}"),
        Estado::Futuro => format!("aún no: se presenta {plazo}"),
        Estado::EnCurso => format!("en curso: se presenta {plazo}"),
        Estado::Proximo => format!("se presenta {plazo}"),
        Estado::EnPlazo(n) => format!("en plazo hasta el {} · {}", dia(&p.fin), trimestre::dias_txt(*n)),
        Estado::Vencido => format!("plazo terminado el {}", dia(&p.fin)),
        Estado::SinDatos => format!("plazo terminado el {} · sin datos en la app (¿lo presentaste fuera?)", dia(&p.fin)),
    }
}

/// «PDF del 303: 2026_3T_303.pdf · o para abrirlo», con la tecla en negrita.
fn linea_pdf(modelo: &str, p: &std::path::Path, tecla: &str) -> Line<'static> {
    let nombre = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let gris = tema::apagado();
    let mut l = Line::from(vec![Span::styled(format!("PDF del {modelo}: "), gris), Span::raw(nombre), Span::styled(" · ", gris)]);
    for s in atajo(tecla, "para abrirlo") {
        l.push_span(s);
    }
    l
}

/// Un modelo del trimestre, por partes: para ponerlo al lado del otro (las tres primeras, en una columna de
/// `ancho`) o debajo de él.
struct PorPartes {
    /// «IVA · modelo 303».
    titulo: Line<'static>,
    /// La banda con el resultado y, debajo, si cuadra con lo presentado o cómo va (en plazo, vencido…).
    resumen: Vec<Line<'static>>,
    /// Las casillas.
    tabla: Vec<Line<'static>>,
    /// Por qué no cuadra con lo presentado, en `ancho_notas` columnas.
    porque: Vec<Line<'static>>,
    /// Lo que conviene saber, en `ancho_notas` columnas.
    notas: Vec<Line<'static>>,
}

/// Todo lo de un modelo: el resultado, si cuadra con lo presentado, las casillas y las notas.
fn por_partes(m: &Modelo, d: &Datos, ancho: usize, ancho_notas: usize, todas: bool) -> PorPartes {
    let es130 = m.modelo == Trimestral::M130;
    let codigo = m.modelo.modelo().codigo();
    let tecla = if es130 { "O" } else { "o" };
    let presentado = matches!(m.estado, Estado::Presentado(_));
    let titulo = if es130 && ancho >= 40 { "IRPF · modelo 130 · pago fraccionado" } else if es130 { "IRPF · modelo 130" } else { "IVA · modelo 303" };
    let titulo = Line::styled(titulo, tema::apagado());
    let (mut resumen, mut notas) = (Vec::new(), Vec::new());
    let estado = frase_estado(m);
    let pdf = match &m.pdf {
        Some(p) => Some(a_medida(linea_pdf(codigo, p, tecla), ancho_notas)),
        None if presentado => Some(Line::styled(format!("{codigo}: sin PDF en justificantes/"), tema::apagado())),
        None => None,
    };

    // Las líneas con un IVA que no es de hoy: no están en este 303.
    if !es130 {
        let (desde, hasta) = fmt::periodo(m.anio, m.q);
        for (f, t) in modelo_303::fuera(d, &desde, &hasta) {
            let t = format!("La factura {} lleva una línea con un IVA del {}: no está en este 303, porque la app solo lo calcula con los tipos de hoy (0, 4, 10 y 21 %). Si es correcto (el 5 % lo hubo hasta el 30/09/2024), súmala tú en su casilla; si no, corrige su IVA en Facturas.", f.numero, fmt::pct(t));
            nota(&mut notas, &t, ancho_notas, Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD));
        }
    }

    // Un año cerrado sin facturas ni gastos de este periodo: solo lo presentado (si se tienen sus casillas).
    if m.solo_presentado() && m.pres.is_none() {
        let t = if es130 && m.renta_sin130 && !presentado {
            format!("Tu Renta de {} no tiene pagos fraccionados: no presentaste el 130.", m.anio)
        } else if !presentado {
            format!("{} está cerrado y en la app no hay facturas ni gastos de este periodo: no hay nada que calcular.", m.anio)
        } else if m.pdf.is_some() {
            format!("Presentado. Sus casillas aún no se han leído del PDF: ábrelo con {tecla}.")
        } else {
            "Presentado, pero sin su PDF en justificantes/ no sé con qué casillas.".to_string()
        };
        nota(&mut resumen, &estado, ancho, m.estado.estilo());
        nota(&mut resumen, &t, ancho, tema::apagado());
        notas.extend(pdf);
        return PorPartes { titulo, resumen, tabla: Vec::new(), porque: Vec::new(), notas };
    }

    // El 130: ¿hay que presentarlo?
    let mut apagar = false;
    if es130 && !presentado {
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
            nota(&mut resumen, &format!("No estás obligado a presentar el 130 este trimestre. {porque}"), ancho, Style::new().fg(tema::BIEN));
        } else if ex.sugerido == Some(true) && (modo.is_empty() || modo == "auto") {
            let t = format!(
                "Este trimestre el {} de tus ingresos lleva retención. Si en {} también fue así (70 % o más), no tienes que presentar el 130. No tengo tus datos de {} para comprobarlo.",
                fmt::pct(ex.pct.round()),
                m.anio - 1,
                m.anio - 1
            );
            nota(&mut notas, &t, ancho_notas, Style::new().fg(tema::acento()));
        }
    }

    // Lo presentado frente a tus datos, o lo que tienes que poner.
    let (cols, marcadas): (Vec<(&str, &Casillas)>, BTreeSet<u32>) = match (&m.pres, &m.conc) {
        (Some((c, _)), Some(conc)) => {
            let marcadas = conc.diferencias.iter().map(|x| x.casilla).chain(conc.arrastradas.iter().copied()).collect();
            if conc.con_datos { (vec![("tú pones", &m.calc), ("presentó", c)], marcadas) } else { (vec![("presentó", c)], marcadas) }
        }
        _ => (vec![(if apagar { "calculado" } else { "a poner" }, &m.calc)], BTreeSet::new()),
    };
    // De dónde sale cada casilla, si se ve lo de tus datos.
    let sin_origen = BTreeMap::new();
    let de_donde = if cols.len() == 1 && cols[0].0 == "presentó" { &sin_origen } else { &m.de_donde };
    let (tabla, fin, sin_sitio) = tabla(casillas::grupos(m.modelo), &cols, &marcadas, de_donde, todas, ancho, apagar);
    // De dónde sale lo que no ha cabido tras su nombre: «07 y 09: 2 facturas · 28 y 29: 1 gasto».
    if !sin_sitio.is_empty() {
        let mut grupos: Vec<(Vec<String>, String)> = Vec::new();
        for (k, de) in sin_sitio {
            match grupos.iter_mut().find(|g| g.1 == de) {
                Some(g) => g.0.push(casillas::numero(k)),
                None => grupos.push((vec![casillas::numero(k)], de)),
            }
        }
        let partes: Vec<String> = grupos.iter().map(|(ks, de)| format!("{}: {de}", trimestre::y_lista(&ks.iter().map(String::as_str).collect::<Vec<_>>()))).collect();
        nota(&mut notas, &format!("De dónde sale cada casilla del {codigo} · {}.", partes.join(" · ")), ancho_notas, tema::apagado());
    }

    // La banda: lo que sale con tus datos (o lo presentado, si es lo único que hay). Ni en lo que no ha empezado
    // ni en el 130 que no hay que presentar.
    let vc = m.modelo.resultado_de(&m.calc);
    let vp = m.pres.as_ref().map(|p| m.modelo.resultado_de(&p.0));
    let solo_lo_presentado = m.conc.as_ref().is_some_and(|c| !c.con_datos);
    if !apagar && !matches!(m.estado, Estado::Futuro) {
        let (que, cifra, st, detras, fondo) = que_sale(m, if solo_lo_presentado { vp.unwrap_or(vc) } else { vc });
        resumen.extend(banda(que, cifra, st, detras, fin, ancho, fondo));
    }

    // Debajo, si cuadra con lo presentado; o cómo va.
    match &m.conc {
        Some(conc) if conc.con_datos => {
            let vp = vp.unwrap_or(vc);
            if conc.cuadra() {
                sello_y(&mut resumen, "CUADRA", tema::BIEN, tema::SUAVE_BIEN, &estado, ancho);
            } else if !conc.mismo_resultado {
                // Cuánto de más o de menos sale a pagar (en el 130, lo negativo es pagar 0).
                let pago = |v: f64| if es130 { v.max(0.0) } else { v };
                let dif = fmt::r2(pago(vp) - pago(vc));
                let cuanto = if vp > DIF { fmt::eur(vp) } else { m.texto(vp) };
                if dif.abs() > DIF {
                    let que = format!("{} DE {}", fmt::eur(dif.abs()), if dif > 0.0 { "MÁS" } else { "MENOS" });
                    sello_y(&mut resumen, &que, tema::AVISO, tema::SUAVE_AVISO, &format!("presentaste {cuanto}"), ancho);
                } else {
                    sello_y(&mut resumen, "NO CUADRA", tema::AVISO, tema::SUAVE_AVISO, &format!("presentaste {cuanto}"), ancho);
                }
                nota(&mut resumen, &estado, ancho, tema::apagado());
            } else {
                let n = conc.distintas();
                let que = if n == 1 { "pero una casilla es distinta".to_string() } else { format!("pero {n} casillas son distintas") };
                sello_y(&mut resumen, "MISMO RESULTADO", tema::acento(), tema::suave(tema::acento()), &que, ancho);
                nota(&mut resumen, &estado, ancho, tema::apagado());
            }
        }
        Some(conc) => {
            if !conc.cuadra() {
                sello_y(&mut resumen, "NO CUADRA", tema::AVISO, tema::SUAVE_AVISO, "lo que viene de antes", ancho);
            }
            nota(&mut resumen, &estado, ancho, m.estado.estilo());
            let t = if conc.cuadra() {
                "En la app no hay facturas ni gastos de este periodo: solo se ve lo presentado."
            } else {
                "En la app no hay facturas ni gastos de este periodo, pero lo que viene de los trimestres anteriores no cuadra."
            };
            nota(&mut notas, t, ancho_notas, tema::apagado());
        }
        None => {
            if !apagar {
                nota(&mut resumen, &estado, ancho, m.estado.estilo());
            }
            if matches!(m.estado, Estado::Futuro) {
                nota(&mut resumen, "Este trimestre todavía no ha empezado.", ancho, tema::apagado());
            }
            if presentado {
                nota(&mut notas, "No tengo las casillas con las que se presentó: se ve lo que sale con tus datos.", ancho_notas, tema::apagado());
            }
        }
    }

    // Lo que sale del resultado.
    let c = m.cuenta();
    let r = c.v(m.k());
    if m.modelo == Trimestral::M303 {
        let (c110, c78, c87) = (c.v(110), c.v(78), c.v(87));
        let compensa = r < -DIF && !m.devolver;
        if c110.abs() > DIF || c78.abs() > DIF || c87.abs() > DIF || compensa {
            let de = if m.pres.is_some() { ", según lo presentado" } else { "" };
            let mut t = format!("Compensación{de}: pendiente de antes {} (110) → aplicado ahora {} (78) → queda {} (87).", fmt::eur(c110), fmt::eur(c78), fmt::eur(c87));
            if compensa {
                t.push_str(&format!(" El trimestre siguiente podrás compensar {}.", fmt::eur(fmt::r2(c87 - r))));
            }
            nota(&mut notas, &t, ancho_notas, tema::apagado());
        }
        // En el 4T se elige: devolver o compensar.
        if m.q == 4 && r < -DIF && m.devolver && !presentado {
            nota(&mut notas, "En el 4T, devolver es opcional: la app cuenta con que pides la devolución, pero también puedes dejarlo a compensar en los 303 del año que viene.", ancho_notas, tema::apagado());
        }
    } else {
        let rn = &m.rn;
        let ant = m.anio - 1;
        let origen = match rn.origen {
            OrigenRn::Escrito => format!("has indicado un rendimiento de {} en {ant}.", fmt::eur(rn.valor.unwrap_or(0.0))),
            OrigenRn::Renta => format!("tu Renta de {ant} dice {} (casilla 0224).", fmt::eur(rn.valor.unwrap_or(0.0))),
            OrigenRn::Alta => format!("empezaste la actividad en {}: el rendimiento del año anterior cuenta como 0.", m.anio),
            OrigenRn::Calculado => format!("calculado con tus datos de {ant}: {}.", fmt::eur(rn.valor.unwrap_or(0.0))),
            OrigenRn::NoSe => format!("no sé cuánto ganaste en {ant} (casilla 0224 de tu Renta)."),
        };
        let t = format!("Casilla 13 (minoración): hasta 100 € por trimestre si en {ant} ganaste 12.000 € o menos. Para la app, {origen}");
        nota(&mut notas, &t, ancho_notas, tema::apagado());
        if m.gdj_fuera > DIF {
            let t = format!(
                "El {} de difícil justificación ({}) la app lo deja para la Renta. Las instrucciones de la casilla 02 dicen que en la estimación directa simplificada va también aquí: con él pagarías unos {} menos ahora (al final del año, lo mismo).",
                fmt::pct(crate::hacienda::irpf::gdj_reglas(m.anio).0),
                fmt::eur(m.gdj_fuera),
                fmt::eur(fmt::r2(m.gdj_fuera * 0.2))
            );
            nota(&mut notas, &t, ancho_notas, tema::apagado());
        }
    }
    notas.extend(pdf);
    let mut porque = Vec::new();
    if let Some(conc) = &m.conc {
        let cols: Vec<&Casillas> = [Some(&m.calc), m.pres.as_ref().map(|p| &p.0)].into_iter().flatten().collect();
        por_que(&mut porque, conc, casillas::grupos(m.modelo), &cols, ancho_notas, &format!("por qué no cuadra el {codigo}"));
    }
    PorPartes { titulo, resumen, tabla, porque, notas }
}

/// Tras `fila` (las pestañas), las `piezas` separadas por « · », en filas de `ancho`: lo que no cabe pasa a la fila
/// de debajo, con `sangria` espacios delante.
fn repartir(mut fila: Vec<Span<'static>>, piezas: Vec<Vec<Span<'static>>>, sangria: usize, ancho: usize) -> Vec<Line<'static>> {
    let ancho_de = |v: &[Span]| v.iter().map(Span::width).sum::<usize>();
    let mut filas = Vec::new();
    for (i, p) in piezas.into_iter().enumerate() {
        let (usado, w) = (ancho_de(&fila), ancho_de(&p));
        if i == 0 && usado + w <= ancho {
            fila.extend(p);
        } else if i > 0 && usado + 3 + w <= ancho {
            fila.push(Span::styled(" · ", tema::apagado()));
            fila.extend(p);
        } else {
            filas.push(Line::from(std::mem::replace(&mut fila, vec![Span::raw(" ".repeat(sangria))])));
            fila.extend(p);
        }
    }
    filas.push(Line::from(fila));
    filas
}

/// «solo las casillas con algo · t todas» (o al revés, si se ven todas).
fn pista(todas: bool) -> Line<'static> {
    let (que, otra) = if todas { ("también las casillas a cero", "solo con valor") } else { ("solo las casillas con algo", "todas") };
    let mut l = Line::from(Span::styled(format!("{que} · "), tema::apagado()));
    for s in atajo("t", otra) {
        l.push_span(s);
    }
    l
}

/// El 303 y el 130 del trimestre: uno al lado del otro si caben (`ANCHO_DOBLE`) y, si no, uno debajo del otro.
/// Después, por qué no cuadran y lo que conviene saber. (La fila del año la pone `pintar_doc`.)
fn documento(t: &DelTrimestre, d: &Datos, ancho: usize, todas: bool) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    let origen = [&t.m303, &t.m130].iter().find_map(|m| m.pres.as_ref().map(|p| p.1));
    if ancho >= ANCHO_DOBLE as usize {
        let w = (ancho - HUECO) / 2;
        let (a, b) = (por_partes(&t.m303, d, w, ancho, todas), por_partes(&t.m130, d, w, ancho, todas));
        juntar(&mut v, vec![a.titulo], vec![b.titulo], w);
        juntar(&mut v, a.resumen, b.resumen, w);
        let hay_tablas = !a.tabla.is_empty() || !b.tabla.is_empty();
        if hay_tablas {
            v.push(Line::default());
            let (ta, tb) = igualar(a.tabla, b.tabla);
            juntar(&mut v, ta, tb, w);
            v.push(Line::default());
            v.push(pista(todas));
        }
        if let Some(o) = origen {
            nota(&mut v, o, ancho, tema::apagado());
        }
        // Primero por qué no cuadran (ya con su fila en blanco delante); luego, lo demás.
        v.extend(a.porque);
        v.extend(b.porque);
        for notas in [a.notas, b.notas].into_iter().filter(|n| !n.is_empty()) {
            v.push(Line::default());
            v.extend(notas);
        }
    } else {
        let mut hay_tablas = false;
        for (i, m) in [&t.m303, &t.m130].into_iter().enumerate() {
            let b = por_partes(m, d, ancho, ancho, todas);
            hay_tablas |= !b.tabla.is_empty();
            if i > 0 {
                v.push(Line::default());
            }
            v.push(b.titulo);
            v.extend(b.resumen);
            if !b.tabla.is_empty() {
                v.push(Line::default());
                v.extend(b.tabla);
            }
            v.extend(b.porque);
            if !b.notas.is_empty() {
                v.push(Line::default());
                v.extend(b.notas);
            }
        }
        if hay_tablas {
            v.push(Line::default());
            v.push(pista(todas));
        }
        if let Some(o) = origen {
            nota(&mut v, o, ancho, tema::apagado());
        }
    }
    v
}

/// Cómo va cada trimestre del año, en una fila (o en más, si no cabe): «1T ✓     2T ≠     3T 303 vencido     4T
/// ahora saldrían 493,50 € + 50,00 €». Con `res`, los resultados de cada trimestre (de la pestaña «Año»).
fn fila_anio(cab: &Cabecera, res: &[(Option<f64>, Option<f64>, bool)], y: i32, ancho: usize) -> Vec<Line<'static>> {
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut grupos: Vec<Vec<Span<'static>>> = Vec::new();
    for q in 1..=4u32 {
        let i = q as usize - 1;
        let [e303, e130] = &cab.estados[i];
        let mut g = vec![Span::raw(format!("{q}T "))];
        let pendientes: Vec<&Estado> = [e303, e130].into_iter().filter(|e| !e.hecho()).collect();
        if cab.no_cuadra[i] {
            g.push(Span::styled("≠", Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)));
        } else if pendientes.is_empty() {
            if [e303, e130].iter().any(|e| matches!(e, Estado::Presentado(_))) {
                g.push(Span::styled("✓", Style::new().fg(tema::BIEN).add_modifier(Modifier::BOLD)));
            } else {
                g.push(Span::styled(e303.corto(), gris));
            }
        } else if pendientes.iter().all(|e| matches!(e, Estado::EnCurso | Estado::Proximo | Estado::EnPlazo(_))) {
            // Lo de ahora: lo que saldría con tus datos de hoy («ahora saldrían 559,70 € + 183,60 €»; si alguno no
            // sale a ingresar, cada uno con lo suyo: «ahora: 303 630,00 € a compensar · 130 0 €»).
            let (r303, r130, devolver) = res.get(i).copied().unwrap_or((None, None, false));
            let mut partes = Vec::new();
            if let (false, Some(r)) = (e303.hecho(), r303) {
                partes.push(("303", r, trimestre::texto303(r, devolver, false)));
            }
            if let (false, Some(r)) = (e130.hecho(), r130) {
                partes.push(("130", r, trimestre::texto130(r, false)));
            }
            let a_ingresar = partes.iter().all(|p| p.1 > DIF);
            if partes.is_empty() {
                g.push(Span::styled(pendientes[0].corto(), pendientes[0].estilo()));
            } else {
                g.push(Span::styled(if a_ingresar { "ahora saldrían " } else { "ahora: " }, gris));
                for (k, (m, r, t)) in partes.into_iter().enumerate() {
                    if k > 0 {
                        g.push(Span::styled(if a_ingresar { " + " } else { " · " }, gris));
                    }
                    if a_ingresar {
                        g.push(Span::styled(fmt::eur(r), negrita));
                    } else {
                        g.push(Span::styled(format!("{m} "), gris));
                        g.push(Span::styled(t, negrita));
                    }
                }
            }
        } else {
            let (t, st) = trimestre::estado_conjunto(e303, e130, &trimestre::plazo(y, q), true);
            g.push(Span::styled(t, st));
        }
        grupos.push(g);
    }
    // En filas de `ancho`, con cinco espacios entre trimestres (tres, si no caben así con sitio detrás para el
    // «↓ más»).
    let ancho_de = |g: &[Span]| g.iter().map(|s| s.width()).sum::<usize>();
    let total: usize = grupos.iter().map(|g| ancho_de(g)).sum();
    let sep = if total + 15 + MAS <= ancho { 5 } else { 3 };
    let mut v: Vec<Line<'static>> = Vec::new();
    let mut fila: Vec<Span<'static>> = Vec::new();
    for g in grupos {
        let usado = ancho_de(&fila);
        if !fila.is_empty() && usado + sep + ancho_de(&g) > ancho {
            v.push(Line::from(std::mem::take(&mut fila)));
        } else if !fila.is_empty() {
            fila.push(Span::raw(" ".repeat(sep)));
        }
        fila.extend(g);
    }
    v.push(Line::from(fila));
    v
}

/// «Por qué no cuadra»: cada casilla de origen que no cuadra, con sus causas (sin repetir las de la anterior), y las
/// sumas que no cuadran por ellas.
pub(super) fn por_que(v: &mut Vec<Line<'static>>, conc: &Conciliacion, grupos: &[Grupo], cols: &[&Casillas], ancho: usize, titulo: &str) {
    if conc.diferencias.is_empty() {
        return;
    }
    v.push(Line::from(""));
    v.push(titulo_bloque(titulo));
    let mut antes: Option<&autonomo::Diferencia> = None;
    for x in &conc.diferencias {
        let t = format!("{} {}: presentado {} · con tus datos {}", casillas::numero(x.casilla), casillas::nombre_de(grupos, x.casilla, cols), fmt::num(x.presentado), fmt::num(x.app));
        nota(v, &t, ancho, Style::new().fg(tema::AVISO));
        match antes.filter(|a| a.causas == x.causas) {
            // La cuota de la misma factura, o la otra casilla del mismo cambio: no se repite.
            Some(a) => sangrada(v, "  → ", &format!("Lo mismo que en la {}.", casillas::numero(a.casilla)), ancho, tema::apagado()),
            None => {
                x.causas.iter().for_each(|c| sangrada(v, "  → ", c, ancho, Style::new()));
                antes = Some(x);
            }
        }
    }
    if !conc.arrastradas.is_empty() {
        let lista: Vec<String> = conc.arrastradas.iter().map(|&k| casillas::numero(k)).collect();
        nota(v, &format!("Las demás que no cuadran ({}) son sumas y restas de estas.", lista.join(", ")), ancho, tema::apagado());
    }
}

impl Impuestos {
    /// ¿Se ve un trimestre? (ni el año ni el 390)
    fn trimestral(&self) -> bool {
        !self.anual && !self.v390
    }

    /// Al entrar por primera vez o con otro año, se ve el año entero.
    fn elegir_pestana(&mut self, ctx: &Ctx) {
        if self.anio != ctx.anio {
            self.anual = true;
            self.v390 = false;
            self.q = 4;
            self.anio = ctx.anio;
            self.scroll = 0;
        }
    }

    /// Rehace lo calculado que haya cambiado (datos, año, trimestre o día).
    fn preparar(&mut self, ctx: &Ctx) {
        self.elegir_pestana(ctx);
        let hoy = fmt::hoy();
        let k = clave(ctx, 0);
        if self.cab.as_ref().is_none_or(|c| c.clave != k) {
            self.cab = Some(Cabecera::nueva(ctx, &hoy, k.clone()));
            self.contar();
        }
        if self.v390 {
            let k = clave(ctx, 0);
            if self.del390.as_ref().is_none_or(|c| c.clave != k) {
                self.del390 = Some(Del390::nuevo(ctx, k));
                self.contar();
            }
        }
        if self.trimestral() {
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

    /// Lo que va arriba tras las pestañas, en trozos (si no caben, se parten entre ellos): el año y el periodo, el
    /// plazo, hasta cuándo se puede domiciliar y cuántas facturas y gastos hay; en el 390, si se presentó.
    fn contexto(&self, ctx: &Ctx) -> Vec<Vec<Span<'static>>> {
        let y = ctx.anio;
        let gris = tema::apagado();
        let anio = Span::styled(y.to_string(), Style::new().add_modifier(Modifier::BOLD));
        if self.anual {
            let que = if y == fmt::anio_actual() {
                "hasta hoy"
            } else if autonomo::cerrado(&ctx.datos, y) {
                "cerrado: se ve con lo que presentaste"
            } else {
                return vec![vec![anio]];
            };
            return vec![vec![anio], vec![Span::styled(que, gris)]];
        }
        if self.v390 {
            let p = trimestre::plazo(y, 4);
            let estado = match self.del390.as_ref().and_then(|t| t.presentado.as_ref()) {
                Some(f) if f.is_empty() => Span::styled("presentado", Style::new().fg(tema::BIEN)),
                Some(f) => Span::styled(format!("presentado el {}", fmt::fecha_corta(f)), Style::new().fg(tema::BIEN)),
                None => Span::styled(format!("se presenta con el 303 del 4T, {}", trimestre::del_al(&p.ini, &p.fin, y)), gris),
            };
            return vec![vec![anio, Span::styled(" · modelo 390, resumen anual del IVA", gris)], vec![estado]];
        }
        let (ini, fin) = fmt::periodo(y, self.q);
        let p = trimestre::plazo(y, self.q);
        let dia = |f: &str| if fmt::anio_de(f) == y { trimestre::dia_mes(f) } else { fmt::fecha_corta(f) };
        let mut v = vec![
            vec![anio, Span::styled(format!(" · {} – {}", trimestre::dia_mes(&ini), trimestre::dia_mes(&fin)), gris)],
            vec![Span::styled(format!("plazo {}", trimestre::del_al(&p.ini, &p.fin, y)), gris)],
            vec![Span::styled(format!("domiciliar hasta el {}", dia(&p.domiciliar)), gris)],
        ];
        if let Some(cab) = &self.cab {
            let (nf, ng) = cab.cuantos[self.q as usize - 1];
            v.push(vec![Span::styled(format!("{nf} factura{} · {ng} gasto{}", if nf == 1 { "" } else { "s" }, if ng == 1 { "" } else { "s" }), gris)]);
        }
        v
    }

    /// Arriba, las pestañas («1T 2T 3T 4T año 390», la elegida en FONDO sobre el color de la sección) y, tras
    /// ellas, `piezas` separadas por « · »; lo que no cabe, en la fila de debajo. Devuelve las filas que ocupa.
    fn cabecera(&self, f: &mut Frame, area: Rect, piezas: Vec<Vec<Span<'static>>>) -> u16 {
        let gris = tema::apagado();
        let mut fila: Vec<Span<'static>> = Vec::new();
        let pestanas = (1..=4u32).map(|q| (format!("{q}T"), q == self.q && self.trimestral())).chain([("año".to_string(), self.anual), ("390".to_string(), self.v390)]);
        for (i, (t, activa)) in pestanas.enumerate() {
            // Todas con su hueco a cada lado (la primera, sin el de la izquierda: empieza en la columna del
            // contenido): al cambiar de pestaña no se mueve nada.
            let t = if i == 0 { format!("{t} ") } else { format!("  {t} ") };
            let st = if activa { tema::pestana_activa() } else { gris };
            match t.strip_prefix(' ') {
                Some(resto) => fila.extend([Span::raw(" "), Span::styled(resto.to_string(), st)]),
                None => fila.push(Span::styled(t, st)),
            }
        }
        fila.push(Span::raw("   "));
        // Lo que pasa a la fila de debajo va bajo lo de arriba; si así no cabe en dos filas, desde el principio.
        let ancho = area.width as usize;
        let x0 = fila.iter().map(Span::width).sum::<usize>();
        let mut filas = repartir(fila.clone(), piezas.clone(), x0, ancho);
        if filas.len() > 2 || filas.iter().any(|l| l.width() > ancho) {
            filas = repartir(fila, piezas, 0, ancho);
        }
        let n = (filas.len() as u16).min(area.height);
        f.render_widget(Paragraph::new(filas), Rect { height: n, ..area });
        n
    }

    /// Pinta unas líneas que se desplazan con ↑ ↓ y, detrás, el `pie` (la fila del año). Si no cabe todo, la
    /// última fila dice que hay más y lleva el pie (si es de una fila y cabe; si no, va con lo demás). Devuelve
    /// cuánto se puede bajar como mucho.
    fn pintar_doc(&self, f: &mut Frame, area: Rect, mut lineas: Vec<Line<'static>>, pie: Vec<Line<'static>>) -> u16 {
        let (h, w) = (area.height as usize, area.width as usize);
        let cabe_todo = lineas.len() + 1 + pie.len() <= h;
        let pie_fijo = !cabe_todo && h > 1 && pie.len() == 1 && pie[0].width() + MAS <= w;
        if !pie_fijo && !pie.is_empty() {
            lineas.push(Line::default());
            lineas.extend(pie.iter().cloned());
        }
        let total = lineas.len() as u16;
        let (alto, max) = if pie_fijo || total > area.height {
            let alto = area.height.saturating_sub(1);
            (alto, total.saturating_sub(alto))
        } else {
            (area.height, 0)
        };
        let scroll = self.scroll.min(max);
        f.render_widget(Paragraph::new(lineas).scroll((scroll, 0)), Rect { height: alto, ..area });
        if alto < area.height {
            let fila = Rect { y: area.y + alto, height: 1, ..area };
            if pie_fijo {
                f.render_widget(Paragraph::new(pie[0].clone()), fila);
            }
            if max > 0 {
                let t = if scroll < max { "↓ más" } else { "↑" };
                f.render_widget(Paragraph::new(Line::styled(t, tema::apagado()).right_aligned()), fila);
            }
        }
        max
    }
}

impl Pantalla for Impuestos {
    fn titulo(&self) -> &'static str {
        "Impuestos"
    }

    fn al_entrar(&mut self, ctx: &mut Ctx) {
        self.elegir_pestana(ctx);
        // Al entrar se recalcula siempre (por si los datos cambiaron sin pasar por `guardar`).
        self.cab = None;
        self.trim = None;
        self.del390 = None;
        self.resumen.olvidar();
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.preparar(ctx);
        let y = ctx.anio;
        let piezas = self.contexto(ctx);
        let alto = self.cabecera(f, area, piezas);
        // Debajo de las pestañas, una fila de aire.
        let quita = (alto + 1).min(area.height);
        let cuerpo = Rect { y: area.y + quita, height: area.height - quita, ..area };
        if self.anual {
            self.pdf303 = false;
            self.pdf130 = false;
            self.resumen.dibujar(f, cuerpo, ctx);
            return;
        }
        if self.v390 {
            let Some(t) = self.del390.take() else { return };
            self.pdf303 = t.pdf.is_some();
            self.pdf130 = false;
            let max = self.pintar_doc(f, cuerpo, doc390(&t, y, cuerpo.width as usize, self.todas), Vec::new());
            self.del390 = Some(t);
            self.scroll = self.scroll.min(max);
            return;
        }
        // Los resultados de cada trimestre, para la fila del año (los de la pestaña «Año»).
        let res = self.resumen.resultados(ctx);
        let Some(t) = self.trim.take() else { return };
        self.pdf303 = t.m303.pdf.is_some();
        self.pdf130 = t.m130.pdf.is_some();
        let ancho = cuerpo.width as usize;
        let anio = self.cab.as_ref().map(|c| fila_anio(c, &res, y, ancho)).unwrap_or_default();
        let max = self.pintar_doc(f, cuerpo, documento(&t, &ctx.datos, ancho, self.todas), anio);
        self.trim = Some(t);
        self.scroll = self.scroll.min(max);
    }

    fn tecla(&mut self, k: KeyEvent, ctx: &mut Ctx) -> bool {
        self.elegir_pestana(ctx);
        match k.code {
            // Las pestañas, en orden: 1T 2T 3T 4T Año 390.
            KeyCode::Left | KeyCode::Char('h') => {
                if self.v390 {
                    (self.v390, self.anual) = (false, true);
                } else if self.anual {
                    self.anual = false;
                } else {
                    self.q = self.q.saturating_sub(1).max(1);
                }
                self.scroll = 0;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.anual {
                    (self.anual, self.v390) = (false, true);
                } else if self.v390 {
                } else if self.q >= 4 {
                    self.anual = true;
                } else {
                    self.q += 1;
                }
                self.scroll = 0;
            }
            _ if self.anual && !matches!(k.code, KeyCode::Char('o' | 'O')) => return false,
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Char('t') => {
                self.todas = !self.todas;
                ctx.info(if self.todas { "Se ven también las casillas a cero." } else { "Solo las casillas con algo." });
            }
            KeyCode::Char(c @ ('o' | 'O')) => {
                let modelo = if c == 'o' { Trimestral::M303 } else { Trimestral::M130 };
                if self.v390 {
                    match self.del390.as_ref().and_then(|t| t.pdf.clone()) {
                        Some(p) => ctx.abrir_ruta(&p),
                        None => ctx.elegir_pdf(ctx.anio),
                    }
                    return true;
                }
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
        if self.anual {
            return vec![("← →", "trimestres y 390")];
        }
        if self.v390 {
            let mut v = vec![("←", "año"), ("↑↓", "bajar"), ("t", if self.todas { "solo con valor" } else { "todas" })];
            if self.pdf303 {
                v.push(("o", "PDF 390"));
            }
            return v;
        }
        let mut v = vec![("← →", "trimestre"), ("↑↓", "bajar"), ("t", if self.todas { "solo con valor" } else { "todas" })];
        if self.pdf303 {
            v.push(("o", "PDF 303"));
        }
        if self.pdf130 {
            v.push(("O", "PDF 130"));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una factura con un IVA que no es de hoy: el 303 lo dice.
    #[test]
    fn el_303_dice_lo_que_no_cuenta() {
        use crate::dominio::{Factura, Linea, Num};
        let mut d = Datos::default();
        let linea = Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(5.0) };
        d.facturas.push(Factura { estado: "emitida".into(), numero: "2026-001".into(), fecha: "2026-02-01".into(), operacion: "nacional".into(), lineas: vec![linea], ..Default::default() });
        let app = super::super::prueba::app_con(d, std::path::Path::new("."));
        let m = Modelo::nuevo(&app.ctx, "2026-03-01", 2026, 1, Trimestral::M303, &mut Memo::default());
        let b = por_partes(&m, &app.ctx.datos, 100, 100, false);
        let texto: String = b.resumen.iter().chain(&b.tabla).chain(&b.porque).chain(&b.notas).map(|l| l.to_string() + "\n").collect();
        assert!(texto.contains("La factura 2026-001 lleva una línea con un IVA del 5"), "{texto}");
    }

    /// Lo presentado frente a tus datos, en un sello: el 303 cuadra; el 130 se presentó con 2,48 € de más. Uno al
    /// lado del otro (a 96 columnas) y uno debajo del otro (a 76), sin pasarse de ancho y sin perder de dónde sale
    /// cada casilla.
    #[test]
    fn el_sello_dice_si_cuadra() {
        use crate::dominio::{Factura, FormularioLeido, Linea, Num};
        let mut d = Datos::default();
        let linea = Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(21.0) };
        d.facturas.push(Factura { estado: "emitida".into(), numero: "2026-001".into(), fecha: "2026-02-01".into(), operacion: "nacional".into(), lineas: vec![linea], ..Default::default() });
        let c303 = Trimestral::M303.calcular(&d, 2026, 1, &mut Memo::default());
        let mut c130 = Trimestral::M130.calcular(&d, 2026, 1, &mut Memo::default());
        for k in [7, 12, 14, 17, 19] {
            c130.insert(k, fmt::r2(c130.v(k) + 2.48));
        }
        d.lecturas.formularios.insert("2026-1T-303".into(), FormularioLeido { casillas: c303, ..Default::default() });
        d.lecturas.formularios.insert("2026-1T-130".into(), FormularioLeido { casillas: c130, ..Default::default() });
        let app = super::super::prueba::app_con(d, std::path::Path::new("."));
        let mut memo = Memo::default();
        let nuevo = |m, memo: &mut Memo| Modelo::nuevo(&app.ctx, "2026-05-01", 2026, 1, m, memo);
        let t = DelTrimestre { clave: clave(&app.ctx, 1), m303: nuevo(Trimestral::M303, &mut memo), m130: nuevo(Trimestral::M130, &mut memo) };
        tema::poner_seccion('a');
        for ancho in [96, 76] {
            let v = documento(&t, &app.ctx.datos, ancho, false);
            let texto: String = v.iter().map(|l| l.to_string() + "\n").collect();
            assert!(v.iter().all(|l| l.width() <= ancho), "{texto}");
            assert!(texto.contains("▕ CUADRA ▏") && texto.contains("▕ 2,48 € DE MÁS ▏"), "{texto}");
            // De dónde sale cada casilla: tras su nombre o, si no cabe, en una nota.
            assert!(texto.contains("· 1 factura") || texto.contains("09: 1 factura"), "{texto}");
        }
        // Uno al lado del otro: los dos títulos en la misma fila.
        let v = documento(&t, &app.ctx.datos, 96, false);
        assert!(v[0].to_string().contains("IVA · modelo 303") && v[0].to_string().contains("IRPF · modelo 130"), "{}", v[0]);
    }

    /// Cada estado de un trimestre (aún no, en curso, en plazo, vencido, sin datos), uno al lado del otro y uno
    /// debajo del otro: nada se pasa de ancho y cada uno dice cómo va.
    #[test]
    fn cada_estado_cabe() {
        use crate::dominio::{Factura, Linea, Num};
        let mut d = Datos::default();
        let linea = Linea { concepto: "x".into(), cantidad: Num::de(1.0), precio: Num::de(1000.0), iva: Num::de(21.0) };
        d.facturas.push(Factura { estado: "emitida".into(), numero: "2026-001".into(), fecha: "2026-02-01".into(), operacion: "nacional".into(), lineas: vec![linea], ..Default::default() });
        let app = super::super::prueba::app_con(d, std::path::Path::new("."));
        tema::poner_seccion('a');
        let casos = [(1, "2025-12-01", "aún no"), (1, "2026-02-15", "en curso"), (1, "2026-04-02", "en plazo hasta el 20 abr"), (1, "2026-06-01", "plazo terminado"), (2, "2026-08-01", "sin datos en la")];
        for (q, hoy, dice) in casos {
            let mut memo = Memo::default();
            let mut nuevo = |m| Modelo::nuevo(&app.ctx, hoy, 2026, q, m, &mut memo);
            let t = DelTrimestre { clave: clave(&app.ctx, q), m303: nuevo(Trimestral::M303), m130: nuevo(Trimestral::M130) };
            for ancho in [96, 76] {
                let v = documento(&t, &app.ctx.datos, ancho, false);
                let texto: String = v.iter().map(|l| l.to_string() + "\n").collect();
                assert!(v.iter().all(|l| l.width() <= ancho), "{hoy} a {ancho}: {texto}");
                assert!(texto.contains(dice), "{hoy} a {ancho}: {texto}");
            }
        }
    }

    #[test]
    fn textos() {
        assert_eq!(ajustar("hola", 6), "hola  ");
        assert_eq!(ajustar("operaciones", 5), "oper…");
        assert_eq!(ajustar("x", 0), "");
        assert_eq!(a_medida(Line::from("abcdef"), 4).to_string(), "abc…");
        assert_eq!(a_medida(Line::from("ab"), 4).to_string(), "ab  ");
    }
}
