#!/bin/sh
# Installs ntu-bus on the Pi. Run from the folder holding the copied files:
#   sudo sh install.sh
# Turns on SPI (screen) and I2C (touch), installs the binary, config and service, then reboots.
set -eu
cd "$(dirname "$0")"
user=${SUDO_USER:?run this with sudo}

case "$(uname -m)" in
    aarch64) bin=ntu-bus-aarch64 ;;
    armv7l) bin=ntu-bus-armv7 ;;
    *) echo "unsupported architecture $(uname -m)"; exit 1 ;;
esac

raspi-config nonint do_spi 0
raspi-config nonint do_i2c 0

install -m 755 "$bin" /usr/local/bin/ntu-bus
# Keep an existing config so reinstalls don't overwrite your edits
[ -e /etc/ntu-bus/config.toml ] || install -D -m 644 config.toml /etc/ntu-bus/config.toml
sed "s/^User=.*/User=$user/" ntu-bus.service > /etc/systemd/system/ntu-bus.service
systemctl daemon-reload
systemctl enable ntu-bus

echo "Installed. Rebooting so SPI and I2C come up; ntu-bus starts after boot."
systemctl reboot
