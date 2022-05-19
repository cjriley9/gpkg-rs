use geo_types::{coord, Coordinate, LineString, Point};
use rusqlite::types::ToSqlOutput;
use rusqlite::ToSql;
use std::convert::Into;

pub struct GeoPackageGeom<T: GeoPackageWKB> {
    t: T,
}

pub trait GeoPackageWKB {
    fn toWKB(&self) -> Result<Vec<u8>, wkb::WKBWriteError>;
    fn fromWKB(wkb: &[u8]) -> Self;
}

impl GeoPackageWKB for geo_types::Point<f64> {
    fn toWKB(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn fromWKB(wkb: &[u8]) -> Self {
        geo_types::Point::new(0.0, 0.0)
    }
}

// {
//     #[inline]
//     fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
//         Ok(ToSqlOutput::from(self.t.toWKB().unwrap()))
//     }
// }

impl GeoPackageWKB for geo_types::LineString<f64> {
    fn toWKB(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn fromWKB(wkb: &[u8]) -> Self {
        let p1 = coord!(x: 0.0f64, y:0.0f64);
        let p2 = coord!(x: 10.0f64, y:10.0f64);
        geo_types::LineString::new(vec![p1, p2])
    }
}

// impl GeoPackageWKB for geo_types::Polygon<f64> {}

// impl GeoPackageWKB for geo_types::MultiPoint<f64> {}

// impl GeoPackageWKB for geo_types::MultiLineString<f64> {}

// impl GeoPackageWKB for geo_types::MultiPolygon<f64> {}
