use rust_cpu::cpu::{FLAG_C, FLAG_H, FLAG_N, FLAG_S, FLAG_V, FLAG_Z};
use rust_cpu::{Cpu, CpuConfig, NullBus, StepOutcome};

// Spec references:
// - ATmega328P datasheet, section 31 "Instruction Set Summary", pages 281-284.
// - ATmega640/1280/1281/2560/2561 datasheet, section 34 "Instruction Set Summary", pages 404-406.
// - AVR Instruction Set Manual (Microchip DS40002198), section 35 "Instruction Set Summary",
//   pages 387-390.
fn rr_opcode(base: u16, d: u8, r: u8) -> u16 {
    base | ((d as u16 & 0x1F) << 4) | (r as u16 & 0x0F) | ((r as u16 & 0x10) << 5)
}

fn imm_opcode(base: u16, d: u8, k: u8) -> u16 {
    assert!((16..=31).contains(&d));
    base | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
}

fn unary_opcode(base: u16, d: u8) -> u16 {
    base | (((d as u16) & 0x1F) << 4)
}

fn ldi(d: u8, k: u8) -> u16 {
    imm_opcode(0xE000, d, k)
}

fn mov(d: u8, r: u8) -> u16 {
    rr_opcode(0x2C00, d, r)
}

fn movw(d: u8, r: u8) -> u16 {
    assert_eq!(d % 2, 0);
    assert_eq!(r % 2, 0);
    0x0100 | ((((d / 2) as u16) & 0x0F) << 4) | (((r / 2) as u16) & 0x0F)
}

fn and_op(d: u8, r: u8) -> u16 {
    rr_opcode(0x2000, d, r)
}

fn sub(d: u8, r: u8) -> u16 {
    rr_opcode(0x1800, d, r)
}

fn sbc(d: u8, r: u8) -> u16 {
    rr_opcode(0x0800, d, r)
}

fn cp(d: u8, r: u8) -> u16 {
    rr_opcode(0x1400, d, r)
}

fn cpc(d: u8, r: u8) -> u16 {
    rr_opcode(0x0400, d, r)
}

fn or_op(d: u8, r: u8) -> u16 {
    rr_opcode(0x2800, d, r)
}

fn eor(d: u8, r: u8) -> u16 {
    rr_opcode(0x2400, d, r)
}

fn andi(d: u8, k: u8) -> u16 {
    imm_opcode(0x7000, d, k)
}

fn cpi(d: u8, k: u8) -> u16 {
    imm_opcode(0x3000, d, k)
}

fn sbci(d: u8, k: u8) -> u16 {
    imm_opcode(0x4000, d, k)
}

fn subi(d: u8, k: u8) -> u16 {
    imm_opcode(0x5000, d, k)
}

fn ori(d: u8, k: u8) -> u16 {
    imm_opcode(0x6000, d, k)
}

fn com(d: u8) -> u16 {
    unary_opcode(0x9400, d)
}

fn neg(d: u8) -> u16 {
    unary_opcode(0x9401, d)
}

fn asr(d: u8) -> u16 {
    unary_opcode(0x9405, d)
}

fn lsr(d: u8) -> u16 {
    unary_opcode(0x9406, d)
}

fn ror(d: u8) -> u16 {
    unary_opcode(0x9407, d)
}

fn inc(d: u8) -> u16 {
    unary_opcode(0x9403, d)
}

fn dec(d: u8) -> u16 {
    unary_opcode(0x940A, d)
}

fn swap(d: u8) -> u16 {
    unary_opcode(0x9402, d)
}

fn adiw(d: u8, k: u8) -> u16 {
    assert!(matches!(d, 24 | 26 | 28 | 30));
    0x9600 | (((k as u16) & 0x30) << 2) | ((((d - 24) / 2) as u16) << 4) | ((k as u16) & 0x0F)
}

fn sbiw(d: u8, k: u8) -> u16 {
    assert!(matches!(d, 24 | 26 | 28 | 30));
    0x9700 | (((k as u16) & 0x30) << 2) | ((((d - 24) / 2) as u16) << 4) | ((k as u16) & 0x0F)
}

fn sbrc(r: u8, bit: u8) -> u16 {
    0xFC00 | (((r as u16) & 0x1F) << 4) | ((bit as u16) & 0x07)
}

fn sbrs(r: u8, bit: u8) -> u16 {
    0xFE00 | (((r as u16) & 0x1F) << 4) | ((bit as u16) & 0x07)
}

fn brbs(flag: u8, offset: i8) -> u16 {
    0xF000 | (((offset as u16) & 0x7F) << 3) | ((flag as u16) & 0x07)
}

fn brbc(flag: u8, offset: i8) -> u16 {
    0xF400 | (((offset as u16) & 0x7F) << 3) | ((flag as u16) & 0x07)
}

fn rjmp(offset: i16) -> u16 {
    0xC000 | ((offset as u16) & 0x0FFF)
}

fn jmp(target: u32) -> (u16, u16) {
    let first = 0x940C | (((target >> 17) as u16 & 0x1F) << 4) | (((target >> 16) as u16) & 0x01);
    let second = (target & 0xFFFF) as u16;
    (first, second)
}

fn ld_x(d: u8) -> u16 {
    0x900C | (((d as u16) & 0x1F) << 4)
}

fn ld_x_predec(d: u8) -> u16 {
    0x900E | (((d as u16) & 0x1F) << 4)
}

fn ld_y_postinc(d: u8) -> u16 {
    0x9009 | (((d as u16) & 0x1F) << 4)
}

fn ld_y_predec(d: u8) -> u16 {
    0x900A | (((d as u16) & 0x1F) << 4)
}

fn ld_z_postinc(d: u8) -> u16 {
    0x9001 | (((d as u16) & 0x1F) << 4)
}

fn ld_z_predec(d: u8) -> u16 {
    0x9002 | (((d as u16) & 0x1F) << 4)
}

fn st_x(r: u8) -> u16 {
    0x920C | (((r as u16) & 0x1F) << 4)
}

fn st_y_postinc(r: u8) -> u16 {
    0x9209 | (((r as u16) & 0x1F) << 4)
}

fn st_y_predec(r: u8) -> u16 {
    0x920A | (((r as u16) & 0x1F) << 4)
}

fn st_z_postinc(r: u8) -> u16 {
    0x9201 | (((r as u16) & 0x1F) << 4)
}

fn st_z_predec(r: u8) -> u16 {
    0x9202 | (((r as u16) & 0x1F) << 4)
}

fn out(a: u8, r: u8) -> u16 {
    0xB800 | (((a as u16) & 0x30) << 5) | (((r as u16) & 0x1F) << 4) | ((a as u16) & 0x0F)
}

fn cbi(a: u8, b: u8) -> u16 {
    0x9800 | (((a as u16) & 0x1F) << 3) | ((b as u16) & 0x07)
}

fn lds(d: u8, address: u16) -> (u16, u16) {
    (0x9000 | (((d as u16) & 0x1F) << 4), address)
}

fn sts(address: u16, r: u8) -> (u16, u16) {
    (0x9200 | (((r as u16) & 0x1F) << 4), address)
}

fn bset(bit_index: u8) -> u16 {
    0x9408 | (((bit_index as u16) & 0x07) << 4)
}

fn bclr(bit_index: u8) -> u16 {
    0x9488 | (((bit_index as u16) & 0x07) << 4)
}

fn ijmp() -> u16 {
    0x9409
}

fn eicall() -> u16 {
    0x9519
}

fn lpm_r0() -> u16 {
    0x95C8
}

fn muls(d: u8, r: u8) -> u16 {
    assert!((16..=31).contains(&d));
    assert!((16..=31).contains(&r));
    0x0200 | ((((d - 16) as u16) & 0x0F) << 4) | (((r - 16) as u16) & 0x0F)
}

fn ret() -> u16 {
    0x9508
}

fn wdr() -> u16 {
    0x95A8
}

fn nop() -> u16 {
    0x0000
}

fn brk() -> u16 {
    0x9598
}

fn pair(cpu: &Cpu<NullBus>, low_register: usize) -> u16 {
    cpu.read_register(low_register).unwrap() as u16
        | ((cpu.read_register(low_register + 1).unwrap() as u16) << 8)
}

fn set_z(cpu: &mut Cpu<NullBus>, value: u16) {
    cpu.write_register(30, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(31, (value >> 8) as u8).unwrap();
}

fn set_x(cpu: &mut Cpu<NullBus>, value: u16) {
    cpu.write_register(26, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(27, (value >> 8) as u8).unwrap();
}

fn set_y(cpu: &mut Cpu<NullBus>, value: u16) {
    cpu.write_register(28, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(29, (value >> 8) as u8).unwrap();
}

fn set_eind(cpu: &mut Cpu<NullBus>, value: u8) {
    let address = cpu.config.eind_address.unwrap();
    cpu.write_data(address, value).unwrap();
}

fn step_with_cycles(cpu: &mut Cpu<NullBus>, expected: StepOutcome, delta_cycles: u64) {
    let before = cpu.cycles;
    assert_eq!(cpu.step().unwrap(), expected);
    assert_eq!(cpu.cycles - before, delta_cycles);
}

fn assert_flag(cpu: &Cpu<NullBus>, flag: u8, expected: bool) {
    assert_eq!(cpu.get_flag(flag), expected);
}

#[test]
fn mov_and_movw_copy_values_without_touching_sreg() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            bset(FLAG_C),
            bset(FLAG_Z),
            ldi(16, 0xA5),
            mov(18, 16),
            ldi(20, 0x12),
            ldi(21, 0x34),
            movw(24, 20),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(18).unwrap(), 0xA5);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(pair(&cpu, 24), 0x3412);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_Z, true);
}

#[test]
fn logic_instructions_follow_the_documented_register_operations() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x0F),
            ldi(17, 0xF0),
            and_op(16, 17),
            or_op(16, 17),
            eor(16, 17),
            andi(17, 0x0F),
            ori(17, 0x80),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);
    assert_flag(&cpu, FLAG_N, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0xF0);
    assert_flag(&cpu, FLAG_Z, false);
    assert_flag(&cpu, FLAG_N, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(17).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(17).unwrap(), 0x80);
    assert_flag(&cpu, FLAG_Z, false);
    assert_flag(&cpu, FLAG_N, true);
}

#[test]
fn subtract_and_compare_instructions_follow_the_documented_operations() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x01),
            ldi(17, 0x01),
            ldi(18, 0x01),
            ldi(19, 0x01),
            sub(16, 17),
            sbc(18, 19),
            ldi(20, 0x10),
            subi(20, 0x01),
            bset(FLAG_C),
            ldi(21, 0x00),
            sbci(21, 0x00),
            ldi(22, 0x33),
            ldi(23, 0x33),
            cp(22, 23),
            cpc(22, 23),
            ldi(24, 0x44),
            cpi(24, 0x44),
            brk(),
        ],
        0,
    )
    .unwrap();

    for _ in 0..4 {
        step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    }

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);
    assert_flag(&cpu, FLAG_C, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(18).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);
    assert_flag(&cpu, FLAG_C, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(20).unwrap(), 0x0F);
    assert_flag(&cpu, FLAG_H, true);
    assert_flag(&cpu, FLAG_Z, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(21).unwrap(), 0xFF);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_N, true);
    assert_flag(&cpu, FLAG_Z, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(22).unwrap(), 0x33);
    assert_eq!(cpu.read_register(23).unwrap(), 0x33);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(24).unwrap(), 0x44);
    assert_flag(&cpu, FLAG_Z, true);
}

#[test]
fn shift_and_swap_instructions_follow_the_summary_bit_moves() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x81),
            asr(16),
            ldi(17, 0x01),
            lsr(17),
            bset(FLAG_C),
            ldi(18, 0x02),
            ror(18),
            ldi(19, 0xA5),
            swap(19),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0xC0);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(17).unwrap(), 0x00);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(18).unwrap(), 0x81);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(19).unwrap(), 0x5A);
}

#[test]
fn unary_arithmetic_instructions_follow_the_summary_results_and_flags() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(16, 0x00),
            com(16),
            ldi(17, 0x01),
            neg(17),
            ldi(18, 0x7F),
            inc(18),
            ldi(19, 0x80),
            dec(19),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0xFF);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_N, true);
    assert_flag(&cpu, FLAG_Z, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(17).unwrap(), 0xFF);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_H, true);
    assert_flag(&cpu, FLAG_N, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(18).unwrap(), 0x80);
    assert_flag(&cpu, FLAG_V, true);
    assert_flag(&cpu, FLAG_N, true);
    assert_flag(&cpu, FLAG_S, false);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(19).unwrap(), 0x7F);
    assert_flag(&cpu, FLAG_V, true);
    assert_flag(&cpu, FLAG_N, false);
    assert_flag(&cpu, FLAG_S, true);
}

#[test]
fn adiw_and_sbiw_use_two_cycles_and_update_word_pairs() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            ldi(24, 0xFF),
            ldi(25, 0xFF),
            adiw(24, 1),
            ldi(30, 0x01),
            ldi(31, 0x00),
            sbiw(30, 1),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(pair(&cpu, 24), 0x0000);
    assert_flag(&cpu, FLAG_Z, true);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(pair(&cpu, 30), 0x0000);
    assert_flag(&cpu, FLAG_Z, true);
}

#[test]
fn indirect_pointer_modes_follow_the_documented_address_updates() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    let base = (cpu.config.sram_start_address + 0x20) as u16;
    cpu.load_program_words(
        &[
            ld_x(16),
            ld_x_predec(17),
            ld_y_postinc(18),
            ld_y_predec(19),
            ld_z_postinc(20),
            ld_z_predec(21),
            st_x(6),
            st_y_postinc(7),
            st_y_predec(8),
            st_z_postinc(9),
            st_z_predec(10),
            brk(),
        ],
        0,
    )
    .unwrap();

    cpu.write_data(base as usize, 0x11).unwrap();
    set_x(&mut cpu, base);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(16).unwrap(), 0x11);
    assert_eq!(pair(&cpu, 26), base);

    cpu.write_data((base + 1) as usize, 0x12).unwrap();
    set_x(&mut cpu, base + 2);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(17).unwrap(), 0x12);
    assert_eq!(pair(&cpu, 26), base + 1);

    cpu.write_data((base + 4) as usize, 0x21).unwrap();
    set_y(&mut cpu, base + 4);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(18).unwrap(), 0x21);
    assert_eq!(pair(&cpu, 28), base + 5);

    cpu.write_data((base + 6) as usize, 0x22).unwrap();
    set_y(&mut cpu, base + 7);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(19).unwrap(), 0x22);
    assert_eq!(pair(&cpu, 28), base + 6);

    cpu.write_data((base + 8) as usize, 0x31).unwrap();
    set_z(&mut cpu, base + 8);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(20).unwrap(), 0x31);
    assert_eq!(pair(&cpu, 30), base + 9);

    cpu.write_data((base + 10) as usize, 0x32).unwrap();
    set_z(&mut cpu, base + 11);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(21).unwrap(), 0x32);
    assert_eq!(pair(&cpu, 30), base + 10);

    cpu.write_register(6, 0x41).unwrap();
    set_x(&mut cpu, base + 12);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data((base + 12) as usize).unwrap(), 0x41);
    assert_eq!(pair(&cpu, 26), base + 12);

    cpu.write_register(7, 0x42).unwrap();
    set_y(&mut cpu, base + 13);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data((base + 13) as usize).unwrap(), 0x42);
    assert_eq!(pair(&cpu, 28), base + 14);

    cpu.write_register(8, 0x43).unwrap();
    set_y(&mut cpu, base + 15);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data((base + 14) as usize).unwrap(), 0x43);
    assert_eq!(pair(&cpu, 28), base + 14);

    cpu.write_register(9, 0x44).unwrap();
    set_z(&mut cpu, base + 16);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data((base + 16) as usize).unwrap(), 0x44);
    assert_eq!(pair(&cpu, 30), base + 17);

    cpu.write_register(10, 0x45).unwrap();
    set_z(&mut cpu, base + 18);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data((base + 17) as usize).unwrap(), 0x45);
    assert_eq!(pair(&cpu, 30), base + 17);
}

#[test]
fn sbrc_and_sbrs_match_the_documented_skip_lengths_and_cycles() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    let (jump0, jump1) = jmp(40);
    cpu.load_program_words(
        &[
            ldi(16, 0x01),
            sbrc(16, 0),
            ldi(17, 0x11),
            ldi(16, 0x00),
            sbrc(16, 0),
            ldi(18, 0x22),
            ldi(16, 0x01),
            sbrs(16, 0),
            jump0,
            jump1,
            ldi(19, 0x33),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.pc, 2);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(17).unwrap(), 0x11);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.pc, 6);
    assert_eq!(cpu.read_register(18).unwrap(), 0x00);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 3);
    assert_eq!(cpu.pc, 10);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(19).unwrap(), 0x33);
}

#[test]
fn brbs_brbc_and_rjmp_follow_relative_pc_updates_from_the_summary() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(
        &[
            bclr(FLAG_C),
            brbs(FLAG_C, 1),
            ldi(16, 0x16),
            brbc(FLAG_C, 1),
            ldi(17, 0x17),
            rjmp(1),
            ldi(18, 0x18),
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.pc, 2);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x16);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.pc, 5);
    assert_eq!(cpu.read_register(17).unwrap(), 0x00);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.pc, 7);
    assert_eq!(cpu.read_register(18).unwrap(), 0x00);
}

#[test]
fn jmp_lpm_and_muls_follow_the_documented_targets_cycles_and_results() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    let (jump0, jump1) = jmp(4);
    cpu.load_program_words(
        &[
            lpm_r0(),
            jump0,
            jump1,
            ldi(16, 0x11),
            ldi(16, 0x44),
            ldi(17, 0xFE),
            ldi(18, 0x03),
            muls(17, 18),
            brk(),
        ],
        0,
    )
    .unwrap();
    cpu.set_program_word(0x0100, 0xBBAA).unwrap();
    set_z(&mut cpu, 0x0200);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 3);
    assert_eq!(cpu.read_register(0).unwrap(), 0xAA);
    assert_eq!(pair(&cpu, 30), 0x0200);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 3);
    assert_eq!(cpu.pc, 4);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x44);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(pair(&cpu, 0), 0xFFFA);
    assert_flag(&cpu, FLAG_C, true);
    assert_flag(&cpu, FLAG_Z, false);
}

#[test]
fn cbi_lds_and_sts_follow_the_documented_data_space_operations() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    let (sts0, sts1) = sts(0x0104, 17);
    let (lds0, lds1) = lds(18, 0x0104);
    cpu.load_program_words(
        &[
            ldi(16, 0xFF),
            out(0x05, 16),
            cbi(0x05, 1),
            ldi(17, 0xAB),
            sts0,
            sts1,
            lds0,
            lds1,
            brk(),
        ],
        0,
    )
    .unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_io(0x05).unwrap(), 0xFD);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_data(0x0104).unwrap(), 0xAB);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.read_register(18).unwrap(), 0xAB);
}

#[test]
fn ijmp_uses_z_and_takes_two_cycles_per_the_summary() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(&[ijmp(), nop(), nop(), ldi(16, 0x44), brk()], 0)
        .unwrap();
    set_z(&mut cpu, 3);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 2);
    assert_eq!(cpu.pc, 3);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x44);
}

#[test]
fn eicall_uses_eind_colon_z_and_the_documented_timing() {
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), NullBus);
    cpu.load_program_words(&[eicall(), brk()], 0).unwrap();
    cpu.load_program_words(&[ldi(16, 0x66), ret()], 0x10005)
        .unwrap();
    set_z(&mut cpu, 0x0005);
    set_eind(&mut cpu, 0x01);
    let initial_sp = cpu.sp();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 4);
    assert_eq!(cpu.pc, 0x10005);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.read_register(16).unwrap(), 0x66);

    step_with_cycles(&mut cpu, StepOutcome::Executed, 5);
    assert_eq!(cpu.pc, 1);
    assert_eq!(cpu.sp(), initial_sp);
}

#[test]
fn wdr_matches_the_documented_single_cycle_execution() {
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
    cpu.load_program_words(&[wdr(), brk()], 0).unwrap();

    step_with_cycles(&mut cpu, StepOutcome::Executed, 1);
    assert_eq!(cpu.pc, 1);
    assert!(!cpu.break_hit);
    assert!(!cpu.sleeping);
}
