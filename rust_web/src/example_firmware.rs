use crate::runtime::SimulationTarget;

#[derive(Clone, Copy)]
pub struct ExampleFirmware {
    pub label: &'static str,
    pub file_name: &'static str,
    pub target: SimulationTarget,
    pub hex: &'static str,
}

pub const NANO_PIN_SWEEP: ExampleFirmware = ExampleFirmware {
    label: "Nano Pin Sweep",
    file_name: "nano_pin_sweep.hex",
    target: SimulationTarget::Nano,
    hex: include_str!("../example_firmware/nano_pin_sweep.hex"),
};

pub const MEGA_PIN_SWEEP: ExampleFirmware = ExampleFirmware {
    label: "Mega Pin Sweep",
    file_name: "mega_pin_sweep.hex",
    target: SimulationTarget::Mega,
    hex: include_str!("../example_firmware/mega_pin_sweep.hex"),
};

pub const ALL: [ExampleFirmware; 2] = [NANO_PIN_SWEEP, MEGA_PIN_SWEEP];

#[cfg(test)]
mod tests {
    use super::{MEGA_PIN_SWEEP, NANO_PIN_SWEEP};

    #[test]
    fn bundled_examples_look_like_intel_hex() {
        for example in [NANO_PIN_SWEEP, MEGA_PIN_SWEEP] {
            assert!(example.hex.starts_with(':'));
            assert!(example.hex.contains('\n'));
        }
    }
}
