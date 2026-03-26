use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

pub const DSL_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Position {
    pub x_mm: f64,
    pub y_mm: f64,
    pub rotation_deg: Option<f64>,
}

impl Position {
    pub fn new(x_mm: f64, y_mm: f64, rotation_deg: Option<f64>) -> Self {
        Self {
            x_mm,
            y_mm,
            rotation_deg,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    pub number: String,
    pub pad_type: String,
    pub shape: String,
    pub layers: Vec<String>,
    pub net_name: Option<String>,
    pub net_code: Option<i32>,
    pub position: Option<Position>,
    pub size_mm: Option<(f64, f64)>,
    pub drill_mm: Option<Vec<f64>>,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub reference: String,
    pub kind: String,
    pub footprint: String,
    pub layer: String,
    pub pads: Vec<Pad>,
    pub value: Option<String>,
    pub position: Option<Position>,
    pub uuid: Option<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetConnection {
    pub component: String,
    pub pad: String,
    pub component_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    pub connections: Vec<NetConnection>,
    pub code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub name: String,
    pub source_path: String,
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
    pub source_format: String,
    pub title: Option<String>,
    pub generator: Option<String>,
    pub generator_version: Option<String>,
    pub board_version: Option<String>,
    pub paper: Option<String>,
    pub layers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DslError {
    message: String,
}

impl DslError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DslError {}

impl From<std::io::Error> for DslError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

pub fn derive_nets(components: &[Component]) -> Vec<Net> {
    let mut connections_by_net: BTreeMap<String, BTreeSet<NetConnection>> = BTreeMap::new();
    let mut codes_by_net: BTreeMap<String, i32> = BTreeMap::new();

    for component in components {
        for pad in &component.pads {
            let Some(net_name) = pad.net_name.clone() else {
                continue;
            };
            connections_by_net
                .entry(net_name.clone())
                .or_default()
                .insert(NetConnection {
                    component: component.reference.clone(),
                    pad: pad.number.clone(),
                    component_kind: Some(component.kind.clone()),
                });
            if let Some(code) = pad.net_code {
                codes_by_net.insert(net_name, code);
            }
        }
    }

    connections_by_net
        .into_iter()
        .map(|(name, connections)| Net {
            code: codes_by_net.get(&name).copied(),
            name,
            connections: connections.into_iter().collect(),
        })
        .collect()
}
