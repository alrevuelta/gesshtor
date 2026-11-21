//! El gráfico de «Años»: por cada año, una barra con lo que ganas apilado por concepto, otra con lo que pagas
//! apilado por concepto y lo que pagó tu empresa; o, en sencillo, lo que ganas, lo que pagas y lo que te queda.
//! Sin caja: arriba, su título en gris y la leyenda. Las barras van por medias filas («▄» y «█»): donde se juntan
//! dos tramos a media celda, el de abajo es el carácter y el de arriba, su fondo. Los años sin Renta van más
//! apagados y con «*».
use crate::dominio::fmt::DIF;
use super::anios::{color, color_ganas, minuscula, COLOR_EMPRESA, COLOR_PAGAS, COLOR_QUEDA};
use crate::tui::texto::compacto;
use crate::informe::anio::{Anio, Concepto};
use crate::tui::widgets::{linea_atajos, muestra, tema, titulo_bloque};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Modo {
    /// Lo que ganas y lo que pagas, apilados por concepto (y lo de tu empresa).
    #[default]
    Conceptos,
    /// Lo que ganas, lo que pagas y lo que te queda.
    Sencillo,
}

/// Una barra: sus tramos (importe, color), de abajo arriba.
type Barra = Vec<(f64, Color)>;

fn barras(a: &Anio, modo: Modo, con_empresa: bool) -> Vec<Barra> {
    match modo {
        Modo::Conceptos => {
            let mut v: Vec<Barra> = vec![
                Concepto::GANAS.iter().map(|&c| (a.de(c), color(c))).collect(),
                Concepto::PAGAS.iter().map(|&c| (a.de(c), color(c))).collect(),
            ];
            if con_empresa {
                v.push(vec![(a.empresa, COLOR_EMPRESA)]);
            }
            v
        }
        Modo::Sencillo => vec![vec![(a.ganas, color_ganas())], vec![(a.pagas, COLOR_PAGAS)], vec![(a.queda, COLOR_QUEDA)]],
    }
}

fn altura(b: &Barra) -> f64 {
    b.iter().map(|t| t.0.max(0.0)).sum()
}

/// Un paso «redondo» (1, 2, 2,5 o 5 por una potencia de 10) que no baja de `x`.
fn paso_redondo(x: f64) -> f64 {
    if x <= 0.0 || !x.is_finite() {
        return 1.0;
    }
    let p = 10f64.powf(x.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].into_iter().map(|m| m * p).find(|v| *v >= x * (1.0 - 1e-9)).unwrap_or(10.0 * p)
}

/// La leyenda: (color, nombre largo, nombre corto) de lo que sale en el gráfico.
fn leyenda(serie: &[Anio], modo: Modo, con_empresa: bool) -> Vec<(Color, &'static str, &'static str)> {
    match modo {
        Modo::Conceptos => {
            let hay = |c: Concepto| serie.iter().any(|a| a.de(c).abs() > DIF);
            let mut v: Vec<_> = Concepto::GANAS.iter().chain(&Concepto::PAGAS).filter(|&&c| hay(c)).map(|&c| (color(c), c.nombre(), c.corto())).collect();
            if con_empresa {
                v.push((COLOR_EMPRESA, "Tu empresa pagó por ti", "Tu empresa"));
            }
            v
        }
        Modo::Sencillo => vec![(color_ganas(), "Lo que ganas", "Ganas"), (COLOR_PAGAS, "Lo que pagas", "Pagas"), (COLOR_QUEDA, "Lo que te queda", "Te queda")],
    }
}

/// La leyenda en las líneas que haga falta (como mucho `max`): «█ sueldo   █ IRPF», primero con los nombres
/// largos; si no caben, cortos.
fn lineas_leyenda(items: &[(Color, &'static str, &'static str)], w: usize, max: usize) -> Vec<Line<'static>> {
    let partir = |largo: bool| -> Vec<Vec<Span<'static>>> {
        let mut lineas: Vec<Vec<Span>> = vec![Vec::new()];
        let mut usado = 0;
        for (c, l, k) in items {
            let t = minuscula(if largo { l } else { k });
            let n = t.chars().count() + 2;
            if usado > 0 && usado + 3 + n > w {
                lineas.push(Vec::new());
                usado = 0;
            }
            let ultima = lineas.last_mut().expect("una línea");
            if usado > 0 {
                ultima.push(Span::raw("   "));
                usado += 3;
            }
            ultima.push(muestra(*c));
            ultima.push(Span::styled(t, tema::apagado()));
            usado += n;
        }
        lineas
    };
    let mut l = partir(true);
    if l.len() > 1 {
        l = partir(false);
    }
    // Si aún no cabe, lo que quepa y «…».
    if l.len() > max.max(1) {
        l.truncate(max.max(1));
        if let Some(u) = l.last_mut() {
            u.push(Span::styled(" …", tema::apagado()));
        }
    }
    l.into_iter().map(Line::from).collect()
}

/// Un año sin Renta: sus colores, más apagados.
fn apagar(c: Color) -> Color {
    tema::mezcla(c, tema::FONDO, 0.45)
}

/// Pinta una barra apilada en `zona` (de abajo arriba; lo más alto, `top`) por medias filas. Tenue: un año sin
/// Renta.
fn pila(buf: &mut Buffer, zona: Rect, top: f64, tramos: &[(f64, Color)], tenue: bool) {
    let (x, w, filas) = (zona.x, zona.width, zona.height);
    let Some(abajo) = zona.bottom().checked_sub(1) else { return };
    let total = filas as i64 * 2;
    // Hasta dónde llega cada tramo, en medias filas (los que no llegan a media fila no se ven).
    let mut lim: Vec<(i64, Color)> = Vec::new();
    let mut acum = 0.0;
    for &(v, c) in tramos {
        if v <= DIF {
            continue;
        }
        acum += v;
        let fin = ((acum / top) * total as f64).round().clamp(0.0, total as f64) as i64;
        if fin > lim.last().map_or(0, |l| l.0) {
            lim.push((fin, if tenue { apagar(c) } else { c }));
        }
    }
    let Some(&(tope, _)) = lim.last() else { return };
    for r in 0..filas {
        let c0 = r as i64 * 2;
        if tope <= c0 || abajo < r {
            break;
        }
        // El tramo que cubre la mitad de abajo de la celda y, si acaba en medio, el que sigue encima.
        let Some(k) = lim.iter().position(|l| l.0 > c0) else { break };
        let (fin_k, col_k) = lim[k];
        let (simbolo, st) = if fin_k >= c0 + 2 {
            ("█", Style::new().fg(col_k))
        } else {
            match lim.get(k + 1) {
                Some(&(_, col_a)) => ("▄", Style::new().fg(col_k).bg(col_a)),
                None => ("▄", Style::new().fg(col_k)),
            }
        };
        let y = abajo - r;
        for dx in 0..w {
            if let Some(celda) = buf.cell_mut((x + dx, y)) {
                celda.set_symbol(simbolo).set_style(st);
            }
        }
    }
}

/// El gráfico de la serie en `area`, sin caja: arriba su título en gris y, a la derecha, `g` con `siguiente` (lo
/// que enseña al pulsarla); debajo, la leyenda, las barras y los años. `sel` es el año elegido (índice en `serie`).
pub(super) fn dibujar(f: &mut Frame, area: Rect, serie: &[Anio], sel: usize, modo: Modo, siguiente: &str) {
    let titulo = match modo {
        Modo::Conceptos => "lo que ganas y lo que pagas",
        Modo::Sencillo => "lo que ganas, lo que pagas y lo que te queda",
    };
    let [a_tit, dentro] = Layout::vertical([Constraint::Length(area.height.min(1)), Constraint::Min(0)]).areas(area);
    f.render_widget(Paragraph::new(titulo_bloque(titulo)), a_tit);
    let teclas = linea_atajos(&[("g", siguiente)]);
    if titulo.chars().count() + 3 + teclas.width() <= a_tit.width as usize {
        f.render_widget(Paragraph::new(teclas.right_aligned()), a_tit);
    }
    if serie.is_empty() || dentro.height < 3 || dentro.width < 16 {
        if dentro.height > 0 {
            let t = if serie.is_empty() { "Aún no hay ningún año con datos." } else { "Más grande para ver el gráfico." };
            f.render_widget(Paragraph::new(Line::styled(t, tema::apagado())), dentro);
        }
        return;
    }
    let con_empresa = modo == Modo::Conceptos && serie.iter().any(|a| a.empresa > DIF);
    let items = leyenda(serie, modo, con_empresa);
    let ley = lineas_leyenda(&items, dentro.width as usize, if dentro.height >= 7 { 2 } else { 1 });
    let [a_ley, a_graf, a_anios] = Layout::vertical([Constraint::Length(ley.len() as u16), Constraint::Min(1), Constraint::Length(1)]).areas(dentro);
    f.render_widget(Paragraph::new(ley), a_ley);
    let filas = a_graf.height;
    if filas == 0 {
        return;
    }

    // Escala: un paso redondo, con unas tres filas entre marcas.
    let max = serie.iter().flat_map(|a| barras(a, modo, con_empresa)).map(|b| altura(&b)).fold(0.0, f64::max).max(1.0);
    let marcas = (filas / 3).clamp(1, 4) as f64;
    let paso = paso_redondo(max / marcas);
    let top = (max / paso).ceil() * paso;
    let ticks: Vec<(u16, String)> = (1..=((top / paso).round() as i64))
        .map(|k| {
            let v = k as f64 * paso;
            let fila = ((v / top) * filas as f64).round() as u16;
            (fila.saturating_sub(1), compacto(v))
        })
        .collect();
    let w_eje = (ticks.iter().map(|t| t.1.chars().count()).max().unwrap_or(1) as u16 + 1).min(a_graf.width);
    let plot = Rect { x: a_graf.x + w_eje, width: a_graf.width.saturating_sub(w_eje), ..a_graf };
    let abajo = a_graf.y + filas - 1;
    let fino = Style::new().fg(tema::PUNTOS);
    {
        let buf = f.buffer_mut();
        for r in 0..filas {
            let y = abajo - r;
            let tick = ticks.iter().find(|t| t.0 == r);
            if let Some((_, t)) = tick {
                let x = (a_graf.x + w_eje).saturating_sub(1 + t.chars().count() as u16).max(a_graf.x);
                buf.set_string(x, y, t, tema::apagado());
                // Línea de guía, por debajo de las barras.
                for x in plot.x..plot.right() {
                    buf.set_string(x, y, "┈", fino);
                }
            }
            buf.set_string(a_graf.x + w_eje - 1, y, if tick.is_some() { "┤" } else { "│" }, fino);
        }
    }

    // Los años que caben (si no caben todos, los de alrededor del elegido).
    let nb = barras(&serie[0], modo, con_empresa).len();
    let ancho = plot.width as usize;
    let caben = (ancho / (nb + 1)).max(1);
    let n = serie.len();
    let (ini, fin) = if n <= caben {
        (0, n)
    } else {
        let ini = sel.saturating_sub(caben / 2).min(n - caben);
        (ini, ini + caben)
    };
    let vis = fin - ini;
    let por = ancho / vis;
    let (w, hueco) = [(4, 1), (3, 1), (2, 1), (2, 0), (1, 0)].into_iter().find(|(w, h)| nb * w + (nb - 1) * h < por).unwrap_or((1, 0));
    let grupo = nb * w + (nb - 1) * hueco;
    let x0 = plot.x as usize + (ancho - por * vis) / 2;
    for (i, a) in serie[ini..fin].iter().enumerate() {
        let banda = x0 + i * por;
        let bx = banda + por.saturating_sub(grupo) / 2;
        for (j, barra) in barras(a, modo, con_empresa).iter().enumerate() {
            let zona = Rect { x: (bx + j * (w + hueco)) as u16, width: w as u16, ..a_graf };
            pila(f.buffer_mut(), zona.intersection(plot), top, barra, !a.con_renta);
        }
        let marca = if a.con_renta { "" } else { "*" };
        let t = if por >= 6 { format!("{}{marca}", a.y) } else if por >= 5 { a.y.to_string() } else if por >= 3 { format!("'{:02}", a.y % 100) } else { format!("{:02}", a.y % 100) };
        let st = if ini + i == sel { tema::titulo() } else { tema::apagado() };
        let x = (banda + por.saturating_sub(t.chars().count()) / 2) as u16;
        if x < a_anios.right() {
            f.buffer_mut().set_stringn(x, a_anios.y, &t, (a_anios.right() - x) as usize, st);
        }
    }
    // Si no caben todos, flechas a los lados.
    if ini > 0 {
        f.buffer_mut().set_string(a_anios.x, a_anios.y, "‹", tema::apagado());
    }
    if fin < n {
        f.buffer_mut().set_string(a_anios.right().saturating_sub(1), a_anios.y, "›", tema::apagado());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn anio(y: i32, ganas: [f64; 5], pagas: [f64; 3], empresa: f64) -> Anio {
        let mut a = Anio { y, empresa, con_renta: true, ..Default::default() };
        for (c, v) in Concepto::GANAS.iter().zip(ganas) {
            a.importes[*c as usize] = v;
        }
        for (c, v) in Concepto::PAGAS.iter().zip(pagas) {
            a.importes[*c as usize] = v;
        }
        a.ganas = ganas.iter().sum();
        a.pagas = pagas.iter().sum();
        a.queda = a.ganas - a.pagas;
        a
    }

    #[test]
    fn pasos_redondos() {
        assert_eq!(paso_redondo(3.0), 5.0);
        assert_eq!(paso_redondo(18_000.0), 20_000.0);
        assert_eq!(paso_redondo(2_100.0), 2_500.0);
        assert_eq!(paso_redondo(10_000.0), 10_000.0);
        assert_eq!(paso_redondo(0.0), 1.0);
    }

    #[test]
    fn la_barra_apilada_cambia_de_color_en_su_sitio() {
        // 4 filas; 50 abajo y 50 encima sobre 100: dos filas de cada.
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 4));
        pila(&mut buf, Rect::new(0, 0, 1, 4), 100.0, &[(50.0, tema::AZUL), (50.0, tema::AVISO)], false);
        let celdas: Vec<(&str, Option<Color>)> = (0..4).map(|y| (buf[(0, y)].symbol(), Some(buf[(0, y)].fg))).collect();
        assert_eq!(celdas, [("█", Some(tema::AVISO)), ("█", Some(tema::AVISO)), ("█", Some(tema::AZUL)), ("█", Some(tema::AZUL))]);
        // Un tramo que acaba a media celda: el de abajo es el carácter y el de encima, el fondo.
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 2));
        pila(&mut buf, Rect::new(0, 0, 1, 2), 100.0, &[(25.0, tema::AZUL), (75.0, tema::AVISO)], false);
        assert_eq!(buf[(0, 1)].symbol(), "▄");
        assert_eq!((buf[(0, 1)].fg, buf[(0, 1)].bg), (tema::AZUL, tema::AVISO));
        assert_eq!((buf[(0, 0)].symbol(), buf[(0, 0)].fg), ("█", tema::AVISO));
        // La punta: medio bloque sin fondo (el de una celda sin pintar); y lo que no llega a nada no pinta.
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 2));
        let sin_pintar = buf[(0, 0)].bg;
        pila(&mut buf, Rect::new(0, 0, 1, 2), 100.0, &[(75.0, tema::AZUL), (-5.0, tema::AVISO)], false);
        assert_eq!((buf[(0, 0)].symbol(), buf[(0, 0)].bg), ("▄", sin_pintar));
    }

    #[test]
    fn pinta_en_cualquier_tamano() {
        let serie: Vec<Anio> = (2020..=2026).map(|y| anio(y, [1000.0 * (y - 2019) as f64, 500.0, 0.0, 0.0, 200.0], [300.0, 100.0, 50.0], 400.0)).collect();
        for modo in [Modo::Conceptos, Modo::Sencillo] {
            for (w, h) in [(120, 18), (80, 10), (40, 8), (20, 5), (10, 3), (3, 2)] {
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| dibujar(f, f.area(), &serie, 3, modo, "sencillo")).unwrap();
            }
        }
        let mut t = Terminal::new(TestBackend::new(120, 18)).unwrap();
        t.draw(|f| dibujar(f, f.area(), &serie, 3, Modo::Conceptos, "sencillo")).unwrap();
        let s: String = (0..18).map(|y| (0..120).map(|x| t.backend().buffer()[(x, y)].symbol().to_string()).collect::<String>() + "\n").collect();
        assert!(s.contains("2023") && s.contains("█") && s.contains("sueldo") && s.contains("tu empresa"), "{s}");
        assert!(s.contains("lo que ganas y lo que pagas") && s.contains("g sencillo"), "el título y la tecla, sin caja: {s}");
        assert!(!s.contains("ganancias") && !s.contains('┌'), "solo lo que hay, y sin caja: {s}");
        // Vacía.
        let mut t = Terminal::new(TestBackend::new(60, 10)).unwrap();
        t.draw(|f| dibujar(f, f.area(), &[], 0, Modo::Conceptos, "sencillo")).unwrap();
        // Muchos años en poco ancho: los de alrededor del elegido, con flechas.
        let muchos: Vec<Anio> = (2000..=2026).map(|y| anio(y, [100.0, 0.0, 0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 0.0)).collect();
        let mut t = Terminal::new(TestBackend::new(40, 10)).unwrap();
        t.draw(|f| dibujar(f, f.area(), &muchos, 5, Modo::Sencillo, "lo de la Renta")).unwrap();
        // Los años, en la última fila (sin caja).
        let fila: String = (0..40).map(|x| t.backend().buffer()[(x, 9)].symbol().to_string()).collect();
        assert!(fila.contains('›') && fila.contains("05"), "{fila}");
    }
}
