#include "controller_can_bus.h"

#include <math.h>

namespace controller_can_bus {

namespace {

mcp2515_can::InitStatus lastInitStatus = mcp2515_can::INIT_INVALID_TIMING;

void resetCacheEntry(Cache *entry) {
  entry->tempC = NAN;
  entry->rhPercent = NAN;
  entry->sensorOk = false;
  entry->received = false;
  entry->identityReceived = false;
  entry->sequence = 0U;
  entry->sensorId = 0ULL;
  entry->lastRxMs = 0UL;
  entry->lastSequenceChangeMs = 0UL;
  entry->lastIdentityRxMs = 0UL;
}

bool updateCache(Cache cache[],
                 size_t nodeCount,
                 const can_air_protocol::Sample &sample,
                 unsigned long nowMs,
                 unsigned long sequenceStaleMs) {
  const size_t zoneIndex = static_cast<size_t>(sample.nodeId - 1U);
  bool sequenceAdvanced = false;

  if (zoneIndex >= nodeCount) {
    return false;
  }

  sequenceAdvanced = !cache[zoneIndex].received || cache[zoneIndex].sequence != sample.sequence;
  if (!sequenceAdvanced &&
      (nowMs - cache[zoneIndex].lastSequenceChangeMs) > sequenceStaleMs) {
    cache[zoneIndex].sensorOk = false;
    return false;
  }

  if (sequenceAdvanced) {
    cache[zoneIndex].lastSequenceChangeMs = nowMs;
  }

  cache[zoneIndex].tempC = sample.tempC;
  cache[zoneIndex].rhPercent = sample.rhPercent;
  cache[zoneIndex].sensorOk = sample.sensorOk;
  cache[zoneIndex].received = true;
  cache[zoneIndex].sequence = sample.sequence;
  cache[zoneIndex].lastRxMs = nowMs;
  return true;
}

void updateIdentity(Cache cache[],
                    size_t nodeCount,
                    const can_air_protocol::Identity &identity,
                    unsigned long nowMs) {
  const size_t zoneIndex = static_cast<size_t>(identity.nodeId - 1U);

  if (zoneIndex >= nodeCount) {
    return;
  }

  cache[zoneIndex].identityReceived = true;
  cache[zoneIndex].sensorId = identity.sensorId;
  cache[zoneIndex].lastIdentityRxMs = nowMs;
}

}  // namespace

void initializeCache(Cache cache[], size_t nodeCount) {
  size_t zoneIndex = 0U;

  for (zoneIndex = 0U; zoneIndex < nodeCount; ++zoneIndex) {
    resetCacheEntry(&cache[zoneIndex]);
  }
}

bool initializeController(SPIClass &spi, const Config &config) {
  const mcp2515_can::Config canConfig = {
      config.chipSelectPin,
      config.interruptPin,
      config.spiClockHz,
      config.oscillatorHz,
      config.bitrate,
  };

  lastInitStatus = mcp2515_can::initialize(spi, canConfig);
  return lastInitStatus == mcp2515_can::INIT_OK;
}

mcp2515_can::InitStatus initStatus() {
  return lastInitStatus;
}

void pollBus(SPIClass &spi,
             const Config &config,
             Cache cache[],
             size_t nodeCount,
             unsigned long nowMs) {
  const mcp2515_can::Config canConfig = {
      config.chipSelectPin,
      config.interruptPin,
      config.spiClockHz,
      config.oscillatorHz,
      config.bitrate,
  };
  mcp2515_can::Frame frame = {};
  can_air_protocol::Frame canFrame = {};
  can_air_protocol::Sample sample = {};
  can_air_protocol::Identity identity = {};
  uint8_t iteration = 0U;

  for (iteration = 0U; iteration < 16U; ++iteration) {
    if (!mcp2515_can::receiveFrame(spi, canConfig, &frame)) {
      return;
    }

    canFrame.id = frame.id;
    canFrame.length = frame.length;
    for (uint8_t byteIndex = 0U; byteIndex < 8U; ++byteIndex) {
      canFrame.data[byteIndex] = frame.data[byteIndex];
    }

    if (can_air_protocol::decodeSampleFrame(canFrame, &sample)) {
      (void)updateCache(cache, nodeCount, sample, nowMs, config.sequenceStaleMs);
      continue;
    }

    if (can_air_protocol::decodeIdentityFrame(canFrame, &identity)) {
      updateIdentity(cache, nodeCount, identity, nowMs);
    }
  }
}

bool airNodeFresh(const Cache cache[],
                  size_t nodeCount,
                  size_t zoneIndex,
                  unsigned long nowMs,
                  unsigned long airTimeoutMs) {
  if (zoneIndex >= nodeCount || !cache[zoneIndex].received || !cache[zoneIndex].sensorOk) {
    return false;
  }

  return (nowMs - cache[zoneIndex].lastRxMs) <= airTimeoutMs;
}

const char *probeStatusName(ProbeStatus status) {
  switch (status) {
    case PROBE_OK:
      return "OK";
    case PROBE_INVALID_NODE:
      return "INVALID_NODE";
    case PROBE_NO_IDENTITY:
      return "NO_IDENTITY";
    case PROBE_NO_SAMPLE:
      return "NO_SAMPLE";
    case PROBE_STALE_SAMPLE:
      return "STALE_SAMPLE";
    case PROBE_SENSOR_FAULT:
    default:
      return "SENSOR_FAULT";
  }
}

ProbeResult probeNode(const Cache cache[],
                      size_t nodeCount,
                      uint8_t nodeId,
                      unsigned long nowMs,
                      unsigned long airTimeoutMs) {
  const size_t zoneIndex = static_cast<size_t>(nodeId - 1U);
  ProbeResult result = {};

  result.status = PROBE_INVALID_NODE;
  result.identityReceived = false;
  result.ageMs = 0UL;
  result.sensorId = 0ULL;

  if (nodeId < 1U || zoneIndex >= nodeCount) {
    return result;
  }

  result.identityReceived = cache[zoneIndex].identityReceived;
  result.sensorId = cache[zoneIndex].sensorId;
  if (!cache[zoneIndex].identityReceived) {
    result.status = PROBE_NO_IDENTITY;
    return result;
  }

  if (!cache[zoneIndex].received) {
    result.status = PROBE_NO_SAMPLE;
    return result;
  }

  result.ageMs = nowMs - cache[zoneIndex].lastRxMs;
  result.sample.nodeId = nodeId;
  result.sample.tempC = cache[zoneIndex].tempC;
  result.sample.rhPercent = cache[zoneIndex].rhPercent;
  result.sample.sensorOk = cache[zoneIndex].sensorOk;
  result.sample.sequence = cache[zoneIndex].sequence;

  if (!cache[zoneIndex].sensorOk) {
    result.status = PROBE_SENSOR_FAULT;
    return result;
  }

  if (result.ageMs > airTimeoutMs) {
    result.status = PROBE_STALE_SAMPLE;
    return result;
  }

  result.status = PROBE_OK;
  return result;
}

void syncIdentityDescriptors(const Cache cache[],
                             size_t nodeCount,
                             commissioning::SensorRecord sensors[],
                             size_t sensorCount) {
  size_t zoneIndex = 0U;
  const size_t count = nodeCount < sensorCount ? nodeCount : sensorCount;

  for (zoneIndex = 0U; zoneIndex < count; ++zoneIndex) {
    if (cache[zoneIndex].identityReceived) {
      sensors[zoneIndex].descriptor.sensorId = cache[zoneIndex].sensorId;
      sensors[zoneIndex].discovered = true;
    }
  }
}

void primeIdentityDiscovery(SPIClass &spi,
                            const Config &config,
                            Cache cache[],
                            commissioning::SensorRecord sensors[],
                            size_t sensorCount,
                            unsigned long timeoutMs) {
  const unsigned long startMs = millis();

  while ((millis() - startMs) < timeoutMs) {
    pollBus(spi, config, cache, config.nodeCount, millis());
    syncIdentityDescriptors(cache, config.nodeCount, sensors, sensorCount);
    delay(10);
  }
}

bool readAirZoneRaw(const Cache cache[],
                    size_t nodeCount,
                    unsigned long airTimeoutMs,
                    uint8_t zoneIndex,
                    unsigned long nowMs,
                    float &tempC,
                    float &rhPercent) {
  if (!airNodeFresh(cache, nodeCount, zoneIndex, nowMs, airTimeoutMs)) {
    return false;
  }

  tempC = cache[zoneIndex].tempC;
  rhPercent = cache[zoneIndex].rhPercent;
  return true;
}

}  // namespace controller_can_bus
