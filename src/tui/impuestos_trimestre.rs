//! Lo que comparten el Resumen y los Impuestos: en qué punto está cada modelo de un trimestre (presentado, en
//! plazo, vencido…), lo que se presentó y cómo se dice un resultado. Sin red: solo con lo que hay en `datos.json`.
use crate::dominio::fmt::DIF;
use crate::hacienda::modelo::Memo;
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

    /// «presentado el 15/10», «en plazo hasta 20/10», «vencido el 20/07», «aún no», «antes del alta»…
    pub fn texto(&self, plazo_ini: &str, plazo_fin: &str) -> String {
        match self {
            Estado::Presentado(f) if f.is_empty() => "presentado".into(),
            Estado::Presentado(f) => format!("presentado el {}", dm(f)),
            Estado::NoToca(m) => m.to_string(),
            Estado::Futuro => "aún no".into(),
            Estado::EnCurso => "en curso".into(),
            Estado::Proximo => format!("plazo desde el {}", dm(plazo_ini)),
            Estado::EnPlazo(_) => format!("en plazo hasta {}", dm(plazo_fin)),
            Estado::Vencido => format!("vencido el {}", dm(plazo_fin)),
            Estado::SinDatos => "sin datos".into(),
        }
    }

    /// Para columnas estrechas: «✓ 15/10», «en plazo», «vencido»…
    pub fn corto(&self) -> String {
        match self {
            Estado::Presentado(f) if f.is_empty() => "✓".into(),
            Estado::Presentado(f) => format!("✓ {}", dm(f)),
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
            Estado::EnPlazo(_) => Style::new().fg(tema::ACENTO),
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
/// (`corto`: más breve, para tablas estrechas).
pub fn estado_conjunto(e303: &Estado, e130: &Estado, p: &Plazo, corto: bool) -> (String, Style) {
    let t = |e: &Estado| e.texto(&p.ini, &p.fin);
    let t2 = |e: &Estado| if corto { e.corto() } else { t(e) };
    if e303.hecho() && e130.hecho() {
        // Lo que cuenta es la presentación (el 130 exento no dice nada nuevo: ya sale en su columna).
        let e = if matches!(e303, Estado::Presentado(_)) || !matches!(e130, Estado::Presentado(_)) { e303 } else { e130 };
        return (t(e), e.estilo());
    }
    if std::mem::discriminant(e303) == std::mem::discriminant(e130) {
        return (t(e303), e303.estilo());
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
    Caso {
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
    .estado()
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

/* ================= actividad y pendientes ================= */

/// ¿Hay facturas o gastos entre esas fechas?
pub fn hay_actividad(d: &Datos, ini: &str, fin: &str) -> bool {
    factura::emitidas(d).any(|f| fmt::en_rango(f.devengo(), ini, fin)) || d.gastos.iter().any(|g| fmt::en_rango(&g.fecha, ini, fin))
}

/// Un trimestre ya empezado con algo por presentar.
#[derive(Debug, Clone)]
pub struct Pendiente {
    pub plazo: Plazo,
    pub pres303: bool,
    /// Presentado o exento.
    pub pres130: bool,
    /// Casilla 71 del 303 y 19 del 130.
    pub r303: f64,
    pub r130: f64,
    pub vencido: bool,
    /// En plazo ahora mismo.
    pub abierto: bool,
}

impl Pendiente {
    /// Los modelos que faltan: «303 y 130», «303, 130 y 390».
    pub fn modelos(&self) -> String {
        let v: Vec<&str> = self
            .plazo
            .modelos
            .iter()
            .filter(|m| match m {
                Modelo::M303 => !self.pres303,
                Modelo::M130 => !self.pres130,
                Modelo::M100 | Modelo::M390 => true,
            })
            .map(|m| m.codigo())
            .collect();
        y_lista(&v)
    }
    /// Lo que habrá que pagar (lo presentado ya no cuenta).
    pub fn a_pagar(&self) -> f64 {
        let iva = if self.pres303 { 0.0 } else { self.r303.max(0.0) };
        let irpf = if self.pres130 { 0.0 } else { self.r130.max(0.0) };
        fmt::r2(iva + irpf)
    }
}

/// Los trimestres de un año ya empezados que tienen algo sin presentar. Los vencidos sin facturas ni gastos no
/// salen (seguramente se presentaron antes de usar la app).
pub fn pendientes(d: &Datos, hoy: &str, anio: i32, memo: &mut Memo) -> Vec<Pendiente> {
    let mut v = Vec::new();
    for p in hacienda::plazos_trimestres(anio) {
        let (ini, fin) = fmt::periodo(anio, p.q);
        if ini.as_str() > hoy {
            continue;
        }
        let pres303 = presentado(d, anio, p.q, Modelo::M303).is_some();
        let exento = modelo_130::se_libra(d, anio, p.q);
        let pres130 = presentado(d, anio, p.q, Modelo::M130).is_some() || exento;
        if pres303 && pres130 {
            continue;
        }
        let vencido = hoy > p.fin.as_str();
        if vencido && !hay_actividad(d, &ini, &fin) {
            continue;
        }
        let r303 = Trimestral::M303.resultado_de(&Trimestral::M303.cuenta(d, anio, p.q, memo));
        let r130 = Trimestral::M130.resultado_de(&Trimestral::M130.cuenta(d, anio, p.q, memo));
        let abierto = hoy >= p.ini.as_str() && hoy <= p.fin.as_str();
        v.push(Pendiente { vencido, abierto, pres303, pres130, r303, r130, plazo: p });
    }
    v
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

/// Rojo si sale a pagar, verde si es a tu favor.
pub fn estilo_resultado(v: f64) -> Style {
    if v.abs() <= DIF { tema::apagado() } else { tema::importe(v) }
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
        assert_eq!(Estado::Presentado("2026-10-15".into()).texto(&p.ini, &p.fin), "presentado el 15/10");
        assert_eq!(Estado::EnPlazo(3).texto(&p.ini, &p.fin), "en plazo hasta 20/10");
        assert_eq!(Estado::NoToca("antes del alta").texto(&p.ini, &p.fin), "antes del alta");
        assert_eq!(Estado::Vencido.texto(&p.ini, &p.fin), "vencido el 20/10");
        assert_eq!(Estado::Futuro.texto(&p.ini, &p.fin), "aún no");
        let pres = Estado::Presentado("2026-10-15".into());
        let (t, _) = estado_conjunto(&pres, &Estado::NoToca("exento"), &p, false);
        assert_eq!(t, "presentado el 15/10");
        let (t, _) = estado_conjunto(&pres, &Estado::Vencido, &p, false);
        assert_eq!(t, "130 vencido el 20/10");
        let (t, _) = estado_conjunto(&pres, &Estado::EnPlazo(2), &p, true);
        assert_eq!(t, "130 en plazo");
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
    }
}
