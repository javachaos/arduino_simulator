#include "control_safety.h"

#include <math.h>

namespace control_safety {

namespace {

bool isFiniteFloat(float value) {
  return !isnan(value) && !isinf(value);
}

bool rhInputSaneForZone(const dewpoint_policy::Config &config,
                        const dewpoint_policy::AirZoneInput &zoneInput) {
  if (!zoneInput.valid) {
    return true;
  }

  return isFiniteFloat(zoneInput.rhPercent) && zoneInput.rhPercent >= config.minValidRhPercent &&
         zoneInput.rhPercent <= config.maxValidRhPercent;
}

}  // namespace

bool humidityInputsRemainSane(const dewpoint_policy::Config &config,
                              const dewpoint_policy::Inputs &inputs) {
  size_t zoneIndex = 0U;

  if (!dewpoint_policy::isValidConfig(config)) {
    return false;
  }

  for (zoneIndex = 0U; zoneIndex < config.airZoneCount; ++zoneIndex) {
    if (!rhInputSaneForZone(config, inputs.airZones[zoneIndex])) {
      return false;
    }
  }

  return true;
}

bool decisionInvariantsHold(const dewpoint_policy::Config &config,
                            const dewpoint_policy::Inputs &inputs,
                            const dewpoint_policy::Decision &decision) {
  if (!humidityInputsRemainSane(config, inputs)) {
    return false;
  }

  // The valve command must remain a single valid enum choice at all times.
  if (!dewpoint_policy::isValidValveCommand(decision.command)) {
    return false;
  }

  if (!dewpoint_policy::decisionSafeForControl(config, inputs, decision)) {
    return false;
  }

  if (decision.faultCode != dewpoint_policy::FAULT_NONE || !decision.allInputsValid ||
      !decision.invariantsOk) {
    return false;
  }

  if (!isFiniteFloat(decision.worstDewPointC) || !isFiniteFloat(decision.minSafeColdTempC) ||
      decision.minSafeColdTempC < decision.worstDewPointC) {
    return false;
  }

  if (!inputs.coolingDemandActive && decision.command == dewpoint_policy::VALVE_COOLER) {
    return false;
  }

  if (decision.pipeTempC < decision.minSafeColdTempC &&
      decision.command != dewpoint_policy::VALVE_WARMER) {
    return false;
  }

  return true;
}

}  // namespace control_safety
