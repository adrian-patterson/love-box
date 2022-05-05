#!/bin/bash

wget http://www.airspayce.com/mikem/bcm2835/bcm2835-1.60.tar.gz
tar zxvf bcm2835-1.60.tar.gz
cd bcm2835-1.60/
sudo ./configure
sudo make
sudo make check
sudo make install

sudo apt-get install wiringpi -y
wget https://project-downloads.drogon.net/wiringpi-latest.deb
sudo dpkg -i wiringpi-latest.deb
gpio -v

sudo apt-get update -y
sudo apt-get install python3-pip -y
sudo apt-get install python3-pil -y
sudo apt-get install python3-numpy -y
sudo pip3 install RPi.GPIO -y
sudo pip3 install spidev -y

chmod a+x update_display.sh
chmod a+x auto_update.sh
