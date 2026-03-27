/*
  Arduino Mega 2560 full output sweep

  Sweep order:
  1. D21_SCL down to D0_RX0
  2. D22..D53, which maps from PA0..PB0 on the ATmega2560
  3. A15..A0, which maps from ADC15..ADC0 on the board header

  Note:
  The separate SDA/SCL header on the Mega Rev3 is electrically the same as
  D20/D21, so those nets are pulsed once through D20 and D21.

  D0 and D1 are also UART0 pins, so per-pin debug logging pauses for those
  two steps while the wave still continues.
*/

constexpr unsigned long SERIAL_BAUD = 115200;
constexpr unsigned long PULSE_HIGH_MS = 1000;
constexpr unsigned long PULSE_LOW_MS = 300;
constexpr unsigned long GROUP_GAP_MS = 250;
constexpr unsigned long CYCLE_GAP_MS = 1000;

struct PinStep {
  uint8_t pin;
  const char* label;
};

const PinStep kLeftSidePins[] = {
  {21, "D21/SCL"},
  {20, "D20/SDA"},
  {19, "D19/RX1"},
  {18, "D18/TX1"},
  {17, "D17/RX2"},
  {16, "D16/TX2"},
  {15, "D15/RX3"},
  {14, "D14/TX3"},
  {13, "D13"},
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
  {1, "D1/TX0"},
  {0, "D0/RX0"}
};

const PinStep kPortOrderedPins[] = {
  {22, "D22/PA0"},
  {23, "D23/PA1"},
  {24, "D24/PA2"},
  {25, "D25/PA3"},
  {26, "D26/PA4"},
  {27, "D27/PA5"},
  {28, "D28/PA6"},
  {29, "D29/PA7"},
  {30, "D30/PC7"},
  {31, "D31/PC6"},
  {32, "D32/PC5"},
  {33, "D33/PC4"},
  {34, "D34/PC3"},
  {35, "D35/PC2"},
  {36, "D36/PC1"},
  {37, "D37/PC0"},
  {38, "D38/PD7"},
  {39, "D39/PG2"},
  {40, "D40/PG1"},
  {41, "D41/PG0"},
  {42, "D42/PL7"},
  {43, "D43/PL6"},
  {44, "D44/PL5"},
  {45, "D45/PL4"},
  {46, "D46/PL3"},
  {47, "D47/PL2"},
  {48, "D48/PL1"},
  {49, "D49/PL0"},
  {50, "D50/PB3"},
  {51, "D51/PB2"},
  {52, "D52/PB1"},
  {53, "D53/PB0"}
};

const PinStep kAnalogHeaderPins[] = {
  {A15, "A15"},
  {A14, "A14"},
  {A13, "A13"},
  {A12, "A12"},
  {A11, "A11"},
  {A10, "A10"},
  {A9, "A9"},
  {A8, "A8"},
  {A7, "A7"},
  {A6, "A6"},
  {A5, "A5"},
  {A4, "A4"},
  {A3, "A3"},
  {A2, "A2"},
  {A1, "A1"},
  {A0, "A0"}
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
  initializePins(kLeftSidePins, sizeof(kLeftSidePins) / sizeof(kLeftSidePins[0]));
  initializePins(kPortOrderedPins, sizeof(kPortOrderedPins) / sizeof(kPortOrderedPins[0]));
  initializePins(kAnalogHeaderPins, sizeof(kAnalogHeaderPins) / sizeof(kAnalogHeaderPins[0]));

  Serial.begin(SERIAL_BAUD);
  delay(50);
  Serial.println(F("[BOOT] Mega pin sweep starting"));
  Serial.print(F("[BOOT] Serial baud: "));
  Serial.println(SERIAL_BAUD);
}

void loop() {
  Serial.println(F("[CYCLE] Begin"));
  pulseGroup("Left side", kLeftSidePins, sizeof(kLeftSidePins) / sizeof(kLeftSidePins[0]));
  delay(GROUP_GAP_MS);

  pulseGroup("Port ordered", kPortOrderedPins, sizeof(kPortOrderedPins) / sizeof(kPortOrderedPins[0]));
  delay(GROUP_GAP_MS);

  pulseGroup("Analog header", kAnalogHeaderPins, sizeof(kAnalogHeaderPins) / sizeof(kAnalogHeaderPins[0]));
  Serial.println(F("[CYCLE] Complete"));
  delay(CYCLE_GAP_MS);
}
