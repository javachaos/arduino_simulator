#ifndef COMMISSIONING_STATE_H
#define COMMISSIONING_STATE_H

#include <stddef.h>
#include <stdint.h>

namespace commissioning {

constexpr size_t kMaxSensors = 8U;
constexpr size_t kRequiredRoleCount = 4U;
constexpr size_t kInvalidSensorIndex = static_cast<size_t>(-1);

enum SensorCapability {
  SENSOR_CAPABILITY_NONE = 0,
  SENSOR_CAPABILITY_TEMP_ONLY = 1,
  SENSOR_CAPABILITY_TEMP_RH = 2
};

enum SensorRole {
  SENSOR_ROLE_NONE = 0,
  SENSOR_ROLE_AIR_ZONE_1 = 1,
  SENSOR_ROLE_AIR_ZONE_2 = 2,
  SENSOR_ROLE_AIR_ZONE_3 = 3,
  SENSOR_ROLE_PIPE_TEMP = 4
};

enum BusType {
  BUS_TYPE_NONE = 0,
  BUS_TYPE_FIXED_SLOT = 1,
  BUS_TYPE_I2C = 2,
  BUS_TYPE_ONEWIRE = 3,
  BUS_TYPE_ANALOG = 4,
  BUS_TYPE_CAN = 5,
  BUS_TYPE_SPI = 6
};

enum Mode {
  MODE_LOCKED = 0,
  MODE_ASSIGN_ROLE = 1,
  MODE_CALIBRATE = 2,
  MODE_VALIDATE = 3,
  MODE_READY = 4,
  MODE_FAULT = 5
};

enum Event {
  EVENT_NONE = 0,
  EVENT_SHORT_PRESS = 1,
  EVENT_LONG_PRESS = 2
};

enum FaultCode {
  FAULT_NONE = 0,
  FAULT_DISCOVERY_INCOMPLETE = 1,
  FAULT_NO_ELIGIBLE_SENSOR = 2,
  FAULT_CALIBRATION_INVALID = 3,
  FAULT_VALIDATION_FAILED = 4
};

struct Config {
  unsigned long validationDurationMs;
  float maxTempOffsetC;
  float maxRhOffsetPercent;
};

struct SensorLocation {
  BusType busType;
  uint8_t busIndex;
  uint8_t channelIndex;
  uint8_t address;
  uint8_t slotIndex;
};

struct SensorDescriptor {
  uint64_t sensorId;
  SensorLocation location;
};

struct CalibrationRecord {
  float tempOffsetC;
  float rhOffsetPercent;
  bool calibrated;
  bool valid;
};

struct SensorRecord {
  SensorDescriptor descriptor;
  SensorCapability capability;
  SensorRole role;
  CalibrationRecord calibration;
  bool discovered;
  bool valid;
  bool commissioned;
};

struct State {
  Mode mode;
  FaultCode faultCode;
  size_t activeRoleIndex;
  size_t highlightedSensorIndex;
  unsigned long validationStartMs;
  bool statusDirty;
};

// Initializes commissioning state and clears transient workflow selections.
void initialize(State *state, SensorRecord sensors[], size_t sensorCount);

// Advances the commissioning state machine for the supplied event and sensor inventory.
void update(State *state,
            SensorRecord sensors[],
            size_t sensorCount,
            const Config &config,
            Event event,
            unsigned long nowMs);

// Returns true only when commissioning has completed and control may run.
bool controlEnabled(const State &state);

// Returns true when every required role has exactly one assigned sensor.
bool assignmentsComplete(const SensorRecord sensors[], size_t sensorCount);

// Returns true when every assigned sensor has valid calibration data.
bool calibrationsComplete(const SensorRecord sensors[], size_t sensorCount);

// Returns the role currently being assigned, or SENSOR_ROLE_NONE if inactive.
SensorRole activeRole(const State &state);

// Returns a stable display/debug name for a commissioning mode value.
const char *modeName(Mode mode);

// Returns a stable display/debug name for a commissioning fault value.
const char *faultName(FaultCode faultCode);

// Returns a stable display/debug name for a sensor role value.
const char *roleName(SensorRole role);

// Returns a stable display/debug name for a sensor capability value.
const char *capabilityName(SensorCapability capability);

// Returns a stable display/debug name for a sensor bus type value.
const char *busTypeName(BusType busType);

}  // namespace commissioning

#endif
