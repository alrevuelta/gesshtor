//! El conector con DEGIRO: entra en su plataforma web (trader.degiro.nl) con tu usuario, como lo hace tu
//! navegador, por la API no oficial que usa la propia web, y traduce lo que trae al formato común
//! (`dominio::inversiones`): tus compras y ventas, tus dividendos e intereses y lo que tenías a cada 31 de
//! diciembre. Solo consulta: nunca da ninguna orden.
//!
//! **Aviso.** El Contrato de Cliente de DEGIRO (versión 20220830, https://www.degiro.es/data/pdf/es/Contrato_Cliente.pdf),
//! en las «Condiciones de los Servicios de Inversión», art. 5.6: «No se permite al Cliente utilizar Herramientas
//! Automatizadas en la plataforma de inversión de DEGIRO. Si DEGIRO detecta Herramientas Automatizadas o actividades
//! que apunten a su uso, las consultas, actividades o las cuentas involucradas podrían ser bloqueadas por DEGIRO.»
//! (el art. 1 las define: «las consultas, las secuencias de comandos o los mecanismos automatizados, o bien cualquier
//! interfaz de programación de aplicaciones…»). Y su ayuda («¿Puedo automatizar operaciones o utilizar bots…?»): «el
//! uso de herramientas de automatización de terceros infringe nuestros términos de servicio». Por eso va desactivado
//! y, al activarlo, se avisa (`AVISO`).
//!
//! **De dónde sale cómo se habla con DEGIRO.** DEGIRO no publica esta API (su ayuda: «actualmente no ofrecemos una
//! API»). Las direcciones, cómo se entra y la forma de las respuestas salen del código de proyectos libres y
//! mantenidos que hablan con ella, leídos el 20 de noviembre de 2026:
//!
//! - [dc] degiro-connector (Python; github.com/Chavithra/degiro-connector, v3.0.36, commit ea209cc, 2026-11-04):
//!   `core/constants/urls.py` (las direcciones), `trading/actions/action_connect.py` y `trading/models/login.py`
//!   (entrar: el código de verificación va a `/totp`; el estado 6 pide el código, el 12 la confirmación en la app,
//!   que se espera en `/in-app` mientras conteste el estado 3; `captchaRequired`; 405 es mantenimiento),
//!   `trading/models/transaction.py` y `account.py` (las operaciones y los movimientos de efectivo),
//!   `action_get_position_report.py` (el informe de cartera) y `action_logout.py` (salir, con PUT).
//! - [mcp] degiro-mcp (Zig; github.com/jaenster/degiro-mcp, commit 29dae46, 2026-10-20): `src/degiro.zig` (el cuerpo
//!   del inicio de sesión, con `queryParams`; la cookie `JSESSIONID` con el `sessionId`; las cabeceras `Origin` y
//!   `Referer`; el `intAccount` de `pa/secure/client`; DEGIRO contesta 429 si se reintenta entrar deprisa, y bloquea
//!   o pide captcha tras varios fallos) y `src/tools.zig` (los informes, como mucho 366 días por petición;
//!   `products/info` falla con ids repetidos). Donde [dc] y [mcp] no coinciden (el cuerpo lleva `queryTarams` en
//!   [dc] y `queryParams` en [mcp]; [dc] deja la cookie a la sesión y [mcp] la pone a mano) se sigue a [mcp].
//! - [gds] ghostfolio-degiro-sync (Python; github.com/flowcool/ghostfolio-degiro-sync, commit 411c5e6,
//!   2026-11-16): `docs/source-contract.md` (las ventas llegan con la cantidad en negativo;
//!   `totalFeesInBaseCurrency = feeInBaseCurrency + autoFxFeeInBaseCurrency`; los campos de los movimientos,
//!   `exchangeRate` incluido; sin movimientos, `data: {}`).
//! - [so] stonks-overwatch (Python; github.com/ctasada/stonks-overwatch, commit edd7b16, 2026-11-19):
//!   `services/brokers/degiro/client/constants.py` (`transactionTypeId` 0 es una compraventa; 101 un split, 106,
//!   108, 112 y 204 otras operaciones de la empresa; `productTypeId` 1 acción, 2 bono, 13 fondo, 131 ETF) y
//!   `tests/resources/…/degiro/*.json` (ejemplos de respuestas).
//! - [dr] degiro-renta (Python; github.com/GITDBC/degiro-renta, commit af445bf, 2026-11-12):
//!   `degiro_renta/parsing/account_parser.py` (las descripciones en español: «Dividendo», «Retención del
//!   dividendo», «Interés»…).
//! - El informe de cartera en CSV: burocratin (github.com/vaijira/burocratin, `src/parsers/degiro_csv.rs`) y
//!   ML_finance (github.com/hugosanz28/ML_finance, `docs/degiro_input_contract.md`): cabecera «Producto,
//!   Symbol/ISIN, Cantidad, Precio de, Valor local, , Valor en EUR» (en algunos, sin la columna vacía), decimales
//!   con coma y una fila «CASH & CASH FUND & FTX CASH (EUR)» con el efectivo.
//!
//! No se ha podido probar con una cuenta de verdad: si DEGIRO cambia algo, falla diciendo qué esperaba y qué ha
//! llegado (la forma de la respuesta, sin ningún dato tuyo).
use super::{Conector, Pide};
use crate::dominio::fmt;
use crate::dominio::fuente::Error;
use crate::dominio::inversiones::{Bien, Cartera, Clase, Cobro, Operacion, Posicion, Producto};
use crate::red::cliente::{Cliente, Cuerpo, Respuesta, DOMINIOS_DEGIRO};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const CONECTOR: Conector = Conector { id: "degiro", nombre: "DEGIRO", dominios: DOMINIOS_DEGIRO, aviso: AVISO, pide: PIDE, deposito: DEPOSITO, traer };

const DEPOSITO: &str = "DEGIRO: según su Contrato de Cliente, tus valores los mantienen sus SPV, fundaciones de los Países Bajos (arts. 9.1 y 17), y tu efectivo está en tu cuenta en flatexDEGIRO Bank, en Alemania (art. 1). Compruébalo en tu informe anual de DEGIRO: antes de 2022, el efectivo podía estar en fondos del mercado monetario.";

const AVISO: &[&str] = &[
    "La app entrará en tu cuenta de DEGIRO como lo hace tu navegador, por la API no oficial de su",
    "web (trader.degiro.nl), para traer tus operaciones, dividendos e intereses y tu cartera a cada",
    "31 de diciembre. Solo consulta: nunca da órdenes. Tu usuario, tu contraseña y el código se",
    "piden cada vez y no se guardan; lo traído se queda en tu carpeta (fuentes/degiro.json).",
    "",
    "Ojo: el Contrato de Cliente de DEGIRO (art. 5.6 de sus Condiciones de los Servicios de",
    "Inversión) no permite «Herramientas Automatizadas» en su plataforma y dice que puede bloquear",
    "las consultas o la cuenta si las detecta. Actívalo solo si aceptas ese riesgo.",
];

const PIDE: &[Pide] = &[
    Pide { etiqueta: "Usuario", secreto: false, una_vez: false, ayuda: "El de DEGIRO (no se guarda)." },
    Pide { etiqueta: "Contraseña", secreto: true, una_vez: false, ayuda: "No se ve al escribirla y no se guarda." },
    Pide { etiqueta: "Código de tu app", secreto: false, una_vez: true, ayuda: "Las 6 cifras de tu app de verificación, si la usas. Si no, déjalo vacío: quizá te pida confirmar en la app de DEGIRO." },
];

/* ================= las direcciones ([dc] urls.py; [mcp] degiro.zig) ================= */

const WEB: &str = "https://trader.degiro.nl";
const ENTRAR: &str = "https://trader.degiro.nl/login/secure/login";
const CLIENTE: &str = "https://trader.degiro.nl/pa/secure/client";
const OPERACIONES: &str = "https://trader.degiro.nl/portfolio-reports/secure/v4/transactions";
const MOVIMIENTOS: &str = "https://trader.degiro.nl/portfolio-reports/secure/v6/accountoverview";
const CARTERA: &str = "https://trader.degiro.nl/portfolio-reports/secure/v3/positionReport/csv";
const PRODUCTOS: &str = "https://trader.degiro.nl/product_search/secure/v5/products/info";
const SALIR: &str = "https://trader.degiro.nl/trading/secure/logout";
/// La página desde la que llama la web ([mcp]: `Referer`).
const REFERER: &str = "https://trader.degiro.nl/trader/";

/// Desde qué año se piden tus operaciones, año a año (los informes no dan más de 366 días de una vez, [mcp]).
/// Hacen falta todas: sin la compra no se sabe lo que costó lo que vendes.
const DESDE: i32 = 2008;
/// Productos por petición a `products/info`.
const POR_PETICION: usize = 50;
/// La confirmación en la app: cada cuánto se pregunta y cuántas veces (unos dos minutos).
const ESPERA_APP: std::time::Duration = std::time::Duration::from_secs(5);
const VECES_APP: usize = 24;

/// Dónde están tus cosas en DEGIRO, según su Contrato de Cliente (versión 20220830): los valores, en sus SPV
/// («todas las posiciones en Instrumentos Financieros … sean mantenidas por SPV», art. 9.1; fundaciones de los
/// Países Bajos, art. 17); el efectivo, en tu cuenta en flatexDEGIRO Bank, en Alemania (art. 1, «Cuenta de
/// Efectivo … mantenida a nombre del Cliente en flatexDEGIRO Bank AG»; BIC BIWBDE33XXX, según su ayuda).
const DEPOSITO_VALORES: (&str, &str) = ("NL", "Stichting DEGIRO (vehículo de DEGIRO en los Países Bajos)");
const DEPOSITO_EFECTIVO: (&str, &str) = ("DE", "flatexDEGIRO Bank (Alemania)");

/* ================= traer ================= */

fn traer(c: Cliente, datos: &[String], paso: &mut dyn FnMut(String)) -> Result<Cartera, Error> {
    let [usuario, clave, codigo] = datos else { return Err(Error::de("Faltan el usuario y la contraseña de DEGIRO.")) };
    if usuario.trim().is_empty() || clave.is_empty() {
        return Err(Error::de("Escribe tu usuario y tu contraseña de DEGIRO."));
    }
    // Como la web ([mcp]: `Origin`; el `Referer` va en cada petición).
    let c = c.con_cabecera("Origin", WEB);
    paso("DEGIRO: entrando…".into());
    let sesion = entrar(&c, usuario.trim(), clave, codigo.trim(), paso)?;
    c.cookie(WEB, "JSESSIONID", &sesion).map_err(Error::de)?;
    let cuenta = pedir_json(&c, &format!("{CLIENTE}?sessionId={sesion}"), "tus datos de cliente")?;
    let cuenta = numero(&cuenta, &["data", "intAccount"]).map(|n| n as i64).ok_or_else(|| cambiado("tus datos de cliente (pa/secure/client)", "«data.intAccount» (un número)", &cuenta))?;
    let s = Sesion { id: sesion, cuenta };
    let r = traer_todo(&c, &s, paso);
    // Salir siempre ([dc] action_logout.py); si falla, la sesión caduca sola.
    let _ = c.put(&format!("{SALIR};jsessionid={}?intAccount={}&sessionId={}", s.id, s.cuenta, s.id));
    r
}

struct Sesion {
    id: String,
    cuenta: i64,
}

impl Sesion {
    fn en(&self, url: &str, consulta: &str) -> String {
        let y = if consulta.is_empty() { "" } else { "&" };
        format!("{url}?{consulta}{y}intAccount={}&sessionId={}", self.cuenta, self.id)
    }
}

fn traer_todo(c: &Cliente, s: &Sesion, paso: &mut dyn FnMut(String)) -> Result<Cartera, Error> {
    let hoy = fmt::hoy();
    let este = fmt::anio_actual();
    let mut operaciones = Vec::new();
    let mut movimientos = Vec::new();
    for y in DESDE..=este {
        paso(format!("DEGIRO: tus operaciones de {y}…"));
        let (desde, hasta) = (format!("01/01/{y}"), if y == este { fmt::dmy(&hoy) } else { format!("31/12/{y}") });
        let fechas = format!("fromDate={desde}&toDate={hasta}");
        let v = pedir_json(c, &s.en(OPERACIONES, &format!("{fechas}&groupTransactionsByOrder=false")), &format!("tus operaciones de {y}"))?;
        operaciones.extend(lista(&v, &["data"], &format!("tus operaciones de {y} (v4/transactions)"))?);
        let v = pedir_json(c, &s.en(MOVIMIENTOS, &fechas), &format!("tus movimientos de {y}"))?;
        movimientos.extend(movimientos_de(&v, y)?);
    }
    // Los productos de todo lo anterior (nombre, ISIN, tipo y divisa).
    let ids: BTreeSet<String> = operaciones.iter().chain(&movimientos).filter_map(|o| id(o.get("productId"))).collect();
    let ids: Vec<String> = ids.into_iter().collect();
    let mut productos = Map::new();
    for (i, lote) in ids.chunks(POR_PETICION).enumerate() {
        paso(format!("DEGIRO: qué es cada valor ({} de {})…", (i * POR_PETICION + lote.len()), ids.len()));
        let cuerpo = serde_json::to_vec(lote).unwrap_or_default();
        let r = c.post(&s.en(PRODUCTOS, ""), Cuerpo::Bytes(&cuerpo, "application/json;charset=UTF-8"), Some(REFERER)).map_err(Error::de)?;
        let v = json_de(&r, "qué es cada valor")?;
        match v.get("data") {
            Some(Value::Object(m)) => productos.extend(m.clone()),
            _ => return Err(cambiado("qué es cada valor (products/info)", "«data» con cada producto por su id", &v)),
        }
    }
    // Lo que tenías a cada 31 de diciembre desde el primer año con algo, y hoy.
    let primero = operaciones.iter().chain(&movimientos).filter_map(|o| texto(o, "date")).map(fmt::anio_de).filter(|y| *y > 0).min().unwrap_or(este);
    let mut informes = Vec::new();
    for y in primero..=este {
        let fecha = if y == este { hoy.clone() } else { format!("{y}-12-31") };
        paso(format!("DEGIRO: tu cartera a {}…", fmt::dmy(&fecha)));
        let consulta = format!("country=ES&lang=es&fromDate=01/01/{y}&toDate={}", fmt::dmy(&fecha));
        let r = c.get(&s.en(CARTERA, &consulta), Some(REFERER)).map_err(Error::de)?;
        if r.codigo != 200 {
            return Err(codigo_mal(&r, &format!("tu cartera a {}", fmt::dmy(&fecha))));
        }
        informes.push((fecha, r.texto()));
    }
    traducir(&operaciones, &movimientos, &productos, &informes, &hoy)
}

/* ================= entrar ([dc] action_connect.py; [mcp] degiro.zig) ================= */

/// Lo que contesta DEGIRO al intentar entrar.
#[derive(Debug, PartialEq)]
enum Entrada {
    Dentro(String),
    /// Hay que confirmar en la app de DEGIRO: con este token se pregunta si ya está.
    EnLaApp(String),
    /// Aún sin confirmar en la app (y lo que dice DEGIRO).
    Esperando(String),
}

fn entrar(c: &Cliente, usuario: &str, clave: &str, codigo: &str, paso: &mut dyn FnMut(String)) -> Result<String, Error> {
    let mut cuerpo = json!({ "username": usuario, "password": clave, "isPassCodeReset": false, "isRedirectToMobile": false, "queryParams": {} });
    let url = if codigo.is_empty() {
        ENTRAR.to_string()
    } else {
        cuerpo["oneTimePassword"] = codigo.into();
        format!("{ENTRAR}/totp")
    };
    let enviar = |url: &str, cuerpo: &Value| -> Result<Entrada, Error> {
        let b = serde_json::to_vec(cuerpo).unwrap_or_default();
        let r = c.post(url, Cuerpo::Bytes(&b, "application/json;charset=UTF-8"), Some(REFERER)).map_err(Error::de)?;
        al_entrar(r.codigo, &r.cuerpo)
    };
    match enviar(&url, &cuerpo)? {
        Entrada::Dentro(s) => Ok(s),
        // El estado 3 solo quiere decir «aún no» mientras se espera la confirmación en la app ([dc]).
        Entrada::Esperando(que) => Err(Error::de(format!("DEGIRO no te deja entrar: «{que}» (estado 3). Revisa el usuario, la contraseña y el código."))),
        Entrada::EnLaApp(token) => {
            paso("DEGIRO: abre su app en el móvil y confirma que eres tú (te espero unos dos minutos)…".into());
            cuerpo["inAppToken"] = token.into();
            for _ in 0..VECES_APP {
                std::thread::sleep(ESPERA_APP);
                match enviar(&format!("{ENTRAR}/in-app"), &cuerpo)? {
                    Entrada::Dentro(s) => return Ok(s),
                    Entrada::Esperando(_) | Entrada::EnLaApp(_) => {}
                }
            }
            Err(Error::de("No has confirmado la entrada en la app de DEGIRO a tiempo. Vuelve a probar."))
        }
    }
}

/// Lo que quiere decir la respuesta al intentar entrar (sin red, para probarlo).
fn al_entrar(codigo: u16, cuerpo: &[u8]) -> Result<Entrada, Error> {
    if codigo == 405 {
        return Err(Error::de("DEGIRO está en mantenimiento: prueba más tarde."));
    }
    if codigo == 429 {
        return Err(Error::de("DEGIRO pide esperar: se ha intentado entrar demasiadas veces. Prueba dentro de un rato (si se insiste, puede bloquear la cuenta)."));
    }
    let Ok(v) = serde_json::from_slice::<Value>(cuerpo) else {
        return Err(Error::de(format!("DEGIRO ha cambiado cómo se entra: esperaba una respuesta en JSON y ha llegado otra cosa (código {codigo}, {} bytes).", cuerpo.len())));
    };
    if let Some(s) = texto(&v, "sessionId").filter(|s| !s.is_empty()) {
        return Ok(Entrada::Dentro(s.to_string()));
    }
    if v.get("captchaRequired").and_then(Value::as_bool) == Some(true) {
        return Err(Error::de("DEGIRO pide resolver un captcha: entra una vez en su web con tu navegador y vuelve a probar."));
    }
    let estado = v.get("status").and_then(Value::as_i64);
    let que = texto(&v, "statusText").unwrap_or_default();
    match estado {
        Some(6) => Err(Error::de("DEGIRO pide el código de tu app de verificación: escríbelo y vuelve a probar.")),
        Some(12) => texto(&v, "inAppToken").map(|t| Entrada::EnLaApp(t.to_string())).ok_or_else(|| cambiado("cómo se entra", "«inAppToken» para confirmar en la app", &v)),
        Some(3) => Ok(Entrada::Esperando(que.to_string())),
        Some(n) => Err(Error::de(format!("DEGIRO no te deja entrar: «{que}» (estado {n}). Revisa el usuario, la contraseña y el código."))),
        None => Err(cambiado("cómo se entra (login/secure/login)", "«sessionId» o un «status»", &v)),
    }
}

/* ================= pedir y leer las respuestas ================= */

fn pedir_json(c: &Cliente, url: &str, que: &str) -> Result<Value, Error> {
    let r = c.get(url, Some(REFERER)).map_err(Error::de)?;
    json_de(&r, que)
}

fn json_de(r: &Respuesta, que: &str) -> Result<Value, Error> {
    if r.codigo != 200 {
        return Err(codigo_mal(r, que));
    }
    serde_json::from_slice(&r.cuerpo).map_err(|_| {
        let tipo = r.cabecera("content-type").unwrap_or("sin tipo");
        Error::de(format!("DEGIRO ha cambiado {que}: esperaba JSON y ha llegado otra cosa ({tipo}, {} bytes).", r.cuerpo.len()))
    })
}

fn codigo_mal(r: &Respuesta, que: &str) -> Error {
    match r.codigo {
        401 | 403 => Error::de(format!("DEGIRO no deja ver {que} (código {}): la sesión ha caducado o te ha cerrado el acceso. Vuelve a probar.", r.codigo)),
        429 => Error::de(format!("DEGIRO pide esperar al pedir {que} (demasiadas peticiones). Prueba dentro de un rato.")),
        c => Error::de(format!("DEGIRO ha contestado con el código {c} al pedir {que}: esperaba 200.")),
    }
}

/// Qué ha cambiado: qué se esperaba y la forma de lo que ha llegado (las claves y los tipos, sin ningún valor).
fn cambiado(que: &str, esperaba: &str, llego: &Value) -> Error {
    Error::de(format!("DEGIRO ha cambiado {que}: esperaba {esperaba}; ha llegado {}.", forma(llego, 2)))
}

/// La forma de un JSON, sin sus valores: `{data: [3 × {date: texto, price: número, …}]}`.
fn forma(v: &Value, hondo: usize) -> String {
    let s = match v {
        Value::Null => "nulo".into(),
        Value::Bool(_) => "sí/no".into(),
        Value::Number(_) => "número".into(),
        Value::String(_) => "texto".into(),
        Value::Array(a) if a.is_empty() => "[]".into(),
        Value::Array(a) if hondo == 0 => format!("[{} elementos]", a.len()),
        Value::Array(a) => format!("[{} × {}]", a.len(), forma(&a[0], hondo - 1)),
        Value::Object(m) if hondo == 0 || m.is_empty() => format!("{{{}}}", m.keys().take(8).cloned().collect::<Vec<_>>().join(", ")),
        Value::Object(m) => {
            let mut v: Vec<String> = m.iter().take(8).map(|(k, x)| format!("{k}: {}", forma(x, hondo - 1))).collect();
            if m.len() > 8 {
                v.push("…".into());
            }
            format!("{{{}}}", v.join(", "))
        }
    };
    if s.chars().count() > 400 { s.chars().take(399).chain(['…']).collect() } else { s }
}

fn en<'a>(v: &'a Value, ruta: &[&str]) -> Option<&'a Value> {
    ruta.iter().try_fold(v, |x, k| x.get(k))
}

fn numero(v: &Value, ruta: &[&str]) -> Option<f64> {
    en(v, ruta).and_then(Value::as_f64)
}

fn texto<'a>(v: &'a Value, campo: &str) -> Option<&'a str> {
    v.get(campo).and_then(Value::as_str)
}

/// Un id (número o texto) como texto.
fn id(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn lista(v: &Value, ruta: &[&str], que: &str) -> Result<Vec<Value>, Error> {
    match en(v, ruta) {
        Some(Value::Array(a)) => Ok(a.clone()),
        _ => Err(cambiado(que, &format!("una lista en «{}»", ruta.join(".")), v)),
    }
}

/// Los movimientos de efectivo de un año: `data.cashMovements`, o `data: {}` si no hay ninguno ([gds]).
fn movimientos_de(v: &Value, anio: i32) -> Result<Vec<Value>, Error> {
    match en(v, &["data"]) {
        Some(Value::Object(m)) if m.is_empty() => Ok(Vec::new()),
        _ => lista(v, &["data", "cashMovements"], &format!("tus movimientos de {anio} (v6/accountoverview)")),
    }
}

/* ================= traducir al formato común (sin red) ================= */

/// Lo que se sabe de un producto.
#[derive(Debug, Default)]
struct Ficha {
    isin: String,
    nombre: String,
    producto: Producto,
    divisa: String,
}

fn producto_de(id: &str, productos: &Map<String, Value>) -> Ficha {
    let Some(p) = productos.get(id) else { return Ficha { nombre: format!("producto {id} de DEGIRO"), ..Default::default() } };
    // [so] constants.py: productTypeId 1 acción, 2 bono, 13 fondo, 131 ETF.
    let producto = match p.get("productTypeId").and_then(Value::as_i64) {
        Some(1) => Producto::Accion,
        Some(2) => Producto::Bono,
        Some(13) => Producto::Fondo,
        Some(131) => Producto::Etf,
        _ => match texto(p, "productType").unwrap_or_default() {
            "STOCK" => Producto::Accion,
            "ETF" => Producto::Etf,
            "FUND" => Producto::Fondo,
            "BOND" => Producto::Bono,
            _ => Producto::Otro,
        },
    };
    Ficha { isin: texto(p, "isin").unwrap_or_default().to_string(), nombre: texto(p, "name").unwrap_or_default().to_string(), producto, divisa: texto(p, "currency").unwrap_or_default().to_string() }
}

/// «2026-03-02T15:32:10+01:00» → «2026-03-02T15:32:10».
fn fecha_de(v: &Value, campo: &str) -> Option<String> {
    let f = texto(v, campo)?;
    fmt::fecha(f)?;
    Some(f.chars().take(19).collect())
}

/// Todo lo traído, al formato común. `informes`: (fecha, CSV del informe de cartera a esa fecha).
fn traducir(operaciones: &[Value], movimientos: &[Value], productos: &Map<String, Value>, informes: &[(String, String)], hoy: &str) -> Result<Cartera, Error> {
    let mut c = Cartera { broker: "DEGIRO".into(), hasta: hoy.into(), ..Default::default() };
    for (i, o) in operaciones.iter().enumerate() {
        if let Some(op) = operacion(o, i, productos, &mut c)? {
            c.operaciones.push(op);
        }
    }
    c.operaciones.sort_by(|a, b| a.fecha.cmp(&b.fecha));
    c.cobros = cobros(movimientos, &c.operaciones, productos, &mut c.avisos)?;
    // Qué es cada ISIN (para saber si una posición es un fondo).
    let tipos: BTreeMap<&str, Producto> = c.operaciones.iter().map(|o| (o.isin.as_str(), o.producto)).collect();
    for (fecha, csv) in informes {
        c.posiciones.extend(posiciones(fecha, csv, &tipos)?);
        c.cierres.push(fecha.clone());
    }
    Ok(c)
}

/// Una operación (`data[i]` de v4/transactions, [dc] transaction.py): una compraventa, o None si es otra cosa (se
/// apunta en `avisos`, y desde ese día el coste de ese valor no se sabe: `sin_coste_desde`).
fn operacion(o: &Value, i: usize, productos: &Map<String, Value>, c: &mut Cartera) -> Result<Option<Operacion>, Error> {
    let que = || format!("tus operaciones (v4/transactions, la {}.ª)", i + 1);
    let falta = |campo: &str, como: &str| cambiado(&que(), &format!("«{campo}» ({como})"), o);
    let fecha = fecha_de(o, "date").ok_or_else(|| falta("date", "una fecha"))?;
    let pid = id(o.get("productId")).ok_or_else(|| falta("productId", "un id"))?;
    let p = producto_de(&pid, productos);
    let dia = fmt::dmy(&fecha[..10]);
    let isin = if p.isin.is_empty() { format!("DEGIRO-{pid}") } else { p.isin.clone() };
    let mut sin_coste = |aviso: String| {
        c.avisos.push(aviso);
        let desde = c.sin_coste_desde.entry(isin.clone()).or_insert_with(|| fecha.clone());
        if fecha < *desde {
            *desde = fecha.clone();
        }
        Ok(None)
    };
    // Sin su tipo no se sabe si es una compraventa (0 lo es; viene en el ejemplo de [so], `transactions_data.json`).
    let tipo = o.get("transactionTypeId").and_then(Value::as_i64).ok_or_else(|| falta("transactionTypeId", "un número"))?;
    if tipo != 0 {
        // [so]: 101 split, 106 operación de la empresa, 108 fusión, 112 exclusión, 204 cambio de producto.
        return sin_coste(format!("El {dia}, en {}, hay una operación que no es una compra ni una venta (un split, una fusión, un cambio de ISIN…; tipo {tipo} de DEGIRO): no la cuento, y lo que vendas de ese valor desde entonces no entra en las cuentas.", p.nombre));
    }
    if o.get("transfered").and_then(Value::as_bool) == Some(true) {
        return sin_coste(format!("El {dia} entraron o salieron por traspaso títulos de {}: no es una compra ni una venta, y la app no sabe lo que te costaron; lo que vendas de ese valor desde entonces no entra en las cuentas.", p.nombre));
    }
    let compra = match texto(o, "buysell") {
        Some("B") => true,
        Some("S") => false,
        _ => return Err(falta("buysell", "B o S")),
    };
    let num = |campo: &str| o.get(campo).and_then(Value::as_f64);
    let cantidad = num("quantity").ok_or_else(|| falta("quantity", "un número"))?.abs();
    let precio = num("price").ok_or_else(|| falta("price", "un número"))?;
    // El cambio: el de la operación; si no viene, el que sale de lo pagado en la divisa y en euros (0: no se sabe).
    let total = num("total").map(f64::abs);
    let en_euros = num("totalInBaseCurrency").map(f64::abs);
    let cambio = match (num("fxRate").filter(|x| *x > 0.0), total, en_euros) {
        (Some(x), _, _) => x,
        _ if p.divisa == "EUR" => 1.0,
        (None, Some(t), Some(e)) if t > 0.0 && e > 0.0 => t / e,
        _ => 0.0,
    };
    let euros = match (en_euros, total) {
        (Some(x), _) => x,
        (None, Some(t)) if cambio > 0.0 => t / cambio,
        _ => return Err(falta("totalInBaseCurrency", "un número")),
    };
    // [gds]: totalFeesInBaseCurrency = feeInBaseCurrency + autoFxFeeInBaseCurrency, en negativo. Sin ninguno de los
    // tres, no son 0: no se sabe.
    let comisiones = match (num("totalFeesInBaseCurrency"), num("feeInBaseCurrency"), num("autoFxFeeInBaseCurrency")) {
        (Some(t), _, _) => t,
        (None, None, None) => return Err(falta("totalFeesInBaseCurrency", "un número, o feeInBaseCurrency y autoFxFeeInBaseCurrency")),
        (None, f, x) => f.unwrap_or(0.0) + x.unwrap_or(0.0),
    }
    .abs();
    if p.isin.is_empty() {
        c.avisos.push(format!("El {dia} hay una operación de un producto que DEGIRO no identifica (id {pid}): la cuento aparte."));
    }
    Ok(Some(Operacion { fecha, isin, nombre: p.nombre, producto: p.producto, compra, cantidad, precio, divisa: p.divisa, cambio, euros, comisiones }))
}

/// Qué es un movimiento de efectivo, por su descripción: DEGIRO la escribe en el idioma de tu cuenta ([dr] en
/// español; [so] y [gds] en inglés, neerlandés, alemán y francés).
#[derive(Debug, PartialEq, Clone, Copy)]
enum Movimiento {
    Dividendo,
    Retencion,
    Interes,
    /// Compras, ventas, comisiones, ingresos, retiradas, cambios de divisa…: no es un cobro.
    Otro,
    /// Con un producto, y no se sabe qué es.
    Desconocido,
}

fn movimiento(descripcion: &str, con_producto: bool) -> Movimiento {
    let d = sin_tildes(descripcion);
    let empieza = |v: &[&str]| v.iter().any(|p| d.starts_with(p));
    let tiene = |v: &[&str]| v.iter().any(|p| d.contains(p));
    if tiene(&["retencion del dividendo", "impuesto sobre dividendo", "dividend tax", "dividendbelasting", "dividendensteuer", "impots sur dividende"]) {
        Movimiento::Retencion
    } else if tiene(&["comision", "coste", "fee", "kosten", "frais", "gebuhr"]) {
        Movimiento::Otro
    } else if empieza(&["dividendo", "reparto de dividendo", "pago de dividendo", "dividend"]) {
        Movimiento::Dividendo
    } else if d.split(|c: char| !c.is_alphanumeric()).any(|p| ["interes", "intereses", "interest", "rente", "zinsen", "interets"].contains(&p)) {
        Movimiento::Interes
    } else if !con_producto
        || empieza(&["compra", "venta", "buy", "sell", "koop", "verkoop", "achat", "vente", "kauf", "verkauf"])
        || tiene(&["cambio de divisa", "valuta", "currency", "change", "transaction tax", "tasa", "impuesto"])
    {
        Movimiento::Otro
    } else {
        Movimiento::Desconocido
    }
}

/// Los dividendos e intereses: cada dividendo con su retención (la del mismo producto, divisa y día, [gds]).
fn cobros(movimientos: &[Value], operaciones: &[Operacion], productos: &Map<String, Value>, avisos: &mut Vec<String>) -> Result<Vec<Cobro>, Error> {
    // Los cambios que se ven: los de cada movimiento en otra divisa que trae `exchangeRate` y los de las operaciones.
    let mut cambios: Vec<(String, String, f64)> = operaciones.iter().filter(|o| o.divisa != "EUR" && o.cambio > 0.0).map(|o| (o.divisa.clone(), o.fecha[..10].to_string(), o.cambio)).collect();
    for m in movimientos {
        if let (Some(d), Some(f), Some(x)) = (texto(m, "currency"), fecha_de(m, "date"), m.get("exchangeRate").and_then(Value::as_f64))
            && d != "EUR"
            && x > 0.0
        {
            cambios.push((d.to_string(), f[..10].to_string(), x));
        }
    }
    let mut grupos: BTreeMap<(String, String, String, bool), Cobro> = BTreeMap::new();
    for (i, m) in movimientos.iter().enumerate() {
        let que = || format!("tus movimientos (v6/accountoverview, el {}.º)", i + 1);
        let Some(descripcion) = texto(m, "description") else { return Err(cambiado(&que(), "«description» (un texto)", m)) };
        let pid = id(m.get("productId"));
        let tipo = movimiento(descripcion, pid.is_some());
        if tipo == Movimiento::Otro {
            continue;
        }
        let fecha = fecha_de(m, "valueDate").or_else(|| fecha_de(m, "date")).ok_or_else(|| cambiado(&que(), "«date» (una fecha)", m))?;
        let dia = fecha[..10].to_string();
        let cambio = m.get("change").and_then(Value::as_f64).ok_or_else(|| cambiado(&que(), "«change» (un número)", m))?;
        let divisa = texto(m, "currency").ok_or_else(|| cambiado(&que(), "«currency» (un texto)", m))?.to_string();
        let p = pid.as_deref().map(|id| producto_de(id, productos)).unwrap_or_default();
        if tipo == Movimiento::Desconocido {
            avisos.push(format!("El {} hay un movimiento de {} que no sé qué es («{descripcion}», {} {divisa}): no lo cuento.", fmt::dmy(&dia), p.nombre, fmt::num(cambio)));
            continue;
        }
        // Los intereses que pagas (por un descubierto) no son un cobro.
        if tipo == Movimiento::Interes && cambio <= 0.0 {
            continue;
        }
        let clase = if tipo == Movimiento::Interes { Clase::Interes } else { Clase::Dividendo };
        let clave = (dia.clone(), p.isin.clone(), divisa.clone(), clase == Clase::Interes);
        let c = grupos.entry(clave).or_insert_with(|| Cobro {
            fecha: dia.clone(),
            clase,
            isin: p.isin.clone(),
            nombre: if clase == Clase::Interes && p.nombre.is_empty() { "Intereses de la cuenta".into() } else { p.nombre.clone() },
            cambio: if divisa == "EUR" { Some(1.0) } else { cambio_de(&cambios, &divisa, &dia) },
            divisa: divisa.clone(),
            ..Default::default()
        });
        match tipo {
            Movimiento::Retencion if p.isin.starts_with("ES") => c.retenido_espana -= cambio,
            Movimiento::Retencion => c.retenido_origen -= cambio,
            _ => c.bruto += cambio,
        }
    }
    let v: Vec<Cobro> = grupos.into_values().map(|mut c| {
        c.bruto = fmt::r2(c.bruto);
        c.retenido_origen = fmt::r2(c.retenido_origen);
        c.retenido_espana = fmt::r2(c.retenido_espana);
        c
    }).collect();
    for c in v.iter().filter(|c| c.cambio.is_none()) {
        avisos.push(format!("No sé a cuánto estaba el {} el {}: el cobro de {} ({} {}) no entra en las cuentas.", c.divisa, fmt::dmy(&c.fecha), c.nombre, fmt::num(c.bruto), c.divisa));
    }
    Ok(v)
}

/// En minúsculas y sin tildes, para buscar en las descripciones.
fn sin_tildes(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'í' | 'ì' | 'î' => 'i',
            'ó' | 'ò' | 'ô' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

/// El cambio de una divisa un día: el de ese día o, si no hay, el más cercano en una semana.
fn cambio_de(cambios: &[(String, String, f64)], divisa: &str, dia: &str) -> Option<f64> {
    cambios.iter().filter(|(d, f, _)| d == divisa && fmt::dias_entre(f, dia).abs() <= 7).min_by_key(|(_, f, _)| fmt::dias_entre(f, dia).abs()).map(|(_, _, x)| *x)
}

/// Lo que había en la cuenta en una fecha, del informe de cartera en CSV (cabeceras en español o en inglés).
fn posiciones(fecha: &str, csv: &str, tipos: &BTreeMap<&str, Producto>) -> Result<Vec<Posicion>, Error> {
    let que = format!("tu informe de cartera a {}", fmt::dmy(fecha));
    let filas: Vec<Vec<String>> = csv.lines().filter(|l| !l.trim().is_empty()).map(campos_csv).collect();
    let Some(cabecera) = filas.first() else { return Ok(Vec::new()) };
    let col = |nombres: &[&str]| cabecera.iter().position(|c| nombres.iter().any(|n| c.trim().eq_ignore_ascii_case(n)));
    let (Some(nombre), Some(isin), Some(cantidad), Some(euros)) = (col(&["Producto", "Product"]), col(&["Symbol/ISIN"]), col(&["Cantidad", "Amount", "Quantity"]), col(&["Valor en EUR", "Value in EUR", "EUR value"])) else {
        return Err(Error::de(format!("DEGIRO ha cambiado {que}: esperaba las columnas «Producto», «Symbol/ISIN», «Cantidad» y «Valor en EUR»; han llegado «{}».", cabecera.join("», «"))));
    };
    let mut v = Vec::new();
    for (i, f) in filas.iter().enumerate().skip(1) {
        let celda = |k: usize| f.get(k).map(|s| s.trim()).unwrap_or("");
        let valor = celda(euros);
        if !fmt::es_num(valor) || valor.is_empty() {
            return Err(Error::de(format!("DEGIRO ha cambiado {que}: en la fila {} esperaba un importe en «Valor en EUR» y ha llegado otra cosa.", i + 1)));
        }
        let isin_ = celda(isin).to_string();
        let efectivo = isin_.is_empty();
        let bien = if efectivo {
            Bien::Cuenta
        } else {
            match tipos.get(isin_.as_str()) {
                Some(Producto::Etf | Producto::Fondo) => Bien::Iic,
                _ => Bien::Valores,
            }
        };
        let (pais, depositario) = if efectivo { DEPOSITO_EFECTIVO } else { DEPOSITO_VALORES };
        let euros = fmt::parse_num(valor);
        if euros.abs() < fmt::DIF {
            continue;
        }
        v.push(Posicion { fecha: fecha.into(), bien, isin: isin_, nombre: celda(nombre).to_string(), cantidad: fmt::parse_num(celda(cantidad)), euros, pais: pais.into(), depositario: depositario.into() });
    }
    Ok(v)
}

/// Los campos de una línea de CSV, con comillas («"8,47"») y comillas dobles dentro.
fn campos_csv(linea: &str) -> Vec<String> {
    let mut v = vec![String::new()];
    let mut entre = false;
    let mut it = linea.trim_end_matches('\r').chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '"' if entre && it.peek() == Some(&'"') => {
                it.next();
                v.last_mut().unwrap().push('"');
            }
            '"' => entre = !entre,
            ',' if !entre => v.push(String::new()),
            c => v.last_mut().unwrap().push(c),
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn productos() -> Map<String, Value> {
        let v = json!({
            "101": { "id": "101", "name": "Empresa Inventada", "isin": "US0000000001", "currency": "USD", "productType": "STOCK", "productTypeId": 1 },
            "102": { "id": "102", "name": "Fondo Cotizado Inventado", "isin": "IE0000000002", "currency": "EUR", "productType": "ETF", "productTypeId": 131 },
            "103": { "id": "103", "name": "Banco Inventado", "isin": "ES0000000003", "currency": "EUR", "productTypeId": 1 }
        });
        v.as_object().unwrap().clone()
    }

    /// Una compra en dólares y una venta en euros, como las da v4/transactions (datos inventados, con la forma de
    /// los ejemplos de [so] y [gds]).
    fn operaciones() -> Vec<Value> {
        vec![
            json!({ "id": 1, "productId": 101, "date": "2025-03-11T18:01:46+01:00", "buysell": "B", "price": 275.0, "quantity": 10, "total": -2750.0,
                    "fxRate": 1.1, "totalInBaseCurrency": -2500.0, "feeInBaseCurrency": -0.5, "autoFxFeeInBaseCurrency": -2.5,
                    "totalFeesInBaseCurrency": -3.0, "transactionTypeId": 0, "transfered": false }),
            json!({ "id": 2, "productId": 102, "date": "2026-02-01T10:00:00+01:00", "buysell": "S", "price": 50.0, "quantity": -4, "total": 200.0,
                    "fxRate": 0, "totalInBaseCurrency": 200.0, "feeInBaseCurrency": -1.0, "transactionTypeId": 0 }),
            json!({ "id": 3, "productId": 101, "date": "2026-05-01T10:00:00+02:00", "buysell": "B", "price": 0.0, "quantity": 10, "transactionTypeId": 101 }),
        ]
    }

    #[test]
    fn las_operaciones_al_formato_comun() {
        let c = traducir(&operaciones(), &[], &productos(), &[], "2026-11-20").unwrap();
        assert_eq!(c.operaciones.len(), 2, "el split no es una compraventa");
        let compra = &c.operaciones[0];
        assert_eq!((compra.fecha.as_str(), compra.isin.as_str(), compra.compra, compra.cantidad, compra.divisa.as_str()), ("2025-03-11T18:01:46", "US0000000001", true, 10.0, "USD"));
        assert_eq!((compra.cambio, compra.euros, compra.comisiones, compra.producto), (1.1, 2500.0, 3.0, Producto::Accion));
        let venta = &c.operaciones[1];
        assert_eq!((venta.compra, venta.cantidad, venta.euros, venta.comisiones, venta.cambio, venta.producto), (false, 4.0, 200.0, 1.0, 1.0, Producto::Etf));
        assert!(c.avisos.iter().any(|a| a.contains("split")), "{:?}", c.avisos);
        // Y desde el split, el coste de ese valor ya no se sabe (con el de antes saldría otra cosa).
        assert_eq!(c.sin_coste_desde, [("US0000000001".to_string(), "2026-05-01T10:00:00".to_string())].into());
        // Un traspaso, igual (el primero, si hay varios).
        let mut o = operaciones();
        o.push(json!({ "id": 4, "productId": 102, "date": "2026-07-01T10:00:00+02:00", "buysell": "B", "price": 50.0, "quantity": 3, "transactionTypeId": 0, "transfered": true }));
        o.push(json!({ "id": 5, "productId": 102, "date": "2026-06-01T10:00:00+02:00", "buysell": "S", "price": 50.0, "quantity": -3, "transactionTypeId": 0, "transfered": true }));
        let c = traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap();
        assert_eq!(c.sin_coste_desde.get("IE0000000002").map(String::as_str), Some("2026-06-01T10:00:00"));
    }

    #[test]
    fn sin_fx_rate_el_cambio_sale_de_los_importes() {
        let mut o = operaciones();
        o[0].as_object_mut().unwrap().remove("fxRate");
        let c = traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap();
        assert_eq!((c.operaciones[0].cambio, c.operaciones[0].euros), (1.1, 2500.0));
        o[0].as_object_mut().unwrap().remove("total");
        let c = traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap();
        assert_eq!(c.operaciones[0].cambio, 0.0, "no se sabe: no sirve para pasar dividendos a euros");
    }

    #[test]
    fn si_cambian_las_operaciones_lo_dice() {
        let mut o = operaciones();
        o[0].as_object_mut().unwrap().remove("buysell");
        let e = traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap_err().mensaje;
        assert!(e.contains("esperaba «buysell» (B o S)") && e.contains("ha llegado {") && e.contains("price: número"), "{e}");
        assert!(!e.contains("275"), "sin valores: {e}");
        // Sin su tipo, no se toma por una compraventa; sin ninguna comisión, no se toman por 0 (con solo
        // `feeInBaseCurrency`, como la 2.ª, sí vale).
        let mut o = operaciones();
        o[0].as_object_mut().unwrap().remove("transactionTypeId");
        assert!(traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap_err().mensaje.contains("«transactionTypeId»"));
        let mut o = operaciones();
        for k in ["totalFeesInBaseCurrency", "feeInBaseCurrency", "autoFxFeeInBaseCurrency"] {
            o[0].as_object_mut().unwrap().remove(k);
        }
        assert!(traducir(&o, &[], &productos(), &[], "2026-11-20").unwrap_err().mensaje.contains("«totalFeesInBaseCurrency»"));
        // Un dividendo sin su importe («change»): no se salta.
        let m = vec![json!({ "id": 10, "date": "2026-03-12T08:16:38+01:00", "description": "Dividendo", "productId": 101, "currency": "USD", "type": "CASH_TRANSACTION" })];
        assert!(traducir(&[], &m, &productos(), &[], "2026-11-20").unwrap_err().mensaje.contains("«change»"));
    }

    #[test]
    fn dividendos_con_su_retencion_y_su_cambio() {
        let m = vec![
            json!({ "id": 10, "date": "2026-03-12T08:16:38+01:00", "valueDate": "2026-03-11T23:59:59+01:00", "description": "Dividendo", "productId": 101, "currency": "USD", "change": 10.0, "type": "CASH_TRANSACTION" }),
            json!({ "id": 11, "date": "2026-03-12T08:16:38+01:00", "valueDate": "2026-03-11T23:59:59+01:00", "description": "Retención del dividendo", "productId": 101, "currency": "USD", "change": -1.5, "type": "CASH_TRANSACTION" }),
            json!({ "id": 12, "date": "2026-03-12T09:00:00+01:00", "description": "Retirada Cambio de Divisa", "currency": "USD", "change": -8.5, "exchangeRate": 1.25, "type": "CASH_TRANSACTION" }),
            json!({ "id": 13, "date": "2026-04-01T08:00:00+02:00", "description": "Dividendo", "productId": 103, "currency": "EUR", "change": 20.0, "type": "CASH_TRANSACTION" }),
            json!({ "id": 14, "date": "2026-04-01T08:00:00+02:00", "description": "Retención del dividendo", "productId": 103, "currency": "EUR", "change": -3.8, "type": "CASH_TRANSACTION" }),
            json!({ "id": 15, "date": "2026-05-01T08:00:00+02:00", "description": "Flatex Interest Income", "currency": "EUR", "change": 2.0, "type": "CASH_TRANSACTION" }),
            json!({ "id": 16, "date": "2026-05-02T08:00:00+02:00", "description": "Comisión de conectividad con el mercado 2026", "currency": "EUR", "change": -2.5, "type": "CASH_TRANSACTION" }),
            json!({ "id": 17, "date": "2026-05-03T08:00:00+02:00", "description": "Compra 4 Fondo@50 EUR", "productId": 102, "currency": "EUR", "change": -200.0, "type": "TRANSACTION" }),
            json!({ "id": 18, "date": "2026-05-04T08:00:00+02:00", "description": "Algo nuevo", "productId": 102, "currency": "EUR", "change": 1.0, "type": "CASH_TRANSACTION" }),
            json!({ "id": 19, "date": "2026-06-01T08:00:00+02:00", "description": "Dividendo", "productId": 101, "currency": "USD", "change": 5.0, "type": "CASH_TRANSACTION" }),
        ];
        let c = traducir(&[], &m, &productos(), &[], "2026-11-20").unwrap();
        assert_eq!(c.cobros.len(), 4, "{:#?}", c.cobros);
        let usd = c.cobros.iter().find(|x| x.fecha == "2026-03-11").unwrap();
        assert_eq!((usd.clase, usd.bruto, usd.retenido_origen, usd.retenido_espana, usd.cambio), (Clase::Dividendo, 10.0, 1.5, 0.0, Some(1.25)));
        assert_eq!(usd.en_euros(usd.bruto), Some(8.0));
        let es = c.cobros.iter().find(|x| x.isin == "ES0000000003").unwrap();
        assert_eq!((es.bruto, es.retenido_espana, es.retenido_origen, es.cambio), (20.0, 3.8, 0.0, Some(1.0)));
        let interes = c.cobros.iter().find(|x| x.clase == Clase::Interes).unwrap();
        assert_eq!((interes.bruto, interes.nombre.as_str()), (2.0, "Intereses de la cuenta"));
        // Sin cambio a menos de una semana: fuera de las cuentas, y se dice.
        assert_eq!(c.cobros.iter().find(|x| x.fecha == "2026-06-01").unwrap().cambio, None);
        assert!(c.avisos.iter().any(|a| a.contains("Algo nuevo")) && c.avisos.iter().any(|a| a.contains("No sé a cuánto estaba el USD")), "{:?}", c.avisos);
        // Sin movimientos, `data: {}`; otra forma, un error.
        assert_eq!(movimientos_de(&json!({ "data": {} }), 2026).unwrap().len(), 0);
        assert!(movimientos_de(&json!({ "datos": [] }), 2026).unwrap_err().mensaje.contains("esperaba una lista en «data.cashMovements»"));
    }

    #[test]
    fn la_cartera_a_una_fecha() {
        let csv = "Producto,Symbol/ISIN,Cantidad,Precio de,Valor local,,Valor en EUR\n\
                   Fondo Cotizado Inventado,IE0000000002,48,\"86,50\",EUR,\"4152,00\",\"4152,00\"\n\
                   Empresa Inventada,US0000000001,10,\"300,00\",USD,\"3000,00\",\"2.700,50\"\n\
                   CASH & CASH FUND & FTX CASH (EUR),,,,EUR,\"2680,70\",\"2680,70\"\n";
        let tipos: BTreeMap<&str, Producto> = [("IE0000000002", Producto::Etf)].into();
        let v = posiciones("2025-12-31", csv, &tipos).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].bien, v[0].euros, v[0].pais.as_str()), (Bien::Iic, 4152.0, "NL"));
        assert_eq!((v[1].bien, v[1].euros, v[1].cantidad), (Bien::Valores, 2700.5, 10.0));
        assert_eq!((v[2].bien, v[2].euros, v[2].pais.as_str(), v[2].isin.as_str()), (Bien::Cuenta, 2680.7, "DE", ""));
        // El formato antiguo, sin la columna vacía.
        let viejo = "Producto,Symbol/ISIN,Cantidad,Precio de,Valor local,Valor en EUR\nCASH & CASH FUND & FTX CASH (EUR),,,,EUR 564.19,\"564,19\"\n";
        assert_eq!(posiciones("2019-12-31", viejo, &tipos).unwrap()[0].euros, 564.19);
        let e = posiciones("2025-12-31", "Product;ISIN;Value\n", &tipos).unwrap_err().mensaje;
        assert!(e.contains("esperaba las columnas") && e.contains("«Product;ISIN;Value»"), "{e}");
        assert_eq!(campos_csv("a,\"b,\"\"c\"\"\",,d"), vec!["a", "b,\"c\"", "", "d"]);
    }

    #[test]
    fn al_entrar() {
        use super::al_entrar as a;
        assert_eq!(a(200, br#"{"sessionId":"ABC","status":0}"#).unwrap(), Entrada::Dentro("ABC".into()));
        assert!(a(400, br#"{"status":6,"statusText":"totpNeeded"}"#).unwrap_err().mensaje.contains("código de tu app"));
        assert_eq!(a(400, br#"{"status":12,"inAppToken":"T"}"#).unwrap(), Entrada::EnLaApp("T".into()));
        assert_eq!(a(400, br#"{"status":3,"statusText":"x"}"#).unwrap(), Entrada::Esperando("x".into()));
        assert!(a(400, br#"{"captchaRequired":true,"status":0}"#).unwrap_err().mensaje.contains("captcha"));
        assert!(a(400, br#"{"status":1,"statusText":"badCredentials"}"#).unwrap_err().mensaje.contains("«badCredentials»"));
        assert!(a(429, b"").unwrap_err().mensaje.contains("esperar"));
        assert!(a(405, b"<html>").unwrap_err().mensaje.contains("mantenimiento"));
        assert!(a(200, b"<html>").unwrap_err().mensaje.contains("esperaba una respuesta en JSON"));
        assert!(a(200, br#"{"x":1}"#).unwrap_err().mensaje.contains("ha llegado {x: número}"));
    }

    #[test]
    fn que_es_cada_movimiento() {
        use Movimiento::*;
        for (d, p, t) in [
            ("Dividendo", true, Dividendo),
            ("Reparto de dividendo", true, Dividendo),
            ("Dividend", true, Dividendo),
            ("Retención del dividendo", true, Retencion),
            ("Dividend Tax", true, Retencion),
            ("Comisión de procesamiento de dividendos", true, Otro),
            ("Interés", false, Interes),
            ("Flatex Interest Income", false, Interes),
            ("Ingreso", false, Otro),
            ("Transferir desde su Cuenta corriente", false, Otro),
            ("Rente", false, Interes),
            ("Compra 4 X@50 EUR", true, Otro),
            ("Costes de transacción y/o externos de DEGIRO", true, Otro),
            ("Algo nuevo", true, Desconocido),
        ] {
            assert_eq!(movimiento(d, p), t, "{d}");
        }
    }
}
