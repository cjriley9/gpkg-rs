/// Represents a spatial reference system as it appears in the GeoPackage [specification](https://www.geopackage.org/spec130/#gpkg_spatial_ref_sys_cols)
pub struct SpatialRefSys<'a> {
    pub name: &'a str,
    pub id: i64,
    pub organization: &'a str,
    pub organization_coordsys_id: i64,
    pub definition: &'a str,
    pub description: &'a str,
}

pub mod defaults {
    use super::SpatialRefSys;
    pub const WGS84: SpatialRefSys = SpatialRefSys {
        name: "WGS 84 geodetic",
        id: 4326,
        organization: "EPSG",
        organization_coordsys_id: 4326,
        definition: "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"degree\",0.0174532925199433,AUTHORITY[\"EPSG\",\"9122\"]],AUTHORITY[\"EPSG\",\"4326\"]]",
        description: "longitude/latitude coordinates in decimal degrees on the WGS 84 spheroid",
    };
    pub const GEOGRAPHIC: SpatialRefSys = SpatialRefSys {
        name: "undefined geographic SRS",
        id: 0,
        organization: "NONE",
        organization_coordsys_id: 0,
        definition: "undefined",
        description: "undefined geographic coordinate reference system",
    };
    pub const CARTESIAN: SpatialRefSys = SpatialRefSys {
        name: "undefined cartesian SRS",
        id: -1,
        organization: "NONE",
        organization_coordsys_id: -1,
        definition: "undefined",
        description: "undefined cartesian coordinate reference system",
    };
}
