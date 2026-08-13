use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItem, ItemImpl};

pub(crate) fn attribute_env_vars_metadata(mut input: ItemImpl) -> TokenStream {
    // Verify that this is an impl for EnvVars
    let impl_type = &input.self_ty;

    let mut env_var_entries = Vec::new();
    let mut hidden_vars = Vec::new();

    // Process each item in the impl block
    for item in &mut input.items {
        if let ImplItem::Const(const_item) = item {
            // Extract the const name and value
            let const_name = &const_item.ident;
            let const_expr = &const_item.expr;

            // Check if the const has the #[attr_hidden] attribute
            let is_hidden = const_item
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("attr_hidden"));

            // Remove our custom attributes
            const_item.attrs.retain(|attr| {
                !attr.path().is_ident("attr_hidden")
                    && !attr.path().is_ident("attr_env_var_pattern")
            });

            if is_hidden {
                hidden_vars.push(const_name.clone());
            } else {
                // Extract documentation from doc comments
                let doc_attrs: Vec<_> = const_item
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("doc"))
                    .collect();

                if !doc_attrs.is_empty() {
                    // Convert doc attributes to a single string
                    let doc_string = extract_doc_string(&doc_attrs);
                    env_var_entries.push((const_name.clone(), const_expr.clone(), doc_string));
                }
            }
        }
    }

    // Generate the metadata method.
    let metadata_entries: Vec<_> = env_var_entries
        .iter()
        .map(|(_name, expr, doc)| {
            quote! {
                (#expr, #doc)
            }
        })
        .collect();

    let metadata_impl = quote! {
        impl #impl_type {
            /// Returns metadata for all non-hidden environment variables.
            pub fn metadata() -> Vec<(&'static str, &'static str)> {
                vec![
                    #(#metadata_entries),*
                ]
            }
        }
    };

    quote! {
        #input
        #metadata_impl
    }
}

/// Extract documentation from doc attributes into a single string
fn extract_doc_string(attrs: &[&syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &meta.value
                {
                    return Some(lit_str.value().trim().to_string());
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}
