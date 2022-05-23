#![allow(dead_code)]
pub mod gpkg_wkb;
mod sql;
pub mod srs;
pub mod types;
use crate::sql::table_definitions::*;
use crate::srs::{defaults::*, SpatialRefSys};
use geo_types::Polygon;
#[doc(inline)]
pub use gpkg_derive::GPKGModel;
use rusqlite::{params, Connection, DatabaseName, OpenFlags, Result};
use std::path::Path;

/// A GeoPackage, upon creation, the necessary tables for conformance to the specification are created,
/// and validation is performed upon opening.
pub struct GeoPackage {
    /// The underlying rusqlite connection for the GeoPackage
    ///
    /// Access is provided here to allow a user to do what is necessary for their specific use case,
    /// but extra care should be taken if using this for write operations, since the
    /// integrity of the GeoPackage could be compromised.
    pub conn: Connection,
    tables: Vec<TableDefinition>,
}

/// A trait that allows for easy writes and reads of a struct into a GeoPackage.
/// Currently usable only for vector features and attribute only data.
pub trait GPKGModel<'a>: Sized {
    /// Creates a table and the associated metadata within the GeoPackage
    fn create_table(gpkg: &GeoPackage) -> Result<()>;
    /// Insert a single record into the corresponding table for the type.
    fn insert_record(&self, gpkg: &GeoPackage) -> Result<()>;
    /// Fetch a single record from the table containing items of this type.
    fn get_first(gpkg: &GeoPackage) -> Result<Option<Self>>;
    /// Fetch all records from the table containing items of this type.
    fn get_all(gpkg: &GeoPackage) -> Result<Vec<Self>>;
    /// Fetch all records from the table containing items of this type that
    /// match the given predicate.
    /// # Examples
    /// ```
    /// struct Item {
    ///     length: f64,
    /// }
    ///
    /// let records: Vec<Item> = Item::get_where(&db, "length > 10.0").unwrap();
    ///
    /// for r in records {
    ///     assert!(r.length > 10.0);
    /// }
    /// ```
    fn get_where(gpkg: &GeoPackage, predicate: &str) -> Result<Vec<Self>>;
}

#[derive(Debug)]
pub enum GPKGDataType {
    Features,
    Attributes,
}

#[derive(Debug)]
struct TableDefinition {
    name: String,
    data_type: GPKGDataType,
    srs_id: Option<i64>,
    identifier: String,
    description: String,
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
    /// # Examples
    /// ```
    /// let path = Path::new("./test.gpkg");
    /// let gp = GeoPackage::create(path).unwrap();
    /// ```
    pub fn create<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
        let conn = Connection::open(path)?;
        let gpkg = GeoPackage {
            conn,
            tables: Vec::new(),
        };
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

    fn new_srs(&self, srs: &SpatialRefSys) -> Result<()> {
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

    /// Close the geopackage
    /// # Examples
    /// ```
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
        assert_eq!(application_id, 0x47504B47);
        let user_version: u32 =
            conn.query_row("SELECT * FROM pragma_user_version()", [], |row| row.get(0))?;
        // what do we do with the user version?
        dbg!(user_version);
        // integrity check from requirement 6
        let integrity_check: String =
            conn.query_row("SELECT * FROM pragma_integrity_check()", [], |row| {
                row.get(0)
            })?;
        assert_eq!(integrity_check, "ok".to_owned());
        // check that there are no foreign keys as per spec requirement 7
        // use a block to force a drop of stmt and release the borrow
        // so that we can move conn
        {
            let mut stmt = conn.prepare("SELECT * FROM pragma_foreign_key_check()")?;
            let mut rows = stmt.query([])?;
            assert!(rows.next()?.is_none());
        }
        // get the tables
        let tables = Vec::new();

        Ok(GeoPackage { conn, tables })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use geo_types::{coord, LineString, Point, Polygon};

    use crate::types::*;

    use super::*;

    #[derive(GPKGModel, Debug)]
    #[table_name = "test"]
    struct TestTable {
        start_node: Option<i64>,
        end_node: i64,
        rev_cost: String,
        #[geom_field]
        geom: types::GPKGLineStringZ,
    }

    #[test]
    fn new_gpkg() {
        let path = Path::new("../test_data/create.gpkg");
        let db = GeoPackage::create(path).unwrap();
        TestTable::create_table(&db).expect("Problem creating table");
        let val = TestTable {
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
        let val2 = TestTable {
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
        let val3 = TestTable {
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
        val.insert_record(&db).unwrap();
        val2.insert_record(&db).unwrap();
        val3.insert_record(&db).unwrap();
        println!("{:?}", TestTable::get_where(&db, "start_node > 50"));

        db.close();
        GeoPackage::open(path).unwrap();

        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
