use rust_cpu::{DecodedInstruction, Mnemonic, OperandSet, PointerMode, PointerRegister};

use super::{
    configured_baud, format_instruction, format_operands, format_pointer, mnemonic_name,
    RuntimeExit, SimulationRuntime, SimulationTarget, U2X0,
};
use crate::example_firmware::{MEGA_PIN_SWEEP, NANO_PIN_SWEEP};

fn ldi(d: u8, k: u8) -> u16 {
    assert!((16..=31).contains(&d));
    0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
}

fn sts(r: u8, address: usize) -> (u16, u16) {
    (
        0x9200 | (((r as u16) & 0x1F) << 4),
        (address & 0xFFFF) as u16,
    )
}

fn brk() -> u16 {
    0x9598
}

fn program_with_serial_tail(mut words: Vec<u16>) -> Vec<u16> {
    words.extend(std::iter::repeat_n(0x0000, 200));
    words.push(brk());
    words
}

fn make_hex(words: &[u16]) -> String {
    let mut program_bytes = Vec::with_capacity(words.len() * 2);
    for word in words {
        program_bytes.push((word & 0xFF) as u8);
        program_bytes.push((word >> 8) as u8);
    }

    let mut records = Vec::new();
    for (offset, chunk) in program_bytes.chunks(16).enumerate() {
        records.push(hex_record((offset * 16) as u16, 0x00, chunk));
    }
    records.push(hex_record(0x0000, 0x01, &[]));
    records.join("\n") + "\n"
}

fn hex_record(address: u16, record_type: u8, payload: &[u8]) -> String {
    let mut body = Vec::with_capacity(payload.len() + 5);
    body.push(payload.len() as u8);
    body.push((address >> 8) as u8);
    body.push((address & 0xFF) as u8);
    body.push(record_type);
    body.extend_from_slice(payload);
    let checksum = (0u8).wrapping_sub(body.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)));
    body.push(checksum);
    format!(
        ":{}",
        body.iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>()
    )
}

fn run_until_halt(runtime: &mut SimulationRuntime, instruction_budget: usize) {
    const MAX_CHUNKS: usize = 10_000;
    for _ in 0..MAX_CHUNKS {
        let exit = runtime.run_chunk(instruction_budget).expect("run chunk");
        if matches!(exit, RuntimeExit::BreakHit | RuntimeExit::Sleeping) {
            return;
        }
    }
    panic!(
        "runtime did not halt after {MAX_CHUNKS} chunks; pc=0x{:06X}",
        runtime.pc()
    );
}

#[test]
fn take_new_serial_bytes_preserves_output_from_previous_chunks() {
    let mut runtime = SimulationRuntime::new(SimulationTarget::Nano);
    let program = program_with_serial_tail(vec![
        ldi(16, 0x00),
        sts(16, rust_mcu::atmega328p::UBRR0L).0,
        sts(16, rust_mcu::atmega328p::UBRR0L).1,
        ldi(16, 1 << 3),
        sts(16, rust_mcu::atmega328p::UCSR0B).0,
        sts(16, rust_mcu::atmega328p::UCSR0B).1,
        ldi(16, b'W'),
        sts(16, rust_mcu::atmega328p::UDR0).0,
        sts(16, rust_mcu::atmega328p::UDR0).1,
    ]);

    runtime.load_hex(&make_hex(&program)).expect("load hex");
    run_until_halt(&mut runtime, 64);

    assert_eq!(runtime.serial_output_bytes(), b"W");
    assert_eq!(runtime.take_new_serial_bytes(), vec![b'W']);
    assert!(runtime.take_new_serial_bytes().is_empty());
}

#[test]
fn bundled_examples_emit_serial_output() {
    for example in [NANO_PIN_SWEEP, MEGA_PIN_SWEEP] {
        let mut runtime = SimulationRuntime::new(example.target);
        runtime.load_hex(example.hex).expect("load example hex");

        let mut emitted_serial = false;
        for _ in 0..400 {
            runtime.run_chunk(20_000).expect("run example");
            if !runtime.serial_output_bytes().is_empty() {
                emitted_serial = true;
                break;
            }
        }

        assert!(
            emitted_serial,
            "{} should emit serial output early in execution",
            example.label
        );
    }
}

#[test]
fn debugger_formats_each_operand_shape_without_losing_addressing_semantics() {
    let cases = [
        (Mnemonic::Nop, OperandSet::default(), ""),
        (
            Mnemonic::Jmp,
            OperandSet {
                k: Some(0x1234),
                ..OperandSet::default()
            },
            "0x001234",
        ),
        (
            Mnemonic::Lds,
            OperandSet {
                d: Some(16),
                k: Some(0x0123),
                ..OperandSet::default()
            },
            "r16, 0x0123",
        ),
        (
            Mnemonic::Sts,
            OperandSet {
                r: Some(17),
                k: Some(0x0456),
                ..OperandSet::default()
            },
            "0x0456, r17",
        ),
        (
            Mnemonic::LdPtr,
            OperandSet {
                d: Some(18),
                pointer: Some(PointerRegister::X),
                mode: Some(PointerMode::PostIncrement),
                ..OperandSet::default()
            },
            "r18, X+",
        ),
        (
            Mnemonic::LdPtr,
            OperandSet {
                d: Some(18),
                ..OperandSet::default()
            },
            "r18",
        ),
        (
            Mnemonic::StPtr,
            OperandSet {
                r: Some(19),
                pointer: Some(PointerRegister::Y),
                mode: Some(PointerMode::PreDecrement),
                ..OperandSet::default()
            },
            "-Y, r19",
        ),
        (
            Mnemonic::LdDisp,
            OperandSet {
                d: Some(20),
                pointer: Some(PointerRegister::Z),
                q: Some(7),
                ..OperandSet::default()
            },
            "r20, Z+7",
        ),
        (
            Mnemonic::StDisp,
            OperandSet {
                r: Some(21),
                pointer: Some(PointerRegister::Y),
                q: Some(2),
                ..OperandSet::default()
            },
            "Y+2, r21",
        ),
        (
            Mnemonic::Des,
            OperandSet {
                k: Some(8),
                ..OperandSet::default()
            },
            "8",
        ),
        (
            Mnemonic::Xch,
            OperandSet {
                d: Some(22),
                ..OperandSet::default()
            },
            "r22",
        ),
        (
            Mnemonic::Bset,
            OperandSet {
                s: Some(3),
                ..OperandSet::default()
            },
            "3",
        ),
        (
            Mnemonic::Cbi,
            OperandSet {
                a: Some(0x1f),
                b: Some(4),
                ..OperandSet::default()
            },
            "0x1F, 4",
        ),
        (
            Mnemonic::Push,
            OperandSet {
                r: Some(23),
                ..OperandSet::default()
            },
            "r23",
        ),
        (
            Mnemonic::Adiw,
            OperandSet {
                d: Some(24),
                k: Some(12),
                ..OperandSet::default()
            },
            "r24, 12",
        ),
        (
            Mnemonic::Mov,
            OperandSet {
                d: Some(2),
                r: Some(3),
                ..OperandSet::default()
            },
            "r2, r3",
        ),
        (
            Mnemonic::Ldi,
            OperandSet {
                d: Some(16),
                k: Some(0xab),
                ..OperandSet::default()
            },
            "r16, 0xAB",
        ),
        (
            Mnemonic::Rjmp,
            OperandSet {
                k: Some(-3),
                ..OperandSet::default()
            },
            "-3",
        ),
        (
            Mnemonic::Sbrc,
            OperandSet {
                r: Some(7),
                b: Some(5),
                ..OperandSet::default()
            },
            "r7, 5",
        ),
        (
            Mnemonic::In,
            OperandSet {
                d: Some(8),
                a: Some(0x2a),
                ..OperandSet::default()
            },
            "r8, 0x2A",
        ),
        (
            Mnemonic::Out,
            OperandSet {
                a: Some(0x2b),
                r: Some(9),
                ..OperandSet::default()
            },
            "0x2B, r9",
        ),
    ];

    for (mnemonic, operands, expected) in cases {
        assert_eq!(
            format_operands(mnemonic, &operands),
            expected,
            "unexpected debugger operands for {mnemonic:?}"
        );
    }

    assert_eq!(
        format_operands(
            Mnemonic::Cbi,
            &OperandSet {
                a: Some(0x1f),
                ..OperandSet::default()
            }
        ),
        "",
        "partial operands must not produce a misleading instruction"
    );
}

#[test]
fn debugger_mnemonic_labels_cover_the_complete_cpu_instruction_set() {
    let cases = [
        (Mnemonic::Nop, "nop"),
        (Mnemonic::Break, "break"),
        (Mnemonic::Sleep, "sleep"),
        (Mnemonic::Wdr, "wdr"),
        (Mnemonic::Ret, "ret"),
        (Mnemonic::Reti, "reti"),
        (Mnemonic::Ijmp, "ijmp"),
        (Mnemonic::Icall, "icall"),
        (Mnemonic::Eijmp, "eijmp"),
        (Mnemonic::Eicall, "eicall"),
        (Mnemonic::Jmp, "jmp"),
        (Mnemonic::Call, "call"),
        (Mnemonic::LdPtr, "ld"),
        (Mnemonic::StPtr, "st"),
        (Mnemonic::LdDisp, "ldd"),
        (Mnemonic::StDisp, "std"),
        (Mnemonic::Lpm, "lpm"),
        (Mnemonic::Des, "des"),
        (Mnemonic::Xch, "xch"),
        (Mnemonic::Lac, "lac"),
        (Mnemonic::Las, "las"),
        (Mnemonic::Lat, "lat"),
        (Mnemonic::Lds, "lds"),
        (Mnemonic::Sts, "sts"),
        (Mnemonic::Bset, "bset"),
        (Mnemonic::Bclr, "bclr"),
        (Mnemonic::Cbi, "cbi"),
        (Mnemonic::Sbi, "sbi"),
        (Mnemonic::Sbic, "sbic"),
        (Mnemonic::Sbis, "sbis"),
        (Mnemonic::Pop, "pop"),
        (Mnemonic::Push, "push"),
        (Mnemonic::Com, "com"),
        (Mnemonic::Neg, "neg"),
        (Mnemonic::Swap, "swap"),
        (Mnemonic::Inc, "inc"),
        (Mnemonic::Dec, "dec"),
        (Mnemonic::Asr, "asr"),
        (Mnemonic::Lsr, "lsr"),
        (Mnemonic::Ror, "ror"),
        (Mnemonic::Adiw, "adiw"),
        (Mnemonic::Sbiw, "sbiw"),
        (Mnemonic::Mov, "mov"),
        (Mnemonic::Movw, "movw"),
        (Mnemonic::Add, "add"),
        (Mnemonic::Adc, "adc"),
        (Mnemonic::Sub, "sub"),
        (Mnemonic::Sbc, "sbc"),
        (Mnemonic::Cp, "cp"),
        (Mnemonic::Cpc, "cpc"),
        (Mnemonic::Cpse, "cpse"),
        (Mnemonic::And, "and"),
        (Mnemonic::Or, "or"),
        (Mnemonic::Eor, "eor"),
        (Mnemonic::Cpi, "cpi"),
        (Mnemonic::Sbci, "sbci"),
        (Mnemonic::Subi, "subi"),
        (Mnemonic::Ori, "ori"),
        (Mnemonic::Andi, "andi"),
        (Mnemonic::Ldi, "ldi"),
        (Mnemonic::Rjmp, "rjmp"),
        (Mnemonic::Rcall, "rcall"),
        (Mnemonic::Brbs, "brbs"),
        (Mnemonic::Brbc, "brbc"),
        (Mnemonic::Bld, "bld"),
        (Mnemonic::Bst, "bst"),
        (Mnemonic::Sbrc, "sbrc"),
        (Mnemonic::Sbrs, "sbrs"),
        (Mnemonic::In, "in"),
        (Mnemonic::Out, "out"),
        (Mnemonic::Mul, "mul"),
        (Mnemonic::Muls, "muls"),
        (Mnemonic::Mulsu, "mulsu"),
        (Mnemonic::Fmul, "fmul"),
        (Mnemonic::Fmuls, "fmuls"),
        (Mnemonic::Fmulsu, "fmulsu"),
        (Mnemonic::Unsupported, "unsupported"),
    ];

    for (mnemonic, expected) in cases {
        assert_eq!(mnemonic_name(mnemonic), expected);
    }
}

#[test]
fn debugger_instruction_and_pointer_rendering_are_unambiguous() {
    for (pointer, mode, expected) in [
        (PointerRegister::X, PointerMode::Direct, "X"),
        (PointerRegister::Y, PointerMode::PostIncrement, "Y+"),
        (PointerRegister::Z, PointerMode::PreDecrement, "-Z"),
    ] {
        assert_eq!(format_pointer(pointer, mode), expected);
    }

    let no_operands = DecodedInstruction {
        address: 0x2a,
        opcode: 0x0000,
        next_word: None,
        mnemonic: Mnemonic::Nop,
        word_length: 1,
        operands: OperandSet::default(),
    };
    assert_eq!(
        format_instruction(&no_operands),
        "0x00002A: nop ; opcode=0x0000"
    );

    let immediate = DecodedInstruction {
        address: 0x123,
        opcode: 0xe20f,
        next_word: None,
        mnemonic: Mnemonic::Ldi,
        word_length: 1,
        operands: OperandSet {
            d: Some(16),
            k: Some(0x2f),
            ..OperandSet::default()
        },
    };
    assert_eq!(
        format_instruction(&immediate),
        "0x000123: ldi r16, 0x2F ; opcode=0xE20F"
    );
}

#[test]
fn serial_baud_calculation_matches_normal_and_double_speed_modes() {
    assert_eq!(configured_baud(16_000_000, 0, 103, 0), 9_615);
    assert_eq!(configured_baud(16_000_000, U2X0, 207, 0), 9_615);
    assert_eq!(configured_baud(16_000_000, 0, 0, 0), 1_000_000);
}
