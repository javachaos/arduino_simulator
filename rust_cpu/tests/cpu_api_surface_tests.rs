use rust_cpu::{Cpu, CpuConfig, CpuError, NullBus, StepOutcome};

fn des(k: u8) -> u16 {
    0x940B | (((k as u16) & 0x0F) << 4)
}

fn custom_config_without_default_sp() -> CpuConfig {
    CpuConfig {
        name: "test-device",
        program_size_bytes: 4,
        data_size_bytes: 64,
        sram_start_address: 0x20,
        pc_bits: 8,
        sreg_address: 60,
        spl_address: 61,
        sph_address: 62,
        rampd_address: None,
        rampx_address: None,
        rampy_address: None,
        rampz_address: None,
        eind_address: None,
        default_sp: None,
        supports_des: false,
    }
}

#[test]
fn public_api_bounds_and_reset_paths_report_expected_errors() {
    let config = custom_config_without_default_sp();
    assert_eq!(config.program_size_words(), 2);
    assert_eq!(config.pc_mask(), 0x00FF);
    assert_eq!(config.return_address_bytes(), 2);
    assert_eq!(config.stack_reset_value(), 63);

    let mut cpu = Cpu::new(config, NullBus);
    assert_eq!(cpu.sp(), 63);

    cpu.set_program_word(0, 0x1234).unwrap();
    assert_eq!(cpu.fetch_word(0).unwrap(), 0x1234);
    assert_eq!(cpu.read_program_byte(0), 0x34);
    assert_eq!(cpu.read_program_byte(99), 0xFF);

    assert_eq!(
        cpu.load_program_bytes(&[0xAA, 0xBB, 0xCC], 3).unwrap_err(),
        CpuError::ProgramBounds
    );
    assert_eq!(
        cpu.set_program_word(2, 0x5678).unwrap_err(),
        CpuError::ProgramBounds
    );
    assert_eq!(cpu.fetch_word(2).unwrap_err(), CpuError::ProgramBounds);

    assert_eq!(
        cpu.read_register(32).unwrap_err(),
        CpuError::InvalidRegister
    );
    assert_eq!(
        cpu.write_register(32, 0x55).unwrap_err(),
        CpuError::InvalidRegister
    );

    assert_eq!(cpu.read_data(64).unwrap_err(), CpuError::DataBounds);
    assert_eq!(cpu.write_data(64, 0x55).unwrap_err(), CpuError::DataBounds);

    cpu.reset(true);
    assert_eq!(cpu.fetch_word(0).unwrap(), 0xFFFF);
}

#[test]
fn execution_error_paths_are_exposed_through_the_public_cpu_api() {
    let mut sleeping_cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    sleeping_cpu.load_program_words(&[0x9588], 0).unwrap();
    assert_eq!(sleeping_cpu.step().unwrap(), StepOutcome::Sleeping);
    assert_eq!(sleeping_cpu.step().unwrap_err(), CpuError::Sleeping);

    let mut unsupported_cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    unsupported_cpu.load_program_words(&[0xFFFF], 0).unwrap();
    assert_eq!(
        unsupported_cpu.step().unwrap_err(),
        CpuError::UnsupportedInstruction {
            opcode: 0xFFFF,
            address: 0,
        }
    );

    let mut extended_pc_cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    extended_pc_cpu.load_program_words(&[0x9519], 0).unwrap();
    assert_eq!(
        extended_pc_cpu.step().unwrap_err(),
        CpuError::ExtendedPcRequired("eicall")
    );

    let mut interrupt_cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    assert_eq!(
        interrupt_cpu.take_interrupt(0, 4).unwrap_err(),
        CpuError::UnsupportedMnemonic("interrupt vector 0")
    );

    let mut des_cpu = Cpu::new(
        CpuConfig {
            name: "des-test-device",
            supports_des: true,
            ..custom_config_without_default_sp()
        },
        NullBus,
    );
    des_cpu.load_program_words(&[des(3)], 0).unwrap();
    assert_eq!(
        des_cpu.step().unwrap_err(),
        CpuError::UnsupportedMnemonic("des")
    );
}

#[test]
fn cpu_error_display_messages_cover_every_variant() {
    let cases = [
        (
            CpuError::ProgramBounds,
            "program address is out of range".to_string(),
        ),
        (
            CpuError::DataBounds,
            "data address is out of range".to_string(),
        ),
        (
            CpuError::InvalidRegister,
            "register index is out of range".to_string(),
        ),
        (CpuError::Sleeping, "CPU is sleeping".to_string()),
        (
            CpuError::InvalidSramOperation {
                instruction: "xch",
                address: 0x005F,
            },
            "xch requires an internal SRAM address, got 0x005F".to_string(),
        ),
        (
            CpuError::ExtendedPcRequired("eicall"),
            "eicall requires a device with a 22-bit PC".to_string(),
        ),
        (
            CpuError::InstructionUnavailable {
                instruction: "des",
                device: "atmega328p",
            },
            "des is not available on atmega328p".to_string(),
        ),
        (
            CpuError::UnsupportedInstruction {
                opcode: 0xFFFF,
                address: 7,
            },
            "unsupported opcode 0xFFFF at word address 7".to_string(),
        ),
        (
            CpuError::UnsupportedMnemonic("des"),
            "unsupported mnemonic des".to_string(),
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}
