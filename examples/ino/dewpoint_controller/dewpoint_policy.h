#ifndef DEWPOINT_POLICY_H
#define DEWPOINT_POLICY_H

#include <stddef.h>

namespace dewpoint_policy {

constexpr size_t kMaxAirZones = 3U;

enum ValveCommand {
  VALVE_HOLD = 0,
  VALVE_WARMER = 1,
  VALVE_COOLER = 2
};

enum FaultCode {
  FAULT_NONE = 0,
  FAULT_COMMISSIONING_INCOMPLETE = 1,
  FAULT_INVALID_CONFIG = 2,
  FAULT_INVALID_AIR_SENSOR = 3,
  FAULT_INVALID_PIPE_SENSOR = 4,
  FAULT_INTERNAL_INVARIANT = 5
};

struct Config {
  size_t airZoneCount;
  float safetyMarginC;
  float controlDeadbandC;
  float minValidTempC;
  float maxValidTempC;
  float minValidRhPercent;
  float maxValidRhPercent;
};

struct AirZoneInput {
  float airTempC;
  float rhPercent;
  bool valid;
};

struct Inputs {
  AirZoneInput airZones[kMaxAirZones];
  float pipeTempC;
  bool pipeTempValid;
  bool coolingDemandActive;
  bool commissioningComplete;
};

struct AirZoneStatus {
  float airTempC;
  float rhPercent;
  float dewPointC;
  bool valid;
};

struct Decision {
  AirZoneStatus airZones[kMaxAirZones];
  size_t airZoneCount;
  float pipeTempC;
  float worstDewPointC;
  float minSafeColdTempC;
  bool allInputsValid;
  bool invariantsOk;
  FaultCode faultCode;
  ValveCommand command;
};

// Computes dew point from dry-bulb temperature and relative humidity using the
// full Sonntag saturation-vapor-pressure formulation over liquid water.
float computeDewPointC(float airTempC, float rhPercent);

// Validates control configuration limits and zone-count bounds.
bool isValidConfig(const Config &config);

// Returns true when the command enum is within the defined valve command range.
bool isValidValveCommand(ValveCommand command);

// Returns true when the decision carries the fields required for safe downstream use.
bool decisionHasRequiredFields(const Decision &decision);

// Returns true when inputs are suitable for executing closed-loop control.
bool inputsSaneForControl(const Config &config, const Inputs &inputs);

// Returns true when a policy decision is safe to hand to the actuator layer.
bool decisionSafeForControl(const Config &config, const Inputs &inputs, const Decision &decision);

// Evaluates the current air and pipe conditions into a control decision.
Decision evaluate(const Config &config, const Inputs &inputs);

// Returns a stable display/debug name for a policy fault code.
const char *faultCodeName(FaultCode faultCode);

// Returns a stable display/debug name for a valve command.
const char *valveCommandName(ValveCommand command);

}  // namespace dewpoint_policy

#endif
