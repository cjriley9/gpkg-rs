#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error decoding WKB geometry")]
    GeomDecodeError,
    #[error("Error encoding WKB geometry")]
    GeomEncodeError,
}

pub struct PointM {
    x: f64,
    y: f64,
    m: f64,
}

pub struct PointZ {
    x: f64,
    y: f64,
    z: f64,
}

pub struct PointZM {
    x: f64,
    y: f64,
    z: f64,
    m: f64,
}
pub type MultiPointM = Vec<PointM>;
pub type MultiPointZ = Vec<PointZ>;
pub type MultiPointZM = Vec<PointZM>;

pub type LineStringM = Vec<PointM>;
pub type LineStringZ = Vec<PointZ>;
pub type LineStringZM = Vec<PointZM>;

pub type MultiLineStringM = Vec<LineStringM>;
pub type MultiLineStringZ = Vec<LineStringZ>;
pub type MultiLineStringZM = Vec<LineStringZM>;

pub struct PolygonM {
    exterior: LineStringM,
    interiors: Vec<LineStringM>,
}

pub struct PolygonZ {
    exterior: LineStringZ,
    interiors: Vec<LineStringZ>,
}

pub struct PolygonZM {
    exterior: LineStringZM,
    interiors: Vec<LineStringZM>,
}

pub type MultiPolygonM = Vec<PolygonM>;
pub type MultiPolygonZ = Vec<PolygonZ>;
pub type MultiPolygonZM = Vec<PolygonZM>;
