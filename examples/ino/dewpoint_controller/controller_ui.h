#ifndef CONTROLLER_UI_H
#define CONTROLLER_UI_H

#include <stddef.h>

#include "commissioning_state.h"

namespace controller_ui {

enum UiKey {
  UI_KEY_NONE = 0,
  UI_KEY_RIGHT = 1,
  UI_KEY_UP = 2,
  UI_KEY_DOWN = 3,
  UI_KEY_LEFT = 4,
  UI_KEY_SELECT = 5,
};

struct UiInputEvent {
  UiKey key;
  bool shortPress;
  bool longPress;
};

enum DisplayPage {
  DISPLAY_PAGE_SUMMARY = 0,
  DISPLAY_PAGE_ZONE_1 = 1,
  DISPLAY_PAGE_ZONE_2 = 2,
  DISPLAY_PAGE_ZONE_3 = 3,
  DISPLAY_PAGE_PIPE = 4,
  DISPLAY_PAGE_COMMISSIONING = 5,
  DISPLAY_PAGE_COUNT = 6,
};

enum CalibrationField {
  CAL_FIELD_TEMP = 0,
  CAL_FIELD_RH = 1,
};

enum UiOverlayMode {
  UI_OVERLAY_NONE = 0,
  UI_OVERLAY_STAGE_MENU = 1,
  UI_OVERLAY_SENSOR_VIEW = 2,
  UI_OVERLAY_CAL_SELECT = 3,
  UI_OVERLAY_CAL_EDIT = 4,
  UI_OVERLAY_PWM_CAL = 5,
};

enum StageMenuItem {
  STAGE_MENU_VIEW_DATA = 0,
  STAGE_MENU_CALIBRATE = 1,
  STAGE_MENU_CAL_PWM = 2,
  STAGE_MENU_EXIT = 3,
  STAGE_MENU_COUNT = 4,
};

struct State {
  DisplayPage page;
  UiOverlayMode overlayMode;
  uint8_t menuIndex;
  size_t browseSensorIndex;
  size_t calibrationSensorIndex;
  CalibrationField calibrationField;
  float pendingTempC;
  float pendingRhPercent;
  bool calibrationEditorActive;

  // Builds the default UI state for a fresh boot or reset.
  State();

  // Returns true when any modal overlay is active on top of the base screen.
  bool overlayActive() const;

  // Returns true when the manual calibration editor overlay is active.
  bool manualCalibrationModeActive() const;

  // Returns true when the PWM calibration overlay is active.
  bool pwmCalibrationModeActive() const;

  // Returns true when the menu may be opened from the current commissioning state.
  bool menuAllowed(commissioning::Mode mode, commissioning::FaultCode faultCode) const;

  // Returns the sensor currently being calibrated, accounting for workflow and manual modes.
  size_t activeCalibrationSensorIndex(commissioning::Mode mode,
                                      size_t highlightedSensorIndex) const;

  // Closes any active overlay and returns to the base screen.
  void closeOverlay();

  // Opens the stage menu and chooses the first available item.
  void openStageMenu(bool hasViewData, bool hasCalibration);

  // Opens the live sensor viewer for the supplied sensor slot.
  void openSensorView(size_t sensorIndex);

  // Opens the manual calibration selection overlay.
  void openCalibrationSelection(size_t sensorIndex);

  // Opens the manual calibration editor for one sensor slot.
  void openManualCalibrationEditor(size_t sensorIndex);

  // Opens the PWM calibration overlay.
  void openPwmCalibration();

  // Returns from the calibration editor to the calibration selection overlay.
  void returnToCalibrationSelection(size_t sensorIndex);

  // Clears manual-calibration selection state after mode changes.
  void resetManualCalibrationSelection();

  // Moves the stage-menu cursor to the next available item in the chosen direction.
  void moveStageMenu(bool forward, bool hasViewData, bool hasCalibration);

  // Returns the typed stage-menu item represented by the current menu index.
  StageMenuItem currentStageMenuItem() const;

  // Advances the ready-state display page forward or backward with wraparound.
  void advanceDisplayPage(bool forward);
};

// Converts a stored menu index into a valid stage-menu item.
StageMenuItem stageMenuItemFromIndex(uint8_t menuIndex);

// Returns the short LCD label for a stage-menu item.
const char *stageMenuItemName(StageMenuItem menuItem);

}  // namespace controller_ui

#endif  // CONTROLLER_UI_H
