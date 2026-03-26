#ifndef CAN_AIR_PROTOCOL_H
#define CAN_AIR_PROTOCOL_H

#include <stddef.h>
#include <stdint.h>

namespace can_air_protocol {

constexpr uint8_t kMinNodeId = 1U;
constexpr uint8_t kMaxNodeId = 3U;
constexpr uint16_t kAirSampleBaseId = 0x180U;
constexpr uint16_t kNodeIdentityBaseId = 0x190U;
constexpr uint8_t kAirSampleLength = 6U;
constexpr uint8_t kNodeIdentityLength = 6U;
constexpr uint8_t kStatusSensorOk = 0x01U;

struct Frame {
  uint16_t id;
  uint8_t length;
  uint8_t data[8];
};

struct Sample {
  uint8_t nodeId;
  float tempC;
  float rhPercent;
  bool sensorOk;
  uint8_t sequence;
};

struct Identity {
  uint8_t nodeId;
  uint64_t sensorId;
};

// Returns true when the node id is within the supported air-node address range.
bool isValidNodeId(uint8_t nodeId);

// Encodes one air-sample payload into a CAN frame for the given node.
bool encodeSampleFrame(uint8_t nodeId,
                       float tempC,
                       float rhPercent,
                       bool sensorOk,
                       uint8_t sequence,
                       Frame *frame);

// Decodes an air-sample CAN frame into engineering units.
bool decodeSampleFrame(const Frame &frame, Sample *sample);

// Encodes the node identity announcement frame for a discovered sensor.
bool encodeIdentityFrame(uint8_t nodeId, uint64_t sensorId, Frame *frame);

// Decodes a node identity announcement frame.
bool decodeIdentityFrame(const Frame &frame, Identity *identity);

}  // namespace can_air_protocol

#endif
