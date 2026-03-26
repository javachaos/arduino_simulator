#include "mcp2515_can.h"

namespace mcp2515_can {

namespace {

constexpr uint8_t kCommandReset = 0xC0U;
constexpr uint8_t kCommandRead = 0x03U;
constexpr uint8_t kCommandWrite = 0x02U;
constexpr uint8_t kCommandBitModify = 0x05U;
constexpr uint8_t kCommandReadStatus = 0xA0U;
constexpr uint8_t kCommandRtsTx0 = 0x81U;

constexpr uint8_t kRegisterCanStat = 0x0EU;
constexpr uint8_t kRegisterCanCtrl = 0x0FU;
constexpr uint8_t kRegisterCnf3 = 0x28U;
constexpr uint8_t kRegisterCnf2 = 0x29U;
constexpr uint8_t kRegisterCnf1 = 0x2AU;
constexpr uint8_t kRegisterCanIntf = 0x2CU;
constexpr uint8_t kRegisterRxB0Ctrl = 0x60U;
constexpr uint8_t kRegisterRxB0SidH = 0x61U;
constexpr uint8_t kRegisterRxB1Ctrl = 0x70U;
constexpr uint8_t kRegisterRxB1SidH = 0x71U;
constexpr uint8_t kRegisterTxB0Ctrl = 0x30U;
constexpr uint8_t kRegisterTxB0SidH = 0x31U;

constexpr uint8_t kModeMask = 0xE0U;
constexpr uint8_t kModeNormal = 0x00U;
constexpr uint8_t kModeConfiguration = 0x80U;

constexpr uint8_t kRx0IfMask = 0x01U;
constexpr uint8_t kRx1IfMask = 0x02U;
constexpr uint8_t kTxReqMask = 0x08U;

struct BitTiming {
  uint8_t cnf1;
  uint8_t cnf2;
  uint8_t cnf3;
};

void beginTransaction(SPIClass &spi, const Config &config) {
  spi.beginTransaction(SPISettings(config.spiClockHz, MSBFIRST, SPI_MODE0));
  digitalWrite(config.chipSelectPin, LOW);
}

void endTransaction() {
  digitalWrite(SS, HIGH);
}

void endTransaction(SPIClass &spi, const Config &config) {
  digitalWrite(config.chipSelectPin, HIGH);
  spi.endTransaction();
}

void resetController(SPIClass &spi, const Config &config) {
  beginTransaction(spi, config);
  spi.transfer(kCommandReset);
  endTransaction(spi, config);
  delay(10);
}

uint8_t readRegister(SPIClass &spi, const Config &config, uint8_t address) {
  uint8_t value = 0U;

  beginTransaction(spi, config);
  spi.transfer(kCommandRead);
  spi.transfer(address);
  value = spi.transfer(0x00U);
  endTransaction(spi, config);
  return value;
}

void writeRegister(SPIClass &spi, const Config &config, uint8_t address, uint8_t value) {
  beginTransaction(spi, config);
  spi.transfer(kCommandWrite);
  spi.transfer(address);
  spi.transfer(value);
  endTransaction(spi, config);
}

void writeRegisters(SPIClass &spi,
                    const Config &config,
                    uint8_t startAddress,
                    const uint8_t *values,
                    size_t length) {
  size_t index = 0U;

  beginTransaction(spi, config);
  spi.transfer(kCommandWrite);
  spi.transfer(startAddress);
  for (index = 0U; index < length; ++index) {
    spi.transfer(values[index]);
  }
  endTransaction(spi, config);
}

void bitModify(SPIClass &spi, const Config &config, uint8_t address, uint8_t mask, uint8_t data) {
  beginTransaction(spi, config);
  spi.transfer(kCommandBitModify);
  spi.transfer(address);
  spi.transfer(mask);
  spi.transfer(data);
  endTransaction(spi, config);
}

uint8_t readStatus(SPIClass &spi, const Config &config) {
  uint8_t value = 0U;

  beginTransaction(spi, config);
  spi.transfer(kCommandReadStatus);
  value = spi.transfer(0x00U);
  endTransaction(spi, config);
  return value;
}

bool waitForMode(SPIClass &spi,
                 const Config &config,
                 uint8_t expectedMode,
                 unsigned long timeoutMs) {
  const unsigned long startMs = millis();

  while ((millis() - startMs) < timeoutMs) {
    if ((readRegister(spi, config, kRegisterCanStat) & kModeMask) == expectedMode) {
      return true;
    }
    delay(1);
  }

  return false;
}

bool computeBitTiming(const Config &config, BitTiming *timing) {
  uint32_t brp = 0UL;
  const uint32_t targetTqCount = 16UL;
  const uint32_t denominator = config.bitrate * targetTqCount;
  const uint32_t numerator = config.oscillatorHz;

  if (timing == nullptr || denominator == 0UL || numerator < denominator ||
      (numerator % denominator) != 0UL) {
    return false;
  }

  brp = (numerator / denominator);
  if (brp == 0UL || brp > 64UL) {
    return false;
  }

  brp -= 1UL;
  timing->cnf1 = static_cast<uint8_t>(brp & 0x3FU);
  timing->cnf2 = 0x9EU;  // BTLMODE=1, PHSEG1=4 TQ, PRSEG=7 TQ.
  timing->cnf3 = 0x03U;  // PHSEG2=4 TQ.
  return true;
}

void encodeStandardId(uint16_t id, uint8_t *sidH, uint8_t *sidL) {
  *sidH = static_cast<uint8_t>((id >> 3U) & 0xFFU);
  *sidL = static_cast<uint8_t>((id & 0x0007U) << 5U);
}

uint16_t decodeStandardId(uint8_t sidH, uint8_t sidL) {
  return static_cast<uint16_t>((static_cast<uint16_t>(sidH) << 3U) | (sidL >> 5U));
}

bool txBufferReady(SPIClass &spi, const Config &config) {
  return (readRegister(spi, config, kRegisterTxB0Ctrl) & kTxReqMask) == 0U;
}

}  // namespace

const char *initStatusName(InitStatus status) {
  switch (status) {
    case INIT_OK:
      return "OK";
    case INIT_INVALID_TIMING:
      return "INVALID_TIMING";
    case INIT_MODE_TIMEOUT:
    default:
      return "MODE_TIMEOUT";
  }
}

InitStatus initialize(SPIClass &spi, const Config &config) {
  BitTiming timing = {};

  pinMode(config.chipSelectPin, OUTPUT);
  digitalWrite(config.chipSelectPin, HIGH);
  if (config.interruptPin != 0xFFU) {
    pinMode(config.interruptPin, INPUT_PULLUP);
  }

  resetController(spi, config);
  bitModify(spi, config, kRegisterCanCtrl, kModeMask, kModeConfiguration);
  if (!waitForMode(spi, config, kModeConfiguration, 50UL)) {
    return INIT_MODE_TIMEOUT;
  }

  if (!computeBitTiming(config, &timing)) {
    return INIT_INVALID_TIMING;
  }

  writeRegister(spi, config, kRegisterCnf1, timing.cnf1);
  writeRegister(spi, config, kRegisterCnf2, timing.cnf2);
  writeRegister(spi, config, kRegisterCnf3, timing.cnf3);

  writeRegister(spi, config, kRegisterRxB0Ctrl, 0x64U);
  writeRegister(spi, config, kRegisterRxB1Ctrl, 0x60U);
  writeRegister(spi, config, kRegisterCanIntf, 0x00U);

  bitModify(spi, config, kRegisterCanCtrl, kModeMask, kModeNormal);
  if (!waitForMode(spi, config, kModeNormal, 50UL)) {
    return INIT_MODE_TIMEOUT;
  }

  return INIT_OK;
}

bool messagePending(SPIClass &spi, const Config &config) {
  const uint8_t interruptFlags = readRegister(spi, config, kRegisterCanIntf);
  return (interruptFlags & (kRx0IfMask | kRx1IfMask)) != 0U;
}

bool receiveFrame(SPIClass &spi, const Config &config, Frame *frame) {
  const uint8_t interruptFlags = readRegister(spi, config, kRegisterCanIntf);
  uint8_t startAddress = 0U;
  uint8_t clearMask = 0U;
  uint8_t raw[13] = {};
  size_t index = 0U;

  if (frame == nullptr) {
    return false;
  }

  if ((interruptFlags & kRx0IfMask) != 0U) {
    startAddress = kRegisterRxB0SidH;
    clearMask = kRx0IfMask;
  } else if ((interruptFlags & kRx1IfMask) != 0U) {
    startAddress = kRegisterRxB1SidH;
    clearMask = kRx1IfMask;
  } else {
    return false;
  }

  beginTransaction(spi, config);
  spi.transfer(kCommandRead);
  spi.transfer(startAddress);
  for (index = 0U; index < sizeof(raw); ++index) {
    raw[index] = spi.transfer(0x00U);
  }
  endTransaction(spi, config);

  frame->id = decodeStandardId(raw[0], raw[1]);
  frame->length = static_cast<uint8_t>(raw[4] & 0x0FU);
  if (frame->length > 8U) {
    frame->length = 8U;
  }

  for (index = 0U; index < 8U; ++index) {
    frame->data[index] = raw[5U + index];
  }

  bitModify(spi, config, kRegisterCanIntf, clearMask, 0x00U);
  return true;
}

bool sendFrame(SPIClass &spi, const Config &config, const Frame &frame) {
  uint8_t raw[13] = {};
  size_t index = 0U;
  unsigned long startMs = 0UL;

  if (frame.length > 8U || frame.id > 0x7FFU) {
    return false;
  }

  if (!txBufferReady(spi, config)) {
    return false;
  }

  encodeStandardId(frame.id, &raw[0], &raw[1]);
  raw[2] = 0U;
  raw[3] = 0U;
  raw[4] = static_cast<uint8_t>(frame.length & 0x0FU);
  for (index = 0U; index < frame.length; ++index) {
    raw[5U + index] = frame.data[index];
  }
  for (; index < 8U; ++index) {
    raw[5U + index] = 0U;
  }

  writeRegisters(spi, config, kRegisterTxB0SidH, raw, sizeof(raw));

  beginTransaction(spi, config);
  spi.transfer(kCommandRtsTx0);
  endTransaction(spi, config);

  startMs = millis();
  while ((millis() - startMs) < 20UL) {
    if (txBufferReady(spi, config)) {
      return true;
    }
    delay(1);
  }

  return false;
}

}  // namespace mcp2515_can
