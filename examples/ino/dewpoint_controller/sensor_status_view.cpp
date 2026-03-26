#include "sensor_status_view.h"

namespace sensor_status_view {

namespace {

uint8_t lcdStatusRowMask(bool leftOn, bool rightOn) {
  uint8_t mask = 0U;

  if (leftOn) {
    mask = static_cast<uint8_t>(mask | 0b11000U);
  }
  if (rightOn) {
    mask = static_cast<uint8_t>(mask | 0b00011U);
  }

  return mask;
}

}  // namespace

bool sensorStatusLedShouldBlink(const commissioning::SensorRecord sensors[],
                                size_t sensorCount,
                                size_t sensorIndex) {
  if (sensorIndex >= sensorCount) {
    return false;
  }

  if (sensors[sensorIndex].discovered) {
    return !sensors[sensorIndex].valid;
  }

  return sensors[sensorIndex].commissioned;
}

bool sensorStatusLedOn(const commissioning::SensorRecord sensors[],
                       size_t sensorCount,
                       size_t sensorIndex,
                       unsigned long nowMs,
                       unsigned long blinkPeriodMs) {
  if (sensorIndex >= sensorCount) {
    return false;
  }

  if (sensorStatusLedShouldBlink(sensors, sensorCount, sensorIndex)) {
    return ((nowMs / blinkPeriodMs) % 2UL) == 0UL;
  }

  return sensors[sensorIndex].discovered && sensors[sensorIndex].valid;
}

void buildLcdSensorStatusGlyphFromFlags(bool zone1On,
                                        bool zone2On,
                                        bool zone3On,
                                        bool pipeOn,
                                        uint8_t glyph[8]) {
  uint8_t rowIndex = 0U;

  if (glyph == nullptr) {
    return;
  }

  for (rowIndex = 0U; rowIndex < 8U; ++rowIndex) {
    glyph[rowIndex] = 0U;
  }

  glyph[1] = lcdStatusRowMask(zone1On, zone2On);
  glyph[2] = lcdStatusRowMask(zone1On, zone2On);
  glyph[5] = lcdStatusRowMask(zone3On, pipeOn);
  glyph[6] = lcdStatusRowMask(zone3On, pipeOn);
}

void buildLcdSensorStatusGlyph(const commissioning::SensorRecord sensors[],
                               size_t sensorCount,
                               unsigned long nowMs,
                               unsigned long blinkPeriodMs,
                               uint8_t glyph[8]) {
  buildLcdSensorStatusGlyphFromFlags(sensorStatusLedOn(sensors, sensorCount, 0U, nowMs, blinkPeriodMs),
                                     sensorStatusLedOn(sensors, sensorCount, 1U, nowMs, blinkPeriodMs),
                                     sensorStatusLedOn(sensors, sensorCount, 2U, nowMs, blinkPeriodMs),
                                     sensorStatusLedOn(sensors, sensorCount, 3U, nowMs, blinkPeriodMs),
                                     glyph);
}

}  // namespace sensor_status_view
