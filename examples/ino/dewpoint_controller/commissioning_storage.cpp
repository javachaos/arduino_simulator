#include "commissioning_storage.h"

#include <math.h>
#include <string.h>

namespace commissioning_storage {

namespace {

bool validBusType(uint8_t busType) {
  return busType <= static_cast<uint8_t>(commissioning::BUS_TYPE_SPI);
}

bool validCapability(uint8_t capability) {
  return capability <= static_cast<uint8_t>(commissioning::SENSOR_CAPABILITY_TEMP_RH);
}

bool validRole(uint8_t role) {
  return role <= static_cast<uint8_t>(commissioning::SENSOR_ROLE_PIPE_TEMP);
}

bool entryDescriptorMatches(const SensorEntry &entry,
                            const commissioning::SensorRecord &sensor) {
  return entry.sensorId == sensor.descriptor.sensorId &&
         entry.busType ==
             static_cast<uint8_t>(sensor.descriptor.location.busType) &&
         entry.busIndex == sensor.descriptor.location.busIndex &&
         entry.channelIndex == sensor.descriptor.location.channelIndex &&
         entry.address == sensor.descriptor.location.address &&
         entry.slotIndex == sensor.descriptor.location.slotIndex &&
         entry.capability == static_cast<uint8_t>(sensor.capability);
}

bool calibrationWithinLimits(const SensorEntry &entry,
                             float maxTempOffsetC,
                             float maxRhOffsetPercent) {
  if (!isfinite(entry.tempOffsetC) || !isfinite(entry.rhOffsetPercent)) {
    return false;
  }

  if (fabsf(entry.tempOffsetC) > maxTempOffsetC) {
    return false;
  }

  if (entry.capability == static_cast<uint8_t>(commissioning::SENSOR_CAPABILITY_TEMP_RH) &&
      fabsf(entry.rhOffsetPercent) > maxRhOffsetPercent) {
    return false;
  }

  if (entry.capability == static_cast<uint8_t>(commissioning::SENSOR_CAPABILITY_TEMP_ONLY) &&
      entry.rhOffsetPercent != 0.0f) {
    return false;
  }

  return true;
}

bool commissioningFlagsConsistent(const SensorEntry &entry) {
  const bool commissioned = (entry.flags & kFlagCommissioned) != 0U;
  const bool calibrationValid = (entry.flags & kFlagCalibrationValid) != 0U;
  const bool calibrated = (entry.flags & kFlagCalibrated) != 0U;

  if (commissioned &&
      (!calibrated || !calibrationValid ||
       entry.role == static_cast<uint8_t>(commissioning::SENSOR_ROLE_NONE))) {
    return false;
  }

  if (entry.role == static_cast<uint8_t>(commissioning::SENSOR_ROLE_NONE) &&
      (commissioned || calibrated || calibrationValid)) {
    return false;
  }

  return true;
}

bool assignedRolesUnique(const Image &image) {
  bool seenRoles[commissioning::kRequiredRoleCount + 1U] = {};
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];
    const uint8_t role = entry.role;

    if (role == static_cast<uint8_t>(commissioning::SENSOR_ROLE_NONE)) {
      continue;
    }

    if (role > commissioning::kRequiredRoleCount) {
      return false;
    }

    if (seenRoles[role]) {
      return false;
    }

    seenRoles[role] = true;
  }

  return true;
}

bool commissionedRolesComplete(const Image &image) {
  bool seenRoles[commissioning::kRequiredRoleCount + 1U] = {};
  bool sawCommissioned = false;
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];
    const bool commissioned = (entry.flags & kFlagCommissioned) != 0U;
    const uint8_t role = entry.role;

    if (!commissioned) {
      continue;
    }

    sawCommissioned = true;
    if (role == static_cast<uint8_t>(commissioning::SENSOR_ROLE_NONE) ||
        role > commissioning::kRequiredRoleCount ||
        seenRoles[role]) {
      return false;
    }

    seenRoles[role] = true;
  }

  if (!sawCommissioned) {
    return true;
  }

  for (sensorIndex = 1U; sensorIndex <= commissioning::kRequiredRoleCount; ++sensorIndex) {
    if (!seenRoles[sensorIndex]) {
      return false;
    }
  }

  return true;
}

}  // namespace

void clearImage(Image *image) {
  (void)memset(image, 0, sizeof(*image));
}

uint32_t computeChecksum(const Image &image) {
  const uint8_t *bytes = reinterpret_cast<const uint8_t *>(&image);
  const size_t byteCount = offsetof(Image, checksum);
  uint32_t hash = 2166136261UL;
  size_t index = 0U;

  for (index = 0U; index < byteCount; ++index) {
    hash ^= bytes[index];
    hash *= 16777619UL;
  }

  return hash;
}

bool validateImage(const Image &image) {
  size_t sensorIndex = 0U;

  if (image.magic != kImageMagic || image.version != kImageVersion) {
    return false;
  }

  if (image.sensorCount == 0U || image.sensorCount > commissioning::kMaxSensors) {
    return false;
  }

  if (computeChecksum(image) != image.checksum) {
    return false;
  }

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];

    if (!validBusType(entry.busType) ||
        !validCapability(entry.capability) ||
        !validRole(entry.role) ||
        !commissioningFlagsConsistent(entry)) {
      return false;
    }
  }

  return assignedRolesUnique(image) && commissionedRolesComplete(image);
}

bool captureImage(const commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  Image *image) {
  size_t sensorIndex = 0U;

  if (sensorCount == 0U || sensorCount > commissioning::kMaxSensors) {
    return false;
  }

  clearImage(image);
  image->magic = kImageMagic;
  image->version = kImageVersion;
  image->sensorCount = static_cast<uint16_t>(sensorCount);

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    SensorEntry &entry = image->sensors[sensorIndex];
    const commissioning::SensorRecord &sensor = sensors[sensorIndex];

    entry.sensorId = sensor.descriptor.sensorId;
    entry.busType = static_cast<uint8_t>(sensor.descriptor.location.busType);
    entry.busIndex = sensor.descriptor.location.busIndex;
    entry.channelIndex = sensor.descriptor.location.channelIndex;
    entry.address = sensor.descriptor.location.address;
    entry.slotIndex = sensor.descriptor.location.slotIndex;
    entry.capability = static_cast<uint8_t>(sensor.capability);
    entry.role = static_cast<uint8_t>(sensor.role);
    entry.flags = 0U;
    if (sensor.commissioned) {
      entry.flags |= kFlagCommissioned;
    }
    if (sensor.calibration.valid) {
      entry.flags |= kFlagCalibrationValid;
    }
    if (sensor.calibration.calibrated) {
      entry.flags |= kFlagCalibrated;
    }
    entry.tempOffsetC = sensor.calibration.tempOffsetC;
    entry.rhOffsetPercent = sensor.calibration.rhOffsetPercent;
  }

  image->checksum = computeChecksum(*image);
  return validateImage(*image);
}

bool restoreImage(const Image &image,
                  commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  float maxTempOffsetC,
                  float maxRhOffsetPercent) {
  commissioning::SensorRecord working[commissioning::kMaxSensors] = {};
  bool usedSensors[commissioning::kMaxSensors] = {};
  size_t sensorIndex = 0U;

  if (sensorCount == 0U || sensorCount > commissioning::kMaxSensors) {
    return false;
  }

  if (!validateImage(image) || sensorCount < image.sensorCount) {
    return false;
  }

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    working[sensorIndex] = sensors[sensorIndex];
    working[sensorIndex].role = commissioning::SENSOR_ROLE_NONE;
    working[sensorIndex].commissioned = false;
    working[sensorIndex].calibration.tempOffsetC = 0.0f;
    working[sensorIndex].calibration.rhOffsetPercent = 0.0f;
    working[sensorIndex].calibration.calibrated = false;
    working[sensorIndex].calibration.valid = false;
  }

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];
    size_t liveIndex = 0U;
    bool matched = false;

    if (!calibrationWithinLimits(entry, maxTempOffsetC, maxRhOffsetPercent)) {
      return false;
    }

    for (liveIndex = 0U; liveIndex < sensorCount; ++liveIndex) {
      if (usedSensors[liveIndex]) {
        continue;
      }

      if (!entryDescriptorMatches(entry, working[liveIndex])) {
        continue;
      }

      usedSensors[liveIndex] = true;
      working[liveIndex].role =
          static_cast<commissioning::SensorRole>(entry.role);
      working[liveIndex].commissioned = (entry.flags & kFlagCommissioned) != 0U;
      working[liveIndex].calibration.valid =
          (entry.flags & kFlagCalibrationValid) != 0U;
      working[liveIndex].calibration.calibrated =
          (entry.flags & kFlagCalibrated) != 0U;
      working[liveIndex].calibration.tempOffsetC = entry.tempOffsetC;
      working[liveIndex].calibration.rhOffsetPercent = entry.rhOffsetPercent;
      matched = true;
      break;
    }

    if (!matched) {
      return false;
    }
  }

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    sensors[sensorIndex] = working[sensorIndex];
  }

  return true;
}

}  // namespace commissioning_storage
