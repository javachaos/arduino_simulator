#include <Arduino.h>
#include <EEPROM.h>
#include <LiquidCrystal.h>
#include <SPI.h>

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "commissioning_state.h"
#include "controller_can_bus.h"
#include "controller_sensor_helpers.h"
#include "controller_ui.h"
#include "control_safety.h"
#include "can_air_protocol.h"
#include "dewpoint_policy.h"
#include "lcd_ui_framework.h"
#include "max31865_rtd.h"
#include "modulating_actuator.h"
#include "sensor_calibration_storage.h"
#include "sensor_status_view.h"

constexpr uint8_t PIN_LCD_RS = 8;
constexpr uint8_t PIN_LCD_ENABLE = 9;
constexpr uint8_t PIN_LCD_D4 = 4;
constexpr uint8_t PIN_LCD_D5 = 5;
constexpr uint8_t PIN_LCD_D6 = 6;
constexpr uint8_t PIN_LCD_D7 = 7;
constexpr uint8_t PIN_LCD_BACKLIGHT = 10;
constexpr uint8_t PIN_LCD_KEYPAD = A0;
constexpr uint8_t PIN_ACTUATOR_COMMAND_PWM = 44;
constexpr uint8_t PIN_ACTUATOR_FEEDBACK = A10;
constexpr uint8_t PIN_COOLING_CALL = 24;  // LOW = cooling demand, with INPUT_PULLUP
constexpr uint8_t PIN_PIPE_RTD_CS = 26;
constexpr uint8_t PIN_CAN_CS = 27;
constexpr uint8_t PIN_CAN_INT = 28;
// Controller-side sensor status LEDs:
// - Off: sensor has not reported yet.
// - Solid: sensor is discovered and valid.
// - Blink: fixed-slot sensor is missing or reporting invalid data.
constexpr uint8_t PIN_SENSOR_STATUS_LED_ZONE_1 = 30;
constexpr uint8_t PIN_SENSOR_STATUS_LED_ZONE_2 = 31;
constexpr uint8_t PIN_SENSOR_STATUS_LED_ZONE_3 = 32;
constexpr uint8_t PIN_SENSOR_STATUS_LED_PIPE = 33;

constexpr bool USE_COOLING_CALL_INPUT = false;

constexpr size_t SENSOR_SLOT_COUNT = 4U;
constexpr size_t AIR_NODE_COUNT = 3U;
constexpr size_t LCD_COLUMNS = 16U;
constexpr size_t LCD_ROWS = 2U;
constexpr unsigned long SAMPLE_PERIOD_MS = 2000UL;
constexpr unsigned long KEYPAD_DEBOUNCE_MS = 30UL;
constexpr unsigned long KEYPAD_LONG_PRESS_MS = 1500UL;
constexpr unsigned long LCD_REFRESH_MS = 200UL;
constexpr unsigned long LCD_NOTICE_DURATION_MS = 5000UL;
constexpr uint8_t LCD_SENSOR_STATUS_CHAR_SLOT = 0U;
constexpr uint8_t LCD_SENSOR_STATUS_COLUMN = static_cast<uint8_t>(LCD_COLUMNS - 1U);
constexpr unsigned long LCD_SENSOR_STATUS_SELF_TEST_STEP_MS = 120UL;
constexpr unsigned long LCD_SENSOR_STATUS_SELF_TEST_ALL_ON_MS = 180UL;
constexpr size_t SERIAL_COMMAND_CAPACITY = 64U;
constexpr int EEPROM_IMAGE_ADDRESS = 0;
constexpr unsigned long BUS_DIAG_DEFAULT_COUNT = 12UL;
constexpr unsigned long BUS_DIAG_DEFAULT_DELAY_MS = 150UL;
constexpr unsigned long BUS_DIAG_MAX_COUNT = 50UL;
constexpr unsigned long BUS_DIAG_MAX_DELAY_MS = 2000UL;
constexpr unsigned long CAN_AIR_TIMEOUT_MS = 4000UL;
constexpr unsigned long CAN_BOOT_DISCOVERY_MS = 5500UL;
constexpr unsigned long CAN_SEQUENCE_STALE_MS = 1500UL;
constexpr uint32_t CAN_MCP2515_SPI_HZ = 1000000UL;
constexpr uint32_t CAN_MCP2515_OSCILLATOR_HZ = 8000000UL;
constexpr uint32_t CAN_BITRATE = 125000UL;
constexpr unsigned long PIPE_RTD_BIAS_SETTLE_MS = 10UL;
constexpr unsigned long PIPE_RTD_CONVERSION_MS = 65UL;
constexpr unsigned long PIPE_RTD_SPI_HZ = 1000000UL;
constexpr unsigned long SENSOR_STATUS_LED_BLINK_PERIOD_MS = 700UL;
constexpr unsigned long SENSOR_STATUS_LED_SELF_TEST_STEP_MS = 120UL;
constexpr unsigned long SENSOR_STATUS_LED_SELF_TEST_ALL_ON_MS = 180UL;
// Tuned from bench readings on the current 1602 LCD keypad shield:
// RIGHT ~3, UP ~104, DOWN ~260, LEFT ~410, SELECT ~641, NONE ~1023.
constexpr int KEYPAD_RIGHT_MAX = 60;
constexpr int KEYPAD_UP_MAX = 180;
constexpr int KEYPAD_DOWN_MAX = 335;
constexpr int KEYPAD_LEFT_MAX = 525;
constexpr int KEYPAD_SELECT_MAX = 850;
constexpr int KEYPAD_NONE_MIN = 1000;
constexpr float LCD_CAL_TEMP_STEP_C = 0.1f;
constexpr float LCD_CAL_RH_STEP_PERCENT = 0.5f;
constexpr bool PIPE_RTD_THREE_WIRE = false;
constexpr max31865_rtd::FilterMode PIPE_RTD_FILTER_MODE = max31865_rtd::FILTER_60HZ;
constexpr float ACTUATOR_COMMAND_PWM_VREF = 5.0f;
constexpr float ACTUATOR_OUTPUT_STAGE_GAIN = 2.0f;
constexpr float ACTUATOR_FEEDBACK_ADC_VREF = 5.0f;
constexpr float ACTUATOR_FEEDBACK_DIVIDER_GAIN = 2.0f;
constexpr bool ACTUATOR_WARMER_ON_HIGH_SIGNAL = false;
constexpr float PWM_CALIBRATION_TARGET_VOLTAGE_V = 5.0f;
constexpr uint8_t PWM_CALIBRATION_DUTY = 128U;
constexpr float MAX_CAL_TEMP_OFFSET_C = 5.0f;
constexpr float MAX_CAL_RH_OFFSET_PERCENT = 15.0f;

constexpr uint8_t SENSOR_STATUS_LED_PINS[SENSOR_SLOT_COUNT] = {
    PIN_SENSOR_STATUS_LED_ZONE_1,
    PIN_SENSOR_STATUS_LED_ZONE_2,
    PIN_SENSOR_STATUS_LED_ZONE_3,
    PIN_SENSOR_STATUS_LED_PIPE,
};

using controller_ui::CAL_FIELD_RH;
using controller_ui::CAL_FIELD_TEMP;
using controller_ui::CalibrationField;
using controller_ui::DISPLAY_PAGE_COMMISSIONING;
using controller_ui::DISPLAY_PAGE_COUNT;
using controller_ui::DISPLAY_PAGE_PIPE;
using controller_ui::DISPLAY_PAGE_SUMMARY;
using controller_ui::DISPLAY_PAGE_ZONE_1;
using controller_ui::DISPLAY_PAGE_ZONE_2;
using controller_ui::DISPLAY_PAGE_ZONE_3;
using controller_ui::DisplayPage;
using controller_ui::STAGE_MENU_CALIBRATE;
using controller_ui::STAGE_MENU_CAL_PWM;
using controller_ui::STAGE_MENU_EXIT;
using controller_ui::STAGE_MENU_VIEW_DATA;
using controller_ui::StageMenuItem;
using controller_ui::UI_KEY_DOWN;
using controller_ui::UI_KEY_LEFT;
using controller_ui::UI_KEY_NONE;
using controller_ui::UI_KEY_RIGHT;
using controller_ui::UI_KEY_SELECT;
using controller_ui::UI_KEY_UP;
using controller_ui::UI_OVERLAY_CAL_EDIT;
using controller_ui::UI_OVERLAY_CAL_SELECT;
using controller_ui::UI_OVERLAY_NONE;
using controller_ui::UI_OVERLAY_PWM_CAL;
using controller_ui::UI_OVERLAY_SENSOR_VIEW;
using controller_ui::UI_OVERLAY_STAGE_MENU;
using controller_ui::UiInputEvent;
using controller_ui::UiKey;
using controller_ui::UiOverlayMode;
using controller_ui::stageMenuItemFromIndex;
using controller_ui::stageMenuItemName;

constexpr dewpoint_policy::Config CONTROL_CONFIG = {
    AIR_NODE_COUNT,
    3.0f,
    0.5f,
    0.0f,
    50.0f,
    0.5f,
    100.0f,
};

const max31865_rtd::RtdModel PIPE_RTD_MODEL = max31865_rtd::pt1000Model();

constexpr modulating_actuator::Config ACTUATOR_CONFIG = {
    0.0f,
    100.0f,
    1.5f,
    12.0f,
    20000UL,
    ACTUATOR_WARMER_ON_HIGH_SIGNAL,
    true,
};

constexpr controller_can_bus::Config AIR_BUS_CONFIG = {
    AIR_NODE_COUNT,
    CAN_AIR_TIMEOUT_MS,
    CAN_SEQUENCE_STALE_MS,
    PIN_CAN_CS,
    PIN_CAN_INT,
    CAN_MCP2515_SPI_HZ,
    CAN_MCP2515_OSCILLATOR_HZ,
    CAN_BITRATE,
};

struct ActuatorTelemetry {
  float commandVoltageV;
  float measuredPercent;
  float measuredVoltageV;
  uint8_t pwmDuty;
  bool commandVoltageValid;
  bool feedbackWiringValid;
};

enum ControlSafetyFault {
  CONTROL_SAFETY_NONE = 0,
  CONTROL_SAFETY_INPUTS_INVALID = 1,
  CONTROL_SAFETY_DECISION_INVALID = 2,
  CONTROL_SAFETY_ACTUATOR_INVALID = 3,
  CONTROL_SAFETY_OUTPUT_INVALID = 4,
};

struct KeypadState {
  UiKey stableKey;
  UiKey lastRawKey;
  bool longPressReported;
  unsigned long lastRawChangeMs;
  unsigned long pressStartMs;
};

struct SensorSample {
  float rawTempC;
  float rawRhPercent;
  bool hasHumidity;
};

using AirNodeCache = controller_can_bus::Cache;

struct PipeSensorState {
  uint8_t faultStatus;
  bool driverResponding;
};

modulating_actuator::State actuatorState = {
    0.0f,
    NAN,
    false,
    false,
    modulating_actuator::FAULT_INVALID_STATE,
    0UL,
    false,
};

ActuatorTelemetry actuatorTelemetry = {
    NAN,
    NAN,
    NAN,
    0U,
    false,
    false,
};

KeypadState keypadState = {
    UI_KEY_NONE,
    UI_KEY_NONE,
    false,
    0UL,
    0UL,
};

commissioning::SensorRecord sensorRegistry[SENSOR_SLOT_COUNT];
SensorSample sensorSamples[SENSOR_SLOT_COUNT];
commissioning::State commissioningState = {};
dewpoint_policy::Inputs currentInputs = {};
dewpoint_policy::Decision currentDecision = {};
unsigned long lastSampleMs = 0UL;
char serialCommandBuffer[SERIAL_COMMAND_CAPACITY];
size_t serialCommandLength = 0U;
bool persistentStorageReady = false;
bool persistedCalibrationLoaded = false;
bool periodicStatusEnabled = true;
bool keypadTraceEnabled = true;
AirNodeCache airNodeCache[AIR_NODE_COUNT];
PipeSensorState pipeSensorState = {
    0U,
    false,
};
ControlSafetyFault controlSafetyFault = CONTROL_SAFETY_NONE;

LiquidCrystal lcd(PIN_LCD_RS, PIN_LCD_ENABLE, PIN_LCD_D4, PIN_LCD_D5, PIN_LCD_D6, PIN_LCD_D7);
lcd_ui::View lcdView(LCD_COLUMNS, LCD_REFRESH_MS);
controller_ui::State lcdUiState;
uint8_t lcdSensorStatusGlyph[8] = {};
uint8_t lcdSensorStatusGlyphCache[8] = {};
bool lcdSensorStatusGlyphInitialized = false;

bool coolingDemandActive() {
  if (!USE_COOLING_CALL_INPUT) {
    return true;
  }

  return digitalRead(PIN_COOLING_CALL) == LOW;
}

const char *controlSafetyFaultName(ControlSafetyFault fault) {
  switch (fault) {
    case CONTROL_SAFETY_INPUTS_INVALID:
      return "INPUTS_INVALID";
    case CONTROL_SAFETY_DECISION_INVALID:
      return "DECISION_INVALID";
    case CONTROL_SAFETY_ACTUATOR_INVALID:
      return "ACTUATOR_INVALID";
    case CONTROL_SAFETY_OUTPUT_INVALID:
      return "OUTPUT_INVALID";
    case CONTROL_SAFETY_NONE:
    default:
      return "NONE";
  }
}

void setControlSafetyFault(ControlSafetyFault fault) {
  if (controlSafetyFault == fault) {
    return;
  }

  controlSafetyFault = fault;
  Serial.print(F("Control safety="));
  Serial.println(controlSafetyFaultName(fault));
}

void initializeAirNodeCache() {
  controller_can_bus::initializeCache(airNodeCache, AIR_NODE_COUNT);
}

bool airNodeFresh(size_t zoneIndex, unsigned long nowMs) {
  return controller_can_bus::airNodeFresh(airNodeCache,
                                          AIR_NODE_COUNT,
                                          zoneIndex,
                                          nowMs,
                                          CAN_AIR_TIMEOUT_MS);
}

bool initializeAirBus() {
  return controller_can_bus::initializeController(SPI, AIR_BUS_CONFIG);
}

void pollAirBus(unsigned long nowMs) {
  controller_can_bus::pollBus(SPI, AIR_BUS_CONFIG, airNodeCache, AIR_NODE_COUNT, nowMs);
}

void syncAirNodeIdentityDescriptors() {
  controller_can_bus::syncIdentityDescriptors(
      airNodeCache, AIR_NODE_COUNT, sensorRegistry, SENSOR_SLOT_COUNT);
}

void primeAirNodeIdentityDiscovery(unsigned long timeoutMs) {
  controller_can_bus::primeIdentityDiscovery(
      SPI, AIR_BUS_CONFIG, airNodeCache, sensorRegistry, SENSOR_SLOT_COUNT, timeoutMs);
}

bool readAirZoneRaw(uint8_t zoneIndex, float &tempC, float &rhPercent) {
  return controller_can_bus::readAirZoneRaw(
      airNodeCache, AIR_NODE_COUNT, CAN_AIR_TIMEOUT_MS, zoneIndex, millis(), tempC, rhPercent);
}

void pipeRtdBeginTransaction() {
  SPI.beginTransaction(SPISettings(PIPE_RTD_SPI_HZ, MSBFIRST, SPI_MODE1));
  digitalWrite(PIN_PIPE_RTD_CS, LOW);
}

void pipeRtdEndTransaction() {
  digitalWrite(PIN_PIPE_RTD_CS, HIGH);
  SPI.endTransaction();
}

uint8_t pipeRtdReadRegister(uint8_t reg) {
  uint8_t value = 0U;

  pipeRtdBeginTransaction();
  SPI.transfer(reg & 0x7FU);
  value = SPI.transfer(0x00U);
  pipeRtdEndTransaction();
  return value;
}

void pipeRtdWriteRegister(uint8_t reg, uint8_t value) {
  pipeRtdBeginTransaction();
  SPI.transfer(static_cast<uint8_t>(0x80U | (reg & 0x7FU)));
  SPI.transfer(value);
  pipeRtdEndTransaction();
}

void pipeRtdDisableBias() {
  pipeRtdWriteRegister(max31865_rtd::kRegisterConfig,
                       max31865_rtd::buildConfigByte(false,
                                                     false,
                                                     PIPE_RTD_THREE_WIRE,
                                                     false,
                                                     PIPE_RTD_FILTER_MODE));
}

bool pipeRtdConfigMatches(uint8_t expectedConfig) {
  const uint8_t actualConfig = pipeRtdReadRegister(max31865_rtd::kRegisterConfig);
  const uint8_t stableMask = static_cast<uint8_t>(max31865_rtd::kConfigBias |
                                                  max31865_rtd::kConfigThreeWire |
                                                  max31865_rtd::kConfigFilter50Hz);

  return (actualConfig & stableMask) == (expectedConfig & stableMask);
}

bool pipeRtdReadRaw(uint16_t &rawRegister, uint8_t &faultStatus) {
  const uint8_t biasConfig = max31865_rtd::buildConfigByte(true,
                                                           false,
                                                           PIPE_RTD_THREE_WIRE,
                                                           false,
                                                           PIPE_RTD_FILTER_MODE);
  const uint8_t oneShotConfig = max31865_rtd::buildConfigByte(true,
                                                              true,
                                                              PIPE_RTD_THREE_WIRE,
                                                              false,
                                                              PIPE_RTD_FILTER_MODE);

  pipeRtdWriteRegister(max31865_rtd::kRegisterConfig,
                       max31865_rtd::buildConfigByte(false,
                                                     false,
                                                     PIPE_RTD_THREE_WIRE,
                                                     true,
                                                     PIPE_RTD_FILTER_MODE));
  pipeRtdWriteRegister(max31865_rtd::kRegisterConfig, biasConfig);
  delay(PIPE_RTD_BIAS_SETTLE_MS);
  if (!pipeRtdConfigMatches(biasConfig)) {
    pipeRtdDisableBias();
    return false;
  }

  pipeRtdWriteRegister(max31865_rtd::kRegisterConfig, oneShotConfig);
  delay(PIPE_RTD_CONVERSION_MS);

  pipeRtdBeginTransaction();
  SPI.transfer(max31865_rtd::kRegisterRtdMsb & 0x7FU);
  rawRegister = static_cast<uint16_t>(SPI.transfer(0x00U)) << 8;
  rawRegister |= SPI.transfer(0x00U);
  pipeRtdEndTransaction();
  faultStatus = pipeRtdReadRegister(max31865_rtd::kRegisterFaultStatus);
  pipeRtdDisableBias();
  return true;
}

bool readPipeTempRaw(float &tempC) {
  uint16_t rawRegister = 0U;
  uint16_t rawCode = 0U;
  float resistanceOhms = NAN;
  uint8_t faultStatus = 0U;

  tempC = NAN;
  pipeSensorState.faultStatus = 0U;
  pipeSensorState.driverResponding = false;

  if (!pipeRtdReadRaw(rawRegister, faultStatus)) {
    return false;
  }

  pipeSensorState.driverResponding = true;
  if ((rawRegister & 0x0001U) != 0U || faultStatus != 0U) {
    pipeSensorState.faultStatus = faultStatus;
    pipeRtdWriteRegister(max31865_rtd::kRegisterConfig,
                         max31865_rtd::buildConfigByte(false,
                                                       false,
                                                       PIPE_RTD_THREE_WIRE,
                                                       true,
                                                       PIPE_RTD_FILTER_MODE));
    return false;
  }

  rawCode = static_cast<uint16_t>(rawRegister >> 1U);
  if (!max31865_rtd::rawCodeToResistanceOhms(rawCode, PIPE_RTD_MODEL, &resistanceOhms) ||
      !max31865_rtd::resistanceOhmsToTemperatureC(resistanceOhms, PIPE_RTD_MODEL, &tempC) ||
      !isfinite(tempC) || tempC < CONTROL_CONFIG.minValidTempC ||
      tempC > CONTROL_CONFIG.maxValidTempC) {
    return false;
  }

  return true;
}

void printHex64(uint64_t value) {
  char buffer[17] = {};

  (void)snprintf(buffer,
                 sizeof(buffer),
                 "%08lX%08lX",
                 static_cast<unsigned long>(value >> 32),
                 static_cast<unsigned long>(value & 0xFFFFFFFFULL));
  Serial.print(buffer);
}

void printHexByte(uint8_t value) {
  if (value < 0x10U) {
    Serial.print('0');
  }
  Serial.print(value, HEX);
}

void printFrameBytes(const uint8_t *frame, size_t length) {
  size_t index = 0U;

  if (frame == nullptr || length == 0U) {
    Serial.println(F("(none)"));
    return;
  }

  for (index = 0U; index < length; ++index) {
    if (index > 0U) {
      Serial.print(' ');
    }
    printHexByte(frame[index]);
  }
  Serial.println();
}

void printAirNodeProbe(uint8_t nodeId, unsigned long nowMs) {
  pollAirBus(nowMs);
  const controller_can_bus::ProbeResult result =
      controller_can_bus::probeNode(airNodeCache, AIR_NODE_COUNT, nodeId, nowMs, CAN_AIR_TIMEOUT_MS);

  Serial.print(F("Probing node "));
  Serial.println(nodeId);
  if (result.status != controller_can_bus::PROBE_OK) {
    Serial.print(F("Probe result: "));
    Serial.println(controller_can_bus::probeStatusName(result.status));
    if (result.identityReceived) {
      Serial.print(F("Identity id=0x"));
      printHex64(result.sensorId);
      Serial.println();
    }
    return;
  }

  Serial.println(F("Probe result: OK"));
  Serial.print(F("Sample id=0x"));
  printHex64(result.sensorId);
  Serial.print(F(", seq="));
  Serial.print(result.sample.sequence);
  Serial.print(F(", sensorOk="));
  Serial.print(result.sample.sensorOk ? F("YES") : F("NO"));
  Serial.print(F(", temp="));
  Serial.print(result.sample.tempC, 2);
  Serial.print(F(" C, rh="));
  Serial.print(result.sample.rhPercent, 1);
  Serial.println(F(" %"));
  Serial.print(F("Age: "));
  Serial.print(result.ageMs);
  Serial.println(F(" ms"));
}

void printProbeResultDetails(uint8_t nodeId,
                             const controller_can_bus::ProbeResult &result,
                             unsigned long /*nowMs*/) {
  Serial.print(F("status="));
  Serial.println(controller_can_bus::probeStatusName(result.status));

  if (result.identityReceived) {
    Serial.print(F("identity=0x"));
    printHex64(result.sensorId);
    Serial.println();
  }

  if (result.status != controller_can_bus::PROBE_OK) {
    return;
  }

  Serial.print(F("sample id=0x"));
  printHex64(result.sensorId);
  Serial.print(F(", seq="));
  Serial.print(result.sample.sequence);
  Serial.print(F(", sensorOk="));
  Serial.print(result.sample.sensorOk ? F("YES") : F("NO"));
  Serial.print(F(", temp="));
  Serial.print(result.sample.tempC, 2);
  Serial.print(F(" C, rh="));
  Serial.print(result.sample.rhPercent, 1);
  Serial.println(F(" %"));
  Serial.print(F("age_ms="));
  Serial.println(result.ageMs);
}

bool calibrationUsableForControl(size_t sensorIndex) {
  return sensorRegistry[sensorIndex].calibration.calibrated &&
         sensorRegistry[sensorIndex].calibration.valid;
}

float correctedTempC(size_t sensorIndex) {
  float value = sensorSamples[sensorIndex].rawTempC;

  if (calibrationUsableForControl(sensorIndex)) {
    value += sensorRegistry[sensorIndex].calibration.tempOffsetC;
  }

  return value;
}

float correctedRhPercent(size_t sensorIndex) {
  float value = sensorSamples[sensorIndex].rawRhPercent;

  if (calibrationUsableForControl(sensorIndex)) {
    value += sensorRegistry[sensorIndex].calibration.rhOffsetPercent;
  }

  return value;
}

bool correctedReadingValid(size_t sensorIndex) {
  const float correctedTemp = correctedTempC(sensorIndex);

  if (!sensorRegistry[sensorIndex].valid || !isfinite(sensorSamples[sensorIndex].rawTempC)) {
    return false;
  }

  if (!isfinite(correctedTemp)) {
    return false;
  }

  if (!sensorSamples[sensorIndex].hasHumidity) {
    return true;
  }

  {
    const float correctedRh = correctedRhPercent(sensorIndex);
    return isfinite(sensorSamples[sensorIndex].rawRhPercent) && isfinite(correctedRh) &&
           correctedRh >= 0.0f && correctedRh <= 100.0f;
  }
}

// Drives one controller-side LED per sensor slot so the enclosure shows what the Mega sees.
void updateSensorStatusLeds(unsigned long nowMs) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    digitalWrite(SENSOR_STATUS_LED_PINS[sensorIndex],
                 sensor_status_view::sensorStatusLedOn(sensorRegistry,
                                                       SENSOR_SLOT_COUNT,
                                                       sensorIndex,
                                                       nowMs,
                                                       SENSOR_STATUS_LED_BLINK_PERIOD_MS)
                     ? HIGH
                     : LOW);
  }
}

void updateLcdSensorStatusGlyph(unsigned long nowMs) {
  sensor_status_view::buildLcdSensorStatusGlyph(sensorRegistry,
                                                SENSOR_SLOT_COUNT,
                                                nowMs,
                                                SENSOR_STATUS_LED_BLINK_PERIOD_MS,
                                                lcdSensorStatusGlyph);
  if (!lcdSensorStatusGlyphInitialized ||
      memcmp(lcdSensorStatusGlyphCache, lcdSensorStatusGlyph, sizeof(lcdSensorStatusGlyph)) != 0) {
    memcpy(lcdSensorStatusGlyphCache, lcdSensorStatusGlyph, sizeof(lcdSensorStatusGlyph));
    lcd.createChar(LCD_SENSOR_STATUS_CHAR_SLOT, lcdSensorStatusGlyphCache);
    lcdSensorStatusGlyphInitialized = true;
  }
}

void drawLcdSensorStatusGlyph(unsigned long nowMs) {
  updateLcdSensorStatusGlyph(nowMs);
  lcd.setCursor(LCD_SENSOR_STATUS_COLUMN, 0U);
  lcd.write(LCD_SENSOR_STATUS_CHAR_SLOT);
}

void writeLcdSensorStatusGlyphPattern(bool zone1On, bool zone2On, bool zone3On, bool pipeOn) {
  sensor_status_view::buildLcdSensorStatusGlyphFromFlags(zone1On,
                                                         zone2On,
                                                         zone3On,
                                                         pipeOn,
                                                         lcdSensorStatusGlyph);
  memcpy(lcdSensorStatusGlyphCache, lcdSensorStatusGlyph, sizeof(lcdSensorStatusGlyph));
  lcd.createChar(LCD_SENSOR_STATUS_CHAR_SLOT, lcdSensorStatusGlyphCache);
  lcdSensorStatusGlyphInitialized = true;
  lcd.setCursor(LCD_SENSOR_STATUS_COLUMN, 0U);
  lcd.write(LCD_SENSOR_STATUS_CHAR_SLOT);
}

void runLcdSensorStatusSelfTest() {
  lcd.clear();
  writeLcdSensorStatusGlyphPattern(true, false, false, false);
  delay(LCD_SENSOR_STATUS_SELF_TEST_STEP_MS);
  writeLcdSensorStatusGlyphPattern(false, true, false, false);
  delay(LCD_SENSOR_STATUS_SELF_TEST_STEP_MS);
  writeLcdSensorStatusGlyphPattern(false, false, true, false);
  delay(LCD_SENSOR_STATUS_SELF_TEST_STEP_MS);
  writeLcdSensorStatusGlyphPattern(false, false, false, true);
  delay(LCD_SENSOR_STATUS_SELF_TEST_STEP_MS);
  writeLcdSensorStatusGlyphPattern(true, true, true, true);
  delay(LCD_SENSOR_STATUS_SELF_TEST_ALL_ON_MS);
  writeLcdSensorStatusGlyphPattern(false, false, false, false);
}

// Runs a short startup chase so lid wiring can be verified before any sensors are connected.
void runSensorStatusLedSelfTest() {
  size_t sensorIndex = 0U;
  size_t activeIndex = 0U;

  for (activeIndex = 0U; activeIndex < SENSOR_SLOT_COUNT; ++activeIndex) {
    for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
      digitalWrite(SENSOR_STATUS_LED_PINS[sensorIndex], sensorIndex == activeIndex ? HIGH : LOW);
    }
    delay(SENSOR_STATUS_LED_SELF_TEST_STEP_MS);
  }

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    digitalWrite(SENSOR_STATUS_LED_PINS[sensorIndex], HIGH);
  }
  delay(SENSOR_STATUS_LED_SELF_TEST_ALL_ON_MS);

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    digitalWrite(SENSOR_STATUS_LED_PINS[sensorIndex], LOW);
  }
}

void initializeSensorRegistry() {
  size_t sensorIndex = 0U;
  static const commissioning::SensorRole kFixedRoles[SENSOR_SLOT_COUNT] = {
      commissioning::SENSOR_ROLE_AIR_ZONE_1,
      commissioning::SENSOR_ROLE_AIR_ZONE_2,
      commissioning::SENSOR_ROLE_AIR_ZONE_3,
      commissioning::SENSOR_ROLE_PIPE_TEMP,
  };

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    sensorRegistry[sensorIndex].descriptor.sensorId =
        sensorIndex < AIR_NODE_COUNT ? 0ULL : (0x4D33313836350000ULL + PIN_PIPE_RTD_CS);
    sensorRegistry[sensorIndex].descriptor.location.busType =
        sensorIndex < AIR_NODE_COUNT ? commissioning::BUS_TYPE_CAN
                                     : commissioning::BUS_TYPE_SPI;
    sensorRegistry[sensorIndex].descriptor.location.busIndex = 0U;
    sensorRegistry[sensorIndex].descriptor.location.channelIndex = 0U;
    sensorRegistry[sensorIndex].descriptor.location.address =
        sensorIndex < AIR_NODE_COUNT ? static_cast<uint8_t>(sensorIndex + 1U)
                                     : PIN_PIPE_RTD_CS;
    sensorRegistry[sensorIndex].descriptor.location.slotIndex =
        static_cast<uint8_t>(sensorIndex);
    sensorRegistry[sensorIndex].capability =
        sensorIndex == 3U ? commissioning::SENSOR_CAPABILITY_TEMP_ONLY
                          : commissioning::SENSOR_CAPABILITY_TEMP_RH;
    sensorRegistry[sensorIndex].role = kFixedRoles[sensorIndex];
    sensorRegistry[sensorIndex].calibration.tempOffsetC = 0.0f;
    sensorRegistry[sensorIndex].calibration.rhOffsetPercent = 0.0f;
    sensorRegistry[sensorIndex].calibration.calibrated = false;
    sensorRegistry[sensorIndex].calibration.valid = false;
    sensorRegistry[sensorIndex].discovered = false;
    sensorRegistry[sensorIndex].valid = false;
    sensorRegistry[sensorIndex].commissioned = true;

    sensorSamples[sensorIndex].rawTempC = NAN;
    sensorSamples[sensorIndex].rawRhPercent = NAN;
    sensorSamples[sensorIndex].hasHumidity =
        sensorRegistry[sensorIndex].capability == commissioning::SENSOR_CAPABILITY_TEMP_RH;
  }
}

void resetSensorSlotReading(size_t sensorIndex) {
  sensorSamples[sensorIndex].rawTempC = NAN;
  sensorSamples[sensorIndex].rawRhPercent = NAN;
  sensorRegistry[sensorIndex].discovered = false;
  sensorRegistry[sensorIndex].valid = false;
}

void refreshAirNodeReading(size_t sensorIndex) {
  float tempC = NAN;
  float rhPercent = NAN;
  bool readOk = false;
  bool identityAvailable = false;

  sensorSamples[sensorIndex].hasHumidity = true;
  identityAvailable = airNodeCache[sensorIndex].identityReceived;
  if (identityAvailable && airNodeCache[sensorIndex].sensorId != 0ULL) {
    sensorRegistry[sensorIndex].descriptor.sensorId = airNodeCache[sensorIndex].sensorId;
  }

  sensorRegistry[sensorIndex].discovered = identityAvailable;
  if (!sensorRegistry[sensorIndex].discovered) {
    return;
  }

  readOk = readAirZoneRaw(static_cast<uint8_t>(sensorIndex), tempC, rhPercent);
  if (!readOk) {
    return;
  }

  sensorSamples[sensorIndex].rawTempC = tempC;
  sensorSamples[sensorIndex].rawRhPercent = rhPercent;
  sensorRegistry[sensorIndex].valid = true;
}

void refreshPipeSensorReading(size_t sensorIndex) {
  float tempC = NAN;
  const bool readOk = readPipeTempRaw(tempC);

  sensorSamples[sensorIndex].hasHumidity = false;
  sensorRegistry[sensorIndex].discovered = readOk;
  if (!readOk) {
    return;
  }

  sensorSamples[sensorIndex].rawTempC = tempC;
  sensorRegistry[sensorIndex].valid = true;
}

// Polls the pipe RTD and CAN air nodes and refreshes the live sensor registry.
void refreshSensorReadings() {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    resetSensorSlotReading(sensorIndex);

    if (sensorRegistry[sensorIndex].capability == commissioning::SENSOR_CAPABILITY_TEMP_RH &&
        sensorIndex < AIR_NODE_COUNT) {
      refreshAirNodeReading(sensorIndex);
      continue;
    }

    refreshPipeSensorReading(sensorIndex);
  }
}

bool initializePersistentStorage() {
#if defined(ESP8266) || defined(ESP32)
  return EEPROM.begin(sizeof(sensor_calibration_storage::Image));
#else
  return true;
#endif
}

size_t discoveredSensorCount() {
  return controller_sensor_helpers::discoveredSensorCount(sensorRegistry, SENSOR_SLOT_COUNT);
}

bool stageMenuHasViewData() {
  return discoveredSensorCount() > 0U;
}

bool stageMenuHasCalibration() {
  return findNextMenuCalibratableSensor(SENSOR_SLOT_COUNT - 1U, true) !=
         commissioning::kInvalidSensorIndex;
}

void setLcdNotice(const char *line0, const char *line1, unsigned long nowMs);
bool lcdNoticeActive(unsigned long nowMs);
void clearLcdNotice();

bool menuAllowedInCurrentState() {
  return lcdUiState.menuAllowed(commissioningState.mode, commissioningState.faultCode);
}

bool overlayActive() {
  return lcdUiState.overlayActive();
}

bool manualCalibrationModeActive() {
  return lcdUiState.manualCalibrationModeActive();
}

bool pwmCalibrationModeActive() {
  return lcdUiState.pwmCalibrationModeActive();
}

size_t activeCalibrationSensorIndex() {
  return lcdUiState.activeCalibrationSensorIndex(commissioningState.mode,
                                                 commissioningState.highlightedSensorIndex);
}

size_t findNextDiscoveredSensor(size_t startAfterIndex, bool forward) {
  return controller_sensor_helpers::findNextDiscoveredSensor(sensorRegistry,
                                                             SENSOR_SLOT_COUNT,
                                                             startAfterIndex,
                                                             forward);
}

bool sensorCanBeMenuCalibrated(size_t sensorIndex) {
  return controller_sensor_helpers::sensorCanBeMenuCalibrated(sensorRegistry,
                                                              SENSOR_SLOT_COUNT,
                                                              sensorIndex);
}

size_t findNextMenuCalibratableSensor(size_t startAfterIndex, bool forward) {
  return controller_sensor_helpers::findNextMenuCalibratableSensor(sensorRegistry,
                                                                   SENSOR_SLOT_COUNT,
                                                                   startAfterIndex,
                                                                   forward);
}

void closeOverlayMenu() {
  lcdUiState.closeOverlay();
  lcdView.invalidate();
}

void openStageMenu() {
  clearLcdNotice();
  lcdUiState.openStageMenu(stageMenuHasViewData(), stageMenuHasCalibration());
  lcdView.invalidate();
}

void openSensorView() {
  const size_t sensorIndex = findNextDiscoveredSensor(SENSOR_SLOT_COUNT - 1U, true);

  if (sensorIndex == commissioning::kInvalidSensorIndex) {
    setLcdNotice("No live sensor", "data yet", millis());
    closeOverlayMenu();
    return;
  }

  lcdUiState.openSensorView(sensorIndex);
  lcdView.invalidate();
}

void openCalibrationSelectionMenu() {
  const size_t sensorIndex =
      findNextMenuCalibratableSensor(SENSOR_SLOT_COUNT - 1U, true);

  if (sensorIndex == commissioning::kInvalidSensorIndex) {
    setLcdNotice("No live staged", "sensor to cal", millis());
    closeOverlayMenu();
    return;
  }

  lcdUiState.openCalibrationSelection(sensorIndex);
  lcdView.invalidate();
}

void openManualCalibrationEditor(size_t sensorIndex) {
  if (!sensorCanBeMenuCalibrated(sensorIndex)) {
    setLcdNotice("Sensor not live", "cannot cal", millis());
    return;
  }

  lcdUiState.openManualCalibrationEditor(sensorIndex);
  lcdView.invalidate();
}

void writePwmCalibrationOutput() {
  actuatorTelemetry.commandVoltageV = PWM_CALIBRATION_TARGET_VOLTAGE_V;
  actuatorTelemetry.commandVoltageValid = true;
  actuatorTelemetry.pwmDuty = PWM_CALIBRATION_DUTY;
  analogWrite(PIN_ACTUATOR_COMMAND_PWM, actuatorTelemetry.pwmDuty);
}

void openPwmCalibrationMode() {
  lcdUiState.openPwmCalibration();
  lcdView.invalidate();
  writePwmCalibrationOutput();
}

void moveStageMenuSelection(bool forward) {
  lcdUiState.moveStageMenu(forward, stageMenuHasViewData(), stageMenuHasCalibration());
  lcdView.invalidate();
}

bool handleStageMenuOverlayEvent(const UiInputEvent &event) {
  const StageMenuItem currentMenuItem = stageMenuItemFromIndex(lcdUiState.menuIndex);

  if (!event.shortPress) {
    return true;
  }

  if (event.key == UI_KEY_LEFT) {
    closeOverlayMenu();
    return true;
  }

  if (event.key == UI_KEY_UP) {
    moveStageMenuSelection(false);
    return true;
  }

  if (event.key == UI_KEY_DOWN || event.key == UI_KEY_RIGHT) {
    moveStageMenuSelection(true);
    return true;
  }

  if (event.key != UI_KEY_SELECT) {
    return true;
  }

  switch (currentMenuItem) {
    case STAGE_MENU_VIEW_DATA:
      openSensorView();
      break;
    case STAGE_MENU_CALIBRATE:
      openCalibrationSelectionMenu();
      break;
    case STAGE_MENU_CAL_PWM:
      openPwmCalibrationMode();
      break;
    case STAGE_MENU_EXIT:
    default:
      closeOverlayMenu();
      break;
  }

  return true;
}

bool handleSensorViewOverlayEvent(const UiInputEvent &event) {
  if (!event.shortPress) {
    return true;
  }

  if (event.key == UI_KEY_LEFT) {
    openStageMenu();
    return true;
  }

  if (event.key == UI_KEY_UP || event.key == UI_KEY_RIGHT || event.key == UI_KEY_SELECT) {
    const size_t nextIndex = findNextDiscoveredSensor(lcdUiState.browseSensorIndex, true);
    if (nextIndex != commissioning::kInvalidSensorIndex) {
      lcdUiState.browseSensorIndex = nextIndex;
    }
    return true;
  }

  if (event.key == UI_KEY_DOWN) {
    const size_t nextIndex = findNextDiscoveredSensor(lcdUiState.browseSensorIndex, false);
    if (nextIndex != commissioning::kInvalidSensorIndex) {
      lcdUiState.browseSensorIndex = nextIndex;
    }
  }

  return true;
}

bool handleCalibrationSelectionOverlayEvent(const UiInputEvent &event) {
  if (!event.shortPress) {
    return true;
  }

  if (event.key == UI_KEY_LEFT) {
    openStageMenu();
    return true;
  }

  if (event.key == UI_KEY_UP || event.key == UI_KEY_RIGHT) {
    const size_t nextIndex = findNextMenuCalibratableSensor(lcdUiState.browseSensorIndex, true);
    if (nextIndex != commissioning::kInvalidSensorIndex) {
      lcdUiState.browseSensorIndex = nextIndex;
    }
    return true;
  }

  if (event.key == UI_KEY_DOWN) {
    const size_t nextIndex = findNextMenuCalibratableSensor(lcdUiState.browseSensorIndex, false);
    if (nextIndex != commissioning::kInvalidSensorIndex) {
      lcdUiState.browseSensorIndex = nextIndex;
    }
    return true;
  }

  if (event.key == UI_KEY_SELECT) {
    openManualCalibrationEditor(lcdUiState.browseSensorIndex);
  }

  return true;
}

bool handleCalibrationEditorOverlayEvent(const UiInputEvent &event, unsigned long nowMs) {
  syncCalibrationEditor();
  if (event.longPress && event.key == UI_KEY_LEFT) {
    lcdUiState.returnToCalibrationSelection(activeCalibrationSensorIndex());
    lcdView.invalidate();
    return true;
  }

  if (event.longPress && event.key == UI_KEY_SELECT &&
      activeCalibrationSensorIndex() < SENSOR_SLOT_COUNT) {
    clearCalibration(activeCalibrationSensorIndex());
    initializeCalibrationEditor();
    commissioningState.statusDirty = true;
    maybePrintSystemInventoryStatus();
    return true;
  }

  if (!event.shortPress) {
    return true;
  }

  switch (event.key) {
    case UI_KEY_LEFT:
      moveCalibrationField(false);
      break;
    case UI_KEY_RIGHT:
      moveCalibrationField(true);
      break;
    case UI_KEY_UP:
      adjustCalibrationReference(true);
      break;
    case UI_KEY_DOWN:
      adjustCalibrationReference(false);
      break;
    case UI_KEY_SELECT:
      saveCalibrationFromLcd(nowMs);
      maybePrintSystemInventoryStatus();
      break;
    case UI_KEY_NONE:
    default:
      break;
  }

  return true;
}

bool handlePwmCalibrationOverlayEvent(const UiInputEvent &event, unsigned long nowMs) {
  if (!event.shortPress) {
    return true;
  }

  if (event.key == UI_KEY_LEFT) {
    forceActuatorWarmSafe(nowMs);
    openStageMenu();
  }

  return true;
}

bool handleOverlayUiEvent(const UiInputEvent &event, unsigned long nowMs) {
  if (!overlayActive()) {
    return false;
  }

  switch (lcdUiState.overlayMode) {
    case UI_OVERLAY_STAGE_MENU:
      return handleStageMenuOverlayEvent(event);
    case UI_OVERLAY_SENSOR_VIEW:
      return handleSensorViewOverlayEvent(event);
    case UI_OVERLAY_CAL_SELECT:
      return handleCalibrationSelectionOverlayEvent(event);
    case UI_OVERLAY_CAL_EDIT:
      return handleCalibrationEditorOverlayEvent(event, nowMs);
    case UI_OVERLAY_PWM_CAL:
      return handlePwmCalibrationOverlayEvent(event, nowMs);
    case UI_OVERLAY_NONE:
    default:
      return false;
  }
}

bool handleReadyPageUiEvent(const UiInputEvent &event) {
  if (!event.shortPress) {
    return false;
  }

  if (event.key == UI_KEY_LEFT) {
    advanceDisplayPage(false);
    return true;
  }

  if (event.key == UI_KEY_RIGHT || event.key == UI_KEY_UP || event.key == UI_KEY_DOWN ||
      event.key == UI_KEY_SELECT) {
    advanceDisplayPage(true);
    return true;
  }

  return false;
}

char *skipLeadingSpaces(char *cursor) {
  while (cursor != nullptr && *cursor == ' ') {
    ++cursor;
  }

  return cursor;
}

bool handleProbeCommand(char *cursor, unsigned long nowMs) {
  unsigned long parsedNodeId = 1UL;
  char *probeCursor = skipLeadingSpaces(cursor + 5);
  char *endPtr = nullptr;

  if (*probeCursor != '\0') {
    parsedNodeId = strtoul(probeCursor, &endPtr, 10);
    endPtr = skipLeadingSpaces(endPtr);
    if (endPtr == probeCursor || (endPtr != nullptr && *endPtr != '\0')) {
      Serial.println(F("Usage: probe [1-3]"));
      return true;
    }
  }

  if (parsedNodeId < 1UL || parsedNodeId > AIR_NODE_COUNT) {
    Serial.println(F("Probe rejected: node id must be 1-3"));
    return true;
  }

  printAirNodeProbe(static_cast<uint8_t>(parsedNodeId), nowMs);
  commissioningState.statusDirty = true;
  return true;
}

bool handleBusDiagCommand(char *cursor) {
  char *diagCursor = skipLeadingSpaces(cursor + 7);
  char *endPtr = nullptr;
  unsigned long parsedNodeId = 0UL;
  unsigned long probeCount = BUS_DIAG_DEFAULT_COUNT;
  unsigned long delayMs = BUS_DIAG_DEFAULT_DELAY_MS;
  unsigned long attempt = 0UL;

  if (*diagCursor == '\0') {
    Serial.println(F("Usage: busdiag <1-3> [count] [delay_ms]"));
    return true;
  }

  parsedNodeId = strtoul(diagCursor, &endPtr, 10);
  endPtr = skipLeadingSpaces(endPtr);
  if (endPtr == diagCursor || parsedNodeId < 1UL || parsedNodeId > AIR_NODE_COUNT) {
    Serial.println(F("Usage: busdiag <1-3> [count] [delay_ms]"));
    return true;
  }

  if (endPtr != nullptr && *endPtr != '\0') {
    diagCursor = endPtr;
    probeCount = strtoul(diagCursor, &endPtr, 10);
    endPtr = skipLeadingSpaces(endPtr);
    if (endPtr == diagCursor || probeCount == 0UL || probeCount > BUS_DIAG_MAX_COUNT) {
      Serial.println(F("Bus diag rejected: count must be 1-50"));
      return true;
    }
  }

  if (endPtr != nullptr && *endPtr != '\0') {
    diagCursor = endPtr;
    delayMs = strtoul(diagCursor, &endPtr, 10);
    endPtr = skipLeadingSpaces(endPtr);
    if (endPtr == diagCursor || delayMs > BUS_DIAG_MAX_DELAY_MS) {
      Serial.println(F("Bus diag rejected: delay must be 0-2000 ms"));
      return true;
    }
  }

  if (endPtr != nullptr && *endPtr != '\0') {
    Serial.println(F("Usage: busdiag <1-3> [count] [delay_ms]"));
    return true;
  }

  Serial.print(F("Bus diag node "));
  Serial.print(parsedNodeId);
  Serial.print(F(" on CAN (CS="));
  Serial.print(PIN_CAN_CS);
  Serial.print(F(", INT="));
  Serial.print(PIN_CAN_INT);
  Serial.print(F("), count="));
  Serial.print(probeCount);
  Serial.print(F(", delay="));
  Serial.print(delayMs);
  Serial.println(F(" ms"));

  for (attempt = 0UL; attempt < probeCount; ++attempt) {
    const unsigned long probeNowMs = millis();
    const controller_can_bus::ProbeResult result = (
        pollAirBus(probeNowMs),
        controller_can_bus::probeNode(airNodeCache,
                                      AIR_NODE_COUNT,
                                      static_cast<uint8_t>(parsedNodeId),
                                      probeNowMs,
                                      CAN_AIR_TIMEOUT_MS));

    Serial.print(F("#"));
    Serial.print(attempt + 1UL);
    Serial.print(F(" t="));
    Serial.print(probeNowMs);
    Serial.print(F("ms "));
    printProbeResultDetails(static_cast<uint8_t>(parsedNodeId), result, probeNowMs);

    if ((attempt + 1UL) < probeCount && delayMs > 0UL) {
      delay(delayMs);
    }
  }

  commissioningState.statusDirty = true;
  return true;
}

bool handleWipeAirCommand(char *cursor) {
  (void)cursor;
  Serial.println(F("wipeair unsupported on CAN air nodes"));
  Serial.println(F("If you need a clean node, reflash the target CAN sketch"));
  return true;
}

bool clearCalibrationFromSerial() {
  const size_t sensorIndex = activeCalibrationSensorIndex();

  if (sensorIndex >= SENSOR_SLOT_COUNT) {
    Serial.println(F("Calibration clear rejected: no highlighted sensor"));
    return true;
  }

  clearCalibration(sensorIndex);
  if (manualCalibrationModeActive()) {
    initializeCalibrationEditor();
  }
  commissioningState.statusDirty = true;
  return true;
}

bool applyCalibrationSetCommand(char *cursor, unsigned long nowMs) {
  float referenceTempC = NAN;
  float referenceRhPercent = NAN;
  bool hasRhReference = false;
  const size_t sensorIndex = activeCalibrationSensorIndex();

  if (!parseNextFloat(cursor, referenceTempC)) {
    Serial.println(F("Calibration rejected: missing temperature reference"));
    return true;
  }

  cursor = skipLeadingSpaces(cursor);
  if (*cursor != '\0') {
    if (!parseNextFloat(cursor, referenceRhPercent)) {
      Serial.println(F("Calibration rejected: malformed RH reference"));
      return true;
    }
    hasRhReference = true;

    cursor = skipLeadingSpaces(cursor);
    if (*cursor != '\0') {
      Serial.println(F("Calibration rejected: unexpected trailing input"));
      return true;
    }
  }

  if (!applyCalibrationReference(sensorIndex,
                                 referenceTempC,
                                 hasRhReference,
                                 referenceRhPercent)) {
    return true;
  }

  (void)nowMs;
  if (manualCalibrationModeActive()) {
    lcdUiState.returnToCalibrationSelection(sensorIndex);
    lcdView.invalidate();
  }
  maybePrintSystemInventoryStatus();
  return true;
}

bool handleCommonSerialCommand(char *cursor, unsigned long nowMs) {
  if (strcmp(cursor, "show") == 0) {
    commissioningState.statusDirty = true;
    return true;
  }

  if (strcmp(cursor, "pause") == 0) {
    periodicStatusEnabled = false;
    Serial.println(F("Periodic status paused"));
    return true;
  }

  if (strcmp(cursor, "resume") == 0) {
    periodicStatusEnabled = true;
    Serial.println(F("Periodic status resumed"));
    commissioningState.statusDirty = true;
    printStatus(currentDecision);
    return true;
  }

  if (strcmp(cursor, "traceoff") == 0) {
    keypadTraceEnabled = false;
    Serial.println(F("Keypad event trace paused"));
    return true;
  }

  if (strcmp(cursor, "traceon") == 0) {
    keypadTraceEnabled = true;
    Serial.println(F("Keypad event trace resumed"));
    return true;
  }

  if (strcmp(cursor, "keypad") == 0) {
    const int analogValue = readKeypadAnalogValue();
    Serial.print(F("LCD key raw="));
    Serial.print(analogValue);
    Serial.print(F(" decoded="));
    Serial.println(shortUiKeyName(decodeKeypadKey(analogValue)));
    commissioningState.statusDirty = true;
    return true;
  }

  if (strcmp(cursor, "menu") == 0) {
    if (menuAllowedInCurrentState()) {
      openStageMenu();
    } else {
      Serial.println(F("Menu unavailable in current controller state"));
    }
    return true;
  }

  if (strncmp(cursor, "probe", 5) == 0 && (cursor[5] == '\0' || cursor[5] == ' ')) {
    return handleProbeCommand(cursor, nowMs);
  }

  if (strncmp(cursor, "busdiag", 7) == 0 && (cursor[7] == '\0' || cursor[7] == ' ')) {
    return handleBusDiagCommand(cursor);
  }

  if (strncmp(cursor, "wipeair", 7) == 0 && (cursor[7] == '\0' || cursor[7] == ' ')) {
    return handleWipeAirCommand(cursor);
  }

  if (strcmp(cursor, "wipeeeprom") == 0) {
    if (!persistentStorageReady) {
      Serial.println(F("EEPROM wipe rejected: persistent storage unavailable"));
      return true;
    }

    if (!clearPersistedCalibration()) {
      Serial.println(F("EEPROM wipe failed"));
      return true;
    }

    persistedCalibrationLoaded = false;
    clearCalibrationRecordsInMemory();
    clearLcdNotice();
    lcdView.invalidate();
    commissioningState.statusDirty = true;
    Serial.println(F("Persisted calibration EEPROM cleared"));
    return true;
  }

  return false;
}

bool commitPersistentStorage() {
#if defined(ESP8266) || defined(ESP32)
  return EEPROM.commit();
#else
  return true;
#endif
}

void clearCalibrationRecordsInMemory() {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    sensorRegistry[sensorIndex].calibration.tempOffsetC = 0.0f;
    sensorRegistry[sensorIndex].calibration.rhOffsetPercent = 0.0f;
    sensorRegistry[sensorIndex].calibration.calibrated = false;
    sensorRegistry[sensorIndex].calibration.valid = false;
  }
}

void initializeFixedControllerState() {
  commissioningState.mode = commissioning::MODE_READY;
  commissioningState.faultCode = commissioning::FAULT_NONE;
  commissioningState.activeRoleIndex = 0U;
  commissioningState.highlightedSensorIndex = commissioning::kInvalidSensorIndex;
  commissioningState.validationStartMs = 0UL;
  commissioningState.statusDirty = true;
}

bool clearPersistedCalibration() {
  sensor_calibration_storage::Image image = {};
  sensor_calibration_storage::Image verify = {};

  if (!persistentStorageReady) {
    return false;
  }

  sensor_calibration_storage::clearImage(&image);
  EEPROM.put(EEPROM_IMAGE_ADDRESS, image);
  if (!commitPersistentStorage()) {
    return false;
  }

  EEPROM.get(EEPROM_IMAGE_ADDRESS, verify);
  return !sensor_calibration_storage::validateImage(verify);
}

bool loadPersistedCalibration() {
  sensor_calibration_storage::Image image = {};

  if (!persistentStorageReady) {
    return false;
  }

  EEPROM.get(EEPROM_IMAGE_ADDRESS, image);
  if (!sensor_calibration_storage::restoreImage(image,
                                                sensorRegistry,
                                                SENSOR_SLOT_COUNT,
                                                MAX_CAL_TEMP_OFFSET_C,
                                                MAX_CAL_RH_OFFSET_PERCENT)) {
    return false;
  }

  persistedCalibrationLoaded = true;
  return true;
}

bool savePersistedCalibration() {
  sensor_calibration_storage::Image image = {};
  sensor_calibration_storage::Image verify = {};

  if (!persistentStorageReady) {
    return false;
  }

  if (!sensor_calibration_storage::captureImage(sensorRegistry, SENSOR_SLOT_COUNT, &image)) {
    return false;
  }

  EEPROM.put(EEPROM_IMAGE_ADDRESS, image);
  if (!commitPersistentStorage()) {
    return false;
  }

  EEPROM.get(EEPROM_IMAGE_ADDRESS, verify);
  return sensor_calibration_storage::validateImage(verify) &&
         memcmp(&image, &verify, sizeof(image)) == 0;
}

void persistCurrentCalibration(const __FlashStringHelper *successMessage,
                               const __FlashStringHelper *failureMessage) {
  if (savePersistedCalibration()) {
    if (successMessage != nullptr) {
      Serial.println(successMessage);
    }
  } else if (failureMessage != nullptr) {
    Serial.println(failureMessage);
  }
}

UiInputEvent makeUiInputEvent(UiKey key, bool shortPress, bool longPress) {
  UiInputEvent event = {};

  event.key = key;
  event.shortPress = shortPress;
  event.longPress = longPress;
  return event;
}

const char *shortUiKeyName(UiKey key) {
  switch (key) {
    case UI_KEY_RIGHT:
      return "RIGHT";
    case UI_KEY_UP:
      return "UP";
    case UI_KEY_DOWN:
      return "DOWN";
    case UI_KEY_LEFT:
      return "LEFT";
    case UI_KEY_SELECT:
      return "SELECT";
    case UI_KEY_NONE:
    default:
      return "NONE";
  }
}

int readKeypadAnalogValue() {
  return analogRead(PIN_LCD_KEYPAD);
}

UiKey decodeKeypadKey(int analogValue) {
  if (analogValue < KEYPAD_RIGHT_MAX) {
    return UI_KEY_RIGHT;
  }

  if (analogValue < KEYPAD_UP_MAX) {
    return UI_KEY_UP;
  }

  if (analogValue < KEYPAD_DOWN_MAX) {
    return UI_KEY_DOWN;
  }

  if (analogValue < KEYPAD_LEFT_MAX) {
    return UI_KEY_LEFT;
  }

  if (analogValue < KEYPAD_SELECT_MAX) {
    return UI_KEY_SELECT;
  }

  if (analogValue < KEYPAD_NONE_MIN) {
    return UI_KEY_SELECT;
  }

  return UI_KEY_NONE;
}

UiInputEvent pollKeypadEvent(unsigned long nowMs) {
  const int analogValue = readKeypadAnalogValue();
  const UiKey rawKey = decodeKeypadKey(analogValue);

  if (rawKey != keypadState.lastRawKey) {
    keypadState.lastRawKey = rawKey;
    keypadState.lastRawChangeMs = nowMs;
  }

  if ((nowMs - keypadState.lastRawChangeMs) < KEYPAD_DEBOUNCE_MS) {
    return makeUiInputEvent(UI_KEY_NONE, false, false);
  }

  if (rawKey != keypadState.stableKey) {
    const UiKey previousKey = keypadState.stableKey;

    keypadState.stableKey = rawKey;
    if (keypadTraceEnabled) {
      Serial.print(F("LCD key raw="));
      Serial.print(analogValue);
      Serial.print(F(" decoded="));
      Serial.println(shortUiKeyName(rawKey));
    }
    if (rawKey != UI_KEY_NONE) {
      keypadState.pressStartMs = nowMs;
      keypadState.longPressReported = false;
      return makeUiInputEvent(UI_KEY_NONE, false, false);
    }

    if (previousKey != UI_KEY_NONE && !keypadState.longPressReported) {
      return makeUiInputEvent(previousKey, true, false);
    }

    return makeUiInputEvent(UI_KEY_NONE, false, false);
  }

  if (keypadState.stableKey != UI_KEY_NONE && !keypadState.longPressReported &&
      (nowMs - keypadState.pressStartMs) >= KEYPAD_LONG_PRESS_MS) {
    keypadState.longPressReported = true;
    if (keypadTraceEnabled) {
      Serial.print(F("LCD long press="));
      Serial.println(shortUiKeyName(keypadState.stableKey));
    }
    return makeUiInputEvent(keypadState.stableKey, false, true);
  }

  return makeUiInputEvent(UI_KEY_NONE, false, false);
}

float actuatorFeedbackPercentFromAdc(int rawAdcCount) {
  const float sensedVoltageV =
      (static_cast<float>(rawAdcCount) * ACTUATOR_FEEDBACK_ADC_VREF) / 1023.0f;
  const float feedbackVoltageV = sensedVoltageV * ACTUATOR_FEEDBACK_DIVIDER_GAIN;

  actuatorTelemetry.measuredVoltageV = feedbackVoltageV;
  return feedbackVoltageV * 10.0f;
}

float warmSafeCommandVoltageV() {
  return ACTUATOR_WARMER_ON_HIGH_SIGNAL ? 10.0f : 0.0f;
}

uint8_t pwmDutyForCommandVoltage(float commandVoltageV) {
  const float safeCommandVoltageV =
      isfinite(commandVoltageV) ? commandVoltageV : (ACTUATOR_WARMER_ON_HIGH_SIGNAL ? 10.0f : 0.0f);
  const float pwmStageVoltageV = safeCommandVoltageV / ACTUATOR_OUTPUT_STAGE_GAIN;
  float duty = (pwmStageVoltageV / ACTUATOR_COMMAND_PWM_VREF) * 255.0f;

  if (duty < 0.0f) {
    duty = 0.0f;
  } else if (duty > 255.0f) {
    duty = 255.0f;
  }

  return static_cast<uint8_t>(duty + 0.5f);
}

bool actuatorOutputInvariantHolds() {
  const float commandVoltageV = modulating_actuator::commandVoltageV(actuatorState);

  if (!actuatorTelemetry.commandVoltageValid || !isfinite(commandVoltageV) || commandVoltageV < 0.0f ||
      commandVoltageV > 10.0f) {
    return false;
  }

  if (fabsf(commandVoltageV - actuatorTelemetry.commandVoltageV) > 0.01f) {
    return false;
  }

  return actuatorTelemetry.pwmDuty == pwmDutyForCommandVoltage(commandVoltageV);
}

// Converts the current actuator target into a PWM output for the external 0-10V stage.
bool writeActuatorCommand() {
  const float commandVoltageV = modulating_actuator::commandVoltageV(actuatorState);
  const bool commandVoltageValid =
      isfinite(commandVoltageV) && commandVoltageV >= 0.0f && commandVoltageV <= 10.0f;
  const float appliedVoltageV = commandVoltageValid ? commandVoltageV : warmSafeCommandVoltageV();

  actuatorTelemetry.commandVoltageV = commandVoltageV;
  actuatorTelemetry.commandVoltageValid = commandVoltageValid;
  actuatorTelemetry.pwmDuty = pwmDutyForCommandVoltage(appliedVoltageV);
  analogWrite(PIN_ACTUATOR_COMMAND_PWM, actuatorTelemetry.pwmDuty);
  return commandVoltageValid;
}

bool actuatorStateConsistent() {
  return modulating_actuator::invariantsHold(ACTUATOR_CONFIG, actuatorState);
}

void forceActuatorWarmSafe(unsigned long nowMs) {
  modulating_actuator::forceWarmPosition(ACTUATOR_CONFIG, nowMs, &actuatorState);
  (void)writeActuatorCommand();
}

// Samples actuator feedback voltage and updates actuator tracking state.
void sampleActuatorFeedback(unsigned long nowMs) {
  const int rawAdcCount = analogRead(PIN_ACTUATOR_FEEDBACK);
  const float feedbackPercent = actuatorFeedbackPercentFromAdc(rawAdcCount);
  const bool feedbackWiringValid =
      !isnan(feedbackPercent) && !isinf(feedbackPercent) && feedbackPercent >= -1.0f &&
      feedbackPercent <= 101.0f;

  actuatorTelemetry.measuredPercent = feedbackPercent;
  actuatorTelemetry.feedbackWiringValid = feedbackWiringValid;
  modulating_actuator::updateFeedback(ACTUATOR_CONFIG,
                                      feedbackPercent,
                                      feedbackWiringValid,
                                      nowMs,
                                      &actuatorState);
}

// Applies one policy decision to the actuator layer, forcing warm-safe on invalid output paths.
bool applyDecisionToActuator(const dewpoint_policy::Inputs &inputs,
                             const dewpoint_policy::Decision &decision,
                             unsigned long nowMs) {
  if (!dewpoint_policy::decisionSafeForControl(CONTROL_CONFIG, inputs, decision)) {
    forceActuatorWarmSafe(nowMs);
    return false;
  }

  if (decision.faultCode != dewpoint_policy::FAULT_NONE) {
    forceActuatorWarmSafe(nowMs);
    return false;
  }

  modulating_actuator::applyDecision(ACTUATOR_CONFIG, decision.command, nowMs, &actuatorState);
  if (!actuatorStateConsistent() || actuatorState.faultCode != modulating_actuator::FAULT_NONE) {
    forceActuatorWarmSafe(nowMs);
    return false;
  }

  if (!writeActuatorCommand()) {
    forceActuatorWarmSafe(nowMs);
    return false;
  }

  if (!actuatorOutputInvariantHolds()) {
    forceActuatorWarmSafe(nowMs);
    return false;
  }

  return true;
}

// Collects the current fixed-slot air/pipe inputs for the dew-point policy engine.
dewpoint_policy::Inputs collectInputs() {
  dewpoint_policy::Inputs inputs = {};

  inputs.coolingDemandActive = coolingDemandActive();
  inputs.commissioningComplete = true;
  inputs.pipeTempC = NAN;
  inputs.pipeTempValid = false;

  if (correctedReadingValid(0U)) {
    inputs.airZones[0].airTempC = correctedTempC(0U);
    inputs.airZones[0].rhPercent = correctedRhPercent(0U);
    inputs.airZones[0].valid = true;
  }

  if (correctedReadingValid(1U)) {
    inputs.airZones[1].airTempC = correctedTempC(1U);
    inputs.airZones[1].rhPercent = correctedRhPercent(1U);
    inputs.airZones[1].valid = true;
  }

  if (correctedReadingValid(2U)) {
    inputs.airZones[2].airTempC = correctedTempC(2U);
    inputs.airZones[2].rhPercent = correctedRhPercent(2U);
    inputs.airZones[2].valid = true;
  }

  if (correctedReadingValid(3U)) {
    inputs.pipeTempC = correctedTempC(3U);
    inputs.pipeTempValid = true;
  }

  return inputs;
}

void printZoneStatus(uint8_t zoneIndex, const dewpoint_policy::AirZoneStatus &zoneStatus) {
  Serial.print(F("Zone "));
  Serial.print(zoneIndex + 1U);
  Serial.print(F(": "));

  if (!zoneStatus.valid) {
    Serial.println(F("INVALID"));
    return;
  }

  Serial.print(F("T="));
  Serial.print(zoneStatus.airTempC, 2);
  Serial.print(F(" C, RH="));
  Serial.print(zoneStatus.rhPercent, 1);
  Serial.print(F(" %, DP="));
  Serial.print(zoneStatus.dewPointC, 2);
  Serial.println(F(" C"));
}

void printSensorSlot(size_t sensorIndex) {
  Serial.print(F("Slot "));
  Serial.print(sensorIndex);
  Serial.print(F(": id=0x"));
  printHex64(sensorRegistry[sensorIndex].descriptor.sensorId);
  Serial.print(F(", bus="));
  Serial.print(commissioning::busTypeName(sensorRegistry[sensorIndex].descriptor.location.busType));
  Serial.print(F(", loc="));
  Serial.print(sensorRegistry[sensorIndex].descriptor.location.slotIndex);
  Serial.print(F(", cap="));
  Serial.print(commissioning::capabilityName(sensorRegistry[sensorIndex].capability));
  Serial.print(F(", role="));
  Serial.print(commissioning::roleName(sensorRegistry[sensorIndex].role));
  Serial.print(F(", discovered="));
  Serial.print(sensorRegistry[sensorIndex].discovered ? F("YES") : F("NO"));
  Serial.print(F(", valid="));
  Serial.print(sensorRegistry[sensorIndex].valid ? F("YES") : F("NO"));
  Serial.print(F(", calibrated="));
  Serial.print(sensorRegistry[sensorIndex].calibration.calibrated ? F("YES") : F("NO"));

  if (sensorRegistry[sensorIndex].calibration.calibrated) {
    Serial.print(F(", dT="));
    Serial.print(sensorRegistry[sensorIndex].calibration.tempOffsetC, 2);
    if (sensorSamples[sensorIndex].hasHumidity) {
      Serial.print(F(" C, dRH="));
      Serial.print(sensorRegistry[sensorIndex].calibration.rhOffsetPercent, 1);
      Serial.print(F(" %"));
    } else {
      Serial.print(F(" C"));
    }
  }

  if (sensorRegistry[sensorIndex].valid) {
    Serial.print(F(", rawT="));
    Serial.print(sensorSamples[sensorIndex].rawTempC, 2);
    if (sensorSamples[sensorIndex].hasHumidity) {
      Serial.print(F(" C, rawRH="));
      Serial.print(sensorSamples[sensorIndex].rawRhPercent, 1);
      Serial.print(F(" %"));
    } else {
      Serial.print(F(" C"));
    }

    if (calibrationUsableForControl(sensorIndex)) {
      Serial.print(F(", correctedT="));
      Serial.print(correctedTempC(sensorIndex), 2);
      if (sensorSamples[sensorIndex].hasHumidity) {
        Serial.print(F(" C, correctedRH="));
        Serial.print(correctedRhPercent(sensorIndex), 1);
        Serial.print(F(" %"));
      } else {
        Serial.print(F(" C"));
      }
    }
  }

  Serial.println();
}

void printSystemInventoryStatus() {
  size_t sensorIndex = 0U;

  Serial.println(F("==== SYSTEM ===="));
  Serial.println(F("Layout: fixed slots Z1, Z2, Z3, PIPE"));
  Serial.println(F("Menu: short SELECT"));
  Serial.println(F("Calibration: menu only"));

  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    printSensorSlot(sensorIndex);
  }
}

void maybePrintSystemInventoryStatus() {
  if (!commissioningState.statusDirty) {
    return;
  }

  printSystemInventoryStatus();
  commissioningState.statusDirty = false;
}

// Prints a serial status snapshot of inputs, decision, and actuator state.
void printStatus(const dewpoint_policy::Decision &decision) {
  uint8_t zoneIndex = 0U;

  Serial.println(F("-----"));
  Serial.println(F("Layout: fixed slots"));

  for (zoneIndex = 0U; zoneIndex < CONTROL_CONFIG.airZoneCount; ++zoneIndex) {
    printZoneStatus(zoneIndex, decision.airZones[zoneIndex]);
  }

  Serial.print(F("Pipe temp: "));
  if (!currentInputs.pipeTempValid) {
    if (!pipeSensorState.driverResponding) {
      Serial.println(F("INVALID (RTD no response)"));
    } else if (pipeSensorState.faultStatus != 0U) {
      Serial.print(F("INVALID (RTD fault 0x"));
      if (pipeSensorState.faultStatus < 0x10U) {
        Serial.print('0');
      }
      Serial.print(pipeSensorState.faultStatus, HEX);
      Serial.println(F(")"));
    } else {
      Serial.println(F("INVALID"));
    }
  } else {
    Serial.print(decision.pipeTempC, 2);
    Serial.println(F(" C"));
  }

  Serial.print(F("Fault: "));
  Serial.println(dewpoint_policy::faultCodeName(decision.faultCode));

  Serial.print(F("Command: "));
  Serial.println(dewpoint_policy::valveCommandName(decision.command));

  Serial.print(F("Actuator cmd: "));
  Serial.print(actuatorState.commandedPercent, 1);
  Serial.print(F("% ("));
  Serial.print(modulating_actuator::commandVoltageV(actuatorState), 2);
  Serial.println(F(" V)"));

  Serial.print(F("Actuator fb: "));
  if (!actuatorState.feedbackValid) {
    Serial.print(F("INVALID ("));
    Serial.print(modulating_actuator::faultCodeName(actuatorState.faultCode));
    Serial.println(F(")"));
  } else {
    Serial.print(actuatorState.feedbackPercent, 1);
    Serial.print(F("% ("));
    Serial.print(actuatorTelemetry.measuredVoltageV, 2);
    Serial.println(F(" V)"));
  }

  Serial.print(F("Cooling call: "));
  Serial.println(currentInputs.coolingDemandActive ? F("YES") : F("NO"));

  if (decision.faultCode != dewpoint_policy::FAULT_NONE) {
    return;
  }

  Serial.print(F("Worst dew point: "));
  Serial.print(decision.worstDewPointC, 2);
  Serial.println(F(" C"));

  Serial.print(F("Minimum safe cold temp: "));
  Serial.print(decision.minSafeColdTempC, 2);
  Serial.println(F(" C"));
}

bool parseNextFloat(char *&cursor, float &value) {
  char *endPtr = nullptr;
  double parsedValue = 0.0;

  while (*cursor == ' ') {
    ++cursor;
  }

  if (*cursor == '\0') {
    return false;
  }

  parsedValue = strtod(cursor, &endPtr);
  if (endPtr == cursor) {
    return false;
  }

  value = static_cast<float>(parsedValue);
  cursor = endPtr;
  return isfinite(value);
}

void clearCalibration(size_t sensorIndex) {
  sensorRegistry[sensorIndex].calibration.tempOffsetC = 0.0f;
  sensorRegistry[sensorIndex].calibration.rhOffsetPercent = 0.0f;
  sensorRegistry[sensorIndex].calibration.calibrated = false;
  sensorRegistry[sensorIndex].calibration.valid = false;
  persistCurrentCalibration(F("Persisted calibration saved"),
                            F("Warning: failed to save calibration"));
}

bool applyCalibrationReference(size_t sensorIndex,
                               float referenceTempC,
                               bool hasRhReference,
                               float referenceRhPercent) {
  float tempOffsetC = 0.0f;
  float rhOffsetPercent = 0.0f;

  if (sensorIndex >= SENSOR_SLOT_COUNT || !sensorRegistry[sensorIndex].valid) {
    Serial.println(F("Calibration rejected: highlighted sensor is not valid"));
    return false;
  }

  if (!isfinite(referenceTempC)) {
    Serial.println(F("Calibration rejected: invalid reference temperature"));
    return false;
  }

  if (sensorSamples[sensorIndex].hasHumidity) {
    if (!hasRhReference) {
      Serial.println(F("Calibration rejected: RH reference required for air sensor"));
      return false;
    }

    if (!isfinite(referenceRhPercent) || referenceRhPercent < 0.0f || referenceRhPercent > 100.0f) {
      Serial.println(F("Calibration rejected: RH reference must be 0-100%"));
      return false;
    }
  } else if (hasRhReference) {
    Serial.println(F("Calibration rejected: pipe sensor uses temperature only"));
    return false;
  }

  tempOffsetC = referenceTempC - sensorSamples[sensorIndex].rawTempC;
  if (fabsf(tempOffsetC) > MAX_CAL_TEMP_OFFSET_C) {
    Serial.println(F("Calibration rejected: temperature offset exceeds limit"));
    return false;
  }

  if (sensorSamples[sensorIndex].hasHumidity) {
    rhOffsetPercent = referenceRhPercent - sensorSamples[sensorIndex].rawRhPercent;
    if (fabsf(rhOffsetPercent) > MAX_CAL_RH_OFFSET_PERCENT) {
      Serial.println(F("Calibration rejected: RH offset exceeds limit"));
      return false;
    }
  }

  sensorRegistry[sensorIndex].calibration.tempOffsetC = tempOffsetC;
  sensorRegistry[sensorIndex].calibration.rhOffsetPercent = rhOffsetPercent;
  sensorRegistry[sensorIndex].calibration.calibrated = true;
  sensorRegistry[sensorIndex].calibration.valid = true;
  commissioningState.statusDirty = true;
  persistCurrentCalibration(F("Persisted calibration saved"),
                            F("Warning: failed to save calibration"));

  Serial.print(F("Calibration stored for slot "));
  Serial.println(sensorIndex);
  return true;
}

const char *shortRoleName(commissioning::SensorRole role) {
  switch (role) {
    case commissioning::SENSOR_ROLE_AIR_ZONE_1:
      return "Z1";
    case commissioning::SENSOR_ROLE_AIR_ZONE_2:
      return "Z2";
    case commissioning::SENSOR_ROLE_AIR_ZONE_3:
      return "Z3";
    case commissioning::SENSOR_ROLE_PIPE_TEMP:
      return "PIPE";
    case commissioning::SENSOR_ROLE_NONE:
    default:
      return "NONE";
  }
}

const char *shortFaultName(dewpoint_policy::FaultCode faultCode) {
  switch (faultCode) {
    case dewpoint_policy::FAULT_NONE:
      return "NONE";
    case dewpoint_policy::FAULT_COMMISSIONING_INCOMPLETE:
      return "COMM";
    case dewpoint_policy::FAULT_INVALID_CONFIG:
      return "CFG";
    case dewpoint_policy::FAULT_INVALID_AIR_SENSOR:
      return "AIR";
    case dewpoint_policy::FAULT_INVALID_PIPE_SENSOR:
      return "PIPE";
    case dewpoint_policy::FAULT_INTERNAL_INVARIANT:
    default:
      return "INV";
  }
}

const char *shortValveName(dewpoint_policy::ValveCommand command) {
  switch (command) {
    case dewpoint_policy::VALVE_WARMER:
      return "WARM";
    case dewpoint_policy::VALVE_COOLER:
      return "COOL";
    case dewpoint_policy::VALVE_HOLD:
    default:
      return "HOLD";
  }
}

int roundedPercent(float value) {
  if (isnan(value) || isinf(value)) {
    return -1;
  }

  if (value < 0.0f) {
    value = 0.0f;
  } else if (value > 100.0f) {
    value = 100.0f;
  }

  return static_cast<int>(value + 0.5f);
}

void formatLcdFloat(char *buffer,
                    size_t bufferSize,
                    float value,
                    signed char minWidth = 4,
                    unsigned char decimals = 1,
                    const char *invalidText = "INV") {
  if (buffer == nullptr || bufferSize == 0U) {
    return;
  }

  buffer[0] = '\0';
  if (!isfinite(value)) {
    (void)snprintf(buffer, bufferSize, "%s", invalidText);
    return;
  }

  dtostrf(static_cast<double>(value), minWidth, decimals, buffer);
}

void setLcdNotice(const char *line0, const char *line1, unsigned long nowMs) {
  lcdView.showTimedMessage(line0, line1, nowMs, LCD_NOTICE_DURATION_MS);
}

bool lcdNoticeActive(unsigned long nowMs) {
  return lcdView.timedMessageActive(nowMs);
}

void clearLcdNotice() {
  lcdView.clearTimedMessage();
  lcdView.invalidate();
}

void advanceDisplayPage(bool forward) {
  lcdUiState.advanceDisplayPage(forward);
  lcdView.invalidate();
}

void initializeCalibrationEditor() {
  const size_t sensorIndex = activeCalibrationSensorIndex();

  lcdUiState.calibrationSensorIndex = sensorIndex;
  lcdUiState.calibrationField = CAL_FIELD_TEMP;
  lcdUiState.calibrationEditorActive = false;
  lcdUiState.pendingTempC = NAN;
  lcdUiState.pendingRhPercent = NAN;

  if (sensorIndex >= SENSOR_SLOT_COUNT || !sensorRegistry[sensorIndex].valid) {
    return;
  }

  lcdUiState.pendingTempC = calibrationUsableForControl(sensorIndex) ? correctedTempC(sensorIndex)
                                                                     : sensorSamples[sensorIndex].rawTempC;
  if (sensorSamples[sensorIndex].hasHumidity) {
    lcdUiState.pendingRhPercent =
        calibrationUsableForControl(sensorIndex) ? correctedRhPercent(sensorIndex)
                                                 : sensorSamples[sensorIndex].rawRhPercent;
  }
  lcdUiState.calibrationEditorActive = true;
}

void syncCalibrationEditor() {
  if (!manualCalibrationModeActive()) {
    lcdUiState.calibrationSensorIndex = commissioning::kInvalidSensorIndex;
    lcdUiState.calibrationEditorActive = false;
    return;
  }

  if (!lcdUiState.calibrationEditorActive ||
      lcdUiState.calibrationSensorIndex != activeCalibrationSensorIndex()) {
    initializeCalibrationEditor();
  }
}

void moveCalibrationField(bool forward) {
  const size_t sensorIndex = activeCalibrationSensorIndex();
  const bool hasHumidity =
      sensorIndex < SENSOR_SLOT_COUNT && sensorSamples[sensorIndex].hasHumidity;

  if (!hasHumidity) {
    lcdUiState.calibrationField = CAL_FIELD_TEMP;
    return;
  }

  if (forward) {
    lcdUiState.calibrationField = lcdUiState.calibrationField == CAL_FIELD_TEMP
                                      ? CAL_FIELD_RH
                                      : CAL_FIELD_TEMP;
    return;
  }

  lcdUiState.calibrationField = lcdUiState.calibrationField == CAL_FIELD_RH
                                    ? CAL_FIELD_TEMP
                                    : CAL_FIELD_RH;
}

void adjustCalibrationReference(bool increase) {
  if (!lcdUiState.calibrationEditorActive) {
    return;
  }

  if (lcdUiState.calibrationField == CAL_FIELD_TEMP) {
    const float delta = increase ? LCD_CAL_TEMP_STEP_C : -LCD_CAL_TEMP_STEP_C;
    lcdUiState.pendingTempC = constrain(lcdUiState.pendingTempC + delta,
                                        CONTROL_CONFIG.minValidTempC,
                                        CONTROL_CONFIG.maxValidTempC);
    return;
  }

  if (!isfinite(lcdUiState.pendingRhPercent)) {
    const size_t sensorIndex = activeCalibrationSensorIndex();
    if (sensorIndex >= SENSOR_SLOT_COUNT) {
      return;
    }
    lcdUiState.pendingRhPercent = sensorSamples[sensorIndex].rawRhPercent;
  }

  {
    const float delta = increase ? LCD_CAL_RH_STEP_PERCENT : -LCD_CAL_RH_STEP_PERCENT;
    lcdUiState.pendingRhPercent = constrain(lcdUiState.pendingRhPercent + delta, 0.0f, 100.0f);
  }
}

void saveCalibrationFromLcd(unsigned long nowMs) {
  const size_t sensorIndex = activeCalibrationSensorIndex();
  const bool hasHumidity =
      sensorIndex < SENSOR_SLOT_COUNT && sensorSamples[sensorIndex].hasHumidity;

  if (!lcdUiState.calibrationEditorActive) {
    return;
  }

  if (!applyCalibrationReference(sensorIndex,
                                 lcdUiState.pendingTempC,
                                 hasHumidity,
                                 lcdUiState.pendingRhPercent)) {
    return;
  }

  lcdUiState.calibrationEditorActive = false;
  (void)nowMs;
  lcdUiState.returnToCalibrationSelection(sensorIndex);
  lcdView.invalidate();
}

void formatSummaryScreen(char *line0, char *line1) {
  const int commandedPercent = roundedPercent(actuatorState.commandedPercent);
  const int feedbackPercent = roundedPercent(actuatorState.feedbackPercent);
  char dewPointText[8] = {};
  char pipeText[8] = {};

  if (currentDecision.faultCode == dewpoint_policy::FAULT_NONE) {
    formatLcdFloat(dewPointText, sizeof(dewPointText), currentDecision.worstDewPointC);
    formatLcdFloat(pipeText,
                   sizeof(pipeText),
                   currentInputs.pipeTempValid ? currentDecision.pipeTempC : NAN);
    (void)snprintf(line0, LCD_COLUMNS + 1U, "WDP%sC P%s", dewPointText, pipeText);
  } else {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "Fault %-4s", shortFaultName(currentDecision.faultCode));
  }

  (void)snprintf(line1,
                 LCD_COLUMNS + 1U,
                 actuatorState.feedbackValid ? "Cmd %-4s Y%02d U%02d" : "Cmd %-4s Y%02d UF!",
                 shortValveName(currentDecision.command),
                 commandedPercent,
                 feedbackPercent);
}

void formatZoneScreen(uint8_t zoneIndex, char *line0, char *line1) {
  const dewpoint_policy::AirZoneStatus &zoneStatus = currentDecision.airZones[zoneIndex];
  char tempText[8] = {};
  char rhText[8] = {};
  char dewPointText[8] = {};

  if (!zoneStatus.valid) {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "Z%u INVALID", zoneIndex + 1U);
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Need live sensor");
    return;
  }

  formatLcdFloat(tempText, sizeof(tempText), zoneStatus.airTempC);
  formatLcdFloat(rhText, sizeof(rhText), zoneStatus.rhPercent);
  formatLcdFloat(dewPointText, sizeof(dewPointText), zoneStatus.dewPointC);
  (void)snprintf(line0, LCD_COLUMNS + 1U, "Z%u %sC %s%%", zoneIndex + 1U, tempText, rhText);
  (void)snprintf(line1, LCD_COLUMNS + 1U, "DP%sC %-4s", dewPointText, shortValveName(currentDecision.command));
}

void formatPipeScreen(char *line0, char *line1) {
  const int commandedPercent = roundedPercent(actuatorState.commandedPercent);
  const int feedbackPercent = roundedPercent(actuatorState.feedbackPercent);
  char pipeText[8] = {};
  char minSafeText[8] = {};

  if (!currentInputs.pipeTempValid) {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "Pipe INVALID");
    if (!pipeSensorState.driverResponding) {
      (void)snprintf(line1, LCD_COLUMNS + 1U, "RTD no response");
    } else if (pipeSensorState.faultStatus != 0U) {
      (void)snprintf(line1, LCD_COLUMNS + 1U, "RTD fault %02X", pipeSensorState.faultStatus);
    } else {
      (void)snprintf(line1, LCD_COLUMNS + 1U, "Fault %-4s", shortFaultName(currentDecision.faultCode));
    }
    return;
  }

  formatLcdFloat(pipeText, sizeof(pipeText), currentDecision.pipeTempC);
  (void)snprintf(line0, LCD_COLUMNS + 1U, "Pipe %sC", pipeText);
  if (currentDecision.faultCode == dewpoint_policy::FAULT_NONE) {
    formatLcdFloat(minSafeText, sizeof(minSafeText), currentDecision.minSafeColdTempC);
    (void)snprintf(line1,
                   LCD_COLUMNS + 1U,
                   actuatorState.feedbackValid ? "Min%s Y%02d U%02d" : "Min%s Y%02d UF!",
                   minSafeText,
                   commandedPercent,
                   feedbackPercent);
  } else {
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Fault %-4s", shortFaultName(currentDecision.faultCode));
  }
}

void formatCalibrationScreen(char *line0, char *line1) {
  const size_t sensorIndex = activeCalibrationSensorIndex();
  char rawTempText[8] = {};
  char rawRhText[8] = {};
  char pendingTempText[8] = {};
  char pendingRhText[8] = {};

  syncCalibrationEditor();
  if (sensorIndex >= SENSOR_SLOT_COUNT || !sensorRegistry[sensorIndex].valid) {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "Cal slot invalid");
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Wait live sensor");
    return;
  }

  if (sensorSamples[sensorIndex].hasHumidity) {
    formatLcdFloat(rawTempText, sizeof(rawTempText), sensorSamples[sensorIndex].rawTempC);
    formatLcdFloat(rawRhText, sizeof(rawRhText), sensorSamples[sensorIndex].rawRhPercent);
    formatLcdFloat(pendingTempText, sizeof(pendingTempText), lcdUiState.pendingTempC);
    formatLcdFloat(pendingRhText, sizeof(pendingRhText), lcdUiState.pendingRhPercent);
    (void)snprintf(line0,
                   LCD_COLUMNS + 1U,
                   "S%u M%s %s%%",
                   static_cast<unsigned>(sensorIndex),
                   rawTempText,
                   rawRhText);
    (void)snprintf(line1,
                   LCD_COLUMNS + 1U,
                   lcdUiState.calibrationField == CAL_FIELD_TEMP ? ">T%s H%s"
                                                                 : " T%s>H%s",
                   pendingTempText,
                   pendingRhText);
    return;
  }

  formatLcdFloat(rawTempText, sizeof(rawTempText), sensorSamples[sensorIndex].rawTempC);
  formatLcdFloat(pendingTempText, sizeof(pendingTempText), lcdUiState.pendingTempC);
  (void)snprintf(line0, LCD_COLUMNS + 1U, "S%u M%sC PIPE", static_cast<unsigned>(sensorIndex), rawTempText);
  (void)snprintf(line1, LCD_COLUMNS + 1U, ">T%s Sel=OK", pendingTempText);
}

void formatStageMenuScreen(char *line0, char *line1) {
  const StageMenuItem currentItem = stageMenuItemFromIndex(lcdUiState.menuIndex);

  (void)snprintf(line0,
                 LCD_COLUMNS + 1U,
                 "Menu>%s",
                 stageMenuItemName(currentItem));
  (void)snprintf(line1, LCD_COLUMNS + 1U, "U/D Sel LeftBk");
}

void formatSensorViewerScreen(char *line0, char *line1) {
  const size_t sensorIndex = lcdUiState.browseSensorIndex;
  const bool useCalibratedView = calibrationUsableForControl(sensorIndex);
  char tempText[8] = {};
  char rhText[8] = {};

  if (sensorIndex >= SENSOR_SLOT_COUNT || !sensorRegistry[sensorIndex].discovered) {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "No live sensor");
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Left=menu");
    return;
  }

  if (sensorRegistry[sensorIndex].capability == commissioning::SENSOR_CAPABILITY_TEMP_RH) {
    (void)snprintf(line0,
                   LCD_COLUMNS + 1U,
                   "S%u %-4s ID%u %s",
                   static_cast<unsigned>(sensorIndex),
                   shortRoleName(sensorRegistry[sensorIndex].role),
                   sensorRegistry[sensorIndex].descriptor.location.address,
                   useCalibratedView ? "CAL" : "RAW");
    if (sensorRegistry[sensorIndex].valid) {
      formatLcdFloat(tempText,
                     sizeof(tempText),
                     useCalibratedView ? correctedTempC(sensorIndex)
                                       : sensorSamples[sensorIndex].rawTempC);
      formatLcdFloat(rhText,
                     sizeof(rhText),
                     useCalibratedView ? correctedRhPercent(sensorIndex)
                                       : sensorSamples[sensorIndex].rawRhPercent);
      (void)snprintf(line1, LCD_COLUMNS + 1U, "%sC %s%%", tempText, rhText);
    } else {
      (void)snprintf(line1, LCD_COLUMNS + 1U, "No live sample");
    }
    return;
  }

  (void)snprintf(line0,
                 LCD_COLUMNS + 1U,
                 "S%u %-4s %s",
                 static_cast<unsigned>(sensorIndex),
                 shortRoleName(sensorRegistry[sensorIndex].role),
                 useCalibratedView ? "CAL" : "RAW");
  if (sensorRegistry[sensorIndex].valid) {
    formatLcdFloat(tempText,
                   sizeof(tempText),
                   useCalibratedView ? correctedTempC(sensorIndex)
                                     : sensorSamples[sensorIndex].rawTempC);
    (void)snprintf(line1, LCD_COLUMNS + 1U, "%sC temp", tempText);
  } else {
    (void)snprintf(line1, LCD_COLUMNS + 1U, "No live sample");
  }
}

void formatCalibrationSelectScreen(char *line0, char *line1) {
  const size_t sensorIndex = lcdUiState.browseSensorIndex;

  if (!sensorCanBeMenuCalibrated(sensorIndex)) {
    (void)snprintf(line0, LCD_COLUMNS + 1U, "No live sensor");
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Left=menu");
    return;
  }

  (void)snprintf(line0,
                 LCD_COLUMNS + 1U,
                 "Cal S%u %-4s",
                 static_cast<unsigned>(sensorIndex),
                 shortRoleName(sensorRegistry[sensorIndex].role));
  if (sensorSamples[sensorIndex].hasHumidity) {
    (void)snprintf(line1, LCD_COLUMNS + 1U, "U/D Sel LeftBk");
  } else {
    (void)snprintf(line1, LCD_COLUMNS + 1U, "Pipe Sel LeftBk");
  }
}

void formatPwmCalibrationScreen(char *line0, char *line1) {
  (void)snprintf(line0, LCD_COLUMNS + 1U, "PWM cal 50%%");
  (void)snprintf(line1, LCD_COLUMNS + 1U, "Adj 5.00V Left");
}

void formatCommissioningScreen(char *line0, char *line1) {
  const unsigned liveCount = static_cast<unsigned>(discoveredSensorCount());
  (void)snprintf(line0, LCD_COLUMNS + 1U, "Fixed Z1 Z2 Z3 P");
  (void)snprintf(line1, LCD_COLUMNS + 1U, "Live %u/4 Sel=menu", liveCount);
}

// Renders the active LCD screen, including overlays and timed notices.
void renderLcd(unsigned long nowMs) {
  char line0[LCD_COLUMNS + 1U] = {};
  char line1[LCD_COLUMNS + 1U] = {};

  if (!lcdView.shouldRender(nowMs)) {
    return;
  }

  lcdView.markRendered(nowMs);
  if (lcdNoticeActive(nowMs)) {
    lcdView.copyTimedMessage(line0, sizeof(line0), line1, sizeof(line1));
  } else {
    if (lcdView.timedMessageExpired(nowMs)) {
      clearLcdNotice();
    }

    if (overlayActive()) {
      switch (lcdUiState.overlayMode) {
        case UI_OVERLAY_STAGE_MENU:
          formatStageMenuScreen(line0, line1);
          break;
        case UI_OVERLAY_SENSOR_VIEW:
          formatSensorViewerScreen(line0, line1);
          break;
        case UI_OVERLAY_CAL_SELECT:
          formatCalibrationSelectScreen(line0, line1);
          break;
        case UI_OVERLAY_CAL_EDIT:
          formatCalibrationScreen(line0, line1);
          break;
        case UI_OVERLAY_PWM_CAL:
          formatPwmCalibrationScreen(line0, line1);
          break;
        case UI_OVERLAY_NONE:
        default:
          formatCommissioningScreen(line0, line1);
          break;
      }
    } else if (lcdUiState.page == DISPLAY_PAGE_COMMISSIONING) {
      formatCommissioningScreen(line0, line1);
    } else {
      switch (lcdUiState.page) {
        case DISPLAY_PAGE_ZONE_1:
          formatZoneScreen(0U, line0, line1);
          break;
        case DISPLAY_PAGE_ZONE_2:
          formatZoneScreen(1U, line0, line1);
          break;
        case DISPLAY_PAGE_ZONE_3:
          formatZoneScreen(2U, line0, line1);
          break;
        case DISPLAY_PAGE_PIPE:
          formatPipeScreen(line0, line1);
          break;
        case DISPLAY_PAGE_COMMISSIONING:
          formatCommissioningScreen(line0, line1);
          break;
        case DISPLAY_PAGE_SUMMARY:
        default:
          formatSummaryScreen(line0, line1);
          break;
      }
    }
  }

  lcdView.writeLines(lcd, line0, line1);
  drawLcdSensorStatusGlyph(nowMs);
}

// Dispatches one debounced keypad event into the active UI.
void handleUiEvent(const UiInputEvent &event, unsigned long nowMs) {
  if (event.key == UI_KEY_NONE) {
    return;
  }

  if (handleOverlayUiEvent(event, nowMs)) {
    return;
  }

  if (event.shortPress && event.key == UI_KEY_SELECT && menuAllowedInCurrentState()) {
    openStageMenu();
    return;
  }

  (void)nowMs;
  (void)handleReadyPageUiEvent(event);
}

void handleCalibrationCommand(char *commandLine, unsigned long nowMs) {
  char *cursor = skipLeadingSpaces(commandLine);
  const bool calibrationModeActive = manualCalibrationModeActive();

  if (*cursor == '\0') {
    return;
  }

  if (handleCommonSerialCommand(cursor, nowMs)) {
    return;
  }

  if (!calibrationModeActive) {
    Serial.println(F("Serial commands here: show, pause, resume, traceoff, traceon, keypad, menu, probe [1-3], busdiag <1-3> [count] [delay_ms], wipeeeprom; calibration commands only while the calibration editor is open"));
    return;
  }

  if (strcmp(cursor, "clear") == 0) {
    (void)clearCalibrationFromSerial();
    return;
  }

  if (strncmp(cursor, "set", 3) != 0 || (cursor[3] != '\0' && cursor[3] != ' ')) {
    Serial.println(F("Unknown command; use set <tempC> [rh%], clear, or show"));
    return;
  }

  cursor = skipLeadingSpaces(cursor + 3);
  (void)applyCalibrationSetCommand(cursor, nowMs);
}

// Drains the serial command port for controller runtime and calibration commands.
void processControllerSerial(unsigned long nowMs) {
  while (Serial.available() > 0) {
    const int nextByte = Serial.read();

    if (nextByte < 0) {
      return;
    }

    if (nextByte == '\r') {
      continue;
    }

    if (nextByte == '\n') {
      serialCommandBuffer[serialCommandLength] = '\0';
      handleCalibrationCommand(serialCommandBuffer, nowMs);
      serialCommandLength = 0U;
      maybePrintSystemInventoryStatus();
      continue;
    }

    if (serialCommandLength + 1U >= SERIAL_COMMAND_CAPACITY) {
      serialCommandLength = 0U;
      Serial.println(F("Command rejected: line too long"));
      continue;
    }

    serialCommandBuffer[serialCommandLength] = static_cast<char>(nextByte);
    serialCommandLength += 1U;
  }
}

// Initializes hardware, persistent state, UI, and control subsystems.
void setup() {
  size_t sensorIndex = 0U;

  pinMode(PIN_ACTUATOR_COMMAND_PWM, OUTPUT);
  pinMode(PIN_ACTUATOR_FEEDBACK, INPUT);
  pinMode(PIN_PIPE_RTD_CS, OUTPUT);
  pinMode(PIN_LCD_BACKLIGHT, OUTPUT);
  for (sensorIndex = 0U; sensorIndex < SENSOR_SLOT_COUNT; ++sensorIndex) {
    pinMode(SENSOR_STATUS_LED_PINS[sensorIndex], OUTPUT);
    digitalWrite(SENSOR_STATUS_LED_PINS[sensorIndex], LOW);
  }
  runSensorStatusLedSelfTest();
  digitalWrite(PIN_PIPE_RTD_CS, HIGH);
  digitalWrite(PIN_LCD_BACKLIGHT, HIGH);
  if (USE_COOLING_CALL_INPUT) {
    pinMode(PIN_COOLING_CALL, INPUT_PULLUP);
  }

  Serial.begin(115200);
  {
    const unsigned long serialWaitStartMs = millis();
    while (!Serial && (millis() - serialWaitStartMs) < 1500UL) {
      delay(10);
    }
  }

  persistentStorageReady = initializePersistentStorage();
  lcd.begin(LCD_COLUMNS, LCD_ROWS);
  runLcdSensorStatusSelfTest();
  updateLcdSensorStatusGlyph(millis());
  initializeAirNodeCache();
  SPI.begin();
  const bool airBusReady = initializeAirBus();
  initializeSensorRegistry();
  if (airBusReady) {
    primeAirNodeIdentityDiscovery(CAN_BOOT_DISCOVERY_MS);
  }
  if (!loadPersistedCalibration()) {
    persistedCalibrationLoaded = false;
  }
  initializeFixedControllerState();
  modulating_actuator::initialize(ACTUATOR_CONFIG, 100.0f, millis(), &actuatorState);
  forceActuatorWarmSafe(millis());

  Serial.println(F("Dew point controller starting"));
  Serial.print(F("CAN air bus on MCP2515 (CS="));
  Serial.print(PIN_CAN_CS);
  Serial.print(F(", INT="));
  Serial.print(PIN_CAN_INT);
  Serial.println(F(")"));
  Serial.print(F("CAN settings: bitrate="));
  Serial.print(CAN_BITRATE);
  Serial.print(F(" bps, osc="));
  Serial.print(CAN_MCP2515_OSCILLATOR_HZ);
  Serial.println(F(" Hz"));
  if (!airBusReady) {
    Serial.print(F("CAN init failed: "));
    Serial.println(mcp2515_can::initStatusName(controller_can_bus::initStatus()));
  }
  Serial.println(F("Actuator expects external 0-10V command stage on PWM pin 44 and 0-10V feedback on A10"));
  if (!persistentStorageReady) {
    Serial.println(F("Persistent storage unavailable; calibration will not survive reboot"));
  } else if (persistedCalibrationLoaded) {
    Serial.println(F("Restored sensor calibration from EEPROM"));
  } else {
    Serial.println(F("No valid stored calibration found in EEPROM"));
  }
  Serial.println(F("Fixed slot layout active: Z1, Z2, Z3, PIPE"));
  Serial.println(F("Sensor status LEDs: D30 Z1, D31 Z2, D32 Z3, D33 PIPE"));
  maybePrintSystemInventoryStatus();
  updateSensorStatusLeds(millis());
  renderLcd(millis());
}

// Runs the cooperative control loop, UI handling, sampling, and safety enforcement.
void loop() {
  const unsigned long nowMs = millis();
  const UiInputEvent uiEvent = pollKeypadEvent(nowMs);

  pollAirBus(nowMs);
  processControllerSerial(nowMs);
  handleUiEvent(uiEvent, nowMs);
  maybePrintSystemInventoryStatus();
  sampleActuatorFeedback(nowMs);
  updateSensorStatusLeds(nowMs);
  renderLcd(nowMs);

  if (pwmCalibrationModeActive()) {
    setControlSafetyFault(CONTROL_SAFETY_NONE);
    writePwmCalibrationOutput();
    return;
  }

  if ((nowMs - lastSampleMs) < SAMPLE_PERIOD_MS) {
    return;
  }

  lastSampleMs = nowMs;
  refreshSensorReadings();
  maybePrintSystemInventoryStatus();

  currentInputs = collectInputs();
  currentDecision = dewpoint_policy::evaluate(CONTROL_CONFIG, currentInputs);
  if (periodicStatusEnabled) {
    printStatus(currentDecision);
  }
  updateSensorStatusLeds(nowMs);
  renderLcd(nowMs);

  if (!control_safety::humidityInputsRemainSane(CONTROL_CONFIG, currentInputs) ||
      !dewpoint_policy::inputsSaneForControl(CONTROL_CONFIG, currentInputs)) {
    setControlSafetyFault(CONTROL_SAFETY_INPUTS_INVALID);
    forceActuatorWarmSafe(nowMs);
    return;
  }

  if (!control_safety::decisionInvariantsHold(CONTROL_CONFIG, currentInputs, currentDecision)) {
    setControlSafetyFault(CONTROL_SAFETY_DECISION_INVALID);
    forceActuatorWarmSafe(nowMs);
    return;
  }

  if (!actuatorStateConsistent() || actuatorState.faultCode != modulating_actuator::FAULT_NONE) {
    setControlSafetyFault(CONTROL_SAFETY_ACTUATOR_INVALID);
    forceActuatorWarmSafe(nowMs);
    return;
  }

  if (!applyDecisionToActuator(currentInputs, currentDecision, nowMs)) {
    setControlSafetyFault(CONTROL_SAFETY_OUTPUT_INVALID);
    return;
  }

  setControlSafetyFault(CONTROL_SAFETY_NONE);
}
