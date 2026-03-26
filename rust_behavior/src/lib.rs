use rust_project::{
    BehaviorDefinition, BehaviorEngine, BehaviorPortBinding, BehaviorValue, DefinitionReference,
    DefinitionReferenceKind, ProjectError,
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
pub enum BehaviorInstance {
    Sht31(Sht31Behavior),
    Mcp2515(Mcp2515Behavior),
    Max31865(Max31865Behavior),
    PwmToVoltage(PwmToVoltageBehavior),
}

impl BehaviorInstance {
    pub fn engine(&self) -> BehaviorEngine {
        match self {
            Self::Sht31(_) => BehaviorEngine::Sht31I2cSensor,
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

pub fn built_in_behavior_names() -> Vec<&'static str> {
    vec![
        "gy_sht31_d_behavior",
        "lc_lm358_pwm_to_0_10v_behavior",
        "max31865_breakout_behavior",
        "mcp2515_tja1050_can_module_behavior",
    ]
}

pub fn suggested_builtin_behavior_for_board_model(board_name: &str) -> Option<&'static str> {
    match board_name {
        "gy_sht31_d" => Some("gy_sht31_d_behavior"),
        "lc_lm358_pwm_to_0_10v" => Some("lc_lm358_pwm_to_0_10v_behavior"),
        "max31865_breakout" => Some("max31865_breakout_behavior"),
        "mcp2515_tja1050_can_module" => Some("mcp2515_tja1050_can_module_behavior"),
        _ => None,
    }
}

pub fn load_built_in_behavior_definition(name: &str) -> Result<BehaviorDefinition, BehaviorError> {
    let definition = match name {
        "gy_sht31_d_behavior" => gy_sht31_d_behavior(),
        "lc_lm358_pwm_to_0_10v_behavior" => lc_lm358_pwm_to_0_10v_behavior(),
        "max31865_breakout_behavior" => max31865_breakout_behavior(),
        "mcp2515_tja1050_can_module_behavior" => mcp2515_tja1050_can_module_behavior(),
        _ => return Err(BehaviorError::UnknownBuiltInBehavior(name.to_string())),
    };
    definition.validate()?;
    Ok(definition)
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

fn gy_sht31_d_behavior() -> BehaviorDefinition {
    let mut definition =
        BehaviorDefinition::new("gy_sht31_d_behavior", BehaviorEngine::Sht31I2cSensor);
    definition.description =
        Some("Behavior definition for the GY-SHT31-D breakout used on the air node.".to_string());
    definition.ports = vec![
        BehaviorPortBinding::new("SDA", "sda"),
        BehaviorPortBinding::new("SCL", "scl"),
        BehaviorPortBinding::new("VCC", "vcc"),
        BehaviorPortBinding::new("GND", "gnd"),
    ];
    definition
        .parameters
        .insert("address".to_string(), BehaviorValue::Integer(0x44));
    definition.parameters.insert(
        "measurement_delay_ms".to_string(),
        BehaviorValue::Integer(15),
    );
    definition
        .parameters
        .insert("ambient_temp_c".to_string(), BehaviorValue::Float(21.5));
    definition.parameters.insert(
        "relative_humidity_percent".to_string(),
        BehaviorValue::Float(50.0),
    );
    definition
}

fn mcp2515_tja1050_can_module_behavior() -> BehaviorDefinition {
    let mut definition = BehaviorDefinition::new(
        "mcp2515_tja1050_can_module_behavior",
        BehaviorEngine::Mcp2515CanModule,
    );
    definition.description =
        Some("Behavior definition for the SPI MCP2515 + TJA1050 CAN breakout.".to_string());
    definition.ports = vec![
        BehaviorPortBinding::new("INT", "interrupt"),
        BehaviorPortBinding::new("SCK", "spi_sck"),
        BehaviorPortBinding::new("SI", "spi_mosi"),
        BehaviorPortBinding::new("SO", "spi_miso"),
        BehaviorPortBinding::new("CS", "spi_cs"),
        BehaviorPortBinding::new("VCC", "vcc"),
        BehaviorPortBinding::new("GND", "gnd"),
        BehaviorPortBinding::new("CANH", "can_h"),
        BehaviorPortBinding::new("CANL", "can_l"),
    ];
    definition.parameters.insert(
        "oscillator_hz".to_string(),
        BehaviorValue::Integer(16_000_000),
    );
    definition
}

fn max31865_breakout_behavior() -> BehaviorDefinition {
    let mut definition = BehaviorDefinition::new(
        "max31865_breakout_behavior",
        BehaviorEngine::Max31865RtdFrontend,
    );
    definition.description =
        Some("Behavior definition for the MAX31865 breakout driving a PT1000 probe.".to_string());
    definition.ports = vec![
        BehaviorPortBinding::new("CLK", "spi_sck"),
        BehaviorPortBinding::new("SDO", "spi_miso"),
        BehaviorPortBinding::new("SDI", "spi_mosi"),
        BehaviorPortBinding::new("CS", "spi_cs"),
        BehaviorPortBinding::new("VCC", "vcc"),
        BehaviorPortBinding::new("GND", "gnd"),
        BehaviorPortBinding::new("F+", "force_plus"),
        BehaviorPortBinding::new("F-", "force_minus"),
        BehaviorPortBinding::new("RTD+", "sense_plus"),
        BehaviorPortBinding::new("RTD-", "sense_minus"),
    ];
    definition
        .parameters
        .insert("nominal_rtd_ohms".to_string(), BehaviorValue::Float(1000.0));
    definition.parameters.insert(
        "reference_resistor_ohms".to_string(),
        BehaviorValue::Float(4300.0),
    );
    definition
        .parameters
        .insert("temperature_c".to_string(), BehaviorValue::Float(20.0));
    definition
}

fn lc_lm358_pwm_to_0_10v_behavior() -> BehaviorDefinition {
    let mut definition = BehaviorDefinition::new(
        "lc_lm358_pwm_to_0_10v_behavior",
        BehaviorEngine::PwmToVoltage,
    );
    definition.description =
        Some("Behavior definition for the LC-LM358 PWM to 0-10V interface board.".to_string());
    definition.ports = vec![
        BehaviorPortBinding::new("GND", "gnd"),
        BehaviorPortBinding::new("PWM", "pwm_input"),
        BehaviorPortBinding::new("VOUT", "analog_output"),
        BehaviorPortBinding::new("VCC", "vcc"),
    ];
    definition
        .parameters
        .insert("supply_voltage".to_string(), BehaviorValue::Float(12.0));
    definition
        .parameters
        .insert("output_min_voltage".to_string(), BehaviorValue::Float(0.0));
    definition
        .parameters
        .insert("output_max_voltage".to_string(), BehaviorValue::Float(10.0));
    definition
        .parameters
        .insert("pwm_duty".to_string(), BehaviorValue::Float(0.0));
    definition
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
        BehaviorInstance,
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
    fn behavior_references_can_load_builtin_definitions() {
        let reference = rust_project::DefinitionReference::builtin(
            rust_project::DefinitionReferenceKind::BehaviorDefinition,
            "gy_sht31_d_behavior",
        );
        let definition = load_behavior_definition_from_reference(&reference).expect("definition");
        assert_eq!(definition.name, "gy_sht31_d_behavior");
    }
}
