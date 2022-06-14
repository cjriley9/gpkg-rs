#![allow(dead_code)]
use lazy_static::lazy_static;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::collections::HashMap;
use std::ops::Deref;
use syn::{
    parse2, Attribute, DeriveInput, Field, GenericArgument, GenericParam, Generics, Ident, Lit,
    LitInt, Meta, MetaNameValue, Type, TypePath, TypeReference,
};

lazy_static! {
    static ref GEO_TYPES: HashMap<&'static str, (MZOptions, MZOptions)> = {
        let mut m = HashMap::new();
        m.insert("POLYGON", (MZOptions::Prohibited, MZOptions::Prohibited));
        m.insert("LINESTRING", (MZOptions::Prohibited, MZOptions::Prohibited));
        m.insert("POINT", (MZOptions::Prohibited, MZOptions::Prohibited));
        m.insert(
            "MULTIPOLYGON",
            (MZOptions::Prohibited, MZOptions::Prohibited),
        );
        m.insert(
            "MULTILINESTRING",
            (MZOptions::Prohibited, MZOptions::Prohibited),
        );
        m.insert("MULTIPOINT", (MZOptions::Prohibited, MZOptions::Prohibited));
        m.insert("POLYGONM", (MZOptions::Mandatory, MZOptions::Prohibited));
        m.insert("LINESTRINGM", (MZOptions::Mandatory, MZOptions::Prohibited));
        m.insert("POINTM", (MZOptions::Mandatory, MZOptions::Prohibited));
        m.insert(
            "MULTIPOLYGONM",
            (MZOptions::Mandatory, MZOptions::Prohibited),
        );
        m.insert(
            "MULTILINESTRINGM",
            (MZOptions::Mandatory, MZOptions::Prohibited),
        );
        m.insert("MULTIPOINTM", (MZOptions::Mandatory, MZOptions::Prohibited));
        m.insert("POLYGONZ", (MZOptions::Prohibited, MZOptions::Mandatory));
        m.insert("LINESTRINGZ", (MZOptions::Prohibited, MZOptions::Mandatory));
        m.insert("POINTZ", (MZOptions::Prohibited, MZOptions::Mandatory));
        m.insert(
            "MULTIPOLYGONZ",
            (MZOptions::Prohibited, MZOptions::Mandatory),
        );
        m.insert(
            "MULTILINESTRINGZ",
            (MZOptions::Prohibited, MZOptions::Mandatory),
        );
        m.insert("MULTIPOINTZ", (MZOptions::Prohibited, MZOptions::Mandatory));
        m.insert("POLYGONZM", (MZOptions::Mandatory, MZOptions::Mandatory));
        m.insert("LINESTRINGZM", (MZOptions::Mandatory, MZOptions::Mandatory));
        m.insert("POINTZM", (MZOptions::Mandatory, MZOptions::Mandatory));
        m.insert(
            "MULTIPOLYGONZM",
            (MZOptions::Mandatory, MZOptions::Mandatory),
        );
        m.insert(
            "MULTILINESTRINGZM",
            (MZOptions::Mandatory, MZOptions::Mandatory),
        );
        m.insert("MULTIPOINTZM", (MZOptions::Mandatory, MZOptions::Mandatory));
        m
    };
}

/// A macro for deriving an implementation of GPKGModel for a struct
///
/// The layer_name attribute controls the name of the SQLite table that instances of this Struct will be read and written as
///
/// The geom_field attribute can only be used on one field, and the geometry type will be cast to uppercase
/// the used as the geomtry type for the layer. If the letters Z and/or M are present in the geometry type,
/// the corresponding flags will be set within the GeoPackage indicating that the geometry has M or Z values.
///
/// When this macro is used, an "object_id" primary key column will be created in order to comply with the specifcation,
/// but will be transparent to you as a user of this crate
///
/// When using this macro for reading an existing GeoPackage layer, any unspecified columns will not be read.
/// # Usage
/// ```ignore
/// # // would be great to get this test working, but I'm not sure how to do it without curculare dependency issues
/// # use gpkg_derive::GPKGModel;
/// # use gpkg::types::{GPKGPoint, GPKGPointZ};
///
/// #[derive(GPKGModel)]
/// #[layer_name = "test_table"]
/// struct TestTable {
///     field1: i64,
///     field2: i32,
///     #[geom_field("Point")]
///     shape: GPKGPoint,
/// }
///
/// #[derive(GPKGModel)]
/// #[layer_name = "test_tableZ"]
/// struct TestTableZ {
///     field1: i64,
///     field2: i32,
///     #[geom_field("PointZ")]
///     shape: GPKGPointZ,
/// }
#[proc_macro_derive(GPKGModel, attributes(layer_name, geom_field))]
pub fn derive_gpkg(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let inner_input = proc_macro2::TokenStream::from(input);
    proc_macro::TokenStream::from(derive_gpkg_inner(inner_input))
}

fn derive_gpkg_inner(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ast = parse2::<DeriveInput>(input).unwrap();

    let tbl_name_meta = get_meta_attr(&ast.attrs, "layer_name");
    let tbl_name = tbl_name_meta.and_then(|meta| match meta {
        Meta::NameValue(MetaNameValue {
            lit: Lit::Str(ls), ..
        }) => Some(ls.value()),
        _ => None,
    });

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

    impl_model(&name.clone(), &fields, tbl_name, &ast.generics)
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
        _ => {}
    }

    (final_segment.ident.to_string(), false)
}

fn impl_model(
    name: &Ident,
    fields: &Vec<&Field>,
    tbl_name: Option<String>,
    generics: &Generics,
) -> TokenStream {
    // overwrite the struct name with a provided table name if one is given
    // TODO: add some level of validation here based on sqlite's rules
    let layer_name_final = match tbl_name {
        Some(n) => Ident::new(&n, name.span()),
        None => name.to_owned(),
    };

    let geom_field_name: String;

    // need to get this in order to make liftimes on the Impl work correctly
    let mut final_generics = generics.clone();
    if let Some(g) = final_generics.params.first_mut() {
        match g {
            GenericParam::Lifetime(l) => match l.lifetime.ident.to_string().as_str() {
                "static" | "_" => {}
                _ => l.lifetime.ident = Ident::new("_", Span::call_site()),
            },
            _ => {}
        }
    }

    // the goal is to support everything here (https://www.geopackage.org/spec130/index.html#table_column_data_types)
    // as well as allow the user change whether a field can have nulls or not with the option type
    let field_infos: Vec<FieldInfo> = fields
        .iter()
        .map(|f| {
            let mut optional = false;
            let field_name = f.ident.as_ref().expect("Expected named field").to_string();
            let type_name: String;
            let geom_info = get_geom_field_info(&f);
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
                // all geometry types are a blob inside sqlite
                _ if geom_info.is_some() => quote!(BLOB),
                _ => panic!("Don't know how to map to SQL type {}", type_name),
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
        layer_name_final, "attributes"
    );

    if geom_fields.len() > 0 {
        let geom_field = geom_fields[0];
        let geom_info = geom_field.geom_info.clone().unwrap();
        let geom_type_sql = geom_info.geom_type.clone();
        geom_field_name = geom_field.name.clone();
        geom_column_sql = Some(format!(
            r#"INSERT INTO gpkg_geometry_columns VALUES("{}", "{}", "{}", {}, {}, {});"#,
            layer_name_final,
            geom_field_name,
            geom_type_sql.to_uppercase(),
            geom_info.srs_id,
            geom_info.m as i32,
            geom_info.z as i32
        ));
        contents_sql = format!(
            r#"INSERT INTO gpkg_contents (table_name, data_type, srs_id) VALUES ("{}", "{}", {});"#,
            layer_name_final, "features", geom_info.srs_id
        );
    };
    let contents_ts: TokenStream = contents_sql
        .parse()
        .expect("Unable to convert contents table insert statement into token stream");
    let geom_column_ts: TokenStream = match geom_column_sql {
        Some(s) => s
            .parse()
            .expect("Unable to convert contents table insert statement into token stream"),
        None => TokenStream::new(),
    };

    let column_defs = field_infos
        .iter()
        .map(|f| {
            let null_str = if f.optional { "" } else { " NOT NULL" };
            format!("{} {}{}", f.name, f.type_for_sql, null_str)
                .parse()
                .unwrap()
        })
        .collect::<Vec<TokenStream>>();

    let column_names: Vec<Ident> = field_infos
        .iter()
        .map(|i| Ident::new(i.name.as_str(), Span::call_site()))
        .collect();

    let params = vec![quote!(?); column_names.len()];

    let column_nums = (0..column_defs.len())
        .map(|i| LitInt::new(i.to_string().as_str(), Span::call_site()))
        .collect::<Vec<LitInt>>();

    // need to add some generic support like in here: https://github.com/diesel-rs/diesel/blob/master/diesel_derives/src/insertable.rs#L88
    // this is so that lifetimes will work
    let new = quote!(
        impl GPKGModel <'_> for #name #final_generics {
            #[inline]
            fn get_gpkg_layer_name() -> &'static str {
                std::stringify!(#layer_name_final)
            }

            #[inline]
            fn get_create_sql() -> &'static str {
                std::stringify!(
                    BEGIN;
                    CREATE TABLE #layer_name_final (
                        object_id INTEGER PRIMARY KEY,
                        #(#column_defs ),*
                    );
                    #geom_column_ts
                    #contents_ts
                    COMMIT;
                )
            }

            #[inline]
            fn get_insert_sql() -> &'static str {
                std::stringify!(
                    INSERT INTO #layer_name_final (
                        #(#column_names),*
                    ) VALUES (
                        #(#params),*
                    )
                )
            }

            #[inline]
            fn get_select_sql() -> &'static str {
                std::stringify!(
                    SELECT #(#column_names),* FROM #layer_name_final;
                )
            }

            #[inline]
            fn get_select_where(predicate: &str) -> String {
                (std::stringify!(
                    SELECT #(#column_names),* FROM #layer_name_final WHERE
                ).to_owned() + " " + predicate + ";")
            }

            fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
                Ok(Self {
                    #(#column_names: row.get((#column_nums))?,)*
                })
            }

            fn as_params(&self) -> Vec<&(dyn rusqlite::ToSql + '_)> {
                vec![
                    #(&self.#column_names as &dyn rusqlite::ToSql),*
                ]
            }
        }
    );
    new
}

fn get_geom_field_info(field: &Field) -> Option<GeomInfo> {
    for attr in &field.attrs {
        if let Some(ident) = attr.path.get_ident() {
            if ident.to_string() == "geom_field" {
                let geom_type_name =
                    get_meta_attr(&field.attrs, "geom_field").and_then(|meta| match meta {
                        Meta::List(l) => l.nested.first().and_then(|n| match n {
                            syn::NestedMeta::Lit(Lit::Str(ls)) => Some(ls.value()),
                            _ => panic!("You must specify a geometry type when using the geom_field attribute"),
                        }),
                        _ => panic!("You must specify a geometry type when using the geom_field attribute"),
                    });
                if let Some(name) = geom_type_name {
                    let upper_name = name.to_uppercase();
                    if let Some((m, z)) = GEO_TYPES.get((&upper_name).as_str()) {
                        return Some(GeomInfo {
                            geom_type: upper_name,
                            srs_id: 4326,
                            m: *m,
                            z: *z,
                        });
                    } else {
                        panic!("{} is not a supported geometry type", name);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;
    use quote::quote;
    #[test]
    fn basic_test() {
        let tstream = quote!(
            #[layer_name = "streetlights"]
            // #[test_thing = "blah"]
            struct StreetLight {
                id: i64,
                height: f64,
                string_ref: Option<String>,
                buf_ref: &'a [u8],
                #[geom_field("LineStringZ")]
                geom: GPKGLineStringZ,
            }
        );
        println!("{}", derive_gpkg_inner(tstream.into()));
    }
}
