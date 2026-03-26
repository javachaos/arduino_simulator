#ifndef MCP2515_CAN_H
#define MCP2515_CAN_H

#include <Arduino.h>
#include <SPI.h>

#include <stdint.h>

namespace mcp2515_can {

constexpr uint32_t kDefaultSpiClockHz = 1000000UL;
constexpr uint32_t kDefaultOscillatorHz = 8000000UL;
constexpr uint32_t kDefaultBitrate = 125000UL;

struct Frame {
  uint16_t id;
  uint8_t length;
  uint8_t data[8];
};

struct Config {
  uint8_t chipSelectPin;
  uint8_t interruptPin;
  uint32_t spiClockHz;
  uint32_t oscillatorHz;
  uint32_t bitrate;
};

enum InitStatus {
  INIT_OK = 0,
  INIT_INVALID_TIMING,
  INIT_MODE_TIMEOUT,
};

// Returns a stable name for MCP2515 initialization status codes.
const char *initStatusName(InitStatus status);

// Configures the MCP2515 for standard 11-bit CAN frames and normal mode.
InitStatus initialize(SPIClass &spi, const Config &config);

// Returns true when the controller reports a pending received CAN frame.
bool messagePending(SPIClass &spi, const Config &config);

// Receives one pending standard CAN frame if available.
bool receiveFrame(SPIClass &spi, const Config &config, Frame *frame);

// Sends one standard CAN frame using TX buffer 0.
bool sendFrame(SPIClass &spi, const Config &config, const Frame &frame);

}  // namespace mcp2515_can

#endif
