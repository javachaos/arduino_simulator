#include "lcd_ui_framework.h"

#include <string.h>

namespace lcd_ui {

View::View(size_t columns, unsigned long refreshMs)
    : columns_(columns <= kMaxColumns ? columns : kMaxColumns),
      refreshMs_(refreshMs),
      lastRenderMs_(0UL),
      messageExpiresMs_(0UL),
      cachedLine0_(),
      cachedLine1_(),
      messageLine0_(),
      messageLine1_() {
  normalizeLine(cachedLine0_, "", sizeof(cachedLine0_));
  normalizeLine(cachedLine1_, "", sizeof(cachedLine1_));
  normalizeLine(messageLine0_, "", sizeof(messageLine0_));
  normalizeLine(messageLine1_, "", sizeof(messageLine1_));
}

void View::invalidate() {
  lastRenderMs_ = 0UL;
}

void View::showTimedMessage(const char *line0,
                            const char *line1,
                            unsigned long nowMs,
                            unsigned long durationMs) {
  normalizeLine(messageLine0_, line0, sizeof(messageLine0_));
  normalizeLine(messageLine1_, line1, sizeof(messageLine1_));
  messageExpiresMs_ = nowMs + durationMs;
  invalidate();
}

bool View::timedMessageActive(unsigned long nowMs) const {
  return messageExpiresMs_ != 0UL && nowMs < messageExpiresMs_;
}

bool View::timedMessageExpired(unsigned long nowMs) const {
  return messageExpiresMs_ != 0UL && nowMs >= messageExpiresMs_;
}

void View::clearTimedMessage() {
  messageExpiresMs_ = 0UL;
  normalizeLine(messageLine0_, "", sizeof(messageLine0_));
  normalizeLine(messageLine1_, "", sizeof(messageLine1_));
}

void View::copyTimedMessage(char *line0,
                            size_t line0Size,
                            char *line1,
                            size_t line1Size) const {
  normalizeLine(line0, messageLine0_, line0Size);
  normalizeLine(line1, messageLine1_, line1Size);
}

bool View::shouldRender(unsigned long nowMs) const {
  return (nowMs - lastRenderMs_) >= refreshMs_;
}

void View::markRendered(unsigned long nowMs) {
  lastRenderMs_ = nowMs;
}

void View::writeLines(LiquidCrystal &lcd, const char *line0, const char *line1) {
  writeLine(lcd, 0U, cachedLine0_, line0);
  writeLine(lcd, 1U, cachedLine1_, line1);
}

void View::normalizeLine(char *destination, const char *source, size_t capacity) const {
  size_t index = 0U;
  const size_t usableColumns = columns_ < (capacity - 1U) ? columns_ : (capacity - 1U);

  for (index = 0U; index < usableColumns; ++index) {
    destination[index] = ' ';
  }

  destination[usableColumns] = '\0';
  if (source == nullptr) {
    return;
  }

  for (index = 0U; index < usableColumns && source[index] != '\0'; ++index) {
    destination[index] = source[index];
  }
}

void View::writeLine(LiquidCrystal &lcd, uint8_t row, char *cached, const char *source) {
  char normalized[kMaxColumns + 1U] = {};

  normalizeLine(normalized, source, sizeof(normalized));
  if (memcmp(cached, normalized, columns_ + 1U) == 0) {
    return;
  }

  memcpy(cached, normalized, columns_ + 1U);
  lcd.setCursor(0, row);
  lcd.print(normalized);
}

}  // namespace lcd_ui
