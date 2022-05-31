/// The result returned by many methods within the crate
pub type Result<T, E = Error> = std::result::Result<T, E>;

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
    #[error("Tried to create a geopackage that already exists")]
    CreateExistingError,
    #[error("GeoPackage failed validation check when opening")]
    ValidationError,
}
