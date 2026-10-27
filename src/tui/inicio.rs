//! «Inicio»: el saludo y qué eres ahora (por cuenta ajena, por tu cuenta o las dos cosas) y, en tarjetas, tu
//! certificado (y cuándo caduca), tu última nómina, tu alta, actividad y cuota de autónomo, el próximo plazo y lo
//! último presentado. Abajo, la carpeta de tus datos y la versión de la app (con su commit).
use crate::dominio::fmt::{eur, fecha_corta};
use crate::dominio::fmt::nombre_mes;
use crate::informe::inicio::{inicio, Inicio as Datos};
use crate::informe::reparto::{reparto, Reparto};
use crate::dominio::fmt;
use crate::tui::texto::{cuando, envolver, frase, ruta_corta};
use crate::tui::widgets::{bloque, kpi, tema};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph, Wrap};
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

/// Lo del borde de abajo en `ancho` columnas: « Tus datos: ~/Impuestos/ana » y « gesshtor 0.1.0 · 1a2b3c ». Si no
/// cabe, la ruta se corta por delante (lo último es la carpeta de la persona) y, si ni así, sin la versión.
fn borde(ruta: &str, version: &str, ancho: usize) -> (String, Option<String>) {
    let version = format!(" gesshtor {version} ");
    let n = |s: &str| s.chars().count();
    let con_version = ancho.saturating_sub(4 + n(&version));
    let (libre, version) = if con_version >= 30 { (con_version, Some(version)) } else { (ancho.saturating_sub(4), None) };
    let entera = format!(" Tus datos: {ruta} ");
    if n(&entera) <= libre {
        return (entera, version);
    }
    let cabe = libre.saturating_sub(n(" Tus datos: … "));
    let cola: String = ruta.chars().skip(n(ruta).saturating_sub(cabe)).collect();
    (format!(" Tus datos: …{cola} "), version)
}

/// Una tarjeta: su título y sus líneas.
type Tarjeta = (&'static str, Vec<Line<'static>>);

/// Lo de arriba (el saludo y qué eres ahora) y las tarjetas: tu certificado (o de cuándo es lo que ves), empleo,
/// autónomo y Hacienda.
fn partes(i: &Datos, ctx: &Ctx) -> (Vec<Line<'static>>, Vec<Tarjeta>) {
    let gris = tema::apagado();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let ahora = match (i.empleo_ahora, i.autonomo_ahora) {
        (true, true) => "Ahora trabajas por cuenta ajena y por tu cuenta (en autónomos).",
        (false, true) => "Ahora trabajas por tu cuenta (de alta en autónomos).",
        (true, false) => "Ahora trabajas por cuenta ajena.",
        (false, false) => "Ahora no constas de alta en autónomos ni con nóminas recientes.",
    };
    let saludo = vec![
        Line::styled(if i.nombre.is_empty() { "Hola.".to_string() } else { format!("Hola, {}.", i.nombre) }, negrita.fg(tema::ACENTO)),
        Line::styled(ahora, gris),
    ];
    let mut tarjetas: Vec<Tarjeta> = Vec::new();

    // El certificado y lo traído.
    // Sin certificado no se abre la app (solo en las pruebas falta).
    if let Some(c) = ctx.certificado.as_deref() {
        let mut v = Vec::new();
        let dias = c.dias_para_caducar();
        v.push(Line::from(format!("{} · {}", c.nombre_completo(), c.nif)));
        let st = if dias < 0 {
            Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD)
        } else if dias < 60 {
            Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        let verbo = if dias < 0 { "Caducó" } else { "Caduca" };
        let mut l = vec![Span::styled(format!("{verbo} el {} ({})", fecha_corta(&c.caduca), plazo_en_dias(dias)), st)];
        if (0..60).contains(&dias) {
            l.push(Span::styled(" · renuévalo en la web de la FNMT", st));
        }
        v.push(Line::from(l));
        if !c.emisor.is_empty() {
            v.push(Line::styled(format!("Emitido por {}", c.emisor), gris));
        }
        match (&ctx.sincronizando, &ctx.sincronizado) {
            (Some(p), _) => v.push(Line::styled(format!("Trayendo lo último… {p}"), Style::new().fg(tema::ACENTO))),
            (None, Some((c, res, errores))) => {
                v.push(Line::styled(format!("Traído {}: {res}.", cuando(c)), Style::new().fg(tema::BIEN)));
                for e in errores.iter().take(4) {
                    v.push(Line::styled(format!("No se ha podido: {e}"), Style::new().fg(tema::MAL)));
                }
            }
            (None, None) => v.push(Line::styled("S: traer lo último de Hacienda y la Seguridad Social.", gris)),
        }
        tarjetas.push(("Tu certificado", v));
    }

    // Empleo.
    if let Some(n) = &i.empleo {
        let mut v = Vec::new();
        let donde = if n.empresas.is_empty() { String::new() } else { format!(", en {}", n.empresas.join(" y ")) };
        v.push(Line::from(format!("Tu última nómina: {}{donde}.", nombre_mes(&n.mes))));
        if n.consultado_hasta.as_str() > n.mes.as_str() {
            v.push(Line::styled(format!("(La Seguridad Social está consultada hasta {}.)", nombre_mes(&n.consultado_hasta)), gris));
        } else {
            v.push(Line::styled(format!("Según la Seguridad Social, consultada hasta {}.", nombre_mes(&n.consultado_hasta)), gris));
        }
        tarjetas.push(("Empleo", v));
    }

    // Autónomo.
    if let Some(a) = &i.autonomo {
        let mut v = Vec::new();
        let alta = if a.alta.is_empty() { String::new() } else { format!("De alta desde el {}", fecha_corta(&a.alta)) };
        let baja = if a.baja.is_empty() { String::new() } else { format!(", de baja desde el {}", fecha_corta(&a.baja)) };
        if !alta.is_empty() {
            let mutua = a.mutua.entidad.trim();
            v.push(Line::from(format!("{alta}{baja}{}.", if mutua.is_empty() { String::new() } else { format!(" · mutua {mutua}") })));
        }
        for act in &a.actividades {
            let desde = act.inicio.trim();
            let desde = if desde.is_empty() { String::new() } else { format!(", desde el {}", fecha_corta(desde)) };
            v.push(Line::from(format!("En la Seguridad Social: {}{desde}", act.tipo.trim())));
            let cnae = [act.cnae.trim().to_string(), frase(&act.cnae_texto)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            if !cnae.is_empty() {
                v.push(Line::styled(cnae, gris));
            }
        }
        if let Some((cuota, mes)) = &a.cuota {
            let mut s = format!("Tu cuota de {mes}: {}", eur(*cuota));
            if let Some(b) = a.cotizacion.base {
                s += &format!(" (base de cotización {}", eur(b));
                if let Some(r) = a.cotizacion.rendimientos_previstos {
                    s += &format!(", por unos rendimientos previstos de {}", eur(r));
                }
                s.push(')');
            }
            s.push('.');
            v.push(Line::from(s));
        }
        if let Some(b) = a.cotizacion.base_proxima {
            let desde = &a.cotizacion.base_proxima_desde;
            let d = if desde.is_empty() { String::new() } else { format!("Desde el {}", fecha_corta(desde)) };
            v.push(Line::from(format!("{d}, tu base pasa a {}.", eur(b))));
        }
        tarjetas.push(("Autónomo", v));
    }

    // Hacienda.
    if i.ultimo_presentado.is_some() || i.proximo_plazo.is_some() {
        let mut v = Vec::new();
        if let Some((q, ini, fin)) = &i.proximo_plazo {
            v.push(Line::from(vec![Span::styled("Próximo plazo:", gris), Span::raw(format!("el {q}, del {} al {}.", fecha_corta(ini), fecha_corta(fin)))]));
        }
        if let Some((modelos, periodo, anio, fecha)) = &i.ultimo_presentado {
            let cuando = if fecha.is_empty() { String::new() } else { format!(" (el {})", fecha_corta(fecha)) };
            v.push(Line::from(vec![Span::styled("Lo último presentado:", gris), Span::raw(format!("{} del {periodo} {anio}{cuando}.", modelos.join(" y ")))]));
        }
        tarjetas.push(("Hacienda", v));
    }
    (saludo, tarjetas)
}

/// Las líneas de una tarjeta, partidas para que quepan en `w` columnas.
fn partir_lineas(v: &[Line<'static>], w: usize) -> Vec<Line<'static>> {
    v.iter().flat_map(|l| envolver(&l.spans.iter().map(|s| (s.content.to_string(), s.style)).collect::<Vec<_>>(), w, 0)).collect()
}

/// Dónde va cada tarjeta en estas columnas (cada una en la que va más corta, para que queden parejas): (columna,
/// y, alto, líneas), y cuántas líneas no caben.
fn colocar(tarjetas: &[Tarjeta], columnas: &[Rect]) -> (Vec<(Rect, Vec<Line<'static>>)>, usize) {
    let mut y: Vec<u16> = columnas.iter().map(|c| c.y).collect();
    let (mut v, mut fuera) = (Vec::new(), 0);
    for (_, lineas) in tarjetas {
        let c = (0..columnas.len()).min_by_key(|&k| y[k]).unwrap_or(0);
        let col = columnas[c];
        let lineas = partir_lineas(lineas, col.width.saturating_sub(4) as usize);
        let quiere = lineas.len() as u16 + 2;
        let alto = quiere.min(col.bottom().saturating_sub(y[c]));
        fuera += quiere.saturating_sub(alto.max(2)) as usize;
        v.push((Rect { y: y[c], height: alto, ..col }, lineas));
        y[c] += alto;
    }
    (v, fuera)
}

/// Las tarjetas en dos columnas o, si así cabe más, en una.
fn pintar_tarjetas(f: &mut Frame, area: Rect, tarjetas: Vec<Tarjeta>) {
    let [a, _, b] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let (dos, fuera_dos) = colocar(&tarjetas, &[a, b]);
    let (una, fuera_una) = colocar(&tarjetas, &[area]);
    let sitios = if area.width >= 76 && fuera_dos <= fuera_una { dos } else { una };
    for ((titulo, _), (r, lineas)) in tarjetas.iter().zip(sitios) {
        if r.height >= 3 {
            f.render_widget(Paragraph::new(lineas).block(bloque(titulo).padding(Padding::horizontal(1))), r);
        }
    }
}

impl Inicio {
    /// Arriba, en grande: cómo va el año (lo generado, lo que se llevaron impuestos y cotizaciones y lo que te
    /// queda) y cuánto falta para el próximo plazo.
    fn pintar_cifras(&self, f: &mut Frame, area: Rect) {
        let y = fmt::anio_actual();
        // (etiqueta, valor, nota, estilo), cada texto en largo y en corto (para las tarjetas estrechas).
        type Cifra = ((String, String), String, (String, String), Style);
        let mut c: Vec<Cifra> = Vec::new();
        let dos = |a: String, b: &str| (a, b.to_string());
        if let Some(r) = &self.anio {
            let t = &r.total;
            c.push((dos(format!("Generado en {y}"), "Generado"), eur(fmt::r2(t.generado - t.iva)), dos("hasta hoy".into(), "hasta hoy"), Style::new()));
            c.push((dos("Impuestos".into(), "Impuestos"), eur(t.impuestos()), dos("IRPF y Seg. Social".into(), "y Seg. Social"), Style::new().fg(tema::MAL)));
            let st = Style::new().fg(if t.queda < -0.005 { tema::MAL } else { tema::BIEN });
            c.push((dos("Te queda".into(), "Te queda"), eur(t.queda), dos("limpio, hasta hoy".into(), "limpio"), st));
        }
        if let Some((q, ini, fin)) = &self.i.proximo_plazo {
            let hoy = fmt::hoy();
            let (n, que, dia) = if ini.as_str() > hoy.as_str() { (fmt::dias_entre(&hoy, ini), "empieza", ini) } else { (fmt::dias_entre(&hoy, fin), "termina", fin) };
            let dias = if n == 1 { "1 día".to_string() } else { format!("{n} días") };
            c.push((dos(format!("Próximo: {q}"), q), dias, (format!("{que} el {}", fecha_corta(dia)), format!("el {}", fecha_corta(dia))), Style::new().fg(tema::ACENTO)));
        }
        let zonas = Layout::horizontal(vec![Constraint::Ratio(1, c.len() as u32); c.len()]).split(area);
        for (((etq, etq_c), v, (nota, nota_c), st), z) in c.iter().zip(zonas.iter()) {
            // La etiqueta, en el borde con un espacio a cada lado; la nota, dentro con un margen.
            let w = z.width as usize;
            let etq = if etq.chars().count() + 4 <= w { etq } else { etq_c };
            let nota = if nota.chars().count() + 3 <= w { nota } else { nota_c };
            kpi(f, *z, etq, v, nota, *st);
        }
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
        let (saludo, tarjetas) = partes(&self.i, ctx);
        let cifras = self.anio.is_some() || self.i.proximo_plazo.is_some();
        // Las cifras del año, si queda sitio para las tarjetas.
        let alto_cifras = if cifras && area.height >= 28 { 6 } else { 0 };
        let [a_saludo, a_cifras, a_tarjetas, a_pie] =
            Layout::vertical([Constraint::Length(3), Constraint::Length(alto_cifras), Constraint::Min(0), Constraint::Length(1)]).areas(area);
        f.render_widget(Paragraph::new(saludo).wrap(Wrap { trim: true }), Rect { x: a_saludo.x + 1, width: a_saludo.width.saturating_sub(1), ..a_saludo });
        if alto_cifras > 0 {
            self.pintar_cifras(f, Rect { height: 5, ..a_cifras });
        }
        pintar_tarjetas(f, a_tarjetas, tarjetas);
        // Abajo, siempre a la vista: dónde están tus datos y qué versión es.
        let (datos, version) = borde(&ruta_corta(&ctx.dir), crate::VERSION, area.width as usize);
        f.render_widget(Paragraph::new(Line::styled(datos, tema::apagado())), a_pie);
        if let Some(v) = version {
            f.render_widget(Paragraph::new(Line::styled(v, tema::apagado()).right_aligned()), a_pie);
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
        assert_eq!((d.as_str(), v.as_deref()), (" Tus datos: ~/Impuestos/ana ", Some(" gesshtor 0.1.0 · 1a2b3c4d5e6f ")));
        // Si no cabe, se ve el final de la ruta: la carpeta de la persona.
        let (d, v) = borde("~/Documentos/mis papeles/Impuestos/ana-perez", "0.1.0 · 1a2b3c4d5e6f", 70);
        assert!(d.ends_with("Impuestos/ana-perez ") && d.contains('…') && v.is_some(), "{d}");
        assert!(d.chars().count() + v.unwrap().chars().count() <= 70 - 4);
        // Muy estrecho: solo la ruta.
        let (d, v) = borde("~/Impuestos/ana-perez", "0.1.0 · 1a2b3c4d5e6f", 40);
        assert!(v.is_none() && d.ends_with("ana-perez ") && d.chars().count() <= 36, "{d}");
    }
}
