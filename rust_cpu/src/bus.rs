use crate::{config::CpuConfig, instruction::DecodedInstruction};

pub trait DataBus {
    fn reset(&mut self, _config: &CpuConfig, _data: &mut [u8]) {}

    fn read_data(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _cycles: u64,
        _address: usize,
    ) -> Option<u8> {
        None
    }

    fn write_data(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _cycles: u64,
        _address: usize,
        _value: u8,
    ) -> bool {
        false
    }

    fn after_step(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _pc: u32,
        _cycles: u64,
        _instruction: &DecodedInstruction,
        _step_cycles: u8,
    ) {
    }

    fn pending_interrupt(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _pc: u32,
        _cycles: u64,
    ) -> Option<u8> {
        None
    }

    fn on_interrupt(
        &mut self,
        _config: &CpuConfig,
        _data: &mut [u8],
        _pc: u32,
        _cycles: u64,
        _vector_number: u8,
        _latency_cycles: u8,
    ) {
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullBus;

impl DataBus for NullBus {}
