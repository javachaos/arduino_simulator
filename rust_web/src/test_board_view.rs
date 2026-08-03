use std::collections::HashSet;

use rust_mcu::{BoardPin, BoardPinLevel};

use super::{active_pin_count, board_canvas_size, board_slots, pin_level_map, PinSide, PinSlot};
use crate::runtime::SimulationTarget;

fn slot_for(slots: &[PinSlot], pin: BoardPin) -> &PinSlot {
    slots
        .iter()
        .find(|slot| slot.pin == pin)
        .unwrap_or_else(|| panic!("missing slot for {pin:?}"))
}

#[test]
fn board_slot_layouts_cover_every_exposed_pin_once() {
    for (target, expected_digital, expected_analog, expected_total) in [
        (SimulationTarget::Nano, 14, 8, 22),
        (SimulationTarget::Mega, 54, 16, 70),
    ] {
        let slots = board_slots(target);
        assert_eq!(slots.len(), expected_total);

        let unique_pins = slots.iter().map(|slot| slot.pin).collect::<HashSet<_>>();
        let expected_pins = match target {
            SimulationTarget::Nano => (0..=13)
                .map(BoardPin::Digital)
                .chain((0..=7).map(BoardPin::Analog))
                .collect::<HashSet<_>>(),
            SimulationTarget::Mega => (0..=53)
                .map(BoardPin::Digital)
                .chain((0..=15).map(BoardPin::Analog))
                .collect::<HashSet<_>>(),
        };
        assert_eq!(
            unique_pins.len(),
            slots.len(),
            "duplicate pin in {target:?}"
        );
        assert_eq!(unique_pins, expected_pins, "wrong pin set for {target:?}");
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot.pin, BoardPin::Digital(_)))
                .count(),
            expected_digital
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot.pin, BoardPin::Analog(_)))
                .count(),
            expected_analog
        );

        for slot in &slots {
            let expected_label = match slot.pin {
                BoardPin::Digital(index) => format!("D{index}"),
                BoardPin::Analog(index) => format!("A{index}"),
            };
            assert_eq!(slot.label, expected_label);
        }

        let edge_slots = match target {
            SimulationTarget::Nano => [
                (BoardPin::Digital(13), true, 0, 0),
                (BoardPin::Analog(7), true, 8, 0),
                (BoardPin::Digital(12), false, 0, 0),
                (BoardPin::Digital(0), false, 12, 0),
            ],
            SimulationTarget::Mega => [
                (BoardPin::Digital(21), true, 0, 0),
                (BoardPin::Digital(0), true, 21, 0),
                (BoardPin::Analog(15), true, 0, 1),
                (BoardPin::Digital(53), false, 0, 0),
            ],
        };
        for (pin, expected_left, expected_order, expected_group) in edge_slots {
            let slot = slot_for(&slots, pin);
            assert_eq!(matches!(slot.side, PinSide::Left), expected_left);
            assert_eq!(slot.order, expected_order);
            assert_eq!(slot.group, expected_group);
        }
    }
}

#[test]
fn board_canvas_dimensions_keep_large_mega_headers_visible() {
    let nano_slots = board_slots(SimulationTarget::Nano);
    let nano_size = board_canvas_size(SimulationTarget::Nano, &nano_slots);
    assert_eq!((nano_size.x, nano_size.y), (720.0, 560.0));

    let mega_slots = board_slots(SimulationTarget::Mega);
    let mega_size = board_canvas_size(SimulationTarget::Mega, &mega_slots);
    assert_eq!((mega_size.x, mega_size.y), (840.0, 946.0));
    assert!(mega_size.y > nano_size.y);
}

#[test]
fn active_pin_summary_and_lookup_preserve_levels() {
    let levels = [
        BoardPinLevel {
            pin: BoardPin::Digital(2),
            level: 0,
        },
        BoardPinLevel {
            pin: BoardPin::Digital(3),
            level: 1,
        },
        BoardPinLevel {
            pin: BoardPin::Analog(0),
            level: u8::MAX,
        },
    ];
    assert_eq!(active_pin_count(&levels), 2);

    let map = pin_level_map(&levels);
    assert_eq!(map.get(&BoardPin::Digital(2)), Some(&0));
    assert_eq!(map.get(&BoardPin::Digital(3)), Some(&1));
    assert_eq!(map.get(&BoardPin::Analog(0)), Some(&u8::MAX));

    let repeated_pin = [
        BoardPinLevel {
            pin: BoardPin::Digital(3),
            level: 1,
        },
        BoardPinLevel {
            pin: BoardPin::Digital(3),
            level: 0,
        },
    ];
    assert_eq!(
        pin_level_map(&repeated_pin).get(&BoardPin::Digital(3)),
        Some(&0)
    );
}
