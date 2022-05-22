#![allow(dead_code)]
pub mod gpkg_wkb;
mod sql;
pub mod srs;
pub mod types;
use crate::sql::table_definitions::*;
use crate::srs::{defaults::*, SpatialRefSys};
use geo_types::Polygon;
pub use gpkg_derive::GPKGModel;
use rusqlite::{params, Connection, DatabaseName, OpenFlags, Result};
use std::path::Path;

pub struct GeoPackage {
    pub conn: Connection,
    tables: Vec<TableDefinition>,
}

pub trait GPKGModel<'a> {
    fn create_table(gpkg: &GeoPackage) -> Result<()>;
    fn insert_record(&self, gpkg: &GeoPackage) -> Result<()>;
    fn get_first(gpkg: &GeoPackage) -> Self;
}

struct ATestTable<'a> {
    start_node: i64,
    end_node: i64,
    for_cost: f64,
    rev_cost: &'a [u8],
    geom: Polygon<f64>,
}

struct TableDefinition {
    name: String,
}

impl GeoPackage {
    // creates an empty geopackage that conforms to the spec
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

    // fn create_table(&self, def: &TableDefinition) -> Result<()> {
    //     todo!()
    // }

    pub fn close(self) {
        self.conn.close().unwrap();
    }

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

    use crate::gpkg_wkb::{GPKGLineString, GPKGPolygon};

    use super::*;

    #[derive(GPKGModel, Debug)]
    #[table_name = "test"]
    struct TestTable {
        start_node: Option<i64>,
        end_node: i64,
        rev_cost: String,
        #[geom_field]
        geom: gpkg_wkb::GPKGPolygon,
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
            geom: GPKGPolygon(Polygon::new(
                LineString::new(vec![
                    coord!(x: 40.0, y:-105.0),
                    coord!(x:41.0, y:-106.0),
                    coord!(x:40.0, y:-106.0),
                    coord!(x: 40.0, y:-105.0),
                ]),
                vec![],
            )),
        };
        val.insert_record(&db).unwrap();
        println!("{:?}", TestTable::get_first(&db));

        db.close();
        GeoPackage::open(path).unwrap();

        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
