/*
  Arduino Nano V3 full output sweep

  Sweep order:
  1. Left header, top to bottom: D13, A0, A1, A2, A3, A4/SDA, A5/SCL
  2. Right header, top to bottom: D12 down to D0, then D1

  Notes:
  - A6 and A7 are intentionally excluded because they are analog-input only
    on the classic ATmega328P Nano and cannot be used as digital outputs.
  - D0 and D1 are included because they are output-capable, but they are also
    the UART pins, so per-pin debug logging pauses for those two steps.
*/

constexpr unsigned long SERIAL_BAUD = 115200;
constexpr unsigned long PULSE_HIGH_MS = 1000;
constexpr unsigned long PULSE_LOW_MS = 300;
constexpr unsigned long GROUP_GAP_MS = 500;
constexpr unsigned long CYCLE_GAP_MS = 1200;

struct PinStep {
  uint8_t pin;
  const char* label;
};

const PinStep kLeftHeaderPins[] = {
  {13, "D13"},
  {A0, "A0"},
  {A1, "A1"},
  {A2, "A2"},
  {A3, "A3"},
  {A4, "A4/SDA"},
  {A5, "A5/SCL"}
};

const PinStep kRightHeaderPins[] = {
  {12, "D12"},
  {11, "D11"},
  {10, "D10"},
  {9, "D9"},
  {8, "D8"},
  {7, "D7"},
  {6, "D6"},
  {5, "D5"},
  {4, "D4"},
  {3, "D3"},
  {2, "D2"},
  {0, "D0/RX"},
  {1, "D1/TX"}
};

void initializePins(const PinStep* pins, size_t count) {
  for (size_t i = 0; i < count; ++i) {
    pinMode(pins[i].pin, OUTPUT);
    digitalWrite(pins[i].pin, LOW);
  }
}

bool isSerial0Pin(uint8_t pin) {
  return pin == 0 || pin == 1;
}

void logPulse(const char* groupLabel, const PinStep& step) {
  if (isSerial0Pin(step.pin)) {
    Serial.print(F("[SWEEP] "));
    Serial.print(groupLabel);
    Serial.print(F(" -> "));
    Serial.print(step.label);
    Serial.println(F(" (UART0 pin, per-pin logging limited)"));
    return;
  }

  Serial.print(F("[SWEEP] "));
  Serial.print(groupLabel);
  Serial.print(F(" -> "));
  Serial.println(step.label);
}

void pulsePin(const char* groupLabel, const PinStep& step) {
  logPulse(groupLabel, step);
  digitalWrite(step.pin, HIGH);
  delay(PULSE_HIGH_MS);
  digitalWrite(step.pin, LOW);
  delay(PULSE_LOW_MS);
}

void pulseGroup(const char* groupLabel, const PinStep* pins, size_t count) {
  Serial.print(F("[GROUP] "));
  Serial.println(groupLabel);
  for (size_t i = 0; i < count; ++i) {
    pulsePin(groupLabel, pins[i]);
  }
}

void setup() {
  initializePins(kLeftHeaderPins, sizeof(kLeftHeaderPins) / sizeof(kLeftHeaderPins[0]));
  initializePins(kRightHeaderPins, sizeof(kRightHeaderPins) / sizeof(kRightHeaderPins[0]));

  Serial.begin(SERIAL_BAUD);
  delay(50);
  Serial.println(F("[BOOT] Nano pin sweep starting"));
  Serial.print(F("[BOOT] Serial baud: "));
  Serial.println(SERIAL_BAUD);
}

void loop() {
  Serial.println(F("[CYCLE] Begin"));
  pulseGroup("Left header", kLeftHeaderPins, sizeof(kLeftHeaderPins) / sizeof(kLeftHeaderPins[0]));
  delay(GROUP_GAP_MS);

  pulseGroup("Right header", kRightHeaderPins, sizeof(kRightHeaderPins) / sizeof(kRightHeaderPins[0]));
  Serial.println(F("[CYCLE] Complete"));
  delay(CYCLE_GAP_MS);
}
