#include "controller_ui.h"

#include <math.h>

namespace controller_ui {

namespace {

bool stageMenuItemAvailable(StageMenuItem menuItem, bool hasViewData, bool hasCalibration) {
  switch (menuItem) {
    case STAGE_MENU_VIEW_DATA:
      return hasViewData;
    case STAGE_MENU_CALIBRATE:
      return hasCalibration;
    case STAGE_MENU_CAL_PWM:
    case STAGE_MENU_EXIT:
    default:
      return true;
  }
}

StageMenuItem defaultStageMenuItem(bool hasViewData, bool hasCalibration) {
  static const StageMenuItem kPriorityOrder[] = {
      STAGE_MENU_VIEW_DATA,
      STAGE_MENU_CALIBRATE,
      STAGE_MENU_CAL_PWM,
      STAGE_MENU_EXIT,
  };
  size_t itemIndex = 0U;

  for (itemIndex = 0U; itemIndex < (sizeof(kPriorityOrder) / sizeof(kPriorityOrder[0])); ++itemIndex) {
    if (stageMenuItemAvailable(kPriorityOrder[itemIndex], hasViewData, hasCalibration)) {
      return kPriorityOrder[itemIndex];
    }
  }

  return STAGE_MENU_EXIT;
}

StageMenuItem adjacentStageMenuItem(StageMenuItem currentItem,
                                    bool forward,
                                    bool hasViewData,
                                    bool hasCalibration) {
  uint8_t offset = 0U;

  for (offset = 1U; offset < STAGE_MENU_COUNT; ++offset) {
    const uint8_t candidateIndex =
        forward ? static_cast<uint8_t>((static_cast<uint8_t>(currentItem) + offset) % STAGE_MENU_COUNT)
                : static_cast<uint8_t>((static_cast<uint8_t>(currentItem) + STAGE_MENU_COUNT - offset) %
                                       STAGE_MENU_COUNT);
    const StageMenuItem candidateItem = stageMenuItemFromIndex(candidateIndex);

    if (stageMenuItemAvailable(candidateItem, hasViewData, hasCalibration)) {
      return candidateItem;
    }
  }

  return currentItem;
}

}  // namespace

State::State()
    : page(DISPLAY_PAGE_SUMMARY),
      overlayMode(UI_OVERLAY_NONE),
      menuIndex(static_cast<uint8_t>(STAGE_MENU_EXIT)),
      browseSensorIndex(commissioning::kInvalidSensorIndex),
      calibrationSensorIndex(commissioning::kInvalidSensorIndex),
      calibrationField(CAL_FIELD_TEMP),
      pendingTempC(NAN),
      pendingRhPercent(NAN),
      calibrationEditorActive(false) {}

bool State::overlayActive() const {
  return overlayMode != UI_OVERLAY_NONE;
}

bool State::manualCalibrationModeActive() const {
  return overlayMode == UI_OVERLAY_CAL_EDIT;
}

bool State::pwmCalibrationModeActive() const {
  return overlayMode == UI_OVERLAY_PWM_CAL;
}

bool State::menuAllowed(commissioning::Mode mode, commissioning::FaultCode faultCode) const {
  (void)mode;
  (void)faultCode;
  return true;
}

size_t State::activeCalibrationSensorIndex(commissioning::Mode mode,
                                           size_t highlightedSensorIndex) const {
  (void)mode;
  (void)highlightedSensorIndex;
  if (manualCalibrationModeActive()) {
    return calibrationSensorIndex;
  }

  return commissioning::kInvalidSensorIndex;
}

void State::closeOverlay() {
  overlayMode = UI_OVERLAY_NONE;
}

void State::openStageMenu(bool hasViewData, bool hasCalibration) {
  overlayMode = UI_OVERLAY_STAGE_MENU;
  menuIndex = static_cast<uint8_t>(defaultStageMenuItem(hasViewData, hasCalibration));
}

void State::openSensorView(size_t sensorIndex) {
  overlayMode = UI_OVERLAY_SENSOR_VIEW;
  browseSensorIndex = sensorIndex;
}

void State::openCalibrationSelection(size_t sensorIndex) {
  overlayMode = UI_OVERLAY_CAL_SELECT;
  browseSensorIndex = sensorIndex;
}

void State::openManualCalibrationEditor(size_t sensorIndex) {
  overlayMode = UI_OVERLAY_CAL_EDIT;
  calibrationSensorIndex = sensorIndex;
  calibrationEditorActive = false;
}

void State::openPwmCalibration() {
  overlayMode = UI_OVERLAY_PWM_CAL;
}

void State::returnToCalibrationSelection(size_t sensorIndex) {
  overlayMode = UI_OVERLAY_CAL_SELECT;
  browseSensorIndex = sensorIndex;
  calibrationEditorActive = false;
}

void State::resetManualCalibrationSelection() {
  calibrationSensorIndex = commissioning::kInvalidSensorIndex;
  calibrationEditorActive = false;
}

void State::moveStageMenu(bool forward, bool hasViewData, bool hasCalibration) {
  menuIndex = static_cast<uint8_t>(
      adjacentStageMenuItem(currentStageMenuItem(), forward, hasViewData, hasCalibration));
}

StageMenuItem State::currentStageMenuItem() const {
  return stageMenuItemFromIndex(menuIndex);
}

void State::advanceDisplayPage(bool forward) {
  if (forward) {
    page = static_cast<DisplayPage>((page + 1U) % DISPLAY_PAGE_COUNT);
    return;
  }

  if (page == DISPLAY_PAGE_SUMMARY) {
    page = static_cast<DisplayPage>(DISPLAY_PAGE_COUNT - 1U);
    return;
  }

  page = static_cast<DisplayPage>(page - 1U);
}

StageMenuItem stageMenuItemFromIndex(uint8_t menuIndex) {
  switch (menuIndex) {
    case STAGE_MENU_VIEW_DATA:
      return STAGE_MENU_VIEW_DATA;
    case STAGE_MENU_CALIBRATE:
      return STAGE_MENU_CALIBRATE;
    case STAGE_MENU_CAL_PWM:
      return STAGE_MENU_CAL_PWM;
    case STAGE_MENU_EXIT:
    default:
      return STAGE_MENU_EXIT;
  }
}

const char *stageMenuItemName(StageMenuItem menuItem) {
  switch (menuItem) {
    case STAGE_MENU_VIEW_DATA:
      return "View data";
    case STAGE_MENU_CALIBRATE:
      return "Calibrate";
    case STAGE_MENU_CAL_PWM:
      return "Cal PWM";
    case STAGE_MENU_EXIT:
    default:
      return "Return";
  }
}

}  // namespace controller_ui
