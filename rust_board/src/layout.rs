#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub x_mm: f64,
    pub y_mm: f64,
}

impl Point {
    pub fn new(x_mm: f64, y_mm: f64) -> Self {
        Self { x_mm, y_mm }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    pub min_x_mm: f64,
    pub min_y_mm: f64,
    pub max_x_mm: f64,
    pub max_y_mm: f64,
}

impl Bounds {
    pub fn new(min_x_mm: f64, min_y_mm: f64, max_x_mm: f64, max_y_mm: f64) -> Self {
        Self {
            min_x_mm,
            min_y_mm,
            max_x_mm,
            max_y_mm,
        }
    }

    pub fn width_mm(&self) -> f64 {
        self.max_x_mm - self.min_x_mm
    }

    pub fn height_mm(&self) -> f64 {
        self.max_y_mm - self.min_y_mm
    }

    pub fn expand(&self, margin_mm: f64) -> Self {
        Self {
            min_x_mm: self.min_x_mm - margin_mm,
            min_y_mm: self.min_y_mm - margin_mm,
            max_x_mm: self.max_x_mm + margin_mm,
            max_y_mm: self.max_y_mm + margin_mm,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinePrimitive {
    pub start: Point,
    pub end: Point,
    pub layer: String,
    pub width_mm: f64,
    pub owner: Option<String>,
    pub owner_kind: Option<String>,
    pub net_name: Option<String>,
    pub stroke_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CirclePrimitive {
    pub center: Point,
    pub radius_mm: f64,
    pub layer: String,
    pub width_mm: f64,
    pub owner: Option<String>,
    pub owner_kind: Option<String>,
    pub fill: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextPrimitive {
    pub text: String,
    pub position: Point,
    pub layer: String,
    pub owner: Option<String>,
    pub size_mm: Option<(f64, f64)>,
    pub rotation_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PadGeometry {
    pub component: String,
    pub number: String,
    pub shape: String,
    pub pad_type: String,
    pub position: Point,
    pub size_mm: (f64, f64),
    pub layers: Vec<String>,
    pub net_name: Option<String>,
    pub rotation_deg: Option<f64>,
    pub drill_mm: Option<Vec<f64>>,
    pub display_layer: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViaGeometry {
    pub position: Point,
    pub size_mm: f64,
    pub layers: Vec<String>,
    pub net_name: Option<String>,
    pub drill_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZonePolygon {
    pub layer: String,
    pub points: Vec<Point>,
    pub name: Option<String>,
    pub keepout: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FootprintLayout {
    pub reference: String,
    pub footprint: String,
    pub layer: String,
    pub position: Point,
    pub pads: Vec<PadGeometry>,
    pub graphics: Vec<LinePrimitive>,
    pub label: Option<TextPrimitive>,
    pub rotation_deg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardLayout {
    pub name: String,
    pub source_path: String,
    pub bounds: Bounds,
    pub footprints: Vec<FootprintLayout>,
    pub edge_cuts: Vec<LinePrimitive>,
    pub drawings: Vec<LinePrimitive>,
    pub circles: Vec<CirclePrimitive>,
    pub texts: Vec<TextPrimitive>,
    pub tracks: Vec<LinePrimitive>,
    pub vias: Vec<ViaGeometry>,
    pub zones: Vec<ZonePolygon>,
}
