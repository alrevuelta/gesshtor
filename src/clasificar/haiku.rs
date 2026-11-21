//! Preguntar a Claude Haiku por un documento, de una de dos formas (ver `Modo`): con el comando `claude` de Claude
//! Code, que solo puede leer ese fichero (una copia en una carpeta temporal), o con la API de Anthropic y tu clave,
//! con el documento en base64. Las dos piden la respuesta en JSON con un esquema (salida estructurada), pero eso no
//! la hace de fiar: se comprueba en `entendido`.
//!
//! Los prompts son pequeños: uno para saber qué es y otro para sacar los datos de cada cosa. Solo describen lo que
//! hay que copiar del documento; lo fiscal (qué IVA se deduce, qué operación es) lo decide la app.
use super::{Fichero, Modo};
use crate::dominio::gasto::CATS;
use crate::red::cliente::{Cliente, Cuerpo, DOMINIOS_ANTHROPIC};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// El modelo, por la API (el id exacto) y con el comando `claude` (su alias).
const MODELO_API: &str = "claude-haiku-5-5";
const MODELO_CLAUDE: &str = "haiku";
/// La versión de la API de mensajes.
const VERSION_API: &str = "2026-10-21";
const URL_API: &str = "https://api.anthropic.com/v1/messages";
/// Precio de Claude Haiku 5.5 por millón de tokens (con prompts de hasta 100.000 tokens): entrada y salida, en dólares.
const PRECIO: (f64, f64) = (0.10, 0.50);
/// Lo más que se espera al comando `claude`, y lo más que puede gastar en una pregunta.
const ESPERA_CLAUDE: Duration = Duration::from_secs(180);
const TOPE_DOLARES: &str = "0.25";

const SISTEMA: &str = "Lees documentos de una persona que vive en España para llevar sus cuentas. Contesta solo con el JSON pedido. Copia los datos tal y como salen en el documento; si un dato no sale, déjalo vacío (o 0) en vez de inventarlo.";

/// Una pregunta: el texto, el esquema de la respuesta y el esfuerzo (`low`, `medium`…).
pub struct Pregunta {
    pub texto: String,
    pub esquema: Value,
    pub esfuerzo: &'static str,
}

/// Un objeto JSON con todos los campos obligatorios y ninguno más.
fn objeto(campos: Value) -> Value {
    let requeridos: Vec<String> = campos.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    json!({"type": "object", "properties": campos, "required": requeridos, "additionalProperties": false})
}

/// ¿Qué es?
pub fn clase() -> Pregunta {
    let texto = "¿Qué es este documento?
- nomina: el recibo de salarios que una empresa da a su trabajador, con cotizaciones a la Seguridad Social y retención de IRPF.
- gasto: una factura, tique o recibo de algo que la persona ha comprado o pagado.
- ingreso: lo que cobra la persona por su trabajo por cuenta propia: una factura que emite a un cliente, o el aviso de pago de un cliente (sin cotizaciones a la Seguridad Social española).
- otro: cualquier otra cosa.
En «que», di en pocas palabras qué es (por ejemplo, «factura de un hotel»).";
    let esquema = objeto(json!({"tipo": {"type": "string", "enum": ["nomina", "gasto", "ingreso", "otro"]}, "que": {"type": "string"}}));
    Pregunta { texto: texto.into(), esquema, esfuerzo: "low" }
}

/// Los datos de una nómina.
pub fn nomina() -> Pregunta {
    let texto = "Es una nómina. Saca:
- empresa: el nombre de la empresa.
- anio y mes: a los que corresponde (los del periodo; si no hay periodo, los de la fecha de la nómina).
- total_devengado: el total devengado.
- en_especie: lo que no se cobra en dinero (acciones, RSU, seguro médico, coche…); 0 si no hay.
- irpf: la retención de IRPF.
- seguridad_social: la suma de todas las cotizaciones del trabajador (contingencias comunes, desempleo, formación, MEI, solidaridad…).
- total_a_deducir y liquido (el líquido a percibir, que puede ser negativo).";
    let n = json!({"type": "number"});
    let esquema = objeto(json!({
        "empresa": {"type": "string"}, "anio": {"type": "integer"}, "mes": {"type": "integer"},
        "total_devengado": n, "en_especie": n, "irpf": n, "seguridad_social": n, "total_a_deducir": n, "liquido": n,
    }));
    Pregunta { texto: texto.into(), esquema, esfuerzo: "medium" }
}

/// Los datos de un gasto, con las categorías de la app.
pub fn gasto() -> Pregunta {
    let cats: Vec<String> = CATS.iter().map(|c| format!("  {}: {}", c.id, c.label)).collect();
    let texto = format!(
        "Es un gasto (una factura o un tique). Saca:
- proveedor, proveedor_nif y proveedor_pais (código de 2 letras: ES, IE, US…).
- cliente_nif: el NIF, CIF o número de IVA del cliente si sale; si no, vacío.
- numero: el número de la factura o del tique; si no hay, vacío.
- fecha: la de la factura o la del pago (AAAA-MM-DD).
- concepto: qué se ha comprado, en pocas palabras.
- categoria: la que mejor encaje:
{}
- moneda: EUR, USD…
- base: el importe sin impuestos.
- iva_tipo e iva_cuota: el % y la cuota de IVA (o VAT); sin IVA, 0 y 0.
- iva_pais: el país del IVA cobrado (ES si es IVA español); sin IVA, vacío.
- iva_desglosado: true si la cuota de IVA sale como una cifra aparte; false si solo dice «IVA incluido» o no sale.
- total: lo pagado.",
        cats.join("\n")
    );
    let ids: Vec<&str> = CATS.iter().map(|c| c.id).collect();
    let (s, n) = (json!({"type": "string"}), json!({"type": "number"}));
    let esquema = objeto(json!({
        "proveedor": s, "proveedor_nif": s, "proveedor_pais": s, "cliente_nif": s, "numero": s, "fecha": s, "concepto": s,
        "categoria": {"type": "string", "enum": ids}, "moneda": s, "base": n, "iva_tipo": n, "iva_cuota": n, "iva_pais": s,
        "iva_desglosado": {"type": "boolean"}, "total": n,
    }));
    Pregunta { texto, esquema, esfuerzo: "medium" }
}

/// Los datos de un ingreso: una factura emitida o el aviso de pago de un cliente.
pub fn ingreso() -> Pregunta {
    let texto = "Es un ingreso por un trabajo por cuenta propia (una factura emitida o el aviso de pago de un cliente). Saca:
- cliente, cliente_nif y cliente_pais (código de 2 letras: ES, IE, US…): quien paga.
- emisor_nif: el NIF de quien emite la factura; si no es una factura, vacío.
- numero: el número de la factura; si no hay, vacío.
- fecha: la de la factura o, si no hay, la del pago (AAAA-MM-DD).
- concepto: por qué se cobra, en pocas palabras.
- moneda: EUR, USD…
- base: el importe sin IVA ni retención.
- iva_tipo e iva_cuota: el % y la cuota de IVA; sin IVA, 0 y 0.
- retencion_tipo y retencion_cuota: el % y el importe de IRPF retenido; sin retención, 0 y 0.
- total: lo que paga el cliente.
- cobrado: true si el documento dice que ya se ha pagado.";
    let (s, n) = (json!({"type": "string"}), json!({"type": "number"}));
    let esquema = objeto(json!({
        "cliente": s, "cliente_nif": s, "cliente_pais": s, "emisor_nif": s, "numero": s, "fecha": s, "concepto": s, "moneda": s,
        "base": n, "iva_tipo": n, "iva_cuota": n, "retencion_tipo": n, "retencion_cuota": n, "total": n, "cobrado": {"type": "boolean"},
    }));
    Pregunta { texto: texto.into(), esquema, esfuerzo: "medium" }
}

/// Pregunta por el documento y devuelve el JSON contestado y lo que ha costado (en dólares, aproximado).
pub fn preguntar(modo: &Modo, f: &Fichero, p: &Pregunta) -> Result<(Value, f64), String> {
    match modo {
        Modo::Claude => por_claude(f, p),
        Modo::Api(clave) => por_api(clave, f, p),
    }
}

/* ================= con el comando `claude` ================= */

/// Con Claude Code (`claude -p`): el documento, copiado a una carpeta temporal vacía, es lo único que puede leer
/// (solo la herramienta Read, solo ese fichero, sin la configuración ni los MCP de la persona y sin guardar la
/// sesión). Si tarda demasiado, se para.
fn por_claude(f: &Fichero, p: &Pregunta) -> Result<(Value, f64), String> {
    // Una carpeta para cada pregunta, aunque vayan varias a la vez.
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("gesshtor-clasificar-{}-{n}-{}", std::process::id(), f.huella));
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se puede crear una carpeta temporal: {e}"))?;
    let r = claude_en(&dir, f, p);
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn claude_en(dir: &Path, f: &Fichero, p: &Pregunta) -> Result<(Value, f64), String> {
    let ext = if f.tipo == "application/pdf" { "pdf" } else { f.tipo.trim_start_matches("image/") };
    let doc = format!("documento.{ext}");
    std::fs::write(dir.join(&doc), &f.bytes).map_err(|e| format!("No se puede copiar el documento: {e}"))?;
    let salida = std::fs::File::create(dir.join("salida.json")).map_err(|e| e.to_string())?;
    let errores = std::fs::File::create(dir.join("errores.txt")).map_err(|e| e.to_string())?;
    let esquema = p.esquema.to_string();
    let lee = format!("Read(./{doc})");
    let texto = format!("{}\n\nEl documento es ./{doc}: léelo con la herramienta Read.", p.texto);
    let args = [
        "-p", "--model", MODELO_CLAUDE, "--output-format", "json", "--tools", "Read", "--allowedTools", &lee, "--permission-mode", "dontAsk",
        "--restricted", "--strict-mcp-config", "--no-session-persistence", "--effort", p.esfuerzo, "--max-budget-usd", TOPE_DOLARES,
        "--system-prompt", SISTEMA, "--json-schema", &esquema, &texto,
    ];
    let mut hijo = orden(dir, &args, salida, errores).spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "No encuentro el comando «claude» (Claude Code): instálalo o usa PANEL_CLASIFICAR=api.".to_string(),
        _ => format!("No se puede ejecutar «claude»: {e}"),
    })?;
    let desde = Instant::now();
    loop {
        if hijo.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if desde.elapsed() > ESPERA_CLAUDE {
            let _ = hijo.kill();
            let _ = hijo.wait();
            return Err("Claude tarda demasiado: se ha parado.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let bytes = std::fs::read(dir.join("salida.json")).map_err(|e| e.to_string())?;
    let errores = std::fs::read(dir.join("errores.txt")).unwrap_or_default();
    respuesta_claude(&bytes, &errores)
}

/// El comando `claude` en esa carpeta, con su salida y su salida de errores a esos ficheros (a un fichero y no a una
/// tubería: con solo `try_wait`, una tubería llena lo bloquearía). Sin la contraseña del certificado en su entorno.
fn orden(dir: &Path, args: &[&str], salida: std::fs::File, errores: std::fs::File) -> Command {
    let mut o = Command::new("claude");
    o.args(args).current_dir(dir).env_remove(crate::red::certificado::CLAVE_EN_EL_ENTORNO).stdin(Stdio::null()).stdout(salida).stderr(errores);
    o
}

/// Lo que escribe `claude -p --output-format json` (y, aparte, su salida de errores): el resultado, si ha ido bien, y
/// lo que ha costado. Si no contesta, la primera línea de lo que dijo en su salida de errores.
fn respuesta_claude(bytes: &[u8], errores: &[u8]) -> Result<(Value, f64), String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| {
        let txt = String::from_utf8_lossy(bytes);
        let txt = txt.trim();
        let err = String::from_utf8_lossy(errores);
        let err = err.trim();
        match (txt.is_empty(), err.is_empty()) {
            (true, true) => "El comando «claude» no ha contestado (¿has iniciado sesión en Claude Code?).".to_string(),
            (true, false) => format!("El comando «claude» no ha contestado: {}", corto(err)),
            (false, _) => format!("El comando «claude» ha contestado algo raro: {}", corto(txt)),
        }
    })?;
    let coste = v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0);
    let bien = v.get("is_error") == Some(&Value::Bool(false)) && v.get("subtype").and_then(Value::as_str) == Some("success");
    if !bien {
        let que = v.get("result").and_then(Value::as_str).or_else(|| v.get("subtype").and_then(Value::as_str)).unwrap_or("error");
        return Err(format!("Claude no ha podido: {}", corto(que)));
    }
    match v.get("structured_output") {
        Some(o @ Value::Object(_)) => Ok((o.clone(), coste)),
        _ => Err("Claude no ha contestado con los datos pedidos.".into()),
    }
}

/* ================= con la API ================= */

/// Con la API de mensajes de Anthropic (solo a `api.anthropic.com`, ver `red::cliente`): el documento va en base64
/// como `document` (PDF) o `image`, y la respuesta con su esquema (`output_config.format`).
fn por_api(clave: &str, f: &Fichero, p: &Pregunta) -> Result<(Value, f64), String> {
    let datos = crate::dominio::fmt::base64(&f.bytes);
    let fuente = json!({"type": "base64", "media_type": f.tipo, "data": datos});
    let bloque = if f.tipo == "application/pdf" { json!({"type": "document", "source": fuente}) } else { json!({"type": "image", "source": fuente}) };
    let cuerpo = json!({
        "model": MODELO_API,
        "max_tokens": 8000,
        "system": SISTEMA,
        "output_config": {"effort": p.esfuerzo, "format": {"type": "json_schema", "schema": p.esquema}},
        "messages": [{"role": "user", "content": [bloque, {"type": "text", "text": p.texto}]}],
    });
    let bytes = serde_json::to_vec(&cuerpo).map_err(|e| e.to_string())?;
    let c = Cliente::new(None, DOMINIOS_ANTHROPIC).con_cabeceras(vec![("x-api-key", clave.to_string()), ("anthropic-version", VERSION_API.to_string())]);
    let r = c.post(URL_API, Cuerpo::Bytes(&bytes, "application/json"), None)?;
    respuesta_api(r.codigo, &r.cuerpo)
}

/// Lo que contesta la API: el JSON del primer bloque de texto, si ha terminado bien, y su coste aproximado.
fn respuesta_api(codigo: u16, cuerpo: &[u8]) -> Result<(Value, f64), String> {
    let v: Value = serde_json::from_slice(cuerpo).map_err(|_| format!("La API de Claude ha contestado algo raro (código {codigo})."))?;
    if codigo != 200 {
        let msg = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("error");
        return Err(match codigo {
            401 | 403 => "La API de Claude no acepta tu clave (ANTHROPIC_API_KEY).".into(),
            429 | 529 => "La API de Claude está saturada: prueba en un rato.".into(),
            _ => format!("La API de Claude ha dado un error ({codigo}): {}", corto(msg)),
        });
    }
    let uso = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_f64).unwrap_or(0.0);
    let coste = (uso("input_tokens") + uso("cache_creation_input_tokens") + uso("cache_read_input_tokens")) * PRECIO.0 / 1e6 + uso("output_tokens") * PRECIO.1 / 1e6;
    match v.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn") => {}
        Some("refusal") => return Err("Claude no ha querido leer este documento.".into()),
        Some("max_tokens") => return Err("La respuesta de Claude ha salido cortada.".into()),
        otro => return Err(format!("Claude ha parado sin terminar ({}).", otro.unwrap_or("?"))),
    }
    let texto = v.get("content").and_then(Value::as_array).and_then(|c| c.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("text"))).and_then(|b| b.get("text")).and_then(Value::as_str);
    let Some(texto) = texto else { return Err("Claude no ha contestado con los datos pedidos.".into()) };
    match serde_json::from_str::<Value>(texto) {
        Ok(o @ Value::Object(_)) => Ok((o, coste)),
        _ => Err("Claude no ha contestado con los datos pedidos.".into()),
    }
}

/// Un mensaje de error, de una línea y no muy largo.
fn corto(s: &str) -> String {
    let l = s.lines().next().unwrap_or_default().trim();
    if l.chars().count() > 160 { l.chars().take(159).collect::<String>() + "…" } else { l.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;


    /// Los esquemas: todos los campos obligatorios y nada más (lo pide la salida estructurada); las categorías de
    /// gasto, las de la app.
    #[test]
    fn esquemas() {
        for p in [clase(), nomina(), gasto(), ingreso()] {
            let props = p.esquema["properties"].as_object().unwrap();
            assert_eq!(p.esquema["required"].as_array().unwrap().len(), props.len());
            assert_eq!(p.esquema["additionalProperties"], json!(false));
        }
        let g = gasto();
        assert_eq!(g.esquema["properties"]["categoria"]["enum"].as_array().unwrap().len(), CATS.len());
        assert!(g.texto.contains("  vehiculo: Vehículo"));
    }

    /// Lo que devuelve el comando `claude`: bien, con error, vacío o roto.
    #[test]
    fn respuestas_del_comando() {
        let bien = br#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0012,"stop_reason":"tool_use","structured_output":{"tipo":"gasto","que":"un tique"}}"#;
        let (v, c) = respuesta_claude(bien, b"").unwrap();
        assert_eq!((v["tipo"].as_str(), c), (Some("gasto"), 0.0012));
        let mal = br#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"result":"presupuesto agotado"}"#;
        assert!(respuesta_claude(mal, b"").unwrap_err().contains("presupuesto agotado"));
        let sin = br#"{"type":"result","subtype":"success","is_error":false,"result":"no puedo"}"#;
        assert!(respuesta_claude(sin, b"").unwrap_err().contains("datos pedidos"));
        assert!(respuesta_claude(b"", b"").unwrap_err().contains("sesi"));
        assert!(respuesta_claude(b"Error: algo\nmas", b"").unwrap_err().contains("algo raro: Error: algo"));
        // Sin respuesta, lo que dijo en su salida de errores (la primera línea), no «¿has iniciado sesión?».
        let e = respuesta_claude(b"", b"\nerror: unknown option '--restricted'\nUsage: claude [options]").unwrap_err();
        assert!(e.contains("unknown option '--restricted'") && !e.contains("Usage") && !e.contains("sesi"), "{e}");
    }

    /// El comando `claude` no recibe la contraseña del certificado, aunque la app la tuviera en su entorno.
    #[test]
    fn claude_sin_la_clave_del_certificado() {
        let f = std::env::temp_dir().join(format!("gesshtor-orden-{}.txt", std::process::id()));
        let o = orden(&std::env::temp_dir(), &["-p"], std::fs::File::create(&f).unwrap(), std::fs::File::create(&f).unwrap());
        assert!(o.get_envs().any(|(k, v)| k == crate::red::certificado::CLAVE_EN_EL_ENTORNO && v.is_none()));
        let _ = std::fs::remove_file(f);
    }

    /// Lo que devuelve la API: bien, rechazada, cortada, con error o con un texto que no es JSON.
    #[test]
    fn respuestas_de_la_api() {
        let con = |stop: &str, texto: &str| json!({"stop_reason": stop, "usage": {"input_tokens": 1000, "output_tokens": 200}, "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": texto}]}).to_string();
        let (v, c) = respuesta_api(200, con("end_turn", r#"{"tipo":"nomina","que":"x"}"#).as_bytes()).unwrap();
        assert_eq!(v["tipo"], json!("nomina"));
        assert!((c - 0.0002).abs() < 1e-9);
        assert!(respuesta_api(200, con("refusal", "").as_bytes()).unwrap_err().contains("no ha querido"));
        assert!(respuesta_api(200, con("max_tokens", "{\"tipo\":").as_bytes()).unwrap_err().contains("cortada"));
        assert!(respuesta_api(200, con("end_turn", "Es una nómina").as_bytes()).unwrap_err().contains("datos pedidos"));
        assert!(respuesta_api(401, br#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#).unwrap_err().contains("clave"));
        assert!(respuesta_api(400, br#"{"type":"error","error":{"message":"mal"}}"#).unwrap_err().contains("(400): mal"));
        assert!(respuesta_api(502, b"<html>").unwrap_err().contains("502"));
    }
}
