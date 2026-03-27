use rust_cpu::cpu::{FLAG_C, FLAG_H, FLAG_I, FLAG_N, FLAG_S, FLAG_T, FLAG_V, FLAG_Z};
use rust_cpu::{Cpu, CpuConfig, CpuError, NullBus};

fn rr_opcode(base: u16, d: u8, r: u8) -> u16 {
    base | ((d as u16 & 0x1F) << 4) | (r as u16 & 0x0F) | ((r as u16 & 0x10) << 5)
}

fn ldi(d: u8, k: u8) -> u16 {
    assert!((16..=31).contains(&d));
    0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
}

fn add(d: u8, r: u8) -> u16 {
    rr_opcode(0x0C00, d, r)
}

fn adc(d: u8, r: u8) -> u16 {
    rr_opcode(0x1C00, d, r)
}

fn cpse(d: u8, r: u8) -> u16 {
    rr_opcode(0x1000, d, r)
}

fn out(a: u8, r: u8) -> u16 {
    0xB800 | (((a as u16) & 0x30) << 5) | (((r as u16) & 0x1F) << 4) | ((a as u16) & 0x0F)
}

fn input(d: u8, a: u8) -> u16 {
    0xB000 | (((a as u16) & 0x30) << 5) | (((d as u16) & 0x1F) << 4) | ((a as u16) & 0x0F)
}

fn sbi(a: u8, b: u8) -> u16 {
    0x9A00 | (((a as u16) & 0x1F) << 3) | ((b as u16) & 0x07)
}

fn sbic(a: u8, b: u8) -> u16 {
    0x9900 | (((a as u16) & 0x1F) << 3) | ((b as u16) & 0x07)
}

fn sbis(a: u8, b: u8) -> u16 {
    0x9B00 | (((a as u16) & 0x1F) << 3) | ((b as u16) & 0x07)
}

fn push(r: u8) -> u16 {
    0x920F | (((r as u16) & 0x1F) << 4)
}

fn pop(d: u8) -> u16 {
    0x900F | (((d as u16) & 0x1F) << 4)
}

fn rcall(offset: i16) -> u16 {
    0xD000 | ((offset as u16) & 0x0FFF)
}

fn ret() -> u16 {
    0x9508
}

fn brk() -> u16 {
    0x9598
}

fn jmp(target: u32) -> (u16, u16) {
    let first = 0x940C | (((target >> 17) as u16 & 0x1F) << 4) | (((target >> 16) as u16) & 0x01);
    let second = (target & 0xFFFF) as u16;
    (first, second)
}

fn call(target: u32) -> (u16, u16) {
    let first = 0x940E | (((target >> 17) as u16 & 0x1F) << 4) | (((target >> 16) as u16) & 0x01);
    let second = (target & 0xFFFF) as u16;
    (first, second)
}

fn st_x_postinc(r: u8) -> u16 {
    0x920D | (((r as u16) & 0x1F) << 4)
}

fn ld_x_postinc(d: u8) -> u16 {
    0x900D | (((d as u16) & 0x1F) << 4)
}

fn std_y(q: u8, r: u8) -> u16 {
    0x8208
        | (((q as u16) & 0x20) << 8)
        | (((q as u16) & 0x18) << 7)
        | (((r as u16) & 0x1F) << 4)
        | ((q as u16) & 0x07)
}

fn ldd_y(q: u8, d: u8) -> u16 {
    0x8008
        | (((q as u16) & 0x20) << 8)
        | (((q as u16) & 0x18) << 7)
        | (((d as u16) & 0x1F) << 4)
        | ((q as u16) & 0x07)
}

fn lpm(d: u8, post_increment: bool) -> u16 {
    (if post_increment { 0x9005 } else { 0x9004 }) | (((d as u16) & 0x1F) << 4)
}

fn elpm(d: u8, post_increment: bool) -> u16 {
    (if post_increment { 0x9007 } else { 0x9006 }) | (((d as u16) & 0x1F) << 4)
}

fn bset(bit_index: u8) -> u16 {
    0x9408 | (((bit_index as u16) & 0x07) << 4)
}

fn bclr(bit_index: u8) -> u16 {
    0x9488 | (((bit_index as u16) & 0x07) << 4)
}

fn bst(d: u8, bit_index: u8) -> u16 {
    0xFA00 | (((d as u16) & 0x1F) << 4) | ((bit_index as u16) & 0x07)
}

fn bld(d: u8, bit_index: u8) -> u16 {
    0xF800 | (((d as u16) & 0x1F) << 4) | ((bit_index as u16) & 0x07)
}

fn mul(d: u8, r: u8) -> u16 {
    rr_opcode(0x9C00, d, r)
}

fn fmul(d: u8, r: u8) -> u16 {
    0x0308 | ((((d - 16) as u16) & 0x07) << 4) | (((r - 16) as u16) & 0x07)
}

fn des(round_index: u8) -> u16 {
    0x940B | (((round_index as u16) & 0x0F) << 4)
}

fn assert_flag(cpu: &Cpu<NullBus>, flag: u8, expected: bool) {
    assert_eq!(cpu.get_flag(flag), expected);
}

#[test]
fn add_and_adc_flags_follow_reference_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x7F),
            ldi(17, 0x01),
            add(16, 17),
            ldi(18, 0xFF),
            ldi(19, 0x00),
            bset(FLAG_C),
            adc(18, 19),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(16).unwrap(), 0x80);
    assert_flag(&cpu, FLAG_H, true);
    assert_flag(&cpu, FLAG_V, false);
    assert_flag(&cpu, FLAG_N, false);
    assert_flag(&cpu, FLAG_S, false);
    assert_flag(&cpu, FLAG_Z, true);
    assert_flag(&cpu, FLAG_C, true);
    assert_eq!(cpu.read_register(18).unwrap(), 0x00);
}

#[test]
fn stack_rcall_push_pop_and_ret_round_trip() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x11),
            rcall(3),
            brk(),
            0x0000,
            0x0000,
            push(16),
            ldi(16, 0x22),
            pop(17),
            ret(),
        ],
        0,
    )
    .unwrap();
    let initial_sp = cpu.sp();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(16).unwrap(), 0x22);
    assert_eq!(cpu.read_register(17).unwrap(), 0x11);
    assert_eq!(cpu.sp(), initial_sp);
    assert!(cpu.break_hit);
}

#[test]
fn three_byte_pc_call_and_ret_match_mega2560_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), NullBus);
    let target = 0x10010u32;
    let (call0, call1) = call(target);
    cpu.load_program_words(&[call0, call1, brk()], 0).unwrap();
    cpu.load_program_words(&[ldi(16, 0x33), ret()], target as usize)
        .unwrap();
    let initial_sp = cpu.sp();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(16).unwrap(), 0x33);
    assert_eq!(cpu.sp(), initial_sp);
    assert!(cpu.break_hit);
}

#[test]
fn cpse_skips_two_word_instruction() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    let (jmp0, jmp1) = jmp(8);
    cpu.load_program_words(
        &[
            ldi(16, 0x42),
            ldi(17, 0x42),
            cpse(16, 17),
            jmp0,
            jmp1,
            ldi(18, 0x55),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(18).unwrap(), 0x55);
    assert!(cpu.break_hit);
}

#[test]
fn x_and_y_addressing_modes_match_reference_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(26, 0x00),
            ldi(27, 0x01),
            ldi(16, 0x12),
            ldi(17, 0x34),
            st_x_postinc(16),
            st_x_postinc(17),
            ldi(28, 0x00),
            ldi(29, 0x01),
            std_y(2, 17),
            ldi(26, 0x00),
            ldi(27, 0x01),
            ld_x_postinc(18),
            ld_x_postinc(19),
            ldd_y(2, 20),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_data(0x0100).unwrap(), 0x12);
    assert_eq!(cpu.read_data(0x0101).unwrap(), 0x34);
    assert_eq!(cpu.read_data(0x0102).unwrap(), 0x34);
    assert_eq!(cpu.read_register(18).unwrap(), 0x12);
    assert_eq!(cpu.read_register(19).unwrap(), 0x34);
    assert_eq!(cpu.read_register(20).unwrap(), 0x34);
}

#[test]
fn in_out_and_skip_bit_in_io_register_match_reference_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x00),
            out(0x05, 16),
            sbi(0x05, 2),
            sbic(0x05, 2),
            ldi(17, 0xAA),
            sbis(0x05, 2),
            ldi(18, 0xBB),
            input(19, 0x05),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(17).unwrap(), 0xAA);
    assert_eq!(cpu.read_register(18).unwrap(), 0x00);
    assert_eq!(cpu.read_register(19).unwrap(), 0x04);
}

#[test]
fn lpm_and_elpm_read_program_memory_bytes() {
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), NullBus);
    cpu.load_program_words(
        &[
            ldi(30, 0x00),
            ldi(31, 0x02),
            lpm(16, false),
            lpm(17, true),
            ldi(30, 0x00),
            ldi(31, 0x00),
            ldi(20, 0x01),
            out(0x3B, 20),
            elpm(18, false),
            brk(),
        ],
        0,
    )
    .unwrap();
    cpu.set_program_word(0x0100, 0xBBAA).unwrap();
    cpu.program[0x10000] = 0x5C;

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(16).unwrap(), 0xAA);
    assert_eq!(cpu.read_register(17).unwrap(), 0xAA);
    assert_eq!(cpu.read_register(18).unwrap(), 0x5C);
}

#[test]
fn flag_bit_instructions_and_bit_transfer_match_reference_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            bset(FLAG_I),
            bset(FLAG_T),
            ldi(16, 0x00),
            bld(16, 3),
            bst(16, 3),
            bclr(FLAG_I),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(16).unwrap(), 0x08);
    assert_flag(&cpu, FLAG_T, true);
    assert_flag(&cpu, FLAG_I, false);
}

#[test]
fn mul_and_fmul_write_r1_r0_and_flags_match_reference_behavior() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x03),
            ldi(17, 0x07),
            mul(16, 17),
            ldi(18, 0x40),
            ldi(19, 0x20),
            fmul(18, 19),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.run(None).unwrap();

    assert_eq!(cpu.read_register(0).unwrap(), 0x00);
    assert_eq!(cpu.read_register(1).unwrap(), 0x10);
    assert_flag(&cpu, FLAG_Z, false);
    assert_flag(&cpu, FLAG_C, false);
}

#[test]
fn des_decodes_and_fails_explicitly() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(&[des(0x03)], 0).unwrap();
    let instruction = cpu.decode_at(0).unwrap();

    assert_eq!(instruction.mnemonic, rust_cpu::Mnemonic::Des);
    assert_eq!(
        cpu.step().unwrap_err(),
        CpuError::InstructionUnavailable {
            instruction: "des",
            device: "atmega328p",
        }
    );
}
