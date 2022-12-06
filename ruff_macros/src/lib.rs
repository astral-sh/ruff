#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::token::Comma;
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, Attribute, Data, DataStruct, DeriveInput,
    Field, Fields, Lit, LitStr, Path, PathArguments, PathSegment, Token, Type, TypePath,
};

#[proc_macro_derive(ConfigurationOptions, attributes(option, option_group))]
pub fn derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    derive_impl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let mut output = vec![];

            for field in fields.named.iter() {
                if let Some(attr) = field.attrs.iter().find(|a| a.path.is_ident("option")) {
                    output.push(handle_option(field, attr)?);
                };

                if field.attrs.iter().any(|a| a.path.is_ident("option_group")) {
                    output.push(handle_option_group(field)?);
                };
            }

            Ok(quote! {
              use crate::settings::options_base::{OptionEntry, OptionField, OptionGroup, ConfigurationOptions};

              #[automatically_derived]
              impl ConfigurationOptions for #ident {
                  fn get_available_options() -> Vec<OptionEntry> {
                      vec![#(#output),*]
                  }
              }
            })
        }
        _ => Err(syn::Error::new(
            ident.span(),
            "Can only derive ConfigurationOptions from structs with named fields.",
        )),
    }
}

/// For a field with type `Option<Foobar>` where `Foobar` itself is a struct
/// deriving `ConfigurationOptions`, create code that calls retrieves options
/// from that group: `Foobar::get_available_options()`
fn handle_option_group(field: &Field) -> syn::Result<proc_macro2::TokenStream> {
    // unwrap is safe because we're only going over named fields
    let ident = field.ident.as_ref().unwrap();

    match &field.ty {
        Type::Path(TypePath {
            path: Path { segments, .. },
            ..
        }) => match segments.first() {
            Some(PathSegment {
                ident: type_ident,
                arguments:
                    PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }),
                ..
            }) if type_ident == "Option" => {
                let path = &args[0];
                let kebab_name = LitStr::new(&ident.to_string().replace('_', "-"), ident.span());

                Ok(quote_spanned!(
                    ident.span() => OptionEntry::Group(OptionGroup {
                        name: #kebab_name,
                        fields: #path::get_available_options(),
                    })
                ))
            }
            _ => Err(syn::Error::new(
                ident.span(),
                "Expected `Option<_>`  as type.",
            )),
        },
        _ => Err(syn::Error::new(ident.span(), "Expected type.")),
    }
}

/// Parse an `#[option(doc="...", default="...", value_type="...",
/// example="...")]` attribute and return data in the form of an `OptionField`.
fn handle_option(field: &Field, attr: &Attribute) -> syn::Result<proc_macro2::TokenStream> {
    // unwrap is safe because we're only going over named fields
    let ident = field.ident.as_ref().unwrap();

    let FieldAttributes {
        doc,
        default,
        value_type,
        example,
    } = attr.parse_args::<FieldAttributes>()?;
    let kebab_name = LitStr::new(&ident.to_string().replace('_', "-"), ident.span());

    Ok(quote_spanned!(
        ident.span() => OptionEntry::Field(OptionField {
            name: #kebab_name,
            doc: &#doc,
            default: &#default,
            value_type: &#value_type,
            example: &#example,
        })
    ))
}

#[derive(Debug)]
struct FieldAttributes {
    doc: String,
    default: String,
    value_type: String,
    example: String,
}

impl Parse for FieldAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let doc = _parse_key_value(input, "doc")?;
        input.parse::<Comma>()?;
        let default = _parse_key_value(input, "default")?;
        input.parse::<Comma>()?;
        let value_type = _parse_key_value(input, "value_type")?;
        input.parse::<Comma>()?;
        let example = _parse_key_value(input, "example")?;
        if !input.is_empty() {
            input.parse::<Comma>()?;
        }

        Ok(FieldAttributes {
            doc: textwrap::dedent(&doc).trim_matches('\n').to_string(),
            default,
            value_type,
            example: textwrap::dedent(&example).trim_matches('\n').to_string(),
        })
    }
}

fn _parse_key_value(input: ParseStream, name: &str) -> syn::Result<String> {
    let ident: proc_macro2::Ident = input.parse()?;
    if ident != name {
        return Err(syn::Error::new(
            ident.span(),
            format!("Expected `{name}` name"),
        ));
    }

    input.parse::<Token![=]>()?;
    let value: Lit = input.parse()?;

    match &value {
        Lit::Str(v) => Ok(v.value()),
        _ => Err(syn::Error::new(value.span(), "Expected literal string")),
    }
}
