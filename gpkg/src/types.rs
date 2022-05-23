use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use wkb::WKBReadError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error decoding WKB geometry")]
    GeomDecodeError,
    #[error("Error encoding WKB geometry")]
    GeomEncodeError,
    #[error("Unsupported WKB geometry type")]
    UnsupportedGeometryType,
    #[error("Error when accessing the SQLite database")]
    SQLiteError(#[from] rusqlite::Error),
}

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

impl GPKGPointZ {
    pub fn write_as_bytes(&self, w: &mut impl Write) -> Result<(), wkb::WKBWriteError> {
        w.write_all(&self.x.to_le_bytes())?;
        w.write_all(&self.y.to_le_bytes())?;
        w.write_all(&self.z.to_le_bytes())?;
        Ok(())
    }
    pub fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self, WKBReadError> {
        let x = r.read_f64::<T>()?;
        let y = r.read_f64::<T>()?;
        let z = r.read_f64::<T>()?;
        Ok(GPKGPointZ { x, y, z })
    }
}

impl GPKGLineStringZ {
    pub fn write_as_bytes(&self, w: &mut impl Write) -> Result<(), wkb::WKBWriteError> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    pub fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self, WKBReadError> {
        let num_points = r.read_u32::<T>()?;
        dbg!(num_points);
        let mut out_vec: Vec<GPKGPointZ> = Vec::new();
        for _ in 0..num_points {
            out_vec.push(GPKGPointZ::read_from_bytes::<T, _>(r)?);
        }
        Ok(GPKGLineStringZ(out_vec))
    }
}

macro_rules! wkb_ext {
    ($t:ty, $x:expr) => {
        impl wkb::WKBAbleExt for $t {
            fn write_as_wkb(&self, w: &mut impl Write) -> Result<(), wkb::WKBWriteError> {
                // we will always write as little endian
                w.write_u8(1)?;
                w.write_u32::<LittleEndian>($x)?;
                self.write_as_bytes(w)?;
                Ok(())
            }

            fn read_from_wkb(r: &mut impl Read) -> Result<Self, WKBReadError> {
                // the resolution of this issue (https://github.com/rust-lang/rust/issues/83701)
                // will make this code much more simple. The plan would be to declare
                // a variable with the type impl ByteOrder and then assign it a value
                // in the first match statement
                let endianness = match r.read_u8()? {
                    0 => 0u8,
                    1 => 1u8,
                    _ => return Err(WKBReadError::WrongType),
                };
                let geom_type: u32 = match endianness {
                    0 => r.read_u32::<BigEndian>()?,
                    1 => r.read_u32::<LittleEndian>()?,
                    _ => unreachable!(),
                };
                if geom_type != $x {
                    return Err(WKBReadError::WrongType);
                }
                match endianness {
                    0 => Ok(Self::read_from_bytes::<BigEndian, _>(r)?),
                    1 => Ok(Self::read_from_bytes::<LittleEndian, _>(r)?),
                    _ => unreachable!(),
                }
            }
        }
    };
}

wkb_ext! {GPKGPointZ, 1001}
wkb_ext! {GPKGLineStringZ, 1002}

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
