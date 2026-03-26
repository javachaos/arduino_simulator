#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuConfig {
    pub name: &'static str,
    pub program_size_bytes: usize,
    pub data_size_bytes: usize,
    pub sram_start_address: usize,
    pub pc_bits: u8,
    pub sreg_address: usize,
    pub spl_address: usize,
    pub sph_address: usize,
    pub rampd_address: Option<usize>,
    pub rampx_address: Option<usize>,
    pub rampy_address: Option<usize>,
    pub rampz_address: Option<usize>,
    pub eind_address: Option<usize>,
    pub default_sp: Option<u16>,
    pub supports_des: bool,
}

impl CpuConfig {
    pub const fn atmega328p() -> Self {
        Self {
            name: "atmega328p",
            program_size_bytes: 0x8000,
            data_size_bytes: 0x0900,
            sram_start_address: 0x0100,
            pc_bits: 14,
            sreg_address: 0x5F,
            spl_address: 0x5D,
            sph_address: 0x5E,
            rampd_address: Some(0x58),
            rampx_address: Some(0x59),
            rampy_address: Some(0x5A),
            rampz_address: Some(0x5B),
            eind_address: Some(0x5C),
            default_sp: Some(0x08FF),
            supports_des: false,
        }
    }

    pub const fn atmega2560() -> Self {
        Self {
            name: "atmega2560",
            program_size_bytes: 0x40000,
            data_size_bytes: 0x2200,
            sram_start_address: 0x0200,
            pc_bits: 17,
            sreg_address: 0x5F,
            spl_address: 0x5D,
            sph_address: 0x5E,
            rampd_address: Some(0x58),
            rampx_address: Some(0x59),
            rampy_address: Some(0x5A),
            rampz_address: Some(0x5B),
            eind_address: Some(0x5C),
            default_sp: Some(0x21FF),
            supports_des: false,
        }
    }

    pub const fn program_size_words(&self) -> usize {
        self.program_size_bytes / 2
    }

    pub const fn pc_mask(&self) -> u32 {
        (1u32 << self.pc_bits) - 1
    }

    pub const fn return_address_bytes(&self) -> usize {
        if self.pc_bits > 16 {
            3
        } else {
            2
        }
    }

    pub const fn stack_reset_value(&self) -> u16 {
        match self.default_sp {
            Some(value) => value,
            None => (self.data_size_bytes - 1) as u16,
        }
    }
}
