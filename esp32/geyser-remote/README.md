# Geyser remote (ESP32)

An ESP32 next to the geyser sends its IR remote codes. Solar Hub talks to it
over Wi-Fi (`src-tauri/src/geyser/`).

## Wiring

| ESP32 pin | Goes to |
|---|---|
| D25 | 1 kΩ → base of a C945 (NPN) |
| VIN (5 V) | 2 × 47 Ω in parallel → IR LED → IR LED → C945 collector; emitter → GND (about 95 mA) |
| D34 | IR receiver OUT |
| D33 | IR receiver VCC (driven high in `setup`) |
| GND | receiver GND, transistor emitter |

## HTTP

| Request | Does |
|---|---|
| `POST /press?button=power\|800\|1200\|up\|down\|<learned>` | Sends that button |
| `POST /send?protocol=&code=&bits=` | Sends any code |
| `GET /buttons` | Built-in and learned buttons |
| `POST /learn?name=` | Learns a remote button within 15 s |
| `POST /forget?name=` | Forgets a learned button |
| `GET /status` | ip, rssi, uptime, last button sent and last code heard |
| `/update` | Firmware upload (user `admin`, `OTA_PASSWORD`) |

It also answers as `geyser.local`, and `/` is a page with the buttons.

On the panel every button toggles: On/Off, and 800 and 1200 one element each
(both is 2000 W). 800 and 1200 do nothing while the geyser is off. There is no
2000 button, because pressing it after 800 switches 800 off.

## Build and upload

1. Copy `geyser_remote/secrets.example.h` to `geyser_remote/secrets.h` and fill it in.
2. Install the `esp32` core and the `IRremoteESP8266` library (2.9.0) in arduino-cli.
3. First time, over USB:
   `arduino-cli compile --upload -p /dev/cu.usbserial-XXXX --fqbn esp32:esp32:esp32:UploadSpeed=115200 geyser_remote`
4. After that, over Wi-Fi: `./ota_upload.sh [host]` (default `192.168.1.100`).
