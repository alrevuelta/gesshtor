//! Tus inversiones, para Hacienda (funciones puras: datos → cifras):
//!
//! 1. **Lo declarado** (`declarado`): de cada Renta leída, lo del ahorro: los rendimientos del capital mobiliario
//!    (intereses y dividendos), las ganancias y pérdidas (las de criptomonedas aparte), la base del ahorro y lo que
//!    pagaste por ella.
//! 2. **Un año con lo traído de tus brókeres** (`anio`, solo con el formato común de `dominio::inversiones`): lo
//!    que has ganado o perdido con lo que has vendido, tus dividendos e intereses con sus retenciones y la cuota
//!    que te tocaría con la escala del ahorro de ese año. Las reglas (Ley 35/2006 del IRPF; `hacienda::ahorro`):
//!    - cada venta, con lo que costaron los primeros que compraste del mismo valor (art. 37.2: «se considerará que
//!      los transmitidos por el contribuyente son aquellos que adquirió en primer lugar»), de todos tus brókeres;
//!    - las comisiones, en lo que te costó y en lo que te dieron (art. 35.1.b y 35.2);
//!    - en euros, al cambio de cada operación (el que aplicó el bróker, no el oficial del BCE). Así lo dice la
//!      Dirección General de Tributos para los fondos (consulta V0775-21: la compra, al cambio de su fecha, y la
//!      venta, al de la suya); para las acciones, en cambio, calcula la diferencia en la divisa y la pasa a euros
//!      al cambio del día de la venta (V1740-17, V0775-21, V0282-22). La app hace lo primero en todo: para
//!      acciones en otra divisa, la pantalla lo avisa;
//!    - una pérdida no cuenta si recompras valores homogéneos dentro de los dos meses anteriores o posteriores
//!      (art. 33.5.f; un año si no cotizan, 33.5.g), en la parte de lo recomprado que sigues teniendo; cuenta
//!      cuando vendes lo recomprado (el Manual de Renta 2025: se imputa «a medida que se vayan transmitiendo, de
//!      forma definitiva, los valores que se consideran recomprados»). Los traspasos entre fondos (art. 94) no
//!      se tratan: un bróker como DEGIRO no los hace;
//!    - los dividendos e intereses, en bruto (art. 25.1 y 25.2), con lo retenido en España como pago a cuenta y lo
//!      retenido fuera para la deducción por doble imposición (art. 80: la menor de lo pagado fuera y el tipo medio
//!      del ahorro por esa renta; lo pagado fuera, como mucho lo que permite el convenio, DGT V0080-20, que la app
//!      supone del 15 %, `ahorro::TOPE_CONVENIO`);
//!    - y los dos saldos, integrados y compensados (art. 49).
//! 3. **Lo que tenías fuera** (`fuera`): por año, para los modelos 720 y 721 (`hacienda::modelo_720`).
use crate::dominio::fmt::{self, r2, DIF};
use crate::dominio::inversiones::{Cartera, Clase, Cobro, Operacion, Posicion, Producto};
use crate::dominio::renta::{Capital, Ganancias, Renta};
use crate::dominio::Datos;
use crate::hacienda::modelo_720::{self, Bloque, Toca};
use crate::hacienda::{ahorro, Decl};
use chrono::Months;
use std::collections::BTreeMap;

/* ================= 1. lo declarado ================= */

/// Casilla de la Renta (numeración de 2018 en adelante) con la deducción por doble imposición internacional
/// «por razón de las rentas obtenidas y gravadas en el extranjero».
pub const CASILLA_DOBLE_IMPOSICION: u32 = 588;

/// Lo del ahorro de una Renta.
#[derive(Debug, Clone, Copy)]
pub struct Declarado<'a> {
    pub anio: i32,
    pub renta: &'a Renta,
}

impl<'a> Declarado<'a> {
    /// Intereses, dividendos y otros rendimientos del capital mobiliario.
    pub fn capital(&self) -> Option<&'a Capital> {
        self.renta.capital.as_ref()
    }
    /// Ganancias y pérdidas de lo que vendiste (con las de criptomonedas).
    pub fn ganancias(&self) -> Option<&'a Ganancias> {
        self.renta.ganancias.as_ref()
    }
    /// La base liquidable del ahorro.
    pub fn base(&self) -> Option<f64> {
        self.renta.cuotas.as_ref().map(|c| c.liquidable_ahorro).or(self.renta.bases.as_ref().map(|b| b.ahorro))
    }
    /// Lo que pagaste por ella: la cuota íntegra del ahorro (estatal y autonómica).
    pub fn cuota(&self) -> Option<f64> {
        self.renta.cuotas.as_ref().map(|c| c.ahorro)
    }
    /// Lo que ya te habían retenido de tus intereses y dividendos.
    pub fn retenido(&self) -> Option<f64> {
        self.renta.pagos.as_ref().map(|p| p.capital)
    }
    /// La deducción por doble imposición internacional, si la hubo.
    pub fn doble_imposicion(&self) -> Option<f64> {
        self.renta.casilla(CASILLA_DOBLE_IMPOSICION).filter(|x| *x > DIF)
    }
    /// ¿Hay algo del ahorro en esa Renta?
    pub fn hay_algo(&self) -> bool {
        let c = self.capital().is_some_and(|c| c.integros.abs() > DIF);
        let g = self.ganancias().is_some_and(|g| g.ganancias > DIF || g.perdidas > DIF);
        c || g || self.base().is_some_and(|b| b > DIF)
    }
}

/// Lo del ahorro de cada Renta leída, de la más nueva a la más vieja.
pub fn declarado(d: &Datos) -> Vec<Declarado<'_>> {
    d.lecturas.rentas.iter().rev().map(|(anio, renta)| Declarado { anio: *anio, renta }).collect()
}

/* ================= 2. un año con lo traído ================= */

/// Una venta, con lo que costó.
#[derive(Debug, Clone)]
pub struct Venta<'a> {
    pub op: &'a Operacion,
    /// Lo que te dieron, menos las comisiones (en euros).
    pub transmision: f64,
    /// Lo que te costaron los vendidos, con las comisiones de su compra (en euros).
    pub adquisicion: f64,
    /// De qué compras salen: (fecha, cantidad, lo que costaron).
    pub lotes: Vec<(&'a str, f64, f64)>,
    /// Cuántos vendiste sin que conste la compra (llegaron de otro bróker, un split…): sin eso no se sabe el
    /// resultado, y la venta no entra en las cuentas.
    pub sin_coste: f64,
    /// transmisión − adquisición.
    pub resultado: f64,
    /// La parte de la pérdida que no cuenta por haber recomprado (positiva).
    pub no_computa: f64,
    /// Cuándo recompraste (la primera recompra que la bloquea).
    pub recompra: Option<&'a str>,
    /// Pérdidas de antes que no contaron por una recompra y cuentan ahora, al vender lo recomprado (negativa).
    pub integradas: f64,
    /// Es una pérdida y aún no han pasado los meses en los que una recompra la haría no contar.
    pub provisional: bool,
}

impl Venta<'_> {
    pub fn completa(&self) -> bool {
        self.sin_coste < DIF
    }
    /// Lo que cuenta en la Renta.
    pub fn computa(&self) -> f64 {
        r2(self.resultado + self.no_computa + self.integradas)
    }
}

/// Lo que te tocaría pagar por un año, con la escala del ahorro.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cuenta {
    /// Rendimientos del capital mobiliario: dividendos, intereses y lo de vender bonos.
    pub capital: f64,
    /// Ganancias menos pérdidas que cuentan.
    pub ganancias: f64,
    /// Lo compensado de un saldo negativo con el otro (como mucho el 25 % de este).
    pub compensado: f64,
    /// Lo negativo que queda para los cuatro años siguientes.
    pub para_otros_anios: f64,
    /// La base liquidable del ahorro (sin otras rentas ni pérdidas de años anteriores).
    pub base: f64,
    /// El año de la escala que se usa.
    pub escala: i32,
    pub estatal: f64,
    pub autonomica: f64,
    pub cuota: f64,
    pub tipo_medio: f64,
    /// La deducción por doble imposición internacional.
    pub doble_imposicion: f64,
    /// Lo ya retenido en España (pagos a cuenta).
    pub retenido_espana: f64,
    /// Lo que quedaría por pagar en tu Renta por esto (negativo: te devolverían).
    pub a_pagar: f64,
}

/// Un año, con lo traído de tus brókeres.
#[derive(Debug, Clone)]
pub struct Anio<'a> {
    pub anio: i32,
    /// Los brókeres, y hasta qué día llega lo de cada uno.
    pub brokers: Vec<(&'a str, &'a str)>,
    pub ventas: Vec<Venta<'a>>,
    /// Los dividendos e intereses del año (también los que no se sabe pasar a euros: `sin_cambio`).
    pub cobros: Vec<&'a Cobro>,
    /// En euros, brutos.
    pub dividendos: f64,
    pub intereses: f64,
    pub retenido_origen: f64,
    pub retenido_espana: f64,
    /// Ganancias y pérdidas que cuentan (las pérdidas, en positivo), de las ventas completas que no son de bonos.
    pub ganancias: f64,
    pub perdidas: f64,
    /// Lo de vender bonos (va con el capital mobiliario, art. 25.2).
    pub bonos: f64,
    /// Pérdidas que aún no cuentan por haber recomprado (cuentan al vender lo recomprado).
    pub pendientes: f64,
    pub cuenta: Cuenta,
    /// Lo que hay que mirar: ventas sin compra, cobros sin cambio, lo que el bróker no sabe traducir.
    pub avisos: Vec<String>,
}

/// Las carteras traídas (de todos los brókeres), con su nombre.
fn carteras(d: &Datos) -> Vec<&Cartera> {
    d.carteras.values().filter_map(|f| f.dato.as_ref()).collect()
}

/// ¿Hay algo traído de algún bróker?
pub fn hay_cartera(d: &Datos) -> bool {
    !carteras(d).is_empty()
}

/// Un año, con lo traído de tus brókeres (None si no hay nada traído). `hoy`, para saber qué pérdidas aún pueden
/// dejar de contar por una recompra.
pub fn anio<'a>(d: &'a Datos, anio: i32, hoy: &str) -> Option<Anio<'a>> {
    let cs = carteras(d);
    if cs.is_empty() {
        return None;
    }
    let ops: Vec<&Operacion> = cs.iter().flat_map(|c| &c.operaciones).collect();
    // Desde cuándo no se sabe el coste de cada valor (lo primero de cualquier bróker).
    let mut sin_coste_desde: BTreeMap<&str, &str> = BTreeMap::new();
    for (isin, f) in cs.iter().flat_map(|c| &c.sin_coste_desde) {
        let desde = sin_coste_desde.entry(isin).or_insert(f);
        *desde = (*desde).min(f.as_str());
    }
    let todas = ventas(&ops, &sin_coste_desde, hoy);
    let del_anio = |f: &str| fmt::anio_de(f) == anio;
    let mut a = Anio {
        anio,
        brokers: cs.iter().map(|c| (c.broker.as_str(), c.hasta.as_str())).collect(),
        ventas: Vec::new(),
        cobros: cs.iter().flat_map(|c| &c.cobros).filter(|c| del_anio(&c.fecha)).collect(),
        dividendos: 0.0,
        intereses: 0.0,
        retenido_origen: 0.0,
        retenido_espana: 0.0,
        ganancias: 0.0,
        perdidas: 0.0,
        bonos: 0.0,
        pendientes: 0.0,
        cuenta: Cuenta::default(),
        avisos: cs.iter().flat_map(|c| c.avisos.iter().cloned()).collect(),
    };
    // Las pérdidas que no contaron, menos las que ya han contado, hasta fin de año.
    let hasta_fin = |v: &&Venta| fmt::anio_de(&v.op.fecha) <= anio;
    a.pendientes = r2(todas.iter().filter(hasta_fin).map(|v| v.no_computa + v.integradas).sum::<f64>()).max(0.0);
    a.ventas = todas.into_iter().filter(|v| del_anio(&v.op.fecha)).collect();
    a.cobros.sort_by(|x, y| x.fecha.cmp(&y.fecha));
    for v in &a.ventas {
        // De una venta sin todo su coste, solo cuenta la pérdida de antes que libera (la de lo recomprado que vendes).
        let x = if v.completa() { v.computa() } else { v.integradas };
        if !v.completa() {
            let libera = if v.integradas < -DIF { format!(" (sí cuentan los {} de una pérdida de antes, por vender lo recomprado)", fmt::eur(-v.integradas)) } else { String::new() };
            a.avisos.push(format!(
                "Vendiste {} de {} el {} sin que conste lo que te costaron {} de ellos (¿llegaron de otro bróker, o hubo un split?): esa venta no entra en las cuentas{libera}.",
                fmt::num(v.op.cantidad), v.op.nombre, fmt::dmy(&v.op.fecha), fmt::num(v.sin_coste)
            ));
        }
        if v.op.producto == Producto::Bono {
            a.bonos += x;
        } else if x > 0.0 {
            a.ganancias += x;
        } else {
            a.perdidas -= x;
        }
    }
    let mut doble = 0.0;
    let mut sin_cambio = Vec::new();
    for c in &a.cobros {
        let (Some(bruto), Some(origen), Some(espana)) = (c.en_euros(c.bruto), c.en_euros(c.retenido_origen), c.en_euros(c.retenido_espana)) else {
            sin_cambio.push(*c);
            continue;
        };
        match c.clase {
            Clase::Dividendo => a.dividendos += bruto,
            Clase::Interes => a.intereses += bruto,
        }
        a.retenido_origen += origen;
        a.retenido_espana += espana;
        // Para la deducción, cada cobro con lo que permite el convenio (el tipo medio se aplica después).
        doble += origen.min(ahorro::TOPE_CONVENIO * bruto);
    }
    for c in sin_cambio {
        a.avisos.push(format!("No se sabe a cuánto estaba el {} el {}: {} de {} no entra en las cuentas.", c.divisa, fmt::dmy(&c.fecha), fmt::num(c.bruto), c.nombre));
    }
    [&mut a.dividendos, &mut a.intereses, &mut a.retenido_origen, &mut a.retenido_espana, &mut a.ganancias, &mut a.perdidas, &mut a.bonos].into_iter().for_each(|x| *x = r2(*x));
    // El límite de la deducción: el tipo medio sobre lo cobrado fuera (con lo retenido fuera).
    let fuera: f64 = a.cobros.iter().filter(|c| c.retenido_origen > DIF).filter_map(|c| c.en_euros(c.bruto)).sum();
    a.cuenta = cuenta(anio, r2(a.dividendos + a.intereses + a.bonos), r2(a.ganancias - a.perdidas), r2(doble), r2(fuera), a.retenido_espana);
    Some(a)
}

/// La cuota de un año: integra y compensa los dos saldos (art. 49), la escala del ahorro (arts. 66 y 76) y la
/// deducción por doble imposición (art. 80: la menor de lo pagado fuera, `doble`, y el tipo medio por lo cobrado
/// fuera, `fuera`).
pub fn cuenta(anio: i32, capital: f64, ganancias: f64, doble: f64, fuera: f64, retenido_espana: f64) -> Cuenta {
    let (base, compensado, para_otros_anios) = if capital >= 0.0 && ganancias < 0.0 {
        let c = (-ganancias).min(ahorro::COMPENSACION * capital);
        (capital - c, c, -ganancias - c)
    } else if ganancias >= 0.0 && capital < 0.0 {
        let c = (-capital).min(ahorro::COMPENSACION * ganancias);
        (ganancias - c, c, -capital - c)
    } else {
        (capital.max(0.0) + ganancias.max(0.0), 0.0, -(capital.min(0.0) + ganancias.min(0.0)))
    };
    let base = r2(base);
    let (escala, _) = ahorro::escala(anio);
    let (estatal, autonomica) = ahorro::cuota(anio, base);
    let cuota = r2(estatal + autonomica);
    let tipo_medio = if base > DIF { cuota / base } else { 0.0 };
    let doble_imposicion = r2(doble.min(tipo_medio * fuera).min(cuota));
    Cuenta {
        capital,
        ganancias,
        compensado: r2(compensado),
        para_otros_anios: r2(para_otros_anios),
        base,
        escala,
        estatal,
        autonomica,
        cuota,
        tipo_medio,
        doble_imposicion,
        retenido_espana,
        a_pagar: r2(cuota - doble_imposicion - retenido_espana),
    }
}

/// Lo que se sabe de una compra mientras se va vendiendo.
struct Lote<'a> {
    op: &'a Operacion,
    /// Los que quedan (0 hasta que se compra).
    quedan: f64,
    /// Lo que costó cada uno, con su parte de las comisiones.
    coste: f64,
    /// Los que aún pueden hacer que una pérdida no cuente (cada uno, una vez).
    libres: f64,
    /// Las pérdidas que no contaron por esta compra: cuentan al venderla.
    pendiente: f64,
}

/// Todas las ventas, en orden, con su coste (FIFO por valor, de todos los brókeres) y la regla de la recompra. Las
/// de un valor desde el día en que su coste deja de saberse (`sin_coste_desde`: un split, un traspaso…), sin coste:
/// no entran en las cuentas ni gastan las compras de antes.
pub fn ventas<'a>(ops: &[&'a Operacion], sin_coste_desde: &BTreeMap<&str, &str>, hoy: &str) -> Vec<Venta<'a>> {
    let mut por_valor: BTreeMap<&str, Vec<&Operacion>> = BTreeMap::new();
    for o in ops {
        por_valor.entry(o.isin.as_str()).or_default().push(o);
    }
    let mut out = Vec::new();
    for (isin, mut v) in por_valor {
        v.sort_by(|a, b| a.fecha.cmp(&b.fecha));
        let desde = sin_coste_desde.get(isin).copied();
        let mut lotes: Vec<Lote> = v.iter().filter(|o| o.compra && o.cantidad > 0.0).map(|o| Lote { op: o, quedan: 0.0, coste: (o.euros + o.comisiones) / o.cantidad, libres: o.cantidad, pendiente: 0.0 }).collect();
        let mut compradas = 0;
        for o in v {
            if o.compra {
                if let Some(l) = lotes.get_mut(compradas).filter(|l| std::ptr::eq(l.op, o)) {
                    l.quedan = o.cantidad;
                    compradas += 1;
                }
                continue;
            }
            if desde.is_some_and(|d| o.fecha.as_str() >= d) {
                // Como una venta sin compras.
                out.push(vender(o, &mut [], 0, hoy));
                continue;
            }
            out.push(vender(o, &mut lotes, compradas, hoy));
        }
    }
    out.sort_by(|a, b| a.op.fecha.cmp(&b.op.fecha));
    out
}

fn vender<'a>(o: &'a Operacion, lotes: &mut [Lote<'a>], compradas: usize, hoy: &str) -> Venta<'a> {
    let mut falta = o.cantidad;
    let mut v = Venta { op: o, transmision: r2(o.euros - o.comisiones), adquisicion: 0.0, lotes: Vec::new(), sin_coste: 0.0, resultado: 0.0, no_computa: 0.0, recompra: None, integradas: 0.0, provisional: false };
    for l in lotes[..compradas].iter_mut().filter(|l| l.quedan > DIF) {
        if falta <= DIF {
            break;
        }
        let t = falta.min(l.quedan);
        let parte = l.pendiente * t / l.quedan;
        l.pendiente -= parte;
        v.integradas -= parte;
        l.quedan -= t;
        v.adquisicion += t * l.coste;
        v.lotes.push((l.op.fecha.as_str(), t, r2(t * l.coste)));
        falta -= t;
    }
    v.sin_coste = if falta > DIF { falta } else { 0.0 };
    v.adquisicion = r2(v.adquisicion);
    v.integradas = r2(v.integradas);
    v.resultado = r2(v.transmision - v.adquisicion);
    if v.resultado >= 0.0 || !v.completa() {
        return v;
    }
    // Una pérdida: ¿recompraste valores homogéneos en los meses de antes o de después? (art. 33.5.f y g)
    let meses = Months::new(ahorro::meses_recompra(o.producto));
    let (Some(dia), Some(hoy)) = (fmt::fecha(&o.fecha), fmt::fecha(hoy)) else { return v };
    let (desde, hasta) = (dia.checked_sub_months(meses).unwrap_or(dia), dia.checked_add_months(meses).unwrap_or(dia));
    let perdida = -v.resultado;
    let mut bloquea = 0.0;
    for (i, l) in lotes.iter_mut().enumerate() {
        let Some(f) = fmt::fecha(&l.op.fecha) else { continue };
        if f < desde || f > hasta || bloquea >= o.cantidad - DIF {
            continue;
        }
        // Las de antes, las que sigues teniendo tras vender; las de después, todas.
        let caben = if i < compradas { l.quedan.min(l.libres) } else { l.libres };
        let t = caben.min(o.cantidad - bloquea);
        if t <= DIF {
            continue;
        }
        l.libres -= t;
        l.pendiente += perdida * t / o.cantidad;
        bloquea += t;
        v.recompra.get_or_insert(l.op.fecha.as_str());
    }
    v.no_computa = r2(perdida * bloquea / o.cantidad);
    v.provisional = hoy < hasta && v.no_computa < perdida - DIF;
    v
}

/* ================= 3. lo que tenías fuera (720 y 721) ================= */

/// Un bloque del 720 (o el 721) en un año.
#[derive(Debug, Clone)]
pub struct BloqueFuera<'a> {
    pub bloque: Bloque,
    pub euros: f64,
    pub posiciones: Vec<&'a Posicion>,
    /// Por dónde está depositado: (país, entidad, euros).
    pub por_deposito: Vec<(&'a str, &'a str, f64)>,
    /// El de ese año, si consta presentado en lo consultado a Hacienda.
    pub presentado: Option<Decl>,
    /// El último presentado antes, y lo que valía este bloque a su 31 de diciembre, si se sabe.
    pub ultimo: Option<(i32, Option<f64>)>,
    /// Si toca presentarlo (None en el año en curso: se sabrá el 31 de diciembre).
    pub toca: Option<Toca>,
}

/// Lo que tenías fuera de España en un año.
#[derive(Debug, Clone)]
pub struct Fuera<'a> {
    pub anio: i32,
    /// La fecha de lo que se ve: el 31 de diciembre o, en el año en curso, lo último traído.
    pub fecha: String,
    pub bloques: Vec<BloqueFuera<'a>>,
}

impl Fuera<'_> {
    pub fn al_cierre(&self) -> bool {
        self.fecha == format!("{}-12-31", self.anio)
    }
}

/// Lo presentado de un modelo de un ejercicio (lo último), en lo consultado a Hacienda.
fn presentada(d: &Datos, modelo: &str, ejercicio: i32) -> Option<Decl> {
    d.hacienda.values().flat_map(|h| h.modelos.values()).flat_map(|m| &m.decl).filter(|x| x.modelo == modelo && x.ejercicio == ejercicio).max_by(|a, b| a.fecha.cmp(&b.fecha)).cloned()
}

/// Lo que valía cada bloque a una fecha (si se trajo lo de esa fecha).
fn valor(posiciones: &[&Posicion], fechas: &[&str], fecha: &str, b: Bloque) -> Option<f64> {
    fechas.contains(&fecha).then(|| r2(posiciones.iter().filter(|p| p.fecha == fecha && Bloque::de(p.bien) == b).map(|p| p.euros).sum()))
}

/// Lo que tenías fuera en un año, por bloque, según tus brókeres (None si no hay nada traído de ese año).
pub fn fuera(d: &Datos, anio: i32) -> Option<Fuera<'_>> {
    let cs = carteras(d);
    let todas: Vec<&Posicion> = cs.iter().flat_map(|c| &c.posiciones).collect();
    // Las fechas traídas, también las que no tenían nada (lo vendiste todo).
    let fechas: Vec<&str> = cs.iter().flat_map(|c| c.cierres.iter().map(String::as_str)).chain(todas.iter().map(|p| p.fecha.as_str())).collect();
    let cierre = format!("{anio}-12-31");
    let fecha = if fechas.contains(&cierre.as_str()) {
        cierre
    } else {
        fechas.iter().filter(|f| fmt::anio_de(f) == anio).max()?.to_string()
    };
    let al_cierre = fecha.ends_with("-12-31");
    let bloques = Bloque::TODOS
        .into_iter()
        .map(|b| {
            let posiciones: Vec<&Posicion> = todas.iter().copied().filter(|p| p.fecha == fecha && Bloque::de(p.bien) == b).collect();
            let euros = r2(posiciones.iter().map(|p| p.euros).sum());
            let mut por_deposito: Vec<(&str, &str, f64)> = Vec::new();
            for p in &posiciones {
                match por_deposito.iter_mut().find(|(pa, de, _)| *pa == p.pais && *de == p.depositario) {
                    Some(x) => x.2 = r2(x.2 + p.euros),
                    None => por_deposito.push((&p.pais, &p.depositario, p.euros)),
                }
            }
            // El último presentado en el que iba este bloque: si entonces no llegaba al umbral, no iba.
            let ultimo = (b.desde()..anio)
                .rev()
                .filter(|y| presentada(d, b.modelo(), *y).is_some())
                .map(|y| (y, valor(&todas, &fechas, &format!("{y}-12-31"), b)))
                .find(|(_, v)| v.is_none_or(|v| v > modelo_720::UMBRAL));
            // ¿Has dejado de tener este año algo de lo que declaraste? (lo que había el 31 de diciembre del año
            // del último presentado y el del año anterior, y ya no está). Si no se trajo alguno de esos dos, no se
            // sabe (None): no es lo mismo que no tener nada.
            let isines = |f: &str| -> Option<Vec<&str>> {
                fechas.contains(&f).then(|| todas.iter().filter(|p| p.fecha == f && Bloque::de(p.bien) == b && !p.isin.is_empty()).map(|p| p.isin.as_str()).collect())
            };
            let extinguido = match ultimo {
                None => Some(false),
                Some((y, _)) => match (isines(&format!("{y}-12-31")), isines(&format!("{}-12-31", anio - 1)), isines(&fecha)) {
                    (Some(declarado), Some(antes), Some(ahora)) => Some(declarado.iter().any(|i| antes.contains(i) && !ahora.contains(i))),
                    _ => None,
                },
            };
            let toca = (al_cierre && anio >= b.desde()).then(|| modelo_720::toca(b, euros, ultimo, extinguido));
            BloqueFuera { bloque: b, euros, posiciones, por_deposito, presentado: presentada(d, b.modelo(), anio), ultimo, toca }
        })
        .collect();
    Some(Fuera { anio, fecha, bloques })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::fuente::Fuente;
    use crate::dominio::inversiones::{Bien, Cartera};

    fn op(fecha: &str, isin: &str, compra: bool, cantidad: f64, euros: f64, comisiones: f64) -> Operacion {
        Operacion { fecha: fecha.into(), isin: isin.into(), nombre: format!("Valor {isin}"), compra, cantidad, precio: euros / cantidad, divisa: "EUR".into(), cambio: 1.0, euros, comisiones, ..Default::default() }
    }

    fn con(ops: Vec<Operacion>, cobros: Vec<Cobro>, posiciones: Vec<Posicion>) -> Datos {
        let mut d = Datos::default();
        let c = Cartera { broker: "Bróker de prueba".into(), hasta: "2026-11-20".into(), operaciones: ops, cobros, posiciones, ..Default::default() };
        d.carteras.insert("prueba".into(), Fuente::nueva("prueba", "2026-11-20T10:00:00", Some(c), None));
        d
    }

    fn ventas_de<'a>(ops: &'a [Operacion], hoy: &str) -> Vec<Venta<'a>> {
        ventas(&ops.iter().collect::<Vec<_>>(), &BTreeMap::new(), hoy)
    }

    /// FIFO (art. 37.2): se venden primero los primeros que compraste; las comisiones, en los dos lados.
    #[test]
    fn fifo_con_comisiones() {
        let ops = vec![
            op("2024-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 2.0), // 100,20 cada uno
            op("2024-06-10T10:00:00", "XX1", true, 10.0, 1500.0, 3.0), // 150,30 cada uno
            op("2025-03-01T10:00:00", "XX1", false, 15.0, 2400.0, 4.0),
        ];
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!(v.len(), 1);
        let x = &v[0];
        assert_eq!((x.transmision, x.adquisicion), (2396.0, 1002.0 + 751.5));
        assert_eq!(x.lotes.iter().map(|l| l.1).collect::<Vec<_>>(), vec![10.0, 5.0]);
        assert_eq!((x.resultado, x.computa(), x.no_computa), (642.5, 642.5, 0.0));
        // Otro valor no se mezcla, y vender sin compra se dice.
        let ops = vec![op("2024-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0), op("2025-01-10T10:00:00", "XX2", false, 5.0, 500.0, 0.0)];
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!((v[0].sin_coste, v[0].completa()), (5.0, false));
    }

    /// En dólares: cada operación, al cambio del día de la operación (el bróker ya trae los euros a ese cambio).
    #[test]
    fn divisas_al_cambio_de_cada_operacion() {
        let mut compra = op("2024-01-10T10:00:00", "US1", true, 10.0, 0.0, 1.0);
        (compra.divisa, compra.precio, compra.cambio, compra.euros) = ("USD".into(), 110.0, 1.10, 1000.0);
        let mut venta = op("2025-01-10T10:00:00", "US1", false, 10.0, 0.0, 1.0);
        (venta.divisa, venta.precio, venta.cambio, venta.euros) = ("USD".into(), 110.0, 1.0, 1100.0);
        let ops = vec![compra, venta];
        let v = ventas_de(&ops, "2026-11-20");
        // El mismo precio en dólares, pero el dólar ha subido: se gana en euros.
        assert_eq!(v[0].resultado, 1099.0 - 1001.0);
    }

    /// Art. 33.5.f: la pérdida no cuenta si recompras en los dos meses de antes o de después, en la parte
    /// recomprada; cuenta cuando vendes lo recomprado.
    #[test]
    fn la_regla_de_los_dos_meses() {
        let base = || vec![op("2024-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0), op("2025-03-01T10:00:00", "XX1", false, 10.0, 600.0, 0.0)];
        // Sin recomprar: cuenta entera.
        let ops = base();
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!((v[0].resultado, v[0].computa(), v[0].recompra), (-400.0, -400.0, None));
        // Recompra 4 dentro de los dos meses de después: no cuentan 4/10 de la pérdida.
        let mut ops = base();
        ops.push(op("2025-04-15T10:00:00", "XX1", true, 4.0, 300.0, 0.0));
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!((v[0].no_computa, v[0].computa(), v[0].recompra), (160.0, -240.0, Some("2025-04-15T10:00:00")));
        // Fuera de los dos meses (el 2 de mayo), sí cuenta.
        let mut ops = base();
        ops.push(op("2025-05-02T10:00:00", "XX1", true, 4.0, 300.0, 0.0));
        assert_eq!(ventas_de(&ops, "2026-11-20")[0].no_computa, 0.0);
        // Una compra de antes, que sigues teniendo tras vender, también bloquea.
        let ops = vec![
            op("2024-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0),
            op("2025-02-01T10:00:00", "XX1", true, 5.0, 300.0, 0.0),
            op("2025-03-01T10:00:00", "XX1", false, 10.0, 600.0, 0.0),
        ];
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!((v[0].no_computa, v[0].computa()), (200.0, -200.0));
        // Y al vender lo recomprado, la pérdida de antes cuenta.
        let mut ops = base();
        ops.push(op("2025-04-15T10:00:00", "XX1", true, 4.0, 300.0, 0.0));
        ops.push(op("2026-01-10T10:00:00", "XX1", false, 4.0, 300.0, 0.0));
        let v = ventas_de(&ops, "2026-11-20");
        assert_eq!((v[1].resultado, v[1].integradas, v[1].computa()), (0.0, -160.0, -160.0));
        let d = con(ops.clone(), vec![], vec![]);
        assert_eq!(anio(&d, 2025, "2026-11-20").unwrap().pendientes, 160.0, "a fin de 2025, pendiente");
        assert_eq!(anio(&d, 2026, "2026-11-20").unwrap().pendientes, 0.0, "y en 2026 ya ha contado");
        // Si aún no han pasado los dos meses, es provisional.
        let ops = base();
        assert!(ventas_de(&ops, "2025-04-01")[0].provisional && !ventas_de(&ops, "2025-05-02")[0].provisional);
        // Un fondo que no cotiza: un año (art. 33.5.g).
        let mut ops = base();
        ops.iter_mut().for_each(|o| o.producto = Producto::Fondo);
        ops.push(op("2025-12-01T10:00:00", "XX1", true, 10.0, 600.0, 0.0));
        ops[2].producto = Producto::Fondo;
        assert_eq!(ventas_de(&ops, "2026-11-20")[0].no_computa, 400.0);
    }

    /// Una pérdida que no contó por la recompra cuenta al vender lo recomprado, aunque en esa venta haya más de lo que
    /// consta comprado (esa parte no entra, pero la pérdida de antes sí).
    #[test]
    fn la_perdida_de_antes_cuenta_aunque_la_venta_no_entre() {
        let ops = vec![
            op("2025-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0),
            op("2025-02-10T10:00:00", "XX1", false, 10.0, 500.0, 0.0), // pierdes 500
            op("2025-03-01T10:00:00", "XX1", true, 10.0, 500.0, 0.0), // recompras: no cuentan aún
            op("2026-06-01T10:00:00", "XX1", false, 15.0, 900.0, 0.0), // vendes lo recomprado y 5 más, sin compra
        ];
        let d = con(ops, vec![], vec![]);
        assert_eq!(anio(&d, 2025, "2026-11-20").unwrap().pendientes, 500.0);
        let a = anio(&d, 2026, "2026-11-20").unwrap();
        assert!(!a.ventas[0].completa());
        assert_eq!((a.perdidas, a.ganancias, a.pendientes, a.cuenta.ganancias), (500.0, 0.0, 0.0, -500.0));
        assert!(a.avisos.iter().any(|x| x.contains("no entra en las cuentas") && x.contains("500")), "{:?}", a.avisos);
    }

    /// Un split (o un traspaso) que el bróker no sabe traducir: lo vendido de ese valor desde ese día no entra en las
    /// cuentas, ni gasta las compras de antes (su coste ya no vale).
    #[test]
    fn tras_un_split_no_se_sabe_el_coste() {
        // Compras 10 a 100 €; split 10:1 (no llega como operación); vendes 10 de las 100 por 100 €.
        let ops = vec![op("2025-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0), op("2026-03-10T10:00:00", "XX1", false, 10.0, 100.0, 0.0), op("2026-01-10T10:00:00", "XX2", false, 1.0, 50.0, 0.0)];
        let mut d = con(ops, vec![], vec![]);
        if let Some(c) = d.carteras.get_mut("prueba").and_then(|f| f.dato.as_mut()) {
            c.operaciones.push(op("2024-01-10T10:00:00", "XX2", true, 1.0, 40.0, 0.0));
            c.sin_coste_desde.insert("XX1".into(), "2026-02-01T10:00:00".into());
        }
        let a = anio(&d, 2026, "2026-11-20").unwrap();
        let v = a.ventas.iter().find(|v| v.op.isin == "XX1").unwrap();
        assert!(!v.completa() && v.lotes.is_empty(), "{v:?}");
        assert_eq!((a.ganancias, a.perdidas), (10.0, 0.0), "solo la de XX2: la de XX1 no cuenta 900 € de pérdida");
        assert!(a.avisos.iter().any(|x| x.contains("no entra en las cuentas")), "{:?}", a.avisos);
        // Lo vendido antes del split, sí.
        let mut d = con(vec![op("2025-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0), op("2026-01-10T10:00:00", "XX1", false, 5.0, 600.0, 0.0)], vec![], vec![]);
        if let Some(c) = d.carteras.get_mut("prueba").and_then(|f| f.dato.as_mut()) {
            c.sin_coste_desde.insert("XX1".into(), "2026-02-01T10:00:00".into());
        }
        assert_eq!(anio(&d, 2026, "2026-11-20").unwrap().ganancias, 100.0);
    }

    fn cobro(fecha: &str, isin: &str, clase: Clase, bruto: f64, origen: f64, espana: f64, cambio: Option<f64>) -> Cobro {
        Cobro { fecha: fecha.into(), clase, isin: isin.into(), nombre: format!("Valor {isin}"), divisa: "EUR".into(), bruto, retenido_origen: origen, retenido_espana: espana, cambio }
    }

    /// Dividendos e intereses con sus retenciones; la deducción por doble imposición, con su límite.
    #[test]
    fn dividendos_retenciones_y_doble_imposicion() {
        let cobros = vec![
            cobro("2025-03-01", "US1", Clase::Dividendo, 100.0, 15.0, 0.0, Some(1.0)),
            cobro("2025-04-01", "US2", Clase::Dividendo, 100.0, 30.0, 0.0, Some(1.0)), // retenido de más: solo el 15 %
            cobro("2025-05-01", "ES1", Clase::Dividendo, 100.0, 0.0, 19.0, Some(1.0)),
            cobro("2025-06-01", "", Clase::Interes, 50.0, 0.0, 0.0, Some(1.0)),
            cobro("2025-07-01", "US3", Clase::Dividendo, 80.0, 12.0, 0.0, None), // sin cambio: fuera
            cobro("2024-07-01", "US3", Clase::Dividendo, 80.0, 12.0, 0.0, Some(1.0)), // otro año
        ];
        let d = con(vec![], cobros, vec![]);
        let a = anio(&d, 2025, "2026-11-20").unwrap();
        assert_eq!((a.dividendos, a.intereses, a.retenido_origen, a.retenido_espana), (300.0, 50.0, 45.0, 19.0));
        assert!(a.avisos.iter().any(|x| x.contains("no entra en las cuentas")), "{:?}", a.avisos);
        let c = &a.cuenta;
        assert_eq!((c.base, c.cuota), (350.0, 66.5), "350 € al 19 %");
        // Lo pagado fuera, como mucho el 15 % de cada uno (15 + 15), y como mucho el tipo medio (19 %) de lo cobrado fuera.
        assert_eq!((c.doble_imposicion, c.a_pagar), (30.0, r2(66.5 - 30.0 - 19.0)));
        // Con un tipo medio menor que lo retenido, manda el tipo medio.
        let c = cuenta(2025, 100.0, 0.0, 30.0, 100.0, 0.0);
        assert_eq!(c.doble_imposicion, 19.0);
    }

    /// Art. 49: un saldo negativo se compensa con el otro hasta el 25 % de este; lo demás, para otros años.
    #[test]
    fn integrar_y_compensar() {
        let c = cuenta(2025, 1000.0, -400.0, 0.0, 0.0, 0.0);
        assert_eq!((c.compensado, c.base, c.para_otros_anios), (250.0, 750.0, 150.0));
        let c = cuenta(2025, -100.0, 1000.0, 0.0, 0.0, 0.0);
        assert_eq!((c.compensado, c.base, c.para_otros_anios), (100.0, 900.0, 0.0));
        let c = cuenta(2025, 100.0, 7000.0, 0.0, 0.0, 0.0);
        assert_eq!((c.base, c.cuota, c.escala), (7100.0, r2(6000.0 * 0.19 + 1100.0 * 0.21), 2025));
        // La escala de cada año: 400.000 € en 2024 y en 2025.
        assert_eq!(cuenta(2025, 0.0, 400000.0, 0.0, 0.0, 0.0).cuota - cuenta(2024, 0.0, 400000.0, 0.0, 0.0, 0.0).cuota, 2000.0);
    }

    #[test]
    fn un_anio_con_todo() {
        let ops = vec![
            op("2024-01-10T10:00:00", "XX1", true, 10.0, 1000.0, 0.0),
            op("2025-03-01T10:00:00", "XX1", false, 10.0, 1500.0, 0.0),
            op("2024-01-10T10:00:00", "XX2", true, 10.0, 1000.0, 0.0),
            op("2025-03-01T10:00:00", "XX2", false, 10.0, 800.0, 0.0),
        ];
        let d = con(ops, vec![], vec![]);
        let a = anio(&d, 2025, "2026-11-20").unwrap();
        assert_eq!((a.ventas.len(), a.ganancias, a.perdidas, a.cuenta.ganancias, a.cuenta.base), (2, 500.0, 200.0, 300.0, 300.0));
        assert!(anio(&Datos::default(), 2025, "2026-11-20").is_none(), "sin nada traído");
        assert_eq!(anio(&d, 2024, "2026-11-20").unwrap().ventas.len(), 0);
    }

    /// ¿Hace falta guardar lo calculado? `cargo test --release mide_un_anio -- --ignored --nocapture`: un año con
    /// 5.000 operaciones inventadas de 50 valores en diez años.
    #[test]
    #[ignore]
    fn mide_un_anio() {
        let mut ops = Vec::new();
        for i in 0..5000 {
            let (y, m, d) = (2016 + i % 10, 1 + (i / 10) % 12, 1 + i % 28);
            ops.push(op(&format!("{y}-{m:02}-{d:02}T10:00:00"), &format!("XX{}", i % 50), i % 3 != 2, 10.0, 1000.0 + i as f64, 1.0));
        }
        let d = con(ops, vec![], vec![]);
        let t = std::time::Instant::now();
        for _ in 0..20 {
            assert!(anio(&d, 2025, "2026-11-20").is_some());
        }
        println!("5.000 operaciones: {:.2} ms por año calculado", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
    }

    fn pos(fecha: &str, bien: Bien, euros: f64) -> Posicion {
        Posicion { fecha: fecha.into(), bien, isin: "XX1".into(), nombre: "Valor".into(), cantidad: 1.0, euros, pais: "NL".into(), depositario: "Depositario de prueba".into() }
    }

    #[test]
    fn lo_que_tenias_fuera() {
        use crate::dominio::hacienda::{Consulta, Modelo};
        let posiciones = vec![
            pos("2024-12-31", Bien::Valores, 30000.0),
            pos("2024-12-31", Bien::Iic, 25000.0),
            pos("2024-12-31", Bien::Cuenta, 1000.0),
            pos("2025-12-31", Bien::Valores, 70000.0),
            pos("2026-11-20", Bien::Valores, 90000.0),
        ];
        let mut d = con(vec![], vec![], posiciones);
        let f = fuera(&d, 2024).unwrap();
        let v = &f.bloques[0];
        assert_eq!((v.bloque, v.euros, v.por_deposito.len()), (Bloque::Valores, 55000.0, 1));
        assert!(matches!(v.toca, Some(Toca::Si(_))), "55.000 en valores, sin haberlo presentado nunca");
        assert!(matches!(f.bloques[1].toca, Some(Toca::No(_))), "1.000 en cuentas");
        // Presentado el de 2024: en 2025 sube 15.000, no toca.
        let decl = crate::dominio::hacienda::Decl { modelo: "720".into(), ejercicio: 2024, periodo: "0A".into(), fecha: "2025-03-20T10:00:00".into(), ..Default::default() };
        d.hacienda.insert(2024, Consulta { modelos: [("720".to_string(), Modelo { fecha: String::new(), decl: vec![decl] })].into(), ..Default::default() });
        let f = fuera(&d, 2024).unwrap();
        assert!(f.bloques[0].presentado.is_some());
        let f = fuera(&d, 2025).unwrap();
        assert_eq!(f.bloques[0].ultimo, Some((2024, Some(55000.0))));
        assert!(matches!(f.bloques[0].toca, Some(Toca::No(_))), "{:?}", f.bloques[0].toca);
        // Si en 2025 ya no tienes un valor que declaraste en 2024, toca.
        let mut e = d.clone();
        if let Some(c) = e.carteras.get_mut("prueba").and_then(|f| f.dato.as_mut()) {
            c.posiciones.iter_mut().filter(|p| p.fecha == "2025-12-31").for_each(|p| p.isin = "XX9".into());
        }
        assert!(matches!(fuera(&e, 2025).unwrap().bloques[0].toca, Some(Toca::Si(ref por)) if por.contains("dejado de tener")));
        // El año en curso: lo último traído, sin decir aún si toca.
        let f = fuera(&d, 2026).unwrap();
        assert!(!f.al_cierre() && f.bloques[0].toca.is_none() && f.bloques[0].euros == 90000.0);
        assert!(fuera(&d, 2023).is_none());
        // Lo vendiste todo en 2025: no hay nada a 31 de diciembre, pero se trajo, y dejaste de tener lo declarado.
        let mut e = d.clone();
        if let Some(c) = e.carteras.get_mut("prueba").and_then(|f| f.dato.as_mut()) {
            c.posiciones.retain(|p| p.fecha != "2025-12-31");
            c.cierres = vec!["2024-12-31".into(), "2025-12-31".into()];
        }
        let f = fuera(&e, 2025).unwrap();
        assert!(f.al_cierre() && f.bloques[0].euros == 0.0 && matches!(f.bloques[0].toca, Some(Toca::Si(_))), "{:?}", f.bloques[0].toca);
        // Si no se trajo lo que tenías el 31 de diciembre del año anterior, no se sabe si has dejado de tener algo de lo
        // declarado (no es lo mismo que no tener nada): sube 5.000 € desde el de 2024, pero no se sabe.
        let mut e = d.clone();
        if let Some(c) = e.carteras.get_mut("prueba").and_then(|f| f.dato.as_mut()) {
            c.posiciones.retain(|p| p.fecha == "2024-12-31");
            c.posiciones.push(pos("2026-12-31", Bien::Valores, 60000.0));
        }
        let t = fuera(&e, 2026).unwrap().bloques[0].toca.clone();
        assert!(matches!(t, Some(Toca::NoSe(ref por)) if por.contains("no sé si has dejado de tener")), "{t:?}");
    }

    /// Un 720 de antes de 2020 (se consulta desde 2012) cuenta como el último presentado: ya no «toca» cada año por
    /// pasar de 50.000 €; sin saber con qué valor se presentó, no se sabe.
    #[test]
    fn un_720_de_antes_de_2020() {
        use crate::dominio::hacienda::{Consulta, Modelo};
        let mut d = con(vec![], vec![], vec![pos("2024-12-31", Bien::Valores, 60000.0), pos("2025-12-31", Bien::Valores, 65000.0)]);
        assert!(matches!(fuera(&d, 2025).unwrap().bloques[0].toca, Some(Toca::Si(_))), "sin ninguno presentado, toca");
        let decl = crate::dominio::hacienda::Decl { modelo: "720".into(), ejercicio: 2015, periodo: "0A".into(), fecha: "2016-03-20T10:00:00".into(), ..Default::default() };
        d.hacienda.insert(2015, Consulta { modelos: [("720".to_string(), Modelo { fecha: String::new(), decl: vec![decl] })].into(), ..Default::default() });
        let b = &fuera(&d, 2025).unwrap().bloques[0];
        assert_eq!(b.ultimo, Some((2015, None)));
        assert!(matches!(b.toca, Some(Toca::NoSe(ref por)) if por.contains("presentaste el de 2015")), "{:?}", b.toca);
    }
}
