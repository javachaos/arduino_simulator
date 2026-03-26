#ifndef LCD_UI_FRAMEWORK_H
#define LCD_UI_FRAMEWORK_H

#include <Arduino.h>
#include <LiquidCrystal.h>

namespace lcd_ui {

class View {
 public:
  static const size_t kMaxColumns = 20U;

  // Creates a cached two-line LCD view helper with a fixed column width and refresh cadence.
  View(size_t columns, unsigned long refreshMs);

  // Forces the next render pass to rewrite the current screen contents.
  void invalidate();

  // Displays a temporary two-line message until the requested duration expires.
  void showTimedMessage(const char *line0,
                        const char *line1,
                        unsigned long nowMs,
                        unsigned long durationMs);

  // Returns true while a timed message should still be displayed.
  bool timedMessageActive(unsigned long nowMs) const;

  // Returns true after a timed message duration has elapsed.
  bool timedMessageExpired(unsigned long nowMs) const;

  // Clears any active timed message.
  void clearTimedMessage();

  // Copies the active timed message into caller-provided line buffers.
  void copyTimedMessage(char *line0,
                        size_t line0Size,
                        char *line1,
                        size_t line1Size) const;

  // Returns true when enough time has passed to perform another LCD render.
  bool shouldRender(unsigned long nowMs) const;

  // Records the timestamp of the most recent render pass.
  void markRendered(unsigned long nowMs);

  // Writes both LCD rows, suppressing unchanged line updates.
  void writeLines(LiquidCrystal &lcd, const char *line0, const char *line1);

 private:
  void normalizeLine(char *destination, const char *source, size_t capacity) const;
  void writeLine(LiquidCrystal &lcd, uint8_t row, char *cached, const char *source);

  size_t columns_;
  unsigned long refreshMs_;
  unsigned long lastRenderMs_;
  unsigned long messageExpiresMs_;
  char cachedLine0_[kMaxColumns + 1U];
  char cachedLine1_[kMaxColumns + 1U];
  char messageLine0_[kMaxColumns + 1U];
  char messageLine1_[kMaxColumns + 1U];
};

}  // namespace lcd_ui

#endif  // LCD_UI_FRAMEWORK_H
