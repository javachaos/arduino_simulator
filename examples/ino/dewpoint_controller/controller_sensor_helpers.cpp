#include "controller_sensor_helpers.h"

namespace controller_sensor_helpers {

namespace {

commissioning::SensorRole nextMissingRoleForUi(const commissioning::SensorRecord sensors[],
                                               size_t sensorCount) {
  static const commissioning::SensorRole kRequiredRoles[] = {
      commissioning::SENSOR_ROLE_AIR_ZONE_1,
      commissioning::SENSOR_ROLE_AIR_ZONE_2,
      commissioning::SENSOR_ROLE_AIR_ZONE_3,
      commissioning::SENSOR_ROLE_PIPE_TEMP,
  };
  size_t roleIndex = 0U;
  size_t sensorIndex = 0U;

  for (roleIndex = 0U; roleIndex < (sizeof(kRequiredRoles) / sizeof(kRequiredRoles[0])); ++roleIndex) {
    size_t roleCount = 0U;

    for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
      if (sensors[sensorIndex].role == kRequiredRoles[roleIndex]) {
        roleCount += 1U;
      }
    }

    if (roleCount == 0U) {
      return kRequiredRoles[roleIndex];
    }
  }

  return commissioning::SENSOR_ROLE_NONE;
}

bool sensorSupportsRoleForUi(const commissioning::SensorRecord &sensor,
                             commissioning::SensorRole role) {
  if (!sensor.discovered || sensor.role != commissioning::SENSOR_ROLE_NONE) {
    return false;
  }

  switch (role) {
    case commissioning::SENSOR_ROLE_AIR_ZONE_1:
    case commissioning::SENSOR_ROLE_AIR_ZONE_2:
    case commissioning::SENSOR_ROLE_AIR_ZONE_3:
      return sensor.capability == commissioning::SENSOR_CAPABILITY_TEMP_RH;
    case commissioning::SENSOR_ROLE_PIPE_TEMP:
      return sensor.capability == commissioning::SENSOR_CAPABILITY_TEMP_ONLY;
    case commissioning::SENSOR_ROLE_NONE:
    default:
      return false;
  }
}

bool stagedCommissioningCanAdvance(const commissioning::SensorRecord sensors[], size_t sensorCount) {
  const commissioning::SensorRole nextRole = nextMissingRoleForUi(sensors, sensorCount);
  size_t sensorIndex = 0U;

  if (nextRole == commissioning::SENSOR_ROLE_NONE) {
    return false;
  }

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensorSupportsRoleForUi(sensors[sensorIndex], nextRole)) {
      return true;
    }
  }

  return false;
}

}  // namespace

size_t stagedRoleCount(const commissioning::SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  size_t count = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].role != commissioning::SENSOR_ROLE_NONE) {
      count += 1U;
    }
  }

  return count;
}

size_t discoveredSensorCount(const commissioning::SensorRecord sensors[], size_t sensorCount) {
  size_t sensorIndex = 0U;
  size_t count = 0U;

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    if (sensors[sensorIndex].discovered) {
      count += 1U;
    }
  }

  return count;
}

bool stagedCommissioningRoleAssignmentComplete(const commissioning::SensorRecord sensors[],
                                               size_t sensorCount) {
  return commissioning::assignmentsComplete(sensors, sensorCount);
}

bool stagedWorkflowCanContinue(const commissioning::SensorRecord sensors[], size_t sensorCount) {
  if (stagedRoleCount(sensors, sensorCount) == 0U) {
    return false;
  }

  if (stagedCommissioningRoleAssignmentComplete(sensors, sensorCount)) {
    return true;
  }

  return stagedCommissioningCanAdvance(sensors, sensorCount);
}

bool stagedWorkflowNeedsAdditionalSensor(const commissioning::SensorRecord sensors[],
                                         size_t sensorCount) {
  return stagedRoleCount(sensors, sensorCount) > 0U &&
         !stagedCommissioningRoleAssignmentComplete(sensors, sensorCount) &&
         !stagedCommissioningCanAdvance(sensors, sensorCount);
}

size_t findNextDiscoveredSensor(const commissioning::SensorRecord sensors[],
                                size_t sensorCount,
                                size_t startAfterIndex,
                                bool forward) {
  size_t offset = 0U;

  if (sensorCount == 0U) {
    return commissioning::kInvalidSensorIndex;
  }

  for (offset = 1U; offset <= sensorCount; ++offset) {
    const size_t candidateIndex =
        forward ? ((startAfterIndex + offset) % sensorCount)
                : ((startAfterIndex + sensorCount - (offset % sensorCount)) % sensorCount);

    if (sensors[candidateIndex].discovered) {
      return candidateIndex;
    }
  }

  return commissioning::kInvalidSensorIndex;
}

bool sensorCanBeMenuCalibrated(const commissioning::SensorRecord sensors[],
                               size_t sensorCount,
                               size_t sensorIndex) {
  return sensorIndex < sensorCount &&
         sensors[sensorIndex].role != commissioning::SENSOR_ROLE_NONE &&
         sensors[sensorIndex].discovered && sensors[sensorIndex].valid;
}

size_t findNextMenuCalibratableSensor(const commissioning::SensorRecord sensors[],
                                      size_t sensorCount,
                                      size_t startAfterIndex,
                                      bool forward) {
  size_t offset = 0U;

  if (sensorCount == 0U) {
    return commissioning::kInvalidSensorIndex;
  }

  for (offset = 1U; offset <= sensorCount; ++offset) {
    const size_t candidateIndex =
        forward ? ((startAfterIndex + offset) % sensorCount)
                : ((startAfterIndex + sensorCount - (offset % sensorCount)) % sensorCount);

    if (sensorCanBeMenuCalibrated(sensors, sensorCount, candidateIndex)) {
      return candidateIndex;
    }
  }

  return commissioning::kInvalidSensorIndex;
}

}  // namespace controller_sensor_helpers
