use proc_macro2::{TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{
    AngleBracketedGenericArguments, Attribute, Data, DataStruct, DeriveInput, ExprLit, Field,
    Fields, Lit, LitStr, Meta, Path, PathArguments, PathSegment, Type, TypePath,
};

use ruff_python_trivia::textwrap::dedent;

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    let DeriveInput {
        ident,
        data,
        attrs: struct_attributes,
        ..
    } = input;

    match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let mut output = vec![];

            for field in &fields.named {
                if let Some(attr) = field
                    .attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("option"))
                {
                    output.push(handle_option(field, attr)?);
                } else if field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("option_group"))
                {
                    output.push(handle_option_group(field)?);
                } else if let Some(serde) = field
                    .attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("serde"))
                {
                    // If a field has the `serde(flatten)` attribute, flatten the options into the parent
                    // by calling `Type::record` instead of `visitor.visit_set`
                    if let (Type::Path(ty), Meta::List(list)) = (&field.ty, &serde.meta) {
                        for token in list.tokens.clone() {
                            if let TokenTree::Ident(ident) = token {
                                if ident == "flatten" {
                                    let ty_name = ty.path.require_ident()?;
                                    output.push(quote_spanned!(
                                        ident.span() => (#ty_name::record(visit))
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let docs: Vec<&Attribute> = struct_attributes
                .iter()
                .filter(|attr| attr.path().is_ident("doc"))
                .collect();

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

            let documentation = if doc.is_empty() {
                None
            } else {
                Some(quote!(
                    fn documentation() -> Option<&'static str> {
                        Some(&#doc)
                    }
                ))
            };

            Ok(quote! {
                impl crate::options_base::OptionsMetadata for #ident {
                    fn record(visit: &mut dyn crate::options_base::Visit) {
                        #(#output);*
                    }

                    #documentation
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
                    ident.span() => (visit.record_set(#kebab_name, crate::options_base::OptionSet::of::<#path>()))
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
fn handle_option(field: &Field, attr: &Attribute) -> syn::Result<proc_macro2::TokenStream> {
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
    } = parse_field_attributes(attr)?;
    let kebab_name = LitStr::new(&ident.to_string().replace('_', "-"), ident.span());

    let deprecated = if let Some(deprecated) = field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("deprecated"))
    {
        fn quote_option(option: Option<String>) -> TokenStream {
            match option {
                None => quote!(None),
                Some(value) => quote!(Some(#value)),
            }
        }

        let deprecated = parse_deprecated_attribute(deprecated)?;
        let note = quote_option(deprecated.note);
        let since = quote_option(deprecated.since);

        quote!(Some(crate::options_base::Deprecated { since: #since, message: #note }))
    } else {
        quote!(None)
    };

    Ok(quote_spanned!(
        ident.span() => {
            visit.record_field(#kebab_name, crate::options_base::OptionField{
                doc: &#doc,
                default: &#default,
                value_type: &#value_type,
                example: &#example,
                deprecated: #deprecated
            })
        }
    ))
}

#[derive(Debug)]
struct FieldAttributes {
    default: String,
    value_type: String,
    example: String,
}

fn parse_field_attributes(attribute: &Attribute) -> syn::Result<FieldAttributes> {
    let mut default = None;
    let mut value_type = None;
    let mut example = None;

    attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("default") {
            default = Some(get_string_literal(&meta, "default", "option")?.value());
        } else if meta.path.is_ident("value_type") {
            value_type = Some(get_string_literal(&meta, "value_type", "option")?.value());
        } else if meta.path.is_ident("example") {
            let example_text = get_string_literal(&meta, "value_type", "option")?.value();
            example = Some(dedent(&example_text).trim_matches('\n').to_string());
        } else {
            return Err(syn::Error::new(
                meta.path.span(),
                format!(
                    "Deprecated meta {:?} is not supported by ruff's option macro.",
                    meta.path.get_ident()
                ),
            ));
        }

        Ok(())
    })?;

    let Some(default) = default else {
        return Err(syn::Error::new(attribute.span(), "Mandatory `default` field is missing in `#[option]` attribute. Specify the default using `#[option(default=\"..\")]`."));
    };

    let Some(value_type) = value_type else {
        return Err(syn::Error::new(attribute.span(), "Mandatory `value_type` field is missing in `#[option]` attribute. Specify the value type using `#[option(value_type=\"..\")]`."));
    };

    let Some(example) = example else {
        return Err(syn::Error::new(attribute.span(), "Mandatory `example` field is missing in `#[option]` attribute. Add an example using `#[option(example=\"..\")]`."));
    };

    Ok(FieldAttributes {
        default,
        value_type,
        example,
    })
}

fn parse_deprecated_attribute(attribute: &Attribute) -> syn::Result<DeprecatedAttribute> {
    let mut deprecated = DeprecatedAttribute::default();
    attribute.parse_nested_meta(|meta| {
        if meta.path.is_ident("note") {
            deprecated.note = Some(get_string_literal(&meta, "note", "deprecated")?.value());
        } else if meta.path.is_ident("since") {
            deprecated.since = Some(get_string_literal(&meta, "since", "deprecated")?.value());
        } else {
            return Err(syn::Error::new(
                meta.path.span(),
                format!(
                    "Deprecated meta {:?} is not supported by ruff's option macro.",
                    meta.path.get_ident()
                ),
            ));
        }

        Ok(())
    })?;

    Ok(deprecated)
}

fn get_string_literal(
    meta: &ParseNestedMeta,
    meta_name: &str,
    attribute_name: &str,
) -> syn::Result<syn::LitStr> {
    let expr: syn::Expr = meta.value()?.parse()?;

    let mut value = &expr;
    while let syn::Expr::Group(e) = value {
        value = &e.expr;
    }

    if let syn::Expr::Lit(ExprLit {
        lit: Lit::Str(lit), ..
    }) = value
    {
        let suffix = lit.suffix();
        if !suffix.is_empty() {
            return Err(syn::Error::new(
                lit.span(),
                format!("unexpected suffix `{suffix}` on string literal"),
            ));
        }

        Ok(lit.clone())
    } else {
        Err(syn::Error::new(
            expr.span(),
            format!("expected {attribute_name} attribute to be a string: `{meta_name} = \"...\"`"),
        ))
    }
}

#[derive(Default, Debug)]
struct DeprecatedAttribute {
    since: Option<String>,
    note: Option<String>,
}
