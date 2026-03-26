use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    ProgramBounds,
    DataBounds,
    InvalidRegister,
    Sleeping,
    InvalidSramOperation {
        instruction: &'static str,
        address: u32,
    },
    ExtendedPcRequired(&'static str),
    InstructionUnavailable {
        instruction: &'static str,
        device: &'static str,
    },
    UnsupportedInstruction {
        opcode: u16,
        address: u32,
    },
    UnsupportedMnemonic(&'static str),
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpuError::ProgramBounds => write!(f, "program address is out of range"),
            CpuError::DataBounds => write!(f, "data address is out of range"),
            CpuError::InvalidRegister => write!(f, "register index is out of range"),
            CpuError::Sleeping => write!(f, "CPU is sleeping"),
            CpuError::InvalidSramOperation {
                instruction,
                address,
            } => {
                write!(
                    f,
                    "{instruction} requires an internal SRAM address, got 0x{address:04X}"
                )
            }
            CpuError::ExtendedPcRequired(name) => {
                write!(f, "{name} requires a device with a 22-bit PC")
            }
            CpuError::InstructionUnavailable {
                instruction,
                device,
            } => {
                write!(f, "{instruction} is not available on {device}")
            }
            CpuError::UnsupportedInstruction { opcode, address } => {
                write!(
                    f,
                    "unsupported opcode 0x{opcode:04X} at word address {address}"
                )
            }
            CpuError::UnsupportedMnemonic(name) => {
                write!(f, "unsupported mnemonic {name}")
            }
        }
    }
}

impl std::error::Error for CpuError {}
