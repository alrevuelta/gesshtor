//! Modelo 036 · cuándo estabas dado de alta en Hacienda. Se puede pagar la cuota de autónomo estando de baja en el censo
//! (son dos altas distintas), y entonces no toca presentar el 303 ni el 130: sin esto, la app daría por vencidos
//! trimestres que no lo están.
//!
//! De cada 036 de `justificantes/` se miran las causas marcadas: la 111 (alta en el censo, con la fecha de la casilla 425)
//! y la 150 (baja, con la fecha efectiva de la 152). Sin esas fechas, la de presentación.
use super::{lectura, pdf};
use crate::dominio::CambioCenso;
use std::path::Path;

/// Las altas y bajas de los 036 de la carpeta, por fecha. Un PDF que no se pueda abrir no cuenta.
pub fn de_la_carpeta(dir: &Path) -> Vec<CambioCenso> {
    let mut v: Vec<CambioCenso> = lectura::justificantes(dir).iter().filter(|j| j.modelo == "036").filter_map(|j| pdf::abrir(&j.ruta).ok()).flat_map(|doc| cambios(&doc.texto())).collect();
    v.sort_by(|a, b| a.fecha.cmp(&b.fecha));
    v.dedup();
    v
}

/// «25/02/2025» o «25-02-2025» → «2025-02-25».
fn iso(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let bien = b.len() == 10 && matches!(b[2], b'/' | b'-') && b[2] == b[5] && [0, 1, 3, 4, 6, 7, 8, 9].iter().all(|&i| b[i].is_ascii_digit());
    bien.then(|| format!("{}-{}-{}", &s[6..10], &s[3..5], &s[..2]))
}

/// Lo que dice un 036: si es un alta, una baja, las dos o ninguna (una modificación).
fn cambios(texto: &str) -> Vec<CambioCenso> {
    let p: Vec<&str> = texto.split_whitespace().collect();
    let marcada = |n: &str| p.windows(2).any(|w| w[0] == n && w[1] == "X");
    let tras = |n: &str| p.iter().position(|x| *x == n).and_then(|i| p[i + 1..].iter().take(3).find_map(|x| iso(x)));
    let presentado = p.windows(3).find(|w| w[0] == "realizada" && w[1] == "el:").and_then(|w| iso(w[2]));
    let mut v = Vec::new();
    if marcada("111")
        && let Some(fecha) = tras("425").or_else(|| presentado.clone())
    {
        v.push(CambioCenso { fecha, alta: true });
    }
    if marcada("150")
        && let Some(fecha) = tras("152").or_else(|| presentado.clone())
    {
        v.push(CambioCenso { fecha, alta: false });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::Lecturas;

    #[test]
    fn lee_altas_y_bajas() {
        let baja = "Presentación realizada el: 24-02-2025 a las 14:18:22\n111 Alta en el censo\n150 X Baja en el censo Causa 151 Cese\nFecha efectiva de la baja 152 25/02/2025";
        assert_eq!(cambios(baja), [CambioCenso { fecha: "2025-02-25".into(), alta: false }]);
        let alta = "Presentación realizada el: 15-10-2026 a las 08:51:11\n111 X Alta en el censo\n150 Baja\nCausa de presentación 424 X Alta Fecha 425 01/08/2026";
        assert_eq!(cambios(alta), [CambioCenso { fecha: "2026-08-01".into(), alta: true }]);
        // Sin la fecha de la actividad, la de presentación; una modificación no cambia nada.
        assert_eq!(cambios("realizada el: 15-10-2026\n111 X Alta")[0].fecha, "2026-10-15");
        assert!(cambios("realizada el: 15-10-2026\n111 Alta\n127 X Modificación\n150 Baja").is_empty());
    }

    #[test]
    fn en_el_censo_por_periodos() {
        let c = |fecha: &str, alta| CambioCenso { fecha: fecha.into(), alta };
        let l = Lecturas { censo: vec![c("2025-02-25", false), c("2026-08-01", true)], ..Default::default() };
        assert!(l.en_censo("2025-01-01", "2025-03-31"), "se dio de baja a mitad");
        assert!(!l.en_censo("2025-07-01", "2025-09-30"));
        assert!(!l.en_censo("2026-04-01", "2026-06-30"));
        assert!(l.en_censo("2026-07-01", "2026-09-30"), "se dio de alta a mitad");
        assert!(l.en_censo("2026-10-01", "2026-12-31"));
        // Sin ningún 036 no se sabe: cuenta como de alta.
        assert!(Lecturas::default().en_censo("2025-07-01", "2025-09-30"));
        // Si lo primero que se sabe es un alta, antes no estaba.
        let l = Lecturas { censo: vec![c("2026-08-01", true)], ..Default::default() };
        assert!(!l.en_censo("2026-01-01", "2026-03-31") && l.en_censo("2026-07-01", "2026-09-30"));
    }
}
