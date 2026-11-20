#include <WiFi.h>
#include <WebServer.h>
#include <ESPmDNS.h>
#include <HTTPUpdateServer.h>
#include <Preferences.h>
#include <vector>
#include <IRremoteESP8266.h>
#include <IRsend.h>
#include <IRrecv.h>
#include <IRutils.h>
#include "secrets.h"

const uint16_t IR_LED_PIN = 25;
const uint16_t IR_RECEIVER_PIN = 34;
const uint8_t RECEIVER_POWER_PIN = 33;
const uint16_t DEFAULT_REPEATS = 1;
const uint16_t MAX_REPEATS = 5;
const char *HOSTNAME = "geyser";
const unsigned long LEARN_WINDOW_MS = 15000;
const size_t MAX_LEARNED = 40;
const size_t MAX_NAME_LENGTH = 20;

struct Code {
  String name;
  decode_type_t protocol;
  uint16_t bits;
  uint64_t value;
};

const Code BUILT_IN[] = {
  {"power", NEC, 32, 0x4FB609F},
  {"800", NEC, 32, 0x4FB08F7},
  {"1200", NEC, 32, 0x4FB6897},
  {"up", NEC, 32, 0x4FB00FF},
  {"down", NEC, 32, 0x4FB12ED},
};

const char PAGE[] PROGMEM = R"rawliteral(<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>IR remote</title>
<style>
:root{--bg:#f5f5f5;--fg:#111;--card:#fff;--line:#ccc;--press:#e8e8e8;--muted:#555}
@media (prefers-color-scheme:dark){:root{--bg:#121212;--fg:#eee;--card:#1e1e1e;--line:#444;--press:#2a2a2a;--muted:#aaa}}
body{font-family:system-ui,sans-serif;margin:0;padding:16px;background:var(--bg);color:var(--fg);max-width:460px}
h1{font-size:20px;margin:0 0 12px}
h2{font-size:16px;margin:22px 0 8px}
.grid{display:grid;grid-template-columns:repeat(2,1fr);gap:10px}
button,input{font:inherit;font-size:17px;border:1px solid var(--line);border-radius:10px;background:var(--card);color:var(--fg)}
button{padding:16px 8px}
button:active{background:var(--press)}
.row{display:flex;gap:8px;margin-top:8px}
.row input{flex:1;padding:12px;min-width:0}
.row button{padding:12px 16px}
.learned{display:flex;gap:6px}
.learned button:first-child{flex:1}
.learned button:last-child{padding:16px 12px;color:var(--muted)}
#status{margin-top:14px;font-size:15px;color:var(--muted);min-height:20px}
.hint{font-size:14px;color:var(--muted);margin:6px 0 0}
</style></head><body>
<h1>IR remote</h1>
<h2>Geyser</h2>
<div class="grid">
<button data-b="power">Power</button>
<button data-b="800">800W</button>
<button data-b="1200">1200W</button>
<button data-b="up">Temp &#9650;</button>
<button data-b="down">Temp &#9660;</button>
</div>
<h2>Other buttons</h2>
<div class="grid" id="learned"></div>
<p class="hint" id="empty">None yet. Teach one below.</p>
<h2>Teach a button</h2>
<div class="row"><input id="name" placeholder="name, e.g. ac_on" maxlength="20" autocapitalize="none"><button id="learn">Teach</button></div>
<p class="hint">Tap Teach, then press the button on its remote within 15 seconds, pointed at this board.</p>
<p id="status"></p>
<script>
const status = document.getElementById('status');
async function press(name, label) {
  status.textContent = 'Sending ' + label + '...';
  try {
    const r = await fetch('/press?button=' + encodeURIComponent(name), { method: 'POST' });
    const j = await r.json();
    status.textContent = j.ok ? 'Sent ' + label : 'Error: ' + j.error;
  } catch (e) { status.textContent = 'No reply from the ESP32'; }
}
document.querySelectorAll('[data-b]').forEach((b) => { b.onclick = () => press(b.dataset.b, b.textContent); });
async function loadLearned() {
  const box = document.getElementById('learned');
  try {
    const list = (await (await fetch('/buttons')).json()).filter((c) => !c.built_in);
    box.replaceChildren(...list.map((c) => {
      const wrap = document.createElement('div'); wrap.className = 'learned';
      const send = document.createElement('button'); send.textContent = c.name; send.onclick = () => press(c.name, c.name);
      const forget = document.createElement('button'); forget.textContent = '×'; forget.title = 'Forget ' + c.name;
      forget.onclick = async () => {
        if (!confirm('Forget ' + c.name + '?')) return;
        await fetch('/forget?name=' + encodeURIComponent(c.name), { method: 'POST' });
        loadLearned();
      };
      wrap.append(send, forget); return wrap;
    }));
    document.getElementById('empty').hidden = list.length > 0;
  } catch (e) { status.textContent = 'No reply from the ESP32'; }
}
document.getElementById('learn').onclick = async () => {
  const name = document.getElementById('name').value.trim().toLowerCase();
  try {
    const j = await (await fetch('/learn?name=' + encodeURIComponent(name), { method: 'POST' })).json();
    if (!j.ok) { status.textContent = 'Error: ' + j.error; return; }
  } catch (e) { status.textContent = 'No reply from the ESP32'; return; }
  status.textContent = 'Press the remote button now...';
  const timer = setInterval(async () => {
    try {
      const s = await (await fetch('/status')).json();
      if (s.learning) return;
      clearInterval(timer);
      status.textContent = s.learn_result;
      document.getElementById('name').value = '';
      loadLearned();
    } catch (e) {}
  }, 1000);
};
loadLearned();
</script>
</body></html>)rawliteral";

IRsend irsend(IR_LED_PIN);
IRrecv irrecv(IR_RECEIVER_PIN);
decode_results results;
WebServer server(80);
HTTPUpdateServer updater;
Preferences prefs;
std::vector<Code> learned;

String lastButton = "";
unsigned long lastPressedAt = 0;
String lastHeard = "";
unsigned long lastHeardAt = 0;
String learnName = "";
unsigned long learnStartedAt = 0;
String learnResult = "";

String hex(uint64_t value) {
  return "0x" + uint64ToString(value, 16);
}

const Code *findCode(const String &name) {
  for (const Code &code : BUILT_IN) {
    if (code.name == name) return &code;
  }
  for (const Code &code : learned) {
    if (code.name == name) return &code;
  }
  return nullptr;
}

String nameFor(decode_type_t protocol, uint64_t value) {
  for (const Code &code : BUILT_IN) {
    if (code.protocol == protocol && code.value == value) return code.name;
  }
  for (const Code &code : learned) {
    if (code.protocol == protocol && code.value == value) return code.name;
  }
  return typeToString(protocol) + " " + hex(value);
}

bool validName(const String &name) {
  if (name.length() == 0 || name.length() > MAX_NAME_LENGTH) return false;
  for (char c : name) {
    if (!isalnum(c) && c != '_' && c != '-') return false;
  }
  return true;
}

void saveLearned() {
  String lines = "";
  for (const Code &code : learned) {
    lines += code.name + "," + typeToString(code.protocol) + "," + String(code.bits) + "," + uint64ToString(code.value, 16) + "\n";
  }
  prefs.putString("codes", lines);
}

void loadLearned() {
  String lines = prefs.getString("codes", "");
  int start = 0;
  while (start < (int)lines.length()) {
    int end = lines.indexOf('\n', start);
    if (end < 0) end = lines.length();
    String line = lines.substring(start, end);
    start = end + 1;
    int a = line.indexOf(','), b = line.indexOf(',', a + 1), c = line.indexOf(',', b + 1);
    if (a < 0 || b < 0 || c < 0) continue;
    Code code;
    code.name = line.substring(0, a);
    code.protocol = strToDecodeType(line.substring(a + 1, b).c_str());
    code.bits = line.substring(b + 1, c).toInt();
    code.value = strtoull(line.substring(c + 1).c_str(), nullptr, 16);
    if (code.protocol != UNKNOWN) learned.push_back(code);
  }
}

void sendJson(int status, const String &json) {
  server.send(status, "application/json", json);
}

void sendError(int status, const String &message) {
  sendJson(status, "{\"ok\":false,\"error\":\"" + message + "\"}");
}

uint16_t requestedRepeats() {
  if (!server.hasArg("repeat")) return DEFAULT_REPEATS;
  return constrain(server.arg("repeat").toInt(), 0, MAX_REPEATS);
}

void markPressed(const String &name) {
  lastButton = name;
  lastPressedAt = millis();
  Serial.println("Pressed " + name);
}

void handlePress() {
  String name = server.arg("button");
  uint16_t repeats = requestedRepeats();
  const Code *code = findCode(name);
  if (code == nullptr) {
    sendError(400, "unknown button");
    return;
  }
  irsend.send(code->protocol, code->value, code->bits, repeats);
  markPressed(name);
  sendJson(200, "{\"ok\":true,\"button\":\"" + name + "\"}");
}

void handleSend() {
  decode_type_t protocol = strToDecodeType(server.hasArg("protocol") ? server.arg("protocol").c_str() : "NEC");
  if (protocol == UNKNOWN || hasACState(protocol)) {
    sendError(400, "unsupported protocol");
    return;
  }
  if (!server.hasArg("code")) {
    sendError(400, "code is required");
    return;
  }
  uint64_t value = strtoull(server.arg("code").c_str(), nullptr, 16);
  uint16_t bits = server.hasArg("bits") ? server.arg("bits").toInt() : IRsend::defaultBits(protocol);
  if (!irsend.send(protocol, value, bits, requestedRepeats())) {
    sendError(400, "send failed");
    return;
  }
  String label = typeToString(protocol) + " " + hex(value);
  markPressed(label);
  sendJson(200, "{\"ok\":true,\"sent\":\"" + label + "\"}");
}

void handleButtons() {
  String json = "[";
  auto add = [&json](const Code &code, bool builtIn) {
    if (json.length() > 1) json += ",";
    json += "{\"name\":\"" + code.name + "\",\"protocol\":\"" + typeToString(code.protocol) + "\",\"bits\":" + String(code.bits) +
            ",\"code\":\"" + hex(code.value) + "\",\"built_in\":" + (builtIn ? "true" : "false") + "}";
  };
  for (const Code &code : BUILT_IN) add(code, true);
  for (const Code &code : learned) add(code, false);
  json += "]";
  sendJson(200, json);
}

void handleLearn() {
  String name = server.arg("name");
  name.toLowerCase();
  if (!validName(name)) {
    sendError(400, "use 1-20 letters, digits, _ or -");
    return;
  }
  for (const Code &code : BUILT_IN) {
    if (code.name == name) {
      sendError(400, "that name is a geyser button");
      return;
    }
  }
  if (findCode(name) == nullptr && learned.size() >= MAX_LEARNED) {
    sendError(400, "memory full, forget a button first");
    return;
  }
  learnName = name;
  learnStartedAt = millis();
  learnResult = "";
  sendJson(200, "{\"ok\":true,\"learning\":\"" + name + "\"}");
}

void handleForget() {
  String name = server.arg("name");
  for (auto it = learned.begin(); it != learned.end(); ++it) {
    if (it->name == name) {
      learned.erase(it);
      saveLearned();
      sendJson(200, "{\"ok\":true}");
      return;
    }
  }
  sendError(404, "no such learned button");
}

String secondsAgo(unsigned long at) {
  return at == 0 ? String("null") : String((millis() - at) / 1000);
}

void handleStatus() {
  String json = "{\"ok\":true";
  json += ",\"ip\":\"" + WiFi.localIP().toString() + "\"";
  json += ",\"rssi\":" + String(WiFi.RSSI());
  json += ",\"uptime_s\":" + String(millis() / 1000);
  json += ",\"last_button\":\"" + lastButton + "\"";
  json += ",\"last_pressed_s_ago\":" + secondsAgo(lastPressedAt);
  json += ",\"last_heard\":\"" + lastHeard + "\"";
  json += ",\"last_heard_s_ago\":" + secondsAgo(lastHeardAt);
  json += ",\"learning\":" + String(learnName.length() ? "true" : "false");
  json += ",\"learn_result\":\"" + learnResult + "\"";
  json += ",\"learned\":" + String(learned.size());
  json += "}";
  sendJson(200, json);
}

void learnFrom(const decode_results &heard) {
  if (hasACState(heard.decode_type) || heard.bits > 64) {
    learnResult = "That remote sends long codes (like an AC), not supported yet";
  } else {
    Code code = {learnName, heard.decode_type, heard.bits, heard.value};
    bool replaced = false;
    for (Code &existing : learned) {
      if (existing.name == code.name) {
        existing = code;
        replaced = true;
      }
    }
    if (!replaced) learned.push_back(code);
    saveLearned();
    learnResult = "Saved " + code.name + " (" + typeToString(code.protocol) + " " + hex(code.value) + ")";
  }
  Serial.println(learnResult);
  learnName = "";
}

void setup() {
  Serial.begin(115200);
  pinMode(RECEIVER_POWER_PIN, OUTPUT);
  digitalWrite(RECEIVER_POWER_PIN, HIGH);
  irsend.begin();
  prefs.begin("ir", false);
  loadLearned();

  WiFi.mode(WIFI_STA);
  WiFi.setHostname(HOSTNAME);
  WiFi.setAutoReconnect(true);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  Serial.print("Connecting to Wi-Fi");
  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
  }
  Serial.println();
  Serial.println("Connected. Open http://" + WiFi.localIP().toString());

  if (MDNS.begin(HOSTNAME)) {
    MDNS.addService("http", "tcp", 80);
    Serial.println("Also at http://geyser.local");
  }

  server.on("/", HTTP_GET, [] { server.send_P(200, "text/html", PAGE); });
  server.on("/press", HTTP_POST, handlePress);
  server.on("/send", HTTP_POST, handleSend);
  server.on("/buttons", HTTP_GET, handleButtons);
  server.on("/learn", HTTP_POST, handleLearn);
  server.on("/forget", HTTP_POST, handleForget);
  server.on("/status", HTTP_GET, handleStatus);
  updater.setup(&server, "/update", "admin", OTA_PASSWORD);
  server.begin();
  irrecv.enableIRIn();
}

void loop() {
  server.handleClient();

  if (learnName.length() && millis() - learnStartedAt > LEARN_WINDOW_MS) {
    learnResult = "Nothing heard in 15 seconds, try again closer";
    learnName = "";
  }

  if (irrecv.decode(&results)) {
    if (!results.repeat && results.decode_type != UNKNOWN) {
      lastHeard = nameFor(results.decode_type, results.value);
      lastHeardAt = millis();
      Serial.println("Heard " + lastHeard);
      if (learnName.length()) learnFrom(results);
    }
    irrecv.resume();
  }
}
