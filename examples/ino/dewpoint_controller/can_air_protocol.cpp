#include "can_air_protocol.h"

#include <math.h>

namespace can_air_protocol {

namespace {

bool finiteFloat(float value) {
  return !isnan(value) && !isinf(value);
}

bool validTempC(float value) {
  return finiteFloat(value) && value >= -40.0f && value <= 125.0f;
}

bool validRhPercent(float value) {
  return finiteFloat(value) && value >= 0.0f && value <= 100.0f;
}

int16_t packSignedCenti(float value) {
  return static_cast<int16_t>(lroundf(value * 100.0f));
}

uint16_t packUnsignedCenti(float value) {
  return static_cast<uint16_t>(lroundf(value * 100.0f));
}

float unpackSignedCenti(int16_t value) {
  return static_cast<float>(value) / 100.0f;
}

float unpackUnsignedCenti(uint16_t value) {
  return static_cast<float>(value) / 100.0f;
}

}  // namespace

bool isValidNodeId(uint8_t nodeId) {
  return nodeId >= kMinNodeId && nodeId <= kMaxNodeId;
}

bool encodeSampleFrame(uint8_t nodeId,
                       float tempC,
                       float rhPercent,
                       bool sensorOk,
                       uint8_t sequence,
                       Frame *frame) {
  int16_t packedTempC = 0;
  uint16_t packedRhPercent = 0U;

  if (frame == nullptr || !isValidNodeId(nodeId) ||
      !validTempC(tempC) || !validRhPercent(rhPercent)) {
    return false;
  }

  packedTempC = packSignedCenti(tempC);
  packedRhPercent = packUnsignedCenti(rhPercent);

  frame->id = static_cast<uint16_t>(kAirSampleBaseId + (nodeId - 1U));
  frame->length = kAirSampleLength;
  frame->data[0] = static_cast<uint8_t>(packedTempC & 0xFF);
  frame->data[1] = static_cast<uint8_t>((packedTempC >> 8) & 0xFF);
  frame->data[2] = static_cast<uint8_t>(packedRhPercent & 0xFF);
  frame->data[3] = static_cast<uint8_t>((packedRhPercent >> 8) & 0xFF);
  frame->data[4] = sensorOk ? kStatusSensorOk : 0U;
  frame->data[5] = sequence;
  frame->data[6] = 0U;
  frame->data[7] = 0U;
  return true;
}

bool decodeSampleFrame(const Frame &frame, Sample *sample) {
  const uint16_t rawIdOffset = frame.id - kAirSampleBaseId;
  const uint8_t nodeId = static_cast<uint8_t>(rawIdOffset + 1U);
  int16_t packedTempC = 0;
  uint16_t packedRhPercent = 0U;
  float tempC = NAN;
  float rhPercent = NAN;

  if (sample == nullptr || frame.id < kAirSampleBaseId ||
      frame.id >= (kAirSampleBaseId + kMaxNodeId) ||
      frame.length != kAirSampleLength ||
      !isValidNodeId(nodeId)) {
    return false;
  }

  packedTempC = static_cast<int16_t>(
      static_cast<uint16_t>(frame.data[0]) |
      (static_cast<uint16_t>(frame.data[1]) << 8));
  packedRhPercent = static_cast<uint16_t>(
      static_cast<uint16_t>(frame.data[2]) |
      (static_cast<uint16_t>(frame.data[3]) << 8));

  tempC = unpackSignedCenti(packedTempC);
  rhPercent = unpackUnsignedCenti(packedRhPercent);
  if (!validTempC(tempC) || !validRhPercent(rhPercent)) {
    return false;
  }

  sample->nodeId = nodeId;
  sample->tempC = tempC;
  sample->rhPercent = rhPercent;
  sample->sensorOk = (frame.data[4] & kStatusSensorOk) != 0U;
  sample->sequence = frame.data[5];
  return true;
}

bool encodeIdentityFrame(uint8_t nodeId, uint64_t sensorId, Frame *frame) {
  size_t byteIndex = 0U;

  if (frame == nullptr || !isValidNodeId(nodeId) ||
      (sensorId >> 48U) != 0U || sensorId == 0ULL) {
    return false;
  }

  frame->id = static_cast<uint16_t>(kNodeIdentityBaseId + (nodeId - 1U));
  frame->length = kNodeIdentityLength;

  for (byteIndex = 0U; byteIndex < kNodeIdentityLength; ++byteIndex) {
    frame->data[byteIndex] =
        static_cast<uint8_t>((sensorId >> (8U * byteIndex)) & 0xFFU);
  }

  frame->data[6] = 0U;
  frame->data[7] = 0U;
  return true;
}

bool decodeIdentityFrame(const Frame &frame, Identity *identity) {
  const uint16_t rawIdOffset = frame.id - kNodeIdentityBaseId;
  const uint8_t nodeId = static_cast<uint8_t>(rawIdOffset + 1U);
  uint64_t sensorId = 0ULL;
  size_t byteIndex = 0U;

  if (identity == nullptr || frame.id < kNodeIdentityBaseId ||
      frame.id >= (kNodeIdentityBaseId + kMaxNodeId) ||
      frame.length != kNodeIdentityLength ||
      !isValidNodeId(nodeId)) {
    return false;
  }

  for (byteIndex = 0U; byteIndex < kNodeIdentityLength; ++byteIndex) {
    sensorId |= static_cast<uint64_t>(frame.data[byteIndex]) << (8U * byteIndex);
  }

  if (sensorId == 0ULL || (sensorId >> 48U) != 0U) {
    return false;
  }

  identity->nodeId = nodeId;
  identity->sensorId = sensorId;
  return true;
}

}  // namespace can_air_protocol
