#![allow(dead_code)]
use rusqlite::{Connection, DatabaseName, OpenFlags, Result, params};
use std::path::Path;

struct GeoPackage {
    conn: Connection,
}

struct SpatialRefSys {
    name: String,
    id: u32,
    organization: String,
    organization_coordsys_id: u32,
    definition: String,
    description: String,
}
const CREATE_EXTENSTIONS_TABLE: &str = "CREATE TABLE gpkg_extensions (
        table_name TEXT,
        column_name TEXT,
        extension_name TEXT NOT NULL,
        definition TEXT NOT NULL,
        scope TEXT NOT NULL,
        CONSTRAINT ge_tce UNIQUE (table_name, column_name, extension_name)
    );";

const CREATE_GEOMETRY_COLUMNS_TABLE: &str = "CREATE TABLE gpkg_geometry_columns (
        table_name TEXT NOT NULL,
        column_name TEXT NOT NULL,
        geometry_type_name TEXT NOT NULL,
        srs_id INTEGER NOT NULL,
        z TINYINT NOT NULL,
        m TINYINT NOT NULL,
        CONSTRAINT pk_geom_cols PRIMARY KEY (table_name, column_name),
        CONSTRAINT uk_gc_table_name UNIQUE (table_name),
        CONSTRAINT fk_gc_tn FOREIGN KEY (table_name) REFERENCES gpkg_contents(table_name),
        CONSTRAINT fk_gc_srs FOREIGN KEY (srs_id) REFERENCES gpkg_spatial_ref_sys (srs_id)
    );";

const CREATE_SPATIAL_REF_SYS_TABLE: &str = "CREATE TABLE gpkg_spatial_ref_sys (
        srs_name TEXT NOT NULL,
        srs_id INTEGER NOT NULL PRIMARY KEY,
        organization TEXT NOT NULL,
        organization_coordsys_id INTEGER NOT NULL,
        definition TEXT NOT NULL,
        description TEXT NOT NULL
    )";

const CREATE_CONTENTS_TABLE: &str = "CREATE TABLE gpkg_contents (
        table_name TEXT NOT NULL PRIMARY KEY,
        data_type TEXT NOT NULL,
        identifier TEXT UNIQUE,
        description TEXT DEFAULT '',
        last_change DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        min_x DOUBLE,
        min_y DOUBLE,
        max_x DOUBLE,
        max_y DOUBLE,
        srs_id INTEGER,
        CONSTRAINT fk_gc_r_srs_id FOREIGN KEY (srs_id) REFERENCES gpkg_spatial_ref_sys(srs_id)
    )";

const CREATE_TILE_MATRIX_TABLE: &str = "CREATE TABLE gpkg_tile_matrix (
        table_name TEXT NOT NULL,
        zoom_level INTEGER NOT NULL,
        matrix_width INTEGER NOT NULL,
        matrix_height INTEGER NOT NULL,
        tile_width INTEGER NOT NULL,
        tile_height INTEGER NOT NULL,
        pixel_x_size DOUBLE NOT NULL,
        pixel_y_size DOUBLE NOT NULL,
        CONSTRAINT pk_ttm PRIMARY KEY (table_name, zoom_level),
        CONSTRAINT fk_tmm_table_name FOREIGN KEY (table_name) REFERENCES gpkg_contents(table_name)
    );";

// let default_ref_systems = vec![
//     SpatialRefSys {
//         name: "WGS 84 geodetic", 
//         id: 4326, 
//         organization: "EPSG", 
//         organization_coordsys_id: 4326, 
//         definition: "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"degree\",0.0174532925199433,AUTHORITY[\"EPSG\",\"9122\"]],AUTHORITY[\"EPSG\",\"4326\"]]", 
//         description: "longitude/latitude coordinates in decimal degrees on the WGS 84 spheroid",
//     },
// ];

impl GeoPackage {
    pub fn close(self) {
        self.conn.close().unwrap();
    }
}

fn create<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
    let conn = Connection::open(path)?;
    conn.pragma_update(Some(DatabaseName::Main), "application_id", 0x47504B47)?;
    conn.pragma_update(Some(DatabaseName::Main), "user_version", 10300)?;
    conn.execute(CREATE_CONTENTS_TABLE, [])?;
    conn.execute(CREATE_SPATIAL_REF_SYS_TABLE, [])?;
    conn.execute("INSERT INTO gpkg_spatial_ref_sys VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![
                "WGS 84 geodetic", 
                4326, 
                "EPSG", 
                4326, 
                "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"degree\",0.0174532925199433,AUTHORITY[\"EPSG\",\"9122\"]],AUTHORITY[\"EPSG\",\"4326\"]]", 
                "longitude/latitude coordinates in decimal degrees on the WGS 84 spheroid",
                ])?;
    conn.execute("INSERT INTO gpkg_spatial_ref_sys VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![
                "undefined cartesian SRS", 
                -1, 
                "NONE", 
                -1, 
                "undefined", 
                "undefined cartesian coordinate reference system",
                ])?;
    conn.execute("INSERT INTO gpkg_spatial_ref_sys VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![
                "undefined geographic SRS", 
                0, 
                "NONE", 
                0, 
                "undefined", 
                "undefined geographic coordinate reference system",
                ])?;
    Ok(GeoPackage { conn })
}

fn open<P: AsRef<Path>>(path: P) -> Result<GeoPackage> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let integrity_check: String =
        conn.query_row("SELECT * FROM pragma_integrity_check()", [], |row| {
            row.get(0)
        })?;
    assert_eq!(integrity_check, "ok".to_owned());
    let application_id: u32 =
        conn.query_row("SELECT * FROM pragma_application_id()", [], |row| {
            row.get(0)
        })?;
    assert_eq!(application_id, 0x47504B47);
    let user_version: u32 =
        conn.query_row("SELECT * FROM pragma_user_version()", [], |row| row.get(0))?;
    // what do we do with the user version?
    dbg!(user_version);
    // check that there are no foreign keys as per spec
    // use a block to force a drop of stmt and release the borrow
    // so that we can move conn
    {
        let mut stmt = conn.prepare("SELECT * FROM pragma_foreign_key_check()")?;
        let mut rows = stmt.query([])?;
        assert!(rows.next()?.is_none());
    }

    Ok(GeoPackage { conn })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn new_gpkg() {
        let path = Path::new("test_data/create.gpkg");
        let db = create(path).unwrap();
        db.close();
        open(path).unwrap();

        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
