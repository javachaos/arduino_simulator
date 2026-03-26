#ifndef CONTROLLER_CAN_BUS_H
#define CONTROLLER_CAN_BUS_H

#include <Arduino.h>
#include <SPI.h>

#include "can_air_protocol.h"
#include "commissioning_state.h"
#include "mcp2515_can.h"

namespace controller_can_bus {

struct Config {
  size_t nodeCount;
  unsigned long airTimeoutMs;
  unsigned long sequenceStaleMs;
  uint8_t chipSelectPin;
  uint8_t interruptPin;
  uint32_t spiClockHz;
  uint32_t oscillatorHz;
  uint32_t bitrate;
};

struct Cache {
  float tempC;
  float rhPercent;
  bool sensorOk;
  bool received;
  bool identityReceived;
  uint16_t sequence;
  uint64_t sensorId;
  unsigned long lastRxMs;
  unsigned long lastSequenceChangeMs;
  unsigned long lastIdentityRxMs;
};

enum ProbeStatus {
  PROBE_OK = 0,
  PROBE_INVALID_NODE,
  PROBE_NO_IDENTITY,
  PROBE_NO_SAMPLE,
  PROBE_STALE_SAMPLE,
  PROBE_SENSOR_FAULT,
};

struct ProbeResult {
  ProbeStatus status;
  bool identityReceived;
  unsigned long ageMs;
  can_air_protocol::Sample sample;
  uint64_t sensorId;
};

// Resets all cached air-node state.
void initializeCache(Cache cache[], size_t nodeCount);

// Initializes the MCP2515 controller for the air-node CAN bus.
bool initializeController(SPIClass &spi, const Config &config);

// Returns the last MCP2515 initialization status.
mcp2515_can::InitStatus initStatus();

// Drains received CAN frames into the controller cache.
void pollBus(SPIClass &spi,
             const Config &config,
             Cache cache[],
             size_t nodeCount,
             unsigned long nowMs);

// Returns true when a cached sample is recent and sensor-valid enough to use.
bool airNodeFresh(const Cache cache[],
                  size_t nodeCount,
                  size_t zoneIndex,
                  unsigned long nowMs,
                  unsigned long airTimeoutMs);

// Returns a stable display/debug name for one probe result.
const char *probeStatusName(ProbeStatus status);

// Returns the current cached state for one fixed air node.
ProbeResult probeNode(const Cache cache[],
                      size_t nodeCount,
                      uint8_t nodeId,
                      unsigned long nowMs,
                      unsigned long airTimeoutMs);

// Copies any discovered node identities into the fixed sensor registry.
void syncIdentityDescriptors(const Cache cache[],
                             size_t nodeCount,
                             commissioning::SensorRecord sensors[],
                             size_t sensorCount);

// Waits briefly for initial identity/sample broadcasts so boot-time status is populated.
void primeIdentityDiscovery(SPIClass &spi,
                            const Config &config,
                            Cache cache[],
                            commissioning::SensorRecord sensors[],
                            size_t sensorCount,
                            unsigned long timeoutMs);

// Returns the latest cached temperature/RH sample for one air zone when fresh.
bool readAirZoneRaw(const Cache cache[],
                    size_t nodeCount,
                    unsigned long airTimeoutMs,
                    uint8_t zoneIndex,
                    unsigned long nowMs,
                    float &tempC,
                    float &rhPercent);

}  // namespace controller_can_bus

#endif
