#include "sensor_calibration_storage.h"

#include <math.h>
#include <string.h>

namespace sensor_calibration_storage {

namespace {

bool validBusType(uint8_t busType) {
  return busType <= static_cast<uint8_t>(commissioning::BUS_TYPE_SPI);
}

bool validCapability(uint8_t capability) {
  return capability <= static_cast<uint8_t>(commissioning::SENSOR_CAPABILITY_TEMP_RH);
}

bool calibrationFlagsConsistent(const SensorEntry &entry) {
  const bool calibrationValid = (entry.flags & kFlagCalibrationValid) != 0U;
  const bool calibrated = (entry.flags & kFlagCalibrated) != 0U;

  return calibrationValid == calibrated;
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

bool entryDescriptorMatches(const SensorEntry &entry,
                            const commissioning::SensorRecord &sensor) {
  return entry.slotIndex == sensor.descriptor.location.slotIndex &&
         entry.busType == static_cast<uint8_t>(sensor.descriptor.location.busType) &&
         entry.address == sensor.descriptor.location.address &&
         entry.capability == static_cast<uint8_t>(sensor.capability);
}

bool slotIndicesUnique(const Image &image) {
  bool seenSlots[commissioning::kMaxSensors] = {};
  size_t sensorIndex = 0U;

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const uint8_t slotIndex = image.sensors[sensorIndex].slotIndex;

    if (slotIndex >= image.sensorCount || seenSlots[slotIndex]) {
      return false;
    }

    seenSlots[slotIndex] = true;
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

  if (!slotIndicesUnique(image)) {
    return false;
  }

  for (sensorIndex = 0U; sensorIndex < image.sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];

    if (!validBusType(entry.busType) ||
        !validCapability(entry.capability) ||
        !calibrationFlagsConsistent(entry)) {
      return false;
    }
  }

  return true;
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

    entry.slotIndex = sensor.descriptor.location.slotIndex;
    entry.busType = static_cast<uint8_t>(sensor.descriptor.location.busType);
    entry.address = sensor.descriptor.location.address;
    entry.capability = static_cast<uint8_t>(sensor.capability);
    entry.flags = 0U;
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
  size_t sensorIndex = 0U;

  if (!validateImage(image) || sensorCount != image.sensorCount) {
    return false;
  }

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    sensors[sensorIndex].calibration.tempOffsetC = 0.0f;
    sensors[sensorIndex].calibration.rhOffsetPercent = 0.0f;
    sensors[sensorIndex].calibration.calibrated = false;
    sensors[sensorIndex].calibration.valid = false;
  }

  for (sensorIndex = 0U; sensorIndex < sensorCount; ++sensorIndex) {
    const SensorEntry &entry = image.sensors[sensorIndex];
    commissioning::SensorRecord &sensor = sensors[entry.slotIndex];
    const bool calibrated = (entry.flags & kFlagCalibrated) != 0U;

    if (!entryDescriptorMatches(entry, sensor) ||
        !calibrationWithinLimits(entry, maxTempOffsetC, maxRhOffsetPercent)) {
      return false;
    }

    sensor.calibration.tempOffsetC = entry.tempOffsetC;
    sensor.calibration.rhOffsetPercent = entry.rhOffsetPercent;
    sensor.calibration.calibrated = calibrated;
    sensor.calibration.valid = calibrated;
  }

  return true;
}

}  // namespace sensor_calibration_storage
