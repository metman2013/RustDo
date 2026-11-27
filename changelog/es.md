# Novedades (español)

Traducción de CHANGELOG.md para la ventana «Novedades». Mismo formato, secciones
traducidas. Las versiones que falten aquí aparecen en inglés.

## 0.4.9 - 2026-11-28

### Mejoras
- El asa ≡ para arrastrar tareas aparece ahora en teléfonos y tabletas con cualquier orden; al mover una tarea, la lista pasa sola a «Mi orden».
- Al añadir una tarea, su cuadro «Añadir una subtarea» se abre justo debajo (no arriba de la lista). Si añades otra tarea, el cuadro pasa a esa; ✖ lo cierra.

### Correcciones
- La página de una lista muestra ahora el mismo icono que en el menú (🏠 para Tareas, 👥 para listas compartidas, 📋 para tus listas, 📚 para Todas) en lugar de ☰, que parecía el botón del menú.

## 0.4.8 - 2026-11-27

### Correcciones
- La página de notas adhesivas ahora empieza como las demás: ☰ (en teléfonos), el título, y luego «+ Nueva nota» y un menú «Más» (copia de seguridad, restaurar, ordenar), todo en una línea.
- Teléfonos y tabletas: ahora las tareas se pueden arrastrar a tu propio orden. En «Mi orden», cada tarea tiene un asa ≡: arrástrala hacia arriba o hacia abajo (arrastrar en otro punto de la tarea sigue desplazando la lista).
- Teléfonos y tabletas: las listas también se pueden reordenar arrastrando el asa ≡ de cada lista en el menú.
- Las notas adhesivas tienen en teléfonos y tabletas una esquina más grande para cambiar su tamaño con el dedo.

## 0.4.7 - 2026-11-27

### Novedades
- Ordena las tareas a tu manera: arrastra una tarea hacia arriba o hacia abajo (o clic derecho o mantener pulsada › Subir, Bajar, Mover al principio). El orden cambia al nuevo «Mi orden», que se sincroniza con tus otros dispositivos y con las listas compartidas.
- Las notas adhesivas tienen en teléfonos y tabletas botones + / 🎨 / 🗑 más grandes y fáciles de tocar.
- RustDo para Linux en ARM64 (escritorios Raspberry Pi, portátiles ARM): .deb, .rpm, AppImage y .tar.gz.
- RustDo Hub para Linux sin Docker: descargas listas para PC (x86_64) y Raspberry Pi (arm64), y un script de instalación que configura el hub como servicio que arranca con el ordenador.
- RustDo para Linux ahora también viene como .rpm (Fedora, openSUSE) y como .tar.gz para cualquier distribución, con un script de instalación que añade RustDo al menú de aplicaciones.
- Configuración paso a paso de RustDo Hub en un NAS Synology (Container Manager), en la guía del hub y en «Cómo funcionan la sincronización y el uso compartido» de RustDo.

### Correcciones
- App web en teléfonos y tabletas Android: las letras podían verse irregulares, algunas cortadas o desplazadas (el chip gráfico dibujaba el texto con poca precisión). Ahora el texto se dibuja con precisión completa y se ve tan nítido como en la app de Android.
- Android: en Configuración › Fondo faltaba el botón «+ Imagen…», así que no se podían usar fotos propias. Ya está, y abre el selector de fotos de RustDo.
- El texto se lee mejor sobre las imágenes de fondo: las fechas, las indicaciones y otros textos tenues ya no se confunden con la imagen, tanto en el tema oscuro como en el claro.

## 0.4.6 - 2026-11-27

### Novedades
- Imágenes de fondo: elige una para toda la app en Configuración › Fondo y dale a cada lista la suya (clic derecho o mantener pulsada la lista › Fondo…). Elige entre los fondos incluidos o tu propia imagen (en Android, RustDo muestra tus fotos cuando lo permites). «Oscurecer el fondo» mantiene las tareas fáciles de leer.
- Los fondos se sincronizan con Sincronizar mis dispositivos, también tus propias imágenes: una imagen elegida en el PC aparece también en el teléfono. (Tu RustDo Hub necesita la versión 0.4.6 o posterior para llevar imágenes.)
- Las notas adhesivas también pueden tener una imagen: abre el menú 🎨 de una nota › Imagen y elige una incluida o una tuya. El color de la nota la tiñe para que el texto se lea bien.
- Obtener RustDo (en el menú ⚙) muestra todas las formas de conseguir RustDo: Microsoft Store, el instalador de Windows, Android, Linux, la app web y el Hub.
- Un botón «Únete a la comunidad de RustDo en Discord» al final de las listas, para ideas, errores y ayuda.
- RustDo Hub puede funcionar como app de Home Assistant: en la tienda de apps de Home Assistant, añade el repositorio de RustDo e instala RustDo Hub. Más en la guía del hub.

### Correcciones
- App web en teléfonos y tabletas: en algunas pantallas las letras se veían comprimidas. «Texto nítido» (activado por defecto, en el menú ⚙) ahora funciona también ahí, como en la app de Android.
- La imagen de Docker de RustDo Hub ahora funciona cuando su carpeta de datos pertenece a root (habitual en NAS y en Home Assistant).

## 0.4.5 - 2026-11-26

### Correcciones
- Android: después de ocultar el teclado en pantalla (con su tecla de ocultar, el gesto Atrás o al cambiar de aplicación), tocar un campo de texto no lo volvía a mostrar. Ahora tocar un campo siempre abre el teclado, y vuelve a aparecer al regresar a RustDo.
- «Añadir una tarea» muestra un botón Añadir en cuanto escribes, para guardar una tarea sin la tecla Intro del teclado.

## 0.4.4 - 2026-11-26

### Novedades
- Mostrar lista: una página grande y fácil de leer con las tareas de una lista y sus notas, por ejemplo una lista de medicamentos para enseñar a tu médico. Cópiala como texto para pegarla en un correo o un mensaje o, en un ordenador, imprímela o guárdala como PDF. Está junto al nombre de la lista o al hacer clic derecho (mantener pulsada) en una lista.

### Mejoras
- Teléfonos: un botón Atrás claro (una flecha en un botón redondeado) sustituye al pequeño «<» en el menú, la configuración, los detalles de la tarea y las notas rápidas.
- Android: el botón y el gesto Atrás ya funcionan en RustDo. Atrás cierra lo que esté abierto, vuelve a la lista y a Mi día, y desde Mi día sale de la aplicación.
- Reordena tus listas: arrastra una lista hacia arriba o hacia abajo en el menú o, en el teléfono, mantenla pulsada y elige Subir o Bajar. El nuevo orden se sincroniza con tus otros dispositivos.
- Las instalaciones nuevas en alemán, español o francés empiezan con el reloj de 24 horas, y el nombre de la lista predeterminada cambia cuando cambias de idioma.
- «Novedades» aparece ahora en tu idioma (alemán, español o francés).
- RustDo Hub guarda cada día una copia de sus datos (los últimos 14 días) en una carpeta backups junto a hub.json.

## 0.4.3 - 2026-11-26

### Novedades
- RustDo ahora también habla español y francés: Configuración › Idioma ofrece Español y Français junto a English y Deutsch, y Automático los elige cuando tu dispositivo los usa. Las fechas también se muestran al estilo local, como «Miércoles, 25 de noviembre» y «Mercredi 25 novembre».

## 0.4.2 - 2026-11-25

### Novedades
- Importa directamente desde Microsoft To Do: elige Importar de To Do en el menú y luego Iniciar sesión con Microsoft. RustDo lee todas tus listas, tareas, pasos, notas, fechas de vencimiento y recordatorios y te muestra lo que va a importar. Solo lee tus datos de To Do y nunca los cambia. Funciona en Windows, Linux, Android y la aplicación web, con cuentas personales y cuentas profesionales o educativas de Microsoft.
- Android ya puede importar desde Microsoft To Do (iniciando sesión).

### Correcciones
- Alemán: algunos mensajes largos que aún aparecían en inglés ahora están traducidos.

## 0.4.1 - 2026-11-25

### Novedades
- RustDo habla alemán: Configuración › Idioma ofrece Automático, English y Deutsch. Automático sigue el idioma de tu dispositivo.
- La compatibilidad con idiomas ya está integrada, para que puedan llegar más idiomas.

## 0.4.0 - 2026-11-25

### Novedades
- Sincronizar mis dispositivos: mantén tus listas, tareas, notas rápidas y ajustes (tema, reloj, orden) iguales en todos tus dispositivos, incluida la aplicación web, a través de tu propio RustDo Hub. Actívalo en Configuración, en Sincronizar mis dispositivos: tu primer dispositivo obtiene una clave de sincronización y los demás usan la misma dirección del hub y la misma clave. Cada dispositivo conserva sus propios avisos de recordatorio y la posición de sus notas rápidas.
- Las listas que compartes con la familia quedan separadas de tu propia sincronización: los demás solo ven la lista compartida, y tus otros dispositivos se unen a ella solos.
- Para Sincronizar mis dispositivos hace falta RustDo Hub 0.4.0 (actualiza la imagen de Docker o el hub de Windows). También permite que la aplicación web se conecte cuando el hub tiene una dirección https://.
- La guía del hub en Configuración ahora explica cómo sincronizar tus propios dispositivos además de compartir listas.
