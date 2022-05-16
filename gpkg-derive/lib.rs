#![allow(dead_code)]
use geo_types::GeometryCollection;
use proc_macro2::TokenStream;
use quote::quote;
use std::{fmt::format, ops::Deref};
use syn::{
    parse2, parse_macro_input, Attribute, DeriveInput, Field, GenericArgument, Ident, Lit, Meta,
    Type, TypePath, TypeReference,
};

const GEO_TYPES: &'static [&'static str] = &[
    "Polygon",
    "LineString",
    "Point",
    "MultiPolygon",
    "MultiLineString",
    "MultiPoint",
];

// #[proc_macro_derive(GPKGModel)]
// pub fn derive_gpkg(input: TokenStream) -> TokenStream {
//     derive_gpkg_inner(input)
// }

fn derive_gpkg_inner(input: TokenStream) -> TokenStream {
    let ast = parse2::<DeriveInput>(input).unwrap();
    // let ast = parse_macro_input!(input as DeriveInput);

    let tbl_name_meta = get_meta_attr(&ast.attrs, "table_name");
    // dbg!(&tbl_name_meta);
    let tbl_name = match tbl_name_meta {
        Some(meta) => match meta {
            Meta::NameValue(nv) => match nv.lit {
                Lit::Str(ls) => Some(ls.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    };

    // ge the name for our table name
    let name = &ast.ident;

    let fields = match &ast.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => fields.named.iter(),
            _ => panic!("GPKGModel derive expected named fields"),
        },
        _ => panic!("GPKGModel derive expected a struct"),
    }
    .collect();

    impl_model(&name.clone(), &fields, tbl_name)
}

fn get_meta_attr<'a>(attrs: &Vec<Attribute>, name: &'a str) -> Option<Meta> {
    let mut temp = attrs
        .iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .filter(|i| match i.path().get_ident() {
            Some(i) => i.to_string() == name.to_owned(),
            None => false,
        })
        .collect::<Vec<Meta>>();
    temp.pop()
}

#[derive(Debug, Clone, Copy)]
enum MZOptions {
    Prohibited = 0,
    Mandatory = 1,
    Optional = 2,
}

#[derive(Debug, Clone)]
struct GeomInfo {
    geom_type: String,
    // this is mostly for future proofing, we'll default to wgs84 for now
    srs_id: i64,
    m: MZOptions,
    z: MZOptions,
}

#[derive(Debug)]
struct FieldInfo {
    name: String,
    geom_info: Option<GeomInfo>,
    optional: bool,
    type_for_sql: String,
}

// only going to support &str and &[u8] for now
fn get_reference_type_name(t: &TypeReference) -> String {
    match t.elem.deref() {
        syn::Type::Path(p) => {
            assert!(p.path.segments.len() == 1);
            match get_path_type_name(p).0.as_str() {
                "str" => return String::from("str"),
                _ => panic!("The only reference types supported are &str and &[u8]"),
            }
        }
        syn::Type::Slice(s) => match s.elem.deref() {
            Type::Path(p) => match get_path_type_name(p).0.as_str() {
                "u8" => return String::from("buf"),
                _ => panic!("The only reference types supported are &str and &[u8]"),
            },
            _ => panic!("The only reference types supported are &str and &[u8]"),
        },
        _ => panic!("The only reference types supported are &str and &[u8]"),
    };
}

// return the field name and whether or not it's optional
fn get_path_type_name(p: &TypePath) -> (String, bool) {
    let mut optional = false;
    assert!(p.path.segments.len() > 0);
    let final_segment = p.path.segments.last().unwrap();
    let id_string = final_segment.ident.to_string();
    match id_string.as_str() {
        // get the inner
        "Option" => {
            optional = true;
            if let syn::PathArguments::AngleBracketed(a) = &final_segment.arguments {
                assert!(a.args.len() == 1, "Only one argument allowed in an Option");
                if let GenericArgument::Type(t) = &a.args[0] {
                    match t {
                        Type::Path(p) => {
                            return (get_path_type_name(p).0, optional);
                        }
                        Type::Reference(r) => {
                            return (get_reference_type_name(r), optional);
                        }
                        _ => panic!("Unsupported type within Option"),
                    }
                }
            } else {
                panic!("Unsupported use of the option type");
            }
        }
        "Vec" => {
            if let syn::PathArguments::AngleBracketed(a) = &final_segment.arguments {
                assert!(a.args.len() == 1, "Only one argument allowed in a Vec");
                if let GenericArgument::Type(t) = &a.args[0] {
                    match t {
                        Type::Path(p) => {
                            let type_return = get_path_type_name(p).0;
                            match type_return.as_str() {
                                "u8" => return (String::from("buf"), optional),
                                _ => panic!("Vec<u8> is the only allowed use of the Vec type"),
                            };
                        }
                        _ => panic!("Vec<u8> is the only allowed use of the Vec type"),
                    }
                }
            } else {
                panic!("Vec<u8> is the only allowed use of the Vec type");
            }
        }
        _ if GEO_TYPES.contains(&id_string.as_str()) => {
            if let syn::PathArguments::AngleBracketed(a) = &final_segment.arguments {
                assert!(
                    a.args.len() == 1,
                    "Only one argument allowed in a Geometry type"
                );
                if let GenericArgument::Type(t) = &a.args[0] {
                    match t {
                        Type::Path(p) => {
                            let type_return = get_path_type_name(p).0;
                            match type_return.as_str() {
                            "f64" => return (id_string, optional),
                            _ => panic!("Geo types must use f64 coordinates for geopackage compatibility"),
                        };
                        }
                        _ => panic!(
                            "Geo types must use f64 coordinates for geopackage compatibility"
                        ),
                    }
                }
            } else {
                panic!("Geo types must use f64 coordinates for geopackage compatibility");
            }
        }
        // fall through and use the basic type we got at the beginning
        _ => {}
    }

    (final_segment.ident.to_string(), false)
}

fn impl_model(name: &Ident, fields: &Vec<&Field>, tbl_name: Option<String>) -> TokenStream {
    // overwrite the struct name with a provided table name if one is given
    // TODO: add some level of validation here based on sqlite's rules
    let table_name_final = match tbl_name {
        Some(n) => Ident::new(&n, name.span()),
        None => name.to_owned(),
    };

    let geom_field_name: String;

    // the goal is to support everything here (https://www.geopackage.org/spec130/index.html#table_column_data_types)
    // as well as allow the user change whether a field can have nulls or not with the option type
    let field_infos: Vec<FieldInfo> = fields
        .iter()
        .map(|f| {
            let mut optional = false;
            let field_name = f.ident.as_ref().expect("Expected named field").to_string();
            let type_name: String;
            let is_geom_field = is_geom_field(&f);
            match &f.ty {
                syn::Type::Reference(r) => {
                    type_name = get_reference_type_name(r);
                }
                syn::Type::Path(tp) => {
                    (type_name, optional) = get_path_type_name(tp);
                }
                _ => panic!("Don't know how to map to GPKG type {:?}", f.ty),
            }
            let sql_type = match type_name.as_str() {
                "bool" => quote!(INTEGER),
                "String" | "str" => quote!(TEXT),
                "i64" | "i32" | "i16" | "i8" => quote!(INTEGER),
                "f64" | "f32" => quote!(REAL),
                "buf" => quote!(BLOB),
                "u128" | "u64" | "u32" | "u16" | "u8" => {
                    panic!("SQLite doesn't support unsigned integers, use a signed integer value")
                }
                _ if GEO_TYPES.contains(&type_name.as_str()) => quote!(BLOB),
                _ => panic!("Don't know how to map to SQL type {}", type_name),
            };
            let geom_info = match is_geom_field {
                true => Some(GeomInfo {
                    geom_type: type_name.clone(),
                    // default wgs84
                    srs_id: 4326,
                    m: MZOptions::Prohibited,
                    z: MZOptions::Prohibited,
                }),
                false => None,
            };
            FieldInfo {
                name: field_name,
                optional,
                geom_info,
                type_for_sql: sql_type.to_string(),
            }
        })
        .collect();
    let geom_fields: Vec<&FieldInfo> = field_infos
        .iter()
        .filter(|f| f.geom_info.is_some())
        .collect();
    assert!(
        geom_fields.len() <= 1,
        "Found {} geometry fields, 1 is the maximum allowed amount",
        geom_fields.len()
    );
    let mut geom_column_sql: Option<String> = None;
    let mut contents_sql = format!(
        r#"INSERT INTO gpkg_contents (table_name, data_type) VALUES ("{}", "{}");"#,
        table_name_final, "attributes"
    );

    if geom_fields.len() > 0 {
        let geom_field = geom_fields[0];
        let geom_info = geom_field.geom_info.clone().unwrap();
        geom_field_name = geom_field.name.clone();
        geom_column_sql = Some(format!(
            r#"INSERT INTO gpkg_geometry_columns VALUES("{}", "{}", "{}", {}, {}, {});"#,
            table_name_final,
            geom_field_name,
            geom_info.geom_type.to_uppercase(),
            geom_info.srs_id,
            geom_info.m as i32,
            geom_info.z as u32
        ));
        contents_sql = format!(
            r#"INSERT INTO gpkg_contents (table_name, data_type, srs_id) VALUES ("{}", "{}", {});"#,
            table_name_final, "features", geom_info.srs_id
        );
        println!("{}", geom_column_sql.unwrap());
    };
    println!("{}", contents_sql);

    let column_defs = field_infos
        .iter()
        .map(|f| {
            let null_str = if f.optional { "" } else { " NOT NULL" };
            format!("{} {}{}", f.name, f.type_for_sql, null_str)
                .parse()
                .unwrap()
        })
        .collect::<Vec<TokenStream>>();

    let new = quote!(
        impl GPKGModel for #name {
            fn create_table(&self, gpkg: &GeoPackage) -> String {
                std::stringify!(
                    CREATE TABLE #table_name_final (
                        id INTEGER AUTOINCREMENT PRIMARY KEY,
                        #(#column_defs ),*
                    );
                )
            }
        }
    );
    new
}

fn is_geom_field(field: &Field) -> bool {
    for attr in &field.attrs {
        if let Some(ident) = attr.path.get_ident() {
            if ident.to_string() == "geom_field" {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod test {
    use super::*;
    use quote::quote;
    #[test]
    fn basic_test() {
        let tstream = quote!(
            #[table_name = "streetlights"]
            // #[test_thing = "blah"]
            struct StreetLight {
                id: i64,
                height: f64,
                string_ref: &'a str,
                buf_ref: &'a [u8],
                #[geom_field]
                geom: geo_types::Polygon<f64>,
                // test_blob: [u8],
            }
        );
        println!("{}", derive_gpkg_inner(tstream.into()));
    }
}
