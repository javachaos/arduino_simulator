#ifndef MAX31865_RTD_H
#define MAX31865_RTD_H

#include <stdint.h>

namespace max31865_rtd {

enum FilterMode {
  FILTER_60HZ = 0,
  FILTER_50HZ = 1
};

struct RtdModel {
  float referenceResistorOhms;
  float nominalResistanceOhms;
  float coefficientA;
  float coefficientB;
  float coefficientC;
  float minTempC;
  float maxTempC;
};

constexpr uint8_t kRegisterConfig = 0x00U;
constexpr uint8_t kRegisterRtdMsb = 0x01U;
constexpr uint8_t kRegisterRtdLsb = 0x02U;
constexpr uint8_t kRegisterFaultStatus = 0x07U;

constexpr uint8_t kConfigBias = 0x80U;
constexpr uint8_t kConfigOneShot = 0x20U;
constexpr uint8_t kConfigThreeWire = 0x10U;
constexpr uint8_t kConfigFaultClear = 0x02U;
constexpr uint8_t kConfigFilter50Hz = 0x01U;

constexpr uint8_t kFaultHighThreshold = 0x80U;
constexpr uint8_t kFaultLowThreshold = 0x40U;
constexpr uint8_t kFaultRefinHigh = 0x20U;
constexpr uint8_t kFaultRefinLow = 0x10U;
constexpr uint8_t kFaultRtdinLow = 0x08U;
constexpr uint8_t kFaultOverUnderVoltage = 0x04U;

// Returns the RTD model constants for a PT1000 probe on the MAX31865 front-end.
RtdModel pt1000Model();

// Builds the MAX31865 configuration register value from individual option flags.
uint8_t buildConfigByte(bool enableBias,
                        bool oneShot,
                        bool threeWire,
                        bool clearFault,
                        FilterMode filterMode);

// Converts a raw MAX31865 RTD code into resistance in ohms.
bool rawCodeToResistanceOhms(uint16_t rawCode, const RtdModel &model, float *resistanceOhms);

// Converts a temperature setpoint into the equivalent RTD resistance.
bool temperatureCToResistanceOhms(float tempC, const RtdModel &model, float *resistanceOhms);

// Converts RTD resistance back into temperature using the configured probe model.
bool resistanceOhmsToTemperatureC(float resistanceOhms, const RtdModel &model, float *tempC);

}  // namespace max31865_rtd

#endif
