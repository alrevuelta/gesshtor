//! Para las pruebas: pintar la app en un terminal falso y mandarle teclas, sin tocar el disco. Los datos son los
//! de `pruebas/<caso>` (personas inventadas: `datos.json` y, como no tienen PDF, `lecturas.json`).
//!
//! ```ignore
//! let mut app = prueba::app();               // con el caso «a-de-todo» (prueba::app_de("c-sin-gdj") para otro)
//! prueba::ir(&mut app, "Facturas");          // a una pantalla, por su título
//! prueba::teclas(&mut app, "n<tab>Hola<c-s>");
//! println!("{}", prueba::pantalla(&mut app, 140, 40));
//! ```
use super::{App, Ctx};
use crate::dominio::almacen;
use crate::dominio::Datos;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// La carpeta de un caso de `pruebas/`.
pub fn caso(nombre: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pruebas").join(nombre)
}

/// Los datos de una carpeta, con lo leído de sus PDF (o, si no tiene, lo de `lecturas.json`).
pub fn cargar(dir: &Path) -> Datos {
    let mut d = almacen::cargar(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    crate::hacienda::lectura::renombrar_antiguos(dir);
    d.lecturas = match std::fs::read_to_string(dir.join("lecturas.json")) {
        Ok(t) => serde_json::from_str(&t).unwrap(),
        Err(_) => crate::hacienda::lectura::leer_todo(dir, &mut |_, _| {}),
    };
    d
}

/// La app con unos datos, sin guardar en disco ni abrir ficheros.
pub fn app_con(datos: Datos, dir: &Path) -> App {
    let mut ctx = Ctx::new(datos, dir.to_path_buf());
    ctx.sin_disco = true;
    App::new(ctx)
}

/// La app con el caso «a-de-todo» (autónoma con facturas, gastos y clientes).
pub fn app() -> App {
    app_de("a-de-todo")
}

pub fn app_de(nombre: &str) -> App {
    let dir = caso(nombre);
    app_con(cargar(&dir), &dir)
}

/// Va a la pantalla con ese título («Facturas», «Años», «Sueldo»…), esté en la sección que esté.
pub fn ir(app: &mut App, titulo: &str) {
    assert!(app.ir_a(titulo), "no hay ninguna pantalla «{titulo}»");
}

/// Los títulos de todas las pantallas, en orden.
pub fn titulos(app: &App) -> Vec<&'static str> {
    app.secciones.iter().flat_map(|s| s.pantallas.iter().map(|p| p.titulo())).collect()
}

/// Pinta la app en un terminal de `w` × `h` y devuelve el texto de la pantalla.
pub fn pantalla(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| app.dibujar(f)).unwrap();
    let b = t.backend().buffer();
    let mut s = String::new();
    for y in 0..h {
        let mut fila = String::new();
        for x in 0..w {
            fila.push_str(b[(x, y)].symbol());
        }
        s.push_str(fila.trim_end());
        s.push('\n');
    }
    s
}

/// Lo que se ve sin el menú de la izquierda (en pantallas anchas ocupa las primeras columnas).
pub fn sin_menu(s: &str) -> String {
    s.lines().map(|l| l.chars().skip(super::MENU as usize).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// Manda teclas a la app. Cada carácter es una tecla, salvo los nombres entre < >:
/// <enter> <esc> <tab> <s-tab> <up> <down> <left> <right> <bs> <del> <home> <end> <pgup> <pgdn> <space> <c-x> (Ctrl+x).
pub fn teclas(app: &mut App, s: &str) {
    for k in parsear(s) {
        app.tecla(k);
    }
}

pub fn parsear(s: &str) -> Vec<KeyEvent> {
    let mut v = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '<' {
            let nombre: String = it.clone().take_while(|&x| x != '>').collect();
            if it.clone().any(|x| x == '>') && !nombre.is_empty() && nombre.len() <= 6 {
                for _ in 0..=nombre.chars().count() {
                    it.next();
                }
                let k = match nombre.as_str() {
                    "enter" => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                    "esc" => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                    "tab" => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                    "s-tab" => KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                    "up" => KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                    "down" => KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                    "left" => KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
                    "right" => KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                    "bs" => KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                    "del" => KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
                    "home" => KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
                    "end" => KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
                    "pgup" => KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
                    "pgdn" => KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
                    "space" => KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                    n if n.starts_with("c-") && n.chars().count() == 3 => KeyEvent::new(KeyCode::Char(n.chars().nth(2).unwrap()), KeyModifiers::CONTROL),
                    n => panic!("tecla desconocida <{n}>"),
                };
                v.push(k);
                continue;
            }
        }
        v.push(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    v
}

/// Todas las pantallas, en todos los años, en tres tamaños y tras
/// unas teclas de moverse (nunca las que guardan, borran, abren PDF o sincronizan): «pantalla · año · cuál» → lo
/// que se ve (sin el menú de la izquierda, para que un cambio en él no cambie todas).
pub fn recorrer(dir: &Path) -> BTreeMap<String, String> {
    let datos = cargar(dir);
    let nueva = || app_con(datos.clone(), dir);
    let todas = titulos(&nueva());
    let sin_izquierda = |s: String, w: u16| if w >= super::ANCHO_MENU { sin_menu(&s) } else { s };
    let pasos: &[(&str, &str)] = &[("abajo y Enter", "<down><down><enter>"), ("Esc", "<esc>"), ("fin y Enter", "<end><enter>"), ("Esc e inicio", "<esc><home>")];
    let mut out = BTreeMap::new();
    for t in &todas {
        for y in crate::dominio::PRIMER_ANIO..=crate::dominio::fmt::anio_actual() {
            let mut a = nueva();
            a.ctx.anio = y;
            ir(&mut a, t);
            for (w, h) in [(140, 40), (100, 30), (80, 24)] {
                out.insert(format!("{t} · {y} · {w}x{h}"), sin_izquierda(pantalla(&mut a, w, h), w));
            }
            let mut extra: Vec<(&str, &str)> = pasos.to_vec();
            match *t {
                "Años" => extra.extend([("g", "g"), ("g otra vez", "g"), ("g y otra", "g"), ("+ desde el detalle", "<enter>+"), ("- desde el detalle", "-"), ("Esc y el anterior", "<esc>-<enter>"), ("Esc tras el anterior", "<esc>")]),
                "Impuestos" => extra.extend([("← el 4T", "<left>"), ("← el 3T", "<left>"), ("→ → al año", "<right><right>")]),
                _ => {}
            }
            for (i, (nombre, k)) in extra.iter().enumerate() {
                teclas(&mut a, k);
                out.insert(format!("{t} · {y} · paso {i} {nombre}"), sin_izquierda(pantalla(&mut a, 120, 36), 120));
            }
        }
    }
    out
}

mod tests {
    use super::*;

    #[test]
    fn teclas_especiales() {
        let v = parsear("a<enter><c-s>b");
        assert_eq!(v.len(), 4);
        assert_eq!(v[1].code, KeyCode::Enter);
        assert!(v[2].modifiers.contains(KeyModifiers::CONTROL));
    }

    /// Todas las pantallas de los casos de `pruebas/`, en todos los años y tamaños: ninguna se rompe.
    #[test]
    fn todas_las_pantallas_de_los_casos() {
        for c in ["a-de-todo", "b-hacienda", "c-sin-gdj"] {
            let v = recorrer(&caso(c));
            assert!(v.len() > 500, "{c}: {}", v.len());
            assert!(v.values().all(|p| !p.trim().is_empty()), "{c}");
        }
    }

    /// En «Años», el último es el que está en curso: Enter lo abre, con lo que saldría en su Renta; `-` y `+`
    /// cambian de año sin cerrarlo.
    #[test]
    fn el_anio_en_curso_esta_en_anios() {
        let hoy = crate::dominio::fmt::anio_actual();
        let mut a = app();
        ir(&mut a, "Años");
        assert!(pantalla(&mut a, 120, 36).lines().any(|l| l.contains(&format!("{hoy}*")) && l.contains("en curso")), "la fila del año en curso");
        teclas(&mut a, "<enter>");
        assert_eq!((a.titulo_activa(), a.ctx.anio), ("Años", hoy), "Enter abre el año en curso");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(&format!("{hoy} · en curso")) && p.contains(&format!("Tu Renta de {hoy}")), "{p}");
        teclas(&mut a, "-");
        assert_eq!(a.ctx.anio, hoy - 1);
        assert!(pantalla(&mut a, 120, 36).contains(&format!("{} · sin Renta", hoy - 1)), "sigue abierto, en el anterior");
        teclas(&mut a, "+");
        assert!(pantalla(&mut a, 120, 36).contains(&format!("{hoy} · en curso")), "y vuelve");
    }

    /// Cada entrada del menú lleva su número al lado, y ese número va allí; escribiendo, es un número.
    #[test]
    fn un_numero_por_entrada() {
        // Una autónoma sin nómina: Resumen (1–3) y Autónomo (4–8), sin Empleo.
        let mut a = app();
        teclas(&mut a, "4");
        assert_eq!(a.titulo_activa(), "Facturas");
        let p = pantalla(&mut a, 120, 36);
        assert!(p.contains(" › 4 Facturas") && p.contains("   7 Impuestos"), "{p}");
        teclas(&mut a, "7");
        assert_eq!(a.titulo_activa(), "Impuestos");
        teclas(&mut a, "9");
        assert_eq!(a.titulo_activa(), "Impuestos", "no hay novena");
        teclas(&mut a, "2");
        assert_eq!(a.titulo_activa(), "Años");
        // Con el menú arriba: cada sección con el número de su primera entrada, y sus pestañas con el suyo.
        let p = pantalla(&mut a, 80, 24);
        assert!(p.contains("4 Autónomo") && p.contains("2 Años"), "{p}");
        // Buscando, los números se escriben.
        teclas(&mut a, "4/1");
        assert_eq!(a.titulo_activa(), "Facturas");
        assert!(a.pantalla().capturando());
        // Caben todas: con las tres secciones son 10 (1–9 y 0).
        assert!(titulos(&a).len() <= 10, "{:?}", titulos(&a));
    }

    /// El año va arriba, en el centro (en grande si hay sitio), salvo en lo que no depende de él: allí `-` no hace nada.
    #[test]
    fn el_anio_arriba_salvo_en_inicio() {
        let hoy = crate::dominio::fmt::anio_actual();
        let mut a = app();
        ir(&mut a, "Facturas");
        assert!(pantalla(&mut a, 120, 24).lines().next().unwrap().trim_end().ends_with(&format!(" {hoy}")));
        assert!(pantalla(&mut a, 120, 36).contains("▀▀▀ ▀▀▀ ▀▀▀ ▀▀▀"), "en grande");
        ir(&mut a, "Inicio");
        assert!(!pantalla(&mut a, 120, 36).contains("▀▀▀ ▀▀▀ ▀▀▀ ▀▀▀"));
        assert!(!pantalla(&mut a, 120, 24).lines().next().unwrap().contains(&hoy.to_string()));
        teclas(&mut a, "-");
        assert_eq!(a.ctx.anio, hoy, "en Inicio, - no cambia el año");
    }

    /// Ver una pantalla: `PANTALLA=Sueldo TECLAS="<down>" W=120 H=36 DATOS=mock/ana cargo test ver_pantalla --
    /// --ignored --nocapture` (sin `DATOS`, el caso «a-de-todo»).
    #[test]
    #[ignore]
    fn ver_pantalla() {
        let var = |k: &str, defecto: &str| std::env::var(k).unwrap_or_else(|_| defecto.into());
        let dir = std::env::var("DATOS").map(PathBuf::from).unwrap_or_else(|_| caso("a-de-todo"));
        let mut a = app_con(cargar(&dir), &dir);
        ir(&mut a, &var("PANTALLA", "Inicio"));
        teclas(&mut a, &var("TECLAS", ""));
        println!("{}", pantalla(&mut a, var("W", "120").parse().unwrap(), var("H", "36").parse().unwrap()));
    }

    /// Instantáneas, para comparar antes y después de un cambio que no debería cambiar lo que se ve:
    /// `DATOS=mock/ana SALIDA=/tmp/antes.txt cargo test instantaneas -- --ignored`
    #[test]
    #[ignore]
    fn instantaneas() {
        let (Ok(dir), Ok(salida)) = (std::env::var("DATOS"), std::env::var("SALIDA")) else { panic!("DATOS=carpeta SALIDA=fichero") };
        let texto: String = recorrer(Path::new(&dir)).iter().map(|(k, v)| format!("### {k}\n{v}\n")).collect();
        std::fs::write(&salida, texto).unwrap();
    }
}
