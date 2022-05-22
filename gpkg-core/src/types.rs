#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error decoding WKB geometry")]
    GeomDecodeError,
    #[error("Error encoding WKB geometry")]
    GeomEncodeError,
    #[error("Unsupported WKB geometry type")]
    UnsupportedGeometryType,
}

pub struct GPKGPointM {
    x: f64,
    y: f64,
    m: f64,
}

pub struct GPKGPointZ {
    x: f64,
    y: f64,
    z: f64,
}

pub struct GPKGPointZM {
    x: f64,
    y: f64,
    z: f64,
    m: f64,
}
pub type GPKGMultiPointM = Vec<GPKGPointM>;
pub type GPKGMultiPointZ = Vec<GPKGPointZ>;
pub type GPKGMultiPointZM = Vec<GPKGPointZM>;

pub type GPKGLineStringM = Vec<GPKGPointM>;
pub type GPKGLineStringZ = Vec<GPKGPointZ>;
pub type GPKGLineStringZM = Vec<GPKGPointZM>;

pub type GPKGMultiLineStringM = Vec<GPKGLineStringM>;
pub type GPKGMultiLineStringZ = Vec<GPKGLineStringZ>;
pub type GPKGMultiLineStringZM = Vec<GPKGLineStringZM>;

pub struct GPKGPolygonM {
    exterior: GPKGLineStringM,
    interiors: Vec<GPKGLineStringM>,
}

pub struct GPKGPolygonZ {
    exterior: GPKGLineStringZ,
    interiors: Vec<GPKGLineStringZ>,
}

pub struct GPKGPolygonZM {
    exterior: GPKGLineStringZM,
    interiors: Vec<GPKGLineStringZM>,
}

pub type GPKGMultiPolygonM = Vec<GPKGPolygonM>;
pub type GPKGMultiPolygonZ = Vec<GPKGPolygonZ>;
pub type GPKGMultiPolygonZM = Vec<GPKGPolygonZM>;
