use geo_types::{coord, Coordinate, LineString, Point};
use rusqlite::types::ToSqlOutput;
use rusqlite::ToSql;
use std::convert::Into;
use std::io::{Cursor, Read};

// pub struct GeoPackageGeom<T: GeoPackageWKB> {
//     t: T,
// }

pub trait GeoPackageWKB: Sized {
    fn toWKB(&self) -> Result<Vec<u8>, wkb::WKBWriteError>;
    fn fromWKB(wkb: &mut [u8]) -> Result<Self, wkb::WKBReadError>;
}

enum EnvelopeType {
    Missing,
    XY,
    XYM,
    XYZ,
    XYZM,
}

struct GPKGGeomFlags {
    extended: bool,
    empty_geom: bool,
    little_endian: bool,
    envelope: EnvelopeType,
}

impl GPKGGeomFlags {
    // https://www.geopackage.org/spec130/#flags_layout
    // need to add error handling
    fn from_byte(b: u8) -> Self {
        let extended = ((b >> 5) & 1) > 0;
        let empty_geom = ((b >> 4) & 1) > 0;
        let little_endian = (b & 1) > 0;
        let envelope = match (b >> 1) & 0b111 {
            0 => EnvelopeType::Missing,
            1 => EnvelopeType::XY,
            2 => EnvelopeType::XYZ,
            3 => EnvelopeType::XYM,
            4 => EnvelopeType::XYZM,
            _ => panic!("invalid envelope flag, don't know how to get geometry"),
        };
        GPKGGeomFlags {
            extended,
            empty_geom,
            little_endian,
            envelope,
        }
    }

    fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        let envelope_val = match self.envelope {
            EnvelopeType::Missing => 0,
            EnvelopeType::XY => 1,
            EnvelopeType::XYZ => 2,
            EnvelopeType::XYM => 3,
            EnvelopeType::XYZM => 4,
        };
        flags |= (self.extended as u8) << 5;
        flags |= (self.empty_geom as u8) << 4;
        flags |= (envelope_val as u8) << 1;
        flags |= self.little_endian as u8;

        flags
    }
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
    fn fromWKB(wkb: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
        Ok(geo_types::Point::new(0.0, 0.0))
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
    fn fromWKB(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
        // for now we should just kinda ignore the header and just chew through it
        // let magic = u16::from(wkb[0..2]);
        let flags = GPKGGeomFlags::from_byte(bytes[3]);
        let mut srs_bytes: [u8; 4] = Default::default();
        srs_bytes.copy_from_slice(&bytes[4..8]);
        let _srs = match flags.little_endian {
            true => i32::from_le_bytes(srs_bytes),
            false => i32::from_be_bytes(srs_bytes),
        };
        let envelope_length: usize = match flags.envelope {
            EnvelopeType::Missing => 0,
            EnvelopeType::XY => 32,
            EnvelopeType::XYZ | EnvelopeType::XYM => 48,
            EnvelopeType::XYZM => 64,
        };

        let geom_start = 8 + envelope_length;

        let mut bytes_cursor = Cursor::new(&bytes[geom_start..]);

        let geom = wkb::wkb_to_geom(&mut bytes_cursor)?;

        Ok(geom.try_into().unwrap())
    }
}

// impl GeoPackageWKB for geo_types::Polygon<f64> {}

// impl GeoPackageWKB for geo_types::MultiPoint<f64> {}

// impl GeoPackageWKB for geo_types::MultiLineString<f64> {}

// impl GeoPackageWKB for geo_types::MultiPolygon<f64> {}
