use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse2, parse_macro_input, Attribute, DeriveInput, Field, GenericArgument, Ident, Lit, Meta,
    Type,
};

// #[proc_macro_derive(GPKGModel)]
fn derive_gpkg(input: TokenStream) -> TokenStream {
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
    dbg!(&tbl_name);

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
        .filter_map(|attr| {
            let out = attr.parse_meta().ok();
            attr.parse_meta().ok()
        })
        .filter(|i| match i.path().get_ident() {
            Some(i) => i.to_string() == name.to_owned(),
            None => false,
        })
        .collect::<Vec<Meta>>();
    // we can call unwrap here because we used .ok() in the filter map
    temp.pop()
}

fn impl_model(name: &Ident, fields: &Vec<&Field>, tbl_name: Option<String>) -> TokenStream {
    let field_names: Vec<_> = fields
        .iter()
        .map(|f| f.ident.as_ref().expect("expected named field"))
        .collect();

    // overwrite the struct name with a provided table name if one is given
    // TODO: add some level of validation here based on sqlite's rules
    let table_name_final = match tbl_name {
        Some(n) => Ident::new(&n, name.span()),
        None => name.to_owned(),
    };

    // the goal is to support everything here (https://www.geopackage.org/spec130/index.html#table_column_data_types)
    // as well as allow the user change whether a field can have nulls or not with the option type
    for f in fields.iter() {
        match &f.ty {
            syn::Type::Path(tp) => {
                let mut optional = false;
                let mut type_name = String::new();
                match tp.path.get_ident() {
                    Some(ident) => {
                        type_name = ident.to_string();
                    }
                    // need to get check if this is an option or a vec
                    None => {
                        let wrapper = &tp.path.segments[0].ident;
                        if wrapper.to_string() == "Option" {
                            optional = true;
                            // now need to get the inside
                            match &tp.path.segments[0].arguments {
                                syn::PathArguments::AngleBracketed(ab) => {
                                    assert!(ab.args.len() == 1);
                                    match &ab.args[0] {
                                        GenericArgument::Type(t) => match t {
                                            Type::Path(tp) => {
                                                type_name = (tp.path.segments[0].ident).to_string();
                                                // dbg!(&tp.path.segments[0].ident);
                                            }
                                            _ => panic!(
                                                "Got something that wasn't a type within an Option"
                                            ),
                                        },
                                        _ => {
                                            panic!(
                                                "Unsupported type within Option: {:?}",
                                                &ab.args[0]
                                            )
                                        }
                                    }
                                }
                                _ => {
                                    panic!("Unsupported type. TODO add a more informative message here")
                                }
                            }
                        }
                    }
                };
                println!("Field name: {}, optional: {}", type_name, optional)
            }
            _ => panic!("Don't know how to map to SQL type {:?}", f.ty),
        }
        // Some(ident) => match ident.to_string().as_ref() {
        //     "bool" => quote!(BOOL NOT NULL),
        //     "String" => quote!(TEXT NOT NULL),
        //     "i64" | "i32" | "i16" | "i8" => quote!(INTEGER NOT NULL),
        //     "f64" | "f32" => quote!(REAL NOT NULL),
        //     "u128" | "u64" | "u32" | "u16" | "u8" => panic!("SQLite doesn't support unsigned integers, use a signed integer value"),
        //     _ => panic!("Don't know how to map to SQL type {}", ident.to_string()),
        // },
    }

    let sql_types = fields.iter().map(|f| {
        // for attr in &f.attrs
        //     if let Some(attrident) = attr.path.get_ident() {
        //         if attrident.to_string() == "sql_type" {
        //             return attr.parse_args().expect("failed to read sql_type");
        //         }
        //     }
        // }
        println!("Test");
    });

    let new = quote!(
        impl GPKGModel for #name {
            fn create_table(&self) -> String {
                std::stringify!(
                    CREATE TABLE #table_name_final (
                        id INTEGER AUTOINCREMENT PRIMARY KEY,
                        #(#field_names),*
                    );
                )
            }
        }
    );
    new
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
                needs_replace: Option<i8>,
                needs_replace2: Option<String>,
                // test_blob: [u8],
            }
        );
        println!("{}", derive_gpkg(tstream.into()));
    }
}
