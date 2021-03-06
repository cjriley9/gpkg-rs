# GeoPackage
_______

A Rust crate for reading and writing [GeoPackages](https://www.geopackage.org/).

The goal of the crate is to adhere to the Geopackage [specification](https://www.geopackage.org/spec130/index.html) and provide interop with popular geospatial libraries in the Rust ecosystem.


- [X] Read 2D vector data
- [X] Write 2D vector data
- [ ] Read vector data with M and Z coordinates
- [ ] Write vector data with M and Z coordinates
- [ ] Support for user specified SRS other than WGS84 
- [ ] Support writing bounding boxes for geometries
- [ ] Support for the [RTree Spatial Indexes](https://www.geopackage.org/spec130/#extension_rtree) extension
- [ ] Read image tile data 
- [ ] Write image tile data 

## Notes:
* Reading and writing 3D vector data is currently in a holding pattern until this [pull request](https://github.com/georust/geo/pull/797) is either accepted or rejected.