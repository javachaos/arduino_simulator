/*
  Arduino Mega 2560 full output sweep

  Sweep order:
  1. D21_SCL down to D0_RX0
  2. D22..D53, which maps from PA0..PB0 on the ATmega2560
  3. A15..A0, which maps from ADC15..ADC0 on the board header

  Note:
  The separate SDA/SCL header on the Mega Rev3 is electrically the same as
  D20/D21, so those nets are pulsed once through D20 and D21.
*/

constexpr unsigned long PULSE_HIGH_MS = 1000;
constexpr unsigned long PULSE_LOW_MS = 300;
constexpr unsigned long GROUP_GAP_MS = 250;
constexpr unsigned long CYCLE_GAP_MS = 1000;

const uint8_t kLeftSidePins[] = {
  21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11,
  10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0
};

const uint8_t kPortOrderedPins[] = {
  22, 23, 24, 25, 26, 27, 28, 29,  // PA0..PA7
  30, 31, 32, 33, 34, 35, 36, 37,  // PC7..PC0
  38, 39, 40, 41,                  // PD7, PG2, PG1, PG0
  42, 43, 44, 45, 46, 47, 48, 49,  // PL7..PL0
  50, 51, 52, 53                   // PB3, PB2, PB1, PB0
};

const uint8_t kAnalogHeaderPins[] = {
  A15, A14, A13, A12, A11, A10, A9, A8,
  A7, A6, A5, A4, A3, A2, A1, A0
};

template <size_t N>
void initializePins(const uint8_t (&pins)[N]) {
  for (size_t i = 0; i < N; ++i) {
    pinMode(pins[i], OUTPUT);
    digitalWrite(pins[i], LOW);
  }
}

void pulsePin(uint8_t pin) {
  digitalWrite(pin, HIGH);
  delay(PULSE_HIGH_MS);
  digitalWrite(pin, LOW);
  delay(PULSE_LOW_MS);
}

template <size_t N>
void pulseGroup(const uint8_t (&pins)[N]) {
  for (size_t i = 0; i < N; ++i) {
    pulsePin(pins[i]);
  }
}

void setup() {
  initializePins(kLeftSidePins);
  initializePins(kPortOrderedPins);
  initializePins(kAnalogHeaderPins);
}

void loop() {
  pulseGroup(kLeftSidePins);
  delay(GROUP_GAP_MS);

  pulseGroup(kPortOrderedPins);
  delay(GROUP_GAP_MS);

  pulseGroup(kAnalogHeaderPins);
  delay(CYCLE_GAP_MS);
}
