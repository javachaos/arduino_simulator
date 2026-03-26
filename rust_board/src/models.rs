use std::collections::BTreeMap;

use serde::Deserialize;

use crate::dsl::{derive_nets, Board, Component, DslError, Pad, Position, DSL_VERSION};

const DEFAULT_BOARD_LAYERS: [&str; 3] = ["virtual", "F.Cu", "B.Cu"];
const HEADER_LAYERS: [&str; 4] = ["F.Cu", "B.Cu", "F.Mask", "B.Mask"];
const VIRTUAL_LAYERS: [&str; 1] = ["virtual"];

struct BuiltInBoardAssetRef {
    name: &'static str,
    json: &'static str,
}

const BUILT_IN_BOARD_ASSETS: &[BuiltInBoardAssetRef] = &[
    BuiltInBoardAssetRef {
        name: "arduino_mega_2560_rev3",
        json: include_str!("../builtins/arduino_mega_2560_rev3.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "arduino_nano_v3",
        json: include_str!("../builtins/arduino_nano_v3.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "gy_sht31_d",
        json: include_str!("../builtins/gy_sht31_d.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "lc_lm358_pwm_to_0_10v",
        json: include_str!("../builtins/lc_lm358_pwm_to_0_10v.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "max31865_breakout",
        json: include_str!("../builtins/max31865_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "mcp2515_tja1050_can_module",
        json: include_str!("../builtins/mcp2515_tja1050_can_module.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "aht20_breakout",
        json: include_str!("../builtins/aht20_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "ads1115_breakout",
        json: include_str!("../builtins/ads1115_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "bh1750_breakout",
        json: include_str!("../builtins/bh1750_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "bme280_breakout",
        json: include_str!("../builtins/bme280_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "bmp280_breakout",
        json: include_str!("../builtins/bmp280_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "ina219_breakout",
        json: include_str!("../builtins/ina219_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "max31855_breakout",
        json: include_str!("../builtins/max31855_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "max6675_breakout",
        json: include_str!("../builtins/max6675_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "mpu6050_breakout",
        json: include_str!("../builtins/mpu6050_breakout.board.json"),
    },
    BuiltInBoardAssetRef {
        name: "vl53l0x_breakout",
        json: include_str!("../builtins/vl53l0x_breakout.board.json"),
    },
];

#[derive(Debug, Clone, Deserialize)]
struct BuiltInBoardAsset {
    name: String,
    title: Option<String>,
    #[serde(default)]
    layers: Vec<String>,
    components: Vec<BuiltInBoardComponentAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "template", rename_all = "snake_case")]
enum BuiltInBoardComponentAsset {
    Header {
        reference: String,
        value: String,
        footprint: String,
        signals: Vec<String>,
        position: Position,
    },
    Module {
        reference: String,
        value: String,
        footprint: String,
        signals: Vec<String>,
        position: Position,
    },
    Mcu {
        reference: String,
        value: String,
        footprint: String,
        signals: Vec<String>,
        position: Position,
    },
}

impl BuiltInBoardAsset {
    fn into_board(self, expected_name: &str) -> Result<Board, DslError> {
        if self.name != expected_name {
            return Err(DslError::new(format!(
                "built-in board asset name mismatch: expected {expected_name:?}, found {:?}",
                self.name
            )));
        }

        let components = self
            .components
            .into_iter()
            .map(BuiltInBoardComponentAsset::into_component)
            .collect::<Vec<_>>();
        let layers = if self.layers.is_empty() {
            default_board_layers()
        } else {
            self.layers
        };

        Ok(Board {
            name: self.name.clone(),
            source_path: format!("builtin://{}", self.name),
            components: components.clone(),
            nets: derive_nets(&components),
            source_format: "builtin".to_string(),
            title: self.title,
            generator: Some("arduino_simulator".to_string()),
            generator_version: Some(DSL_VERSION.to_string()),
            board_version: None,
            paper: None,
            layers,
        })
    }
}

impl BuiltInBoardComponentAsset {
    fn into_component(self) -> Component {
        match self {
            Self::Header {
                reference,
                value,
                footprint,
                signals,
                position,
            } => header_component(&reference, &value, &footprint, &signals, position),
            Self::Module {
                reference,
                value,
                footprint,
                signals,
                position,
            } => module_component(&reference, &value, &footprint, &signals, position),
            Self::Mcu {
                reference,
                value,
                footprint,
                signals,
                position,
            } => mcu_component(&reference, &value, &footprint, &signals, position),
        }
    }
}

fn default_board_layers() -> Vec<String> {
    DEFAULT_BOARD_LAYERS
        .iter()
        .map(|layer| (*layer).to_string())
        .collect()
}

fn unique_signals(signals: &[String]) -> Vec<String> {
    let mut ordered = Vec::new();
    for signal in signals {
        if ordered.iter().any(|existing| existing == signal) {
            continue;
        }
        ordered.push(signal.clone());
    }
    ordered
}

fn header_component(
    reference: &str,
    value: &str,
    footprint: &str,
    signals: &[String],
    position: Position,
) -> Component {
    let pads = signals
        .iter()
        .enumerate()
        .map(|(index, signal)| Pad {
            number: (index + 1).to_string(),
            pad_type: "through_hole".to_string(),
            shape: "oval".to_string(),
            layers: HEADER_LAYERS
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            net_name: Some(signal.clone()),
            net_code: None,
            position: Some(Position::new(0.0, (index as f64) * 2.54, None)),
            size_mm: Some((1.7, 1.7)),
            drill_mm: Some(vec![1.0]),
            uuid: None,
        })
        .collect();

    Component {
        reference: reference.to_string(),
        value: Some(value.to_string()),
        kind: "connector".to_string(),
        footprint: footprint.to_string(),
        layer: "F.Cu".to_string(),
        pads,
        position: Some(position),
        uuid: None,
        properties: BTreeMap::from([("model_role".to_string(), "logical_header".to_string())]),
    }
}

fn virtual_signal_component(
    reference: &str,
    value: &str,
    footprint: &str,
    kind: &str,
    role: &str,
    signals: &[String],
    position: Position,
) -> Component {
    let signals = unique_signals(signals);
    let split = signals.len().div_ceil(2);
    let pads = signals
        .iter()
        .enumerate()
        .map(|(index, signal)| {
            let column = if index < split { 0.0 } else { 10.0 };
            let row = if index < split { index } else { index - split };
            Pad {
                number: signal.clone(),
                pad_type: "virtual".to_string(),
                shape: "round".to_string(),
                layers: VIRTUAL_LAYERS
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect(),
                net_name: Some(signal.clone()),
                net_code: None,
                position: Some(Position::new(column, (row as f64) * 1.27, None)),
                size_mm: Some((0.8, 0.8)),
                drill_mm: None,
                uuid: None,
            }
        })
        .collect();

    Component {
        reference: reference.to_string(),
        value: Some(value.to_string()),
        kind: kind.to_string(),
        footprint: footprint.to_string(),
        layer: "virtual".to_string(),
        pads,
        position: Some(position),
        uuid: None,
        properties: BTreeMap::from([("model_role".to_string(), role.to_string())]),
    }
}

fn mcu_component(
    reference: &str,
    value: &str,
    footprint: &str,
    signals: &[String],
    position: Position,
) -> Component {
    virtual_signal_component(
        reference,
        value,
        footprint,
        "mcu",
        "mcu_abstraction",
        signals,
        position,
    )
}

fn module_component(
    reference: &str,
    value: &str,
    footprint: &str,
    signals: &[String],
    position: Position,
) -> Component {
    virtual_signal_component(
        reference,
        value,
        footprint,
        "module",
        "module_abstraction",
        signals,
        position,
    )
}

fn load_builtin_board_asset(board_name: &str) -> Result<Board, DslError> {
    let asset = BUILT_IN_BOARD_ASSETS
        .iter()
        .find(|asset| asset.name == board_name)
        .ok_or_else(|| {
            DslError::new(format!(
                "unknown built-in board model {board_name:?}; available models: {}",
                built_in_board_model_names().join(", ")
            ))
        })?;
    let parsed = serde_json::from_str::<BuiltInBoardAsset>(asset.json).map_err(|error| {
        DslError::new(format!(
            "invalid built-in board asset {board_name:?}: {error}"
        ))
    })?;
    parsed.into_board(asset.name)
}

pub fn build_arduino_mega_2560_rev3_board() -> Board {
    load_builtin_board_asset("arduino_mega_2560_rev3").expect("valid built-in board asset")
}

pub fn build_gy_sht31_d_board() -> Board {
    load_builtin_board_asset("gy_sht31_d").expect("valid built-in board asset")
}

pub fn build_mcp2515_tja1050_can_module_board() -> Board {
    load_builtin_board_asset("mcp2515_tja1050_can_module").expect("valid built-in board asset")
}

pub fn build_max31865_breakout_board() -> Board {
    load_builtin_board_asset("max31865_breakout").expect("valid built-in board asset")
}

pub fn build_lc_lm358_pwm_to_0_10v_board() -> Board {
    load_builtin_board_asset("lc_lm358_pwm_to_0_10v").expect("valid built-in board asset")
}

pub fn build_arduino_nano_v3_board() -> Board {
    load_builtin_board_asset("arduino_nano_v3").expect("valid built-in board asset")
}

pub fn built_in_board_model_names() -> Vec<&'static str> {
    BUILT_IN_BOARD_ASSETS
        .iter()
        .map(|asset| asset.name)
        .collect()
}

pub fn load_built_in_board_model(board_name: &str) -> Result<Board, DslError> {
    load_builtin_board_asset(board_name)
}

#[cfg(test)]
mod tests {
    use crate::lang::dump_board_dsl;

    use super::{
        build_arduino_mega_2560_rev3_board, build_arduino_nano_v3_board, build_gy_sht31_d_board,
        build_lc_lm358_pwm_to_0_10v_board, build_max31865_breakout_board,
        build_mcp2515_tja1050_can_module_board, built_in_board_model_names,
        load_built_in_board_model,
    };

    fn component_by_ref<'a>(
        board: &'a crate::dsl::Board,
        reference: &str,
    ) -> &'a crate::dsl::Component {
        board
            .components
            .iter()
            .find(|component| component.reference == reference)
            .expect("component")
    }

    fn pad_net<'a>(component: &'a crate::dsl::Component, number: &str) -> Option<&'a str> {
        component
            .pads
            .iter()
            .find(|pad| pad.number == number)
            .and_then(|pad| pad.net_name.as_deref())
    }

    fn net_connections(board: &crate::dsl::Board, net_name: &str) -> Vec<(String, String)> {
        board
            .nets
            .iter()
            .find(|net| net.name == net_name)
            .expect("net")
            .connections
            .iter()
            .map(|connection| (connection.component.clone(), connection.pad.clone()))
            .collect()
    }

    #[test]
    fn built_in_board_model_names_are_stable() {
        assert_eq!(
            built_in_board_model_names(),
            vec![
                "arduino_mega_2560_rev3",
                "arduino_nano_v3",
                "gy_sht31_d",
                "lc_lm358_pwm_to_0_10v",
                "max31865_breakout",
                "mcp2515_tja1050_can_module",
                "aht20_breakout",
                "ads1115_breakout",
                "bh1750_breakout",
                "bme280_breakout",
                "bmp280_breakout",
                "ina219_breakout",
                "max31855_breakout",
                "max6675_breakout",
                "mpu6050_breakout",
                "vl53l0x_breakout"
            ]
        );
    }

    #[test]
    fn mega_board_model_matches_expected_nets() {
        let board = build_arduino_mega_2560_rev3_board();
        assert_eq!(board.name, "arduino_mega_2560_rev3");
        assert_eq!(board.source_format, "builtin");
        assert_eq!(board.components.len(), 5);

        let power = component_by_ref(&board, "J_POWER");
        assert_eq!(pad_net(power, "1"), Some("IOREF"));
        assert_eq!(pad_net(power, "4"), Some("+5V"));
        assert_eq!(pad_net(power, "8"), Some("AREF"));

        let digital_high = component_by_ref(&board, "J_DIGITAL_HIGH");
        assert_eq!(pad_net(digital_high, "7"), Some("D28"));
        assert_eq!(pad_net(digital_high, "31"), Some("D52_SCK"));
        assert_eq!(pad_net(digital_high, "32"), Some("D53_SS"));

        assert_eq!(
            net_connections(&board, "D52_SCK"),
            vec![
                ("J_DIGITAL_HIGH".to_string(), "31".to_string()),
                ("U1".to_string(), "D52_SCK".to_string())
            ]
        );
    }

    #[test]
    fn nano_board_model_matches_expected_nets() {
        let board = build_arduino_nano_v3_board();
        assert_eq!(board.name, "arduino_nano_v3");
        assert_eq!(board.components.len(), 3);

        let left = component_by_ref(&board, "J_LEFT");
        let right = component_by_ref(&board, "J_RIGHT");
        assert_eq!(pad_net(left, "1"), Some("D13_SCK"));
        assert_eq!(pad_net(left, "15"), Some("VIN"));
        assert_eq!(pad_net(right, "1"), Some("D12_MISO"));
        assert_eq!(pad_net(right, "14"), Some("D0_RX"));
        assert_eq!(pad_net(right, "15"), Some("D1_TX"));

        assert_eq!(
            net_connections(&board, "RESET"),
            vec![
                ("J_LEFT".to_string(), "13".to_string()),
                ("J_RIGHT".to_string(), "13".to_string()),
                ("U1".to_string(), "RESET".to_string())
            ]
        );
    }

    #[test]
    fn sht31_breakout_model_matches_expected_nets() {
        let board = build_gy_sht31_d_board();
        assert_eq!(board.name, "gy_sht31_d");
        assert_eq!(board.components.len(), 2);

        let header = component_by_ref(&board, "J1");
        assert_eq!(pad_net(header, "1"), Some("SDA"));
        assert_eq!(pad_net(header, "2"), Some("SCL"));
        assert_eq!(pad_net(header, "3"), Some("VCC"));
        assert_eq!(pad_net(header, "4"), Some("GND"));

        assert_eq!(
            net_connections(&board, "SDA"),
            vec![
                ("J1".to_string(), "1".to_string()),
                ("U1".to_string(), "SDA".to_string())
            ]
        );
    }

    #[test]
    fn mcp2515_can_module_model_matches_expected_nets() {
        let board = build_mcp2515_tja1050_can_module_board();
        assert_eq!(board.name, "mcp2515_tja1050_can_module");
        assert_eq!(board.components.len(), 3);

        let host = component_by_ref(&board, "J_HOST");
        let can = component_by_ref(&board, "J_CAN");
        assert_eq!(pad_net(host, "1"), Some("INT"));
        assert_eq!(pad_net(host, "5"), Some("CS"));
        assert_eq!(pad_net(host, "7"), Some("VCC"));
        assert_eq!(pad_net(can, "1"), Some("CANH"));
        assert_eq!(pad_net(can, "2"), Some("CANL"));
        assert_eq!(pad_net(can, "3"), Some("GND"));
    }

    #[test]
    fn max31865_breakout_model_matches_expected_nets() {
        let board = build_max31865_breakout_board();
        assert_eq!(board.name, "max31865_breakout");
        assert_eq!(board.components.len(), 3);

        let host = component_by_ref(&board, "J_HOST");
        let rtd = component_by_ref(&board, "J_RTD");
        assert_eq!(pad_net(host, "1"), Some("CLK"));
        assert_eq!(pad_net(host, "4"), Some("CS"));
        assert_eq!(pad_net(host, "5"), Some("VCC"));
        assert_eq!(pad_net(rtd, "1"), Some("F+"));
        assert_eq!(pad_net(rtd, "4"), Some("RTD-"));
    }

    #[test]
    fn pwm_to_0_10v_module_model_matches_expected_nets() {
        let board = build_lc_lm358_pwm_to_0_10v_board();
        assert_eq!(board.name, "lc_lm358_pwm_to_0_10v");
        assert_eq!(board.components.len(), 4);

        let output = component_by_ref(&board, "J_OUT");
        let pwm = component_by_ref(&board, "J_PWM");
        let power = component_by_ref(&board, "J_PWR");
        assert_eq!(pad_net(output, "1"), Some("GND"));
        assert_eq!(pad_net(output, "2"), Some("VOUT"));
        assert_eq!(pad_net(pwm, "2"), Some("PWM"));
        assert_eq!(pad_net(power, "2"), Some("VCC"));

        assert_eq!(
            net_connections(&board, "GND"),
            vec![
                ("J_OUT".to_string(), "1".to_string()),
                ("J_PWM".to_string(), "1".to_string()),
                ("J_PWR".to_string(), "1".to_string()),
                ("U1".to_string(), "GND".to_string())
            ]
        );
    }

    #[test]
    fn loading_and_dsl_dump_work() {
        let board = load_built_in_board_model("arduino_nano_v3").expect("board");
        assert_eq!(
            board.title.as_deref(),
            Some("Arduino Nano V3 (Logical Model)")
        );
        let dsl = dump_board_dsl(&build_arduino_mega_2560_rev3_board());
        assert!(dsl.contains("(name \"arduino_mega_2560_rev3\")"));
        assert!(dsl.contains("(source_format \"builtin\")"));
        assert!(dsl.contains("(reference \"J_DIGITAL_HIGH\")"));
        let sensor = load_built_in_board_model("gy_sht31_d").expect("sensor");
        assert_eq!(
            sensor.title.as_deref(),
            Some("GY-SHT31-D SHT31 Breakout (Logical Model)")
        );
    }

    #[test]
    fn new_builtin_sensor_models_load_with_expected_ports() {
        let bme = load_built_in_board_model("bme280_breakout").expect("bme");
        let bme_header = component_by_ref(&bme, "J1");
        assert_eq!(pad_net(bme_header, "1"), Some("SDA"));
        assert_eq!(pad_net(bme_header, "5"), Some("GND"));

        let ads = load_built_in_board_model("ads1115_breakout").expect("ads");
        let analog = component_by_ref(&ads, "J_IN");
        assert_eq!(pad_net(analog, "1"), Some("AIN0"));
        assert_eq!(pad_net(analog, "4"), Some("AIN3"));

        let imu = load_built_in_board_model("mpu6050_breakout").expect("imu");
        let imu_header = component_by_ref(&imu, "J1");
        assert_eq!(pad_net(imu_header, "3"), Some("INT"));
    }
}
