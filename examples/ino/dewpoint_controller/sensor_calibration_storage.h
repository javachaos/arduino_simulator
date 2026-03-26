#ifndef SENSOR_CALIBRATION_STORAGE_H
#define SENSOR_CALIBRATION_STORAGE_H

#include <stddef.h>
#include <stdint.h>

#include "commissioning_state.h"

namespace sensor_calibration_storage {

constexpr uint32_t kImageMagic = 0x44504341UL;
constexpr uint16_t kImageVersion = 1U;
constexpr uint8_t kFlagCalibrationValid = 0x01U;
constexpr uint8_t kFlagCalibrated = 0x02U;

struct SensorEntry {
  uint8_t slotIndex;
  uint8_t busType;
  uint8_t address;
  uint8_t capability;
  uint8_t flags;
  uint8_t reserved[3];
  float tempOffsetC;
  float rhOffsetPercent;
};

struct Image {
  uint32_t magic;
  uint16_t version;
  uint16_t sensorCount;
  SensorEntry sensors[commissioning::kMaxSensors];
  uint32_t checksum;
};

// Resets an image to the empty, invalid state.
void clearImage(Image *image);

// Computes the checksum used to validate EEPROM persistence.
uint32_t computeChecksum(const Image &image);

// Verifies magic, version, checksum, and field-level invariants for an image.
bool validateImage(const Image &image);

// Captures the current per-slot calibration records into a persistable image.
bool captureImage(const commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  Image *image);

// Restores a validated calibration image back into the live fixed-slot sensor records.
bool restoreImage(const Image &image,
                  commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  float maxTempOffsetC,
                  float maxRhOffsetPercent);

}  // namespace sensor_calibration_storage

#endif
