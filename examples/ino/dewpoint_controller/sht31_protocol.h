#ifndef SHT31_PROTOCOL_H
#define SHT31_PROTOCOL_H

#include <stddef.h>
#include <stdint.h>

namespace sht31 {

constexpr uint8_t kDefaultAddress = 0x44U;
constexpr uint16_t kCommandMeasureHighRepeatabilityNoStretch = 0x2400U;
constexpr uint16_t kCommandReadSerialNumberNoStretch = 0x3682U;
constexpr size_t kPayloadSize = 6U;

// Computes the SHT3x CRC-8 over the provided two-byte word.
inline uint8_t computeCrc(const uint8_t *data, size_t length) {
  uint8_t crc = 0xFFU;
  size_t index = 0U;
  uint8_t bitIndex = 0U;

  if (data == nullptr) {
    return 0U;
  }

  for (index = 0U; index < length; ++index) {
    crc ^= data[index];
    for (bitIndex = 0U; bitIndex < 8U; ++bitIndex) {
      if ((crc & 0x80U) != 0U) {
        crc = static_cast<uint8_t>((crc << 1U) ^ 0x31U);
      } else {
        crc <<= 1U;
      }
    }
  }

  return crc;
}

namespace detail {

inline bool crcMatchesWord(const uint8_t *raw) {
  return computeCrc(raw, 2U) == raw[2];
}

}  // namespace detail

// Decodes a six-byte serial-number payload into a 32-bit serial number.
inline bool decodeSerialNumber(const uint8_t *raw,
                               size_t length,
                               uint32_t *serialNumber) {
  if (raw == nullptr || serialNumber == nullptr || length != kPayloadSize ||
      !detail::crcMatchesWord(&raw[0]) || !detail::crcMatchesWord(&raw[3])) {
    return false;
  }

  *serialNumber = (static_cast<uint32_t>(raw[0]) << 24U) |
                  (static_cast<uint32_t>(raw[1]) << 16U) |
                  (static_cast<uint32_t>(raw[3]) << 8U) |
                  static_cast<uint32_t>(raw[4]);
  return *serialNumber != 0UL;
}

// Decodes a six-byte measurement payload into temperature and relative humidity.
inline bool decodeMeasurement(const uint8_t *raw,
                              size_t length,
                              float *tempC,
                              float *rhPercent) {
  uint16_t rawTemp = 0U;
  uint16_t rawRh = 0U;

  if (raw == nullptr || tempC == nullptr || rhPercent == nullptr || length != kPayloadSize ||
      !detail::crcMatchesWord(&raw[0]) || !detail::crcMatchesWord(&raw[3])) {
    return false;
  }

  rawTemp = static_cast<uint16_t>((static_cast<uint16_t>(raw[0]) << 8U) | raw[1]);
  rawRh = static_cast<uint16_t>((static_cast<uint16_t>(raw[3]) << 8U) | raw[4]);

  *tempC = -45.0f + (175.0f * static_cast<float>(rawTemp) / 65535.0f);
  *rhPercent = 100.0f * static_cast<float>(rawRh) / 65535.0f;
  return true;
}

}  // namespace sht31

#endif
