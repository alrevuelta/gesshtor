//! Lo que comparten el Resumen y los Impuestos: en qué punto está cada modelo de un trimestre (presentado, en
//! plazo, vencido…), lo que se presentó y cómo se dice un resultado. Sin red: solo con lo que hay en `datos.json`.
use crate::dominio::fmt::DIF;
use crate::hacienda::{modelo_130, modelo_303};
use crate::dominio::{factura, fmt};
use crate::hacienda::{self, Modelo, Plazo, Trimestral};
use crate::dominio::{Casillas, Datos};
use crate::dominio::almacen;
use crate::tui::widgets::tema;
use ratatui::style::{Modifier, Style};
use std::path::{Path, PathBuf};


/* ================= estado de un modelo ================= */

/// En qué punto está un modelo de un periodo.
#[derive(Debug, Clone, PartialEq)]
pub enum Estado {
    /// Presentado el día que sea (ISO; vacío si solo se tiene el PDF leído).
    Presentado(String),
    /// No hay que presentarlo: «antes del alta», «después de la baja», «exento».
    NoToca(&'static str),
    /// El periodo aún no ha empezado.
    Futuro,
    EnCurso,
    /// Terminó el periodo y aún no se abre el plazo.
    Proximo,
    /// En plazo: los días que quedan.
    EnPlazo(i64),
    Vencido,
    /// Se pasó el plazo, pero en la app no hay nada de ese periodo (¿se presentó fuera?).
    SinDatos,
}

/// Lo que hace falta saber para el estado de un modelo.
#[derive(Debug, Clone, Copy)]
pub struct Caso<'a> {
    pub hoy: &'a str,
    /// Primer y último día del periodo.
    pub ini: &'a str,
    pub fin: &'a str,
    pub plazo_ini: &'a str,
    pub plazo_fin: &'a str,
    /// Fecha de presentación, si consta.
    pub presentado: Option<&'a str>,
    pub alta: &'a str,
    pub baja: &'a str,
    /// No obligado (exento del 130).
    pub exento: bool,
    /// Hay facturas o gastos del periodo.
    pub actividad: bool,
}

impl Caso<'_> {
    pub fn estado(&self) -> Estado {
        if let Some(f) = self.presentado {
            return Estado::Presentado(f.to_string());
        }
        if !self.alta.is_empty() && self.alta > self.fin {
            return Estado::NoToca("antes del alta");
        }
        let realta = !self.alta.is_empty() && self.alta > self.baja;
        if !self.baja.is_empty() && self.baja < self.ini && !realta {
            return Estado::NoToca("después de la baja");
        }
        if self.hoy < self.ini {
            return Estado::Futuro;
        }
        if self.exento {
            return Estado::NoToca("exento");
        }
        if self.hoy <= self.fin {
            return Estado::EnCurso;
        }
        if self.hoy < self.plazo_ini {
            return Estado::Proximo;
        }
        if self.hoy <= self.plazo_fin {
            return Estado::EnPlazo(fmt::dias_entre(self.hoy, self.plazo_fin));
        }
        if !self.actividad {
            return Estado::SinDatos;
        }
        Estado::Vencido
    }
}

impl Estado {
    /// No queda nada por hacer.
    pub fn hecho(&self) -> bool {
        matches!(self, Estado::Presentado(_) | Estado::NoToca(_))
    }

    /// «presentado el 15 oct», «en plazo hasta el 20 oct», «vencido el 20 jul», «aún no», «antes del alta»…
    pub fn texto(&self, plazo_ini: &str, plazo_fin: &str) -> String {
        match self {
            Estado::Presentado(f) if f.is_empty() => "presentado".into(),
            Estado::Presentado(f) => format!("presentado el {}", dia_mes(f)),
            Estado::NoToca(m) => m.to_string(),
            Estado::Futuro => "aún no".into(),
            Estado::EnCurso => "en curso".into(),
            Estado::Proximo => format!("plazo desde el {}", dia_mes(plazo_ini)),
            Estado::EnPlazo(_) => format!("en plazo hasta el {}", dia_mes(plazo_fin)),
            Estado::Vencido => format!("vencido el {}", dia_mes(plazo_fin)),
            Estado::SinDatos => "sin datos".into(),
        }
    }

    /// Para columnas estrechas: «✓ 15 oct», «en plazo», «vencido»…
    pub fn corto(&self) -> String {
        match self {
            Estado::Presentado(f) if f.is_empty() => "✓".into(),
            Estado::Presentado(f) => format!("✓ {}", dia_mes(f)),
            Estado::NoToca("exento") => "exento".into(),
            Estado::NoToca(_) => "no toca".into(),
            Estado::Futuro => "aún no".into(),
            Estado::EnCurso => "en curso".into(),
            Estado::Proximo => "próximo".into(),
            Estado::EnPlazo(_) => "en plazo".into(),
            Estado::Vencido => "vencido".into(),
            Estado::SinDatos => "sin datos".into(),
        }
    }

    pub fn estilo(&self) -> Style {
        match self {
            Estado::Presentado(_) => Style::new().fg(tema::BIEN),
            Estado::NoToca(_) | Estado::Futuro | Estado::SinDatos => tema::apagado(),
            Estado::EnCurso | Estado::Proximo => Style::new(),
            Estado::EnPlazo(d) if *d <= 7 => Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD),
            Estado::EnPlazo(_) => Style::new().fg(tema::acento()),
            Estado::Vencido => Style::new().fg(tema::MAL).add_modifier(Modifier::BOLD),
        }
    }

    /// Cuánto apremia (para quedarse con el peor de dos).
    fn urgencia(&self) -> u8 {
        match self {
            Estado::Vencido => 6,
            Estado::EnPlazo(_) => 5,
            Estado::Proximo => 4,
            Estado::EnCurso => 3,
            Estado::SinDatos => 2,
            Estado::Futuro => 1,
            Estado::Presentado(_) | Estado::NoToca(_) => 0,
        }
    }
}

/// Una sola frase para el 303 y el 130 de un trimestre: si van igual, una; si no, cada uno con lo suyo
/// (`corto`: más breve, para tablas estrechas: «✓ 15 oct», «vencido»).
pub fn estado_conjunto(e303: &Estado, e130: &Estado, p: &Plazo, corto: bool) -> (String, Style) {
    let t = |e: &Estado| e.texto(&p.ini, &p.fin);
    let t2 = |e: &Estado| if corto { e.corto() } else { t(e) };
    if e303.hecho() && e130.hecho() {
        // Lo que cuenta es la presentación (el 130 exento no dice nada nuevo: ya sale en su columna).
        let e = if matches!(e303, Estado::Presentado(_)) || !matches!(e130, Estado::Presentado(_)) { e303 } else { e130 };
        return (t2(e), e.estilo());
    }
    if std::mem::discriminant(e303) == std::mem::discriminant(e130) {
        return (t2(e303), e303.estilo());
    }
    if e130.hecho() {
        return (format!("303 {}", t2(e303)), e303.estilo());
    }
    if e303.hecho() {
        return (format!("130 {}", t2(e130)), e130.estilo());
    }
    let peor = if e303.urgencia() >= e130.urgencia() { e303 } else { e130 };
    (format!("303 {} · 130 {}", e303.corto(), e130.corto()), peor.estilo())
}

/// El plazo de un trimestre (1–4).
pub fn plazo(anio: i32, q: u32) -> Plazo {
    let mut v = hacienda::plazos_trimestres(anio);
    v.swap_remove(q.clamp(1, 4) as usize - 1)
}

/// El estado del 303 o el 130 de un trimestre, a fecha de `hoy`.
pub fn estado_modelo(d: &Datos, hoy: &str, anio: i32, q: u32, modelo: Trimestral) -> Estado {
    let q = q.clamp(1, 4);
    let (ini, fin) = fmt::periodo(anio, q);
    let p = plazo(anio, q);
    let pres = presentado(d, anio, q, modelo.modelo());
    let e = Caso {
        hoy,
        ini: &ini,
        fin: &fin,
        plazo_ini: &p.ini,
        plazo_fin: &p.fin,
        presentado: pres.as_deref(),
        alta: d.fechas_actividad().0,
        baja: d.fechas_actividad().1,
        exento: modelo == Trimestral::M130 && pres.is_none() && modelo_130::se_libra(d, anio, q),
        actividad: hay_actividad(d, &ini, &fin),
    }
    .estado();
    if pres.is_none() && !matches!(e, Estado::NoToca(_)) && hacienda::de_baja(d, &ini, &fin) {
        return Estado::NoToca("de baja en Hacienda");
    }
    e
}

/// El estado de la Renta de un año (se presenta de abril a junio del siguiente).
pub fn estado_renta(d: &Datos, hoy: &str, anio: i32) -> (Estado, String, String) {
    let (ini, fin) = (format!("{anio}-01-01"), format!("{anio}-12-31"));
    let (pi, pf) = hacienda::plazo_renta(anio);
    let pres = presentado(d, anio, 0, Modelo::M100);
    let e = Caso {
        hoy,
        ini: &ini,
        fin: &fin,
        plazo_ini: &pi,
        plazo_fin: &pf,
        presentado: pres.as_deref(),
        alta: d.fechas_actividad().0,
        baja: d.fechas_actividad().1,
        exento: false,
        actividad: hay_actividad(d, &ini, &fin),
    }
    .estado();
    (e, pi, pf)
}

/* ================= lo presentado ================= */

/// Si consta presentado (en la consulta a Hacienda o con su PDF leído), la fecha en ISO (vacía si no se sabe).
pub fn presentado(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> Option<String> {
    if let Some(x) = hacienda::presentada(d, anio, q, modelo) {
        return Some(x.fecha.chars().take(10).collect());
    }
    hacienda::consta(d, anio, q, modelo).then(String::new)
}

/// La fecha y hora de la presentación según Hacienda («2026-10-15T09:05:30»), si se consultó.
pub fn fecha_hacienda(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> Option<String> {
    hacienda::presentada(d, anio, q, modelo).map(|x| x.fecha).filter(|f| !f.is_empty())
}

/// Las casillas con las que se presentó, si se conocen, y de dónde salen.
pub fn presentadas(d: &Datos, anio: i32, q: u32, modelo: Modelo) -> Option<(Casillas, &'static str)> {
    let l = modelo.leido(d, anio, q).filter(|l| !l.casillas.is_empty())?;
    Some((l.casillas.clone(), "Casillas de Hacienda: los valores con los que lo presentaste."))
}

/// ¿El 303 negativo se pidió devolver? Según lo presentado; si no se sabe, en el 4T sí.
pub fn devolver303(d: &Datos, anio: i32, q: u32) -> bool {
    modelo_303::negativo(d, anio, q) == modelo_303::Negativo::Devolver
}

/// El PDF de lo presentado, si está en la carpeta de datos.
pub fn pdf(dir: &Path, d: &Datos, anio: i32, q: u32, modelo: Modelo) -> Option<PathBuf> {
    if let Some(p) = hacienda::presentada(d, anio, q, modelo).and_then(|x| hacienda::pdf_de(dir, d, &x)) {
        return Some(p);
    }
    if let Some(p) = modelo.leido(d, anio, q).and_then(|l| almacen::documento(dir, &l.pdf)) {
        return Some(p);
    }
    hacienda::pdf_periodo(dir, anio, q, modelo.codigo())
}

/* ================= actividad ================= */

/// ¿Hay facturas, gastos o cuotas de autónomo entre esas fechas? Si los hay y no consta nada presentado, el modelo sale
/// vencido: a la persona se le puede haber olvidado. Una regularización no cuenta: se carga meses después de lo que
/// regulariza y no dice que ese trimestre hubiera actividad.
pub fn hay_actividad(d: &Datos, ini: &str, fin: &str) -> bool {
    let es_gasto_o_cuota = |g: &&crate::dominio::gasto::Gasto| crate::dominio::gasto::recibo_de(g, d).is_none_or(|r| r.es_cuota());
    factura::emitidas(d).any(|f| fmt::en_rango(f.devengo(), ini, fin)) || d.gastos.iter().filter(es_gasto_o_cuota).any(|g| fmt::en_rango(&g.fecha, ini, fin))
}

/* ================= textos ================= */

/// «303», «303 y 130», «303, 130 y 390»
pub fn y_lista(v: &[&str]) -> String {
    match v {
        [] => String::new(),
        [a] => a.to_string(),
        [resto @ .., ultimo] => format!("{} y {ultimo}", resto.join(", ")),
    }
}

/// «2026-10-15» → «15/10»
pub fn dm(s: &str) -> String {
    fmt::fecha(s).map(|d| d.format("%d/%m").to_string()).unwrap_or_else(|| s.to_string())
}

/// «2026-08-31» → «31 ago»
pub fn dia_mes(s: &str) -> String {
    let t = fmt::fecha_corta(s);
    match t.rsplit_once(' ') {
        Some((a, _)) if fmt::fecha(s).is_some() => a.to_string(),
        _ => t,
    }
}

/// Un plazo, como en los bocetos: «del 1 al 20 oct», «del 1 ene al 1 feb 2027». El año solo sale si no es `anio`.
pub fn del_al(ini: &str, fin: &str, anio: i32) -> String {
    use chrono::Datelike;
    let (Some(a), Some(b)) = (fmt::fecha(ini), fmt::fecha(fin)) else { return rango(ini, fin) };
    let y = if b.year() == anio { String::new() } else { format!(" {}", b.year()) };
    let mes = |d: chrono::NaiveDate| fmt::MESES_C[d.month0() as usize];
    if (a.year(), a.month()) == (b.year(), b.month()) {
        format!("del {} al {} {}{y}", a.day(), b.day(), mes(b))
    } else {
        format!("del {} {} al {} {}{y}", a.day(), mes(a), b.day(), mes(b))
    }
}

/// «1 jul – 30 sep 2026» (con el año en los dos lados si no es el mismo).
pub fn rango(ini: &str, fin: &str) -> String {
    if fmt::anio_de(ini) == fmt::anio_de(fin) {
        format!("{} – {}", dia_mes(ini), fmt::fecha_corta(fin))
    } else {
        format!("{} – {}", fmt::fecha_corta(ini), fmt::fecha_corta(fin))
    }
}

/// «hoy», «mañana», «quedan 5 días»
pub fn dias_txt(n: i64) -> String {
    match n {
        ..=0 => "hoy".into(),
        1 => "mañana".into(),
        n => format!("quedan {n} días"),
    }
}

fn importe(x: f64, corto: bool) -> String {
    if corto { fmt::eur0(x) } else { fmt::eur(x) }
}

/// El resultado del 303 (casilla 71): «56,70 € a compensar», «120,00 € a ingresar», «0 €».
/// `corto`: sin céntimos y abreviado, para las tablas estrechas.
pub fn texto303(v: f64, devolver: bool, corto: bool) -> String {
    if v > DIF {
        format!("{} {}", importe(v, corto), if corto { "a pagar" } else { "a ingresar" })
    } else if v < -DIF {
        let que = if devolver {
            "a devolver"
        } else if corto {
            "a comp."
        } else {
            "a compensar"
        };
        format!("{} {que}", importe(-v, corto))
    } else {
        "0 €".into()
    }
}

/// El resultado del 130 (casilla 19): «1.234,56 € a ingresar», «0 € (negativa)».
pub fn texto130(v: f64, corto: bool) -> String {
    if v > DIF {
        format!("{} {}", importe(v, corto), if corto { "a pagar" } else { "a ingresar" })
    } else if v < -DIF {
        if corto { "0 € (neg.)".into() } else { "0 € (negativa)".into() }
    } else {
        "0 €".into()
    }
}

/// Del color de la sección si sale a pagar, verde si es a tu favor y en gris si es cero.
pub fn estilo_resultado(v: f64) -> Style {
    if v > DIF {
        Style::new().fg(tema::acento())
    } else if v < -DIF {
        Style::new().fg(tema::BIEN)
    } else {
        tema::apagado()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caso<'a>(hoy: &'a str, presentado: Option<&'a str>) -> Caso<'a> {
        // 3T 2026: del 1 de julio al 30 de septiembre; se presenta del 1 al 20 de octubre.
        Caso {
            hoy,
            ini: "2026-07-01",
            fin: "2026-09-30",
            plazo_ini: "2026-10-01",
            plazo_fin: "2026-10-20",
            presentado,
            alta: "",
            baja: "",
            exento: false,
            actividad: true,
        }
    }

    #[test]
    fn estado_segun_la_fecha() {
        assert_eq!(caso("2026-06-15", None).estado(), Estado::Futuro);
        assert_eq!(caso("2026-08-15", None).estado(), Estado::EnCurso);
        assert_eq!(caso("2026-10-15", None).estado(), Estado::EnPlazo(5));
        assert_eq!(caso("2026-10-20", None).estado(), Estado::EnPlazo(0));
        assert_eq!(caso("2026-10-21", None).estado(), Estado::Vencido);
        assert_eq!(caso("2026-10-21", Some("2026-10-15")).estado(), Estado::Presentado("2026-10-15".into()));
        assert_eq!(Caso { actividad: false, ..caso("2026-11-01", None) }.estado(), Estado::SinDatos);
        assert_eq!(Caso { plazo_ini: "2026-10-05", ..caso("2026-10-02", None) }.estado(), Estado::Proximo);
    }

    #[test]
    fn estado_con_alta_baja_y_exento() {
        assert_eq!(Caso { alta: "2026-10-01", ..caso("2026-10-15", None) }.estado(), Estado::NoToca("antes del alta"));
        assert_eq!(Caso { baja: "2026-05-01", ..caso("2026-10-15", None) }.estado(), Estado::NoToca("después de la baja"));
        // De baja y vuelta a darse de alta antes del trimestre: sí toca.
        assert_eq!(Caso { baja: "2026-02-01", alta: "2026-05-01", ..caso("2026-10-15", None) }.estado(), Estado::EnPlazo(5));
        assert_eq!(Caso { exento: true, ..caso("2026-10-15", None) }.estado(), Estado::NoToca("exento"));
        // Exento, pero aún no ha empezado: primero «aún no».
        assert_eq!(Caso { exento: true, ..caso("2026-06-01", None) }.estado(), Estado::Futuro);
    }

    #[test]
    fn textos_del_estado() {
        let p = plazo(2026, 3);
        assert_eq!(Estado::Presentado("2026-10-15".into()).texto(&p.ini, &p.fin), "presentado el 15 oct");
        assert_eq!(Estado::EnPlazo(3).texto(&p.ini, &p.fin), "en plazo hasta el 20 oct");
        assert_eq!(Estado::NoToca("antes del alta").texto(&p.ini, &p.fin), "antes del alta");
        assert_eq!(Estado::Vencido.texto(&p.ini, &p.fin), "vencido el 20 oct");
        assert_eq!(Estado::Futuro.texto(&p.ini, &p.fin), "aún no");
        let pres = Estado::Presentado("2026-10-15".into());
        let (t, _) = estado_conjunto(&pres, &Estado::NoToca("exento"), &p, false);
        assert_eq!(t, "presentado el 15 oct");
        let (t, _) = estado_conjunto(&pres, &Estado::Vencido, &p, false);
        assert_eq!(t, "130 vencido el 20 oct");
        let (t, _) = estado_conjunto(&pres, &Estado::EnPlazo(2), &p, true);
        assert_eq!(t, "130 en plazo");
        let (t, _) = estado_conjunto(&pres, &Estado::NoToca("exento"), &p, true);
        assert_eq!(t, "✓ 15 oct");
        let (t, _) = estado_conjunto(&Estado::EnCurso, &Estado::EnCurso, &p, false);
        assert_eq!(t, "en curso");
        let (t, _) = estado_conjunto(&Estado::Vencido, &Estado::SinDatos, &p, false);
        assert_eq!(t, "303 vencido · 130 sin datos");
    }

    #[test]
    fn textos_de_resultados() {
        assert_eq!(texto303(120.0, false, false), "120,00 € a ingresar");
        assert_eq!(texto303(-56.7, false, false), "56,70 € a compensar");
        assert_eq!(texto303(-56.7, true, false), "56,70 € a devolver");
        assert_eq!(texto303(-56.7, false, true), "57 € a comp.");
        assert_eq!(texto303(0.001, false, false), "0 €");
        assert_eq!(texto130(1234.56, false), "1.234,56 € a ingresar");
        assert_eq!(texto130(-10.0, false), "0 € (negativa)");
        assert_eq!(y_lista(&["303", "130", "390"]), "303, 130 y 390");
        assert_eq!(y_lista(&["303"]), "303");
        assert_eq!(dias_txt(1), "mañana");
        assert_eq!(rango("2026-07-01", "2026-09-30"), "1 jul – 30 sep 2026");
        assert_eq!(rango("2026-12-31", "2027-01-30"), "31 dic 2026 – 30 ene 2027");
        assert_eq!(del_al("2026-10-01", "2026-10-20", 2026), "del 1 al 20 oct");
        assert_eq!(del_al("2027-01-01", "2027-02-01", 2026), "del 1 ene al 1 feb 2027");
    }
}
