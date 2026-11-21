# gesshtor

Instalar:

```sh
curl -fsSL https://raw.githubusercontent.com/alrevuelta/gesshtor/main/install.sh | sh
```

En Windows, el `.zip` de la última release. Los ejecutables para Linux, Mac (ARM e Intel) y Windows los hace el
CI de GitHub (`.github/workflows/ci.yml`) en cada push, después de pasar las pruebas: se descargan desde la
ejecución, en «Artifacts». Con una etiqueta (`git tag v0.1.0 && git push --tags`) salen además en una release, con
su `SHA256SUMS`. El de Linux es estático (musl): sirve en cualquier distribución. No están firmados: en Mac, si lo
bajas con el navegador (con `install.sh` no pasa), quítale el bloqueo con `xattr -d com.apple.quarantine gesshtor`;
en Windows, SmartScreen avisa la primera vez.

## Con tu certificado digital

`--certificado` acepta el `.p12` / `.pfx` de la FNMT (los modernos con AES y los antiguos). La contraseña se
pregunta sin que se vea (o se toma de `$PANEL_CLAVE_CERTIFICADO`, que la app quita de su entorno nada más leerla: no la
hereda ningún programa que lance, como `claude`), solo sirve para abrirlo y no se guarda en ningún sitio; la clave
privada se queda en memoria mientras la app está abierta.

- Abre la carpeta de la persona del certificado (por su NIF); si es nueva, le crea la suya.
- Trae, en segundo plano y de Hacienda y la Seguridad Social a la vez: lo presentado en Hacienda cada año, de todos los modelos que puede presentar una persona
  (100, 714, 720, 303, 130, 131, 390, 349, 347, 111, 115, 123, 180, 190, 193, 036, 037, 840; los que el servicio no
  da se descartan solos; de antes de 2020, solo el 720, desde 2012, para saber si ya lo presentaste), y los PDF que
  falten, tus datos de autónomo de la
  Seguridad Social (alta, actividad, cuota, recibos, regularizaciones) y las cotizaciones de tus nóminas mes a mes.
  `S` lo vuelve a traer; `--todo` vuelve a consultar todos los años.
- Solo habla con Hacienda (`agenciatributaria.gob.es`) y la Seguridad Social (`seg-social.es`): las redirecciones
  a cualquier otro sitio se cortan. Nada sale de tu ordenador hacia ningún otro lado, salvo dos cosas opcionales
  y apagadas por defecto: si activas la conexión con DEGIRO en Patrimonio, habla con `trader.degiro.nl`; y si
  enciendes que Claude clasifique los documentos que arrastras (ver abajo), esos documentos, y solo esos, van a
  Anthropic.
- La pantalla **Inicio** te saluda y resume: tu certificado y cuándo caduca, si ahora trabajas por cuenta ajena,
  por tu cuenta o las dos cosas, tu última nómina, tu alta, actividad y cuota de autónomo, lo último presentado y
  el próximo plazo.

## Arrastrar documentos (opcional)

Si sueltas en el terminal PDF o fotos (JPG, PNG, GIF, WebP) —uno o varios a la vez—, Claude Haiku dice si cada uno
es una **nómina**, un **gasto** o un **ingreso**, saca sus datos y la app te enseña lo que ha entendido: con `Enter`
va a su sitio (Empleo › Nóminas, Autónomo › Gastos o Autónomo › Facturas) con una copia del documento en
`documentos/`; con `Esc`, se descarta. Después se cambia en su pestaña como cualquier otro apunte. Lo que no se ha
podido leer, lo que no cuadra, lo que ya habías metido (el mismo fichero, o uno igual) y lo de un año cerrado se
dice y no se guarda.

Viene **apagado**, porque necesita Claude. Se enciende al abrir la app con una variable:

```sh
PANEL_CLASIFICAR=claude gesshtor --certificado …     # con Claude Code (el comando `claude`, con tu sesión)
PANEL_CLASIFICAR=api ANTHROPIC_API_KEY=sk-ant-… gesshtor --certificado …   # con la API de Anthropic y tu clave
```

- Con `claude`, cada documento se copia a una carpeta temporal y `claude -p --model haiku` solo puede leer ese
  fichero (sin tu configuración, sin MCP, sin guardar la sesión). Con `api`, va en base64 a `api.anthropic.com`,
  el único dominio nuevo, que solo se usa con esto encendido. En los dos casos el documento sale de tu ordenador
  hacia Anthropic; tus otros datos, no.
- Son dos preguntas pequeñas por documento (qué es y sus datos), con la respuesta en JSON: unos 0,2 céntimos de dólar
  y unos segundos por documento.
- La respuesta no es de fiar: se comprueba (fechas, que base + IVA dé el total, que lo devengado menos lo deducido dé
  el líquido, tipos de IVA y de retención que existen) y, si algo falla, no se apunta nada. Lo fiscal lo decide la
  app (`clasificar/entendido.rs`, con la ley citada): si el IVA de un gasto se deduce (factura a tu nombre con la
  cuota aparte), si lo autoliquidas tú (empresas de la UE o de fuera sin IVA) o si es IVA de otro país; y si una
  factura es nacional, a la UE, de fuera o exenta. Las categorías y lo deducible por defecto son las de siempre.
- Un ingreso que no es una factura tuya con número (el aviso de pago de un cliente) queda en **borrador**: se emite
  con `E` y toma número de tu serie. En otra moneda, la app pide cuántos euros fueron antes de guardarlo.

## Qué enseña

A la izquierda, el nombre y las secciones (arriba si la terminal es estrecha); arriba, las pestañas de la sección
(`1`, `2`, `3`) y, a su lado, el año que ves («‹ 2026 ›»), que se cambia con `-` y `+` (no sale donde no cuenta:
Inicio, tus clientes, la calculadora y Declaraciones con todos los años); a la derecha, tu nombre, la fecha y si está
al día. Abajo, las teclas de la pantalla. Los colores, uno por sección, están todos en `tui::widgets::tema`. Las
secciones salen si te tocan: Empleo si tienes o tuviste nómina, Autónomo si lo eres o lo fuiste (Patrimonio,
siempre).

| Sección | Entradas |
|---|---|
| **Resumen** | Inicio (el saludo, cómo va el año y cuánto falta para el próximo plazo, y en tarjetas lo esencial) · Años (uno por fila, el último el que está en curso: lo que ganaste por partes, lo que pagaste, lo que te quedó y lo que salió en tu Renta; Enter abre uno entero: el resumen de su Renta (modelo 100; `o` abre el PDF), cada fuente paso a paso con de dónde sale cada cifra (de lo que pagó tu empresa por ti a lo que te llegó, con la Seguridad Social por conceptos; de lo facturado a lo que te quedó), el % que se llevó el Estado, tu IRPF y cómo lo pagaste, y la Renta casilla a casilla; el año en curso, hasta hoy y con lo que saldría en tu Renta) · Declaraciones (todo lo presentado en Hacienda, de cualquier modelo, con sus casillas y su PDF) |
| **Empleo** | Sueldo (el año, si te retienen bien y mes a mes; Enter abre un mes con lo cotizado concepto a concepto) · Nóminas (donde se apuntan, una por mes, en cualquier año) |
| **Autónomo** | Facturas (con `C`, tus clientes: se abren con el de la factura elegida, y ahí se cambian, se añaden y se borran; se guardan solos al emitir, o al crearlos en el editor de la factura) · Gastos (con tus recibos de autónomo de la Seguridad Social, que entran solos como «Cuota de autónomos» y cuentan en el 130 y la Renta; filtrando por esa categoría, arriba sale lo de tus cuotas: la cuota y la base, la base próxima, los meses sin recibo y la regularización; Enter abre un recibo con su desglose) · Impuestos (en «Año», cómo va la actividad: lo facturado y lo gastado, los avisos, los trimestres y los meses; con ← →, cada trimestre con su 303 y su 130 casilla a casilla) |
| **Patrimonio** (lo que tienes y lo que te da; hoy, tu ahorro y lo que tienes fuera; los alquileres vendrán aquí) | Este año (con lo traído de tu bróker, `b`: lo que has ganado o perdido con lo que has vendido, tus dividendos e intereses con lo retenido y lo que te tocaría pagar con la escala del ahorro, paso a paso; venta a venta y cobro a cobro) · Declarado (lo del ahorro de cada Renta: intereses, dividendos, ganancias y pérdidas, también de criptomonedas, la base del ahorro y lo que pagaste por ella) · Modelo 720 (lo que tenías fuera de España a 31 de diciembre por bloque y por país, si te toca presentar el 720 o el 721 y si consta presentado) |
| **Utilidades** | Calculadora (de un sueldo bruto, lo que te queda al mes y cuánto se va en impuestos y cotizaciones; en escalera, de lo que le cuestas a tu empresa a lo que te queda, cada paso con qué es y su %; de lo que factura un autónomo, lo mismo: sus gastos, su cuota por tramos y su IRPF. Debajo, lo que te queda al mes en cada comunidad de territorio común, de más a menos: con ↑↓ se elige una y se ve cuánto más o menos al mes te queda en las demás. `d`, el detalle concepto a concepto (MEI y solidaridad incluidos). Con las reglas de su año: las cotizaciones, en `tgss/reglas.rs`; el IRPF, en `hacienda/irpf.rs`) |

En **Años**, los que no tienen Renta van en gris (también el que está en curso): sus cifras no son las de
Hacienda, y al abrirlos se dice qué es aproximado y de dónde sale.

Dos modos, según el año elegido con `-` `+`:

- **Años cerrados** (con Renta): solo lectura, con lo que dicen tu Renta, lo presentado y la Seguridad Social.
  Las nóminas sí se pueden apuntar, para ver el sueldo mes a mes.
- **Año en curso** (y el anterior mientras no tenga Renta): además puedes apuntar facturas y gastos.

«¿Te retienen bien?» (en Sueldo; en Años, al abrir el año en curso, lo que saldría en tu Renta si solo tienes
sueldo): sin Renta,
compara lo que te retienen en tus nóminas con el IRPF que te tocaría por ese sueldo (si cobras lo mismo hasta
diciembre), sin hijos ni discapacidad, sin otras rentas y en la comunidad de tu última Renta. Las reglas (escalas,
mínimos, reducción y deducción del trabajo) están en `hacienda/irpf.rs`, de 2022 a 2025; para otros años se
usan las más cercanas y se dice.

Se navega con letras y números: la letra de cada sección (`r` Resumen, `e` Empleo, `a` Autónomo, `p` Patrimonio,
`u` Utilidades) y, dentro, el número de cada pestaña (`1`, `2`, `3`…), que el menú lleva al lado. Así
`a2` es Gastos y `r3`, Declaraciones. Esas letras y números están reservados: ninguna pantalla los usa, salvo
escribiendo (en un campo, una búsqueda o un formulario), donde se escriben. `Tab` pasa a la pestaña siguiente, `o`
abre el PDF, `S` trae lo último de Hacienda y la Seguridad Social, `!` enseña lo que no se ha podido leer o traer
(ver «Cuando algo falla»), `q` sale y `?` enseña todas las teclas.

| Tecla | Qué hace |
|---|---|
| `r` `e` `a` `i` `u` | ir a la sección (a la pestaña en la que la dejaste) |
| `1` … `9` | ir a esa pestaña de la sección |
| `Tab`, `Mayús+Tab` | pestaña siguiente / anterior |
| `-` `+` | año anterior / siguiente |
| `Enter` | ver o editar lo elegido |
| `n` · `d` · `D` | nuevo · borrar · duplicar (facturas y gastos) |
| `E` · `c` · `C` | en Facturas: emitir · cobrada · tus clientes |
| `b` · `x` | en Patrimonio › Este año: traer lo de tu bróker (la primera vez, activar su conexión) · desconectarlo |
| `t` | en Gastos, el trimestre; en Declaraciones, todos los años; en Impuestos, todas las casillas |
| `/` · `o` · `S` · `?` · `q` | buscar · abrir el PDF · traer lo último · ayuda · salir |
| soltar un fichero | clasificarlo con Claude (si está encendido: ver «Arrastrar documentos») |
| `!` | lo que no se ha podido leer o traer, entero; `c` lo copia y lo guarda |

## Cómo está hecho

```
src/
├─ dominio/   los datos de una persona, con tipos: lo que apuntas (facturas, gastos, clientes, nóminas) y lo que
│             se trae (lo presentado en Hacienda, la Renta leída, la Seguridad Social; fuente.rs: cómo se
│             guarda cada cosa traída o leída; fallo.rs: el informe de lo que no se entiende); las facturas y
│             los gastos con lo que se calcula con ellos
│             (factura.rs: totales, numeración; gasto.rs: categorías, parte deducible, amortizaciones); su
│             carpeta (almacen.rs: datos.json, fuentes/, copias, candado) y cómo se escriben importes y
│             fechas (fmt.rs); lo de tus inversiones, en un formato común para cualquier bróker
│             (inversiones.rs)
├─ hacienda/  todo lo de Hacienda: lo presentado y los plazos (mod.rs); un fichero por modelo con todo lo
│             suyo (modelo_100.rs la Renta, modelo_130.rs, modelo_303.rs, modelo_390.rs) y lo que tienen en
│             común (modelo.rs, formulario.rs); preguntar lo presentado y bajar los PDF (consulta.rs), leerlos
│             (lectura.rs, pdf.rs, generico.rs), el IRPF de la actividad y de un sueldo (irpf.rs), las
│             reglas del ahorro (ahorro.rs) y cuándo toca el 720 y el 721 (modelo_720.rs)
├─ tgss/      la Seguridad Social: entrar con el certificado, Importass (importass.rs) y la Sede (sede.rs)
├─ brokers/   (opcional) los conectores con brókeres (degiro.rs): cada uno trae lo de tu cuenta y lo traduce al
│             formato común de dominio/inversiones.rs; no calculan nada
├─ red/       lo que comparten todos: el certificado y la conexión HTTPS (solo con sus dominios, Rust puro)
├─ sincronizar.rs  traer lo último de las dos con el certificado: lo presentado, los PDF y la Seguridad Social
├─ clasificar/  (opcional) lo que arrastras: las rutas pegadas, preguntar a Claude Haiku (haiku.rs) y comprobar
│             lo que contesta y pasarlo a nómina, gasto o factura (entendido.rs)
├─ informe/   lo que enseñan las pantallas, como funciones puras: el año, el empleo, el autónomo, el inicio, las
│             inversiones (inversiones.rs: lo declarado, FIFO, la recompra, la cuota del ahorro, el 720)
└─ tui/       la app: el menú (menu.rs) y una pantalla por fichero; las que tienen partes, con su nombre
              delante (anios_lista.rs, empleo_sueldo.rs…)
pruebas/      datos de prueba sin datos de nadie (ver Pruebas)
```

Una carpeta por cosa y, dentro, todos los ficheros en plano: sin carpetas dentro de carpetas.

**Dónde va cada cosa.** Cada capa usa solo las de arriba en esta lista: `dominio` no usa nada; `red` usa
`dominio`; `hacienda`, `tgss` y `brokers`, `dominio` y `red`; `sincronizar`, `clasificar` e `informe`, lo
anterior; `tui`, todo.

| Pregunta | Dónde |
|---|---|
| ¿Se guarda, o viaja entre capas tal cual? (también lo que viene de Hacienda y la Seguridad Social) | `dominio` |
| ¿Habla de una sola cosa y no necesita reglas de Hacienda? (los totales de una factura, si un recibo es la cuota de un mes) | `dominio`, como método de esa cosa |
| ¿Es una regla o el vocabulario de Hacienda o de la Seguridad Social? (los modelos, las escalas del IRPF) | `hacienda`, `tgss` |
| ¿Junta varias fuentes para contestar a una pantalla? | `informe` |
| ¿Es cómo se enseña? (colores, textos, columnas) | `tui` |

Y una condición: `informe` y `tui` nunca copian los campos de un tipo de `dominio` en otro tipo; lo usan, lo
referencian o lo suman.

**Cada modelo de Hacienda, en su fichero.** `modelo_303.rs` dice todo lo del 303: de dónde sale su especificación
(los diseños de registro oficiales de la AEAT), qué casillas tiene en cada periodo, las sumas con las que se
comprueba lo leído de su PDF y cómo se calcula con tus facturas y gastos. Lo mismo el 130, el 390 y la Renta. Los
tipos hacen el resto: `Modelo` (cuál es; un `match` sobre él no compila si falta uno), `Trimestral` (303 y 130,
los que la app calcula), `Periodo`, `Fallo` (por qué no se ha podido leer un PDF) y `Tipo` (cómo se presentó). Para
añadir un modelo con casillas: un `modelo_XXX.rs` con su `impl Formulario`, su caso en `Modelo` y su línea en
`lectura::leer_pdf`; el compilador no deja olvidar nada.

- **Datos**: la carpeta de cada persona, sin base de datos:

  ```
  datos.json        lo que apuntas (perfil, ajustes, clientes, facturas, gastos, nóminas): lo único que no se rehace
  fuentes/          lo traído y lo leído, cada cosa en su fichero (se puede borrar: se vuelve a traer o a leer)
    tgss.json                    lo de la Seguridad Social (autónomo, recibos, cotizaciones de las nóminas)
    aeat-2025-consulta.json      lo presentado en Hacienda en 2025 (lo que dice su consulta, modelo a modelo)
    aeat-2025-1T-303-CSV.json    lo leído del PDF de esa presentación (uno por PDF: una complementaria no pisa
    aeat-2024-0A-100-CSV.json    a la original; de varias del mismo periodo vale la última presentada)
    degiro.json                  lo traído de DEGIRO, si lo has conectado (en el formato común: ver Patrimonio y brókeres)
  justificantes/    los PDF de Hacienda (2025_1T_303_CSV.pdf)
  documentos/       los tuyos (facturas de gastos, tiques…)
  copias/           las últimas 20 copias de datos.json (una por sesión con cambios, antes del primero)
  ```

  Todo se escribe de forma atómica (temporal, a disco, renombrar y la carpeta a disco, para que aguante un corte
  de luz) y un candado impide que la abran dos ventanas a la vez. Cada fichero de `fuentes/` dice de dónde sale
  (`origen`), con qué se leyó (`lector`, `gesshtor`), la `huella` del PDF (tamaño y FNV-1a) y `cuando`; y el
  resultado: el último `dato` bueno (con los mismos tipos que usa la app) y, si el último intento falló, el `error`.
  Un fallo nunca pisa el último dato bueno (`Fuente::encima`).

  `datos.json` es de la versión 3. **Migraciones**: una carpeta de una versión anterior se pone al día sola al
  abrirla, paso a paso (`almacen::MIGRACIONES`: de cada versión, la función que la deja en la siguiente); antes deja
  una copia en `copias/antes-de-la-vN-….json`, que no se borra. Hoy hay un paso, de la 2 (con lo traído dentro de
  `datos.json` y `cache.json` con lo leído de los PDF) a la 3. Un cambio de formato es subir `VERSION` y añadir su
  paso. Uno de una versión sin paso no se abre.

  ¿Escala un único `datos.json` reescrito entero en cada cambio? Sí, de sobra: con 200 facturas (230 KB) guardar
  tarda unos 8 ms y cargar 1 ms; con 5.000 (5,7 MB), 16 ms y 11 ms (`cargo test --release mide_guardar_y_cargar --
  --ignored --nocapture`, en un Mac con SSD). Se guarda al confirmar un cambio, no en cada tecla, y si no ha cambiado
  nada no se escribe.
- **PDF**: se leen en segundo plano y en paralelo la primera vez (unos 130 ms para 40 PDF) y lo leído va a
  `fuentes/`; después arrancar es inmediato (≈ 20 ms hasta ver la pantalla). Se relee solo lo que cambia (otro
  nombre u otra huella) y todo al subir `VERSION_LECTOR`.
- **Para ver qué entiende la app** (sin certificado, no sale a la red):

  ```sh
  gesshtor json justificantes/2025_1T_303_CSV.pdf   # lo que lee de ese PDF (o por qué no), sin guardar nada
  gesshtor json renta.pdf 2024_0A_100               # uno con otro nombre: diciendo de qué es
  gesshtor json tgss [carpeta]                      # lo de la Seguridad Social guardado (fuentes/tgss.json)
  ```

  Lo demás está en `fuentes/`, tal cual. Si un PDF no se entiende, `json` sale con el código 2 y escribe aparte (en la
  salida de errores) su informe, para copiarlo (ver «Cuando algo falla»).
- **Lo presentado** sale de la consulta a Hacienda, y sus casillas de los PDF leídos: no se marca nada a mano.
- **Rápido**: cada pantalla calcula lo suyo una vez por (datos, año) y lo reutiliza al repintar.

## Patrimonio y brókeres

Tres piezas que no se mezclan:

- **El formato común** (`dominio/inversiones.rs`): lo que trae cualquier bróker, una `Cartera`: las compras y
  ventas (fecha, ISIN, cantidad, precio, divisa, el cambio de ese día, los euros y las comisiones), los dividendos
  e intereses (con lo retenido fuera y lo retenido en España) y lo que había a cada 31 de diciembre (su valor en
  euros, si es un valor, un fondo, efectivo o cripto, y el país y la entidad donde está depositado; y de qué fechas
  se ha traído, aunque no hubiera nada). Va a
  `fuentes/<conector>.json` (`Fuente<Cartera>`: un fallo nunca pisa lo último bueno). En `datos.json` solo va qué
  conectores has activado (`inversiones.conectores`).
- **El cálculo** (`informe/inversiones.rs`, con las reglas en `hacienda/ahorro.rs` y `hacienda/modelo_720.rs`): solo
  conoce ese formato. FIFO por valor homogéneo con todas las carteras juntas (art. 37.2 de la Ley del IRPF), las
  comisiones en los dos lados (art. 35), cada operación en euros a su cambio, la recompra en dos meses (un año si no
  cotiza) que deja la pérdida para cuando vendas lo recomprado (art. 33.5.f y g), dividendos e intereses en bruto
  con sus retenciones, la deducción por doble imposición con su límite (art. 80), integrar y compensar (art. 49) y
  la escala del ahorro de cada año (arts. 66 y 76; comprobada hasta 2026). Y el 720 y el 721: por bloque, desde
  50.000 €, y otra vez si sube más de 20.000 € o si dejas de tener algo declarado. De dónde sale cada regla, en los
  `//!` de cada fichero.
- **Los conectores** (`brokers/`): cada uno, en su fichero, solo traduce de la web de su bróker al formato común.

**Añadir un bróker** (Interactive Brokers, o leer un CSV): un fichero `brokers/<id>.rs` con su
`pub const CONECTOR: Conector` (su id, su nombre, los dominios con los que habla, el aviso que se enseña al
activarlo, lo que se pide para entrar, dónde guarda tus cosas para el 720 y su función `traer`, que devuelve una `Cartera` o un error que diga qué
esperaba y qué llegó), su `mod` en `brokers/mod.rs` y su línea en `CONECTORES`. Sus dominios, en `red/cliente.rs` y
en la lista de con quién habla la app (aquí y en el README). Ni el cálculo ni la pantalla cambian. Lo que pide se
pide cada vez en una ventana (lo secreto no se ve) y no se guarda; `brokers::traer` no lo llama si el conector no
está activado, y su conexión solo puede ir a sus dominios. Con más de un bróker, `b` y `x` preguntan cuál.

**DEGIRO** (`brokers/degiro.rs`). Entra como tu navegador, por la API no oficial de su web: DEGIRO no tiene una
pública («actualmente no ofrecemos una API», dice su ayuda). Las direcciones, cómo se entra (con el código de tu app
de verificación o confirmando en la app de DEGIRO) y la forma de las respuestas salen del código de proyectos libres
y mantenidos que hablan con ella (degiro-connector, degiro-mcp, ghostfolio-degiro-sync, stonks-overwatch,
degiro-renta), citados en el fichero. **Ojo**: el Contrato de Cliente de DEGIRO (versión 20220830, art. 5.6 de sus
Condiciones de los Servicios de Inversión) dice: «No se permite al Cliente utilizar Herramientas Automatizadas en la
plataforma de inversión de DEGIRO. Si DEGIRO detecta Herramientas Automatizadas o actividades que apunten a su uso,
las consultas, actividades o las cuentas involucradas podrían ser bloqueadas por DEGIRO»; y su ayuda, que «el uso de
herramientas de automatización de terceros infringe nuestros términos de servicio». Por eso va desactivado y se
avisa al activarlo. Para el 720, según ese contrato, tus valores los mantienen sus SPV (fundaciones de los Países
Bajos, arts. 9.1 y 17) y tu efectivo está en tu cuenta en flatexDEGIRO Bank, en Alemania (art. 1).

Cómo se prueba con tu cuenta (las pruebas no salen a la red, y no hay cuenta de prueba):

1. `gesshtor --certificado …`, `p` (Patrimonio) y `b`: lee el aviso y actívalo.
2. Escribe tu usuario, tu contraseña y, si usas una app de verificación, su código de 6 cifras (si no, déjalo vacío
   y confirma en la app de DEGIRO cuando lo diga la pantalla; espera unos dos minutos). Un solo intento por `b`:
   DEGIRO castiga los reintentos seguidos (429, captcha o bloqueo).
3. Arriba se ve por dónde va (tus operaciones año a año desde 2008, qué es cada valor, tu cartera a cada 31 de
   diciembre). Al acabar, mira `fuentes/degiro.json` y compáralo con tu «Informe anual» de DEGIRO: las ventas, los
   dividendos y la cartera a 31 de diciembre.
4. Si falla, el mensaje (también en `fuentes/degiro.json`, en `error`) dice qué esperaba y la forma de lo que llegó,
   sin ningún dato tuyo: con eso se corrige `brokers/degiro.rs`. Lo de antes no se pierde.

## Cuando algo falla

Lo que enseña la app sale de los PDF de Hacienda y de las páginas de la Seguridad Social, que pueden cambiar sin
avisar. Por eso lo que no entiende es un **fallo**: a la vista, con un informe para arreglarlo, y nunca un 0 o un vacío
que parezca un dato.

**La regla.** Vacío (o 0) solo vale si el PDF o la página lo dicen (su señal de «sin datos») o tienen las anclas de su
estructura y simplemente no traen eso. Si faltan las anclas, o lo que está no se entiende (una etiqueta sin su
importe, un importe o una fecha que no lo son), es un fallo:

| De dónde | Vacío vale si… | Es un fallo si… |
|---|---|---|
| PDF del 303, 130 o 390 | (las casillas que no se rellenan valen 0, como en el impreso) | es de otro modelo; no se ve su devengo (ejercicio y periodo) o es de otro; no se ve su ancla (el resultado: la 71 del 303, la 19 del 130; la 65 y la 108 del 390); no cuadra con sus sumas; o tiene portada y no se entiende cómo se presentó (salvo el 390, que no lo dice) |
| PDF de la Renta | un bloque (trabajo, capital, actividad, ganancias, cuotas, pagos) no tiene ninguna de sus casillas: no aplica | un bloque tiene alguna de sus casillas y no cuadra; o lo leído no cuadra consigo mismo (`modelo_100::comprobar`, al leer) |
| Lo presentado en Hacienda | Hacienda lo dice con su mensaje («no ha presentado…») | contesta sin declaraciones ni mensaje; o da una sin su CSV o su periodo |
| Importass: tu área personal | — | no lleva la marca de tu colectivo (`ColectivoRETA`…); eras autónomo y ya no lo dice; o lo eres y no está el enlace a tus datos |
| Importass: tus datos de autónomo | un importe sin ninguna cifra («No consta», «-») | no está la fecha de alta; o una fecha o un importe que está no se entiende |
| Importass: tus recibos | la lista está y no tiene recibos | no está la lista (`quota-history`); o un recibo sin importe o sin mes, con un concepto que no se entiende, o una cuota sin las contingencias comunes |
| Importass: tus regularizaciones | la página está y no tiene ninguna | no es la página; un año con un resultado o un importe que no se entiende; o un detalle sin su año |
| La Sede (tus nóminas) | la Sede lo dice: su aviso 7101 («no existe liquidación») | otra página u otro mes; una lista vacía sin ese aviso; una liquidación sin tipo, sin empresa o sin conceptos; o una cifra que no se entiende (vacía vale 0: así deja la Sede la base de las líneas de totales) |
| `fuentes/*.json` | no está: se vuelve a traer o a leer | está y no se entiende (también un importe o un resultado que no existe en `tgss.json`). Al volver a traerlo, se aparta como `<nombre>.ilegible` |

Lo que aún no la sigue, por no romper lo que se lee bien: la comunidad de las Rentas de 2020 a 2022 (su PDF no la trae
junto a la casilla 0070: sale vacía) y la fecha de presentación de algunas portadas antiguas (va en otra columna: para
elegir entre varias del mismo periodo vale el nombre del PDF). Los conceptos de un recibo que la app no desglosa
(reducciones…) se dejan: el importe del recibo ya los cuenta.

**Un fallo nunca pisa el último dato bueno** (`Fuente::encima`): se queda lo de antes, con el fallo apuntado y a la
vista.

**El informe** (`dominio::fallo::Error`, guardado con su fuente en `fuentes/`): un código estable (`AEAT_303_NO_CUADRA`,
`TGSS_RECIBOS_SIN_LISTA`…), de dónde (el modelo y el periodo del PDF, o la página sin su consulta), el código que lo lee
(fichero y función), la versión de gesshtor con su commit y la del lector, qué se esperaba, qué se encontró, un
fragmento corto de alrededor y cómo reproducirlo. El fragmento es solo la estructura: de un PDF, el impreso sin lo que
rellenó Hacienda (de su portada, solo las etiquetas); de una página, sus etiquetas, título y clases, sin su texto. Y
encima se tapa lo que se reconoce como dato personal (NIF, IBAN, correos, CSV, justificantes y otros números largos,
códigos de sesión, lo que sigue a «domicilio», «calle», «nacimiento» o «pagador», los nombres de la persona y los que
se ven junto a un NIF o a «Razón social:»). No todo se reconoce: el usuario lo pega fuera, y el informe le pide que lo
revise antes.

**En la app**, mientras haya fallos la barra de abajo lo dice («⚠ 2 cosas no se han podido leer o traer · ! para
verlas…»). `!` los enseña enteros; `c` los copia al portapapeles (OSC 52: iTerm2, kitty, WezTerm, Alacritty, Windows
Terminal; la Terminal del Mac no) y los guarda en `informe-de-fallos.txt`, en la carpeta de la persona. Ese texto es
para pegárselo a un LLM con el código de gesshtor.

**Para reproducirlo**: un PDF, `gesshtor json justificantes/<el PDF>` (lo leído en JSON y, aparte, el informe); la
Seguridad Social, `S` (o `--sincronizar`) y, sin red, la página guardada en una prueba de su lector. Al subir
`VERSION_LECTOR` se releen todos los PDF con las reglas nuevas.

## Pruebas

```sh
cargo test
PANTALLA=Sueldo TECLAS="<down>" W=120 H=36 cargo test ver_pantalla -- --nocapture --ignored   # ver una pantalla
DATOS=mi/carpeta SALIDA=/tmp/antes.txt cargo test instantaneas -- --ignored                    # todas, a un fichero
PANTALLA=Facturas SALIDA=/tmp/f.json cargo test captura -- --ignored                             # una, con sus colores (JSON)
CLASIFICAR=carpeta cargo test clasificar_de_verdad -- --ignored --nocapture                       # documentos de verdad, con Claude
```

- Cada prueba está en el fichero de lo que prueba. Ninguna necesita datos de nadie; los datos de prueba están en
  `pruebas/`:
  - tres personas inventadas (`a-de-todo/`, `b-hacienda/`, `c-sin-gdj/`: su `datos.json` y, como no tienen PDF,
    `lecturas.json`): con ellas se recorren todas las pantallas en todos los años y tamaños;
  - las Rentas: PDF de verdad, leídos directamente (`renta_*.pdf`, con lo que tiene que salir en
    `renta_*_esperado.json`). **No van al repositorio** (multiplicar los importes por un factor no basta, se puede
    deducir) y sus pruebas se pasan a mano, con `cargo test -- --ignored`. En el repositorio no hay ningún PDF;
  - las respuestas de Hacienda de ejemplo van dentro de sus pruebas. No hay ningún certificado: las pruebas que
    necesitan uno usan `Certificado::de_prueba` (un NIF y un nombre, sin clave), y nada sale a la red.
- Lo que habla con Hacienda o la Seguridad Social (entrar con tu certificado, abrir un `.p12`, leer sus páginas) no
  se prueba: necesita tu certificado.
  Solo se prueban las utilidades para leer HTML, los importes y las fechas, que un trozo de página que no es como se
  espera sea un fallo (con trozos inventados, no páginas enteras) y qué meses de nómina se piden.
- Lo de los brókeres se prueba con respuestas inventadas con la forma de las de verdad (`brokers/degiro.rs`), y el
  cálculo, con una prueba por regla (`informe/inversiones.rs`). Con 5.000 operaciones, un año se calcula en unos
  5 ms (`cargo test --release mide_un_anio -- --ignored --nocapture`); aun así, «Este año» lo guarda hasta que
  cambien los datos, el año o el ancho.
- `instantaneas` pinta todas las pantallas de una carpeta: sirve para comprobar que un cambio no cambia lo que se
  ve (antes y después, y comparar los dos ficheros).
- Clasificar documentos se prueba sin red: las rutas pegadas, las respuestas de Claude (buenas, malas y rotas) y su
  paso a nómina, gasto o factura. `clasificar_de_verdad` manda a Claude (con el comando `claude`, o la API con
  `PANEL_CLASIFICAR=api`) los documentos de una carpeta con `nominas/`, `gastos/` e `ingresos/`, y saca una tabla de
  aciertos sin datos de nadie (con un `esperado.json` opcional para comprobar campo a campo).

## Pendiente

- El 390 en Impuestos; la previsión de la Renta con actividad de autónomo y la de la regularización de la cuota.
- Ajustes: tu situación (hijos, discapacidad, edad, comunidad) para afinar «¿te retienen bien?», los m² de casa y
  despacho, la serie de las facturas…; las reglas del IRPF de 2026 cuando salga el Manual.
- Rectificativas de facturas.
- Presentar declaraciones: fuera de la primera versión (solo se consulta).
- Patrimonio: los alquileres; más conectores (Interactive Brokers, un CSV); los splits, fusiones y cambios de ISIN (hoy se avisan, y
  lo que vendas de ese valor desde entonces no entra en las cuentas); los traspasos entre fondos (art. 94); el saldo
  medio del último trimestre de las cuentas para el 720; las acciones en otra divisa como dice la DGT (la diferencia
  en la divisa, al cambio del día de la venta).
- Firmar los ejecutables de Mac y Windows, para que no avisen al abrirlos.
