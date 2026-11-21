//! El minijuego de la ventana «Leyendo tus documentos», para la espera: un papelito con patitas corre solo y salta
//! con Espacio los sellos de goma, los montones de papeles y los «%» del IVA que le salen al paso. Al chocar, un
//! «¡ups!» breve y vuelta a empezar; el récord dura mientras la app esté abierta. Los obstáculos salen desde el
//! primer Espacio (quien no juega solo lo ve correr) y, si en una carrera no salta nadie, tras chocar ya no salen.
//!
//! Va con el tiempo, no con los fotogramas: lo andado y la altura del salto salen de fórmulas del tiempo de carrera,
//! así va igual se pinte a 25 fotogramas por segundo o a 10. El «ahora» se le pasa (las pruebas usan uno inventado).
//! Los obstáculos salen a distancias variables, con un generador sencillo y determinista.
use super::widgets::tema;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use std::time::{Duration, Instant};

/// Las filas que ocupa: el marcador va en la de arriba (por la que también se salta) y el suelo, en la de abajo.
pub const ALTO: u16 = 5;
/// La columna del corredor y su ancho, en celdas.
const X: f64 = 3.0;
const ANCHO: f64 = 3.0;
/// Lo que se perdona por delante y por detrás al chocar (en celdas): rozar no cuenta.
const HOLGURA: f64 = 0.5;
/// La velocidad, en celdas por segundo: al empezar, lo que sube cada segundo y la máxima.
const VELOCIDAD: f64 = 22.0;
const ACELERACION: f64 = 0.4;
const VELOCIDAD_MAX: f64 = 44.0;
/// El salto, en medias filas: la velocidad al despegar y la gravedad. Sube 6 medias filas y dura 0,6 s.
const IMPULSO: f64 = 40.0;
const GRAVEDAD: f64 = 400.0 / 3.0;
/// Lo que dura el «¡ups!».
const UPS: Duration = Duration::from_millis(900);
/// Más que esto entre dos `avanzar` es que no se estaba viendo (la ventana, oculta): esa espera no cuenta.
const PARADA: Duration = Duration::from_millis(250);
/// Dónde sale el primer obstáculo y hasta dónde se ponen por delante (más que la ventana más ancha), en celdas.
const PRIMERO: f64 = 50.0;
const DELANTE: f64 = 100.0;
/// Cada cuánto se mira si choca, en segundos: así no atraviesa un obstáculo aunque se pinte poco.
const PASO: f64 = 1.0 / 120.0;

/// El corredor, a medias filas y de arriba abajo: un papel con la esquina doblada y, debajo, las patitas, abiertas
/// y juntas (para que corra).
const CORREDOR: [[&str; 4]; 2] = [["##.", "###", "###", "#.#"], ["##.", "###", "###", ".#."]];

/// Lo que le sale al paso.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cosa {
    /// Un sello de goma.
    Sello,
    /// Un montón de papeles, de una o dos filas.
    Papeles(u8),
    /// El «%» del IVA.
    Iva,
}

impl Cosa {
    /// Cómo se ve, de arriba abajo: una fila por texto, apoyado en el suelo.
    fn dibujo(self) -> &'static [&'static str] {
        match self {
            Cosa::Sello => &[" ▄ ", "▟█▙"],
            Cosa::Papeles(1) => &["≡≡"],
            Cosa::Papeles(_) => &["≡≡", "≡≡"],
            Cosa::Iva => &["%"],
        }
    }

    fn ancho(self) -> f64 {
        self.dibujo()[0].chars().count() as f64
    }

    /// Lo alto, en medias filas: lo que hay que saltar.
    fn alto(self) -> f64 {
        match self {
            Cosa::Sello => 3.0,
            Cosa::Papeles(n) => 2.0 * n as f64,
            Cosa::Iva => 2.0,
        }
    }

    fn estilo(self) -> Style {
        match self {
            Cosa::Papeles(_) => Style::new().fg(tema::MARCO),
            _ => Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD),
        }
    }
}

struct Obstaculo {
    /// Dónde empieza, en lo andado.
    x: f64,
    cosa: Cosa,
    saltado: bool,
}

pub struct Juego {
    /// El estado del generador de números.
    azar: u64,
    /// ¿Salen obstáculos? Desde el primer Espacio.
    jugando: bool,
    /// Si ha saltado en esta carrera: si no (nadie juega), tras chocar ya no salen obstáculos.
    ha_saltado: bool,
    /// El tiempo de carrera, en segundos: de él salen lo andado y la velocidad.
    t: f64,
    /// Cuándo saltó (en tiempo de carrera), mientras está en el aire.
    salto: Option<f64>,
    obstaculos: Vec<Obstaculo>,
    /// Dónde va el siguiente obstáculo, en lo andado.
    siguiente: f64,
    /// La última vez que se avanzó y, si acaba de chocar, cuándo.
    ultimo: Option<Instant>,
    ups: Option<Instant>,
    saltados: u32,
    record: u32,
}

impl Juego {
    pub fn new(semilla: u64) -> Self {
        Juego { azar: semilla, jugando: false, ha_saltado: false, t: 0.0, salto: None, obstaculos: Vec::new(), siguiente: PRIMERO, ultimo: None, ups: None, saltados: 0, record: 0 }
    }

    /// Lo andado, en celdas: la velocidad sube poco a poco hasta la máxima y ahí se queda.
    fn andado(&self) -> f64 {
        let tope = (VELOCIDAD_MAX - VELOCIDAD) / ACELERACION;
        let s = self.t.min(tope);
        VELOCIDAD * s + ACELERACION * s * s / 2.0 + VELOCIDAD_MAX * (self.t - s)
    }

    fn velocidad(&self) -> f64 {
        (VELOCIDAD + ACELERACION * self.t).min(VELOCIDAD_MAX)
    }

    /// La altura de las patitas sobre el suelo, en medias filas (una parábola desde que saltó).
    fn altura(&self) -> f64 {
        self.salto.map_or(0.0, |t0| {
            let s = self.t - t0;
            (IMPULSO * s - GRAVEDAD * s * s / 2.0).max(0.0)
        })
    }

    /// ¿Está saltando?
    #[cfg(test)]
    pub fn en_el_aire(&self) -> bool {
        self.salto.is_some()
    }

    /// Espacio: salta, si está en el suelo. El primero empieza el juego, desde cero: desde ahí salen obstáculos.
    pub fn saltar(&mut self, ahora: Instant) {
        self.avanzar(ahora);
        if self.ups.is_some() || self.salto.is_some() {
            return;
        }
        if !self.jugando {
            self.de_nuevo(true);
        }
        (self.ha_saltado, self.salto) = (true, Some(self.t));
    }

    /// Lo lleva hasta `ahora`. Tras el «¡ups!», vuelta a empezar (con obstáculos si estabas jugando).
    pub fn avanzar(&mut self, ahora: Instant) {
        let antes = self.ultimo.replace(ahora);
        if let Some(t) = self.ups {
            if ahora.saturating_duration_since(t) >= UPS {
                self.de_nuevo(self.ha_saltado);
            }
            return;
        }
        let dt = antes.map_or(Duration::ZERO, |a| ahora.saturating_duration_since(a));
        if dt > PARADA {
            return;
        }
        let mut queda = dt.as_secs_f64();
        while queda > 0.0 {
            let paso = queda.min(PASO);
            queda -= paso;
            self.t += paso;
            if self.salto.is_some_and(|t0| self.t - t0 >= 2.0 * IMPULSO / GRAVEDAD) {
                self.salto = None;
            }
            if self.choca() {
                self.ups = Some(ahora);
                return;
            }
            self.contar();
        }
    }

    /// Vuelve a empezar la carrera (con el récord), sin obstáculos hasta el próximo Espacio: al cerrar la ventana.
    pub fn parar(&mut self) {
        self.de_nuevo(false);
    }

    fn de_nuevo(&mut self, jugando: bool) {
        let (record, ultimo) = (self.record, self.ultimo);
        *self = Juego::new(self.azar);
        (self.record, self.ultimo, self.jugando) = (record, ultimo, jugando);
        self.poner_obstaculos();
    }

    fn choca(&self) -> bool {
        let (a, h) = (self.andado(), self.altura());
        self.obstaculos.iter().any(|o| {
            let d = o.x - a;
            d < ANCHO - HOLGURA && d + o.cosa.ancho() > HOLGURA && h < o.cosa.alto()
        })
    }

    /// Cuenta los que ha dejado atrás, quita los que ya no se ven y pone los que vienen.
    fn contar(&mut self) {
        let a = self.andado();
        for o in &mut self.obstaculos {
            if !o.saltado && o.x + o.cosa.ancho() <= a {
                o.saltado = true;
                self.saltados += 1;
            }
        }
        self.record = self.record.max(self.saltados);
        self.obstaculos.retain(|o| o.x + o.cosa.ancho() > a - X);
        self.poner_obstaculos();
    }

    fn poner_obstaculos(&mut self) {
        while self.jugando && self.siguiente < self.andado() + DELANTE {
            let cosa = match (self.azar() * 4.0) as u8 {
                0 => Cosa::Sello,
                1 => Cosa::Papeles(1),
                2 => Cosa::Papeles(2),
                _ => Cosa::Iva,
            };
            self.obstaculos.push(Obstaculo { x: self.siguiente, cosa, saltado: false });
            // Hasta el siguiente, de 0,9 a 2 segundos de carrera: da tiempo a caer y volver a saltar.
            self.siguiente += cosa.ancho() + self.velocidad() * (0.9 + 1.1 * self.azar());
        }
    }

    /// Un número de 0 a 1 (sin llegar): un generador congruencial lineal de 64 bits, con las constantes de Knuth.
    fn azar(&mut self) -> f64 {
        self.azar = self.azar.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.azar >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Lo pinta en `area` (si tiene menos de `ALTO` filas, nada): arriba a la derecha el marcador, en gris; el suelo
    /// con «▁» en PUNTOS; los obstáculos, apoyados en él; y el corredor en TINTA, a medias filas (▀ ▄ █) para que el
    /// salto se vea suave.
    pub fn dibujar(&self, buf: &mut Buffer, area: Rect) {
        if area.height < ALTO || area.width < 16 {
            return;
        }
        let suelo = area.y + ALTO - 1;
        let mut poner = |x: i32, y: u16, c: char, st: Style| {
            if x >= 0
                && x < area.width as i32
                && let Some(celda) = buf.cell_mut((area.x + x as u16, y))
            {
                celda.set_char(c).set_style(st);
            }
        };
        for x in 0..area.width as i32 {
            poner(x, suelo, '▁', Style::new().fg(tema::PUNTOS));
        }
        let a = self.andado();
        for o in &self.obstaculos {
            let col = (X + o.x - a).round() as i32;
            let dibujo = o.cosa.dibujo();
            for (k, fila) in dibujo.iter().enumerate() {
                let y = suelo + 1 + k as u16 - dibujo.len() as u16;
                for (j, c) in fila.chars().enumerate().filter(|(_, c)| *c != ' ') {
                    poner(col + j as i32, y, c, o.cosa.estilo());
                }
            }
        }
        // El corredor: cada celda junta dos medias filas, contadas desde el suelo. Las patitas, 8 pasos por segundo.
        let figura = &CORREDOR[if self.salto.is_some() { 0 } else { (self.t * 8.0) as usize % 2 }];
        let h = self.altura().round() as i32;
        let lleno = |col: usize, desde_suelo: i32| {
            let k = h + 3 - desde_suelo;
            (0..4).contains(&k) && figura[k as usize].as_bytes()[col] == b'#'
        };
        for col in 0..ANCHO as usize {
            for fila in 0..ALTO {
                let abajo = 2 * (ALTO - 1 - fila) as i32;
                let c = match (lleno(col, abajo + 1), lleno(col, abajo)) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => continue,
                };
                poner(X as i32 + col as i32, area.y + fila, c, Style::new().fg(tema::TINTA));
            }
        }
        if self.ups.is_some() {
            poner_texto(buf, area, X as u16 + 5, "¡ups!", Style::new().fg(tema::AVISO).add_modifier(Modifier::BOLD));
        }
        let saltados = if self.saltados == 1 { "1 saltado".to_string() } else { format!("{} saltados", self.saltados) };
        let marcador = format!("{saltados} · récord {}", self.record);
        let n = marcador.chars().count() as u16;
        if n < area.width {
            poner_texto(buf, area, area.width - n, &marcador, tema::apagado());
        }
    }
}

/// Un texto en la fila de arriba de `area`, desde la columna `x` (contada desde su borde).
fn poner_texto(buf: &mut Buffer, area: Rect, x: u16, texto: &str, st: Style) {
    if x < area.width {
        buf.set_stringn(area.x + x, area.y, texto, (area.width - x) as usize, st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El juego avanzado de `paso` en `paso` ms hasta `hasta` ms desde `t0`, como el bucle de la app.
    fn correr(j: &mut Juego, t0: Instant, desde: u64, hasta: u64, paso: u64) {
        let mut ms = desde;
        while ms < hasta {
            ms = (ms + paso).min(hasta);
            j.avanzar(t0 + Duration::from_millis(ms));
        }
    }

    /// Uno recién empezado con un salto en `t0` (ahí echa a correr).
    fn empezado(semilla: u64, t0: Instant) -> Juego {
        let mut j = Juego::new(semilla);
        j.avanzar(t0);
        j.saltar(t0);
        j
    }

    /// Corre solo desde el principio, sin obstáculos (no choca); con el primer Espacio empieza el juego, desde cero:
    /// salta y salen obstáculos.
    #[test]
    fn corre_solo_y_el_primer_espacio_empieza() {
        let t0 = Instant::now();
        let mut j = Juego::new(1);
        correr(&mut j, t0, 0, 5000, 40);
        assert!(j.andado() > 100.0 && j.obstaculos.is_empty() && j.ups.is_none() && j.altura() == 0.0, "{}", j.andado());
        j.saltar(t0 + Duration::from_millis(5000));
        assert!(j.jugando && j.obstaculos.first().is_some_and(|o| o.x == PRIMERO), "desde cero");
        correr(&mut j, t0, 5000, 5200, 40);
        assert!(j.andado() > 3.0 && j.andado() < 6.0 && j.altura() > 3.0, "{} {}", j.andado(), j.altura());
    }

    /// El salto es una parábola: sube 6 medias filas, cae a los 0,6 s y no se puede saltar en el aire.
    #[test]
    fn el_salto_sube_y_baja() {
        let t0 = Instant::now();
        let mut j = empezado(1, t0);
        let mut maximo: f64 = 0.0;
        for ms in (10..=650).step_by(10) {
            j.avanzar(t0 + Duration::from_millis(ms));
            maximo = maximo.max(j.altura());
            if ms == 400 {
                let antes = j.salto;
                j.saltar(t0 + Duration::from_millis(ms));
                assert_eq!(j.salto, antes, "en el aire no se vuelve a saltar");
            }
        }
        assert!((maximo - 6.0).abs() < 0.05, "{maximo}");
        assert_eq!((j.altura(), j.salto), (0.0, None));
    }

    /// Va igual a cualquier ritmo de pintar: cada 10 ms, cada 40 o cada 200 (como si fuera lento).
    #[test]
    fn va_igual_a_cualquier_ritmo() {
        let t0 = Instant::now();
        let mut v: Vec<Juego> = [10, 40, 200].iter().map(|_| empezado(9, t0)).collect();
        for (j, paso) in v.iter_mut().zip([10, 40, 200]) {
            correr(j, t0, 0, 200, paso);
        }
        assert!(v.iter().all(|j| (j.altura() - v[0].altura()).abs() < 1e-9 && (j.andado() - v[0].andado()).abs() < 1e-9));
        assert!(v[0].altura() > 5.0);
        for (j, paso) in v.iter_mut().zip([10, 40, 200]) {
            correr(j, t0, 200, 1800, paso);
        }
        assert!(v.iter().all(|j| (j.andado() - v[0].andado()).abs() < 1e-9 && j.salto.is_none()));
        // La velocidad sube poco a poco.
        assert!(v[0].velocidad() > VELOCIDAD && v[0].velocidad() < VELOCIDAD + 1.0);
        // Una parada larga (la ventana oculta) no cuenta.
        let antes = v[0].andado();
        v[0].avanzar(t0 + Duration::from_millis(60_000));
        assert_eq!(v[0].andado(), antes);
    }

    /// Con la misma semilla, los mismos obstáculos; a distancias variables.
    #[test]
    fn el_mismo_azar_da_los_mismos_obstaculos() {
        let de = |s: u64| empezado(s, Instant::now()).obstaculos.iter().map(|o| (o.x, o.cosa)).collect::<Vec<_>>();
        assert_eq!(de(5), de(5));
        assert_ne!(de(5), de(6));
        let v = de(5);
        assert!(v.len() >= 2 && v[0].0 == PRIMERO);
        let huecos: Vec<f64> = v.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(huecos.iter().all(|h| *h > 0.9 * VELOCIDAD), "{huecos:?}");
        let mut todos: Vec<f64> = (0..20).flat_map(|s| de(s).windows(2).map(|w| w[1].0 - w[0].0).collect::<Vec<_>>()).collect();
        todos.sort_by(f64::total_cmp);
        assert!(todos[todos.len() - 1] - todos[0] > VELOCIDAD / 2.0, "{todos:?}");
    }

    /// Saltando a tiempo se pasan todos (la física lo permite a cualquier velocidad) y el récord los cuenta.
    #[test]
    fn saltando_a_tiempo_se_pasan_todos() {
        let t0 = Instant::now();
        let mut j = empezado(42, t0);
        for k in 1..=4000u64 {
            let ahora = t0 + Duration::from_millis(20 * k);
            j.avanzar(ahora);
            assert!(j.ups.is_none(), "ha chocado a los {} ms", 20 * k);
            // Salta cuando lo más alto del salto cae en medio del obstáculo.
            if let Some(o) = j.obstaculos.iter().find(|o| !o.saltado) {
                let medio = (ANCHO - o.cosa.ancho()) / 2.0;
                if o.x - j.andado() - medio <= j.velocidad() * IMPULSO / GRAVEDAD {
                    j.saltar(ahora);
                }
            }
        }
        assert!(j.saltados > 40 && j.record == j.saltados, "{}", j.saltados);
        assert_eq!(j.velocidad(), VELOCIDAD_MAX);
    }

    /// Al chocar: «¡ups!», quieto un momento y vuelta a empezar, con el récord. Si en una carrera no salta nadie, al
    /// chocar se queda quieto, esperando.
    #[test]
    fn choca_dice_ups_y_vuelve_a_empezar() {
        let t0 = Instant::now();
        let mut j = empezado(3, t0);
        j.record = 7;
        let mut ms = 0;
        while j.ups.is_none() && ms < 10_000 {
            ms += 40;
            j.avanzar(t0 + Duration::from_millis(ms));
        }
        assert!(j.ups.is_some() && ms > 1000, "{ms}");
        let area = Rect::new(0, 0, 40, ALTO);
        let mut buf = Buffer::empty(area);
        j.dibujar(&mut buf, area);
        assert!(fila(&buf, 0).contains("¡ups!"), "{}", fila(&buf, 0));
        let quieto = j.andado();
        correr(&mut j, t0, ms, ms + 400, 40);
        assert_eq!(j.andado(), quieto, "durante el «¡ups!», quieto");
        correr(&mut j, t0, ms + 400, ms + 920, 40);
        assert!(j.ups.is_none() && j.jugando && !j.obstaculos.is_empty() && j.andado() == 0.0 && j.saltados == 0 && j.record == 7);
        // Sin tocar Espacio: choca otra vez y, de nuevo, corre sin obstáculos.
        let mut ms = ms + 920;
        while j.ups.is_none() && ms < 20_000 {
            ms += 40;
            j.avanzar(t0 + Duration::from_millis(ms));
        }
        correr(&mut j, t0, ms, ms + 1000, 40);
        assert!(!j.jugando && j.obstaculos.is_empty() && j.ups.is_none() && j.andado() > 0.0 && j.record == 7);
    }

    fn fila(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect()
    }

    /// Se pinta en cinco filas: el marcador arriba a la derecha, el suelo, los obstáculos y el corredor a medias filas.
    #[test]
    fn se_pinta_a_medias_filas() {
        let t0 = Instant::now();
        let area = Rect::new(0, 0, 60, ALTO);
        let mut j = Juego::new(1);
        let mut buf = Buffer::empty(area);
        j.dibujar(&mut buf, area);
        assert!(fila(&buf, 0).ends_with("0 saltados · récord 0"), "{}", fila(&buf, 0));
        assert!(fila(&buf, 3).starts_with("   ██▄ "), "{}", fila(&buf, 3));
        assert_eq!(fila(&buf, 4), format!("▁▁▁█▀█{}", "▁".repeat(54)), "sin obstáculos hasta el primer Espacio");
        // Media fila más arriba: los medios bloques; y el primer obstáculo, a su distancia.
        j.avanzar(t0);
        j.saltar(t0);
        j.avanzar(t0 + Duration::from_millis(26));
        assert_eq!(j.altura().round(), 1.0);
        let mut buf = Buffer::empty(area);
        j.dibujar(&mut buf, area);
        let (f2, f3, f4) = (fila(&buf, 2), fila(&buf, 3), fila(&buf, 4));
        assert!(f2.starts_with("   ▄▄ ") && f3.starts_with("   ███") && f4.starts_with("▁▁▁▀▁▀"), "{f2}\n{f3}\n{f4}");
        assert!(f4.chars().skip(X as usize + PRIMERO as usize - 1).any(|c| c != '▁'), "{f4}");
        // Demasiado pequeño: nada.
        let poco = Rect::new(0, 0, 60, ALTO - 1);
        let mut buf = Buffer::empty(poco);
        j.dibujar(&mut buf, poco);
        assert!((0..poco.height).all(|y| fila(&buf, y).trim().is_empty()));
    }
}
