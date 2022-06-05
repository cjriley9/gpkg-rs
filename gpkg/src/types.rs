#[derive(Debug)]
pub struct GPKGPointM {
    pub x: f64,
    pub y: f64,
    pub m: f64,
}

#[derive(Debug)]
pub struct GPKGPointZ {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug)]
pub struct GPKGPointZM {
    x: f64,
    y: f64,
    z: f64,
    m: f64,
}
#[derive(Debug)]
pub struct GPKGMultiPointM(pub Vec<GPKGPointM>);
#[derive(Debug)]
pub struct GPKGMultiPointZ(pub Vec<GPKGPointZ>);
#[derive(Debug)]
pub struct GPKGMultiPointZM(pub Vec<GPKGPointZM>);

#[derive(Debug)]
pub struct GPKGLineStringM(pub Vec<GPKGPointM>);
#[derive(Debug)]
pub struct GPKGLineStringZ(pub Vec<GPKGPointZ>);
#[derive(Debug)]
pub struct GPKGLineStringZM(pub Vec<GPKGPointZM>);

#[derive(Debug)]
pub struct GPKGMultiLineStringM(pub Vec<GPKGLineStringM>);
#[derive(Debug)]
pub struct GPKGMultiLineStringZ(pub Vec<GPKGLineStringZ>);
#[derive(Debug)]
pub struct GPKGMultiLineStringZM(pub Vec<GPKGLineStringZM>);

#[derive(Debug)]
pub struct GPKGPolygonM {
    exterior: GPKGLineStringM,
    interiors: Vec<GPKGLineStringM>,
}

#[derive(Debug)]
pub struct GPKGPolygonZ {
    exterior: GPKGLineStringZ,
    interiors: Vec<GPKGLineStringZ>,
}

#[derive(Debug)]
pub struct GPKGPolygonZM {
    exterior: GPKGLineStringZM,
    interiors: Vec<GPKGLineStringZM>,
}

#[derive(Debug)]
pub struct GPKGMultiPolygonM(Vec<GPKGPolygonM>);
#[derive(Debug)]
pub struct GPKGMultiPolygonZ(Vec<GPKGPolygonZ>);
#[derive(Debug)]
pub struct GPKGMultiPolygonZM(Vec<GPKGPolygonZM>);

#[derive(Debug)]
pub struct GPKGPoint(pub geo_types::Point<f64>);
#[derive(Debug)]
pub struct GPKGLineString(pub geo_types::LineString<f64>);
#[derive(Debug)]
pub struct GPKGPolygon(pub geo_types::Polygon<f64>);
#[derive(Debug)]
pub struct GPKGMultiPoint(pub geo_types::MultiPoint<f64>);
#[derive(Debug)]
pub struct GPKGMultiLineString(pub geo_types::MultiLineString<f64>);
#[derive(Debug)]
pub struct GPKGMultiPolygon(pub geo_types::MultiPolygon<f64>);

#[derive(Debug)]
pub struct GPKGGeometry(pub geo_types::Geometry<f64>);

#[derive(Debug)]
pub struct GPKGGeometryCollection(pub geo_types::GeometryCollection<f64>);
