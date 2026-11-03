#include <Arduino.h>
#include <ESPmDNS.h>
#include <IRrecv.h>
#include <IRsend.h>
#include <IRutils.h>
#include <Preferences.h>
#include <WebServer.h>
#include <WiFi.h>

#include <memory>

#if __has_include("secrets.h")
#include "secrets.h"
#else
#error "Copy include/secrets.example.h to include/secrets.h and fill in your Wi-Fi name and password"
#endif

namespace {

constexpr const char *kVersion = "1";
constexpr const char *kHostname = "solarhub-geyser";
constexpr uint16_t kRecvPin = 14;
constexpr uint16_t kSendPin = 4;
constexpr uint16_t kCarrierKhz = 38;
constexpr uint16_t kCaptureBufferSize = 1024;
constexpr uint8_t kCaptureTimeoutMs = 50;
constexpr uint16_t kUnknownThreshold = 12;
constexpr uint16_t kMinRawLength = 16;
constexpr uint32_t kLearnWindowMs = 20000;
constexpr uint32_t kWifiRetryMs = 10000;

const char *const kButtons[] = {"power", "w800", "w1200", "w2000", "temp_up", "temp_down"};
constexpr int kButtonCount = sizeof(kButtons) / sizeof(kButtons[0]);

WebServer server(80);
Preferences prefs;
IRrecv irrecv(kRecvPin, kCaptureBufferSize, kCaptureTimeoutMs, true);
IRsend irsend(kSendPin);
decode_results results;

int learningIndex = -1;
uint32_t learnDeadline = 0;
int lastLearnIndex = -1;
const char *lastLearnError = nullptr;
uint16_t lastLearnLength = 0;
int lastSentIndex = -1;
uint32_t lastSentAt = 0;
bool wifiWasConnected = false;
uint32_t lastWifiRetry = 0;

int buttonIndex(const String &name) {
  for (int i = 0; i < kButtonCount; i++) {
    if (name == kButtons[i]) {
      return i;
    }
  }
  return -1;
}

bool isLearned(int index) { return prefs.isKey(kButtons[index]); }

String quotedOrNull(int index) {
  return index < 0 ? String("null") : String("\"") + kButtons[index] + "\"";
}

void reply(int code, const String &json) { server.send(code, "application/json", json); }

void replyError(int code, const char *error) {
  reply(code, String("{\"ok\":false,\"error\":\"") + error + "\"}");
}

int requestedButton() {
  if (!server.hasArg("button")) {
    replyError(400, "missing button");
    return -1;
  }
  int index = buttonIndex(server.arg("button"));
  if (index < 0) {
    replyError(400, "unknown button");
  }
  return index;
}

void finishLearning(const char *error) {
  lastLearnIndex = learningIndex;
  lastLearnError = error;
  learningIndex = -1;
  irrecv.disableIRIn();
  Serial.printf("Learn %s: %s\n", kButtons[lastLearnIndex], error ? error : "stored");
}

void handleStatus() {
  String json = "{\"ok\":true,\"learned\":{";
  for (int i = 0; i < kButtonCount; i++) {
    json += String("\"") + kButtons[i] + "\":" + (isLearned(i) ? "true" : "false");
    if (i < kButtonCount - 1) {
      json += ",";
    }
  }
  json += "},\"learning\":" + quotedOrNull(learningIndex);
  json += ",\"last_learn\":";
  if (lastLearnIndex < 0) {
    json += "null";
  } else {
    json += String("{\"button\":\"") + kButtons[lastLearnIndex] + "\",\"ok\":" + (lastLearnError ? "false" : "true");
    json += ",\"error\":" + (lastLearnError ? String("\"") + lastLearnError + "\"" : String("null"));
    json += ",\"length\":" + String(lastLearnError ? 0 : lastLearnLength) + "}";
  }
  json += ",\"last_sent\":" + quotedOrNull(lastSentIndex);
  json += ",\"last_sent_ms_ago\":" + (lastSentIndex < 0 ? String("null") : String(millis() - lastSentAt));
  json += ",\"uptime_s\":" + String(millis() / 1000);
  json += ",\"rssi\":" + String(WiFi.RSSI());
  json += String(",\"version\":\"") + kVersion + "\"}";
  reply(200, json);
}

void handleLearn() {
  int index = requestedButton();
  if (index < 0) {
    return;
  }
  learningIndex = index;
  learnDeadline = millis() + kLearnWindowMs;
  lastLearnIndex = -1;
  lastLearnError = nullptr;
  irrecv.enableIRIn();
  Serial.printf("Learning %s for %lu s\n", kButtons[index], (unsigned long)(kLearnWindowMs / 1000));
  reply(200, String("{\"ok\":true,\"learning\":\"") + kButtons[index] + "\"}");
}

void handleSend() {
  int index = requestedButton();
  if (index < 0) {
    return;
  }
  if (learningIndex >= 0) {
    replyError(409, "learning");
    return;
  }
  size_t bytes = isLearned(index) ? prefs.getBytesLength(kButtons[index]) : 0;
  uint16_t length = bytes / sizeof(uint16_t);
  if (length == 0) {
    replyError(404, "not learned");
    return;
  }
  std::unique_ptr<uint16_t[]> raw(new uint16_t[length]);
  prefs.getBytes(kButtons[index], raw.get(), length * sizeof(uint16_t));
  irsend.sendRaw(raw.get(), length, kCarrierKhz);
  lastSentIndex = index;
  lastSentAt = millis();
  Serial.printf("Sent %s (%u timings)\n", kButtons[index], length);
  reply(200, "{\"ok\":true}");
}

void handleForget() {
  int index = requestedButton();
  if (index < 0) {
    return;
  }
  if (isLearned(index)) {
    prefs.remove(kButtons[index]);
  }
  reply(200, "{\"ok\":true}");
}

void pollLearning() {
  if (learningIndex < 0) {
    return;
  }
  if (irrecv.decode(&results)) {
    if (results.overflow) {
      finishLearning("capture too long");
      return;
    }
    uint16_t length = getCorrectedRawLength(&results);
    if (length < kMinRawLength) {
      irrecv.resume();
      return;
    }
    std::unique_ptr<uint16_t[]> raw(resultToRawArray(&results));
    size_t written = prefs.putBytes(kButtons[learningIndex], raw.get(), length * sizeof(uint16_t));
    lastLearnLength = length;
    finishLearning(written == length * sizeof(uint16_t) ? nullptr : "storage failed");
    return;
  }
  if ((int32_t)(millis() - learnDeadline) >= 0) {
    finishLearning("timeout");
  }
}

void startMdns() {
  MDNS.end();
  if (MDNS.begin(kHostname)) {
    MDNS.addService("http", "tcp", 80);
  }
}

void maintainWifi() {
  bool connected = WiFi.status() == WL_CONNECTED;
  if (connected && !wifiWasConnected) {
    startMdns();
    Serial.printf("Wi-Fi connected: http://%s/ (http://%s.local/)\n", WiFi.localIP().toString().c_str(), kHostname);
  }
  wifiWasConnected = connected;
  if (connected || millis() - lastWifiRetry < kWifiRetryMs) {
    return;
  }
  lastWifiRetry = millis();
  WiFi.disconnect();
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
}

}  // namespace

void setup() {
  Serial.begin(115200);
  prefs.begin("ir", false);
  irsend.begin();
  irrecv.setUnknownThreshold(kUnknownThreshold);

  WiFi.setHostname(kHostname);
  WiFi.mode(WIFI_STA);
  WiFi.setSleep(false);
  WiFi.setAutoReconnect(true);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  lastWifiRetry = millis();

  server.on("/status", HTTP_GET, handleStatus);
  server.on("/learn", HTTP_POST, handleLearn);
  server.on("/send", HTTP_POST, handleSend);
  server.on("/forget", HTTP_POST, handleForget);
  server.onNotFound([] { replyError(404, "not found"); });
  server.begin();
}

void loop() {
  server.handleClient();
  maintainWifi();
  pollLearning();
  delay(2);
}
