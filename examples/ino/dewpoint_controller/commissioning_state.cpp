#include "commissioning_state.h"

#include <math.h>

namespace commissioning {

namespace {

bool prepareCalibration(State *state,
                        const SensorRecord sensors[],
                        size_t sensorCount,
                        size_t startAfterIndex);

SensorRole requiredRoleByIndex(size_t roleIndex) {
  switch (roleIndex) {
    case 0U:
      return SENSOR_ROLE_AIR_ZONE_1;
    case 1U:
      return SENSOR_ROLE_AIR_ZONE_2;
    case 2U:
      return SENSOR_ROLE_AIR_ZONE_3;
    case 3U:
      return SENSOR_ROLE_PIPE_TEMP;
    default:
      return SENSOR_ROLE_NONE;
  }
}

bool rolePresent(const SensorRecord sensors[], size_t sensorCount, SensorRole role) {
  size_t sensorIndex = 0U;
  size_t roleCount = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role == role) {
      roleCount += 1U;
    }
  }

  return roleCount == 1U;
}

bool anyAssignedRoles(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role != SENSOR_ROLE_NONE) {
      return true;
    }
  }

  return false;
}

size_t discoveredSensorCount(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  size_t count = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].discovered) {
      count += 1U;
    }
  }

  return count;
}

bool sensorSupportsRole(const SensorRecord &sensor, SensorRole role) {
  if (!sensor.discovered) {
    return false;
  }

  switch (role) {
    case SENSOR_ROLE_AIR_ZONE_1:
    case SENSOR_ROLE_AIR_ZONE_2:
    case SENSOR_ROLE_AIR_ZONE_3:
      return sensor.capability == SENSOR_CAPABILITY_TEMP_RH;
    case SENSOR_ROLE_PIPE_TEMP:
      return sensor.capability == SENSOR_CAPABILITY_TEMP_ONLY;
    case SENSOR_ROLE_NONE:
    default:
      return false;
  }
}

void clearAssignments(SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    sensors[sensorIndex].role = SENSOR_ROLE_NONE;
    sensors[sensorIndex].commissioned = false;
    sensors[sensorIndex].calibration.tempOffsetC = 0.0f;
    sensors[sensorIndex].calibration.rhOffsetPercent = 0.0f;
    sensors[sensorIndex].calibration.calibrated = false;
    sensors[sensorIndex].calibration.valid = false;
  }
}

size_t findEligibleSensor(const SensorRecord sensors[],
                          size_t sensorCount,
                          SensorRole role,
                          size_t startAfterIndex) {
  size_t offset = 0U;

  if (sensorCount == 0U) {
    return kInvalidSensorIndex;
  }

  for (offset = 1U; offset <= sensorCount; ++offset) {
    const size_t candidateIndex = (startAfterIndex + offset) % sensorCount;

    if (sensors[candidateIndex].role != SENSOR_ROLE_NONE) {
      continue;
    }

    if (sensorSupportsRole(sensors[candidateIndex], role)) {
      return candidateIndex;
    }
  }

  return kInvalidSensorIndex;
}

size_t findMissingRoleIndex(const SensorRecord sensors[],
                            size_t sensorCount,
                            size_t startRoleIndex) {
  size_t offset = 0U;

  for (offset = 0U; offset < kRequiredRoleCount; ++offset) {
    const size_t roleIndex = (startRoleIndex + offset) % kRequiredRoleCount;
    const SensorRole role = requiredRoleByIndex(roleIndex);

    if (!rolePresent(sensors, sensorCount, role)) {
      return roleIndex;
    }
  }

  return kInvalidSensorIndex;
}

size_t findNextAssignableRoleIndex(const SensorRecord sensors[],
                                   size_t sensorCount,
                                   size_t startRoleIndex) {
  size_t offset = 0U;

  for (offset = 0U; offset < kRequiredRoleCount; ++offset) {
    const size_t roleIndex = (startRoleIndex + offset) % kRequiredRoleCount;
    const SensorRole role = requiredRoleByIndex(roleIndex);

    if (rolePresent(sensors, sensorCount, role)) {
      continue;
    }

    if (findEligibleSensor(sensors, sensorCount, role, sensorCount - 1U) != kInvalidSensorIndex) {
      return roleIndex;
    }
  }

  return kInvalidSensorIndex;
}

size_t findAssignedSensor(const SensorRecord sensors[],
                          size_t sensorCount,
                          size_t startAfterIndex,
                          bool requireCalibration) {
  size_t offset = 0U;

  if (sensorCount == 0U) {
    return kInvalidSensorIndex;
  }

  for (offset = 1U; offset <= sensorCount; ++offset) {
    const size_t candidateIndex = (startAfterIndex + offset) % sensorCount;

    if (sensors[candidateIndex].role == SENSOR_ROLE_NONE) {
      continue;
    }

    if (requireCalibration && sensors[candidateIndex].calibration.calibrated) {
      continue;
    }

    return candidateIndex;
  }

  return kInvalidSensorIndex;
}

size_t findCalibratableAssignedSensor(const SensorRecord sensors[],
                                      size_t sensorCount,
                                      size_t startAfterIndex) {
  size_t offset = 0U;

  if (sensorCount == 0U) {
    return kInvalidSensorIndex;
  }

  for (offset = 1U; offset <= sensorCount; ++offset) {
    const size_t candidateIndex = (startAfterIndex + offset) % sensorCount;

    if (sensors[candidateIndex].role == SENSOR_ROLE_NONE ||
        sensors[candidateIndex].calibration.calibrated ||
        !sensors[candidateIndex].discovered ||
        !sensors[candidateIndex].valid) {
      continue;
    }

    return candidateIndex;
  }

  return kInvalidSensorIndex;
}

bool hasAssignedSensorsAwaitingCalibration(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role != SENSOR_ROLE_NONE &&
        !sensors[sensorIndex].calibration.calibrated) {
      return true;
    }
  }

  return false;
}

void enterFault(State *state, FaultCode faultCode) {
  state->mode = MODE_FAULT;
  state->faultCode = faultCode;
  state->activeRoleIndex = kRequiredRoleCount;
  state->highlightedSensorIndex = kInvalidSensorIndex;
  state->validationStartMs = 0UL;
  state->statusDirty = true;
}

void enterLockedWaiting(State *state,
                        const SensorRecord sensors[],
                        size_t sensorCount) {
  const size_t nextRoleIndex = findMissingRoleIndex(sensors, sensorCount, 0U);

  state->mode = MODE_LOCKED;
  state->faultCode = FAULT_NONE;
  state->activeRoleIndex = nextRoleIndex == kInvalidSensorIndex ? kRequiredRoleCount : nextRoleIndex;
  state->highlightedSensorIndex = kInvalidSensorIndex;
  state->validationStartMs = 0UL;
  state->statusDirty = true;
}

bool prepareAssignment(State *state, const SensorRecord sensors[], size_t sensorCount) {
  const SensorRole role = requiredRoleByIndex(state->activeRoleIndex);

  state->highlightedSensorIndex = findEligibleSensor(sensors, sensorCount, role, sensorCount - 1U);
  if (state->highlightedSensorIndex == kInvalidSensorIndex) {
    enterFault(state, FAULT_NO_ELIGIBLE_SENSOR);
    return false;
  }

  state->mode = MODE_ASSIGN_ROLE;
  state->faultCode = FAULT_NONE;
  state->statusDirty = true;
  return true;
}

bool allAssignedSensorsCalibrated(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  bool sawAssignedSensor = false;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role == SENSOR_ROLE_NONE) {
      continue;
    }

    sawAssignedSensor = true;
    if (!sensors[sensorIndex].calibration.calibrated ||
        !sensors[sensorIndex].calibration.valid) {
      return false;
    }
  }

  return sawAssignedSensor;
}

bool calibrationWithinLimits(const SensorRecord &sensor, const Config &config) {
  if (!sensor.calibration.calibrated || !sensor.calibration.valid) {
    return false;
  }

  if (fabsf(sensor.calibration.tempOffsetC) > config.maxTempOffsetC) {
    return false;
  }

  if (sensor.capability == SENSOR_CAPABILITY_TEMP_RH &&
      fabsf(sensor.calibration.rhOffsetPercent) > config.maxRhOffsetPercent) {
    return false;
  }

  return true;
}

bool assignedCalibrationInvalid(const SensorRecord sensors[],
                                size_t sensorCount,
                                const Config &config) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role == SENSOR_ROLE_NONE) {
      continue;
    }

    if (sensors[sensorIndex].calibration.calibrated &&
        !calibrationWithinLimits(sensors[sensorIndex], config)) {
      return true;
    }
  }

  return false;
}

void advanceWorkflow(State *state,
                     const SensorRecord sensors[],
                     size_t sensorCount,
                     size_t preferredRoleIndex) {
  const size_t nextAssignableRoleIndex =
      findNextAssignableRoleIndex(sensors, sensorCount, preferredRoleIndex);
  const bool haveAssignedRoles = anyAssignedRoles(sensors, sensorCount);

  if (haveAssignedRoles && !allAssignedSensorsCalibrated(sensors, sensorCount)) {
    (void)prepareCalibration(state, sensors, sensorCount, sensorCount - 1U);
    return;
  }

  if (assignmentsComplete(sensors, sensorCount) &&
      calibrationsComplete(sensors, sensorCount)) {
    state->mode = MODE_VALIDATE;
    state->faultCode = FAULT_NONE;
    state->highlightedSensorIndex = kInvalidSensorIndex;
    state->validationStartMs = 0UL;
    state->statusDirty = true;
    return;
  }

  if (nextAssignableRoleIndex != kInvalidSensorIndex) {
    state->activeRoleIndex = nextAssignableRoleIndex;
    (void)prepareAssignment(state, sensors, sensorCount);
    return;
  }

  enterLockedWaiting(state, sensors, sensorCount);
}

bool prepareCalibration(State *state,
                        const SensorRecord sensors[],
                        size_t sensorCount,
                        size_t startAfterIndex) {
  const size_t nextIndex =
      findCalibratableAssignedSensor(sensors, sensorCount, startAfterIndex);

  if (nextIndex == kInvalidSensorIndex) {
    if (hasAssignedSensorsAwaitingCalibration(sensors, sensorCount)) {
      enterLockedWaiting(state, sensors, sensorCount);
      return true;
    }

    if (allAssignedSensorsCalibrated(sensors, sensorCount)) {
      advanceWorkflow(state, sensors, sensorCount, 0U);
      return true;
    }

    enterFault(state, FAULT_CALIBRATION_INVALID);
    return false;
  }

  state->mode = MODE_CALIBRATE;
  state->faultCode = FAULT_NONE;
  state->highlightedSensorIndex = nextIndex;
  state->statusDirty = true;
  return true;
}

void startCommissioning(State *state, SensorRecord sensors[], size_t sensorCount) {
  if (state->mode == MODE_READY || state->mode == MODE_VALIDATE ||
      (state->mode == MODE_FAULT && state->faultCode == FAULT_VALIDATION_FAILED &&
       assignmentsComplete(sensors, sensorCount) &&
       calibrationsComplete(sensors, sensorCount))) {
    clearAssignments(sensors, sensorCount);
  }

  state->faultCode = FAULT_NONE;
  state->validationStartMs = 0UL;

  if (!anyAssignedRoles(sensors, sensorCount) &&
      discoveredSensorCount(sensors, sensorCount) == 0U) {
    enterFault(state, FAULT_DISCOVERY_INCOMPLETE);
    return;
  }

  advanceWorkflow(state, sensors, sensorCount, 0U);
}

void advanceAssignment(State *state, SensorRecord sensors[], size_t sensorCount) {
  const SensorRole role = requiredRoleByIndex(state->activeRoleIndex);

  if (state->highlightedSensorIndex == kInvalidSensorIndex ||
      state->highlightedSensorIndex >= sensorCount) {
    enterFault(state, FAULT_NO_ELIGIBLE_SENSOR);
    return;
  }

  sensors[state->highlightedSensorIndex].role = role;
  sensors[state->highlightedSensorIndex].commissioned = false;
  sensors[state->highlightedSensorIndex].calibration.tempOffsetC = 0.0f;
  sensors[state->highlightedSensorIndex].calibration.rhOffsetPercent = 0.0f;
  sensors[state->highlightedSensorIndex].calibration.calibrated = false;
  sensors[state->highlightedSensorIndex].calibration.valid = false;

  advanceWorkflow(state, sensors, sensorCount, state->activeRoleIndex + 1U);
}

bool assignedSensorsAreValid(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  size_t assignedCount = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role == SENSOR_ROLE_NONE) {
      continue;
    }

    assignedCount += 1U;
    if (!sensors[sensorIndex].discovered || !sensors[sensorIndex].valid) {
      return false;
    }
  }

  return assignedCount == kRequiredRoleCount;
}

void markAssignedSensorsCommissioned(SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role != SENSOR_ROLE_NONE) {
      sensors[sensorIndex].commissioned = true;
    }
  }
}

}  // namespace

void initialize(State *state, SensorRecord sensors[], size_t sensorCount) {
  state->mode = MODE_LOCKED;
  state->faultCode = FAULT_NONE;
  state->activeRoleIndex = 0U;
  state->highlightedSensorIndex = kInvalidSensorIndex;
  state->validationStartMs = 0UL;
  state->statusDirty = true;

  if (assignmentsComplete(sensors, sensorCount) &&
      calibrationsComplete(sensors, sensorCount)) {
    state->mode = MODE_VALIDATE;
  }
}

void update(State *state,
            SensorRecord sensors[],
            size_t sensorCount,
            const Config &config,
            Event event,
            unsigned long nowMs) {
  if (event == EVENT_LONG_PRESS &&
      (state->mode == MODE_LOCKED || state->mode == MODE_READY || state->mode == MODE_VALIDATE ||
       state->mode == MODE_FAULT)) {
    startCommissioning(state, sensors, sensorCount);
    return;
  }

  if (state->mode == MODE_ASSIGN_ROLE) {
    if (event == EVENT_SHORT_PRESS) {
      const SensorRole role = requiredRoleByIndex(state->activeRoleIndex);
      const size_t nextIndex =
          findEligibleSensor(sensors, sensorCount, role, state->highlightedSensorIndex);

      if (nextIndex == kInvalidSensorIndex) {
        enterFault(state, FAULT_NO_ELIGIBLE_SENSOR);
        return;
      }

      state->highlightedSensorIndex = nextIndex;
      state->statusDirty = true;
      return;
    }

    if (event == EVENT_LONG_PRESS) {
      advanceAssignment(state, sensors, sensorCount);
      return;
    }

    return;
  }

  if (state->mode == MODE_CALIBRATE) {
    if (assignedCalibrationInvalid(sensors, sensorCount, config)) {
      enterFault(state, FAULT_CALIBRATION_INVALID);
      return;
    }

    if (event == EVENT_SHORT_PRESS) {
      const size_t nextIndex =
          findAssignedSensor(sensors, sensorCount, state->highlightedSensorIndex, false);

      if (nextIndex == kInvalidSensorIndex) {
        enterFault(state, FAULT_CALIBRATION_INVALID);
        return;
      }

      state->highlightedSensorIndex = nextIndex;
      state->statusDirty = true;
      return;
    }

    if (calibrationsComplete(sensors, sensorCount)) {
      state->mode = MODE_VALIDATE;
      state->faultCode = FAULT_NONE;
      state->highlightedSensorIndex = kInvalidSensorIndex;
      state->validationStartMs = nowMs;
      state->statusDirty = true;
      return;
    }

    if (state->highlightedSensorIndex == kInvalidSensorIndex ||
        state->highlightedSensorIndex >= sensorCount ||
        sensors[state->highlightedSensorIndex].role == SENSOR_ROLE_NONE ||
        sensors[state->highlightedSensorIndex].calibration.calibrated ||
        !sensors[state->highlightedSensorIndex].discovered ||
        !sensors[state->highlightedSensorIndex].valid) {
      prepareCalibration(state, sensors, sensorCount, state->highlightedSensorIndex);
    }
    return;
  }

  if (state->mode == MODE_VALIDATE) {
    if (!calibrationsComplete(sensors, sensorCount) ||
        assignedCalibrationInvalid(sensors, sensorCount, config)) {
      enterFault(state, FAULT_CALIBRATION_INVALID);
      return;
    }

    if (!assignedSensorsAreValid(sensors, sensorCount)) {
      if (state->validationStartMs != 0UL) {
        enterFault(state, FAULT_VALIDATION_FAILED);
      }
      return;
    }

    if (state->validationStartMs == 0UL) {
      state->validationStartMs = nowMs;
      state->statusDirty = true;
      return;
    }

    if ((nowMs - state->validationStartMs) >= config.validationDurationMs) {
      markAssignedSensorsCommissioned(sensors, sensorCount);
      state->mode = MODE_READY;
      state->faultCode = FAULT_NONE;
      state->activeRoleIndex = kRequiredRoleCount;
      state->highlightedSensorIndex = kInvalidSensorIndex;
      state->statusDirty = true;
    }
  }
}

bool controlEnabled(const State &state) {
  return state.mode == MODE_READY;
}

bool assignmentsComplete(const SensorRecord sensors[], size_t sensorCount) {
  return rolePresent(sensors, sensorCount, SENSOR_ROLE_AIR_ZONE_1) &&
         rolePresent(sensors, sensorCount, SENSOR_ROLE_AIR_ZONE_2) &&
         rolePresent(sensors, sensorCount, SENSOR_ROLE_AIR_ZONE_3) &&
         rolePresent(sensors, sensorCount, SENSOR_ROLE_PIPE_TEMP);
}

bool calibrationsComplete(const SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  size_t calibrationCount = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role == SENSOR_ROLE_NONE) {
      continue;
    }

    calibrationCount += 1U;
    if (!sensors[sensorIndex].calibration.calibrated ||
        !sensors[sensorIndex].calibration.valid) {
      return false;
    }
  }

  return calibrationCount == kRequiredRoleCount;
}

SensorRole activeRole(const State &state) {
  if (state.activeRoleIndex >= kRequiredRoleCount) {
    return SENSOR_ROLE_NONE;
  }

  return requiredRoleByIndex(state.activeRoleIndex);
}

const char *modeName(Mode mode) {
  switch (mode) {
    case MODE_ASSIGN_ROLE:
      return "ASSIGN_ROLE";
    case MODE_CALIBRATE:
      return "CALIBRATE";
    case MODE_VALIDATE:
      return "VALIDATE";
    case MODE_READY:
      return "READY";
    case MODE_FAULT:
      return "FAULT";
    case MODE_LOCKED:
    default:
      return "LOCKED";
  }
}

const char *faultName(FaultCode faultCode) {
  switch (faultCode) {
    case FAULT_DISCOVERY_INCOMPLETE:
      return "DISCOVERY_INCOMPLETE";
    case FAULT_NO_ELIGIBLE_SENSOR:
      return "NO_ELIGIBLE_SENSOR";
    case FAULT_CALIBRATION_INVALID:
      return "CALIBRATION_INVALID";
    case FAULT_VALIDATION_FAILED:
      return "VALIDATION_FAILED";
    case FAULT_NONE:
    default:
      return "NONE";
  }
}

const char *roleName(SensorRole role) {
  switch (role) {
    case SENSOR_ROLE_AIR_ZONE_1:
      return "AIR_ZONE_1";
    case SENSOR_ROLE_AIR_ZONE_2:
      return "AIR_ZONE_2";
    case SENSOR_ROLE_AIR_ZONE_3:
      return "AIR_ZONE_3";
    case SENSOR_ROLE_PIPE_TEMP:
      return "PIPE_TEMP";
    case SENSOR_ROLE_NONE:
    default:
      return "NONE";
  }
}

const char *capabilityName(SensorCapability capability) {
  switch (capability) {
    case SENSOR_CAPABILITY_TEMP_ONLY:
      return "TEMP_ONLY";
    case SENSOR_CAPABILITY_TEMP_RH:
      return "TEMP_RH";
    case SENSOR_CAPABILITY_NONE:
    default:
      return "NONE";
  }
}

const char *busTypeName(BusType busType) {
  switch (busType) {
    case BUS_TYPE_FIXED_SLOT:
      return "FIXED_SLOT";
    case BUS_TYPE_I2C:
      return "I2C";
    case BUS_TYPE_ONEWIRE:
      return "ONEWIRE";
    case BUS_TYPE_ANALOG:
      return "ANALOG";
    case BUS_TYPE_CAN:
      return "CAN";
    case BUS_TYPE_SPI:
      return "SPI";
    case BUS_TYPE_NONE:
    default:
      return "NONE";
  }
}

}  // namespace commissioning
