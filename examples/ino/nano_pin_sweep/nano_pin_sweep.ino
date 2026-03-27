/*
  Arduino Nano V3 full output sweep

  Sweep order:
  1. Left header, top to bottom: D13, A0, A1, A2, A3, A4/SDA, A5/SCL
  2. Right header, top to bottom: D12 down to D0, then D1

  Notes:
  - A6 and A7 are intentionally excluded because they are analog-input only
    on the classic ATmega328P Nano and cannot be used as digital outputs.
  - D0 and D1 are included because they are output-capable, but they are also
    the UART pins, so they may interact with serial hardware if attached.
*/

constexpr unsigned long PULSE_HIGH_MS = 1000;
constexpr unsigned long PULSE_LOW_MS = 300;
constexpr unsigned long GROUP_GAP_MS = 500;
constexpr unsigned long CYCLE_GAP_MS = 1200;

const uint8_t kLeftHeaderPins[] = {
  13, A0, A1, A2, A3, A4, A5
};

const uint8_t kRightHeaderPins[] = {
  12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 0, 1
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
  initializePins(kLeftHeaderPins);
  initializePins(kRightHeaderPins);
}

void loop() {
  pulseGroup(kLeftHeaderPins);
  delay(GROUP_GAP_MS);

  pulseGroup(kRightHeaderPins);
  delay(CYCLE_GAP_MS);
}
