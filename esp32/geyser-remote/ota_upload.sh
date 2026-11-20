#!/bin/sh
set -e
cd "$(dirname "$0")"
HOST="${1:-192.168.1.100}"
BUILD="$(mktemp -d)"
PASSWORD=$(sed -n 's/#define OTA_PASSWORD "\(.*\)"/\1/p' geyser_remote/secrets.h)
arduino-cli compile --fqbn esp32:esp32:esp32 --output-dir "$BUILD" geyser_remote
curl -sS -m 120 -H "Origin: http://$HOST" -u "admin:$PASSWORD" -F "firmware=@$BUILD/geyser_remote.ino.bin" "http://$HOST/update"
echo
rm -rf "$BUILD"
