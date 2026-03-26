# rust_behavior

`rust_behavior` provides the declarative runtime-behavior layer for `arduino_simulator`.

Current scope:

- built-in behavior definitions for the reusable module boards in this project
  stored as embedded `behavior_definition` JSON assets under `builtins/`
- compiled Rust behavior engines for:
  - `SHT31` I2C sensor modules
  - generic `temperature/humidity` I2C sensor modules such as `AHT20`
  - generic `environmental` I2C sensor modules such as `BME280` and `BMP280`
  - generic `ambient light` I2C sensor modules such as `BH1750`
  - generic `power monitor` I2C sensor modules such as `INA219`
  - generic `6-DoF IMU` I2C sensor modules such as `MPU6050`
  - generic `4-channel ADC` I2C sensor modules such as `ADS1115`
  - generic `time-of-flight distance` I2C sensor modules such as `VL53L0X`
  - generic `thermocouple SPI` sensor modules such as `MAX6675` and `MAX31855`
  - `MCP2515 + TJA1050` CAN modules
  - `MAX31865` RTD frontends
  - `PWM to 0-10V` interface boards
- helpers for loading a behavior definition from either a built-in name or a
  saved `behavior_definition` document reference

The board editor uses this crate to suggest behaviors for known built-in
modules and to preview the configured runtime behavior for each member.

Run its tests from the workspace root:

```sh
cargo test -p rust_behavior
```
