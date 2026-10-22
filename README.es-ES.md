

# love-box

- Una pantalla de e-tinta controlable a distancia utilizando la pantalla de e-tinta de 7.5" de waveshare, un Raspberry Pi Zero W y la base de datos en tiempo real de nivel gratuito de Firebase
- Un regalo diseñado para dejar notas/mensajes significativos para alguien que amas

# Estructura del Repositorio

## `/Api`

- Contiene una API sencilla de Flask para gestionar solicitudes de actualización del mensaje/imagen para la pantalla
- `message.py` funciona como una clase que representa los objetos de la base de datos
- `message_repository.py` actúa como un repositorio para recuperar y actualizar valores de la base de datos
- `api.py` gestiona solicitudes para actualizar o previsualizar mensajes/imagen desde la interfaz de usuario (en la siguiente sección)


---

## `/LoveBoxUi`

- Contiene una aplicación React para cambiar fácilmente la imagen de la pantalla 
- Apariencia:
![image](https://user-images.githubusercontent.com/47571939/167021667-e28bef4f-c37d-4e7e-863e-f9327ca73744.png)
- O en dispositivos móviles:

![image](https://user-images.githubusercontent.com/47571939/167021748-930d9bcb-1317-4bc5-859c-ef41181d8780.png)
- La interfaz permite: 
  - Ver la galería de imágenes, elegir una y subirla. Las imágenes se aleatorizan por defecto y pueden volverse a mezclar
  - Seleccionar un tamaño de fuente y subir un mensaje (el botón enviar se utiliza para la subida)
  - Seleccionar un tamaño de fuente y previsualizar dónde se realizarán los saltos de línea en el mensaje (con el botón de previsualización izquierdo)


---

## `/RpiUpdateService`

- Servicio de actualización alojado en el Raspberry Pi Zero W que controla la pantalla
- Script de Python que se ejecuta cada 10 minutos para verificar el contenido de la base de datos y actualizar la pantalla en consecuencia
- El script se ejecuta mediante un cronjob

### `/ShellScripts`

- `auto_update.sh`
  - Se ejecuta cada 24 horas mediante un cronjob para actualizar el Raspberry Pi con el código remoto
- `update_display.sh`
  - Se ejecuta cada 10 minutos mediante un cronjob para actualizar la pantalla con el contenido de la base de datos
- `install.sh`
  - Instala las dependencias necesarias para el Raspberry Pi

### `/src`
- `update_display.py` actualiza la pantalla en sí con el contenido almacenado en la base de datos
  - La hora almacenada en la base de datos se compara con la hora actual
  - Si la base de datos se actualizó por última vez en los últimos 10 minutos, entonces se actualiza la pantalla
- El script `format_images.py` permite colocar imágenes JPEG en la biblioteca `/pic` y luego las transforma a las dimensiones requeridas por la pantalla (800x480)
  - Estas imágenes también se guardan en el directorio `/public` para que la aplicación React las muestre en el sitio web
- `message.py` y `message_repository.py` son los mismos que los encontrados en la API
- `/lib`
  - Todas las bibliotecas de waveshare
        - Se toman directamente del [repositorio waveshare/e-paper](https://github.com/waveshare/e-Paper)
        - Contiene una biblioteca compartida en C y un ejemplo en Python para escribir en la pantalla de 7.5"
- `/pic`
  - Donde se almacenan y leen las imágenes
- `/font`
  - Almacena la fuente para la pantalla

# Entradas de Crontab

- `*/10 * * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/update_display.sh >> /home/pi/logs/update_display_log.txt 2>&1`
  - Se ejecuta cada 10 minutos
- `0 0 * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/auto_update.sh >> /home/pi/logs/git_log.txt 2>&1`
  - Se ejecuta una vez al día
- Tenga en cuenta que estas rutas son relativas y deberán ajustarse según el lugar donde se almacenen los archivos del repositorio en el Raspberry Pi

# Lista de Componentes

- [Waveshare E-Paper 7.5" Display](https://www.waveshare.com/7.5inch-e-paper-hat.htm)
- Marco de fotos simple de 7"x5.5"
- Raspberry Pi Zero W
- Carcasa impresa en 3D para alojar el Raspberry Pi y el Pi HAT de waveshare
- Fuente de alimentación USB para el Pi

# Resultados Finales
## Mostrando una Imagen
![image](https://user-images.githubusercontent.com/47571939/163594413-c6eb3808-4513-4128-835d-d810d2fb4636.png)
## Mostrando Texto
![image](https://user-images.githubusercontent.com/47571939/163594460-b78e4632-db6d-4be3-b008-e5df3094526b.png)
## El Cableado
![image](https://user-images.githubusercontent.com/47571939/163594511-85c280f8-d6a0-4ccc-a2cf-58c4714fc34f.png)
![image](https://user-images.githubusercontent.com/47571939/163594503-563f6b02-9094-4581-a29f-ac31b20f1bf2.png)
## La Carcasa Diseñada para el Pi + Waveshare Pi-HAT
![image](https://user-images.githubusercontent.com/47571939/163594571-d0f96c77-cdce-4f6a-8a29-8d045d3c6322.png)
![image](https://user-images.githubusercontent.com/47571939/163594608-162ef34d-f450-4de3-981d-98be82ceefe8.png)
![image](https://user-images.githubusercontent.com/47571939/163594590-a1441343-8cd4-471c-af65-96cdf93154f1.png)
