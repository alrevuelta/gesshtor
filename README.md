# gesshtor

`gesshtor` es una herramienta para el terminal que permite a autónomos y/o asalariados llevar la contabilidad de su actividad, con las siguientes *features*:
* Acceder a toda la información de Hacienda y Seguridad Social en un único *dashboard*.
* Llevar la contabilidad de nóminas del año en curso y pasados para anticiparse a la retención en la renta.
* Emitir facturas y contabilizar gastos para autónomos, calcular IVA, ver cuotas.
* (Opcional, apagado por defecto) Arrastrar nóminas, facturas y tiques al terminal para que Claude Haiku diga qué son y los apunte: se enciende con `PANEL_CLASIFICAR=claude` (Claude Code) o `PANEL_CLASIFICAR=api` y `ANTHROPIC_API_KEY`. Encendido, esos documentos se mandan a Anthropic; si no, la app solo habla con Hacienda y la Seguridad Social (ver [AVANZADO.md](AVANZADO.md)).
* Ver tus inversiones: lo que declaraste de intereses, dividendos y ventas y, si conectas DEGIRO (opcional, desactivado por defecto), lo que has ganado o perdido este año, lo que te tocaría pagar y si te toca el modelo 720.

# uso

```sh
curl -fsSL https://raw.githubusercontent.com/alrevuelta/gesshtor/main/install.sh | sh
```

Run:
```sh
./gesshtor --certificado tucertificado.p12
```

# con quién habla

Solo con Hacienda (`agenciatributaria.gob.es`) y la Seguridad Social (`seg-social.es`) y, solo si activas su conexión en Inversiones, con DEGIRO (`trader.degiro.nl`). Nada más sale de tu ordenador. Tu usuario y tu contraseña de DEGIRO se piden cada vez y no se guardan.

# disclaimer

Aviso. gesshtor es un proyecto personal, sin relación con la Agencia Tributaria ni con la Seguridad Social. Lo que enseña y calcula es orientativo y puede tener errores. No es asesoramiento fiscal y no sustituye a Hacienda, a la Seguridad Social ni a un asesor. Úsalo bajo tu responsabilidad y comprueba siempre los datos oficiales antes de presentar o pagar nada. Se ofrece tal cual, sin ninguna garantía.