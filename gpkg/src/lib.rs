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
use rusqlite::{params, Connection, DatabaseName, OpenFlags};
use std::path::Path;
use types::{Error, Result};

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
    fn get_create_sql() -> &'static str;

    fn get_insert_sql() -> &'static str;

    fn get_select_sql() -> &'static str;

    fn get_select_where(predicate: &str) -> String;

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>;

    fn as_params(&self) -> Vec<&(dyn rusqlite::ToSql + '_)>;

    fn get_gpkg_table_name() -> &'static str;
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
    /// ```ignore
    /// # use std::path::Path;
    /// let path = Path::new("./test.gpkg");
    /// let gp = GeoPackage::create(path).unwrap();
    /// ```
    pub fn create<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
        if path.as_ref().exists() {
            return Err(Error::CreateExistingError);
        }
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

    fn create_layer<'a, T: GPKGModel<'a>>(&self) -> Result<()> {
        self.conn.execute_batch(T::get_create_sql())?;
        Ok(())
    }

    fn insert_record<'a, T: GPKGModel<'a>>(&self, record: &T) -> Result<()> {
        let sql = T::get_insert_sql();
        self.conn.execute(sql, record.as_params().as_slice())?;
        Ok(())
    }

    fn insert_many<'a, T: GPKGModel<'a>>(&mut self, records: &Vec<T>) -> Result<()> {
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

    fn get_all<'a, T: GPKGModel<'a>>(&self) -> Result<Vec<T>> {
        let mut stmt = self.conn.prepare(T::get_select_sql())?;
        let mut out_vec = Vec::new();
        let rows = stmt.query_map([], |row| T::from_row(row))?;
        for r in rows {
            out_vec.push(r?)
        }
        Ok(out_vec)
    }

    /// Fetch all records from the table containing items of this type that
    /// match the given predicate.
    /// # Examples
    /// ```ignore
    /// struct Item {
    ///     length: f64,
    /// }
    ///
    /// let records: Vec<Item> = gp.get_where::<Item>("length > 10.0").unwrap();
    ///
    /// for r in records {
    ///     assert!(r.length > 10.0);
    /// }
    /// ```
    fn get_where<'a, T: GPKGModel<'a>>(&self, predicate: &str) -> crate::Result<Vec<T>> {
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

    fn update_layer_srs_id(&mut self, table_name: &str, srs_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE gpkg_contents SET srs_id = ?1 WHERE table_name = ?2",
            params![srs_id, table_name],
        )?;
        tx.execute(
            "UPDATE gpkg_geometry_columns SET srs_id = ?1 WHERE table_name = ?2",
            params![srs_id, table_name],
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
    use std::fs;
    use std::path::Path;
    use tempfile::{tempdir, TempDir};

    use crate::types::*;

    use super::*;

    #[derive(GPKGModel, Debug)]
    #[table_name = "test"]
    struct TestTableGeom {
        start_node: Option<i64>,
        end_node: i64,
        rev_cost: String,
        #[geom_field]
        geom: types::GPKGLineStringZ,
    }

    #[derive(GPKGModel, Debug, PartialEq, Eq)]
    #[table_name = "test"]
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
}
