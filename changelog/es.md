# Novedades (español)

Traducción de CHANGELOG.md para la ventana «Novedades». Mismo formato, secciones
traducidas. Las versiones que falten aquí aparecen en inglés.

## 0.4.4 - 2026-11-28

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
