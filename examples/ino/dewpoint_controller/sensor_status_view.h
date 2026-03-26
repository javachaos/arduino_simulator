#ifndef SENSOR_STATUS_VIEW_H
#define SENSOR_STATUS_VIEW_H

#include <stddef.h>
#include <stdint.h>

#include "commissioning_state.h"

namespace sensor_status_view {

// Returns true when the chosen sensor status LED should blink instead of hold steady.
bool sensorStatusLedShouldBlink(const commissioning::SensorRecord sensors[],
                                size_t sensorCount,
                                size_t sensorIndex);

// Returns the instantaneous on/off state for one sensor status LED.
bool sensorStatusLedOn(const commissioning::SensorRecord sensors[],
                       size_t sensorCount,
                       size_t sensorIndex,
                       unsigned long nowMs,
                       unsigned long blinkPeriodMs);

// Builds the 5x8 LCD custom-character glyph from four explicit quadrant flags.
void buildLcdSensorStatusGlyphFromFlags(bool zone1On,
                                        bool zone2On,
                                        bool zone3On,
                                        bool pipeOn,
                                        uint8_t glyph[8]);

// Builds the 5x8 LCD custom-character glyph from the controller sensor inventory.
void buildLcdSensorStatusGlyph(const commissioning::SensorRecord sensors[],
                               size_t sensorCount,
                               unsigned long nowMs,
                               unsigned long blinkPeriodMs,
                               uint8_t glyph[8]);

}  // namespace sensor_status_view

#endif
