#ifndef MODULATING_ACTUATOR_H
#define MODULATING_ACTUATOR_H

#include <stdint.h>

#include "dewpoint_policy.h"

namespace modulating_actuator {

enum FaultCode {
  FAULT_NONE = 0,
  FAULT_INVALID_CONFIG = 1,
  FAULT_INVALID_STATE = 2,
  FAULT_FEEDBACK_INVALID = 3,
  FAULT_TRACKING_ERROR = 4
};

struct Config {
  float minCommandPercent;
  float maxCommandPercent;
  float stepPercent;
  float feedbackTolerancePercent;
  unsigned long feedbackSettleMs;
  bool warmerOnHigherSignal;
  bool feedbackRequired;
};

struct State {
  float commandedPercent;
  float feedbackPercent;
  bool feedbackValid;
  bool trackingFault;
  FaultCode faultCode;
  unsigned long lastCommandChangeMs;
  bool initialized;
};

// Returns true when the actuator configuration bounds and step sizes are valid.
bool isValidConfig(const Config &config);

// Returns true when the actuator state satisfies internal invariants for the config.
bool invariantsHold(const Config &config, const State &state);

// Initializes actuator state with a known command and timestamp origin.
void initialize(const Config &config, float initialCommandPercent, unsigned long nowMs, State *state);

// Forces the actuator command to the warm-safe position.
void forceWarmPosition(const Config &config, unsigned long nowMs, State *state);

// Applies one policy command step to the actuator command state.
void applyDecision(const Config &config,
                   dewpoint_policy::ValveCommand command,
                   unsigned long nowMs,
                   State *state);

// Updates actuator feedback, validity, and tracking fault state.
void updateFeedback(const Config &config,
                    float feedbackPercent,
                    bool feedbackValid,
                    unsigned long nowMs,
                    State *state);

// Converts the commanded actuator percentage into the equivalent 0-10V target.
float commandVoltageV(const State &state);

// Returns a stable display/debug name for an actuator fault code.
const char *faultCodeName(FaultCode faultCode);

}  // namespace modulating_actuator

#endif
