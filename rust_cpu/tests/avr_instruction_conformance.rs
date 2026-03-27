use rust_cpu::cpu::{FLAG_C, FLAG_I, FLAG_Z};
use rust_cpu::{Cpu, CpuConfig, CpuError, DataBus, StepOutcome};

fn ldi(d: u8, k: u8) -> u16 {
    0xE000 | (((k as u16) & 0xF0) << 4) | ((((d - 16) as u16) & 0x0F) << 4) | ((k as u16) & 0x0F)
}

fn bset(bit: u8) -> u16 {
    0x9408 | ((bit as u16) << 4)
}

fn ld_x_postinc(d: u8) -> u16 {
    0x900D | ((d as u16) << 4)
}

fn st_x_predec(r: u8) -> u16 {
    0x920E | ((r as u16) << 4)
}

fn elpm_direct(d: u8) -> u16 {
    0x9006 | ((d as u16) << 4)
}

fn elpm_postinc(d: u8) -> u16 {
    0x9007 | ((d as u16) << 4)
}

fn displacement_opcode(is_store: bool, use_y: bool, reg: u8, q: u8) -> u16 {
    assert!(q <= 63);
    let mut opcode = 0x8000 | ((reg as u16) << 4);
    if is_store {
        opcode |= 0x0200;
    }
    if use_y {
        opcode |= 0x0008;
    }
    opcode |= (((q as u16) & 0x20) << 8) | (((q as u16) & 0x18) << 7) | ((q as u16) & 0x07);
    opcode
}

fn ldd_y(d: u8, q: u8) -> u16 {
    displacement_opcode(false, true, d, q)
}

fn std_y(r: u8, q: u8) -> u16 {
    displacement_opcode(true, true, r, q)
}

fn xch(d: u8) -> u16 {
    0x9204 | ((d as u16) << 4)
}

fn las(d: u8) -> u16 {
    0x9205 | ((d as u16) << 4)
}

fn lac(d: u8) -> u16 {
    0x9206 | ((d as u16) << 4)
}

fn lat(d: u8) -> u16 {
    0x9207 | ((d as u16) << 4)
}

fn mulsu(d: u8, r: u8) -> u16 {
    0x0300 | ((((d - 16) as u16) & 0x07) << 4) | (((r - 16) as u16) & 0x07)
}

fn fmul(d: u8, r: u8) -> u16 {
    0x0308 | ((((d - 16) as u16) & 0x07) << 4) | (((r - 16) as u16) & 0x07)
}

fn fmuls(d: u8, r: u8) -> u16 {
    0x0380 | ((((d - 16) as u16) & 0x07) << 4) | (((r - 16) as u16) & 0x07)
}

fn fmulsu(d: u8, r: u8) -> u16 {
    0x0388 | ((((d - 16) as u16) & 0x07) << 4) | (((r - 16) as u16) & 0x07)
}

fn set_x<B: DataBus>(cpu: &mut Cpu<B>, value: u16) {
    cpu.write_register(26, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(27, (value >> 8) as u8).unwrap();
}

fn x_value<B: DataBus>(cpu: &Cpu<B>) -> u16 {
    (cpu.data[26] as u16) | ((cpu.data[27] as u16) << 8)
}

fn set_y<B: DataBus>(cpu: &mut Cpu<B>, value: u16) {
    cpu.write_register(28, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(29, (value >> 8) as u8).unwrap();
}

fn y_value<B: DataBus>(cpu: &Cpu<B>) -> u16 {
    (cpu.data[28] as u16) | ((cpu.data[29] as u16) << 8)
}

fn set_z<B: DataBus>(cpu: &mut Cpu<B>, value: u16) {
    cpu.write_register(30, (value & 0x00FF) as u8).unwrap();
    cpu.write_register(31, (value >> 8) as u8).unwrap();
}

fn z_value<B: DataBus>(cpu: &Cpu<B>) -> u32 {
    (cpu.data[30] as u32) | ((cpu.data[31] as u32) << 8) | ((cpu.data[0x5B] as u32) << 16)
}

fn product_word<B: DataBus>(cpu: &Cpu<B>) -> u16 {
    (cpu.data[0] as u16) | ((cpu.data[1] as u16) << 8)
}

fn run_z_sram_op(opcode: u16, initial_reg: u8, initial_memory: u8) -> (u8, u8) {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    let address = cpu.config.sram_start_address as u16;
    cpu.load_program_words(&[opcode, 0x9598], 0).unwrap();
    cpu.write_register(16, initial_reg).unwrap();
    cpu.write_data(address as usize, initial_memory).unwrap();
    set_z(&mut cpu, address);
    assert_eq!(cpu.run(Some(2)).unwrap(), 2);
    (
        cpu.read_register(16).unwrap(),
        cpu.read_data(address as usize).unwrap(),
    )
}

#[derive(Default)]
struct AlwaysPendingInterruptBus {
    vector: u8,
}

impl DataBus for AlwaysPendingInterruptBus {
    fn pending_interrupt(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _pc: u32,
        _cycles: u64,
    ) -> Option<u8> {
        Some(self.vector)
    }
}

#[test]
fn icall_pushes_return_and_resumes_the_caller() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    cpu.load_program_words(&[0x9509, 0x9598, 0x0000, ldi(16, 0x77), 0x9508], 0)
        .unwrap();
    set_z(&mut cpu, 3);

    assert_eq!(cpu.run(Some(4)).unwrap(), 4);
    assert!(cpu.break_hit);
    assert_eq!(cpu.read_register(16).unwrap(), 0x77);
    assert_eq!(cpu.sp(), cpu.config.stack_reset_value());
}

#[test]
fn eijmp_uses_eind_on_large_devices() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega2560(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    cpu.load_program_words(&[0x9419], 0).unwrap();
    cpu.set_program_word(0x10005, 0x9598).unwrap();
    cpu.write_data(cpu.config.eind_address.unwrap(), 0x01)
        .unwrap();
    set_z(&mut cpu, 0x0005);

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.pc, 0x10005);
    assert_eq!(cpu.step().unwrap(), StepOutcome::BreakHit);
}

#[test]
fn elpm_reads_extended_program_space_and_updates_rampz_when_requested() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega2560(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    cpu.load_program_words(&[0x95D8, elpm_postinc(17), elpm_direct(18), 0x9598], 0)
        .unwrap();
    cpu.load_program_bytes(&[0xAB, 0xCD], 0x10020).unwrap();
    cpu.write_data(cpu.config.rampz_address.unwrap(), 0x01)
        .unwrap();
    set_z(&mut cpu, 0x0020);

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.read_register(0).unwrap(), 0xAB);
    assert_eq!(z_value(&cpu), 0x1_0020);

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.read_register(17).unwrap(), 0xAB);
    assert_eq!(z_value(&cpu), 0x1_0021);

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.read_register(18).unwrap(), 0xCD);
    assert_eq!(z_value(&cpu), 0x1_0021);
}

#[test]
fn ld_and_st_pointer_modes_update_x_correctly() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    let sram_base = cpu.config.sram_start_address;
    cpu.load_program_words(
        &[ld_x_postinc(18), ldi(18, 0xA5), st_x_predec(18), 0x9598],
        0,
    )
    .unwrap();
    cpu.write_data(sram_base, 0x5A).unwrap();
    set_x(&mut cpu, sram_base as u16);

    assert_eq!(cpu.run(Some(4)).unwrap(), 4);
    assert_eq!(cpu.read_register(18).unwrap(), 0xA5);
    assert_eq!(cpu.read_data(sram_base).unwrap(), 0xA5);
    assert_eq!(x_value(&cpu), sram_base as u16);
}

#[test]
fn ldd_and_std_use_displacement_without_mutating_y() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    let sram_base = (cpu.config.sram_start_address + 0x20) as u16;
    cpu.load_program_words(&[ldd_y(19, 5), std_y(19, 6), 0x9598], 0)
        .unwrap();
    cpu.write_data((sram_base + 5) as usize, 0x44).unwrap();
    set_y(&mut cpu, sram_base);

    assert_eq!(cpu.run(Some(3)).unwrap(), 3);
    assert_eq!(cpu.read_register(19).unwrap(), 0x44);
    assert_eq!(cpu.read_data((sram_base + 6) as usize).unwrap(), 0x44);
    assert_eq!(y_value(&cpu), sram_base);
}

#[test]
fn xch_lac_las_and_lat_match_avr_sram_semantics() {
    assert_eq!(run_z_sram_op(xch(16), 0xAA, 0x55), (0x55, 0xAA));
    assert_eq!(run_z_sram_op(lac(16), 0xAA, 0xF0), (0xF0, 0x50));
    assert_eq!(run_z_sram_op(las(16), 0x55, 0x30), (0x30, 0x75));
    assert_eq!(run_z_sram_op(lat(16), 0xAA, 0xCC), (0xCC, 0x66));
}

#[test]
fn xch_rejects_non_sram_addresses() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    cpu.load_program_words(&[xch(16)], 0).unwrap();
    cpu.write_register(16, 0xAA).unwrap();
    set_z(&mut cpu, 0x005F);

    assert_eq!(
        cpu.step().unwrap_err(),
        CpuError::InvalidSramOperation {
            instruction: "xch",
            address: 0x005F,
        }
    );
}

#[test]
fn multiply_variants_produce_expected_products_and_flags() {
    let mut cpu = Cpu::new(
        CpuConfig::atmega328p(),
        AlwaysPendingInterruptBus { vector: 2 },
    );
    cpu.load_program_words(
        &[mulsu(16, 17), fmul(16, 17), fmuls(16, 17), fmulsu(16, 17)],
        0,
    )
    .unwrap();

    cpu.write_register(16, 0xFE).unwrap();
    cpu.write_register(17, 0x03).unwrap();
    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(product_word(&cpu), 0xFFFA);
    assert!(cpu.get_flag(FLAG_C));
    assert!(!cpu.get_flag(FLAG_Z));

    cpu.write_register(16, 0xFF).unwrap();
    cpu.write_register(17, 0xFF).unwrap();
    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(product_word(&cpu), 0xFC02);
    assert!(cpu.get_flag(FLAG_C));
    assert!(!cpu.get_flag(FLAG_Z));

    cpu.write_register(16, 0x80).unwrap();
    cpu.write_register(17, 0x02).unwrap();
    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(product_word(&cpu), 0xFE00);
    assert!(cpu.get_flag(FLAG_C));
    assert!(!cpu.get_flag(FLAG_Z));

    cpu.write_register(16, 0xFF).unwrap();
    cpu.write_register(17, 0x02).unwrap();
    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(product_word(&cpu), 0xFFFC);
    assert!(cpu.get_flag(FLAG_C));
    assert!(!cpu.get_flag(FLAG_Z));
}

#[test]
fn interrupt_enable_and_reti_defer_servicing_by_one_instruction() {
    let bus = AlwaysPendingInterruptBus { vector: 2 };
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);
    cpu.load_program_words(&[bset(FLAG_I), 0x0000, 0x9598, 0x0000, 0x9518], 0)
        .unwrap();

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.pc, 1);
    assert!(cpu.get_flag(FLAG_I));

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.pc, 4);

    assert_eq!(cpu.step().unwrap(), StepOutcome::Executed);
    assert_eq!(cpu.pc, 2);
    assert!(cpu.get_flag(FLAG_I));

    assert_eq!(cpu.step().unwrap(), StepOutcome::BreakHit);
}
