use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, parse_macro_input, Attribute, DeriveInput, Field, Ident, Lit, Meta};

// #[proc_macro_derive(GPKGModel)]
fn derive_gpkg(input: TokenStream) -> TokenStream {
    let ast = parse2::<DeriveInput>(input).unwrap();
    // let ast = parse_macro_input!(input as DeriveInput);

    let tbl_name_meta = get_meta_attr(&ast.attrs, "table_name");
    let tbl_name = match tbl_name_meta {
        Meta::NameValue(nv) => match nv.lit {
            Lit::Str(ls) => Some(ls.value()),
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

fn get_meta_attr<'a>(attrs: &Vec<Attribute>, name: &'a str) -> Meta {
    let mut temp = attrs
        .iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .collect::<Vec<Meta>>();
    temp.pop().unwrap()
}

fn impl_model(name: &Ident, fields: &Vec<&Field>, tbl_name: Option<String>) -> TokenStream {
    let field_names: Vec<_> = fields
        .iter()
        .map(|f| f.ident.as_ref().expect("expected named field"))
        .collect();

    // check table name
    let table_name_final = match tbl_name {
        Some(n) => Ident::new(&n, name.span()),
        None => name.to_owned(),
    };

    let out = TokenStream::new();

    let new = quote!(
        impl GPKGModel for #name {
            fn create_table(&self) -> String {
                std::stringify!(
                    CREATE TABLE #table_name_final (
                        #(#field_names),*
                    )
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
            struct StreetLight {
                id: i64,
                height: f64,
            }
        );
        println!("{}", derive_gpkg(tstream.into()));
    }
}
