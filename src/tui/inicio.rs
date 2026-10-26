//! «Inicio»: el saludo, tu certificado (y cuándo caduca), qué eres ahora (por cuenta ajena, por tu cuenta o las
//! dos cosas), tu última nómina, tu alta, actividad y cuota de autónomo, lo último presentado y el próximo plazo. Abajo,
//! en el borde, la carpeta de tus datos y la versión de la app (con su commit).
use crate::dominio::fmt::{eur, fecha_corta};
use crate::dominio::fmt::nombre_mes;
use crate::informe::inicio::{inicio, Inicio as Datos};
use crate::tui::texto::{cuando, frase, ruta_corta};
use crate::tui::widgets::{bloque, tema};
use crate::tui::{Ctx, Pantalla};
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

#[derive(Default)]
pub struct Inicio {
    clave: Option<(u64, bool, Option<String>)>,
    i: Datos,
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

fn lineas(i: &Datos, ctx: &Ctx) -> Vec<Line<'static>> {
    let gris = tema::apagado();
    let titulo = tema::titulo();
    let negrita = Style::new().add_modifier(Modifier::BOLD);
    let mut v: Vec<Line> = Vec::new();
    v.push(Line::styled(if i.nombre.is_empty() { "Hola.".to_string() } else { format!("Hola, {}.", i.nombre) }, negrita.fg(tema::ACENTO)));
    v.push(Line::from(""));

    // El certificado y lo traído.
    match ctx.certificado.as_deref() {
        Some(c) => {
            let dias = c.dias_para_caducar();
            v.push(Line::styled("Tu certificado", titulo));
            v.push(Line::from(format!("  {} · {}", c.nombre_completo(), c.nif)));
            let st = if dias < 0 {
                Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD)
            } else if dias < 60 {
                Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            let verbo = if dias < 0 { "Caducó" } else { "Caduca" };
            let mut l = vec![Span::styled(format!("  {verbo} el {} ({})", fecha_corta(&c.caduca), plazo_en_dias(dias)), st)];
            if (0..60).contains(&dias) {
                l.push(Span::styled(" · renuévalo en la web de la FNMT", st));
            }
            v.push(Line::from(l));
            if !c.emisor.is_empty() {
                v.push(Line::styled(format!("  Emitido por {}", c.emisor), gris));
            }
            match (&ctx.sincronizando, &ctx.sincronizado) {
                (Some(p), _) => v.push(Line::styled(format!("  Trayendo lo último… {p}"), Style::new().fg(tema::ACENTO))),
                (None, Some((c, res, errores))) => {
                    v.push(Line::styled(format!("  Traído {}: {res}.", cuando(c)), Style::new().fg(tema::BIEN)));
                    for e in errores.iter().take(4) {
                        v.push(Line::styled(format!("  No se ha podido: {e}"), Style::new().fg(tema::MAL)));
                    }
                }
                (None, None) => v.push(Line::styled("  S: traer lo último de Hacienda y la Seguridad Social.", gris)),
            }
        }
        None => {
            let ult = [i.consultado_hacienda.as_deref(), i.consultado_ss.as_deref()].into_iter().flatten().max().map(cuando);
            let s = match ult {
                Some(u) => format!("Ves lo último que se trajo ({u}). Para traer lo nuevo de Hacienda y la Seguridad Social, arranca con --certificado <tu .p12>."),
                None => "Para traer tus datos de Hacienda y la Seguridad Social, arranca con --certificado <tu .p12>.".to_string(),
            };
            v.push(Line::styled(s, gris));
        }
    }
    v.push(Line::from(""));

    // Qué eres ahora.
    let ahora = match (i.empleo_ahora, i.autonomo_ahora) {
        (true, true) => "Ahora trabajas por cuenta ajena y por tu cuenta (en autónomos).",
        (false, true) => "Ahora trabajas por tu cuenta (de alta en autónomos).",
        (true, false) => "Ahora trabajas por cuenta ajena.",
        (false, false) => "Ahora no constas de alta en autónomos ni con nóminas recientes.",
    };
    v.push(Line::styled(ahora, negrita));
    v.push(Line::from(""));

    // Empleo.
    if let Some(n) = &i.empleo {
        v.push(Line::styled("Empleo", titulo));
        let donde = if n.empresas.is_empty() { String::new() } else { format!(", en {}", n.empresas.join(" y ")) };
        v.push(Line::from(format!("  Tu última nómina: {}{donde}.", nombre_mes(&n.mes))));
        if n.consultado_hasta.as_str() > n.mes.as_str() {
            v.push(Line::styled(format!("  (La Seguridad Social está consultada hasta {}.)", nombre_mes(&n.consultado_hasta)), gris));
        } else {
            v.push(Line::styled(format!("  Según la Seguridad Social, consultada hasta {}.", nombre_mes(&n.consultado_hasta)), gris));
        }
        v.push(Line::from(""));
    }

    // Autónomo.
    if let Some(a) = &i.autonomo {
        v.push(Line::styled("Autónomo", titulo));
        let alta = if a.alta.is_empty() { String::new() } else { format!("De alta desde el {}", fecha_corta(&a.alta)) };
        let baja = if a.baja.is_empty() { String::new() } else { format!(", de baja desde el {}", fecha_corta(&a.baja)) };
        if !alta.is_empty() {
            let mutua = a.mutua.entidad.trim();
            v.push(Line::from(format!("  {alta}{baja}{}.", if mutua.is_empty() { String::new() } else { format!(" · mutua {mutua}") })));
        }
        for act in &a.actividades {
            let desde = act.inicio.trim();
            let desde = if desde.is_empty() { String::new() } else { format!(", desde el {}", fecha_corta(desde)) };
            v.push(Line::from(format!("  En la Seguridad Social: {}{desde}", act.tipo.trim())));
            let cnae = [act.cnae.trim().to_string(), frase(&act.cnae_texto)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            if !cnae.is_empty() {
                v.push(Line::styled(format!("    {cnae}"), gris));
            }
        }
        if let Some((cuota, mes)) = &a.cuota {
            let mut s = format!("  Tu cuota de {mes}: {}", eur(*cuota));
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
            v.push(Line::from(format!("  {d}, tu base pasa a {}.", eur(b))));
        }
        v.push(Line::from(""));
    }

    // Hacienda.
    if i.ultimo_presentado.is_some() || i.proximo_plazo.is_some() {
        v.push(Line::styled("Hacienda", titulo));
        if let Some((modelos, periodo, anio, fecha)) = &i.ultimo_presentado {
            let cuando = if fecha.is_empty() { String::new() } else { format!(" (el {})", fecha_corta(fecha)) };
            v.push(Line::from(format!("  Lo último presentado: {} del {periodo} {anio}{cuando}.", modelos.join(" y "))));
        }
        if let Some((q, ini, fin)) = &i.proximo_plazo {
            v.push(Line::from(format!("  Próximo plazo: el {q}, del {} al {}.", fecha_corta(ini), fecha_corta(fin))));
        }
    }
    v
}

impl Pantalla for Inicio {
    fn titulo(&self) -> &'static str {
        "Inicio"
    }

    fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let clave = (ctx.generacion, ctx.certificado.is_some(), ctx.sincronizando.clone());
        if self.clave.as_ref() != Some(&clave) {
            self.i = inicio(&ctx.datos, ctx.certificado.as_deref());
            self.clave = Some(clave);
        }
        // En el borde de abajo, siempre a la vista: dónde están tus datos y qué versión es.
        let (datos, version) = borde(&ruta_corta(&ctx.dir), crate::VERSION, area.width as usize);
        let mut b = bloque("Inicio").title_bottom(Line::styled(datos, tema::apagado()));
        if let Some(v) = version {
            b = b.title_bottom(Line::styled(v, tema::apagado()).right_aligned());
        }
        f.render_widget(Paragraph::new(lineas(&self.i, ctx)).wrap(Wrap { trim: false }).block(b), area);
    }

    fn tecla(&mut self, _k: KeyEvent, _ctx: &mut Ctx) -> bool {
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
