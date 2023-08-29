use ruff_python_trivia::textwrap::dedent;

use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    AngleBracketedGenericArguments, Attribute, Data, DataStruct, DeriveInput, ExprLit, Field,
    Fields, Lit, LitStr, Path, PathArguments, PathSegment, Token, Type, TypePath,
};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let mut output = vec![];

            for field in &fields.named {
                let docs: Vec<&Attribute> = field
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("doc"))
                    .collect();

                if docs.is_empty() {
                    return Err(syn::Error::new(
                        field.span(),
                        "Missing documentation for field",
                    ));
                }

                if let Some(attr) = field
                    .attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("option"))
                {
                    output.push(handle_option(field, attr, docs)?);
                };

                if field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("option_group"))
                {
                    output.push(handle_option_group(field)?);
                };
            }

            let options_len = output.len();

            Ok(quote! {

                impl #ident {
                    pub const fn metadata() -> crate::options_base::OptionGroup {
                        const OPTIONS: [(&'static str, crate::options_base::OptionEntry); #options_len] = [#(#output),*];
                        crate::options_base::OptionGroup::new(&OPTIONS)
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
    let ident = field
        .ident
        .as_ref()
        .expect("Expected to handle named fields");

    match &field.ty {
        Type::Path(TypePath {
            path: Path { segments, .. },
            ..
        }) => match segments.first() {
            Some(PathSegment {
                ident: type_ident,
                arguments:
                    PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }),
            }) if type_ident == "Option" => {
                let path = &args[0];
                let kebab_name = LitStr::new(&ident.to_string().replace('_', "-"), ident.span());

                Ok(quote_spanned!(
                    ident.span() => (#kebab_name, crate::options_base::OptionEntry::Group(#path::metadata()))
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

/// Parse a `doc` attribute into it a string literal.
fn parse_doc(doc: &Attribute) -> syn::Result<String> {
    match &doc.meta {
        syn::Meta::NameValue(syn::MetaNameValue {
            value:
                syn::Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str),
                    ..
                }),
            ..
        }) => Ok(lit_str.value()),
        _ => Err(syn::Error::new(doc.span(), "Expected doc attribute.")),
    }
}

/// Parse an `#[option(doc="...", default="...", value_type="...",
/// example="...")]` attribute and return data in the form of an `OptionField`.
fn handle_option(
    field: &Field,
    attr: &Attribute,
    docs: Vec<&Attribute>,
) -> syn::Result<proc_macro2::TokenStream> {
    // Convert the list of `doc` attributes into a single string.
    let doc = dedent(
        &docs
            .into_iter()
            .map(parse_doc)
            .collect::<syn::Result<Vec<_>>>()?
            .join("\n"),
    )
    .trim_matches('\n')
    .to_string();

    let ident = field
        .ident
        .as_ref()
        .expect("Expected to handle named fields");

    let FieldAttributes {
        default,
        value_type,
        example,
    } = attr.parse_args::<FieldAttributes>()?;
    let kebab_name = LitStr::new(&ident.to_string().replace('_', "-"), ident.span());

    Ok(quote_spanned!(
        ident.span() => (#kebab_name, crate::options_base::OptionEntry::Field(crate::options_base::OptionField {
            doc: &#doc,
            default: &#default,
            value_type: &#value_type,
            example: &#example,
        }))
    ))
}

#[derive(Debug)]
struct FieldAttributes {
    default: String,
    value_type: String,
    example: String,
}

impl Parse for FieldAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let default = _parse_key_value(input, "default")?;
        input.parse::<Comma>()?;
        let value_type = _parse_key_value(input, "value_type")?;
        input.parse::<Comma>()?;
        let example = _parse_key_value(input, "example")?;
        if !input.is_empty() {
            input.parse::<Comma>()?;
        }

        Ok(Self {
            default,
            value_type,
            example: dedent(&example).trim_matches('\n').to_string(),
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
