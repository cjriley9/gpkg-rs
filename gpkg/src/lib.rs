//! gpkg is a crate intended to enable interactions with [GeoPackages](https://www.geopackage.org/)

#![allow(dead_code)]
mod gpkg_wkb;
mod result;
mod sql;
mod srs;
/// A set of geometry types with the required implementations to be used for readung and writing to GeoPackages
pub mod types;
use crate::sql::table_definitions::*;
use crate::srs::defaults::*;
#[doc(inline)]
pub use gpkg_derive::GPKGModel;
#[doc(inline)]
pub use gpkg_wkb::GeoPackageWKB;
#[doc(inline)]
pub use result::{Error, Result};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, OptionalExtension};
#[doc(inline)]
pub use srs::SpatialRefSys;
use std::path::Path;

/// A GeoPackage, upon creation, the necessary tables for conformance to the specification are created,
/// and validation is performed upon opening.
pub struct GeoPackage {
    /// The underlying rusqlite connection for the GeoPackage
    ///
    /// Access is provided here to allow a user to do what is necessary for their specific use case,
    /// but extra care should be taken when using this, since the
    /// integrity of the GeoPackage could be compromised.
    pub conn: rusqlite::Connection,
}

/// A trait that allows for easy writes and reads of a struct into a GeoPackage.
/// Currently usable only for vector features and attribute only data.
pub trait GPKGModel<'a>: Sized {
    fn get_create_sql() -> &'static str;

    fn get_insert_sql() -> &'static str;

    fn get_select_sql() -> &'static str;

    fn get_select_where(predicate: &str) -> String;

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>;

    fn as_params(&self) -> Vec<&(dyn rusqlite::ToSql + '_)>;

    fn get_gpkg_layer_name() -> &'static str;
}

#[derive(Debug)]
enum GPKGDataType {
    Features,
    Attributes,
}

#[derive(Debug)]
struct LayerDefinition {
    name: String,
    data_type: String,
    identifier: Option<String>,
    description: Option<String>,
    last_change: String,
    min_x: Option<f64>,
    min_y: Option<f64>,
    max_x: Option<f64>,
    max_y: Option<f64>,
    srs_id: Option<i64>,
}

impl GeoPackage {
    /// Creates an empty geopackage with the following metadata tables:
    /// * gpkg_extensions
    /// * gpkg_contents
    /// * gpkg_geometry_columns
    /// * gpkg_spatial_ref_sys
    /// * gpkg_tile_matrix
    /// * gpkg_tile_matrix_set
    ///
    /// # Usage
    /// ```
    /// # use std::path::Path;
    /// # use gpkg::GeoPackage;
    /// # use tempfile::tempdir;
    /// # let dir = tempdir().unwrap();
    /// # let path = dir.path().join("create.gpkg");
    /// let gp = GeoPackage::create(path).unwrap();
    /// ```
    pub fn create<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
        if path.as_ref().exists() {
            return Err(Error::CreateExistingError);
        }
        let conn = Connection::open(path)?;
        let gpkg = GeoPackage { conn };
        gpkg.conn
            .pragma_update(Some(DatabaseName::Main), "application_id", 0x47504B47)?;
        gpkg.conn
            .pragma_update(Some(DatabaseName::Main), "user_version", 10300)?;
        // requrement 10
        gpkg.conn.execute(CREATE_SPATIAL_REF_SYS_TABLE, [])?;
        // insert the default SRS as per spec requirement 11
        gpkg.new_srs(&WGS84)?;
        gpkg.new_srs(&CARTESIAN)?;
        gpkg.new_srs(&GEOGRAPHIC)?;
        // requirement 13
        gpkg.conn.execute(CREATE_CONTENTS_TABLE, [])?;
        gpkg.conn.execute(CREATE_GEOMETRY_COLUMNS_TABLE, [])?;
        gpkg.conn.execute(CREATE_EXTENSTIONS_TABLE, [])?;
        gpkg.conn.execute(CREATE_TILE_MATRIX_TABLE, [])?;
        gpkg.conn.execute(CREATE_TILE_MATRIX_SET_TABLE, [])?;
        Ok(gpkg)
    }
    /// Create a new layer to store instances of a type that implements [GPKGModel]
    /// # Usage
    /// ```
    /// # use std::path::Path;
    /// # use gpkg::{GeoPackage, GPKGModel};
    /// # use tempfile::tempdir;
    /// # let dir = tempdir().unwrap();
    /// # let path = dir.path().join("create_layer.gpkg");
    /// # let gp = GeoPackage::create(path).unwrap();
    /// #[derive(GPKGModel)]
    /// struct TestLayer {
    ///     field1: i64,
    ///     field2: String,
    ///     field3: f64,
    /// }
    ///
    /// gp.create_layer::<TestLayer>().unwrap();
    /// ```
    pub fn create_layer<'a, T: GPKGModel<'a>>(&self) -> Result<()> {
        self.conn.execute_batch(T::get_create_sql())?;
        Ok(())
    }

    pub fn insert_record<'a, T: GPKGModel<'a>>(&self, record: &T) -> Result<()> {
        let sql = T::get_insert_sql();
        self.conn.execute(sql, record.as_params().as_slice())?;
        Ok(())
    }

    pub fn insert_many<'a, T: GPKGModel<'a>>(&mut self, records: &Vec<T>) -> Result<()> {
        let sql = T::get_insert_sql();
        let tx = self.conn.transaction()?;
        // extra block is here so that stmt gets dropped
        {
            let mut stmt = tx.prepare(sql)?;
            for record in records {
                stmt.execute(record.as_params().as_slice())?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Fetch all records in the layer containing items of this type that
    /// match the given predicate.
    /// # Examples
    /// ```
    /// # use std::path::Path;
    /// # use gpkg::{GeoPackage, GPKGModel};
    /// # use tempfile::tempdir;
    /// # let dir = tempdir().unwrap();
    /// # let path = dir.path().join("get_all.gpkg");
    /// # let gp = GeoPackage::create(path).unwrap();
    /// #[derive(GPKGModel)]
    /// struct Item {
    ///     length: f64,
    /// }
    ///
    /// gp.create_layer::<Item>().unwrap();
    ///
    /// let item1 = Item {length: 25.0};
    /// gp.insert_record(&item1).unwrap();
    ///
    /// let item2 = Item {length: 5.0};
    /// gp.insert_record(&item2).unwrap();
    ///
    /// let records: Vec<Item> = gp.get_all::<Item>().unwrap();
    ///
    /// assert_eq!(records.len(), 2);
    /// ```
    pub fn get_all<'a, T: GPKGModel<'a>>(&self) -> Result<Vec<T>> {
        let mut stmt = self.conn.prepare(T::get_select_sql())?;
        let mut out_vec = Vec::new();
        let rows = stmt.query_map([], |row| T::from_row(row))?;
        for r in rows {
            out_vec.push(r?)
        }
        Ok(out_vec)
    }

    /// Fetch all records in the layer containing items of this type that
    /// match the given predicate.
    /// # Examples
    /// ```
    /// # use std::path::Path;
    /// # use gpkg::{GeoPackage, GPKGModel};
    /// # use tempfile::tempdir;
    /// # let dir = tempdir().unwrap();
    /// # let path = dir.path().join("get_all.gpkg");
    /// # let gp = GeoPackage::create(path).unwrap();
    /// #[derive(GPKGModel)]
    /// struct Item {
    ///     length: f64,
    /// }
    ///
    /// gp.create_layer::<Item>().unwrap();
    ///
    /// let item1 = Item {length: 25.0};
    /// gp.insert_record(&item1).unwrap();
    ///
    /// let item2 = Item {length: 5.0};
    /// gp.insert_record(&item2).unwrap();
    ///
    /// let records: Vec<Item> = gp.get_where::<Item>("length >= 10.0").unwrap();
    ///
    /// assert_eq!(records.len(), 1);
    /// ```
    pub fn get_where<'a, T: GPKGModel<'a>>(&self, predicate: &str) -> crate::Result<Vec<T>> {
        let mut stmt = self.conn.prepare(T::get_select_where(predicate).as_str())?;
        let mut out_vec = Vec::new();
        let rows = stmt.query_map([], |row| T::from_row(row))?;
        for r in rows {
            out_vec.push(r?)
        }
        Ok(out_vec)
    }

    /// Add a new spatial reference system to the geopackage
    pub fn new_srs(&self, srs: &SpatialRefSys) -> Result<()> {
        const STMT: &str = "INSERT INTO gpkg_spatial_ref_sys VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
        self.conn.execute(
            STMT,
            params![
                srs.name,
                srs.id,
                srs.organization,
                srs.organization_coordsys_id,
                srs.definition,
                srs.description,
            ],
        )?;
        Ok(())
    }

    /// Retrieve the srs_id for a layer
    pub fn get_layer_srs_id(&self, layer_name: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT FROM gpkg_contents WHERE table_name = ?1")?;
        let temp: Option<i64> = stmt.query_row(params![layer_name], |r| r.get(0).optional())?;
        Ok(temp)
    }

    /// Update the SRS ID for a layer.
    ///
    /// Note that this does not check if the provided SRS has a corresponding entry in the GeoPackage metadata.
    pub fn update_layer_srs_id(&mut self, layer_name: &str, srs_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE gpkg_contents SET srs_id = ?1 WHERE layer_name = ?2",
            params![srs_id, layer_name],
        )?;
        tx.execute(
            "UPDATE gpkg_geometry_columns SET srs_id = ?1 WHERE layer_name = ?2",
            params![srs_id, layer_name],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Close the geopackage
    /// # Examples
    /// ```ignore
    /// # use std::path::Path;
    /// let path = Path::new("./test.gpkg");
    /// let gp = GeoPackage::create(path).unwrap();
    /// // do some things with the GeoPackage
    /// gp.close();
    /// ```
    pub fn close(self) {
        self.conn.close().unwrap();
    }

    /// Open a geopackage, doing validation of the GeoPackage internals to ensure that operation will work correctly.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        // check the user application_id and user_version as per requirement 2
        let application_id: u32 =
            conn.query_row("SELECT * FROM pragma_application_id()", [], |row| {
                row.get(0)
            })?;
        if application_id != 0x47504B47 {
            return Err(Error::ValidationError);
        }
        // what do we do with the user version?
        // it doesn't seem safe to just fail if this doesn't match something
        // maybe this should just have an acceptable range?
        let _user_version: u32 =
            conn.query_row("SELECT * FROM pragma_user_version()", [], |row| row.get(0))?;
        // integrity check from requirement 6
        let integrity_check: String =
            conn.query_row("SELECT * FROM pragma_integrity_check()", [], |row| {
                row.get(0)
            })?;
        if integrity_check.as_str() != "ok" {
            return Err(Error::ValidationError);
        }
        // check that there are no foreign keys as per spec requirement 7
        // use a block to force a drop of stmt and release the borrow
        // so that we can move conn
        {
            let mut stmt = conn.prepare("SELECT * FROM pragma_foreign_key_check()")?;
            let mut rows = stmt.query([])?;
            if !(rows.next()?.is_none()) {
                return Err(Error::ValidationError);
            }
        }

        Ok(GeoPackage { conn })
    }
}

#[cfg(test)]
mod tests {
    use geo_types::*;
    use std::fs;
    use tempfile::tempdir;

    use crate::types::*;

    use super::*;

    #[derive(GPKGModel, Debug)]
    #[layer_name = "test"]
    struct TestTableGeom {
        start_node: Option<i64>,
        end_node: i64,
        rev_cost: String,
        // #[geom_field("LineStringZ")]
        #[geom_field("LineStringZ")]
        geom: types::GPKGLineStringZ,
    }

    #[derive(GPKGModel, Debug, PartialEq, Eq)]
    #[layer_name = "test"]
    struct TestTableAttr {
        field1: Option<i64>,
        field2: i64,
        field3: String,
    }

    #[test]
    fn create_table() {
        let dir = tempdir().unwrap();
        let filename = dir.path().join("create.gpkg");

        let gp = GeoPackage::create(&filename).unwrap();
        gp.create_layer::<TestTableGeom>().unwrap();

        // how do we get a check that the table exists?
        // make sure that we've got something in gpkg_contents

        gp.close();
        fs::remove_file(filename).unwrap();
    }

    #[test]
    #[should_panic]
    fn fail_on_existing() {
        let dir = tempdir().unwrap();
        let filename = dir.path().join("fail_exists.gpkg");

        let gp = GeoPackage::create(&filename).unwrap();
        gp.create_layer::<TestTableGeom>().unwrap();

        // how do we get a check that the table exists?
        // make sure that we've got something in gpkg_contents

        gp.close();

        let _shouldnt_open = GeoPackage::create(&filename).unwrap();

        fs::remove_file(filename).unwrap();
    }

    #[test]
    fn insert_row() {
        let dir = tempdir().unwrap();
        let filename = dir.path().join("create.gpkg");

        let test_val = TestTableAttr {
            field1: Some(999),
            field2: 420,
            field3: String::from("blah"),
        };

        let gp = GeoPackage::create(&filename).unwrap();
        gp.create_layer::<TestTableAttr>().unwrap();
        gp.insert_record(&test_val).unwrap();

        // how do we get a check that the table exists?
        // make sure that we've got something in gpkg_contents
        let retrieved = &gp.get_all::<TestTableAttr>().unwrap();

        assert_eq!(test_val, retrieved[0]);

        gp.close();
        fs::remove_file(filename).unwrap();
    }

    #[test]
    fn insert_many() {
        let dir = tempdir().unwrap();
        let filename = dir.path().join("insert_many.gpkg");
        let mut gp = GeoPackage::create(&filename).unwrap();
        gp.create_layer::<TestTableGeom>()
            .expect("Problem creating table");
        let val = TestTableGeom {
            start_node: Some(42),
            end_node: 918,
            rev_cost: "Test values".to_owned(),
            geom: GPKGLineStringZ(vec![
                GPKGPointZ {
                    x: 40.0,
                    y: -105.0,
                    z: 5280.0,
                },
                GPKGPointZ {
                    x: 41.0,
                    y: -106.0,
                    z: 5280.0,
                },
            ]),
        };
        let val2 = TestTableGeom {
            start_node: Some(45),
            end_node: 918,
            rev_cost: "Test values".to_owned(),
            geom: GPKGLineStringZ(vec![
                GPKGPointZ {
                    x: 40.0,
                    y: -105.0,
                    z: 5280.0,
                },
                GPKGPointZ {
                    x: 41.0,
                    y: -106.0,
                    z: 5280.0,
                },
            ]),
        };
        let val3 = TestTableGeom {
            start_node: Some(48),
            end_node: 918,
            rev_cost: "Test values".to_owned(),
            geom: GPKGLineStringZ(vec![
                GPKGPointZ {
                    x: 40.0,
                    y: -105.0,
                    z: 5280.0,
                },
                GPKGPointZ {
                    x: 41.0,
                    y: -106.0,
                    z: 5280.0,
                },
            ]),
        };
        let vec_for_insert = vec![val, val2, val3];
        gp.insert_many(&vec_for_insert).unwrap();

        gp.close();
        let db2 = GeoPackage::open(&filename).unwrap();
        let retrieved = db2.get_all::<TestTableGeom>().unwrap();
        assert!(retrieved.len() == vec_for_insert.len());
    }

    #[test]
    fn multipoint_test() {
        #[derive(GPKGModel)]
        struct MPTest {
            id: i64,
            #[geom_field("MultiPoint")]
            geom: GPKGMultiPoint,
        }

        let filename = Path::new("./test_data/multipoint.gpkg");
        let gp = GeoPackage::create(&filename).unwrap();

        gp.create_layer::<MPTest>().unwrap();

        let sample = MPTest {
            id: 99,
            geom: GPKGMultiPoint(geo_types::MultiPoint::new(vec![
                (coord! {x: 1.0, y: 1.0}).into(),
                (coord! {x: 3.0, y: 3.0}).into(),
                (coord! {x: 5.0, y: 5.0}).into(),
                (coord! {x: 7.0, y: 7.0}).into(),
            ])),
        };

        gp.insert_record(&sample).unwrap();

        gp.close();
    }

    fn get_test_multilinestring() -> geo_types::MultiLineString<f64> {
        let ls1: geo_types::LineString<f64> = geo_types::LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 41.5},
            coord! {x: -107.0, y: 43.0},
        ]);

        let ls2: geo_types::LineString<f64> = geo_types::LineString::new(vec![
            coord! {x: -15.0, y: 4.0},
            coord! {x: -16.0, y: 4.5},
            coord! {x: -17.0, y: 4.0},
        ]);
        geo_types::MultiLineString::new(vec![ls1, ls2])
    }

    #[test]
    fn multilinestring_test() {
        #[derive(GPKGModel)]
        struct MPTest {
            id: i64,
            #[geom_field("MultiLineString")]
            geom: GPKGMultiLineString,
        }

        let filename = Path::new("./test_data/multilinestring.gpkg");
        let gp = GeoPackage::create(&filename).unwrap();

        gp.create_layer::<MPTest>().unwrap();

        let test_geom = GPKGMultiLineString(get_test_multilinestring());
        dbg!(test_geom.to_wkb().unwrap());

        let sample = MPTest {
            id: 99,
            geom: test_geom,
        };

        gp.insert_record(&sample).unwrap();

        gp.close();
    }

    fn get_test_multipolygon() -> MultiPolygon<f64> {
        let poly1_exterior: LineString<f64> = LineString::new(vec![
            coord! {x: -105.0, y: 40.0},
            coord! {x: -106.0, y: 43.5},
            coord! {x: -107.0, y: 41.0},
            coord! {x: -105.0, y: 40.0},
        ]);
        let poly1 = Polygon::new(poly1_exterior, vec![]);

        let poly2_exterior: LineString<f64> = LineString::new(vec![
            coord! {x: -15.0, y: 4.0},
            coord! {x: 16.0, y: 4.5},
            coord! {x: -1.0, y: 10.0},
            coord! {x: -10.0, y: 10.0},
            coord! {x: -15.0, y: 4.0},
        ]);

        let poly2_interior: LineString<f64> = LineString::new(vec![
            coord! {x: -1.53, y: 4.999},
            coord! {x: 1.609, y: 5.67},
            coord! {x: -2.345, y: 6.2},
            coord! {x: -1.53, y: 4.999},
        ]);
        let poly2 = Polygon::new(poly2_exterior, vec![poly2_interior]);

        MultiPolygon::new(vec![poly1, poly2])
    }

    #[test]
    fn multipolygon_test() {
        #[derive(GPKGModel)]
        struct MPTest {
            id: i64,
            #[geom_field("MultiPolygon")]
            geom: GPKGMultiPolygon,
        }

        let filename = Path::new("./test_data/multipolygon.gpkg");
        let gp = GeoPackage::create(&filename).unwrap();

        gp.create_layer::<MPTest>().unwrap();

        let test_geom = GPKGMultiPolygon(get_test_multipolygon());
        dbg!(test_geom.to_wkb().unwrap());

        let sample = MPTest {
            id: 99,
            geom: test_geom,
        };

        gp.insert_record(&sample).unwrap();

        gp.close();
    }
}
