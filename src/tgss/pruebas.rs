//! Pruebas de la Seguridad Social: cada lector con páginas inventadas (las de `pagina`), comparado con lo que tiene
//! que salir de esas mismas páginas (`pruebas/tgss_esperado.json`, leído con los tipos); los flujos (entrar,
//! consultar, las nóminas) con una Seguridad Social de mentira (`falsa`); qué meses se piden; y que lo traído se
//! guarda y se vuelve a leer igual.
use serde_json::Value;
use std::sync::OnceLock;

/// Una página de prueba, con sus `{MARCAS}` cambiadas. Son inventadas: copian la forma de las de verdad, sin datos de
/// nadie.
pub fn pagina(nombre: &str, cambios: &[(&str, &str)]) -> String {
    let mut t = match nombre {
        "ipce_403.html" => r##"<!DOCTYPE html>
<html><head><title>403 Forbidden</title></head><body><h1>Forbidden</h1><p>Certificado no válido.</p></body></html>"##,
        "login_formulario.html" => r##"<!DOCTYPE html>
<html><head><title>Acceso</title></head>
<body>
<form id="loginForm" action="/PGIS/LoginSubmit;jsessionid=XYZ" method="post">
<input type="hidden" name="loginFormSubmit" value="1">
<input type="hidden" name="javax.faces.ViewState" value="e1s1">
</form>
</body></html>"##,
        "login_respuesta.html" => r##"<!DOCTYPE html>
<html><head><title>IPCE</title></head>
<body onload="document.forms[0].submit()">
<form method="post" action="https://portal.seg-social.gob.es/wps/saml/acs">
<input type="hidden" name="SAMLResponse" value="UFJVRUJBLVNBTUwtUkVTUE9OU0U="/>
<input type="hidden" name="RelayState" value="importass"/>
</form>
</body></html>"##,
        "login_saml.html" => r##"<!DOCTYPE html>
<html><head><title>Redirigiendo</title></head>
<body onload="document.forms[0].submit()">
<noscript><p>Pulse Continuar.</p></noscript>
<FORM METHOD="POST" ACTION="https://idp.seg-social.es/PGIS/Login#inicio">
<input type="hidden" name="SAMLRequest" value="UFJVRUJBLVNBTUwtUkVRVUVTVA=="/>
<input type="hidden" name="RelayState" value="https&#x3a;&#x2f;&#x2f;portal.seg-social.gob.es&#x2f;wps"/>
<input type="submit" value="Continuar"/>
</FORM>
</body></html>"##,
        "login_seleccion.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Seguridad Social - Identificación</title>
<base href="/PGIS/">
</head>
<body>
<h1>Elige cómo identificarte</h1>
<ul>
  <li><a href="Login?seleccion=CLAVE">Cl@ve</a></li>
  <li><a href="Login?seleccion=IPCE">DNIe o certificado</a></li>
</ul>
<form id="buscar" action="/buscar" method="get"><input name="q" value=""></form>
<form id="formSeleccion" name="formSeleccion" action="Login" method='post'>
  <input type="hidden" name="idSesion" value="SES-0001">
  <input type="hidden" name="tipo" value="A&amp;B">
  <input type="radio" name="metodo" value="CLAVE">
  <input type="radio" name="metodo" value="IPCE" checked="checked">
  <input type="checkbox" name="recordar" value="1">
  <select name="idioma"><option value="es" selected>Español</option></select>
  <textarea name="nota"></textarea>
  <button type="submit" name="accion" value="seleccionar">Seguir</button>
  <input type="image" name="imagen" src="x.png">
  <input type="hidden" name="idSesion" value="SES-0002">
</form>
</body></html>"##,
        "mantenimiento.html" => r##"<!DOCTYPE html>
<html><head><title>  Servicio en
  mantenimiento </title></head><body><p>Vuelva a intentarlo más tarde.</p></body></html>"##,
        "perfil.html" => r##"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="UTF-8">
<title>Datos de trabajo aut&oacute;nomo - Importass</title>
<script type="text/javascript">var ibmCfg = {"userConfig":{"authenticated":"true"}};
var ayuda = "Esta es la cuota que corresponde al mes de enero";</script>
<style>.ss-data dt { font-weight: bold }</style>
</head>
<body>
<h1>Datos de trabajo aut&oacute;nomo</h1>
<section class="ss-card">
  <h2>Alta</h2>
  <dl class="ss-data" aria-label="Datos del alta">
    <dt class="ss-data__term">Fecha alta como aut&oacute;nomo</dt>
    <dd class="ss-data__desc">1 de marzo de 2021</dd>
    <dt class="ss-data__term">Cotizas desde</dt>
    <dd class="ss-data__desc">01/03/2021</dd>
  </dl>
</section>
<section class="ss-card">
  <h2>Actividades</h2>
  <dl class="ss-data" aria-label="Actividad 1">
    <dt>Fecha de inicio de la actividad</dt><dd>01/03/2021</dd>
    <dt>Actividad de aut&oacute;nomo</dt><dd>Trabajador por mi cuenta</dd>
    <dt>Actividad profesional (CNAE)</dt><dd>Programaci&oacute;n inform&aacute;tica   CNAE 6201 </dd>
    <dt>Direcci&oacute;n de la actividad</dt><dd>Calle Inventada 1<br>28000 Madrid</dd>
  </dl>
  <dl class="ss-data" aria-label="Actividad 2">
    <dt>Fecha de inicio de la actividad</dt><dd>15 de junio de 2023</dd>
    <dt>Actividad de autónomo</dt><dd>Socio de sociedad mercantil</dd>
  </dl>
  <dl class="ss-data" aria-label="Sociedad">
    <dt>Nombre de la sociedad</dt><dd>SOCIEDAD INVENTADA SL</dd>
    <dt>NIF de la sociedad</dt><dd>B00000000</dd>
  </dl>
  <dl aria-label="Raro"><dt>Solo término</dt><span>x</span><dt>Fecha baja como autónomo</dt><dd>-</dd></dl>
</section>
<section class="ss-card">
  <h2>Cotizaci&oacute;n</h2>
  <dl class="ss-data" aria-label="Cotización">
    <dt>Rendimientos netos previstos</dt><dd>1.700 €/mes</dd>
    <dt>Base de cotización actual</dt><dd>1.000,00 €/mes</dd>
    <dt>Base de cotización próxima</dt><dd>1.100,50 €/mes a partir del 1 de enero de 2027</dd>
  </dl>
  <dl class="ss-data" aria-label="Última cuota">
    <dt>Última cuota emitida</dt><dd>310,00 €</dd>
  </dl>
  <p>Es la cuota que corresponde al mes de <strong>Octubre</strong>.</p>
</section>
<section class="ss-card">
  <h2>Mutua</h2>
  <dl class="ss-data" aria-label="Mutua">
    <dt>Entidad aseguradora</dt><dd>MUTUA INVENTADA N&ordm; 0</dd>
    <dt>Cobertura por contingencias comunes</dt><dd>Sí</dd>
    <dt>Cobertura por contingencias profesionales</dt><dd>Sí</dd>
    <dt>Cobertura por cese de actividad</dt><dd>Sí</dd>
    <dt>Cobertura por formación profesional</dt><dd>No</dd>
  </dl>
</section>
</body>
</html>"##,
        "personal.html" => r##"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="UTF-8">
<title>Importass - Tu &aacute;rea personal</title>
<base href="https://portal.seg-social.gob.es/wps/myportal/importass/importass/personal/!ut/p/z1/04_Sj9CPykssy0xPLMnMz0vMAfIjo8zijQx/">
<script type="text/javascript">var ibmCfg = {"userConfig":{"authenticated":"true","locale":"es"}};</script>
</head>
<body class="importass ColectivoRETA">
<nav>
  <a href="p0/IZ7_PERFIL00=CZ6_ACC_PERFIL_AUTONOMO=LA0=/#Z7_PERFIL00">Datos de trabajo aut&oacute;nomo</a>
  <a href=" p0/IZ7_REG00=CZ6_ACC_REGULARIZACION_AUTONOMO=2024&amp;anio=2024/">Regularizaci&oacute;n 2024</a>
  <a href="p0/IZ7_REG00=CZ6_ACC_REGULARIZACION_AUTONOMO=2021&amp;anio=2021/#arriba">Regularizaci&oacute;n 2021</a>
  <a href="/wps/portal/otra/cosa">Otra cosa</a>
</nav>
</body>
</html>"##,
        "personal_asalariado.html" => r##"<!DOCTYPE html>
<html lang="es"><head><title>Importass - Tu área personal</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body class="importass ColectivoRGSS"><p>Hola</p></body></html>"##,
        "prosa_detalle_l00.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Detalle</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}D</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <DATOS>
  <tituloPantalla>DETALLE DE LA LIQUIDACIÓN PARA {MM}/{A}</tituloPantalla>
  <razonSocialEmp>EMPRESA INVENTADA &amp; ASOCIADOS, S.L.</razonSocialEmp>
  <ccc tipo="texto">28000000000</ccc>
  <tipoLiq>L00</tipoLiq>
  <descTipoLiq>NORMAL</descTipoLiq>
  <fechHoraCalc>15/04/2026 10:20:30</fechHoraCalc>
  <CrtrEnCalcAcumTrabajador><desDatCal>CONTINGENCIAS COMUNES</desDatCal><baseCalValor>2.500,00</baseCalValor><cuotaCalValPatronal>590,00</cuotaCalValPatronal><cuotaCalValObrera>117,00</cuotaCalValObrera><cuotaCalVaTotal>707,00</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>OTRAS COTIZACIONES</desDatCal><baseCalValor>2.500,00</baseCalValor><cuotaCalValPatronal>162,50</cuotaCalValPatronal><cuotaCalValObrera>40,75</cuotaCalValObrera><cuotaCalVaTotal>203,25</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>IT DE ACCIDENTES DE TRABAJO</desDatCal><baseCalValor>2.500,00</baseCalValor><cuotaCalValPatronal>16,25</cuotaCalValPatronal><cuotaCalValObrera>0,00</cuotaCalValObrera><cuotaCalVaTotal>16,25</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>MECANISMO EQUIDAD INTERGENERACIONAL</desDatCal><baseCalValor>2.500,00</baseCalValor><cuotaCalValPatronal>14,50</cuotaCalValPatronal><cuotaCalValObrera>3,00</cuotaCalValObrera><cuotaCalVaTotal>17,50</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>LIQUIDO DE TOTALES</desDatCal><baseCalValor></baseCalValor><cuotaCalValPatronal>783,25</cuotaCalValPatronal><cuotaCalValObrera>160,75</cuotaCalValObrera><cuotaCalVaTotal>944,00</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
 </DATOS>
</SPM>
</script>
</body></html>"##,
        "prosa_detalle_l03.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Detalle</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}D</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <DATOS>
  <tituloPantalla>DETALLE DE LA LIQUIDACIÓN PARA {MM}/{A}</tituloPantalla>
  <razonSocialEmp>EMPRESA INVENTADA &amp; ASOCIADOS, S.L.</razonSocialEmp>
  <ccc>28000000000</ccc>
  <tipoLiq>L03</tipoLiq>
  <descTipoLiq>COMP. ABONO SALARIOS CARACTER RETROACTIV</descTipoLiq>
  <fechHoraCalc>20/06/2026 09:00:00</fechHoraCalc>
  <CrtrEnCalcAcumTrabajador><desDatCal>CONTINGENCIAS COMUNES</desDatCal><baseCalValor>100,10</baseCalValor><cuotaCalValPatronal>23,62</cuotaCalValPatronal><cuotaCalValObrera>4,70</cuotaCalValObrera><cuotaCalVaTotal>28,32</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>OTRAS COTIZACIONES</desDatCal><baseCalValor>100,10</baseCalValor><cuotaCalValPatronal>6,51</cuotaCalValPatronal><cuotaCalValObrera>1,63</cuotaCalValObrera><cuotaCalVaTotal>8,14</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>LIQUIDO CONTINGENCIAS COMUNES</desDatCal><baseCalValor></baseCalValor><cuotaCalValPatronal>23,62</cuotaCalValPatronal><cuotaCalValObrera>4,70</cuotaCalValObrera><cuotaCalVaTotal>28,32</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
 </DATOS>
</SPM>
</script>
</body></html>"##,
        "prosa_directa.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Detalle</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}N</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <CrtrEnSalidaNCBN2211>
  <tituloPantalla>DETALLE DE LA LIQUIDACIÓN</tituloPantalla>
  <annioLiquidacion>{A}</annioLiquidacion><mesLiquidacion>{M}</mesLiquidacion>
  <razonSocialEmp>OTRA EMPRESA INVENTADA SA</razonSocialEmp>
  <ccc>08000000000</ccc>
  <tipoLiq>L00</tipoLiq>
  <descTipoLiq>NORMAL</descTipoLiq>
  <fechHoraCalc>10/03/2026</fechHoraCalc>
  <fprAutorizada></fprAutorizada><fprCalculada>{A}{MM}</fprCalculada>
  <CrtrEnCalcAcumTrabajador><desDatCal>CONTINGENCIAS COMUNES</desDatCal><baseCalValor>1.800,00</baseCalValor><cuotaCalValPatronal>424,80</cuotaCalValPatronal><cuotaCalValObrera>84,60</cuotaCalValObrera><cuotaCalVaTotal>509,40</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>1º TRAMO COTIZACION ADIC. SOLIDARIDAD</desDatCal><baseCalValor>0,00</baseCalValor><cuotaCalValPatronal>0,00</cuotaCalValPatronal><cuotaCalValObrera>0,00</cuotaCalValObrera><cuotaCalVaTotal>0,00</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
  <CrtrEnCalcAcumTrabajador><desDatCal>LIQUIDO DE TOTALES</desDatCal><baseCalValor></baseCalValor><cuotaCalValPatronal>424,80</cuotaCalValPatronal><cuotaCalValObrera>84,60</cuotaCalValObrera><cuotaCalVaTotal>509,40</cuotaCalVaTotal></CrtrEnCalcAcumTrabajador>
 </CrtrEnSalidaNCBN2211>
</SPM>
</script>
</body></html>"##,
        "prosa_error.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Consulta de cálculos de cuotas para trabajador</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}E</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <MENSAJES>
  <MENSAJE><TIPO>AVISO</TIPO><TEXTO><![CDATA[Revise los datos]]></TEXTO></MENSAJE>
  <MENSAJE><TIPO>ERROR</TIPO>
   <TEXTO><![CDATA[  {CODIGO}   {TEXTO}
   ]]></TEXTO></MENSAJE>
 </MENSAJES>
 <DATOS>
  <tituloPantalla>CONSULTA DE CÁLCULOS DE CUOTAS PARA TRABAJADOR</tituloPantalla>
  <MESLIQUIDACION>{MM}</MESLIQUIDACION><ANNIOLIQUIDACION>{A}</ANNIOLIQUIDACION>
 </DATOS>
</SPM>
</script>
</body></html>"##,
        "prosa_formulario.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Consulta de cálculos de cuotas para trabajador</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <DATOS>
  <tituloPantalla>CONSULTA DE CÁLCULOS DE CUOTAS PARA TRABAJADOR</tituloPantalla>
  <MESLIQUIDACION></MESLIQUIDACION><ANNIOLIQUIDACION></ANNIOLIQUIDACION>
 </DATOS>
</SPM>
</script>
<div id="app"></div>
</body></html>"##,
        "prosa_lista.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Liquidaciones</title></head>
<body>
<script id="xml" type="text/plain"><?xml version="1.0" encoding="UTF-8"?>
<SPM>
 <SPM.TICKET id="SPM.TICKET">{TICKET}L</SPM.TICKET>
 <SPM.IDSESSION id="SPM.IDSESSION">;jsessionid=SES0001</SPM.IDSESSION>
 <MENSAJES><MENSAJE><TIPO>AVISO</TIPO><TEXTO><![CDATA[Consulta realizada]]></TEXTO></MENSAJE></MENSAJES>
 <DATOS>
  <tituloPantalla>LIQUIDACIONES PRESENTADAS PARA {M}/{A}</tituloPantalla>
  <CrtrEnLiquidacionesTrabajador><posicion>1</posicion><tipoLiq>L00</tipoLiq><perSolicitud>{A}{MM}</perSolicitud><periodoControl></periodoControl></CrtrEnLiquidacionesTrabajador>
  <CrtrEnLiquidacionesTrabajador><posicion>2</posicion><tipoLiq>L03</tipoLiq><perSolicitud>202606</perSolicitud><periodoControl>202605</periodoControl></CrtrEnLiquidacionesTrabajador>
 </DATOS>
</SPM>
</script>
</body></html>"##,
        "prosa_saml.html" => r##"<!DOCTYPE html>
<html><head><title>Sede Electrónica</title></head>
<body onload="document.forms[0].submit()">
<form method="post" action="https://idp.seg-social.es/PGIS/Login">
<input type="hidden" name="SAMLRequest" value="U0VERS1SRVFVRVNU"/>
</form></body></html>"##,
        "prosa_saml_respuesta.html" => r##"<!DOCTYPE html>
<html><head><title>Sede Electrónica</title></head>
<body onload="document.forms[0].submit()">
<form method="post" action="https://sp.seg-social.es/ProsaInternet/SAML2/POST">
<input type="hidden" name="SAMLResponse" value="U0VERS1SRVNQT05TRQ=="/>
</form></body></html>"##,
        "recibos_1.html" => r##"<!DOCTYPE html>
<html lang="es">
<head><meta charset="UTF-8"><title>Historial de recibos - Importass</title>
<base href="https://portal.seg-social.gob.es/wps/myportal/importass/importass/tramites/historialRecibosRETA/!ut/p/z1/RECIBOS1/">
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body>
<div class="quota-history">
 <div class="quota-history__group">
  <div class="quota-history__year"><h2 class="ss-title">2026</h2></div>
  <div class="quota-history__month"><h3 class="ss-subtitle">Octubre</h3></div>
  <div class="quota-history__item">
    <a class="quota-history__item-title" href="#r1">Cuota octubre 2026</a>
    <div class="quota-history__item-amount">310,00 €</div>
    <div class="quota-history__breakdown">
      <div class="row"><div class="quota-history__breakdown-amount">263,50 €</div>
      <p class="encabezadoLista">Contingencias Comunes</p></div>
      <div class="row"><div class="quota-history__breakdown-amount">40,30 €</div>
      <p class="encabezadoLista">Contingencias Profesionales</p></div>
      <div class="row"><div class="quota-history__breakdown-amount">2,90 €</div>
      <p class="encabezadoLista">Cese de Actividad</p></div>
      <div class="row"><div class="quota-history__breakdown-amount">1,00 €</div>
      <p class="encabezadoLista">Formación Profesional</p></div>
      <div class="row"><div class="quota-history__breakdown-amount">2,30 €</div>
      <p class="encabezadoLista"> MEI - Mecanismo Equidad Intergeneracional </p></div>
      <div class="row"><div class="quota-history__breakdown-amount">0,00 €</div>
      <p class="encabezadoLista">Otros conceptos</p></div>
      <p>Fecha de cargo en cuenta</p>
      <p>30/10/2026</p>
    </div>
  </div>
 </div>
 <div class="quota-history__group">
  <div class="quota-history__month"><h3 class="ss-subtitle">Septiembre</h3></div>
  <div class="quota-history__item">
    <a class="quota-history__item-title" href="#r2">Cuota septiembre 2026</a>
    <div class="quota-history__item-amount">1.310,00 €</div>
    <div class="quota-history__breakdown">
      <div class="row"><div class="quota-history__breakdown-amount">1.263,50 €</div>
      <p class="encabezadoLista">Contingencias Comunes</p></div>
      <div class="row"><div class="quota-history__breakdown-amount">5,00 €</div>
      <p class="encabezadoLista">Formaci&oacute;n Profesional</p></div>
      <p>Fecha de cargo en cuenta</p>
      <p>30 de septiembre de 2026</p>
    </div>
  </div>
  <div class="quota-history__item">
    <a class="quota-history__item-title" href="#r3">Regularización cuotas año 2024</a>
    <div class="quota-history__item-amount">123,45 €</div>
    <p>Fecha de cargo en cuenta</p> <p>15/09/2026</p>
  </div>
 </div>
</div>
<nav class="ss-pagination">
  <a href="p0/IZ7_HIST=CZ6_PAGINA!0/" class="ss-pagination__prev">Anterior</a>
  <a href="p0/IZ7_HIST=CZ6_PAGINA!2/?pagina=2&amp;tam=10#arriba" class="ss-pagination__next" aria-label="Siguiente">Siguiente</a>
</nav>
</body></html>"##,
        "recibos_2.html" => r##"<!DOCTYPE html>
<html lang="es">
<head><meta charset="UTF-8"><title>Historial de recibos - Importass</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body>
<div class="quota-history">
 <div class="quota-history__group">
  <div class="quota-history__year"><h2>
     2025
  </h2></div>
  <div class="quota-history__month"><h3>Diciembre</h3></div>
  <div class="quota-history__item">
    <a class="quota-history__item-title" href="#r4">Cuota diciembre 2025</a>
    <div class="quota-history__item-amount">300,00 €</div>
    <div class="quota-history__breakdown">
      <div class="row"><div class="quota-history__breakdown-amount">255,00 €</div>
      <p class="encabezadoLista">Contingencias Comunes</p></div>
      <p>Fecha de cargo en cuenta</p>
      <p>31/12/2025</p>
    </div>
  </div>
 </div>
</div>
<nav class="ss-pagination"><a href="#" class="ss-pagination__next">Siguiente</a></nav>
</body></html>"##,
        "regularizacion_2021.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Regularización</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body>
<h1>Regularización de cuotas para el año 2021</h1>
<dl><dt>Rendimiento neto anual</dt><dd>9.000,00 €</dd></dl>
<div><p>Resultado de la regularización</p><p>50,00 €</p></div>
<p>Te devolveremos la diferencia en tu cuenta.</p>
</body></html>"##,
        "regularizacion_2024.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Regularización</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body>
<h1>Regularizaci&oacute;n de cuotas para el a&ntilde;o 2024</h1>
<p>Tu base de cotización para el año 2024 es de: <strong>1.234,56 €</strong></p>
<dl><dt>Rendimiento neto anual</dt><dd>20.000,00 €</dd>
<dt>Rendimiento neto mensual</dt><dd>1.666,67 €</dd></dl>
<p>Promedio mensual de tus bases de cotización provisionales: 1.000,00 €</p>
<div><p>Resultado de la regularización</p><p>123,45 €</p></div>
<p>Según tus rendimientos, debes ingresar la diferencia.</p>
<p class="ok">Ingreso ya realizado</p>
</body></html>"##,
        "regularizaciones.html" => r##"<!DOCTYPE html>
<html lang="es"><head><meta charset="UTF-8"><title>Regularización de cuotas de años anteriores</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"true"}};</script></head>
<body>
<ul class="ss-list">
 <li><span>TRABAJADORES AUTÓNOMOS</span> <span>2024</span> <strong>A ingresar: 123,45 €</strong></li>
 <li><span>TRABAJADORES AUTÓNOMOS</span> <span>2023</span> <strong>Sin diferencias</strong></li>
 <li><span>TRABAJADORES AUT&Oacute;NOMOS</span> <span>2022</span> <strong>A devolver: 1.050,00&nbsp;€</strong></li>
</ul>
</body></html>"##,
        "sesion_caducada.html" => r##"<!DOCTYPE html>
<html lang="es"><head><title>Importass</title>
<script>var ibmCfg = {"userConfig":{"authenticated":"false"}};</script></head>
<body onload="document.forms[0].submit()">
<form method="post" action="https://idp.seg-social.es/PGIS/Login">
<input type="hidden" name="SAMLRequest" value="UFJVRUJBLVJFUVVFU1Q=">
<input type="hidden" name="RelayState" value="importass">
<noscript><input type="submit" value="Continuar"></noscript>
</form></body></html>"##,
        _ => panic!("no hay página de prueba {nombre}"),
    }
    .to_string();
    for (k, v) in cambios {
        t = t.replace(&format!("{{{k}}}"), v);
    }
    t
}

/// Lo que tiene que salir de las páginas de prueba.
pub fn esperado() -> &'static Value {
    static E: OnceLock<Value> = OnceLock::new();
    E.get_or_init(|| serde_json::from_str(include_str!("../../pruebas/tgss_esperado.json")).unwrap())
}

/// Un trozo de lo esperado (o de lo guardado), leído con los tipos.
pub fn de<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}

mod falsa {
    //! Una Seguridad Social de mentira (Importass, la pasarela idp/ipce y la Sede), para probar los flujos sin red. Es
    //! la misma que la de `referencia.py`: con las mismas peticiones contesta lo mismo.
    use super::pagina;
    use crate::red::cliente::Respuesta;
    use crate::tgss::sede::{FRONTEND, SEDE_CUOTAS};
    use crate::tgss::{IMPORTASS, Red};
    use std::cell::RefCell;

    const PORTAL: &str = "https://portal.seg-social.gob.es/wps/myportal/importass/importass/";
    const LOGIN: &str = "https://idp.seg-social.es/PGIS/Login";

    /// Para cambiar lo que contesta en una prueba: si devuelve algo, se contesta eso.
    pub type Cambio = Box<dyn Fn(&str, Option<&[(&str, &str)]>) -> Option<Result<Respuesta, String>>>;

    #[derive(Default)]
    struct Estado {
        importass: bool,
        idp: bool,
        sede: bool,
        caducar: bool,
        asalariado: bool,
        origen_sede: bool,
        servicios: u32,
        mes: String,
    }

    #[derive(Default)]
    pub struct Falsa {
        e: RefCell<Estado>,
        /// Las peticiones, como las apunta `referencia.py`: «GET url» o «POST url k=v&k=v».
        pub log: RefCell<Vec<String>>,
        pub cambio: Option<Cambio>,
    }

    impl Falsa {
        pub fn new(dentro: bool, caducar: bool, asalariado: bool) -> Falsa {
            let e = Estado { importass: dentro, idp: dentro, caducar, asalariado, ..Default::default() };
            Falsa { e: RefCell::new(e), ..Default::default() }
        }

        pub fn con(mut self, cambio: Cambio) -> Falsa {
            self.cambio = Some(cambio);
            self
        }

        pub fn peticiones(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    pub fn respuesta(url: &str, codigo: u16, texto: &str) -> Respuesta {
        Respuesta {
            url: url.into(),
            codigo,
            cabeceras: vec![("Content-Type".into(), "text/html; charset=UTF-8".into())],
            cuerpo: texto.as_bytes().to_vec(),
        }
    }

    fn prosa(nombre: &str, ticket: &str, ym: &str, mas: &[(&str, &str)]) -> String {
        let m = ym[5..7].parse::<u32>().unwrap().to_string();
        let mut c = vec![("TICKET", ticket), ("A", &ym[..4]), ("MM", &ym[5..7]), ("M", m.as_str())];
        c.extend_from_slice(mas);
        pagina(nombre, &c)
    }

    impl Red for Falsa {
        fn traer(&self, url: &str, campos: Option<&[(&str, &str)]>, _referer: Option<&str>) -> Result<Respuesta, String> {
            let linea = match campos {
                Some(c) => format!("POST {url} {}", c.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")),
                None => format!("GET {url}"),
            };
            self.log.borrow_mut().push(linea);
            if let Some(r) = self.cambio.as_ref().and_then(|f| f(url, campos)) {
                return r;
            }
            let (u, codigo, texto) = self.responder(url, campos);
            Ok(respuesta(&u, codigo, &texto))
        }
    }

    impl Falsa {
        fn vuelta(&self) -> (String, u16, String) {
            let n = if self.e.borrow().origen_sede { "prosa_saml_respuesta.html" } else { "login_respuesta.html" };
            (LOGIN.into(), 200, pagina(n, &[]))
        }

        fn formulario(&self) -> (String, u16, String) {
            let mut e = self.e.borrow_mut();
            e.servicios += 1;
            (format!("{FRONTEND};jsessionid=SES0001"), 200, pagina("prosa_formulario.html", &[("TICKET", &format!("T{}", e.servicios))]))
        }

        fn personal(&self) -> String {
            pagina(if self.e.borrow().asalariado { "personal_asalariado.html" } else { "personal.html" }, &[])
        }

        fn responder(&self, url: &str, campos: Option<&[(&str, &str)]>) -> (String, u16, String) {
            let campo = |k: &str| campos.and_then(|c| c.iter().find(|(n, _)| *n == k)).map(|(_, v)| v.to_string()).unwrap_or_default();
            let ok = |n: &str| (url.to_string(), 200, pagina(n, &[]));
            match campos {
                None => {
                    if url == IMPORTASS {
                        let dentro = self.e.borrow().importass;
                        return if dentro { (url.into(), 200, self.personal()) } else { ok("login_saml.html") };
                    }
                    if url.contains("ACC_PERFIL_AUTONOMO") {
                        return ok("perfil.html");
                    }
                    if url == format!("{PORTAL}tramites/historialRecibosRETA") {
                        let mut e = self.e.borrow_mut();
                        if e.caducar {
                            e.caducar = false;
                            e.importass = false;
                            return ok("sesion_caducada.html");
                        }
                        return ok("recibos_1.html");
                    }
                    if url.contains("CZ6_PAGINA!2") {
                        return ok("recibos_2.html");
                    }
                    if url == format!("{PORTAL}tramites/regularizacionCuotasAnyosAnteriores") {
                        return ok("regularizaciones.html");
                    }
                    for anio in ["2024", "2021"] {
                        if url.contains(&format!("ACC_REGULARIZACION_AUTONOMO={anio}")) {
                            return ok(&format!("regularizacion_{anio}.html"));
                        }
                    }
                    if url == SEDE_CUOTAS {
                        let dentro = self.e.borrow().sede;
                        return if dentro { self.formulario() } else { ok("prosa_saml.html") };
                    }
                    if url.contains("CANCELAPP") {
                        return (url.into(), 200, "<html></html>".into());
                    }
                    if let Some(q) = url.split_once('?').map(|(_, q)| q).filter(|_| url.contains("LIQUIDACIONSEL=")) {
                        let v = |k: &str| q.split('&').find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
                        let n = if v("LIQUIDACIONSEL") == "1" { "prosa_detalle_l00.html" } else { "prosa_detalle_l03.html" };
                        let mes = self.e.borrow().mes.clone();
                        return (url.into(), 200, prosa(n, &v("ARQ.SPM.TICKET"), &mes, &[]));
                    }
                }
                Some(_) => {
                    if url == LOGIN {
                        let idp = {
                            let mut e = self.e.borrow_mut();
                            e.origen_sede = campo("SAMLRequest") == "U0VERS1SRVFVRVNU";
                            e.idp
                        };
                        return if idp { self.vuelta() } else { ok("login_seleccion.html") };
                    }
                    if url == format!("{LOGIN}?seleccion=IPCE") {
                        let sede = {
                            let mut e = self.e.borrow_mut();
                            e.idp = true;
                            e.origen_sede
                        };
                        return if sede { self.vuelta() } else { ("https://idp.seg-social.es/PGIS/Ipce".into(), 200, pagina("login_formulario.html", &[])) };
                    }
                    if url == "https://idp.seg-social.es/PGIS/LoginSubmit;jsessionid=XYZ" {
                        return self.vuelta();
                    }
                    if url == "https://portal.seg-social.gob.es/wps/saml/acs" {
                        self.e.borrow_mut().importass = true;
                        return (IMPORTASS.into(), 200, self.personal());
                    }
                    if url == "https://sp.seg-social.es/ProsaInternet/SAML2/POST" {
                        self.e.borrow_mut().sede = true;
                        return self.formulario();
                    }
                    if url == format!("{FRONTEND};jsessionid=SES0001") {
                        let ym = format!("{}-{}", campo("ANNIOLIQUIDACION"), campo("MESLIQUIDACION"));
                        self.e.borrow_mut().mes = ym.clone();
                        let t = campo("ARQ.SPM.TICKET");
                        let texto = match ym.as_str() {
                            "2026-02" => prosa("prosa_directa.html", &t, &ym, &[]),
                            "2026-03" => prosa("prosa_lista.html", &t, &ym, &[]),
                            "2026-09" => prosa("prosa_error.html", &t, &ym, &[("CODIGO", "7008"), ("TEXTO", "Periodo liquidación no válido")]),
                            _ => prosa("prosa_error.html", &t, &ym, &[("CODIGO", "7101"), ("TEXTO", "No existe liquidación presentada para el periodo solicitado")]),
                        };
                        return (url.into(), 200, texto);
                    }
                }
            }
            (url.into(), 404, "<html><head><title>No encontrada</title></head></html>".into())
        }
    }
}

mod flujos {
    //! Entrar, consultar y las nóminas con la Seguridad Social de mentira: lo que se trae (como lo esperado) y las
    //! peticiones; lo que se guarda; y los fallos, con mensajes claros.
    use super::falsa::{Falsa, respuesta};
    use super::{de, esperado};
    use crate::dominio::tgss::{Meses, Resultado, Tgss};
    use crate::tgss::{self, IMPORTASS, SeguridadSocial, sede};
    use serde_json::Value;

    fn entrar(f: Falsa) -> SeguridadSocial<Falsa> {
        SeguridadSocial::entrar_con(f).unwrap()
    }

    /// Lo consultado y, aparte, cuándo (que es ahora).
    fn sin_consultado(mut t: Tgss) -> (Tgss, String) {
        let cuando = std::mem::take(&mut t.consultado);
        (t, cuando)
    }

    #[test]
    fn consultar() {
        // Sin sesión al empezar y con la sesión caducada a mitad: entra con el certificado las dos veces.
        let ss = entrar(Falsa::new(false, true, false));
        assert!(ss.es_autonomo());
        let (t, cuando) = sin_consultado(ss.autonomo().unwrap());
        assert_eq!(t, de::<Tgss>(&esperado()["consultar"]["resultado"]));
        assert_eq!(cuando.len(), 19, "{cuando}");
        assert_eq!(cuando.as_bytes()[10], b'T');
        // Las peticiones de lo esperado son las de la app web, que miraba tu área personal antes de entrar (la 0) y
        // otra vez después (la 6). Aquí se entra primero y se aprovecha la página a la que se llega: esas dos sobran.
        let mut esperadas = json_a_lista(&esperado()["consultar"]["peticiones"]);
        for i in [6, 0] {
            assert_eq!(esperadas.remove(i), format!("GET {IMPORTASS}"));
        }
        assert_eq!(ss.red.peticiones(), esperadas);
        // Cada cosa por separado, lo mismo.
        assert_eq!((ss.recibos().unwrap(), ss.regularizaciones().unwrap()), (t.recibos, t.regularizaciones));

        let ss = entrar(Falsa::new(true, false, true));
        assert!(!ss.es_autonomo());
        let (t, _) = sin_consultado(ss.autonomo().unwrap());
        assert_eq!(t, de::<Tgss>(&esperado()["consultar_asalariado"]["resultado"]));
        // Sin ser autónomo, lo de autónomo no se pregunta.
        assert_eq!((ss.perfil().unwrap(), ss.recibos().unwrap().len(), ss.regularizaciones().unwrap().len()), (Default::default(), 0, 0));
        assert_eq!(ss.red.peticiones(), json_a_lista(&esperado()["consultar_asalariado"]["peticiones"]));
    }

    fn json_a_lista(v: &Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn nominas() {
        let f = Falsa::new(false, false, false);
        let pedidos: Vec<String> = ["2026-01", "2026-02", "2026-13", "2026-03", "x", "2026-09"].map(String::from).to_vec();
        let (meses, aun_no) = sede::nominas(&f, &pedidos).unwrap();
        let e = &esperado()["nomina"]["resultado"];
        assert_eq!(meses, de::<Meses>(&e["meses"]));
        assert_eq!(aun_no, de::<Vec<String>>(&e["aunNo"]));
        assert_eq!(f.peticiones(), json_a_lista(&esperado()["nomina"]["peticiones"]));
        // Sin meses no se pide nada.
        let f = Falsa::new(false, false, false);
        assert_eq!(sede::nominas(&f, &["2026-13".into()]).unwrap(), Default::default());
        assert!(f.peticiones().is_empty());
    }

    /// Lo traído, como lo junta `sincronizar`: se guarda en `datos.json` y se vuelve a leer igual, con lo que se ve
    /// en las páginas.
    #[test]
    fn lo_que_se_guarda() {
        let ss = entrar(Falsa::new(false, false, false));
        let mut t = ss.autonomo().unwrap();
        let cuando = t.consultado.clone();
        let (meses, aun_no) = ss.nominas(&["2026-01".into(), "2026-02".into(), "2026-03".into(), "2026-09".into()]).unwrap();
        t.nomina.consultado = meses.keys().chain(&aun_no).map(|m| (m.clone(), cuando.clone())).collect();
        t.nomina.meses = meses;
        let guardado = serde_json::to_value(&t).unwrap();
        assert_eq!(de::<Tgss>(&guardado), t);
        // En `datos.json`, el resultado de una regularización como siempre: «ingresar», «sin», «devolver».
        let resultados: Vec<&Value> = (0..4).map(|i| &guardado["regularizaciones"][i]["resultado"]).collect();
        assert_eq!(resultados, ["ingresar", "sin", "devolver", "devolver"]);

        assert!(t.reta);
        assert_eq!((t.alta.fecha.as_str(), t.alta.cotiza_desde.as_str(), t.alta.baja.as_str()), ("2021-03-01", "2021-03-01", ""));
        assert_eq!(t.actividades.len(), 2);
        assert_eq!((t.cotizacion.rendimientos_previstos, t.cotizacion.base, t.cotizacion.base_proxima), (Some(1700.0), Some(1000.0), Some(1100.5)));
        assert_eq!((t.cotizacion.base_proxima_desde.as_str(), t.cotizacion.ultima_cuota, t.cotizacion.ultima_cuota_mes.as_str()), ("2027-01-01", Some(310.0), "octubre"));
        assert_eq!(t.recibos.len(), 4);
        let r = &t.recibos[0];
        assert_eq!((r.periodo.as_str(), r.emision.as_str(), r.cargo.as_str(), r.importe), ("2026-10", "2026-10", "2026-10-30", 310.0));
        let k = r.conceptos;
        assert_eq!((k.cc, k.cp, k.cese, k.fp, k.mei), (263.5, 40.3, 2.9, 1.0, 2.3));
        let anios: Vec<i32> = t.regularizaciones.iter().map(|r| r.anio).collect();
        assert_eq!(anios, [2024, 2023, 2022, 2021]);
        let r = &t.regularizaciones[0];
        assert_eq!((r.resultado, r.importe, r.base, r.pagado), (Resultado::Ingresar, 123.45, Some(1234.56), Some(true)));
        assert_eq!((t.regularizaciones[3].resultado, t.regularizaciones[3].pagado), (Resultado::Devolver, Some(false)));
        let n = &t.nomina;
        assert_eq!(n.meses.keys().collect::<Vec<_>>(), ["2026-01", "2026-02", "2026-03"]);
        assert!(n.meses["2026-01"].is_empty());
        assert_eq!(n.consultado.len(), 4, "también el que aún no da (2026-09)");
        let l = &n.meses["2026-03"];
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].tipo.as_str(), l[0].descripcion.as_str(), l[0].calculada.as_str()), ("L00", "Normal", "2026-04-15"));
        assert_eq!((l[0].cuota_empresa, l[0].cuota_trabajador, l[0].conceptos.len()), (783.25, 160.75, 5));
        assert_eq!((l[1].tipo.as_str(), l[1].cuota_empresa, l[1].cuota_trabajador), ("L03", 30.13, 6.33));
        assert_eq!(l[1].empresa, "EMPRESA INVENTADA & ASOCIADOS, S.L.");
        assert_eq!(n.meses["2026-02"][0].conceptos[0].base, 1800.0);
    }

    // ---------------------------------------------------------------- fallos

    fn falla_al_entrar(f: Falsa) -> String {
        let e = SeguridadSocial::entrar_con(f).err().expect("no tenía que poder entrar");
        assert!(e.starts_with("No se ha podido entrar en la Seguridad Social"), "{e}");
        e
    }

    #[test]
    fn certificado_no_aceptado() {
        // IPCE contesta 403.
        let f = Falsa::new(false, false, false).con(Box::new(|url, _| {
            url.contains("seleccion=IPCE").then(|| Ok(respuesta("https://ipce.seg-social.es/PGIS/Ipce", 403, &super::pagina("ipce_403.html", &[]))))
        }));
        assert_eq!(falla_al_entrar(f), "No se ha podido entrar en la Seguridad Social: no ha aceptado tu certificado.");
        // IPCE corta la conexión segura.
        let f = Falsa::new(false, false, false).con(Box::new(|url, _| {
            url.contains("seleccion=IPCE").then(|| Err("Fallo en la conexión segura con ipce.seg-social.es (alerta).".to_string()))
        }));
        assert!(falla_al_entrar(f).contains("no ha aceptado tu certificado"));
    }

    #[test]
    fn paginas_inesperadas() {
        let f = Falsa::new(false, false, false).con(Box::new(|url, _| {
            url.contains("seleccion=IPCE").then(|| Ok(respuesta("https://idp.seg-social.es/PGIS/Error", 200, &super::pagina("mantenimiento.html", &[]))))
        }));
        assert_eq!(falla_al_entrar(f), "No se ha podido entrar en la Seguridad Social: no se sabe seguir desde idp.seg-social.es (Servicio en mantenimiento).");
        // Formularios SAML sin fin.
        let f = Falsa::new(false, false, false).con(Box::new(|url, _| Some(Ok(respuesta(url, 200, &super::pagina("login_saml.html", &[]))))));
        assert_eq!(falla_al_entrar(f), "No se ha podido entrar en la Seguridad Social: el inicio de sesión no termina.");
        // Caída.
        let f = Falsa::new(false, false, false).con(Box::new(|url, _| Some(Ok(respuesta(url, 503, "<html><title>Error</title></html>")))));
        let e = SeguridadSocial::entrar_con(f).err().unwrap();
        assert_eq!(e, "La Seguridad Social no está disponible ahora mismo (portal.seg-social.gob.es contesta 503): prueba más tarde.");
        // Sin internet: el error de la red, tal cual.
        let f = Falsa::new(false, false, false).con(Box::new(|_, _| Some(Err("No se encuentra portal.seg-social.gob.es: ¿hay conexión a internet?".to_string()))));
        assert_eq!(SeguridadSocial::entrar_con(f).err().unwrap(), "No se encuentra portal.seg-social.gob.es: ¿hay conexión a internet?");
        // La sesión no se mantiene: entra, pero la página sigue sin estar dentro.
        let f = Falsa::new(true, false, false).con(Box::new(|url, _| {
            url.ends_with("historialRecibosRETA").then(|| Ok(respuesta(url, 200, "<html><title>Importass</title></html>")))
        }));
        assert_eq!(entrar(f).autonomo().unwrap_err(), "No se ha podido abrir tu área personal de Importass: la sesión se cierra sola.");
        // Lo pedido queda en el historial (las otras pruebas también apuntan: solo se mira que hay algo y el límite).
        let h = tgss::historial();
        assert!(!h.is_empty() && h.len() <= 60 && h.iter().all(|l| l.starts_with("GET ") || l.starts_with("POST ")), "{h:?}");
    }

    #[test]
    fn errores_de_la_sede_cierran_el_servicio() {
        let f = Falsa::new(true, false, false).con(Box::new(|url, campos| {
            let mes = campos?.iter().find(|(k, _)| *k == "MESLIQUIDACION")?.1;
            let t = super::pagina("prosa_error.html", &[("TICKET", "T1"), ("A", "2026"), ("MM", mes), ("CODIGO", "9999"), ("TEXTO", "Servicio no disponible")]);
            (mes == "05").then(|| Ok(respuesta(url, 200, &t)))
        }));
        let e = sede::nominas(&f, &["2026-02".into(), "2026-05".into()]).unwrap_err();
        assert_eq!(e, "La Seguridad Social dice: 9999 Servicio no disponible");
        let p = f.peticiones();
        assert!(p.last().unwrap().contains("ARQ.SPM.ACTION=CANCELAPP"), "{p:?}");
        // Otro mes del pedido.
        let f = Falsa::new(true, false, false).con(Box::new(|url, campos| {
            campos?;
            let t = super::pagina("prosa_error.html", &[("TICKET", "T1"), ("A", "2025"), ("MM", "01"), ("CODIGO", "7101"), ("TEXTO", "No existe")]);
            url.contains("OnlineAccessUtf8").then(|| Ok(respuesta(url, 200, &t)))
        }));
        let e = sede::nominas(&f, &["2026-02".into()]).unwrap_err();
        assert_eq!(e, "La Sede de la Seguridad Social ha devuelto otro mes (2025-01) al pedir 2026-02.");
        // La Sede contesta otra cosa.
        let f = Falsa::new(true, false, false).con(Box::new(|url, _| url.contains("OnlineAccess?").then(|| Ok(respuesta(url, 200, "<title>Aviso</title><p>Sede cerrada</p>")))));
        assert_eq!(sede::nominas(&f, &["2026-02".into()]).unwrap_err(), "La Sede de la Seguridad Social no ha contestado como se esperaba (Aviso).");
    }
}

mod lectores {
    //! Cada lector, con las páginas de prueba, contra lo esperado de esas mismas páginas (`esperado.json`).
    use super::{de, esperado, pagina};
    use crate::dominio::tgss::{Liquidacion, Recibo, Regularizacion};
    use crate::tgss::html::{self, Formulario};
    use crate::tgss::importass::{self, Detalle, Perfil};
    use crate::tgss::sede::{self, Prosa};
    use serde_json::{Value, json};

    #[test]
    fn perfil() {
        let p = importass::leer_perfil(&pagina("perfil.html", &[]));
        assert_eq!(p, de::<Perfil>(&esperado()["perfil"]));
        // Lo que se ve, por si acaso: la tarjeta rara (un `<dt>` sin `<dd>`) no da fecha de baja; el mes de la cuota
        // no sale del `<script>`.
        assert_eq!((p.alta.fecha.as_str(), p.alta.cotiza_desde.as_str(), p.alta.baja.as_str()), ("2021-03-01", "2021-03-01", ""));
        assert_eq!(p.cotizacion.ultima_cuota_mes, "octubre");
        assert_eq!(p.actividades.len(), 2);
        assert_eq!(p.actividades[0].cnae_texto, "Programación informática");
        assert_eq!((p.actividades[1].inicio.as_str(), p.actividades[1].tipo.as_str()), ("2023-06-15", "Socio de sociedad mercantil"));
        assert_eq!(p.mutua.entidad, "MUTUA INVENTADA Nº 0");
    }

    #[test]
    fn recibos_y_paginas() {
        for n in ["recibos_1", "recibos_2"] {
            let (r, sig) = importass::leer_recibos(&pagina(&format!("{n}.html"), &[]));
            assert_eq!(r, de::<Vec<Recibo>>(&esperado()[n][0]), "{n}");
            assert_eq!(sig.as_deref(), esperado()[n][1].as_str(), "{n}");
        }
        let (r, sig) = importass::leer_recibos(&pagina("recibos_1.html", &[]));
        assert_eq!(sig.as_deref(), Some("p0/IZ7_HIST=CZ6_PAGINA!2/?pagina=2&tam=10"));
        // «Formaci&oacute;n Profesional» sin desescapar no es un concepto: se queda a 0.
        assert_eq!((r[1].conceptos.cc, r[1].conceptos.fp), (1263.5, 0.0));
        assert_eq!(r[2].periodo, "2026-09");
    }

    #[test]
    fn regularizaciones() {
        let lista = importass::leer_regularizaciones(&pagina("regularizaciones.html", &[]));
        assert_eq!(lista, de::<Vec<Regularizacion>>(&esperado()["regularizaciones"]));
        for a in ["2024", "2021"] {
            let n = format!("regularizacion_{a}");
            assert_eq!(importass::leer_regularizacion(&pagina(&format!("{n}.html"), &[])), de::<Detalle>(&esperado()[&n]), "{n}");
        }
    }

    fn en_json(f: &[Formulario]) -> Value {
        f.iter().map(|f| json!({ "accion": f.accion, "metodo": f.metodo, "campos": f.campos, "botones": f.botones })).collect()
    }

    #[test]
    fn formularios() {
        let urls = [
            ("login_saml.html", crate::tgss::IMPORTASS),
            ("login_seleccion.html", "https://idp.seg-social.es/PGIS/Login"),
            ("login_formulario.html", "https://idp.seg-social.es/PGIS/Ipce"),
            ("sesion_caducada.html", "https://portal.seg-social.gob.es/wps/myportal/importass/importass/tramites/historialRecibosRETA"),
        ];
        for (n, url) in urls {
            assert_eq!(en_json(&html::formularios(url, &pagina(n, &[]))), esperado()["formularios"][n], "{n}");
        }
    }

    #[test]
    fn paginas_de_la_sede() {
        let casos: [(&str, &[(&str, &str)]); 6] = [
            ("prosa_formulario.html", &[]),
            ("prosa_lista.html", &[]),
            ("prosa_detalle_l00.html", &[]),
            ("prosa_detalle_l03.html", &[]),
            ("prosa_directa.html", &[]),
            ("prosa_error.html", &[("CODIGO", "7101"), ("TEXTO", "No existe liquidación")]),
        ];
        for (n, mas) in casos {
            let mut c = vec![("TICKET", "T9"), ("A", "2026"), ("MM", "03"), ("M", "3")];
            c.extend_from_slice(mas);
            let p = Prosa::de(&pagina(n, &c)).unwrap();
            let e = &esperado()["prosa"][n];
            assert_eq!(json!([p.ticket, p.sesion, p.errores, sede::mes_de(&p)]), json!([e["ticket"], e["sesion"], e["errores"], e["mes"]]), "{n}");
            assert_eq!(sede::leer_liquidacion(&p), de::<Liquidacion>(&e["liquidacion"]), "{n}");
        }
        let e = Prosa::de(&pagina("mantenimiento.html", &[])).unwrap_err();
        assert_eq!(e, "La Sede de la Seguridad Social no ha contestado como se esperaba (Servicio en mantenimiento).");
        assert!(Prosa::de("<p>SPM.TICKET sin etiqueta</p>").is_err());
    }

    #[test]
    fn fechas_importes_y_texto() {
        for (s, v) in esperado()["fechas"].as_object().unwrap() {
            assert_eq!(importass::fecha(s), v.as_str().unwrap(), "fecha de {s:?}");
        }
        for (s, v) in esperado()["importes"].as_object().unwrap() {
            assert_eq!(importass::importe(s), v.as_f64(), "importe de {s:?}");
        }
        for (s, v) in esperado()["limpio"].as_object().unwrap() {
            assert_eq!(html::limpio(s), v.as_str().unwrap(), "limpio de {s:?}");
        }
        for caso in esperado()["urljoin"].as_array().unwrap() {
            let (b, r, v) = (caso[0].as_str().unwrap(), caso[1].as_str().unwrap(), caso[2].as_str().unwrap());
            assert_eq!(html::unir(b, r), v, "unir({b:?}, {r:?})");
        }
    }

    #[test]
    fn utilidades() {
        assert_eq!(html::desescapar("a &amp;&amp b &lt;&gt; &quot;x&quot; &#39;&apos; &Aacute;&ordm;&euro;&hellip; &#x20AC; & &; &#; &#xZ"), "a && b <> \"x\" '' Áº€… € & &; &#; &#xZ");
        assert_eq!(html::titulo("<html><TITLE> Hola\n &amp; adiós </TITLE>"), "Hola & adiós");
        assert_eq!(html::donde("<p>Sin   título</p>"), "Sin título");
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
        assert_eq!(importass::enlaces(&pagina("personal.html", &[]), "ACC_REGULARIZACION_AUTONOMO").len(), 2);
    }
}

mod meses {
    //! Qué meses de nómina se piden (como `mesesPorMirar` y `montarNomina` de js/tgss.js), con datos inventados.
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
    fn anios_viejos_solo_si_nunca_se_miraron() {
        // 2023: un mes mirado → el año no se vuelve a mirar (como `montarNomina`), aunque le falten meses.
        // 2022: ningún mes en `meses` → sus meses sin mirar, sí.
        let (mut ms, mut c) = mirados(2020..=2026, "2026-10", HACE_1H);
        ms.retain(|m| !m.starts_with("2023-") || m == "2023-06");
        c.retain(|(m, _)| !m.starts_with("2023-") || m == "2023-06");
        ms.retain(|m| !m.starts_with("2022-"));
        c.retain(|(m, _)| !m.starts_with("2022-") || m == "2022-01");
        let r = meses_a_consultar(&con(&ms, &c), HOY);
        assert_eq!(r.len(), 11);
        assert_eq!((r[0].as_str(), r[10].as_str()), ("2022-12", "2022-02"));
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

mod red_de_verdad {
    //! Con internet: `cargo test --test red -- --ignored --nocapture`.
    use crate::red::certificado::Certificado;
    use crate::tgss::{self, SeguridadSocial};
    use std::time::{Duration, Instant};

    /// El certificado de prueba (autofirmado, no de la FNMT) no lo acepta la Seguridad Social: `entrar` tiene que
    /// fallar con un mensaje claro, sin colgarse. Enseña las peticiones (sin la parte de la consulta de las URL).
    #[test]
    #[ignore]
    fn entrar_con_certificado_de_prueba() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pruebas/prueba.p12");
        let cert = Certificado::abrir(&p, "prueba").unwrap();
        let t0 = Instant::now();
        let r = SeguridadSocial::entrar(&cert).err();
        for l in tgss::historial() {
            println!("{}", l.split(' ').map(|p| p.split('?').next().unwrap_or(p)).collect::<Vec<_>>().join(" "));
        }
        let e = r.expect("el certificado de prueba no lo tiene que aceptar");
        println!("→ {e} ({} s)", t0.elapsed().as_secs());
        assert!(e.starts_with("No se ha podido entrar en la Seguridad Social"), "{e}");
        assert!(t0.elapsed() < Duration::from_secs(240));
    }
}
