//! Los modelos con casillas en formulario (303, 130, 390): lo que tiene que decir cada uno de sí mismo
//! (`Formulario`, en su fichero) y cómo se lee uno cualquiera de su PDF (`leer`).
//!
//! Leer es emparejar cada número de casilla con el importe que tiene a su derecha (ver `generico`). Antes de dar
//! nada por bueno: el PDF tiene que ser de ese modelo y periodo, y lo leído tiene que cuadrar con las sumas del
//! propio modelo.
//!
//! Para añadir un modelo: un fichero `modelo_XXX.rs` con un `impl Formulario`, su caso en `Modelo` y su línea en
//! `lectura::leer_pdf`. El compilador no deja olvidar nada de lo que tiene que decir.
use super::generico::{self, Hoja};
use super::modelo::{Fallo, Modelo, Periodicidad, Periodo};
use super::pdf::{Documento, Palabra};
use crate::dominio::hacienda::Tipo;
use crate::dominio::{Casillas, FormularioLeido, Valor};
use crate::dominio::PRIMER_ANIO;

/// Lo que tiene que decir de sí mismo un modelo con casillas en formulario para que la app lo lea.
pub trait Formulario {
    const MODELO: Modelo;
    const PERIODICIDAD: Periodicidad;
    /// Qué casillas tiene.
    const CASILLAS: QueCasillas;

    /// Las sumas del propio modelo: cada casilla con lo que debe valer según las demás (las que no están valen 0).
    /// Si alguna no cuadra, la lectura no vale.
    fn reglas(c: &Casillas) -> Vec<(u32, f64)>;

    /// El tipo de una declaración que la portada da por negativa o a cero («NEGATIVA», «SIN ACTIVIDAD»…).
    fn sin_resultado(_c: &Casillas) -> Tipo {
        Tipo::Cero
    }
}

/// Qué casillas tiene un modelo.
pub enum QueCasillas {
    /// Las de sus diseños de registro oficiales, del más viejo al más nuevo: en cada periodo, las del último que
    /// empezó antes. Las que no se ven en el PDF valen 0.
    Disenos(&'static [Diseno]),
    /// Las que se vean en el PDF (la app no usa diseño de registro).
    LasQueSeVean,
}

/// Un diseño de registro: desde qué periodo se usa (año × 100 + último mes del periodo) y sus casillas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diseno {
    pub id: &'static str,
    pub desde: u32,
    pub casillas: &'static [u32],
}

/// El diseño de registro de un periodo (mes = último mes del periodo).
fn diseno(lista: &'static [Diseno], ejercicio: i32, mes: u32) -> Option<&'static Diseno> {
    let p = ejercicio as i64 * 100 + mes as i64;
    lista.iter().rev().find(|d| d.desde as i64 <= p)
}

/// Lee el PDF de un modelo de un ejercicio y periodo. Error si el PDF no es de ese modelo o periodo, o si lo leído no
/// cuadra.
pub fn leer<F: Formulario>(doc: &Documento, ejercicio: i32, periodo: &Periodo) -> Result<FormularioLeido, Fallo> {
    let modelo = F::MODELO;
    if ejercicio < PRIMER_ANIO {
        return Err(Fallo::Anterior(modelo));
    }
    // El último mes del periodo: con él se sabe su diseño.
    let mes = match (F::PERIODICIDAD, periodo) {
        (Periodicidad::Trimestral, Periodo::Trimestre(t)) => t.ultimo_mes(),
        (Periodicidad::Anual, Periodo::Anual) => 12,
        (periodicidad, _) => return Err(Fallo::Periodo { modelo, periodicidad, periodo: periodo.clone() }),
    };
    let validas: Option<&[u32]> = match F::CASILLAS {
        QueCasillas::Disenos(lista) => Some(diseno(lista, ejercicio, mes).ok_or_else(|| Fallo::SinDiseno { modelo, ejercicio, periodo: periodo.clone() })?.casillas),
        QueCasillas::LasQueSeVean => None,
    };
    comprobar(doc, modelo, ejercicio, periodo)?;

    let vale = |n: &str| n.parse::<u32>().is_ok_and(|n| validas.is_none_or(|v| v.contains(&n)));
    let mut casillas: Casillas = validas.unwrap_or_default().iter().map(|&n| (n, 0.0)).collect();
    let mut leidas = 0;
    let letra = generico::letra_rellenada(doc);
    for p in &doc.paginas {
        for par in Hoja::de(p, &letra).pares_formulario(&vale) {
            if vale(&par.numero) {
                casillas.insert(par.numero.parse().unwrap_or_default(), par.importe);
                leidas += 1;
            }
        }
    }
    if leidas == 0 {
        return Err(Fallo::SinCasillas);
    }
    let mal = descuadres::<F>(&casillas);
    if !mal.is_empty() {
        return Err(Fallo::NoCuadra { modelo, casillas: mal });
    }
    let tipo = tipo::<F>(doc, &casillas);
    Ok(FormularioLeido { casillas, tipo, pdf: String::new(), justificante: generico::justificante(doc, modelo.codigo()) })
}

/// Las casillas que no cuadran con las sumas del propio modelo (para no fiarse de una lectura mal hecha).
pub fn descuadres<F: Formulario>(c: &Casillas) -> Vec<u32> {
    F::reglas(c).into_iter().filter(|&(n, debe)| (c.v(n) - debe).abs() > 0.015).map(|(n, _)| n).collect()
}

/// La suma de unas casillas.
pub fn suma(c: &Casillas, casillas: &[u32]) -> f64 {
    casillas.iter().map(|&n| c.v(n)).sum()
}

/// El PDF es de ese modelo y, si se ve el devengo («Ejercicio 2026 Período 3T»), de ese ejercicio y periodo.
fn comprobar(doc: &Documento, modelo: Modelo, ejercicio: i32, periodo: &Periodo) -> Result<(), Fallo> {
    match generico::modelo(doc) {
        Some(m) if m == modelo.codigo() => {}
        es => return Err(Fallo::OtroModelo { pedido: modelo, es }),
    }
    if let Some((a, p)) = devengo(doc)
        && (a != ejercicio || p.as_ref().is_some_and(|p| p != periodo))
    {
        let pedido = Some(periodo.clone()).filter(|p| *p != Periodo::Anual);
        return Err(Fallo::OtroPeriodo { pedido: (ejercicio, pedido), es: (a, p) });
    }
    Ok(())
}

/// El devengo que se ve en el impreso (no en la portada): un año seguido de su periodo («2026 Período 3T»,
/// «2023 1T») o tras «Ejercicio» («Ejercicio 2023»).
fn devengo(doc: &Documento) -> Option<(i32, Option<Periodo>)> {
    let periodo = |s: &str| Some(Periodo::de(s)).filter(|p| !s.is_empty() && !matches!(p, Periodo::Otro(_)));
    let portada = generico::portada(doc).map(|p| p.numero);
    for p in doc.paginas.iter().filter(|p| Some(p.numero) != portada) {
        let ps: Vec<&Palabra> = p.palabras().collect();
        for (i, w) in ps.iter().enumerate() {
            let Some(a) = w.texto.parse::<i32>().ok().filter(|a| w.texto.len() == 4 && (2000..2100).contains(a)) else { continue };
            let sig = |k: usize| ps.get(i + k).map(|w| w.texto.as_str()).unwrap_or("");
            let per = periodo(sig(1)).or_else(|| if sig(1).starts_with("Per") { periodo(sig(2)) } else { None });
            let tras_ejercicio = i > 0 && ps[i - 1].texto == "Ejercicio";
            if per.is_some() || tras_ejercicio {
                return Some((a, per));
            }
        }
    }
    None
}

/// Cómo se presentó, según la portada. None si no se ve.
fn tipo<F: Formulario>(doc: &Documento, c: &Casillas) -> Option<Tipo> {
    let p = generico::portada(doc)?;
    let t = p.texto().to_uppercase();
    let hay = |s: &str| t.contains(s);
    if hay("A DEVOLVER") || hay("DEVOLUCI") {
        Some(Tipo::Devolver)
    } else if hay("A COMPENSAR") {
        Some(Tipo::Compensar)
    } else if hay("DOMICILIA") {
        Some(Tipo::Domiciliado)
    } else if hay("RECONOCIMIENTO DE DEUDA") {
        Some(Tipo::ReconocimientoDeuda)
    } else if hay("INGRESAR") || hay("NRC") {
        Some(Tipo::Ingreso)
    } else if hay("NEGATIVA") || hay("SIN ACTIVIDAD") || hay("RESULTADO CERO") {
        Some(F::sin_resultado(c))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hacienda::{modelo_130::M130, modelo_303::M303, modelo_390::M390};

    fn disenos<F: Formulario>() -> &'static [Diseno] {
        match F::CASILLAS {
            QueCasillas::Disenos(l) => l,
            QueCasillas::LasQueSeVean => &[],
        }
    }

    #[test]
    fn disenos_por_periodo() {
        let id = |a, m| diseno(disenos::<M303>(), a, m).unwrap().id;
        assert_eq!(id(2020, 3), "e20");
        assert_eq!(id(2021, 6), "e21a");
        assert_eq!(id(2021, 9), "e21b");
        assert_eq!(id(2024, 6), "e24a");
        assert_eq!(id(2024, 9), "e24b");
        assert_eq!(id(2026, 9), "e26");
        assert_eq!(diseno(disenos::<M130>(), 2026, 12).unwrap().casillas.len(), 19);
        // Todos los diseños son de 2020 en adelante (el primero, el que se usa en 2020).
        assert!(disenos::<M303>().iter().skip(1).all(|d| d.desde > PRIMER_ANIO as u32 * 100));
        assert!(disenos::<M390>().is_empty());
    }

    #[test]
    fn fuera_de_lo_que_lee() {
        let d = Documento::default();
        let p = Periodo::de;
        assert_eq!(leer::<M303>(&d, 2019, &p("4T")).unwrap_err().to_string(), "La app lee declaraciones desde 2020.");
        assert_eq!(leer::<M303>(&d, 2024, &p("0A")).unwrap_err().to_string(), "La app lee el 303 trimestral (1T…4T), no el del periodo «0A».");
        assert_eq!(leer::<M390>(&d, 2024, &p("3T")).unwrap_err().to_string(), "El 390 es anual: no tiene periodo «3T».");
        assert_eq!(leer::<M303>(&d, 2024, &p("2T")).unwrap_err().to_string(), "El PDF no es de un modelo 303.");
    }
}
