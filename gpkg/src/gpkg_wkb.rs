use crate::result::{Error, Result};
use crate::types::*;
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use std::io::{Cursor, Read, Write};

/// A trait containing methods for encoding geometries according to the GeoPackage [specifcation](https://www.geopackage.org/spec130/#gpb_spec)
///
/// This trait allows for an easier implementation of the rusqlite [ToSql] and [FromSql] traits needed to read and write geometries to a GeoPackage
pub trait GeoPackageWKB: Sized {
    fn to_wkb(&self) -> Result<Vec<u8>>;
    fn from_wkb(wkb: &mut [u8]) -> Result<Self>;
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
    GPKGMultiLineString,
    GPKGPointZ,
    GPKGLineStringZ
}

impl<T: FullWKB> GeoPackageWKB for T {
    fn to_wkb(&self) -> Result<Vec<u8>> {
        let mut header: Vec<u8> = Vec::new();
        // magic number that is GP in ASCII
        header.extend_from_slice(&[0x47, 0x50]);
        // version number, 0 means version 1
        header.push(0);
        let flags = 0b00000001;
        header.push(flags);
        let srs = i32::to_le_bytes(4326);
        header.extend_from_slice(&srs);
        self.write_as_wkb(&mut header)?;
        Ok(header)
    }
    fn from_wkb(bytes: &mut [u8]) -> Result<Self> {
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

        Ok(T::read_from_wkb(&mut bytes_cursor)?)
    }
}

// helper macro to reduce boilerplate to implement FullWKB for these newtypes where the inner type
// implements FulWKB
macro_rules! full_wkb_from_inner {
    ($t:ty, $inner:ty) => {
        impl FullWKB for $t {
            fn write_as_wkb(&self, w: &mut impl Write) -> Result<()> {
                self.0.write_as_wkb(w)
            }

            fn read_from_wkb(r: &mut impl Read) -> Result<Self> {
                Ok(Self(<$inner>::read_from_wkb(r)?))
            }
        }
    };
}

full_wkb_from_inner!(GPKGPoint, geo_types::Point::<f64>);
full_wkb_from_inner!(GPKGLineString, geo_types::LineString::<f64>);
full_wkb_from_inner!(GPKGPolygon, geo_types::Polygon::<f64>);
full_wkb_from_inner!(GPKGMultiPoint, geo_types::MultiPoint::<f64>);
full_wkb_from_inner!(GPKGMultiLineString, geo_types::MultiLineString::<f64>);
full_wkb_from_inner!(GPKGMultiPolygon, geo_types::MultiPolygon::<f64>);
full_wkb_from_inner!(GPKGGeometry, geo_types::Geometry::<f64>);
full_wkb_from_inner!(GPKGGeometryCollection, geo_types::GeometryCollection::<f64>);

// a trait meant to be used internally to make it easier to read and write wkb for types that contain other types
trait WKBBytesRaw: Sized {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()>;
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self>;
}

impl WKBBytesRaw for geo_types::Coordinate<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_all(&self.x.to_le_bytes())?;
        w.write_all(&self.y.to_le_bytes())?;
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let x = r.read_f64::<T>()?;
        let y = r.read_f64::<T>()?;
        Ok((x, y).into())
    }
}

impl WKBBytesRaw for geo_types::Point<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_all(&self.x().to_le_bytes())?;
        w.write_all(&self.y().to_le_bytes())?;
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let x = r.read_f64::<T>()?;
        let y = r.read_f64::<T>()?;
        Ok((x, y).into())
    }
}

impl WKBBytesRaw for geo_types::LineString<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_points = r.read_u32::<T>()?;
        let mut out_vec = Vec::with_capacity(num_points as usize);
        for _ in 0..num_points {
            out_vec.push(geo_types::Coordinate::<f64>::read_from_bytes::<T, _>(r)?);
        }
        Ok(geo_types::LineString::new(out_vec))
    }
}

impl WKBBytesRaw for geo_types::Polygon<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>((self.interiors().len() + 1) as u32)?;
        self.exterior().write_as_bytes(w)?;
        for ring in self.interiors() {
            ring.write_as_bytes(w)?;
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_rings = r.read_u32::<T>()?;
        let exterior = geo_types::LineString::<f64>::read_from_bytes::<T, _>(r)?;
        let mut interiors = Vec::with_capacity(num_rings as usize - 1);
        for _ in 1..num_rings {
            interiors.push(geo_types::LineString::<f64>::read_from_bytes::<T, _>(r)?);
        }
        Ok(geo_types::Polygon::new(exterior, interiors))
    }
}

impl WKBBytesRaw for geo_types::MultiPoint<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_points = r.read_u32::<T>()?;
        let mut out_vec = Vec::with_capacity(num_points as usize);
        for _ in 0..num_points {
            out_vec.push(geo_types::Point::<f64>::read_from_bytes::<T, _>(r)?);
        }
        Ok(geo_types::MultiPoint::new(out_vec))
    }
}

impl WKBBytesRaw for geo_types::MultiPolygon<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_polys = r.read_u32::<T>()?;
        let mut out_vec = Vec::with_capacity(num_polys as usize);
        for _ in 0..num_polys {
            out_vec.push(geo_types::Polygon::<f64>::read_from_bytes::<T, _>(r)?);
        }
        Ok(geo_types::MultiPolygon::new(out_vec))
    }
}

impl WKBBytesRaw for geo_types::MultiLineString<f64> {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_lines = r.read_u32::<T>()?;
        let mut out_vec = Vec::with_capacity(num_lines as usize);
        for _ in 0..num_lines {
            out_vec.push(geo_types::LineString::<f64>::read_from_bytes::<T, _>(r)?);
        }
        Ok(geo_types::MultiLineString::new(out_vec))
    }
}

impl WKBBytesRaw for GPKGLineStringZ {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_u32::<LittleEndian>(self.0.len() as u32)?;
        for p in &self.0 {
            p.write_as_bytes(w)?
        }
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let num_points = r.read_u32::<T>()?;
        let mut out_vec: Vec<GPKGPointZ> = Vec::new();
        for _ in 0..num_points {
            out_vec.push(GPKGPointZ::read_from_bytes::<T, _>(r)?);
        }
        Ok(GPKGLineStringZ(out_vec))
    }
}

impl WKBBytesRaw for GPKGPointZ {
    fn write_as_bytes(&self, w: &mut impl Write) -> Result<()> {
        w.write_all(&self.x.to_le_bytes())?;
        w.write_all(&self.y.to_le_bytes())?;
        w.write_all(&self.z.to_le_bytes())?;
        Ok(())
    }
    fn read_from_bytes<T: ByteOrder, U: Read>(r: &mut U) -> Result<Self> {
        let x = r.read_f64::<T>()?;
        let y = r.read_f64::<T>()?;
        let z = r.read_f64::<T>()?;
        Ok(GPKGPointZ { x, y, z })
    }
}

pub(crate) trait FullWKB: Sized {
    fn write_as_wkb(&self, w: &mut impl Write) -> Result<()>;
    fn read_from_wkb(r: &mut impl Read) -> Result<Self>;
}

// implementation of FullWKB for a type that has an implementation of WKBBytesRaw
// a macro is used here because we need the extra info about what the geom flag number
// is for the type
macro_rules! full_wkb {
    ($t:ty, $x:expr) => {
        impl FullWKB for $t {
            fn write_as_wkb(&self, w: &mut impl Write) -> Result<()> {
                // we will always write as little endian
                w.write_u8(1)?;
                w.write_u32::<LittleEndian>($x)?;
                self.write_as_bytes(w)?;
                Ok(())
            }

            fn read_from_wkb(r: &mut impl Read) -> Result<Self> {
                // the resolution of this issue (https://github.com/rust-lang/rust/issues/83701)
                // will make this code much more simple. The plan would be to declare
                // a variable with the type impl ByteOrder and then assign it a value
                // in the first match statement
                let endianness = match r.read_u8()? {
                    0 => 0u8,
                    1 => 1u8,
                    _ => return Err(Error::GeomDecodeError),
                };
                let geom_type: u32 = match endianness {
                    0 => r.read_u32::<BigEndian>()?,
                    1 => r.read_u32::<LittleEndian>()?,
                    _ => unreachable!(),
                };
                if geom_type != $x {
                    return Err(Error::UnsupportedGeometryType);
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

full_wkb! {GPKGPointZ, 1001}
full_wkb! {GPKGLineStringZ, 1002}
full_wkb! {geo_types::Point<f64>, 1}
full_wkb! {geo_types::LineString<f64>, 2}
full_wkb! {geo_types::Polygon<f64>, 3}
full_wkb! {geo_types::MultiPoint<f64>, 4}
full_wkb! {geo_types::MultiLineString<f64>, 5}
full_wkb! {geo_types::MultiPolygon<f64>, 6}

impl FullWKB for geo_types::GeometryCollection<f64> {
    fn write_as_wkb(&self, w: &mut impl Write) -> Result<()> {
        for geom in &self.0 {
            geom.write_as_wkb(w)?
        }
        Ok(())
    }
    fn read_from_wkb(r: &mut impl Read) -> Result<Self> {
        let endianness = match r.read_u8()? {
            0 => 0u8,
            1 => 1u8,
            _ => return Err(Error::GeomDecodeError),
        };
        let geom_type: u32 = match endianness {
            0 => r.read_u32::<BigEndian>()?,
            1 => r.read_u32::<LittleEndian>()?,
            _ => unreachable!(),
        };
        if geom_type != 7 {
            return Err(Error::UnsupportedGeometryType);
        }
        let num_geoms: u32 = match endianness {
            0 => r.read_u32::<BigEndian>()?,
            1 => r.read_u32::<LittleEndian>()?,
            _ => unreachable!(),
        };
        let mut geoms = Vec::with_capacity(num_geoms as usize);
        for _ in 0..num_geoms {
            geoms.push(geo_types::Geometry::<f64>::read_from_wkb(r)?);
        }
        Ok(geo_types::GeometryCollection::new_from(geoms))
    }
}

// this has a ridciulous amount of boilerplate, and will be helped so much by let bindings on impl Trait
impl FullWKB for geo_types::Geometry<f64> {
    fn write_as_wkb(&self, w: &mut impl Write) -> Result<()> {
        match self {
            geo_types::Geometry::Point(p) => p.write_as_wkb(w),
            geo_types::Geometry::LineString(ls) => ls.write_as_wkb(w),
            geo_types::Geometry::Polygon(poly) => poly.write_as_wkb(w),
            geo_types::Geometry::MultiPoint(mp) => mp.write_as_wkb(w),
            geo_types::Geometry::MultiLineString(mls) => mls.write_as_wkb(w),
            geo_types::Geometry::MultiPolygon(mp) => mp.write_as_wkb(w),
            _ => Err(Error::UnsupportedGeometryType),
        }
    }

    fn read_from_wkb(r: &mut impl Read) -> Result<Self> {
        let endianness = match r.read_u8()? {
            0 => 0u8,
            1 => 1u8,
            _ => return Err(Error::GeomDecodeError),
        };
        let geom_type: u32 = match endianness {
            0 => r.read_u32::<BigEndian>()?,
            1 => r.read_u32::<LittleEndian>()?,
            _ => unreachable!(),
        };
        return match geom_type {
            1 => match endianness {
                1 => Ok(geo_types::Geometry::Point(
                    geo_types::Point::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::Point(
                    geo_types::Point::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            2 => match endianness {
                1 => Ok(geo_types::Geometry::LineString(
                    geo_types::LineString::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::LineString(
                    geo_types::LineString::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            3 => match endianness {
                1 => Ok(geo_types::Geometry::Polygon(
                    geo_types::Polygon::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::Polygon(
                    geo_types::Polygon::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            4 => match endianness {
                1 => Ok(geo_types::Geometry::MultiPoint(
                    geo_types::MultiPoint::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::MultiPoint(
                    geo_types::MultiPoint::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            5 => match endianness {
                1 => Ok(geo_types::Geometry::MultiLineString(
                    geo_types::MultiLineString::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::MultiLineString(
                    geo_types::MultiLineString::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            6 => match endianness {
                1 => Ok(geo_types::Geometry::MultiPolygon(
                    geo_types::MultiPolygon::<f64>::read_from_bytes::<LittleEndian, _>(r)?,
                )),
                0 => Ok(geo_types::Geometry::MultiPolygon(
                    geo_types::MultiPolygon::<f64>::read_from_bytes::<BigEndian, _>(r)?,
                )),
                _ => unreachable!(),
            },
            7 => {
                let num_geoms = match endianness {
                    1 => r.read_u32::<LittleEndian>()?,
                    0 => r.read_u32::<LittleEndian>()?,
                    _ => unreachable!(),
                };
                let mut geoms = Vec::new();
                for _ in 0..num_geoms {
                    geoms.push(geo_types::Geometry::read_from_wkb(r)?);
                }
                Ok(geo_types::Geometry::GeometryCollection(
                    geo_types::GeometryCollection::new_from(geoms),
                ))
            }
            // unimplemented types
            _ => Err(Error::UnsupportedGeometryType),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::iter::zip;

    use super::*;
    use byteorder::{BigEndian, LittleEndian};
    use geo_types::{coord, Coordinate, LineString, Point, Polygon};

    fn points_equal(p1: &Point<f64>, p2: &Point<f64>) -> bool {
        return (p1.x().to_ne_bytes() == p2.x().to_ne_bytes())
            && (p1.y().to_ne_bytes() == p2.y().to_ne_bytes());
    }

    fn coords_equal(p1: &Coordinate<f64>, p2: &Coordinate<f64>) -> bool {
        return (p1.x.to_ne_bytes() == p2.x.to_ne_bytes())
            && (p1.y.to_ne_bytes() == p2.y.to_ne_bytes());
    }

    fn linestrings_equal(l1: &LineString<f64>, l2: &LineString<f64>) -> bool {
        for (a, b) in zip(&l1.0, &l2.0) {
            if !coords_equal(&a, &b) {
                return false;
            }
        }
        true
    }

    fn polygons_equal(p1: &Polygon<f64>, p2: &Polygon<f64>) -> bool {
        if !linestrings_equal(p1.exterior(), p2.exterior()) {
            return false;
        }
        for (a, b) in zip(p1.interiors().into_iter(), p1.interiors().into_iter()) {
            if !linestrings_equal(&a, &b) {
                return false;
            }
        }
        true
    }

    #[test]
    fn write_point() {
        let mut manual_buf = Vec::new();
        // little endian
        manual_buf.write_u8(1).unwrap();
        // geom type flag
        manual_buf.write_u32::<LittleEndian>(1).unwrap();
        manual_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();

        let point: Point<f64> = (coord! {x: -105.0, y: 40.0}).into();
        let mut auto_buf = Vec::new();
        point.write_as_wkb(&mut auto_buf).unwrap();

        assert_eq!(manual_buf, auto_buf);

        // lets also make sure we can read in our own output
        let mut rdr = Cursor::new(auto_buf);
        let written_point = Point::read_from_wkb(&mut rdr).unwrap();

        assert!(points_equal(&point, &written_point));
    }

    #[test]
    fn read_point() {
        let mut le_buf = Vec::new();
        le_buf.write_u8(1).unwrap();
        le_buf.write_u32::<LittleEndian>(1).unwrap();
        le_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();

        let pt: Point<f64> = (coord! {x: -105.0, y: 40.0}).into();

        let mut le_rdr = Cursor::new(le_buf);
        let le_cmp_pt = Point::<f64>::read_from_wkb(&mut le_rdr).unwrap();

        assert!(points_equal(&pt, &le_cmp_pt));

        let mut be_buf = Vec::new();
        be_buf.write_u8(0).unwrap();
        be_buf.write_u32::<BigEndian>(1).unwrap();
        be_buf.write_f64::<BigEndian>(-105.0).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();

        let pt: Point<f64> = (coord! {x: -105.0, y: 40.0}).into();

        let mut be_rdr = Cursor::new(be_buf);
        let be_cmp_pt = Point::<f64>::read_from_wkb(&mut be_rdr).unwrap();

        assert!(points_equal(&pt, &be_cmp_pt))
    }

    #[test]
    fn write_linestring() {
        let mut manual_buf = Vec::new();
        // little endian
        manual_buf.write_u8(1).unwrap();
        // geom type flag
        manual_buf.write_u32::<LittleEndian>(2).unwrap();
        // number of points
        manual_buf.write_u32::<LittleEndian>(3).unwrap();
        // points
        manual_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(41.5).unwrap();
        manual_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(43.0).unwrap();

        let ls: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
        ]);
        let mut auto_buf = Vec::new();
        ls.write_as_wkb(&mut auto_buf).unwrap();

        assert_eq!(manual_buf, auto_buf);

        // lets also make sure we can read in our own output
        let mut rdr = Cursor::new(auto_buf);
        let written_ls = LineString::read_from_wkb(&mut rdr).unwrap();

        assert!(linestrings_equal(&ls, &written_ls));
    }

    #[test]
    fn read_linestring() {
        let mut le_buf = Vec::new();
        // little endian
        le_buf.write_u8(1).unwrap();
        // geom type flag
        le_buf.write_u32::<LittleEndian>(2).unwrap();
        // number of points
        le_buf.write_u32::<LittleEndian>(3).unwrap();
        // points
        le_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        le_buf.write_f64::<LittleEndian>(41.5).unwrap();
        le_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        le_buf.write_f64::<LittleEndian>(43.0).unwrap();

        let ls: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
        ]);

        let mut le_rdr = Cursor::new(le_buf);
        let le_cmp_ls = LineString::read_from_wkb(&mut le_rdr).unwrap();

        assert_eq!(&ls, &le_cmp_ls);

        let mut be_buf = Vec::new();
        // big endian
        be_buf.write_u8(0).unwrap();
        // geom type flag
        be_buf.write_u32::<BigEndian>(2).unwrap();
        // number of points
        be_buf.write_u32::<BigEndian>(3).unwrap();
        // points
        be_buf.write_f64::<BigEndian>(-105.0).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();
        be_buf.write_f64::<BigEndian>(-106.0).unwrap();
        be_buf.write_f64::<BigEndian>(41.5).unwrap();
        be_buf.write_f64::<BigEndian>(-107.0).unwrap();
        be_buf.write_f64::<BigEndian>(43.0).unwrap();

        let ls: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
        ]);

        let mut be_rdr = Cursor::new(be_buf);
        let be_cmp_ls = LineString::read_from_wkb(&mut be_rdr).unwrap();

        assert_eq!(&ls, &be_cmp_ls);
    }

    #[test]
    fn write_polygon() {
        let mut manual_buf = Vec::new();
        // little endian
        manual_buf.write_u8(1).unwrap();
        // geom type flag
        manual_buf.write_u32::<LittleEndian>(3).unwrap();
        // number of points
        manual_buf.write_u32::<LittleEndian>(2).unwrap();
        // exterior ring
        manual_buf.write_u32::<LittleEndian>(5).unwrap();
        manual_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(41.5).unwrap();
        manual_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(43.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();
        // interior_ring
        manual_buf.write_u32::<LittleEndian>(4).unwrap();
        manual_buf.write_f64::<LittleEndian>(-105.5).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(41.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(42.0).unwrap();
        manual_buf.write_f64::<LittleEndian>(-105.5).unwrap();
        manual_buf.write_f64::<LittleEndian>(40.0).unwrap();

        let exterior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
            coord! {x: -107.0, y: 40.0},
            coord! {x: -105.0, y: 40.0},
        ]);

        let interior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.5, y: 40.0},
            coord! {x: -106.0, y: 41.0},
            coord! {x: -107.0, y: 42.0},
            coord! {x: -105.5, y: 40.0},
        ]);
        let poly = Polygon::new(exterior_ring, vec![interior_ring]);

        let mut auto_buf = Vec::new();
        poly.write_as_wkb(&mut auto_buf).unwrap();

        assert_eq!(manual_buf, auto_buf);

        // lets also make sure we can read in our own output
        let mut rdr = Cursor::new(auto_buf);
        let written_poly = Polygon::read_from_wkb(&mut rdr).unwrap();

        assert!(polygons_equal(&poly, &written_poly));
    }

    #[test]
    fn read_polygon() {
        let mut le_buf = Vec::new();
        // little endian
        le_buf.write_u8(1).unwrap();
        // geom type flag
        le_buf.write_u32::<LittleEndian>(3).unwrap();
        // number of points
        le_buf.write_u32::<LittleEndian>(2).unwrap();
        // exterior ring
        le_buf.write_u32::<LittleEndian>(5).unwrap();
        le_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        le_buf.write_f64::<LittleEndian>(41.5).unwrap();
        le_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        le_buf.write_f64::<LittleEndian>(43.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-105.0).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();
        // interior_ring
        le_buf.write_u32::<LittleEndian>(4).unwrap();
        le_buf.write_f64::<LittleEndian>(-105.5).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-106.0).unwrap();
        le_buf.write_f64::<LittleEndian>(41.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-107.0).unwrap();
        le_buf.write_f64::<LittleEndian>(42.0).unwrap();
        le_buf.write_f64::<LittleEndian>(-105.5).unwrap();
        le_buf.write_f64::<LittleEndian>(40.0).unwrap();

        let exterior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
            coord! {x: -107.0, y: 40.0},
            coord! {x: -105.0, y: 40.0},
        ]);

        let interior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.5, y: 40.0},
            coord! {x: -106.0, y: 41.0},
            coord! {x: -107.0, y: 42.0},
            coord! {x: -105.5, y: 40.0},
        ]);
        let poly = Polygon::new(exterior_ring, vec![interior_ring]);

        let mut le_rdr = Cursor::new(le_buf);
        let le_cmp_poly = Polygon::read_from_wkb(&mut le_rdr).unwrap();

        assert!(polygons_equal(&poly, &le_cmp_poly));

        let mut be_buf = Vec::new();
        // little endian
        be_buf.write_u8(0).unwrap();
        // geom type flag
        be_buf.write_u32::<BigEndian>(3).unwrap();
        // number of points
        be_buf.write_u32::<BigEndian>(2).unwrap();
        // exterior ring
        be_buf.write_u32::<BigEndian>(5).unwrap();
        be_buf.write_f64::<BigEndian>(-105.0).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();
        be_buf.write_f64::<BigEndian>(-106.0).unwrap();
        be_buf.write_f64::<BigEndian>(41.5).unwrap();
        be_buf.write_f64::<BigEndian>(-107.0).unwrap();
        be_buf.write_f64::<BigEndian>(43.0).unwrap();
        be_buf.write_f64::<BigEndian>(-107.0).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();
        be_buf.write_f64::<BigEndian>(-105.0).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();
        // interior_ring
        be_buf.write_u32::<BigEndian>(4).unwrap();
        be_buf.write_f64::<BigEndian>(-105.5).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();
        be_buf.write_f64::<BigEndian>(-106.0).unwrap();
        be_buf.write_f64::<BigEndian>(41.0).unwrap();
        be_buf.write_f64::<BigEndian>(-107.0).unwrap();
        be_buf.write_f64::<BigEndian>(42.0).unwrap();
        be_buf.write_f64::<BigEndian>(-105.5).unwrap();
        be_buf.write_f64::<BigEndian>(40.0).unwrap();

        let exterior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
            coord! {x: -107.0, y: 40.0},
            coord! {x: -105.0, y: 40.0},
        ]);

        let interior_ring: LineString<f64> = LineString::new(vec![
            coord! {x: -105.5, y: 40.0},
            coord! {x: -106.0, y: 41.0},
            coord! {x: -107.0, y: 42.0},
            coord! {x: -105.5, y: 40.0},
        ]);
        let poly = Polygon::new(exterior_ring, vec![interior_ring]);

        let mut be_rdr = Cursor::new(be_buf);
        let be_cmp_poly = Polygon::read_from_wkb(&mut be_rdr).unwrap();

        assert!(polygons_equal(&poly, &be_cmp_poly));
    }
}
