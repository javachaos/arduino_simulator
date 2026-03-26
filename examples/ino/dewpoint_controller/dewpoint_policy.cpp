#include "dewpoint_policy.h"

#include <math.h>
#include <stdint.h>

namespace dewpoint_policy {

namespace {

constexpr float kKelvinOffset = 273.15f;
constexpr float kMinSolverTempK = 173.15f;  // -100 C
constexpr float kMinRhPercentForLog = 0.001f;
constexpr float kMinSolverTempC = kMinSolverTempK - kKelvinOffset;

bool isFiniteFloat(float value) {
  return !isnan(value) && !isinf(value);
}

float clampFloat(float value, float minValue, float maxValue) {
  if (value < minValue) {
    return minValue;
  }

  if (value > maxValue) {
    return maxValue;
  }

  return value;
}

Decision makeBaseDecision(size_t airZoneCount) {
  Decision decision = {};
  size_t zoneIndex = 0U;

  decision.airZoneCount = airZoneCount;
  decision.pipeTempC = NAN;
  decision.worstDewPointC = NAN;
  decision.minSafeColdTempC = NAN;
  decision.allInputsValid = false;
  decision.invariantsOk = false;
  decision.faultCode = FAULT_INTERNAL_INVARIANT;
  decision.command = VALVE_HOLD;

  for (zoneIndex = 0U; zoneIndex < kMaxAirZones; ++zoneIndex) {
    decision.airZones[zoneIndex].airTempC = NAN;
    decision.airZones[zoneIndex].rhPercent = NAN;
    decision.airZones[zoneIndex].dewPointC = NAN;
    decision.airZones[zoneIndex].valid = false;
  }

  return decision;
}

Decision makeFaultDecision(size_t airZoneCount, FaultCode faultCode, ValveCommand command) {
  Decision decision = makeBaseDecision(airZoneCount);

  decision.faultCode = faultCode;
  decision.command = command;
  decision.invariantsOk = true;
  return decision;
}

bool isValidTempC(const Config &config, float value) {
  return isFiniteFloat(value) && value >= config.minValidTempC && value <= config.maxValidTempC;
}

bool isValidRhPercent(const Config &config, float value) {
  return isFiniteFloat(value) && value >= config.minValidRhPercent &&
         value <= config.maxValidRhPercent;
}

size_t populateAirZoneStatuses(const Config &config,
                               const Inputs &inputs,
                               Decision *decision,
                               float *worstDewPointC) {
  size_t validZoneCount = 0U;
  float runningWorstDewPointC = -1000.0f;
  size_t zoneIndex = 0U;

  for (zoneIndex = 0U; zoneIndex < config.airZoneCount; ++zoneIndex) {
    const AirZoneInput &zoneInput = inputs.airZones[zoneIndex];
    AirZoneStatus &zoneStatus = decision->airZones[zoneIndex];

    zoneStatus.airTempC = zoneInput.airTempC;
    zoneStatus.rhPercent = zoneInput.rhPercent;

    if (!zoneInput.valid || !isValidTempC(config, zoneInput.airTempC) ||
        !isValidRhPercent(config, zoneInput.rhPercent)) {
      continue;
    }

    zoneStatus.dewPointC = computeDewPointC(zoneInput.airTempC, zoneInput.rhPercent);
    if (!isFiniteFloat(zoneStatus.dewPointC)) {
      zoneStatus.dewPointC = NAN;
      continue;
    }

    zoneStatus.valid = true;
    validZoneCount += 1U;
    if (zoneStatus.dewPointC > runningWorstDewPointC) {
      runningWorstDewPointC = zoneStatus.dewPointC;
    }
  }

  *worstDewPointC = runningWorstDewPointC;
  return validZoneCount;
}

bool populatePipeTemperature(const Config &config, const Inputs &inputs, Decision *decision) {
  if (!inputs.pipeTempValid || !isValidTempC(config, inputs.pipeTempC)) {
    return false;
  }

  decision->pipeTempC = inputs.pipeTempC;
  return true;
}

bool validDecisionHasFiniteFields(const Decision &decision) {
  size_t zoneIndex = 0U;
  size_t validZoneCount = 0U;

  if (!decision.allInputsValid) {
    return false;
  }

  if (!isFiniteFloat(decision.pipeTempC) || !isFiniteFloat(decision.worstDewPointC) ||
      !isFiniteFloat(decision.minSafeColdTempC)) {
    return false;
  }

  if (decision.minSafeColdTempC < decision.worstDewPointC) {
    return false;
  }

  for (zoneIndex = 0U; zoneIndex < decision.airZoneCount; ++zoneIndex) {
    if (decision.airZones[zoneIndex].valid) {
      validZoneCount += 1U;
    }
  }

  if (validZoneCount == 0U) {
    return false;
  }

  return true;
}

bool zoneStatusSaneForControl(const Config &config, const AirZoneStatus &zoneStatus) {
  return zoneStatus.valid && isValidTempC(config, zoneStatus.airTempC) &&
         isValidRhPercent(config, zoneStatus.rhPercent) && isFiniteFloat(zoneStatus.dewPointC);
}

float sonntagLnSaturationVaporPressureWater(float tempK) {
  return (-6096.9385f / tempK) + 21.2409642f - (2.711193e-2f * tempK) +
         (1.673952e-5f * tempK * tempK) + (2.433502f * logf(tempK));
}

}  // namespace

float computeDewPointC(float airTempC, float rhPercent) {
  const float boundedRhPercent = clampFloat(rhPercent, kMinRhPercentForLog, 100.0f);
  const float airTempK = airTempC + kKelvinOffset;
  const float targetLnVaporPressure =
      sonntagLnSaturationVaporPressureWater(airTempK) + logf(boundedRhPercent / 100.0f);
  float lowTempK = kMinSolverTempK;
  float highTempK = airTempK;
  uint8_t iteration = 0U;

  for (iteration = 0U; iteration < 40U; ++iteration) {
    const float midTempK = 0.5f * (lowTempK + highTempK);
    if (sonntagLnSaturationVaporPressureWater(midTempK) < targetLnVaporPressure) {
      lowTempK = midTempK;
    } else {
      highTempK = midTempK;
    }
  }

  return (0.5f * (lowTempK + highTempK)) - kKelvinOffset;
}

bool isValidConfig(const Config &config) {
  if (config.airZoneCount == 0U || config.airZoneCount > kMaxAirZones) {
    return false;
  }

  if (!isFiniteFloat(config.safetyMarginC) || config.safetyMarginC < 0.0f) {
    return false;
  }

  if (!isFiniteFloat(config.controlDeadbandC) || config.controlDeadbandC < 0.0f) {
    return false;
  }

  if (!isFiniteFloat(config.minValidTempC) || !isFiniteFloat(config.maxValidTempC) ||
      config.minValidTempC > config.maxValidTempC) {
    return false;
  }

  if (config.minValidTempC < kMinSolverTempC) {
    return false;
  }

  if (!isFiniteFloat(config.minValidRhPercent) || !isFiniteFloat(config.maxValidRhPercent) ||
      config.minValidRhPercent > config.maxValidRhPercent) {
    return false;
  }

  if (config.minValidRhPercent < 0.0f || config.maxValidRhPercent > 100.0f) {
    return false;
  }

  return true;
}

bool isValidValveCommand(ValveCommand command) {
  return command == VALVE_HOLD || command == VALVE_WARMER || command == VALVE_COOLER;
}

bool decisionHasRequiredFields(const Decision &decision) {
  if (!isValidValveCommand(decision.command)) {
    return false;
  }

  if (decision.faultCode == FAULT_NONE) {
    return validDecisionHasFiniteFields(decision);
  }

  if (decision.faultCode == FAULT_INVALID_AIR_SENSOR ||
      decision.faultCode == FAULT_INVALID_PIPE_SENSOR) {
    return !decision.allInputsValid && decision.command == VALVE_WARMER;
  }

  if (decision.faultCode == FAULT_COMMISSIONING_INCOMPLETE ||
      decision.faultCode == FAULT_INVALID_CONFIG ||
      decision.faultCode == FAULT_INTERNAL_INVARIANT) {
    return !decision.allInputsValid && decision.command == VALVE_HOLD;
  }

  return false;
}

bool inputsSaneForControl(const Config &config, const Inputs &inputs) {
  size_t validZoneCount = 0U;
  size_t zoneIndex = 0U;

  if (!isValidConfig(config) || !inputs.commissioningComplete || !inputs.pipeTempValid ||
      !isValidTempC(config, inputs.pipeTempC)) {
    return false;
  }

  for (zoneIndex = 0U; zoneIndex < config.airZoneCount; ++zoneIndex) {
    if (!inputs.airZones[zoneIndex].valid) {
      continue;
    }

    if (!isValidTempC(config, inputs.airZones[zoneIndex].airTempC) ||
        !isValidRhPercent(config, inputs.airZones[zoneIndex].rhPercent)) {
      return false;
    }

    validZoneCount += 1U;
  }

  return validZoneCount > 0U;
}

bool decisionSafeForControl(const Config &config, const Inputs &inputs, const Decision &decision) {
  size_t validZoneCount = 0U;
  size_t zoneIndex = 0U;

  if (!inputsSaneForControl(config, inputs) || decision.faultCode != FAULT_NONE ||
      !decision.invariantsOk || !decision.allInputsValid || !decisionHasRequiredFields(decision) ||
      decision.airZoneCount != config.airZoneCount) {
    return false;
  }

  if (!isFiniteFloat(decision.pipeTempC) || !isFiniteFloat(decision.worstDewPointC) ||
      !isFiniteFloat(decision.minSafeColdTempC) ||
      decision.minSafeColdTempC < decision.worstDewPointC) {
    return false;
  }

  for (zoneIndex = 0U; zoneIndex < decision.airZoneCount; ++zoneIndex) {
    if (!decision.airZones[zoneIndex].valid) {
      continue;
    }

    if (!zoneStatusSaneForControl(config, decision.airZones[zoneIndex])) {
      return false;
    }

    validZoneCount += 1U;
  }

  if (validZoneCount == 0U) {
    return false;
  }

  if (!inputs.coolingDemandActive && decision.command == VALVE_COOLER) {
    return false;
  }

  if (decision.pipeTempC < decision.minSafeColdTempC && decision.command != VALVE_WARMER) {
    return false;
  }

  return true;
}

Decision evaluate(const Config &config, const Inputs &inputs) {
  Decision decision = makeBaseDecision(config.airZoneCount);
  float worstDewPointC = NAN;
  size_t validAirZoneCount = 0U;
  bool pipeTempValid = false;

  if (!isValidConfig(config)) {
    return makeFaultDecision(config.airZoneCount, FAULT_INVALID_CONFIG, VALVE_HOLD);
  }

  if (!inputs.commissioningComplete) {
    return makeFaultDecision(config.airZoneCount, FAULT_COMMISSIONING_INCOMPLETE, VALVE_HOLD);
  }

  validAirZoneCount = populateAirZoneStatuses(config, inputs, &decision, &worstDewPointC);
  pipeTempValid = populatePipeTemperature(config, inputs, &decision);
  if (validAirZoneCount == 0U) {
    decision = makeFaultDecision(config.airZoneCount, FAULT_INVALID_AIR_SENSOR, VALVE_WARMER);
    return decision;
  }

  if (!pipeTempValid) {
    decision = makeFaultDecision(config.airZoneCount, FAULT_INVALID_PIPE_SENSOR, VALVE_WARMER);
    return decision;
  }

  decision.worstDewPointC = worstDewPointC;
  decision.minSafeColdTempC = decision.worstDewPointC + config.safetyMarginC;
  decision.allInputsValid = true;
  decision.faultCode = FAULT_NONE;

  if (decision.pipeTempC < decision.minSafeColdTempC) {
    decision.command = VALVE_WARMER;
  } else if (inputs.coolingDemandActive &&
             decision.pipeTempC > (decision.minSafeColdTempC + config.controlDeadbandC)) {
    decision.command = VALVE_COOLER;
  } else {
    decision.command = VALVE_HOLD;
  }

  decision.invariantsOk = decisionHasRequiredFields(decision);
  if (decision.invariantsOk) {
    return decision;
  }

  return makeFaultDecision(config.airZoneCount, FAULT_INTERNAL_INVARIANT, VALVE_HOLD);
}

const char *faultCodeName(FaultCode faultCode) {
  switch (faultCode) {
    case FAULT_NONE:
      return "NONE";
    case FAULT_COMMISSIONING_INCOMPLETE:
      return "COMMISSIONING_INCOMPLETE";
    case FAULT_INVALID_CONFIG:
      return "INVALID_CONFIG";
    case FAULT_INVALID_AIR_SENSOR:
      return "INVALID_AIR_SENSOR";
    case FAULT_INVALID_PIPE_SENSOR:
      return "INVALID_PIPE_SENSOR";
    case FAULT_INTERNAL_INVARIANT:
    default:
      return "INTERNAL_INVARIANT";
  }
}

const char *valveCommandName(ValveCommand command) {
  switch (command) {
    case VALVE_WARMER:
      return "WARMER";
    case VALVE_COOLER:
      return "COOLER";
    case VALVE_HOLD:
    default:
      return "HOLD";
  }
}

}  // namespace dewpoint_policy
