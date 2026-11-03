# Geyser IR remote (ESP32)

An ESP32 that learns the geyser's IR remote and presses its buttons when
Solar Hub asks. It never touches the geyser's wiring: it only sends the same
infrared signal the remote sends, and the geyser's own thermostat still
decides when the water is hot.

Solar Hub talks to it over your Wi-Fi at `http://solarhub-geyser.local`.

## Parts

| Part | Notes |
|---|---|
| ESP32 DevKit V1 (ESP32-WROOM-32, 30 or 38 pin) | Any ESP32 board with a 5 V (VIN) pin works |
| IR LED, 940 nm, 5 mm | The kind used in TV remotes. Buy 2–3; they're cheap |
| IR receiver VS1838B (or TSOP38238) | 38 kHz, 3 pins. Only needed while learning, but leave it in |
| NPN transistor 2N2222 or S8050 | Drives the LED harder than the ESP32 pin can |
| 1 kΩ resistor | Between the ESP32 pin and the transistor's base |
| 47 Ω resistor, ¼ W | In series with the IR LED. Use 33 Ω if the range is too short |
| USB 5 V adapter (1 A) + micro-USB cable | Powers the ESP32 |
| Breadboard and jumper wires (optional) | Or solder it on a small perfboard |
| Small box (optional) | Keep it away from water and steam |

## Wiring

| From | To |
|---|---|
| ESP32 GPIO 4 | 1 kΩ → transistor base (B) |
| Transistor emitter (E) | GND |
| Transistor collector (C) | IR LED cathode (short leg, flat side) |
| IR LED anode (long leg) | 47 Ω → ESP32 VIN (5 V) |
| IR receiver OUT | ESP32 GPIO 14 |
| IR receiver GND | GND |
| IR receiver VCC | ESP32 3V3 |

```
 ESP32 VIN (5 V) ──[47 Ω]──►|── IR LED ──┐
                       anode    cathode   │
                                          C
 ESP32 GPIO 4 ─────[1 kΩ]──────────── B  NPN (2N2222 / S8050)
                                          E
                                          │
 ESP32 GND ───────────────────────────────┴──────────────┐
                                                         │
                 VS1838B (dome facing you)               │
                 ┌─────────────┐                         │
                 │  OUT GND VCC│                         │
                 └───┬───┬───┬─┘                         │
 ESP32 GPIO 14 ──────┘   │   └────── ESP32 3V3           │
                         └───────────────────────────────┘
```

Check your transistor's pin order on its datasheet. With the flat face
towards you, 2N2222 (TO-92) and S8050 are usually E, B, C from left to right,
but some makers differ. The IR receiver's pin order is also worth checking:
VS1838B and TSOP38238 are usually OUT, GND, VCC with the dome facing you.

## Where to put it

- The IR LED has to point at the geyser's control panel (where the remote's
  signal is received), with nothing in between. Within 3–5 m is safest.
- It needs Wi-Fi there. After flashing, open `/status` and check `rssi`:
  better than −75 is fine.
- Geysers are often in a bathroom or on the roof. Keep the ESP32 dry and away
  from steam, in a box with a small hole for the LED.

## Flashing

You need [PlatformIO](https://platformio.org/) (the VS Code extension, or the
`pio` command line).

1. Copy `include/secrets.example.h` to `include/secrets.h` and put in your
   Wi-Fi name and password. `secrets.h` is never committed.
2. Plug the ESP32 into the Mac with USB.
3. From this folder:

   ```bash
   pio run -t upload
   pio device monitor
   ```

   The monitor prints the address once Wi-Fi connects, for example
   `Wi-Fi connected: http://192.168.1.50/ (http://solarhub-geyser.local/)`.
   Some boards need the BOOT button held while the upload starts.
4. Open `http://solarhub-geyser.local/status` in a browser. You should see
   `"ok":true`.

## Teaching it the remote

The ESP32 learns each button by listening to the real remote once. Codes are
saved on the ESP32 and survive a power cut.

1. In Solar Hub, open the geyser remote settings and enter
   `solarhub-geyser.local` (or the IP the monitor printed).
2. Tap **Learn** next to a button (On/Off, 800 W, 1200 W, 2000 W,
   Temperature up, Temperature down).
3. Within 20 seconds, point the geyser's remote at the ESP32's receiver from
   10–30 cm away and press that same button once.
4. The button shows as learned. Repeat for the others.
5. Tap **Test** for each one while watching the geyser to make sure it reacts.

If learning fails with "capture too long", press the button more briefly. If it
times out, move the remote closer to the receiver.

## Testing with curl

```bash
curl http://solarhub-geyser.local/status
curl -X POST "http://solarhub-geyser.local/learn?button=power"
curl -X POST "http://solarhub-geyser.local/send?button=w800"
curl -X POST "http://solarhub-geyser.local/forget?button=w800"
```

Buttons: `power`, `w800`, `w1200`, `w2000`, `temp_up`, `temp_down`.

| Request | Answer |
|---|---|
| `GET /status` | which buttons are learned, what it's learning now, the last learn result and last button sent, uptime and Wi-Fi signal |
| `POST /learn?button=X` | starts a 20 s learn window and answers at once |
| `POST /send?button=X` | sends the code; 404 if X isn't learned yet, 409 while learning |
| `POST /forget?button=X` | deletes the code for X |
| unknown button | 400 |
