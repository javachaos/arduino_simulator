use rust_project::{AvrSimDocument, BehaviorDefinition, ProjectError};

use crate::BehaviorError;

struct BuiltInBehaviorAssetRef {
    name: &'static str,
    board_model: &'static str,
    json: &'static str,
}

const BUILT_IN_BEHAVIOR_ASSETS: &[BuiltInBehaviorAssetRef] = &[
    BuiltInBehaviorAssetRef {
        name: "gy_sht31_d_behavior",
        board_model: "gy_sht31_d",
        json: include_str!("../builtins/gy_sht31_d_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "lc_lm358_pwm_to_0_10v_behavior",
        board_model: "lc_lm358_pwm_to_0_10v",
        json: include_str!("../builtins/lc_lm358_pwm_to_0_10v_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "max31865_breakout_behavior",
        board_model: "max31865_breakout",
        json: include_str!("../builtins/max31865_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "mcp2515_tja1050_can_module_behavior",
        board_model: "mcp2515_tja1050_can_module",
        json: include_str!("../builtins/mcp2515_tja1050_can_module_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "aht20_breakout_behavior",
        board_model: "aht20_breakout",
        json: include_str!("../builtins/aht20_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "ads1115_breakout_behavior",
        board_model: "ads1115_breakout",
        json: include_str!("../builtins/ads1115_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "bh1750_breakout_behavior",
        board_model: "bh1750_breakout",
        json: include_str!("../builtins/bh1750_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "bme280_breakout_behavior",
        board_model: "bme280_breakout",
        json: include_str!("../builtins/bme280_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "bmp280_breakout_behavior",
        board_model: "bmp280_breakout",
        json: include_str!("../builtins/bmp280_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "ina219_breakout_behavior",
        board_model: "ina219_breakout",
        json: include_str!("../builtins/ina219_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "max31855_breakout_behavior",
        board_model: "max31855_breakout",
        json: include_str!("../builtins/max31855_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "max6675_breakout_behavior",
        board_model: "max6675_breakout",
        json: include_str!("../builtins/max6675_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "mpu6050_breakout_behavior",
        board_model: "mpu6050_breakout",
        json: include_str!("../builtins/mpu6050_breakout_behavior.behavior.avrsim.json"),
    },
    BuiltInBehaviorAssetRef {
        name: "vl53l0x_breakout_behavior",
        board_model: "vl53l0x_breakout",
        json: include_str!("../builtins/vl53l0x_breakout_behavior.behavior.avrsim.json"),
    },
];

pub fn built_in_behavior_names() -> Vec<&'static str> {
    BUILT_IN_BEHAVIOR_ASSETS
        .iter()
        .map(|asset| asset.name)
        .collect()
}

pub fn suggested_builtin_behavior_for_board_model(board_name: &str) -> Option<&'static str> {
    BUILT_IN_BEHAVIOR_ASSETS
        .iter()
        .find(|asset| asset.board_model == board_name)
        .map(|asset| asset.name)
}

pub fn load_built_in_behavior_definition(name: &str) -> Result<BehaviorDefinition, BehaviorError> {
    let asset = BUILT_IN_BEHAVIOR_ASSETS
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| BehaviorError::UnknownBuiltInBehavior(name.to_string()))?;
    let document =
        serde_json::from_str::<AvrSimDocument>(asset.json).map_err(ProjectError::from)?;
    let definition = match document {
        AvrSimDocument::BehaviorDefinition(definition) => definition,
        other => {
            return Err(BehaviorError::Project(
                ProjectError::UnexpectedDocumentKind(other.kind_name().to_string()),
            ));
        }
    };
    if definition.name != asset.name {
        return Err(BehaviorError::Project(
            ProjectError::InvalidDefinitionReference(format!(
                "built-in behavior asset {:?} declared mismatched behavior name {:?}",
                asset.name, definition.name
            )),
        ));
    }
    definition.validate()?;
    Ok(definition)
}
