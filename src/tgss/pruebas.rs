//! Pruebas de la Seguridad Social sin la Seguridad Social: las utilidades para leer HTML, los importes y las fechas,
//! que un trozo de página que no es como se espera sea un fallo (con trozos inventados, no páginas enteras) y qué
//! meses de nómina se piden. Entrar y leer las páginas de verdad necesita tu certificado (de la FNMT): no se prueba
//! aquí.

mod utilidades {
    use crate::tgss::html;
    use crate::tgss::importass;
    use crate::tgss::sede;

    #[test]
    fn utilidades() {
        assert_eq!(html::desescapar("a &amp;&amp b &lt;&gt; &quot;x&quot; &#39;&apos; &Aacute;&ordm;&euro;&hellip; &#x20AC; & &; &#; &#xZ"), "a && b <> \"x\" '' Áº€… € & &; &#; &#xZ");
        assert_eq!(html::titulo("<html><TITLE> Hola\n &amp; adiós </TITLE>"), "Hola & adiós");
        assert_eq!(html::donde("<p>Sin   título</p>"), "una página sin título", "sin su texto: puede ser de la persona");
        assert_eq!(html::servidor("https://User@IPCE.seg-social.es:443/x?y"), "ipce.seg-social.es");
        assert_eq!(html::consulta(&[("ARQ.SPM.TICKET", "a b/c+ñ"), ("X", "1~_.-")]), "ARQ.SPM.TICKET=a+b%2Fc%2B%C3%B1&X=1~_.-");
        assert_eq!(html::base("https://a.es/x/y", "<BASE target=_self HREF='/z/'>"), "https://a.es/z/");
        assert_eq!(html::base("https://a.es/x/y", "<base>"), "https://a.es/x/y");
        assert!(importass::dentro("{\"authenticated\":\"true\"}") && !importass::dentro("{\"authenticated\":\"false\"}"));
        assert_eq!(sede::ym("202602"), "2026-02");
        assert_eq!(sede::ym("2026"), "");
        for (m, ok) in [("2026-01", true), ("2026-12", true), ("2026-13", false), ("2026-00", false), ("1999-01", false), ("2026-1", false), ("x", false), ("2026-1ñ", false)] {
            assert_eq!(sede::mes_valido(m), ok, "{m}");
        }
    }
}

mod fallos {
    //! Lo que no es como se espera es un fallo con su código, no un vacío ni un 0 (con trozos de página inventados).
    use crate::tgss::importass::{self, fecha, importe};
    use crate::tgss::sede::{self, Prosa};

    #[test]
    fn importes_y_fechas() {
        for (t, v) in [("1.234,56 €", 1234.56), ("1234,56", 1234.56), ("10000", 10000.0), ("1.700 €/mes", 1700.0), ("-56,70", -56.7), ("12.34", 12.34), ("300,00.", 300.0), ("1.100,50 €/mes a partir del 1 de enero de 2027", 1100.5)] {
            assert_eq!(importe(t), Some(v), "{t}");
        }
        for t in ["", "x", "1.23.4", "1,2,3", "€"] {
            assert_eq!(importe(t), None, "{t}");
        }
        assert_eq!(fecha("9 de septiembre de 2025").as_deref(), Some("2025-09-09"));
        assert_eq!(fecha("Cargo: 30/10/2026").as_deref(), Some("2026-10-30"));
        assert_eq!(fecha("a partir del 1 de enero de 2027").as_deref(), Some("2027-01-01"));
        for t in ["123 de enero de 2025", "1 de enero de 20255", "31 de febrero de 2025", "130/10/2026", "30/13/2026", "-", ""] {
            assert_eq!(fecha(t), None, "{t}");
        }
    }

    fn codigo<T: std::fmt::Debug>(r: Result<T, crate::dominio::fallo::Error>) -> String {
        r.unwrap_err().codigo
    }

    #[test]
    fn paginas_que_no_son_como_siempre() {
        // Tu colectivo: autónomo, otro, o la página no lo dice.
        assert!(importass::autonomo(r#"<body class="importass ColectivoRETA">"#).unwrap());
        assert!(!importass::autonomo(r#"<body class="importass ColectivoRGSS">"#).unwrap());
        assert_eq!(codigo(importass::autonomo(r#"<body class="importass nuevo">"#)), "TGSS_PERSONAL_SIN_COLECTIVO");
        // El perfil, sin la fecha de alta o con una que no es una fecha.
        let dl = |dt: &str, dd: &str| format!("<dl><dt>{dt}</dt><dd>{dd}</dd></dl>");
        assert_eq!(importass::leer_perfil(&dl("Fecha alta como autónomo", "1 de marzo de 2021")).unwrap().alta.fecha, "2021-03-01");
        assert_eq!(codigo(importass::leer_perfil(&dl("Fecha de alta", "1 de marzo de 2021"))), "TGSS_PERFIL_SIN_ALTA");
        assert_eq!(codigo(importass::leer_perfil(&dl("Fecha alta como autónomo", "marzo"))), "TGSS_PERFIL_FECHA");
        // Mientras sigues de alta, la fecha de baja sale «Sin datos»: no hay baja, no es un fallo.
        let alta = dl("Fecha alta como autónomo", "01/03/2021");
        assert_eq!(importass::leer_perfil(&(alta.clone() + &dl("Fecha baja como autónomo", "Sin datos"))).unwrap().alta.baja, "");
        assert_eq!(codigo(importass::leer_perfil(&(alta + &dl("Fecha baja como autónomo", "pronto")))), "TGSS_PERFIL_FECHA");
        let base = dl("Fecha alta como autónomo", "01/03/2021") + &dl("Base de cotización actual", "No consta");
        assert_eq!(importass::leer_perfil(&base).unwrap().cotizacion.base, None, "sin cifras: no hay");
        assert_eq!(codigo(importass::leer_perfil(&base.replace("No consta", "1.0.00 €"))), "TGSS_PERFIL_IMPORTE");
        // Los recibos: sin la lista es un fallo; con ella y vacía, ninguno.
        assert_eq!(codigo(importass::leer_recibos("<p>Hemos renovado esta página</p>")), "TGSS_RECIBOS_SIN_LISTA");
        assert!(importass::leer_recibos(r#"<div class="quota-history"></div>"#).unwrap().0.is_empty());
        let recibo = |importe: &str, conceptos: &str| {
            format!(
                r#"<div class="quota-history"><div class="quota-history__item"><a class="quota-history__item-title" href="r1">Cuota octubre 2026</a><div class="quota-history__item-amount">{importe}</div>{conceptos}</div></div>"#
            )
        };
        let cc = r#"<div class="quota-history__breakdown-amount">263,50 €</div> <p class="encabezadoLista">Contingencias Comunes</p>"#;
        let fp = r#"<div class="quota-history__breakdown-amount">5,00 €</div> <p class="encabezadoLista">Formaci&oacute;n Profesional</p>"#;
        let (r, _) = importass::leer_recibos(&recibo("310,00 €", &format!("{cc}{fp}"))).unwrap();
        assert_eq!((r[0].importe, r[0].periodo.as_str(), r[0].conceptos.cc, r[0].conceptos.fp), (310.0, "2026-10", 263.5, 5.0), "con su entidad, también");
        assert_eq!(codigo(importass::leer_recibos(&recibo("pendiente", cc))), "TGSS_RECIBO_SIN_IMPORTE");
        assert_eq!(codigo(importass::leer_recibos(&recibo("310,00 €", fp))), "TGSS_RECIBO_SIN_CC");
        // Las regularizaciones: un resultado que no se entiende.
        let reg = |r: &str| format!("<h1>Regularización de cuotas de años anteriores</h1><li>TRABAJADORES AUTÓNOMOS 2024 {r}</li>");
        assert_eq!(importass::leer_regularizaciones(&reg("A ingresar: 123,45 €")).unwrap()[0].importe, 123.45);
        assert_eq!(codigo(importass::leer_regularizaciones(&reg("Pendiente de calcular"))), "TGSS_REGULARIZACION_RESULTADO");
        assert_eq!(codigo(importass::leer_regularizacion("<h1>Regularización</h1>")), "TGSS_REGULARIZACION_SIN_ANIO");
    }

    #[test]
    fn liquidaciones_que_no_son_como_siempre() {
        let concepto = |base: &str, empresa: &str| format!("<CrtrEnCalcAcumTrabajador><desDatCal>CONTINGENCIAS COMUNES</desDatCal><baseCalValor>{base}</baseCalValor>{empresa}<cuotaCalValObrera>117,00</cuotaCalValObrera><cuotaCalVaTotal>707,00</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>");
        let pagina = |conceptos: &str| format!(r#"<script id="xml" type="text/plain"><SPM><SPM.TICKET id="SPM.TICKET">T1</SPM.TICKET><razonSocialEmp>EMPRESA INVENTADA SL</razonSocialEmp><tipoLiq>L00</tipoLiq>{conceptos}</SPM></script>"#);
        let leer = |c: &str| sede::leer_liquidacion(&Prosa::de(&pagina(c)).unwrap());
        let l = leer(&concepto("2.500,00", "<cuotaCalValPatronal>590,00</cuotaCalValPatronal>")).unwrap();
        assert_eq!((l.cuota_empresa, l.cuota_trabajador, l.tipo.as_str()), (590.0, 117.0, "L00"));
        assert_eq!(leer(&concepto("", "<cuotaCalValPatronal>590,00</cuotaCalValPatronal>")).unwrap().conceptos[0].base, 0.0, "vacía vale 0");
        assert_eq!(codigo(leer(&concepto("2.500,00", ""))), "TGSS_SEDE_LIQUIDACION", "sin la cuota de la empresa");
        assert_eq!(codigo(leer(&concepto("2.500,00", "<cuotaCalValPatronal>n/d</cuotaCalValPatronal>"))), "TGSS_SEDE_LIQUIDACION");
        assert_eq!(codigo(leer("")), "TGSS_SEDE_LIQUIDACION", "sin conceptos");
        assert_eq!(codigo(Prosa::de("<html><title>Mantenimiento</title></html>")), "TGSS_SEDE_RESPUESTA");
        // Una página que no es la esperada: de ella, solo su estructura (título y clases), no su texto.
        let e = Prosa::de(r#"<html><body class="aviso"><p>Hola, Pérez Inventada · Calle Mayor 12</p></body></html>"#).unwrap_err();
        let todo = format!("{} {} {}", e.mensaje, e.encontrado, e.fragmento);
        assert!(!todo.contains("Pérez") && !todo.contains("Mayor") && e.encontrado.contains("aviso"), "{e:?}");
    }
}

mod meses {
    //! Qué meses de nómina se piden, con datos inventados.
    use crate::dominio::Datos;
    use crate::tgss::{MAX_MESES, meses_a_consultar};
    use serde_json::{Map, Value, json};

    const HOY: &str = "2026-10-21T12:00:00.000Z";
    const HACE_1H: &str = "2026-10-21T11:00:00.000Z";
    const HACE_13H: &str = "2026-10-20T23:00:00.000Z";
    const HACE_UN_MES: &str = "2026-09-20T10:00:00.000Z";

    /// Datos con `tgss.nomina`: los meses con liquidaciones (o vacíos) y cuándo se miró cada uno.
    fn datos(meses: &[&str], consultado: &[(&str, &str)]) -> Datos {
        let m: Map<String, Value> = meses.iter().map(|k| (k.to_string(), json!([]))).collect();
        let c: Map<String, Value> = consultado.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        serde_json::from_value(json!({ "tgss": { "reta": true, "nomina": { "meses": m, "consultado": c } } })).unwrap()
    }

    /// Todos los meses de esos años, mirados entonces.
    fn mirados(anios: std::ops::RangeInclusive<i32>, hasta: &str, cuando: &'static str) -> (Vec<String>, Vec<(String, &'static str)>) {
        let ms: Vec<String> = anios.flat_map(|a| (1..=12).map(move |m| format!("{a}-{m:02}"))).filter(|m| m.as_str() < hasta).collect();
        let c = ms.iter().map(|m| (m.clone(), cuando)).collect();
        (ms, c)
    }

    fn con(ms: &[String], c: &[(String, &str)]) -> Datos {
        let ms: Vec<&str> = ms.iter().map(String::as_str).collect();
        let c: Vec<(&str, &str)> = c.iter().map(|(a, b)| (a.as_str(), *b)).collect();
        datos(&ms, &c)
    }

    #[test]
    fn sin_nada_todo_desde_el_mas_reciente() {
        let m = meses_a_consultar(&Datos::default(), HOY);
        assert_eq!(m.len(), MAX_MESES);
        assert_eq!(m[0], "2026-09", "el mes en curso no");
        assert_eq!(m[8], "2026-01");
        assert_eq!(m[9], "2025-12");
        assert_eq!(m[23], "2024-10");
        // Con solo la fecha, igual.
        assert_eq!(meses_a_consultar(&Datos::default(), "2026-10-21"), m);
        assert!(meses_a_consultar(&Datos::default(), "ayer").is_empty());
    }

    /// La primera vez se pide por tandas (como mucho `MAX_MESES` cada vez) y lo pedido queda mirado, como hace
    /// `sincronizar::aplicar`: al cabo de unas cuantas están todos los meses, sin huecos en un año que se cortó a medias.
    #[test]
    fn la_primera_vez_por_tandas_sin_huecos() {
        let mut d = Datos::default();
        let mut pedidos = std::collections::BTreeSet::new();
        for _ in 0..10 {
            let m = meses_a_consultar(&d, HOY);
            assert!(m.len() <= MAX_MESES);
            for k in m {
                d.tgss.nomina.meses.insert(k.clone(), Vec::new());
                d.tgss.nomina.consultado.insert(k.clone(), HACE_1H.into());
                pedidos.insert(k);
            }
        }
        let (todos, _) = mirados(2020..=2026, "2026-10", HACE_1H);
        let faltan: Vec<&String> = todos.iter().filter(|m| !pedidos.contains(*m)).collect();
        assert!(faltan.is_empty(), "nunca se piden: {faltan:?}");
        assert!(meses_a_consultar(&d, HOY).is_empty(), "y luego, nada");
    }

    #[test]
    fn todo_mirado_hace_poco_nada() {
        let (ms, c) = mirados(2020..=2026, "2026-10", HACE_1H);
        assert!(meses_a_consultar(&con(&ms, &c), HOY).is_empty());
    }

    #[test]
    fn los_recientes_se_vuelven_a_mirar_pasadas_12_horas() {
        let (ms, mut c) = mirados(2020..=2026, "2026-10", HACE_UN_MES);
        // Los últimos 7 meses (de marzo a septiembre), mirados hace un mes: otra vez. Los de antes, no.
        assert_eq!(meses_a_consultar(&con(&ms, &c), HOY), ["2026-09", "2026-08", "2026-07", "2026-06", "2026-05", "2026-04", "2026-03"]);
        for (m, cuando) in c.iter_mut() {
            *cuando = match m.as_str() {
                "2026-05" | "2026-03" => HACE_13H,
                "2026-02" => HACE_13H,
                _ => HACE_1H,
            };
        }
        assert_eq!(meses_a_consultar(&con(&ms, &c), HOY), ["2026-05", "2026-03"]);
    }

    #[test]
    fn los_que_aun_no_daba_se_vuelven_a_mirar() {
        // Septiembre se miró pero la Seguridad Social aún no lo daba: está en `consultado`, no en `meses`.
        let (mut ms, c) = mirados(2020..=2026, "2026-10", HACE_1H);
        ms.retain(|m| m != "2026-09");
        let mut c13 = c.clone();
        c13.iter_mut().filter(|(m, _)| m == "2026-09").for_each(|x| x.1 = HACE_13H);
        assert!(meses_a_consultar(&con(&ms, &c), HOY).is_empty());
        assert_eq!(meses_a_consultar(&con(&ms, &c13), HOY), ["2026-09"]);
    }

    #[test]
    fn anios_viejos_mes_a_mes() {
        // 2023: un mes mirado → los otros 11, también (un año con algo no se da por mirado entero).
        // 2022: ningún mes en `meses` y uno mirado (la Seguridad Social aún no lo daba) → los otros 11.
        let (mut ms, mut c) = mirados(2020..=2026, "2026-10", HACE_1H);
        ms.retain(|m| !m.starts_with("2023-") || m == "2023-06");
        c.retain(|(m, _)| !m.starts_with("2023-") || m == "2023-06");
        ms.retain(|m| !m.starts_with("2022-"));
        c.retain(|(m, _)| !m.starts_with("2022-") || m == "2022-01");
        let r = meses_a_consultar(&con(&ms, &c), HOY);
        assert_eq!(r.len(), 22);
        assert!(!r.contains(&"2023-06".to_string()) && !r.contains(&"2022-01".to_string()), "{r:?}");
        assert_eq!((r[0].as_str(), r[10].as_str(), r[11].as_str(), r[21].as_str()), ("2023-12", "2023-01", "2022-12", "2022-02"));
        // El año anterior siempre se mira: lo que le falte, también.
        c.retain(|(m, _)| m != "2025-04");
        assert_eq!(meses_a_consultar(&con(&ms, &c), HOY)[0], "2025-04");
    }

    #[test]
    fn en_enero_y_con_fechas_raras() {
        let hoy = "2026-01-10T09:00:00.000Z";
        let (ms, c) = mirados(2020..=2025, "2026-01", "2025-12-01T00:00:00.000Z");
        // Los últimos 7 meses del año pasado (de junio a diciembre), mirados hace más de 12 horas.
        assert_eq!(meses_a_consultar(&con(&ms, &c), hoy), ["2025-12", "2025-11", "2025-10", "2025-09", "2025-08", "2025-07", "2025-06"]);
        // Una fecha que no se entiende cuenta como mirado (no viejo).
        let (ms, c) = mirados(2020..=2026, "2026-10", "ayer");
        assert!(meses_a_consultar(&con(&ms, &c), HOY).is_empty());
        // Vacío o null: nunca mirado.
        let (ms, mut c) = mirados(2020..=2026, "2026-10", HACE_1H);
        c.iter_mut().filter(|(m, _)| m == "2026-02").for_each(|x| x.1 = "");
        assert_eq!(meses_a_consultar(&con(&ms, &c), HOY), ["2026-02"]);
    }
}
