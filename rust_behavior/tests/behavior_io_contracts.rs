use rust_behavior::{
    built_in_behavior_names, instantiate_behavior, load_built_in_behavior_definition,
    BehaviorError, BehaviorInstance,
};
use rust_project::BehaviorValue;

type Update = (&'static str, BehaviorValue, &'static str, BehaviorValue);

fn built_in_instance(name: &str) -> BehaviorInstance {
    let definition = load_built_in_behavior_definition(name).expect("built-in definition");
    instantiate_behavior(&definition).expect("built-in instance")
}

fn verify_updates(name: &str, updates: Vec<Update>) -> BehaviorInstance {
    let mut instance = built_in_instance(name);
    for (input, value, output, expected) in updates {
        instance
            .apply_input(input, value)
            .unwrap_or_else(|error| panic!("{name} rejected {input}: {error}"));
        assert_eq!(
            instance
                .output(output)
                .unwrap_or_else(|error| panic!("{name} did not publish {output}: {error}")),
            expected,
            "{name}: {input} should drive {output}"
        );
    }
    instance
}

#[test]
fn builtin_inputs_drive_their_documented_outputs() {
    let contracts = [
        (
            "gy_sht31_d_behavior",
            vec![
                (
                    "ambient_temp_c",
                    BehaviorValue::Float(12.5),
                    "temperature_c",
                    BehaviorValue::Float(12.5),
                ),
                (
                    "relative_humidity_percent",
                    BehaviorValue::Integer(64),
                    "relative_humidity_percent",
                    BehaviorValue::Float(64.0),
                ),
            ],
        ),
        (
            "aht20_breakout_behavior",
            vec![
                (
                    "temperature_c",
                    BehaviorValue::Float(18.25),
                    "temperature_c",
                    BehaviorValue::Float(18.25),
                ),
                (
                    "relative_humidity_percent",
                    BehaviorValue::Float(47.5),
                    "relative_humidity_percent",
                    BehaviorValue::Float(47.5),
                ),
            ],
        ),
        (
            "bme280_breakout_behavior",
            vec![
                (
                    "temperature_c",
                    BehaviorValue::Float(-4.0),
                    "temperature_c",
                    BehaviorValue::Float(-4.0),
                ),
                (
                    "pressure_hpa",
                    BehaviorValue::Float(987.6),
                    "pressure_hpa",
                    BehaviorValue::Float(987.6),
                ),
                (
                    "relative_humidity_percent",
                    BehaviorValue::Float(81.0),
                    "relative_humidity_percent",
                    BehaviorValue::Float(81.0),
                ),
            ],
        ),
        (
            "bh1750_breakout_behavior",
            vec![(
                "illuminance_lux",
                BehaviorValue::Float(1_234.5),
                "illuminance_lux",
                BehaviorValue::Float(1_234.5),
            )],
        ),
        (
            "ina219_breakout_behavior",
            vec![
                (
                    "bus_voltage_v",
                    BehaviorValue::Float(23.75),
                    "bus_voltage_v",
                    BehaviorValue::Float(23.75),
                ),
                (
                    "shunt_voltage_mv",
                    BehaviorValue::Float(7.5),
                    "shunt_voltage_mv",
                    BehaviorValue::Float(7.5),
                ),
                (
                    "current_ma",
                    BehaviorValue::Float(315.0),
                    "current_ma",
                    BehaviorValue::Float(315.0),
                ),
                (
                    "power_mw",
                    BehaviorValue::Float(7_481.25),
                    "power_mw",
                    BehaviorValue::Float(7_481.25),
                ),
                (
                    "alert_asserted",
                    BehaviorValue::Bool(true),
                    "alert_asserted",
                    BehaviorValue::Bool(true),
                ),
            ],
        ),
        (
            "mpu6050_breakout_behavior",
            vec![
                (
                    "accel_x_g",
                    BehaviorValue::Float(0.125),
                    "accel_x_g",
                    BehaviorValue::Float(0.125),
                ),
                (
                    "accel_y_g",
                    BehaviorValue::Float(-0.25),
                    "accel_y_g",
                    BehaviorValue::Float(-0.25),
                ),
                (
                    "accel_z_g",
                    BehaviorValue::Float(0.975),
                    "accel_z_g",
                    BehaviorValue::Float(0.975),
                ),
                (
                    "gyro_x_dps",
                    BehaviorValue::Float(11.0),
                    "gyro_x_dps",
                    BehaviorValue::Float(11.0),
                ),
                (
                    "gyro_y_dps",
                    BehaviorValue::Float(-22.0),
                    "gyro_y_dps",
                    BehaviorValue::Float(-22.0),
                ),
                (
                    "gyro_z_dps",
                    BehaviorValue::Float(33.0),
                    "gyro_z_dps",
                    BehaviorValue::Float(33.0),
                ),
                (
                    "temperature_c",
                    BehaviorValue::Float(41.5),
                    "temperature_c",
                    BehaviorValue::Float(41.5),
                ),
                (
                    "interrupt_asserted",
                    BehaviorValue::Bool(true),
                    "interrupt_asserted",
                    BehaviorValue::Bool(true),
                ),
            ],
        ),
        (
            "ads1115_breakout_behavior",
            vec![
                (
                    "ain0_v",
                    BehaviorValue::Float(0.25),
                    "ain0_v",
                    BehaviorValue::Float(0.25),
                ),
                (
                    "ain1_v",
                    BehaviorValue::Float(1.25),
                    "ain1_v",
                    BehaviorValue::Float(1.25),
                ),
                (
                    "ain2_v",
                    BehaviorValue::Float(2.25),
                    "ain2_v",
                    BehaviorValue::Float(2.25),
                ),
                (
                    "ain3_v",
                    BehaviorValue::Float(3.25),
                    "ain3_v",
                    BehaviorValue::Float(3.25),
                ),
                (
                    "alert_asserted",
                    BehaviorValue::Bool(true),
                    "alert_asserted",
                    BehaviorValue::Bool(true),
                ),
            ],
        ),
        (
            "vl53l0x_breakout_behavior",
            vec![
                (
                    "distance_mm",
                    BehaviorValue::Integer(2_000),
                    "distance_mm",
                    BehaviorValue::Integer(2_000),
                ),
                (
                    "signal_valid",
                    BehaviorValue::Bool(false),
                    "signal_valid",
                    BehaviorValue::Bool(false),
                ),
            ],
        ),
        (
            "max31855_breakout_behavior",
            vec![
                (
                    "temperature_c",
                    BehaviorValue::Float(250.25),
                    "temperature_c",
                    BehaviorValue::Float(250.25),
                ),
                (
                    "internal_temp_c",
                    BehaviorValue::Float(44.0),
                    "internal_temp_c",
                    BehaviorValue::Float(44.0),
                ),
                (
                    "fault_open",
                    BehaviorValue::Bool(true),
                    "fault_open",
                    BehaviorValue::Bool(true),
                ),
                (
                    "fault_short_to_gnd",
                    BehaviorValue::Bool(true),
                    "fault_short_to_gnd",
                    BehaviorValue::Bool(true),
                ),
                (
                    "fault_short_to_vcc",
                    BehaviorValue::Bool(true),
                    "fault_short_to_vcc",
                    BehaviorValue::Bool(true),
                ),
            ],
        ),
        (
            "mcp2515_tja1050_can_module_behavior",
            vec![
                (
                    "interrupt_asserted",
                    BehaviorValue::Bool(true),
                    "interrupt_asserted",
                    BehaviorValue::Bool(true),
                ),
                (
                    "tx_pending_frames",
                    BehaviorValue::Integer(3),
                    "tx_pending_frames",
                    BehaviorValue::Integer(3),
                ),
                (
                    "can_bus_active",
                    BehaviorValue::Bool(true),
                    "can_bus_active",
                    BehaviorValue::Bool(true),
                ),
            ],
        ),
        (
            "max31865_breakout_behavior",
            vec![
                (
                    "temperature_c",
                    BehaviorValue::Float(-50.0),
                    "temperature_c",
                    BehaviorValue::Float(-50.0),
                ),
                (
                    "fault_status",
                    BehaviorValue::Integer(0x84),
                    "fault_status",
                    BehaviorValue::Integer(0x84),
                ),
            ],
        ),
    ];

    for (name, updates) in contracts {
        verify_updates(name, updates);
    }

    for (name, output, expected) in [
        (
            "gy_sht31_d_behavior",
            "address",
            BehaviorValue::Integer(0x44),
        ),
        (
            "aht20_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x38),
        ),
        (
            "bme280_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x76),
        ),
        (
            "bh1750_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x23),
        ),
        (
            "ina219_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x40),
        ),
        (
            "mpu6050_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x68),
        ),
        (
            "ads1115_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x48),
        ),
        (
            "ads1115_breakout_behavior",
            "gain_volts",
            BehaviorValue::Float(4.096),
        ),
        (
            "ads1115_breakout_behavior",
            "sample_rate_sps",
            BehaviorValue::Integer(128),
        ),
        (
            "vl53l0x_breakout_behavior",
            "address",
            BehaviorValue::Integer(0x29),
        ),
    ] {
        assert_eq!(built_in_instance(name).output(output).unwrap(), expected);
    }

    let rtd = verify_updates(
        "max31865_breakout_behavior",
        vec![(
            "temperature_c",
            BehaviorValue::Float(-50.0),
            "temperature_c",
            BehaviorValue::Float(-50.0),
        )],
    );
    let BehaviorValue::Float(resistance_ohms) = rtd.output("resistance_ohms").unwrap() else {
        panic!("RTD resistance must be numeric");
    };
    assert!(resistance_ohms.is_finite() && resistance_ohms > 0.0);
    assert!((resistance_ohms - 803.062_825).abs() < 0.001);

    let output = verify_updates(
        "lc_lm358_pwm_to_0_10v_behavior",
        vec![(
            "pwm_duty",
            BehaviorValue::Float(1.5),
            "pwm_duty",
            BehaviorValue::Float(1.0),
        )],
    );
    assert_eq!(
        output.output("output_voltage").unwrap(),
        BehaviorValue::Float(10.0)
    );
    verify_updates(
        "lc_lm358_pwm_to_0_10v_behavior",
        vec![(
            "pwm_duty",
            BehaviorValue::Float(-0.5),
            "pwm_duty",
            BehaviorValue::Float(0.0),
        )],
    );
}

#[test]
fn behavior_contracts_reject_unknown_and_mistyped_inputs() {
    for name in built_in_behavior_names() {
        let mut instance = built_in_instance(name);
        assert!(matches!(
            instance.apply_input("not_a_real_input", BehaviorValue::Bool(true)),
            Err(BehaviorError::UnknownInput(input)) if input == "not_a_real_input"
        ));
        assert!(matches!(
            instance.output("not_a_real_output"),
            Err(BehaviorError::UnknownOutput(output)) if output == "not_a_real_output"
        ));
    }

    for (name, input, value, expected_message) in [
        (
            "ina219_breakout_behavior",
            "bus_voltage_v",
            BehaviorValue::Text("high".into()),
            "expects a number",
        ),
        (
            "ina219_breakout_behavior",
            "alert_asserted",
            BehaviorValue::Integer(1),
            "expects a boolean",
        ),
        (
            "vl53l0x_breakout_behavior",
            "distance_mm",
            BehaviorValue::Integer(-1),
            "non-negative",
        ),
        (
            "vl53l0x_breakout_behavior",
            "distance_mm",
            BehaviorValue::Float(1.0),
            "expects an integer",
        ),
        (
            "max31865_breakout_behavior",
            "fault_status",
            BehaviorValue::Integer(256),
            "8-bit integer",
        ),
        (
            "max31865_breakout_behavior",
            "fault_status",
            BehaviorValue::Bool(true),
            "expects an integer",
        ),
    ] {
        assert!(matches!(
            built_in_instance(name).apply_input(input, value),
            Err(BehaviorError::InvalidInput(message)) if message.contains(expected_message)
        ));
    }

    for (name, input) in [
        ("bmp280_breakout_behavior", "relative_humidity_percent"),
        ("max6675_breakout_behavior", "internal_temp_c"),
    ] {
        assert!(matches!(
            built_in_instance(name).apply_input(input, BehaviorValue::Float(25.0)),
            Err(BehaviorError::UnknownInput(_))
        ));
    }
}
