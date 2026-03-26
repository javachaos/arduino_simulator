#ifndef CONTROL_SAFETY_H
#define CONTROL_SAFETY_H

#include "dewpoint_policy.h"

namespace control_safety {

// Returns true when all valid air-zone humidity inputs remain finite and within policy bounds.
bool humidityInputsRemainSane(const dewpoint_policy::Config &config,
                              const dewpoint_policy::Inputs &inputs);

// Returns true when a control-ready decision still satisfies the key runtime safety invariants.
bool decisionInvariantsHold(const dewpoint_policy::Config &config,
                            const dewpoint_policy::Inputs &inputs,
                            const dewpoint_policy::Decision &decision);

}  // namespace control_safety

#endif
