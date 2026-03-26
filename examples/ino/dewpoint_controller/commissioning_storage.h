#ifndef COMMISSIONING_STORAGE_H
#define COMMISSIONING_STORAGE_H

#include <stddef.h>
#include <stdint.h>

#include "commissioning_state.h"

namespace commissioning_storage {

constexpr uint32_t kImageMagic = 0x44505754UL;
constexpr uint16_t kImageVersion = 2U;
constexpr uint8_t kFlagCommissioned = 0x01U;
constexpr uint8_t kFlagCalibrationValid = 0x02U;
constexpr uint8_t kFlagCalibrated = 0x04U;

struct SensorEntry {
  uint64_t sensorId;
  uint8_t busType;
  uint8_t busIndex;
  uint8_t channelIndex;
  uint8_t address;
  uint8_t slotIndex;
  uint8_t capability;
  uint8_t role;
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

// Resets an image to the empty, invalid state before capture or erase.
void clearImage(Image *image);

// Computes the image checksum used to validate EEPROM persistence.
uint32_t computeChecksum(const Image &image);

// Verifies magic, version, checksum, and field-level invariants for an image.
bool validateImage(const Image &image);

// Captures the current commissioning sensor records into a persistable image.
bool captureImage(const commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  Image *image);

// Restores a validated image back into live sensor records, enforcing calibration limits.
bool restoreImage(const Image &image,
                  commissioning::SensorRecord sensors[],
                  size_t sensorCount,
                  float maxTempOffsetC,
                  float maxRhOffsetPercent);

}  // namespace commissioning_storage

#endif
