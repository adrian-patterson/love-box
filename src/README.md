# eink-message-system
# Crontab Entries
* `*/10 * * * * /home/pi/projects/eink-message-system/shell_scripts/update_display.sh >> /home/pi/logs/update_display_log.txt 2>&1`
    * Ran every 10 minutes
* `0 0 * * * /home/pi/projects/eink-message-system/shell_scripts/auto_update.sh >> /home/pi/logs/git_log.txt 2>&1`
    * Ran once a day