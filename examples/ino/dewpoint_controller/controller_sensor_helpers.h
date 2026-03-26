#ifndef CONTROLLER_SENSOR_HELPERS_H
#define CONTROLLER_SENSOR_HELPERS_H

#include <stddef.h>

#include "commissioning_state.h"

namespace controller_sensor_helpers {

// Counts sensors that currently have a staged or commissioned role assignment.
size_t stagedRoleCount(const commissioning::SensorRecord sensors[], size_t sensorCount);

// Counts sensors that are presently discovered on the live bus.
size_t discoveredSensorCount(const commissioning::SensorRecord sensors[], size_t sensorCount);

// Returns true when all required roles are assigned.
bool stagedCommissioningRoleAssignmentComplete(const commissioning::SensorRecord sensors[],
                                               size_t sensorCount);

// Returns true when the staged commissioning workflow can advance immediately.
bool stagedWorkflowCanContinue(const commissioning::SensorRecord sensors[], size_t sensorCount);

// Returns true when staged commissioning is blocked waiting on another live sensor.
bool stagedWorkflowNeedsAdditionalSensor(const commissioning::SensorRecord sensors[],
                                         size_t sensorCount);

// Finds the next discovered sensor, wrapping around the inventory if needed.
size_t findNextDiscoveredSensor(const commissioning::SensorRecord sensors[],
                                size_t sensorCount,
                                size_t startAfterIndex,
                                bool forward);

// Returns true when a sensor is both role-assigned and live enough for menu calibration.
bool sensorCanBeMenuCalibrated(const commissioning::SensorRecord sensors[],
                               size_t sensorCount,
                               size_t sensorIndex);

// Finds the next role-assigned live sensor suitable for menu-driven calibration.
size_t findNextMenuCalibratableSensor(const commissioning::SensorRecord sensors[],
                                      size_t sensorCount,
                                      size_t startAfterIndex,
                                      bool forward);

}  // namespace controller_sensor_helpers

#endif
