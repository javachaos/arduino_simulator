#include "modulating_actuator.h"

#include <math.h>

namespace modulating_actuator {

namespace {

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

float warmExtremePercent(const Config &config) {
  return config.warmerOnHigherSignal ? config.maxCommandPercent : config.minCommandPercent;
}

void setFault(State *state, FaultCode faultCode) {
  state->faultCode = faultCode;
  state->trackingFault = faultCode == FAULT_TRACKING_ERROR;
}

}  // namespace

bool isValidConfig(const Config &config) {
  if (!isFiniteFloat(config.minCommandPercent) || !isFiniteFloat(config.maxCommandPercent) ||
      config.minCommandPercent < 0.0f || config.maxCommandPercent > 100.0f ||
      config.minCommandPercent >= config.maxCommandPercent) {
    return false;
  }

  if (!isFiniteFloat(config.stepPercent) || config.stepPercent <= 0.0f ||
      config.stepPercent > (config.maxCommandPercent - config.minCommandPercent)) {
    return false;
  }

  if (!isFiniteFloat(config.feedbackTolerancePercent) || config.feedbackTolerancePercent < 0.0f ||
      config.feedbackTolerancePercent > 100.0f) {
    return false;
  }

  return true;
}

bool invariantsHold(const Config &config, const State &state) {
  if (!isValidConfig(config) || !state.initialized) {
    return false;
  }

  if (!isFiniteFloat(state.commandedPercent) || state.commandedPercent < config.minCommandPercent ||
      state.commandedPercent > config.maxCommandPercent) {
    return false;
  }

  if (state.feedbackValid &&
      (!isFiniteFloat(state.feedbackPercent) || state.feedbackPercent < config.minCommandPercent ||
       state.feedbackPercent > config.maxCommandPercent)) {
    return false;
  }

  if (state.trackingFault && state.faultCode != FAULT_TRACKING_ERROR) {
    return false;
  }

  if (!state.feedbackValid && config.feedbackRequired && state.faultCode == FAULT_NONE) {
    return false;
  }

  return true;
}

void initialize(const Config &config, float initialCommandPercent, unsigned long nowMs, State *state) {
  if (state == nullptr) {
    return;
  }

  *state = {};
  state->initialized = true;
  state->lastCommandChangeMs = nowMs;

  if (!isValidConfig(config)) {
    state->commandedPercent = 0.0f;
    setFault(state, FAULT_INVALID_CONFIG);
    return;
  }

  state->commandedPercent =
      clampFloat(initialCommandPercent, config.minCommandPercent, config.maxCommandPercent);
  state->feedbackPercent = NAN;
  state->feedbackValid = false;
  setFault(state, config.feedbackRequired ? FAULT_FEEDBACK_INVALID : FAULT_NONE);
}

void forceWarmPosition(const Config &config, unsigned long nowMs, State *state) {
  if (state == nullptr || !state->initialized) {
    return;
  }

  if (!isValidConfig(config)) {
    setFault(state, FAULT_INVALID_CONFIG);
    return;
  }

  state->commandedPercent = warmExtremePercent(config);
  state->lastCommandChangeMs = nowMs;
  if (state->feedbackValid) {
    setFault(state, FAULT_NONE);
  } else {
    setFault(state, config.feedbackRequired ? FAULT_FEEDBACK_INVALID : FAULT_NONE);
  }
}

void applyDecision(const Config &config,
                   dewpoint_policy::ValveCommand command,
                   unsigned long nowMs,
                   State *state) {
  float nextCommandPercent = 0.0f;
  const float warmStep = config.warmerOnHigherSignal ? config.stepPercent : -config.stepPercent;

  if (state == nullptr || !state->initialized) {
    return;
  }

  if (!isValidConfig(config) || !dewpoint_policy::isValidValveCommand(command)) {
    setFault(state, FAULT_INVALID_CONFIG);
    return;
  }

  nextCommandPercent = state->commandedPercent;
  switch (command) {
    case dewpoint_policy::VALVE_WARMER:
      nextCommandPercent += warmStep;
      break;
    case dewpoint_policy::VALVE_COOLER:
      nextCommandPercent -= warmStep;
      break;
    case dewpoint_policy::VALVE_HOLD:
    default:
      break;
  }

  nextCommandPercent =
      clampFloat(nextCommandPercent, config.minCommandPercent, config.maxCommandPercent);
  if (fabsf(nextCommandPercent - state->commandedPercent) > 0.001f) {
    state->commandedPercent = nextCommandPercent;
    state->lastCommandChangeMs = nowMs;
  }

  if (state->trackingFault) {
    setFault(state, FAULT_TRACKING_ERROR);
  } else if (!state->feedbackValid && config.feedbackRequired) {
    setFault(state, FAULT_FEEDBACK_INVALID);
  } else {
    setFault(state, FAULT_NONE);
  }
}

void updateFeedback(const Config &config,
                    float feedbackPercent,
                    bool feedbackValid,
                    unsigned long nowMs,
                    State *state) {
  const bool usableFeedback =
      feedbackValid && isFiniteFloat(feedbackPercent) && feedbackPercent >= config.minCommandPercent &&
      feedbackPercent <= config.maxCommandPercent;

  if (state == nullptr || !state->initialized) {
    return;
  }

  if (!isValidConfig(config)) {
    setFault(state, FAULT_INVALID_CONFIG);
    return;
  }

  if (!usableFeedback) {
    state->feedbackPercent = NAN;
    state->feedbackValid = false;
    setFault(state, config.feedbackRequired ? FAULT_FEEDBACK_INVALID : FAULT_NONE);
    return;
  }

  state->feedbackPercent = feedbackPercent;
  state->feedbackValid = true;
  if ((nowMs - state->lastCommandChangeMs) >= config.feedbackSettleMs &&
      fabsf(state->feedbackPercent - state->commandedPercent) > config.feedbackTolerancePercent) {
    setFault(state, FAULT_TRACKING_ERROR);
    return;
  }

  setFault(state, FAULT_NONE);
}

float commandVoltageV(const State &state) {
  if (!isFiniteFloat(state.commandedPercent)) {
    return NAN;
  }

  return state.commandedPercent * 0.1f;
}

const char *faultCodeName(FaultCode faultCode) {
  switch (faultCode) {
    case FAULT_NONE:
      return "NONE";
    case FAULT_INVALID_CONFIG:
      return "INVALID_CONFIG";
    case FAULT_INVALID_STATE:
      return "INVALID_STATE";
    case FAULT_FEEDBACK_INVALID:
      return "FEEDBACK_INVALID";
    case FAULT_TRACKING_ERROR:
      return "TRACKING_ERROR";
    default:
      return "UNKNOWN";
  }
}

}  // namespace modulating_actuator
