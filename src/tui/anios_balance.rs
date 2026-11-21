//! Un año entero (Enter en «Años»), en el estilo C2 y sin cajas. Arriba, fija, una línea con qué año es y con qué
//! se ha hecho la cuenta. Debajo, lo que se baja con ↑↓:
//! - la idea principal en una banda: con Renta, lo que te devuelven o te sale a pagar (con `o` se abre su PDF); sin
//!   ella, lo que te queda (el año en curso, hasta hoy, y al lado lo que saldría en tu Renta, si se puede estimar);
//! - el porqué, como un recibo con puntos guía (con Renta, el de tu modelo 100; sin ella, tu año: lo que
//!   generaste y su reparto en una barra fina, como en Inicio), y tus Rentas, una al lado de otra;
//! - cada fuente paso a paso, con de dónde sale cada cifra (`anios_reparto`), y la Renta casilla a casilla.
use crate::dominio::fmt::DIF;
use super::anios::Lienzo;
use super::anios_renta as renta;
use super::anios_reparto;
use crate::informe::reparto::{self, Reparto};
use crate::dominio::renta::Renta;
use crate::dominio::fmt::{self, eur, eur0};
use crate::informe::anio::{self, Detalle, euro, pct};
use crate::informe::prevision::{self, Prevision};
use crate::informe::{self};
use crate::tui::texto::envolver;
use crate::tui::widgets::{barra, guia, linea_atajos, muestra, tema, titulo_bloque};
use crate::tui::Ctx;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Desde este ancho, la Renta casilla a casilla va en dos columnas.
const ANCHO_DOS_COLUMNAS: u16 = 100;

/// Lo de un año, ya calculado, y por dónde se va bajando.
#[derive(Default)]
pub(super) struct Vista {
    /// (generación de los datos, año) con que se calculó.
    clave: Option<(u64, i32)>,
    /// El año que se veía (al cambiar de año se vuelve arriba).
    visto: Option<i32>,
    d: Detalle,
    /// A dónde fue lo de cada fuente.
    rep: Reparto,
    /// Su Renta, cuándo se presentó y, si aún no está, hasta cuándo se puede presentar.
    renta: Option<Renta>,
    presentada: Option<String>,
    pendiente: Option<String>,
    /// El año en curso: lo que saldría en su Renta.
    prevision: Option<Prevision>,
    scroll: u16,
    scroll_max: u16,
    lienzo: Lienzo,
}

fn gris() -> Style {
    tema::apagado()
}

impl Vista {
    /// Rehace las cifras si han cambiado los datos o el año.
    pub(super) fn al_dia(&mut self, ctx: &Ctx) {
        let k = (ctx.generacion, ctx.anio);
        if self.clave == Some(k) {
            return;
        }
        if self.visto.is_some_and(|v| v != ctx.anio) {
            self.scroll = 0;
        }
        self.visto = Some(ctx.anio);
        self.d = anio::detalle(&ctx.datos, ctx.anio);
        self.renta = informe::renta_de(&ctx.datos, ctx.anio).cloned();
        self.presentada = crate::hacienda::presentada(&ctx.datos, ctx.anio, 0, crate::hacienda::Modelo::M100).map(|d| d.fecha);
        self.pendiente = informe::renta_pendiente(&ctx.datos, ctx.anio);
        self.rep = reparto::reparto(&ctx.datos, &self.d, self.renta.as_ref());
        self.prevision = self.en_curso().then(|| prevision::prevision(&ctx.datos, ctx.anio, &self.d));
        self.clave = Some(k);
    }

    /// Para recalcular al volver (al entrar en la pestaña).
    pub(super) fn olvidar(&mut self) {
        self.clave = None;
        self.lienzo.olvidar();
    }

    pub(super) fn con_renta(&self) -> bool {
        self.d.anio.con_renta
    }

    /// El año en curso (sin Renta todavía).
    fn en_curso(&self) -> bool {
        self.d.anio.y >= fmt::anio_actual() && !self.con_renta()
    }

    pub(super) fn dibujar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.al_dia(ctx);
        // Mientras no cambie nada (datos, año, por dónde se va, tamaño), se repite lo pintado. (Si se ha bajado de
        // más, al pintar se ajusta y la vez siguiente se pinta otra vez, ya ajustado.)
        let clave = (self.clave, self.scroll);
        let mut lienzo = std::mem::take(&mut self.lienzo);
        lienzo.pintar(f, area, clave, |f| self.pintar(f, area, ctx));
        self.lienzo = lienzo;
    }

    /// Arriba, fija, la línea de qué año es; debajo, tras una fila de aire, todo lo demás, que se baja con ↑↓.
    fn pintar(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let a_tit = Rect { height: area.height.min(1), ..area };
        let resto = area.height.saturating_sub(2);
        let a_cuerpo = Rect { y: area.bottom() - resto, height: resto, ..area };
        self.cabecera(f, a_tit);
        let lineas = self.lineas(ctx, a_cuerpo.width as usize);
        self.desplazable(f, a_cuerpo, lineas);
    }

    /// La línea de arriba y, a la derecha si caben, las teclas que más se usan aquí.
    fn cabecera(&self, f: &mut Frame, area: Rect) {
        let t = self.titulo();
        let mut teclas = vec![("Esc", "volver")];
        if self.con_renta() {
            teclas.insert(0, ("o", "PDF del modelo 100"));
        }
        let teclas = linea_atajos(&teclas);
        let cabe = t.width() + 5 + teclas.width() <= area.width as usize;
        f.render_widget(Paragraph::new(t), area);
        if cabe {
            f.render_widget(Paragraph::new(teclas.right_aligned()), area);
        }
    }

    /// «Tu Renta de 2025 · individual · presentada el 12 may 2026»; sin Renta, con qué se ha hecho la cuenta:
    /// «2026 · en curso: hasta hoy, con la Seguridad Social y lo apuntado».
    fn titulo(&self) -> Line<'static> {
        let y = self.d.anio.y;
        let negrita = Style::new().add_modifier(Modifier::BOLD);
        if let Some(r) = self.renta.as_ref().filter(|_| self.con_renta()) {
            let mut resto = vec![if r.conjunta { "conjunta" } else { "individual" }.to_string()];
            if r.rectificativa {
                resto.push("rectificativa".into());
            }
            if let Some(p) = &self.presentada {
                resto.push(format!("presentada el {}", fmt::fecha_corta(p)));
            }
            return Line::from(vec![Span::styled(format!("Tu Renta de {y}"), negrita), Span::styled(format!(" · {}", resto.join(" · ")), gris())]);
        }
        let aviso = Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD);
        let (modo, resto) = if self.en_curso() {
            ("en curso", "hasta hoy, con la Seguridad Social y lo apuntado".to_string())
        } else {
            match &self.pendiente {
                // Lo de «hasta entonces…», al lado de la banda: aquí no cabría.
                Some(fin) => ("Renta pendiente", format!("se presenta hasta el {}", fmt::fecha_corta(fin))),
                None => ("sin Renta", "con la Seguridad Social y lo apuntado".into()),
            }
        };
        Line::from(vec![Span::styled(y.to_string(), negrita), Span::styled(" · ", gris()), Span::styled(modo, aviso), Span::styled(format!(": {resto}"), gris())])
    }

    /// Todo lo que se baja con ↑↓, en `w` columnas.
    fn lineas(&self, ctx: &Ctx, w: usize) -> Vec<Line<'static>> {
        let blanco = || Line::from("");
        let renta = self.renta.as_ref().filter(|_| self.con_renta());
        let generado = fmt::r2(self.rep.total.generado - self.rep.total.iva);
        let mut l = self.banda(w);
        l.push(blanco());
        l.push(titulo_bloque("por qué"));
        match renta {
            Some(r) => l.extend(renta::recibo(r, generado, &self.nota_generado(), w)),
            None => l.extend(self.tu_anio(w)),
        }
        l.push(blanco());
        l.extend(self.tus_rentas(ctx, w));
        l.push(blanco());
        // Con Renta, el porqué es el de Hacienda; lo que te quedó va después.
        if renta.is_some() {
            l.push(titulo_bloque("tu año"));
            l.extend(self.tu_anio(w));
            l.push(blanco());
        }
        l.extend(anios_reparto::lineas(&self.rep, &self.d, self.prevision.as_ref(), w.min(100)));
        if let Some(r) = &self.renta {
            let dos = w >= ANCHO_DOS_COLUMNAS as usize;
            // En una columna, con los importes acabando donde los de los pasos de arriba (y la casilla detrás).
            let wr = if dos { ((w - 3) / 2).min(72) } else { w.min(66) };
            l.push(blanco());
            l.extend(renta::lineas(self.d.anio.y, r, self.presentada.as_deref(), wr, dos));
        }
        l
    }

    /// La idea principal: tres filas de fondo suave con la cifra en la de en medio (acabando donde acaban las del
    /// recibo) y, al lado, unas notas en gris. Con Renta, lo que salió; sin ella, lo que te queda.
    fn banda(&self, w: usize) -> Vec<Line<'static>> {
        let renta = self.renta.as_ref().filter(|_| self.con_renta());
        let (et, cifra, color, fondo) = match renta {
            Some(r) if r.resultado < -DIF => ("te devuelven", eur(-r.resultado), tema::BIEN, tema::SUAVE_BIEN),
            Some(r) if r.resultado > DIF => ("te sale a pagar", eur(r.resultado), tema::AVISO, tema::SUAVE_AVISO),
            Some(_) => ("te sale a cero", eur(0.0), tema::TINTA, tema::suave(tema::MARCO)),
            None => {
                let q = self.rep.total.queda;
                let et = if self.en_curso() { "te queda, hasta hoy" } else { "te queda" };
                if q < -DIF { (et, euro(q), tema::AVISO, tema::SUAVE_AVISO) } else { (et, euro(q), tema::BIEN, tema::SUAVE_BIEN) }
            }
        };
        // La banda acaba una columna después de la cifra, como en los bocetos.
        let fin = (renta::FIN as usize).min(w.saturating_sub(1));
        let ancho = (fin + 1).min(w);
        let st = Style::new().fg(color).bg(fondo).add_modifier(Modifier::BOLD);
        let vacia = || vec![Span::styled(" ".repeat(ancho), Style::new().bg(fondo))];
        let hueco = fin.saturating_sub(2 + et.chars().count() + cifra.chars().count());
        let medio = vec![Span::styled(format!("  {et}{}{cifra}", " ".repeat(hueco.max(1))), st), Span::styled(" ".repeat(ancho.saturating_sub(fin)), Style::new().bg(fondo))];
        let mut filas = vec![vacia(), medio, vacia()];
        // Las notas: al lado si hay sitio; si no, debajo.
        let notas = self.notas_banda();
        let al_lado = w.saturating_sub(renta::AL_LADO);
        if al_lado >= 36 {
            let v: Vec<Line> = notas.iter().flat_map(|n| envolver(n, al_lado, 0)).collect();
            // Desde la fila de la cifra; si son tres o más, desde la de arriba.
            let desde = usize::from(v.len() < 3);
            for (i, n) in v.into_iter().enumerate() {
                let i = i + desde;
                if i >= filas.len() {
                    filas.push(Vec::new());
                }
                let fila = &mut filas[i];
                let usado: usize = fila.iter().map(|s| s.width()).sum();
                fila.push(Span::raw(" ".repeat(renta::AL_LADO.saturating_sub(usado))));
                fila.extend(n.spans);
            }
            filas.into_iter().map(Line::from).collect()
        } else {
            let mut l: Vec<Line> = filas.into_iter().map(Line::from).collect();
            for n in &notas {
                l.extend(envolver(n, w, 0));
            }
            l
        }
    }

    /// Lo que va al lado de la banda: con Renta, cómo se pagó o se cobró; en el año en curso, lo que saldría en tu
    /// Renta; si la Renta está pendiente, con qué se ha hecho la cuenta hasta entonces.
    fn notas_banda(&self) -> Vec<Vec<(String, Style)>> {
        let y = self.d.anio.y;
        if let Some(r) = self.renta.as_ref().filter(|_| self.con_renta()) {
            return renta::notas_banda(r).into_iter().map(|t| vec![(t, gris())]).collect();
        }
        match &self.prevision {
            Some(Prevision::Estimada { resultado: x, .. }) => {
                let que = if *x > DIF { "a pagar" } else if *x < -DIF { "a devolver" } else { "a cero" };
                // (Cada trozo se escribe con un espacio delante: ninguno empieza por una coma.)
                vec![vec![(format!("Tu Renta de {y}, estimada:"), gris()), (format!("≈{} {que}", eur0(x.abs())), super::anios::color_resultado(*x).add_modifier(Modifier::BOLD))]]
            }
            Some(Prevision::NoSe(_)) => vec![vec![(format!("Tu Renta de {y}: en abril de {}.", y + 1), gris())]],
            None if self.pendiente.is_some() => vec![vec![("Hasta entonces, con la Seguridad Social y lo apuntado.".into(), gris())]],
            None => Vec::new(),
        }
    }

    /// Debajo de «lo que generaste»: con qué se ha contado.
    fn nota_generado(&self) -> String {
        let a = &self.d.anio;
        if a.sueldo_aprox {
            "con el sueldo aproximado".into()
        } else if a.empresa > DIF {
            "con lo que pagó tu empresa por ti".into()
        } else if self.en_curso() {
            "hasta hoy".into()
        } else {
            "antes de impuestos".into()
        }
    }

    /// Tu año, como en Inicio: lo que generaste y, debajo, una barra fina con su reparto y una línea por tramo
    /// (lo que te queda, impuestos y cotizaciones y los gastos de tu actividad), cada una con su nota en gris (al
    /// lado si cabe; si no, debajo). Si no te queda nada, sin barra: no hay reparto que enseñar.
    fn tu_anio(&self, w: usize) -> Vec<Line<'static>> {
        let (a, t) = (&self.d.anio, &self.rep.total);
        let generado = fmt::r2(t.generado - t.iva);
        let nota_impuestos = if a.falta_nomina {
            "sin el IRPF de tu nómina".to_string()
        } else if t.base() > DIF {
            format!("el {} de lo que generaste", pct(t.impuestos(), t.base()))
        } else {
            String::new()
        };
        let nota_queda = if a.especie > DIF { format!("{} en especie", eur0(a.especie)) } else { String::new() };
        let queda: Color = if t.queda < -DIF { tema::AVISO } else { tema::BIEN };
        let con_barra = generado > DIF && t.queda >= 0.0;
        // (muestra, etiqueta, cifra, estilo, nota). Con barra, lo que te queda primero, como en Inicio; sin ella,
        // en el orden de la cuenta: lo generado, menos los gastos y los impuestos, es lo que te queda.
        let mut filas = vec![(tema::BIEN, "te queda", euro(t.queda), Style::new().fg(queda), nota_queda)];
        filas.push((tema::AVISO, "impuestos y cotizaciones", eur(t.impuestos()), Style::new().fg(tema::AVISO), nota_impuestos));
        if t.gastos > DIF {
            filas.push((tema::MARCO, "gastos de tu actividad", eur(t.gastos), Style::new(), String::new()));
        }
        if !con_barra {
            filas.reverse();
        }
        let fin = renta::FIN.min(w as u16);
        let con_nota = |mut g: Line<'static>, nota: String, l: &mut Vec<Line<'static>>| {
            if nota.is_empty() {
                l.push(g);
            } else if g.width() + 3 + nota.chars().count() <= w {
                g.push_span(Span::styled(format!("   {nota}"), gris()));
                l.push(g);
            } else {
                l.push(g);
                l.push(Line::styled(format!("  {nota}"), gris()));
            }
        };
        let mut l = Vec::new();
        con_nota(guia("lo que generaste", euro(generado), Style::new(), fin), self.nota_generado(), &mut l);
        if con_barra {
            l.push(barra(&[(t.queda, tema::BIEN), (t.impuestos(), tema::AVISO), (t.gastos, tema::MARCO)], fin));
        }
        for (color, et, cifra, st, nota) in filas {
            let et = if con_barra { Line::from(vec![muestra(color), Span::raw(et)]) } else { Line::from(et) };
            con_nota(guia(et, cifra, st, fin), nota, &mut l);
        }
        l
    }

    /// Tus Rentas, una al lado de otra como en el boceto («2024   −180,20 €» y debajo «pagaste»): las que quepan
    /// alrededor de este año (todas están en la lista, con `g`). Debajo, la que falta o la del año en curso.
    fn tus_rentas(&self, ctx: &Ctx, w: usize) -> Vec<Line<'static>> {
        let hoy = fmt::anio_actual();
        let y = self.d.anio.y;
        let rentas: Vec<(i32, f64)> = (informe::primer_anio(&ctx.datos)..=hoy).filter_map(|a| informe::renta_de(&ctx.datos, a).map(|r| (a, r.resultado))).collect();
        let mut l = vec![titulo_bloque("tus Rentas")];
        if rentas.is_empty() {
            l.push(Line::styled("Aún no hay ninguna Renta tuya en la app: se leen de los PDF de Hacienda.", gris()));
        } else {
            let cifra = |x: f64| if x < -DIF { format!("+{}", eur(-x)) } else if x > DIF { format!("−{}", eur(x)) } else { eur(0.0) };
            let dice = |x: f64| if x < -DIF { "te devolvieron" } else if x > DIF { "pagaste" } else { "a cero" };
            // Cada una: el año y la cifra acabando en `dentro`; de una a otra, `paso` columnas.
            let dentro = rentas.iter().map(|r| cifra(r.1).chars().count() + 5).max().unwrap_or(0).max(17);
            let paso = (dentro + 8).max(32);
            let k = (w.saturating_sub(dentro) / paso + 1).max(1);
            let i = rentas.iter().rposition(|r| r.0 <= y).unwrap_or(0);
            let fin = (i + 1).max(k).min(rentas.len());
            let (mut arriba, mut abajo): (Vec<Span>, Vec<Span>) = (Vec::new(), Vec::new());
            for (j, &(a, x)) in rentas[fin.saturating_sub(k)..fin].iter().enumerate() {
                let x0 = j * paso;
                let c = cifra(x);
                let st_anio = if a == y { tema::titulo() } else { gris() };
                arriba.push(Span::raw(" ".repeat(x0.saturating_sub(Line::from(arriba.clone()).width()))));
                arriba.push(Span::styled(a.to_string(), st_anio));
                arriba.push(Span::raw(" ".repeat(dentro.saturating_sub(4 + c.chars().count()))));
                arriba.push(Span::styled(c, super::anios::color_resultado(x).add_modifier(Modifier::BOLD)));
                let d = dice(x);
                abajo.push(Span::raw(" ".repeat((x0 + dentro).saturating_sub(Line::from(abajo.clone()).width() + d.chars().count()))));
                abajo.push(Span::styled(d, gris()));
            }
            l.extend([Line::from(arriba), Line::from(abajo)]);
        }
        if let Some(fin) = informe::renta_pendiente(&ctx.datos, hoy - 1) {
            l.extend(envolver(&[(format!("{}: aún sin Renta; se presenta hasta el {}.", hoy - 1, fmt::fecha_corta(&fin)), Style::new().fg(tema::AVISO))], w, 0));
        }
        let (ini, fin) = crate::hacienda::plazo_renta(hoy);
        l.extend(envolver(&[(format!("{hoy}, en curso: la presentarás del {} al {}.", fmt::fecha_corta(&ini), fmt::fecha_corta(&fin)), gris())], w, 0));
        l
    }

    /// Las líneas en `area`, desde `scroll`. Si no caben, la última fila dice que hay más (↑↓).
    fn desplazable(&mut self, f: &mut Frame, area: Rect, lineas: Vec<Line<'static>>) {
        let alto = area.height;
        let visible = if lineas.len() as u16 <= alto { alto } else { alto.saturating_sub(1) };
        self.scroll_max = (lineas.len() as u16).saturating_sub(visible);
        self.scroll = self.scroll.min(self.scroll_max);
        f.render_widget(Paragraph::new(lineas).scroll((self.scroll, 0)), Rect { height: visible, ..area });
        if self.scroll_max > 0 && alto > visible {
            let t = if self.scroll >= self.scroll_max { "↑ más arriba" } else { "↓ más abajo (↑↓)" };
            f.render_widget(Paragraph::new(Line::styled(t, gris()).right_aligned()), Rect { y: area.y + visible, height: 1, ..area });
        }
    }

    /// Bajar por el año. Devuelve true si ha usado la tecla.
    pub(super) fn tecla(&mut self, k: KeyEvent) -> bool {
        match k.code {
            // Al bajar no se mira el tope: se ajusta al pintar, que es cuando se sabe cuánto cabe.
            KeyCode::Up => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.min(self.scroll_max).saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = u16::MAX,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lo que va al lado de la banda, como se lee (cada nota en una línea).
    fn notas(v: &Vista) -> Vec<String> {
        v.notas_banda().iter().map(|n| envolver(n, 200, 0).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect()).collect()
    }

    #[test]
    fn al_lado_de_la_banda() {
        // El año en curso: lo que saldría en tu Renta, estimado o no.
        let mut v = Vista { prevision: Some(Prevision::Estimada { resultado: -310.0, supuestos: String::new() }), ..Default::default() };
        v.d.anio.y = 2026;
        assert_eq!(notas(&v), ["Tu Renta de 2026, estimada: ≈310 € a devolver"]);
        v.prevision = Some(Prevision::NoSe("Con actividad…".into()));
        assert_eq!(notas(&v), ["Tu Renta de 2026: en abril de 2027."]);
        // Un año acabado con la Renta pendiente: con qué se ha hecho la cuenta.
        let v = Vista { pendiente: Some("2026-06-30".into()), ..Default::default() };
        assert_eq!(notas(&v), ["Hasta entonces, con la Seguridad Social y lo apuntado."]);
    }
}
