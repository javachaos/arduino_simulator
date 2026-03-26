mod builtins;

use rust_project::{
    BehaviorDefinition, BehaviorEngine, BehaviorValue, DefinitionReference,
    DefinitionReferenceKind, ProjectError,
};

pub use builtins::{
    built_in_behavior_names, load_built_in_behavior_definition,
    suggested_builtin_behavior_for_board_model,
};

#[derive(Debug)]
pub enum BehaviorError {
    UnknownBuiltInBehavior(String),
    MissingRequiredRole {
        engine: BehaviorEngine,
        role: String,
    },
    InvalidParameterType {
        name: String,
        expected: &'static str,
    },
    UnknownInput(String),
    UnknownOutput(String),
    InvalidInput(String),
    Project(ProjectError),
}

impl std::fmt::Display for BehaviorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBuiltInBehavior(name) => write!(f, "unknown built-in behavior {name}"),
            Self::MissingRequiredRole { engine, role } => {
                write!(
                    f,
                    "behavior {} is missing required role {role}",
                    engine.label()
                )
            }
            Self::InvalidParameterType { name, expected } => {
                write!(f, "behavior parameter {name} must be {expected}")
            }
            Self::UnknownInput(name) => write!(f, "unknown behavior input {name}"),
            Self::UnknownOutput(name) => write!(f, "unknown behavior output {name}"),
            Self::InvalidInput(message) => write!(f, "{message}"),
            Self::Project(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for BehaviorError {}

impl From<ProjectError> for BehaviorError {
    fn from(value: ProjectError) -> Self {
        Self::Project(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sht31Behavior {
    pub address: u8,
    pub measurement_delay_ms: u64,
    pub ambient_temp_c: f64,
    pub relative_humidity_percent: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mcp2515Behavior {
    pub oscillator_hz: u32,
    pub interrupt_asserted: bool,
    pub tx_pending_frames: u32,
    pub can_bus_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Max31865Behavior {
    pub nominal_rtd_ohms: f64,
    pub reference_resistor_ohms: f64,
    pub temperature_c: f64,
    pub fault_status: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PwmToVoltageBehavior {
    pub supply_voltage: f64,
    pub output_min_voltage: f64,
    pub output_max_voltage: f64,
    pub pwm_duty: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TempHumidityI2cBehavior {
    pub address: u8,
    pub measurement_delay_ms: u64,
    pub temperature_c: f64,
    pub relative_humidity_percent: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentalI2cBehavior {
    pub address: u8,
    pub measurement_delay_ms: u64,
    pub temperature_c: f64,
    pub pressure_hpa: f64,
    pub relative_humidity_percent: f64,
    pub supports_humidity: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmbientLightI2cBehavior {
    pub address: u8,
    pub illuminance_lux: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerMonitorI2cBehavior {
    pub address: u8,
    pub bus_voltage_v: f64,
    pub shunt_voltage_mv: f64,
    pub current_ma: f64,
    pub power_mw: f64,
    pub alert_asserted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Imu6DofI2cBehavior {
    pub address: u8,
    pub accel_x_g: f64,
    pub accel_y_g: f64,
    pub accel_z_g: f64,
    pub gyro_x_dps: f64,
    pub gyro_y_dps: f64,
    pub gyro_z_dps: f64,
    pub temperature_c: f64,
    pub interrupt_asserted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Adc4ChannelI2cBehavior {
    pub address: u8,
    pub gain_volts: f64,
    pub sample_rate_sps: u32,
    pub ain0_v: f64,
    pub ain1_v: f64,
    pub ain2_v: f64,
    pub ain3_v: f64,
    pub alert_asserted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TofDistanceI2cBehavior {
    pub address: u8,
    pub distance_mm: u32,
    pub signal_valid: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermocoupleSpiBehavior {
    pub temperature_c: f64,
    pub internal_temp_c: f64,
    pub fault_open: bool,
    pub fault_short_to_gnd: bool,
    pub fault_short_to_vcc: bool,
    pub has_internal_temp: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BehaviorInstance {
    Sht31(Sht31Behavior),
    TempHumidityI2c(TempHumidityI2cBehavior),
    EnvironmentalI2c(EnvironmentalI2cBehavior),
    AmbientLightI2c(AmbientLightI2cBehavior),
    PowerMonitorI2c(PowerMonitorI2cBehavior),
    Imu6DofI2c(Imu6DofI2cBehavior),
    Adc4ChannelI2c(Adc4ChannelI2cBehavior),
    TofDistanceI2c(TofDistanceI2cBehavior),
    ThermocoupleSpi(ThermocoupleSpiBehavior),
    Mcp2515(Mcp2515Behavior),
    Max31865(Max31865Behavior),
    PwmToVoltage(PwmToVoltageBehavior),
}

impl BehaviorInstance {
    pub fn engine(&self) -> BehaviorEngine {
        match self {
            Self::Sht31(_) => BehaviorEngine::Sht31I2cSensor,
            Self::TempHumidityI2c(_) => BehaviorEngine::TempHumidityI2cSensor,
            Self::EnvironmentalI2c(_) => BehaviorEngine::EnvironmentalI2cSensor,
            Self::AmbientLightI2c(_) => BehaviorEngine::AmbientLightI2cSensor,
            Self::PowerMonitorI2c(_) => BehaviorEngine::PowerMonitorI2cSensor,
            Self::Imu6DofI2c(_) => BehaviorEngine::Imu6DofI2cSensor,
            Self::Adc4ChannelI2c(_) => BehaviorEngine::Adc4ChannelI2c,
            Self::TofDistanceI2c(_) => BehaviorEngine::TofDistanceI2cSensor,
            Self::ThermocoupleSpi(_) => BehaviorEngine::ThermocoupleSpiSensor,
            Self::Mcp2515(_) => BehaviorEngine::Mcp2515CanModule,
            Self::Max31865(_) => BehaviorEngine::Max31865RtdFrontend,
            Self::PwmToVoltage(_) => BehaviorEngine::PwmToVoltage,
        }
    }

    pub fn summary_lines(&self) -> Vec<String> {
        match self {
            Self::Sht31(model) => vec![
                format!("SHT31 addr=0x{:02X}", model.address),
                format!(
                    "ambient={:.2}C rh={:.2}% delay={}ms",
                    model.ambient_temp_c,
                    model.relative_humidity_percent,
                    model.measurement_delay_ms
                ),
            ],
            Self::TempHumidityI2c(model) => vec![
                format!("I2C temp/rh addr=0x{:02X}", model.address),
                format!(
                    "temp={:.2}C rh={:.2}% delay={}ms",
                    model.temperature_c,
                    model.relative_humidity_percent,
                    model.measurement_delay_ms
                ),
            ],
            Self::EnvironmentalI2c(model) => {
                let mut lines = vec![format!("I2C env addr=0x{:02X}", model.address)];
                let humidity = if model.supports_humidity {
                    format!(" rh={:.2}%", model.relative_humidity_percent)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "temp={:.2}C pressure={:.2}hPa{} delay={}ms",
                    model.temperature_c, model.pressure_hpa, humidity, model.measurement_delay_ms
                ));
                lines
            }
            Self::AmbientLightI2c(model) => vec![
                format!("I2C light addr=0x{:02X}", model.address),
                format!("illuminance={:.2} lux", model.illuminance_lux),
            ],
            Self::PowerMonitorI2c(model) => vec![
                format!("I2C power addr=0x{:02X}", model.address),
                format!(
                    "bus={:.3}V shunt={:.3}mV current={:.2}mA power={:.2}mW alert={}",
                    model.bus_voltage_v,
                    model.shunt_voltage_mv,
                    model.current_ma,
                    model.power_mw,
                    model.alert_asserted
                ),
            ],
            Self::Imu6DofI2c(model) => vec![
                format!("I2C IMU addr=0x{:02X}", model.address),
                format!(
                    "accel=({:.3},{:.3},{:.3})g gyro=({:.2},{:.2},{:.2})dps temp={:.2}C int={}",
                    model.accel_x_g,
                    model.accel_y_g,
                    model.accel_z_g,
                    model.gyro_x_dps,
                    model.gyro_y_dps,
                    model.gyro_z_dps,
                    model.temperature_c,
                    model.interrupt_asserted
                ),
            ],
            Self::Adc4ChannelI2c(model) => vec![
                format!("I2C ADC addr=0x{:02X}", model.address),
                format!(
                    "ain=({:.3},{:.3},{:.3},{:.3})V gain={:.3}V rate={}sps alert={}",
                    model.ain0_v,
                    model.ain1_v,
                    model.ain2_v,
                    model.ain3_v,
                    model.gain_volts,
                    model.sample_rate_sps,
                    model.alert_asserted
                ),
            ],
            Self::TofDistanceI2c(model) => vec![
                format!("I2C ToF addr=0x{:02X}", model.address),
                format!(
                    "distance={}mm valid={}",
                    model.distance_mm, model.signal_valid
                ),
            ],
            Self::ThermocoupleSpi(model) => {
                let mut lines = vec!["SPI thermocouple".to_string()];
                let internal = if model.has_internal_temp {
                    format!(" internal={:.2}C", model.internal_temp_c)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "temp={:.2}C{} open={} short_gnd={} short_vcc={}",
                    model.temperature_c,
                    internal,
                    model.fault_open,
                    model.fault_short_to_gnd,
                    model.fault_short_to_vcc
                ));
                lines
            }
            Self::Mcp2515(model) => vec![
                format!("MCP2515 osc={}Hz", model.oscillator_hz),
                format!(
                    "int={} tx_pending={} bus_active={}",
                    model.interrupt_asserted, model.tx_pending_frames, model.can_bus_active
                ),
            ],
            Self::Max31865(model) => vec![
                format!(
                    "MAX31865 nominal={}ohm rref={}ohm",
                    model.nominal_rtd_ohms, model.reference_resistor_ohms
                ),
                format!(
                    "temp={:.2}C resistance={:.3}ohm fault=0x{:02X}",
                    model.temperature_c,
                    model.resistance_ohms(),
                    model.fault_status
                ),
            ],
            Self::PwmToVoltage(model) => vec![
                format!(
                    "PWM duty={:.3} supply={:.1}V",
                    model.pwm_duty, model.supply_voltage
                ),
                format!("output={:.3}V", model.output_voltage()),
            ],
        }
    }

    pub fn apply_input(&mut self, input: &str, value: BehaviorValue) -> Result<(), BehaviorError> {
        match self {
            Self::Sht31(model) => match input {
                "ambient_temp_c" => {
                    model.ambient_temp_c = expect_f64("ambient_temp_c", value)?;
                    Ok(())
                }
                "relative_humidity_percent" => {
                    model.relative_humidity_percent =
                        expect_f64("relative_humidity_percent", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::TempHumidityI2c(model) => match input {
                "temperature_c" => {
                    model.temperature_c = expect_f64("temperature_c", value)?;
                    Ok(())
                }
                "relative_humidity_percent" => {
                    model.relative_humidity_percent =
                        expect_f64("relative_humidity_percent", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::EnvironmentalI2c(model) => match input {
                "temperature_c" => {
                    model.temperature_c = expect_f64("temperature_c", value)?;
                    Ok(())
                }
                "pressure_hpa" => {
                    model.pressure_hpa = expect_f64("pressure_hpa", value)?;
                    Ok(())
                }
                "relative_humidity_percent" if model.supports_humidity => {
                    model.relative_humidity_percent =
                        expect_f64("relative_humidity_percent", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::AmbientLightI2c(model) => match input {
                "illuminance_lux" => {
                    model.illuminance_lux = expect_f64("illuminance_lux", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::PowerMonitorI2c(model) => match input {
                "bus_voltage_v" => {
                    model.bus_voltage_v = expect_f64("bus_voltage_v", value)?;
                    Ok(())
                }
                "shunt_voltage_mv" => {
                    model.shunt_voltage_mv = expect_f64("shunt_voltage_mv", value)?;
                    Ok(())
                }
                "current_ma" => {
                    model.current_ma = expect_f64("current_ma", value)?;
                    Ok(())
                }
                "power_mw" => {
                    model.power_mw = expect_f64("power_mw", value)?;
                    Ok(())
                }
                "alert_asserted" => {
                    model.alert_asserted = expect_bool("alert_asserted", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::Imu6DofI2c(model) => match input {
                "accel_x_g" => {
                    model.accel_x_g = expect_f64("accel_x_g", value)?;
                    Ok(())
                }
                "accel_y_g" => {
                    model.accel_y_g = expect_f64("accel_y_g", value)?;
                    Ok(())
                }
                "accel_z_g" => {
                    model.accel_z_g = expect_f64("accel_z_g", value)?;
                    Ok(())
                }
                "gyro_x_dps" => {
                    model.gyro_x_dps = expect_f64("gyro_x_dps", value)?;
                    Ok(())
                }
                "gyro_y_dps" => {
                    model.gyro_y_dps = expect_f64("gyro_y_dps", value)?;
                    Ok(())
                }
                "gyro_z_dps" => {
                    model.gyro_z_dps = expect_f64("gyro_z_dps", value)?;
                    Ok(())
                }
                "temperature_c" => {
                    model.temperature_c = expect_f64("temperature_c", value)?;
                    Ok(())
                }
                "interrupt_asserted" => {
                    model.interrupt_asserted = expect_bool("interrupt_asserted", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::Adc4ChannelI2c(model) => match input {
                "ain0_v" => {
                    model.ain0_v = expect_f64("ain0_v", value)?;
                    Ok(())
                }
                "ain1_v" => {
                    model.ain1_v = expect_f64("ain1_v", value)?;
                    Ok(())
                }
                "ain2_v" => {
                    model.ain2_v = expect_f64("ain2_v", value)?;
                    Ok(())
                }
                "ain3_v" => {
                    model.ain3_v = expect_f64("ain3_v", value)?;
                    Ok(())
                }
                "alert_asserted" => {
                    model.alert_asserted = expect_bool("alert_asserted", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::TofDistanceI2c(model) => match input {
                "distance_mm" => {
                    model.distance_mm = expect_u32("distance_mm", value)?;
                    Ok(())
                }
                "signal_valid" => {
                    model.signal_valid = expect_bool("signal_valid", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::ThermocoupleSpi(model) => match input {
                "temperature_c" => {
                    model.temperature_c = expect_f64("temperature_c", value)?;
                    Ok(())
                }
                "internal_temp_c" if model.has_internal_temp => {
                    model.internal_temp_c = expect_f64("internal_temp_c", value)?;
                    Ok(())
                }
                "fault_open" => {
                    model.fault_open = expect_bool("fault_open", value)?;
                    Ok(())
                }
                "fault_short_to_gnd" => {
                    model.fault_short_to_gnd = expect_bool("fault_short_to_gnd", value)?;
                    Ok(())
                }
                "fault_short_to_vcc" => {
                    model.fault_short_to_vcc = expect_bool("fault_short_to_vcc", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::Mcp2515(model) => match input {
                "interrupt_asserted" => {
                    model.interrupt_asserted = expect_bool("interrupt_asserted", value)?;
                    Ok(())
                }
                "tx_pending_frames" => {
                    model.tx_pending_frames = expect_u32("tx_pending_frames", value)?;
                    Ok(())
                }
                "can_bus_active" => {
                    model.can_bus_active = expect_bool("can_bus_active", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::Max31865(model) => match input {
                "temperature_c" => {
                    model.temperature_c = expect_f64("temperature_c", value)?;
                    Ok(())
                }
                "fault_status" => {
                    model.fault_status = expect_u8("fault_status", value)?;
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
            Self::PwmToVoltage(model) => match input {
                "pwm_duty" => {
                    let duty = expect_f64("pwm_duty", value)?;
                    model.pwm_duty = duty.clamp(0.0, 1.0);
                    Ok(())
                }
                _ => Err(BehaviorError::UnknownInput(input.to_string())),
            },
        }
    }

    pub fn output(&self, output: &str) -> Result<BehaviorValue, BehaviorError> {
        match self {
            Self::Sht31(model) => match output {
                "temperature_c" => Ok(BehaviorValue::Float(model.ambient_temp_c)),
                "relative_humidity_percent" => {
                    Ok(BehaviorValue::Float(model.relative_humidity_percent))
                }
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::TempHumidityI2c(model) => match output {
                "temperature_c" => Ok(BehaviorValue::Float(model.temperature_c)),
                "relative_humidity_percent" => {
                    Ok(BehaviorValue::Float(model.relative_humidity_percent))
                }
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::EnvironmentalI2c(model) => match output {
                "temperature_c" => Ok(BehaviorValue::Float(model.temperature_c)),
                "pressure_hpa" => Ok(BehaviorValue::Float(model.pressure_hpa)),
                "relative_humidity_percent" if model.supports_humidity => {
                    Ok(BehaviorValue::Float(model.relative_humidity_percent))
                }
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::AmbientLightI2c(model) => match output {
                "illuminance_lux" => Ok(BehaviorValue::Float(model.illuminance_lux)),
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::PowerMonitorI2c(model) => match output {
                "bus_voltage_v" => Ok(BehaviorValue::Float(model.bus_voltage_v)),
                "shunt_voltage_mv" => Ok(BehaviorValue::Float(model.shunt_voltage_mv)),
                "current_ma" => Ok(BehaviorValue::Float(model.current_ma)),
                "power_mw" => Ok(BehaviorValue::Float(model.power_mw)),
                "alert_asserted" => Ok(BehaviorValue::Bool(model.alert_asserted)),
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::Imu6DofI2c(model) => match output {
                "accel_x_g" => Ok(BehaviorValue::Float(model.accel_x_g)),
                "accel_y_g" => Ok(BehaviorValue::Float(model.accel_y_g)),
                "accel_z_g" => Ok(BehaviorValue::Float(model.accel_z_g)),
                "gyro_x_dps" => Ok(BehaviorValue::Float(model.gyro_x_dps)),
                "gyro_y_dps" => Ok(BehaviorValue::Float(model.gyro_y_dps)),
                "gyro_z_dps" => Ok(BehaviorValue::Float(model.gyro_z_dps)),
                "temperature_c" => Ok(BehaviorValue::Float(model.temperature_c)),
                "interrupt_asserted" => Ok(BehaviorValue::Bool(model.interrupt_asserted)),
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::Adc4ChannelI2c(model) => match output {
                "ain0_v" => Ok(BehaviorValue::Float(model.ain0_v)),
                "ain1_v" => Ok(BehaviorValue::Float(model.ain1_v)),
                "ain2_v" => Ok(BehaviorValue::Float(model.ain2_v)),
                "ain3_v" => Ok(BehaviorValue::Float(model.ain3_v)),
                "gain_volts" => Ok(BehaviorValue::Float(model.gain_volts)),
                "sample_rate_sps" => Ok(BehaviorValue::Integer(i64::from(model.sample_rate_sps))),
                "alert_asserted" => Ok(BehaviorValue::Bool(model.alert_asserted)),
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::TofDistanceI2c(model) => match output {
                "distance_mm" => Ok(BehaviorValue::Integer(i64::from(model.distance_mm))),
                "signal_valid" => Ok(BehaviorValue::Bool(model.signal_valid)),
                "address" => Ok(BehaviorValue::Integer(i64::from(model.address))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::ThermocoupleSpi(model) => match output {
                "temperature_c" => Ok(BehaviorValue::Float(model.temperature_c)),
                "internal_temp_c" if model.has_internal_temp => {
                    Ok(BehaviorValue::Float(model.internal_temp_c))
                }
                "fault_open" => Ok(BehaviorValue::Bool(model.fault_open)),
                "fault_short_to_gnd" => Ok(BehaviorValue::Bool(model.fault_short_to_gnd)),
                "fault_short_to_vcc" => Ok(BehaviorValue::Bool(model.fault_short_to_vcc)),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::Mcp2515(model) => match output {
                "interrupt_asserted" => Ok(BehaviorValue::Bool(model.interrupt_asserted)),
                "tx_pending_frames" => {
                    Ok(BehaviorValue::Integer(i64::from(model.tx_pending_frames)))
                }
                "can_bus_active" => Ok(BehaviorValue::Bool(model.can_bus_active)),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::Max31865(model) => match output {
                "temperature_c" => Ok(BehaviorValue::Float(model.temperature_c)),
                "resistance_ohms" => Ok(BehaviorValue::Float(model.resistance_ohms())),
                "fault_status" => Ok(BehaviorValue::Integer(i64::from(model.fault_status))),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
            Self::PwmToVoltage(model) => match output {
                "pwm_duty" => Ok(BehaviorValue::Float(model.pwm_duty)),
                "output_voltage" => Ok(BehaviorValue::Float(model.output_voltage())),
                _ => Err(BehaviorError::UnknownOutput(output.to_string())),
            },
        }
    }
}

impl Max31865Behavior {
    pub fn resistance_ohms(&self) -> f64 {
        resistance_from_pt_temperature(self.temperature_c, self.nominal_rtd_ohms)
    }
}

impl PwmToVoltageBehavior {
    pub fn output_voltage(&self) -> f64 {
        self.output_min_voltage
            + (self.output_max_voltage - self.output_min_voltage) * self.pwm_duty
    }
}

pub fn load_behavior_definition_from_reference(
    reference: &DefinitionReference,
) -> Result<BehaviorDefinition, BehaviorError> {
    reference.validate()?;
    if reference.kind != DefinitionReferenceKind::BehaviorDefinition {
        return Err(BehaviorError::Project(
            ProjectError::InvalidDefinitionReference(
                "behavior references must use kind behavior_definition".to_string(),
            ),
        ));
    }

    if let Some(name) = reference.builtin_name.as_deref() {
        return load_built_in_behavior_definition(name);
    }

    let path = reference.path.as_ref().ok_or_else(|| {
        BehaviorError::Project(ProjectError::InvalidDefinitionReference(
            "behavior references must define either a built-in behavior or file path".to_string(),
        ))
    })?;
    Ok(BehaviorDefinition::load_json(path)?)
}

pub fn instantiate_behavior(
    definition: &BehaviorDefinition,
) -> Result<BehaviorInstance, BehaviorError> {
    definition.validate()?;
    validate_required_roles(definition)?;
    match definition.engine {
        BehaviorEngine::Sht31I2cSensor => Ok(BehaviorInstance::Sht31(Sht31Behavior {
            address: expect_u8_parameter(definition, "address", 0x44)?,
            measurement_delay_ms: expect_u64_parameter(definition, "measurement_delay_ms", 15)?,
            ambient_temp_c: expect_f64_parameter(definition, "ambient_temp_c", 21.5)?,
            relative_humidity_percent: expect_f64_parameter(
                definition,
                "relative_humidity_percent",
                50.0,
            )?,
        })),
        BehaviorEngine::TempHumidityI2cSensor => {
            Ok(BehaviorInstance::TempHumidityI2c(TempHumidityI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x38)?,
                measurement_delay_ms: expect_u64_parameter(definition, "measurement_delay_ms", 80)?,
                temperature_c: expect_f64_parameter(definition, "temperature_c", 21.5)?,
                relative_humidity_percent: expect_f64_parameter(
                    definition,
                    "relative_humidity_percent",
                    50.0,
                )?,
            }))
        }
        BehaviorEngine::EnvironmentalI2cSensor => Ok(BehaviorInstance::EnvironmentalI2c(
            EnvironmentalI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x76)?,
                measurement_delay_ms: expect_u64_parameter(definition, "measurement_delay_ms", 20)?,
                temperature_c: expect_f64_parameter(definition, "temperature_c", 21.5)?,
                pressure_hpa: expect_f64_parameter(definition, "pressure_hpa", 1013.25)?,
                relative_humidity_percent: expect_f64_parameter(
                    definition,
                    "relative_humidity_percent",
                    50.0,
                )?,
                supports_humidity: expect_bool_parameter(definition, "supports_humidity", true)?,
            },
        )),
        BehaviorEngine::AmbientLightI2cSensor => {
            Ok(BehaviorInstance::AmbientLightI2c(AmbientLightI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x23)?,
                illuminance_lux: expect_f64_parameter(definition, "illuminance_lux", 250.0)?,
            }))
        }
        BehaviorEngine::PowerMonitorI2cSensor => {
            Ok(BehaviorInstance::PowerMonitorI2c(PowerMonitorI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x40)?,
                bus_voltage_v: expect_f64_parameter(definition, "bus_voltage_v", 12.0)?,
                shunt_voltage_mv: expect_f64_parameter(definition, "shunt_voltage_mv", 24.0)?,
                current_ma: expect_f64_parameter(definition, "current_ma", 120.0)?,
                power_mw: expect_f64_parameter(definition, "power_mw", 1440.0)?,
                alert_asserted: expect_bool_parameter(definition, "alert_asserted", false)?,
            }))
        }
        BehaviorEngine::Imu6DofI2cSensor => Ok(BehaviorInstance::Imu6DofI2c(Imu6DofI2cBehavior {
            address: expect_u8_parameter(definition, "address", 0x68)?,
            accel_x_g: expect_f64_parameter(definition, "accel_x_g", 0.0)?,
            accel_y_g: expect_f64_parameter(definition, "accel_y_g", 0.0)?,
            accel_z_g: expect_f64_parameter(definition, "accel_z_g", 1.0)?,
            gyro_x_dps: expect_f64_parameter(definition, "gyro_x_dps", 0.0)?,
            gyro_y_dps: expect_f64_parameter(definition, "gyro_y_dps", 0.0)?,
            gyro_z_dps: expect_f64_parameter(definition, "gyro_z_dps", 0.0)?,
            temperature_c: expect_f64_parameter(definition, "temperature_c", 24.0)?,
            interrupt_asserted: expect_bool_parameter(definition, "interrupt_asserted", false)?,
        })),
        BehaviorEngine::Adc4ChannelI2c => {
            Ok(BehaviorInstance::Adc4ChannelI2c(Adc4ChannelI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x48)?,
                gain_volts: expect_f64_parameter(definition, "gain_volts", 4.096)?,
                sample_rate_sps: expect_u32_parameter(definition, "sample_rate_sps", 128)?,
                ain0_v: expect_f64_parameter(definition, "ain0_v", 0.0)?,
                ain1_v: expect_f64_parameter(definition, "ain1_v", 0.0)?,
                ain2_v: expect_f64_parameter(definition, "ain2_v", 0.0)?,
                ain3_v: expect_f64_parameter(definition, "ain3_v", 0.0)?,
                alert_asserted: expect_bool_parameter(definition, "alert_asserted", false)?,
            }))
        }
        BehaviorEngine::TofDistanceI2cSensor => {
            Ok(BehaviorInstance::TofDistanceI2c(TofDistanceI2cBehavior {
                address: expect_u8_parameter(definition, "address", 0x29)?,
                distance_mm: expect_u32_parameter(definition, "distance_mm", 250)?,
                signal_valid: expect_bool_parameter(definition, "signal_valid", true)?,
            }))
        }
        BehaviorEngine::ThermocoupleSpiSensor => {
            Ok(BehaviorInstance::ThermocoupleSpi(ThermocoupleSpiBehavior {
                temperature_c: expect_f64_parameter(definition, "temperature_c", 25.0)?,
                internal_temp_c: expect_f64_parameter(definition, "internal_temp_c", 23.0)?,
                fault_open: expect_bool_parameter(definition, "fault_open", false)?,
                fault_short_to_gnd: expect_bool_parameter(definition, "fault_short_to_gnd", false)?,
                fault_short_to_vcc: expect_bool_parameter(definition, "fault_short_to_vcc", false)?,
                has_internal_temp: expect_bool_parameter(definition, "has_internal_temp", true)?,
            }))
        }
        BehaviorEngine::Mcp2515CanModule => Ok(BehaviorInstance::Mcp2515(Mcp2515Behavior {
            oscillator_hz: expect_u32_parameter(definition, "oscillator_hz", 16_000_000)?,
            interrupt_asserted: expect_bool_parameter(definition, "interrupt_asserted", false)?,
            tx_pending_frames: expect_u32_parameter(definition, "tx_pending_frames", 0)?,
            can_bus_active: expect_bool_parameter(definition, "can_bus_active", false)?,
        })),
        BehaviorEngine::Max31865RtdFrontend => Ok(BehaviorInstance::Max31865(Max31865Behavior {
            nominal_rtd_ohms: expect_f64_parameter(definition, "nominal_rtd_ohms", 1000.0)?,
            reference_resistor_ohms: expect_f64_parameter(
                definition,
                "reference_resistor_ohms",
                4300.0,
            )?,
            temperature_c: expect_f64_parameter(definition, "temperature_c", 20.0)?,
            fault_status: expect_u8_parameter(definition, "fault_status", 0)?,
        })),
        BehaviorEngine::PwmToVoltage => Ok(BehaviorInstance::PwmToVoltage(PwmToVoltageBehavior {
            supply_voltage: expect_f64_parameter(definition, "supply_voltage", 12.0)?,
            output_min_voltage: expect_f64_parameter(definition, "output_min_voltage", 0.0)?,
            output_max_voltage: expect_f64_parameter(definition, "output_max_voltage", 10.0)?,
            pwm_duty: expect_f64_parameter(definition, "pwm_duty", 0.0)?.clamp(0.0, 1.0),
        })),
    }
}

fn validate_required_roles(definition: &BehaviorDefinition) -> Result<(), BehaviorError> {
    let roles = definition
        .ports
        .iter()
        .map(|port| port.role.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for role in required_roles(definition.engine) {
        if !roles.contains(role) {
            return Err(BehaviorError::MissingRequiredRole {
                engine: definition.engine,
                role: (*role).to_string(),
            });
        }
    }
    Ok(())
}

fn required_roles(engine: BehaviorEngine) -> &'static [&'static str] {
    match engine {
        BehaviorEngine::Sht31I2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::TempHumidityI2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::EnvironmentalI2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::AmbientLightI2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::PowerMonitorI2cSensor => &[
            "sda",
            "scl",
            "vcc",
            "gnd",
            "sense_positive",
            "sense_negative",
        ],
        BehaviorEngine::Imu6DofI2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::Adc4ChannelI2c => {
            &["sda", "scl", "vcc", "gnd", "ain0", "ain1", "ain2", "ain3"]
        }
        BehaviorEngine::TofDistanceI2cSensor => &["sda", "scl", "vcc", "gnd"],
        BehaviorEngine::ThermocoupleSpiSensor => &[
            "spi_sck",
            "spi_miso",
            "spi_cs",
            "vcc",
            "gnd",
            "thermocouple_positive",
            "thermocouple_negative",
        ],
        BehaviorEngine::Mcp2515CanModule => &[
            "interrupt",
            "spi_sck",
            "spi_mosi",
            "spi_miso",
            "spi_cs",
            "vcc",
            "gnd",
            "can_h",
            "can_l",
        ],
        BehaviorEngine::Max31865RtdFrontend => &[
            "spi_sck",
            "spi_miso",
            "spi_mosi",
            "spi_cs",
            "vcc",
            "gnd",
            "force_plus",
            "force_minus",
            "sense_plus",
            "sense_minus",
        ],
        BehaviorEngine::PwmToVoltage => &["gnd", "pwm_input", "analog_output", "vcc"],
    }
}

fn expect_bool_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: bool,
) -> Result<bool, BehaviorError> {
    match definition.parameters.get(name) {
        None => Ok(default),
        Some(BehaviorValue::Bool(value)) => Ok(*value),
        Some(_) => Err(BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "a boolean",
        }),
    }
}

fn expect_u8_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: u8,
) -> Result<u8, BehaviorError> {
    expect_integer_parameter(definition, name, i64::from(default)).and_then(|value| {
        u8::try_from(value).map_err(|_| BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "an 8-bit integer",
        })
    })
}

fn expect_u32_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: u32,
) -> Result<u32, BehaviorError> {
    expect_integer_parameter(definition, name, i64::from(default)).and_then(|value| {
        u32::try_from(value).map_err(|_| BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "a 32-bit integer",
        })
    })
}

fn expect_u64_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: u64,
) -> Result<u64, BehaviorError> {
    expect_integer_parameter(definition, name, default as i64).and_then(|value| {
        u64::try_from(value).map_err(|_| BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "a non-negative integer",
        })
    })
}

fn expect_f64_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: f64,
) -> Result<f64, BehaviorError> {
    match definition.parameters.get(name) {
        None => Ok(default),
        Some(BehaviorValue::Float(value)) => Ok(*value),
        Some(BehaviorValue::Integer(value)) => Ok(*value as f64),
        Some(_) => Err(BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "a number",
        }),
    }
}

fn expect_integer_parameter(
    definition: &BehaviorDefinition,
    name: &str,
    default: i64,
) -> Result<i64, BehaviorError> {
    match definition.parameters.get(name) {
        None => Ok(default),
        Some(BehaviorValue::Integer(value)) => Ok(*value),
        Some(_) => Err(BehaviorError::InvalidParameterType {
            name: name.to_string(),
            expected: "an integer",
        }),
    }
}

fn expect_bool(name: &str, value: BehaviorValue) -> Result<bool, BehaviorError> {
    match value {
        BehaviorValue::Bool(result) => Ok(result),
        _ => Err(BehaviorError::InvalidInput(format!(
            "{name} expects a boolean"
        ))),
    }
}

fn expect_u32(name: &str, value: BehaviorValue) -> Result<u32, BehaviorError> {
    match value {
        BehaviorValue::Integer(result) => u32::try_from(result).map_err(|_| {
            BehaviorError::InvalidInput(format!("{name} expects a non-negative integer"))
        }),
        _ => Err(BehaviorError::InvalidInput(format!(
            "{name} expects an integer"
        ))),
    }
}

fn expect_u8(name: &str, value: BehaviorValue) -> Result<u8, BehaviorError> {
    match value {
        BehaviorValue::Integer(result) => u8::try_from(result)
            .map_err(|_| BehaviorError::InvalidInput(format!("{name} expects an 8-bit integer"))),
        _ => Err(BehaviorError::InvalidInput(format!(
            "{name} expects an integer"
        ))),
    }
}

fn expect_f64(name: &str, value: BehaviorValue) -> Result<f64, BehaviorError> {
    match value {
        BehaviorValue::Float(result) => Ok(result),
        BehaviorValue::Integer(result) => Ok(result as f64),
        _ => Err(BehaviorError::InvalidInput(format!(
            "{name} expects a number"
        ))),
    }
}

fn resistance_from_pt_temperature(temperature_c: f64, nominal_rtd_ohms: f64) -> f64 {
    const A: f64 = 3.9083e-3;
    const B: f64 = -5.775e-7;
    const C: f64 = -4.183e-12;

    if temperature_c >= 0.0 {
        nominal_rtd_ohms * (1.0 + (A * temperature_c) + (B * temperature_c * temperature_c))
    } else {
        let cubic = (temperature_c - 100.0) * temperature_c * temperature_c * temperature_c;
        nominal_rtd_ohms
            * (1.0 + (A * temperature_c) + (B * temperature_c * temperature_c) + (C * cubic))
    }
}

#[cfg(test)]
mod tests {
    use rust_project::{BehaviorEngine, BehaviorValue};

    use super::{
        built_in_behavior_names, instantiate_behavior, load_behavior_definition_from_reference,
        load_built_in_behavior_definition, suggested_builtin_behavior_for_board_model,
        BehaviorError, BehaviorInstance,
    };

    #[test]
    fn built_in_behavior_names_are_stable() {
        assert_eq!(
            built_in_behavior_names(),
            vec![
                "gy_sht31_d_behavior",
                "lc_lm358_pwm_to_0_10v_behavior",
                "max31865_breakout_behavior",
                "mcp2515_tja1050_can_module_behavior",
                "aht20_breakout_behavior",
                "ads1115_breakout_behavior",
                "bh1750_breakout_behavior",
                "bme280_breakout_behavior",
                "bmp280_breakout_behavior",
                "ina219_breakout_behavior",
                "max31855_breakout_behavior",
                "max6675_breakout_behavior",
                "mpu6050_breakout_behavior",
                "vl53l0x_breakout_behavior",
            ]
        );
    }

    #[test]
    fn built_in_suggestions_match_builtin_board_models() {
        assert_eq!(
            suggested_builtin_behavior_for_board_model("gy_sht31_d"),
            Some("gy_sht31_d_behavior")
        );
        assert_eq!(
            suggested_builtin_behavior_for_board_model("mcp2515_tja1050_can_module"),
            Some("mcp2515_tja1050_can_module_behavior")
        );
        assert_eq!(
            suggested_builtin_behavior_for_board_model("bme280_breakout"),
            Some("bme280_breakout_behavior")
        );
        assert_eq!(
            suggested_builtin_behavior_for_board_model("ads1115_breakout"),
            Some("ads1115_breakout_behavior")
        );
        assert_eq!(
            suggested_builtin_behavior_for_board_model("arduino_nano_v3"),
            None
        );
    }

    #[test]
    fn built_in_behavior_definitions_validate_and_instantiate() {
        for name in built_in_behavior_names() {
            let definition = load_built_in_behavior_definition(name).expect("definition");
            definition.validate().expect("valid");
            let instance = instantiate_behavior(&definition).expect("instance");
            assert_eq!(instance.engine(), definition.engine);
            assert!(!instance.summary_lines().is_empty());
        }
    }

    #[test]
    fn pwm_to_voltage_behavior_tracks_duty_cycle() {
        let definition =
            load_built_in_behavior_definition("lc_lm358_pwm_to_0_10v_behavior").expect("def");
        let mut instance = instantiate_behavior(&definition).expect("instance");
        instance
            .apply_input("pwm_duty", BehaviorValue::Float(0.5))
            .expect("apply");
        assert_eq!(
            instance.output("output_voltage").expect("voltage"),
            BehaviorValue::Float(5.0)
        );
    }

    #[test]
    fn max31865_behavior_reports_resistance_from_temperature() {
        let definition =
            load_built_in_behavior_definition("max31865_breakout_behavior").expect("def");
        let instance = instantiate_behavior(&definition).expect("instance");
        let BehaviorInstance::Max31865(model) = instance else {
            panic!("expected max31865 behavior");
        };
        assert_eq!(model.reference_resistor_ohms, 4300.0);
        assert!(model.resistance_ohms() > 1000.0);
    }

    #[test]
    fn built_in_behavior_engines_are_expected() {
        let definition = load_built_in_behavior_definition("gy_sht31_d_behavior").expect("def");
        assert_eq!(definition.engine, BehaviorEngine::Sht31I2cSensor);
    }

    #[test]
    fn bmp280_behavior_omits_humidity_output() {
        let definition =
            load_built_in_behavior_definition("bmp280_breakout_behavior").expect("def");
        let instance = instantiate_behavior(&definition).expect("instance");
        assert!(matches!(
            instance.output("relative_humidity_percent"),
            Err(BehaviorError::UnknownOutput(_))
        ));
    }

    #[test]
    fn max6675_behavior_omits_internal_temperature_output() {
        let definition =
            load_built_in_behavior_definition("max6675_breakout_behavior").expect("def");
        let instance = instantiate_behavior(&definition).expect("instance");
        assert!(matches!(
            instance.output("internal_temp_c"),
            Err(BehaviorError::UnknownOutput(_))
        ));
    }

    #[test]
    fn behavior_references_can_load_builtin_definitions() {
        let reference = rust_project::DefinitionReference::builtin(
            rust_project::DefinitionReferenceKind::BehaviorDefinition,
            "gy_sht31_d_behavior",
        );
        let definition = load_behavior_definition_from_reference(&reference).expect("definition");
        assert_eq!(definition.name, "gy_sht31_d_behavior");
    }
}
