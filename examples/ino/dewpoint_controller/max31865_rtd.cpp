#include "max31865_rtd.h"

#include <math.h>

namespace max31865_rtd {

namespace {

float evaluateResistance(const float tempC, const RtdModel &model) {
  const float tempSquared = tempC * tempC;

  if (tempC >= 0.0f) {
    return model.nominalResistanceOhms *
           (1.0f + (model.coefficientA * tempC) + (model.coefficientB * tempSquared));
  }

  return model.nominalResistanceOhms *
         (1.0f + (model.coefficientA * tempC) + (model.coefficientB * tempSquared) +
          (model.coefficientC * (tempC - 100.0f) * tempC * tempSquared));
}

float evaluateDerivative(const float tempC, const RtdModel &model) {
  if (tempC >= 0.0f) {
    return model.nominalResistanceOhms *
           (model.coefficientA + (2.0f * model.coefficientB * tempC));
  }

  return model.nominalResistanceOhms *
         (model.coefficientA + (2.0f * model.coefficientB * tempC) +
          (model.coefficientC * ((4.0f * tempC * tempC * tempC) -
                                 (300.0f * tempC * tempC))));
}

}  // namespace

RtdModel pt1000Model() {
  RtdModel model = {};

  model.referenceResistorOhms = 4300.0f;
  model.nominalResistanceOhms = 1000.0f;
  model.coefficientA = 3.9083e-3f;
  model.coefficientB = -5.775e-7f;
  model.coefficientC = -4.183e-12f;
  model.minTempC = -50.0f;
  model.maxTempC = 200.0f;
  return model;
}

uint8_t buildConfigByte(const bool enableBias,
                        const bool oneShot,
                        const bool threeWire,
                        const bool clearFault,
                        const FilterMode filterMode) {
  uint8_t value = 0U;

  if (enableBias) {
    value |= kConfigBias;
  }
  if (oneShot) {
    value |= kConfigOneShot;
  }
  if (threeWire) {
    value |= kConfigThreeWire;
  }
  if (clearFault) {
    value |= kConfigFaultClear;
  }
  if (filterMode == FILTER_50HZ) {
    value |= kConfigFilter50Hz;
  }

  return value;
}

bool rawCodeToResistanceOhms(const uint16_t rawCode,
                             const RtdModel &model,
                             float *const resistanceOhms) {
  if (resistanceOhms == nullptr || !isfinite(model.referenceResistorOhms) ||
      model.referenceResistorOhms <= 0.0f || rawCode > 0x7FFFU) {
    return false;
  }

  *resistanceOhms =
      (static_cast<float>(rawCode) * model.referenceResistorOhms) / 32768.0f;
  return isfinite(*resistanceOhms);
}

bool temperatureCToResistanceOhms(const float tempC,
                                  const RtdModel &model,
                                  float *const resistanceOhms) {
  if (resistanceOhms == nullptr || !isfinite(tempC) || !isfinite(model.nominalResistanceOhms) ||
      model.nominalResistanceOhms <= 0.0f || tempC < model.minTempC ||
      tempC > model.maxTempC) {
    return false;
  }

  *resistanceOhms = evaluateResistance(tempC, model);
  return isfinite(*resistanceOhms) && *resistanceOhms > 0.0f;
}

bool resistanceOhmsToTemperatureC(const float resistanceOhms,
                                  const RtdModel &model,
                                  float *const tempC) {
  float minResistance = 0.0f;
  float maxResistance = 0.0f;
  float estimate = 0.0f;
  int iteration = 0;

  if (tempC == nullptr || !isfinite(resistanceOhms) || resistanceOhms <= 0.0f ||
      !temperatureCToResistanceOhms(model.minTempC, model, &minResistance) ||
      !temperatureCToResistanceOhms(model.maxTempC, model, &maxResistance) ||
      resistanceOhms < minResistance || resistanceOhms > maxResistance) {
    return false;
  }

  estimate =
      ((resistanceOhms / model.nominalResistanceOhms) - 1.0f) / model.coefficientA;
  if (!isfinite(estimate)) {
    return false;
  }

  if (estimate < model.minTempC) {
    estimate = model.minTempC;
  } else if (estimate > model.maxTempC) {
    estimate = model.maxTempC;
  }

  for (iteration = 0; iteration < 8; ++iteration) {
    const float residual = evaluateResistance(estimate, model) - resistanceOhms;
    const float derivative = evaluateDerivative(estimate, model);

    if (!isfinite(residual) || !isfinite(derivative) || fabsf(derivative) < 1.0e-6f) {
      return false;
    }

    estimate -= residual / derivative;
    if (estimate < model.minTempC) {
      estimate = model.minTempC;
    } else if (estimate > model.maxTempC) {
      estimate = model.maxTempC;
    }
  }

  *tempC = estimate;
  return isfinite(*tempC);
}

}  // namespace max31865_rtd
