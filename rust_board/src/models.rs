use std::collections::BTreeMap;

use crate::dsl::{derive_nets, Board, Component, DslError, Pad, Position, DSL_VERSION};

const HEADER_LAYERS: [&str; 4] = ["F.Cu", "B.Cu", "F.Mask", "B.Mask"];
const VIRTUAL_LAYERS: [&str; 1] = ["virtual"];

fn unique_signals(signals: &[&str]) -> Vec<String> {
    let mut ordered = Vec::new();
    for signal in signals {
        if ordered.iter().any(|existing| existing == signal) {
            continue;
        }
        ordered.push((*signal).to_string());
    }
    ordered
}

fn header_component(
    reference: &str,
    value: &str,
    footprint: &str,
    signals: &[&str],
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
            net_name: Some((*signal).to_string()),
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

fn build_board(name: &str, title: &str, components: Vec<Component>) -> Board {
    Board {
        name: name.to_string(),
        title: Some(title.to_string()),
        source_format: "builtin".to_string(),
        source_path: format!("builtin://{name}"),
        generator: Some("avrsim".to_string()),
        generator_version: Some(DSL_VERSION.to_string()),
        board_version: None,
        paper: None,
        layers: vec![
            "virtual".to_string(),
            "F.Cu".to_string(),
            "B.Cu".to_string(),
        ],
        nets: derive_nets(&components),
        components,
    }
}

pub fn build_arduino_mega_2560_rev3_board() -> Board {
    let power_signals = ["IOREF", "RESET", "+3V3", "+5V", "GND", "GND", "VIN", "AREF"];
    let analog_signals = [
        "A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13",
        "A14", "A15",
    ];
    let digital_low_signals = [
        "D0_RX0", "D1_TX0", "D2", "D3_PWM", "D4", "D5_PWM", "D6_PWM", "D7", "D8", "D9_PWM",
        "D10_PWM", "D11_PWM", "D12", "D13", "D14_TX3", "D15_RX3", "D16_TX2", "D17_RX2", "D18_TX1",
        "D19_RX1", "D20_SDA", "D21_SCL",
    ];
    let digital_high_signals = [
        "D22", "D23", "D24", "D25", "D26", "D27", "D28", "D29", "D30", "D31", "D32", "D33", "D34",
        "D35", "D36", "D37", "D38", "D39", "D40", "D41", "D42", "D43", "D44_PWM", "D45_PWM",
        "D46_PWM", "D47", "D48", "D49", "D50_MISO", "D51_MOSI", "D52_SCK", "D53_SS",
    ];
    let exposed_signals = unique_signals(
        &power_signals
            .iter()
            .chain(analog_signals.iter())
            .chain(digital_low_signals.iter())
            .chain(digital_high_signals.iter())
            .copied()
            .collect::<Vec<_>>(),
    );

    let components = vec![
        mcu_component(
            "U1",
            "ATmega2560",
            "Virtual:ATmega2560_BoardAbstraction",
            &exposed_signals,
            Position::new(55.0, 25.0, None),
        ),
        header_component(
            "J_POWER",
            "POWER",
            "Virtual:Header_1x08_2.54mm",
            &power_signals,
            Position::new(5.0, 5.0, None),
        ),
        header_component(
            "J_ANALOG",
            "ANALOG A0-A15",
            "Virtual:Header_1x16_2.54mm",
            &analog_signals,
            Position::new(20.0, 5.0, None),
        ),
        header_component(
            "J_DIGITAL_LOW",
            "DIGITAL D0-D21",
            "Virtual:Header_1x22_2.54mm",
            &digital_low_signals,
            Position::new(95.0, 5.0, None),
        ),
        header_component(
            "J_DIGITAL_HIGH",
            "DIGITAL D22-D53",
            "Virtual:Header_1x32_2.54mm",
            &digital_high_signals,
            Position::new(120.0, 5.0, None),
        ),
    ];

    build_board(
        "arduino_mega_2560_rev3",
        "Arduino Mega 2560 Rev3 (Logical Model)",
        components,
    )
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

pub fn build_gy_sht31_d_board() -> Board {
    let bus_signals = ["SDA", "SCL", "VCC", "GND"];
    let exposed_signals = unique_signals(&bus_signals);

    let components = vec![
        module_component(
            "U1",
            "GY-SHT31-D",
            "Virtual:GY-SHT31-D_BoardAbstraction",
            &exposed_signals,
            Position::new(22.0, 12.0, None),
        ),
        header_component(
            "J1",
            "I2C + Power",
            "Virtual:Header_1x04_2.54mm",
            &bus_signals,
            Position::new(5.0, 5.0, None),
        ),
    ];

    build_board(
        "gy_sht31_d",
        "GY-SHT31-D SHT31 Breakout (Logical Model)",
        components,
    )
}

pub fn build_mcp2515_tja1050_can_module_board() -> Board {
    let host_signals = ["INT", "SCK", "SI", "SO", "CS", "GND", "VCC"];
    let can_signals = ["CANH", "CANL", "GND"];
    let exposed_signals = unique_signals(
        &host_signals
            .iter()
            .chain(can_signals.iter())
            .copied()
            .collect::<Vec<_>>(),
    );

    let components = vec![
        module_component(
            "U1",
            "MCP2515 + TJA1050",
            "Virtual:MCP2515_TJA1050_CAN_Module",
            &exposed_signals,
            Position::new(32.0, 15.0, None),
        ),
        header_component(
            "J_HOST",
            "SPI + Control",
            "Virtual:Header_1x07_2.54mm",
            &host_signals,
            Position::new(5.0, 5.0, None),
        ),
        header_component(
            "J_CAN",
            "CAN Bus",
            "Virtual:TerminalBlock_1x03_5.08mm",
            &can_signals,
            Position::new(62.0, 5.0, None),
        ),
    ];

    build_board(
        "mcp2515_tja1050_can_module",
        "MCP2515 + TJA1050 CAN Module (Logical Model)",
        components,
    )
}

pub fn build_max31865_breakout_board() -> Board {
    let host_signals = ["CLK", "SDO", "SDI", "CS", "VCC", "GND"];
    let rtd_signals = ["F+", "F-", "RTD+", "RTD-"];
    let exposed_signals = unique_signals(
        &host_signals
            .iter()
            .chain(rtd_signals.iter())
            .copied()
            .collect::<Vec<_>>(),
    );

    let components = vec![
        module_component(
            "U1",
            "MAX31865",
            "Virtual:MAX31865_Breakout",
            &exposed_signals,
            Position::new(28.0, 15.0, None),
        ),
        header_component(
            "J_HOST",
            "SPI + Power",
            "Virtual:Header_1x06_2.54mm",
            &host_signals,
            Position::new(5.0, 5.0, None),
        ),
        header_component(
            "J_RTD",
            "RTD Probe",
            "Virtual:TerminalBlock_1x04_3.50mm",
            &rtd_signals,
            Position::new(58.0, 5.0, None),
        ),
    ];

    build_board(
        "max31865_breakout",
        "MAX31865 RTD Breakout (Logical Model)",
        components,
    )
}

pub fn build_lc_lm358_pwm_to_0_10v_board() -> Board {
    let pwm_input_signals = ["GND", "PWM"];
    let analog_output_signals = ["GND", "VOUT"];
    let power_signals = ["GND", "VCC"];
    let exposed_signals = unique_signals(
        &pwm_input_signals
            .iter()
            .chain(analog_output_signals.iter())
            .chain(power_signals.iter())
            .copied()
            .collect::<Vec<_>>(),
    );

    let components = vec![
        module_component(
            "U1",
            "LC-LM358-PWM2V",
            "Virtual:LC-LM358-PWM2V_Module",
            &exposed_signals,
            Position::new(28.0, 18.0, None),
        ),
        header_component(
            "J_OUT",
            "0-10V Output",
            "Virtual:TerminalBlock_1x02_5.08mm",
            &analog_output_signals,
            Position::new(5.0, 5.0, None),
        ),
        header_component(
            "J_PWM",
            "PWM Input",
            "Virtual:TerminalBlock_1x02_5.08mm",
            &pwm_input_signals,
            Position::new(30.0, 5.0, None),
        ),
        header_component(
            "J_PWR",
            "Module Power",
            "Virtual:TerminalBlock_1x02_5.08mm",
            &power_signals,
            Position::new(55.0, 5.0, None),
        ),
    ];

    build_board(
        "lc_lm358_pwm_to_0_10v",
        "LC-LM358 PWM to 0-10V Module (Logical Model)",
        components,
    )
}

pub fn build_arduino_nano_v3_board() -> Board {
    let left_header_signals = [
        "D13_SCK", "+3V3", "AREF", "A0", "A1", "A2", "A3", "A4_SDA", "A5_SCL", "A6", "A7", "+5V",
        "RESET", "GND", "VIN",
    ];
    let right_header_signals = [
        "D12_MISO", "D11_MOSI", "D10_SS", "D9_PWM", "D8", "D7", "D6_PWM", "D5_PWM", "D4", "D3_PWM",
        "D2", "GND", "RESET", "D0_RX", "D1_TX",
    ];
    let exposed_signals = unique_signals(
        &left_header_signals
            .iter()
            .chain(right_header_signals.iter())
            .copied()
            .collect::<Vec<_>>(),
    );
    let components = vec![
        mcu_component(
            "U1",
            "ATmega328P",
            "Virtual:ATmega328P_BoardAbstraction",
            &exposed_signals,
            Position::new(25.0, 20.0, None),
        ),
        header_component(
            "J_LEFT",
            "LEFT HEADER",
            "Virtual:Header_1x15_2.54mm",
            &left_header_signals,
            Position::new(5.0, 5.0, None),
        ),
        header_component(
            "J_RIGHT",
            "RIGHT HEADER",
            "Virtual:Header_1x15_2.54mm",
            &right_header_signals,
            Position::new(45.0, 5.0, None),
        ),
    ];

    build_board(
        "arduino_nano_v3",
        "Arduino Nano V3 (Logical Model)",
        components,
    )
}

pub fn built_in_board_model_names() -> Vec<&'static str> {
    vec![
        "arduino_mega_2560_rev3",
        "arduino_nano_v3",
        "gy_sht31_d",
        "lc_lm358_pwm_to_0_10v",
        "max31865_breakout",
        "mcp2515_tja1050_can_module",
    ]
}

pub fn load_built_in_board_model(board_name: &str) -> Result<Board, DslError> {
    match board_name {
        "arduino_mega_2560_rev3" => Ok(build_arduino_mega_2560_rev3_board()),
        "arduino_nano_v3" => Ok(build_arduino_nano_v3_board()),
        "gy_sht31_d" => Ok(build_gy_sht31_d_board()),
        "lc_lm358_pwm_to_0_10v" => Ok(build_lc_lm358_pwm_to_0_10v_board()),
        "max31865_breakout" => Ok(build_max31865_breakout_board()),
        "mcp2515_tja1050_can_module" => Ok(build_mcp2515_tja1050_can_module_board()),
        _ => Err(DslError::new(format!(
            "unknown built-in board model {board_name:?}; available models: {}",
            built_in_board_model_names().join(", ")
        ))),
    }
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
                "mcp2515_tja1050_can_module"
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
}
