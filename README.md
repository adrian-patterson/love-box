# love-box

- A remotely controllable e-paper display using waveshare's 7.5" e-paper display, a Raspberry Pi Zero W, and Firebase's free-tier real-time database
- A gift meant for leaving meaningful notes/messages for someone you love

# Repository Structure

## `/lib`

- All waveshare libraries
  - These are taken right from the [waveshare/e-paper repo](https://github.com/waveshare/e-Paper)
  - Contains a c shared library and python example for writing to the 7.5" display

---

## `/pic`

- Contains pictures and fonts
- Contains a python script to take any picture and resize it to the dimensions necessary for the 7.5" display
  - Also converts the images to `.bmp` filetypes

---

## `/src`

- All source code

### `/domain`

- Contains a class representing `Message`, which is stored in the firebase real-time database

### `/repository`

- Contains a repository for messages to update/read database values easily

### `/shell_scripts`

- `auto_update.sh`
  - Called every 24 hours using a cronjob to update raspberry pi with remote code
- `update_display.sh`
  - Called every 10 minutes using a cronjob to update the display with database contents
- `install.sh`
  - Installs needed dependencies for the raspberry pi

### Other

- `change_message.py` allows one to change the contents in the real-time database, remotely
- `update_display.py` updates the display itself with the contents within the database
  - The time stored within the database is compared with the current time
  - If the database was last updated in the previous 10 minutes, then the display is changed
- `message_preview.py` allows one to preview the message and how it will be displayed given a chosen font

# Crontab Entries

- `*/10 * * * * /home/pi/projects/love-box/shell_scripts/update_display.sh >> /home/pi/logs/update_display_log.txt 2>&1`
  - Ran every 10 minutes
- `0 0 * * * /home/pi/projects/love-box/shell_scripts/auto_update.sh >> /home/pi/logs/git_log.txt 2>&1`
  - Ran once a day
- Keep in mind that these paths are relative and will have to be adjusted based on where the repository files are stored on the raspberry pi

# Part List

- [Waveshare E-Paper 7.5" Display](https://www.waveshare.com/7.5inch-e-paper-hat.htm)
- Simple 7"x5.5" picture frame
- Raspberry Pi Zero W
- 3D printed case to hold raspberry pi and waveshare pi hat
- USB power supply for the pi

# Final Results
