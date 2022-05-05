# love-box

- A remotely controllable e-paper display using waveshare's 7.5" e-paper display, a Raspberry Pi Zero W, and Firebase's free-tier real-time database
- A gift meant for leaving meaningful notes/messages for someone you love

# Repository Structure

## `/Api`

- Contains a simple flask API to serve requests for updating the message/picture for the display
- `message.py` serves as a class  representing the database objects
- `message_repository.py` serves as a repository for fetching and updating database values
- `api.py` serves requests for updating or previewing messages/images from the UI (in the following section)


---

## `/LoveBoxUi`

- Contains a React application for easily changing the display picture 
- The look:
![image](https://user-images.githubusercontent.com/47571939/167021667-e28bef4f-c37d-4e7e-863e-f9327ca73744.png)
- Or on mobile:

![image](https://user-images.githubusercontent.com/47571939/167021748-930d9bcb-1317-4bc5-859c-ef41181d8780.png)
- The UI allows: 
  - Viewing the picture gallery, choose a picture and upload. Pictures are shuffled by default and can be reshuffled
  - Selecting a font size and uploading a message (send button used for uploading)
  - Selecting a font size and previewing where line breaks will be in message (with left preview button)


---

## `/RpiUpdateService`

- Update service hosted on the raspberry Pi Zero W which controls the display
- Python script which is run every 10 minutes to check database contents and update display accordingly
- Script is called using a cronjob

### `/ShellScripts`

- `auto_update.sh`
  - Called every 24 hours using a cronjob to update raspberry pi with remote code
- `update_display.sh`
  - Called every 10 minutes using a cronjob to update the display with database contents
- `install.sh`
  - Installs needed dependencies for the raspberry pi

### `/src`
- `update_display.py` updates the display itself with the contents within the database
  - The time stored within the database is compared with the current time
  - If the database was last updated in the previous 10 minutes, then the display is changed
- `format_images.py` script allows one to drop JPEG images into the `/pic` library, and then transforms them to the dimensions required by the display (800x480)
  - These images are also saved to the `/public` directory for the react app to be displayed on the website
- `message.py` and `message_repository.py` are the same as found in the API
- `/lib`
  - All waveshare libraries
        - These are taken right from the [waveshare/e-paper repo](https://github.com/waveshare/e-Paper)
        - Contains a c shared library and python example for writing to the 7.5" display
- `/pic`
  - Where pictures are stored and read from
- `/font`
  - Stores font for display

# Crontab Entries

- `*/10 * * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/update_display.sh >> /home/pi/logs/update_display_log.txt 2>&1`
  - Ran every 10 minutes
- `0 0 * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/auto_update.sh >> /home/pi/logs/git_log.txt 2>&1`
  - Ran once a day
- Keep in mind that these paths are relative and will have to be adjusted based on where the repository files are stored on the raspberry pi

# Part List

- [Waveshare E-Paper 7.5" Display](https://www.waveshare.com/7.5inch-e-paper-hat.htm)
- Simple 7"x5.5" picture frame
- Raspberry Pi Zero W
- 3D printed case to hold raspberry pi and waveshare pi hat
- USB power supply for the pi

# Final Results
## Displaying a Picture
![image](https://user-images.githubusercontent.com/47571939/163594413-c6eb3808-4513-4128-835d-d810d2fb4636.png)
## Displaying Text
![image](https://user-images.githubusercontent.com/47571939/163594460-b78e4632-db6d-4be3-b008-e5df3094526b.png)
## The Wiring
![image](https://user-images.githubusercontent.com/47571939/163594511-85c280f8-d6a0-4ccc-a2cf-58c4714fc34f.png)
![image](https://user-images.githubusercontent.com/47571939/163594503-563f6b02-9094-4581-a29f-ac31b20f1bf2.png)
## The Case Designed for the Pi + Waveshare Pi-HAT
![image](https://user-images.githubusercontent.com/47571939/163594571-d0f96c77-cdce-4f6a-8a29-8d045d3c6322.png)
![image](https://user-images.githubusercontent.com/47571939/163594608-162ef34d-f450-4de3-981d-98be82ceefe8.png)
![image](https://user-images.githubusercontent.com/47571939/163594590-a1441343-8cd4-471c-af65-96cdf93154f1.png)


