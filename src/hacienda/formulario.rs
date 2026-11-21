//! Los modelos con casillas en formulario (303, 130, 390): lo que tiene que decir cada uno de sí mismo
//! (`Formulario`, en su fichero) y cómo se lee uno cualquiera de su PDF (`leer`).
//!
//! Leer es emparejar cada número de casilla con el importe que tiene a su derecha (ver `generico`). Antes de dar
//! nada por bueno: el PDF tiene que ser de ese modelo y periodo (y decirlo: su devengo), tienen que verse sus anclas
//! (las casillas que enseña todo PDF de ese modelo), lo leído tiene que cuadrar con las sumas del propio modelo y, si
//! tiene portada, tiene que entenderse cómo se presentó. Si no, es un fallo (`Fallo`), nunca casillas a 0.
//!
//! Para añadir un modelo: un fichero `modelo_XXX.rs` con un `impl Formulario`, su caso en `Modelo` y su línea en
//! `lectura::leer_pdf`. El compilador no deja olvidar nada de lo que tiene que decir.
use super::generico::{self, Hoja};
use super::modelo::{Descuadre, Fallo, Modelo, Periodicidad, Periodo};
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
    /// Las que enseña todo PDF presentado de este modelo (su resultado…): si no se ve alguna, el PDF ha cambiado.
    const ANCLAS: &'static [u32];
    /// ¿Dice su portada cómo se presentó (a ingresar, a compensar…)? (el 390, no)
    const TIPO_EN_PORTADA: bool = true;

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

    let numero = |n: &str| n.parse::<u32>().ok().filter(|n| validas.is_none_or(|v| v.contains(n)));
    let mut casillas: Casillas = validas.unwrap_or_default().iter().map(|&n| (n, 0.0)).collect();
    let mut vistas = std::collections::BTreeSet::new();
    let letra = generico::letra_rellenada(doc);
    for p in &doc.paginas {
        for par in Hoja::de(p, &letra).pares_formulario(&|n| numero(n).is_some()) {
            if let Some(n) = numero(&par.numero) {
                casillas.insert(n, par.importe);
                vistas.insert(n);
            }
        }
    }
    if vistas.is_empty() {
        return Err(Fallo::SinCasillas);
    }
    let faltan: Vec<u32> = F::ANCLAS.iter().filter(|n| !vistas.contains(n)).copied().collect();
    if !faltan.is_empty() {
        return Err(Fallo::FaltaCasilla { modelo, faltan, vistas: vistas.into_iter().collect() });
    }
    let mal = que_no_cuadra::<F>(&casillas);
    if !mal.is_empty() {
        return Err(Fallo::NoCuadra { modelo, casillas: mal });
    }
    let tipo = tipo::<F>(doc, &casillas);
    if F::TIPO_EN_PORTADA && tipo.is_none() && generico::portada(doc).is_some() {
        return Err(Fallo::SinTipo(modelo));
    }
    Ok(FormularioLeido { casillas, tipo, pdf: String::new(), justificante: generico::justificante(doc, modelo.codigo()) })
}

/// Las casillas que no cuadran con las sumas del propio modelo (para no fiarse de una lectura mal hecha).
pub fn descuadres<F: Formulario>(c: &Casillas) -> Vec<u32> {
    que_no_cuadra::<F>(c).iter().map(|d| d.casilla).collect()
}

/// Lo mismo, con lo leído y lo que debería valer cada una.
fn que_no_cuadra<F: Formulario>(c: &Casillas) -> Vec<Descuadre> {
    F::reglas(c).into_iter().filter(|&(n, debe)| (c.v(n) - debe).abs() > 0.015).map(|(casilla, debe)| Descuadre { casilla, leido: c.v(casilla), debe }).collect()
}

/// La suma de unas casillas.
pub fn suma(c: &Casillas, casillas: &[u32]) -> f64 {
    casillas.iter().map(|&n| c.v(n)).sum()
}

/// El PDF es de ese modelo y su devengo («Ejercicio 2026 Período 3T»), de ese ejercicio y periodo.
fn comprobar(doc: &Documento, modelo: Modelo, ejercicio: i32, periodo: &Periodo) -> Result<(), Fallo> {
    match generico::modelo(doc) {
        Some(m) if m == modelo.codigo() => {}
        es => return Err(Fallo::OtroModelo { pedido: modelo, es }),
    }
    let (a, p) = devengo(doc).ok_or(Fallo::SinDevengo(modelo))?;
    if a != ejercicio || p.as_ref().is_some_and(|p| p != periodo) {
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

    /// Un 303 inventado: «Modelo 303», el devengo y unas casillas con su importe (en otra letra que los números).
    fn un_303(casillas: &[(&str, &str)], portada: Option<&str>) -> Documento {
        use crate::hacienda::pdf::{Linea, Pagina};
        let palabra = |t: &str, x0: f64, top: f64, fuente: &str| Palabra { texto: t.into(), x0, x1: x0 + 20.0, top, bottom: top + 8.0, fuente: fuente.into(), tamano: 8.0 };
        let linea = |ps: Vec<Palabra>| Linea { top: ps[0].top, bottom: ps[0].bottom, x0: ps[0].x0, x1: 500.0, palabras: ps };
        let mut lineas = vec![
            linea(vec![palabra("Modelo", 40.0, 20.0, "Impreso"), palabra("303", 70.0, 20.0, "Impreso")]),
            linea(vec![palabra("Ejercicio", 40.0, 40.0, "Impreso"), palabra("2025", 90.0, 40.0, "Impreso"), palabra("Período", 120.0, 40.0, "Impreso"), palabra("1T", 160.0, 40.0, "Impreso")]),
        ];
        for (i, (n, x)) in casillas.iter().enumerate() {
            let y = 60.0 + 12.0 * i as f64;
            lineas.push(linea(vec![palabra("Casilla", 40.0, y, "Impreso"), palabra(n, 300.0, y, "Impreso"), palabra(x, 400.0, y, "Rellena")]));
        }
        let mut paginas = vec![Pagina { numero: 1, ancho: 595.0, alto: 842.0, lineas }];
        if let Some(t) = portada {
            let l = |t: &str, top: f64| linea(vec![palabra(t, 40.0, top, "Impreso")]);
            paginas.insert(0, Pagina { numero: 0, ancho: 595.0, alto: 842.0, lineas: vec![l("INFORMACIÓN DE LA PRESENTACIÓN", 10.0), l(t, 30.0)] });
            for (i, p) in paginas.iter_mut().enumerate() {
                p.numero = i + 1;
            }
        }
        Documento { paginas }
    }

    /// Lo que no está no vale 0: sin el devengo, sin su resultado o sin entender la portada, es un fallo.
    #[test]
    fn sin_sus_anclas_es_un_fallo() {
        let p = Periodo::de("1T");
        let bien = [("27", "21,00"), ("46", "21,00"), ("64", "21,00"), ("65", "100,00"), ("66", "21,00"), ("69", "21,00"), ("71", "21,00"), ("09", "21,00")];
        let l = leer::<M303>(&un_303(&bien, Some("A INGRESAR")), 2025, &p).unwrap();
        assert_eq!((l.casillas[&71], l.tipo), (21.0, Some(Tipo::Ingreso)));
        assert!(leer::<M303>(&un_303(&bien, None), 2025, &p).is_ok(), "sin portada, no hay tipo que entender");
        // Sin el resultado (71): todo lo demás cuadraría con ceros, pero no se da por bueno.
        let sin_71: Vec<(&str, &str)> = bien.iter().copied().filter(|(n, _)| *n != "71").collect();
        let e = leer::<M303>(&un_303(&sin_71, None), 2025, &p).unwrap_err();
        assert_eq!(e.to_string(), "No encuentro en el PDF la casilla 71, que sale en todos los del 303.");
        assert_eq!(e.codigo(), "AEAT_303_FALTA_CASILLA");
        assert!(e.encontrado().contains("09, 27, 46, 64, 65, 66, 69"), "{}", e.encontrado());
        assert_eq!(leer::<M303>(&un_303(&bien, Some("Gracias")), 2025, &p).unwrap_err(), Fallo::SinTipo(Modelo::M303));
        // Sin el devengo, no se sabe de qué periodo es.
        let mut d = un_303(&bien, None);
        d.paginas[0].lineas.remove(1);
        assert_eq!(leer::<M303>(&d, 2025, &p).unwrap_err(), Fallo::SinDevengo(Modelo::M303));
    }
}
