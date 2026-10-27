//! Lo de la pantalla de bienvenida: quién eres (y tu certificado), qué eres ahora (por cuenta ajena, por tu
//! cuenta o las dos cosas), tu última nómina, tu alta, actividad y cuota de autónomo, lo último presentado y el
//! próximo plazo.
use crate::dominio::tgss::{Actividad, Cotizacion, Mutua};
use crate::dominio::Datos;
use crate::dominio::fmt::{self, hoy};
use crate::red::certificado::Certificado;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inicio {
    /// «Ana» (del certificado o del perfil).
    pub nombre: String,
    /// Ahora mismo: con nómina reciente y de alta como autónomo.
    pub empleo_ahora: bool,
    pub autonomo_ahora: bool,
    pub empleo: Option<UltimaNomina>,
    pub autonomo: Option<Autonomo>,
    /// Lo último presentado en Hacienda: (modelos, periodo «3T», año, fecha de presentación).
    pub ultimo_presentado: Option<(Vec<String>, String, i32, String)>,
    /// El próximo plazo trimestral sin presentar: («4T 2026», desde, hasta).
    pub proximo_plazo: Option<(String, String, String)>,
}

/// La última nómina que tiene la Seguridad Social.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UltimaNomina {
    /// «2025-12»
    pub mes: String,
    pub empresas: Vec<String>,
    /// El último mes consultado (para saber si puede haber más recientes).
    pub consultado_hasta: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Autonomo {
    /// Tu alta y tu baja de ahora (`Datos::alta_autonomo`).
    pub alta: String,
    pub baja: String,
    /// Lo que figura en la Seguridad Social (las que dicen qué actividad es).
    pub actividades: Vec<Actividad>,
    pub cotizacion: Cotizacion,
    /// Tu última cuota y de qué mes: la que dice la Seguridad Social o, si no, la del último recibo.
    pub cuota: Option<(f64, String)>,
    pub mutua: Mutua,
}

/// «2025-12» del mes de hace `n` meses.
fn hace_meses(n: u32) -> String {
    fmt::mes_mas(fmt::ym(&hoy()), -(n as i32))
}

pub fn inicio(d: &Datos, cert: Option<&Certificado>) -> Inicio {
    let mut o = Inicio::default();
    // Quién.
    // Un nombre de pila de una letra no es un nombre.
    let pila = d.perfil.nombre_pila.trim().to_string();
    let pila = if pila.chars().count() < 2 { String::new() } else { pila };
    o.nombre = match cert {
        Some(c) => c.saludo(),
        None if !pila.is_empty() => crate::red::certificado::titulo(pila.split_whitespace().next().unwrap_or("")),
        // Sin nombre de pila no se sabe qué palabra es el nombre (a veces van primero los apellidos): entero.
        None if d.perfil.nombre.trim().chars().count() > 2 => d.perfil.nombre.trim().to_string(),
        None => String::new(),
    };

    // Empleo: la última nómina con liquidación.
    let meses = &d.tgss.nomina.meses;
    if let Some((ym, l)) = meses.iter().rev().find(|(_, l)| !l.is_empty()) {
        let mut empresas: Vec<String> = Vec::new();
        for x in l {
            if !x.empresa.is_empty() && !empresas.contains(&x.empresa) {
                empresas.push(x.empresa.clone());
            }
        }
        let consultado_hasta = meses.keys().next_back().cloned().unwrap_or_default();
        o.empleo_ahora = ym.as_str() >= hace_meses(3).as_str();
        o.empleo = Some(UltimaNomina { mes: ym.clone(), empresas, consultado_hasta });
    } else if !d.nominas.is_empty() {
        o.empleo_ahora = d.nominas.iter().any(|n| n.anio == fmt::anio_actual());
    }

    // Autónomo: la Seguridad Social manda; si no, el perfil.
    let ss = &d.tgss;
    let (alta, baja) = d.alta_autonomo();
    let (alta, baja) = (alta.to_string(), baja.to_string());
    if !alta.is_empty() || ss.reta {
        o.autonomo_ahora = !alta.is_empty() && baja.is_empty();
        let actividades = ss.actividades.iter().filter(|a| !a.tipo.trim().is_empty()).cloned().collect();
        let c = &ss.cotizacion;
        let ultimo = ss.ultima_cuota();
        let cuota = c.ultima_cuota.map(|x| (x, c.ultima_cuota_mes.clone())).or_else(|| ultimo.map(|r| (r.importe, fmt::nombre_mes(&r.periodo))));
        o.autonomo = Some(Autonomo { alta, baja, actividades, cotizacion: c.clone(), cuota, mutua: ss.mutua.clone() });
    }

    // Hacienda: lo último presentado (de los trimestrales) y el próximo plazo.
    let hoy_y = fmt::anio_actual();
    let mut decl: Vec<crate::hacienda::Decl> = (hoy_y - 1..=hoy_y).flat_map(|y| crate::hacienda::declaraciones(d, y)).filter(|x| x.trimestre().is_some()).collect();
    decl.sort_by(|a, b| (a.ejercicio, &a.periodo, &a.fecha).cmp(&(b.ejercicio, &b.periodo, &b.fecha)));
    if let Some(u) = decl.last() {
        let mut modelos: Vec<String> = decl.iter().filter(|x| x.ejercicio == u.ejercicio && x.periodo == u.periodo).map(|x| x.modelo.clone()).collect();
        modelos.sort();
        modelos.dedup();
        o.ultimo_presentado = Some((modelos, u.periodo.clone(), u.ejercicio, u.fecha.get(..10).unwrap_or("").to_string()));
    }
    if o.autonomo_ahora {
        let h = hoy();
        // Hecho: con el 303 y el 130 presentados (o sin 130, si te libras).
        use crate::hacienda::{consta, modelo_130, Modelo};
        let hecho = |y: i32, q: u32| consta(d, y, q, Modelo::M303) && (consta(d, y, q, Modelo::M130) || modelo_130::se_libra(d, y, q));
        o.proximo_plazo = (hoy_y - 1..=hoy_y)
            .flat_map(crate::hacienda::plazos_trimestres)
            .filter(|p| p.fin.as_str() >= h.as_str() && !hecho(p.anio, p.q))
            .map(|p| (format!("{}T {}", p.q, p.anio), p.ini, p.fin))
            .next();
    }
    o
}


