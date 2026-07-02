use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Field, Token};
use synstructure::{AddBounds, Structure};

pub(crate) fn derive_cache_key(item: &DeriveInput) -> syn::Result<TokenStream> {
    if matches!(item.data, Data::Union(_)) {
        return Err(Error::new(
            item.span(),
            "CacheKey does not support unions. Only structs and enums are supported",
        ));
    }

    let mut structure = Structure::try_new(item)?;

    if matches!(item.data, Data::Struct(_)) {
        let mut attribute_error: Option<Error> = None;
        structure.filter(|binding| match cache_key_field_attribute(binding.ast()) {
            Ok(attributes) => !attributes.is_some_and(|attributes| attributes.ignore),
            Err(error) => {
                if let Some(attribute_error) = &mut attribute_error {
                    attribute_error.combine(error);
                } else {
                    attribute_error = Some(error);
                }
                true
            }
        });
        if let Some(error) = attribute_error {
            return Err(error);
        }
    }

    let is_enum = matches!(item.data, Data::Enum(_));
    let arms = structure
        .variants()
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let pattern = variant.pat();
            let fields = variant
                .bindings()
                .iter()
                .map(|binding| quote!(#binding.cache_key(key);));
            let discriminant = is_enum.then(|| quote!(key.write_usize(#index);));

            quote! {
                #pattern => {
                    #discriminant
                    #(#fields)*
                }
            }
        });

    let fields = quote! {
        match *self {
            #(#arms)*
        }
    };

    let name = &item.ident;
    let mut generics = item.generics.clone();
    structure.add_trait_bounds(
        &syn::parse_quote!(ruff_cache::CacheKey),
        &mut generics.where_clause,
        AddBounds::Fields,
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote!(
        #[automatically_derived]
        impl #impl_generics ruff_cache::CacheKey for #name #ty_generics #where_clause {
            fn cache_key(&self, key: &mut ruff_cache::CacheKeyHasher) {
                use std::hash::Hasher;
                use ruff_cache::CacheKey;
                #fields
            }
        }
    ))
}

fn cache_key_field_attribute(field: &Field) -> syn::Result<Option<CacheKeyFieldAttributes>> {
    if let Some(attribute) = field
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("cache_key"))
    {
        attribute.parse_args::<CacheKeyFieldAttributes>().map(Some)
    } else {
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct CacheKeyFieldAttributes {
    ignore: bool,
}

impl Parse for CacheKeyFieldAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attributes = CacheKeyFieldAttributes::default();

        let args = input.parse_terminated(Ident::parse, Token![,])?;

        for arg in args {
            match arg.to_string().as_str() {
                "ignore" => {
                    attributes.ignore = true;
                }
                name => {
                    return Err(Error::new(
                        arg.span(),
                        format!("Unknown `cache_field` argument {name}"),
                    ));
                }
            }
        }

        Ok(attributes)
    }
}
