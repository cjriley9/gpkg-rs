use crate::types::Error;
use rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use std::convert::Into;
use std::io::Cursor;

// pub struct GeoPackageGeom<T: GeoPackageWKB> {
//     t: T,
// }

pub trait GeoPackageWKB: Sized {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError>;
    fn from_wkb(wkb: &mut [u8]) -> Result<Self, wkb::WKBReadError>;
}

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

// once there is a GeoPackageWKB impl for the type
// the to/from sql impls are really simple, so the macro
// should help with boilerplate
macro_rules! impl_gpkg_sql_wkb {
    ($($t:ty),*) => {
       $(
            impl ToSql for $t {
                #[inline]
                fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                    Ok(ToSqlOutput::from(self.to_wkb().map_err(|_| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(Error::GeomEncodeError))
                    })?))
                }
            }

            impl FromSql for $t {
                #[inline]
                fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                    let mut vec: Vec<u8> = value.as_blob().map(<[u8]>::to_vec)?;
                    let slice = vec.as_mut_slice();
                    let pt = <$t>::from_wkb(slice)
                        .map_err(|_| rusqlite::types::FromSqlError::Other(Box::new(Error::GeomDecodeError)))?;
                    Ok(pt)
                }
            }
       )*
    };
}

impl_gpkg_sql_wkb! {
    GPKGPoint,
    GPKGPolygon,
    GPKGLineString,
    GPKGMultiPoint,
    GPKGMultiPolygon,
    GPKGMultiLineString
}

impl GeoPackageWKB for GPKGPoint {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGPoint(geom.try_into().unwrap()))
    }
}

impl GeoPackageWKB for GPKGLineString {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGLineString(
            geom.try_into().map_err(|_| wkb::WKBReadError::WrongType)?,
        ))
    }
}

impl GeoPackageWKB for GPKGPolygon {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGPolygon(
            geom.try_into().map_err(|_| wkb::WKBReadError::WrongType)?,
        ))
    }
}

impl GeoPackageWKB for GPKGMultiPoint {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGMultiPoint(
            geom.try_into().map_err(|_| wkb::WKBReadError::WrongType)?,
        ))
    }
}

impl GeoPackageWKB for GPKGMultiLineString {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGMultiLineString(
            geom.try_into().map_err(|_| wkb::WKBReadError::WrongType)?,
        ))
    }
}

impl GeoPackageWKB for GPKGMultiPolygon {
    fn to_wkb(&self) -> Result<Vec<u8>, wkb::WKBWriteError> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        let geom = wkb::geom_to_wkb(&(self.0.clone().into()))?;
        header.extend(geom);
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self, wkb::WKBReadError> {
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

        Ok(GPKGMultiPolygon(
            geom.try_into().map_err(|_| wkb::WKBReadError::WrongType)?,
        ))
    }
}
