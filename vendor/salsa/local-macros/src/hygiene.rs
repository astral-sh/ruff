use std::collections::HashSet;

use quote::ToTokens;

pub struct Hygiene {
    user_tokens: HashSet<String>,
}

impl Hygiene {
    pub fn from1(tokens: &proc_macro::TokenStream) -> Self {
        let mut user_tokens = HashSet::new();
        push_idents1(tokens.clone(), &mut user_tokens);
        Self { user_tokens }
    }

    pub fn from2(tokens: &impl ToTokens) -> Self {
        let mut user_tokens = HashSet::new();
        push_idents2(tokens.to_token_stream(), &mut user_tokens);
        Self { user_tokens }
    }
}

fn push_idents1(input: proc_macro::TokenStream, user_tokens: &mut HashSet<String>) {
    input.into_iter().for_each(|token| match token {
        proc_macro::TokenTree::Group(g) => {
            push_idents1(g.stream(), user_tokens);
        }
        proc_macro::TokenTree::Ident(ident) => {
            user_tokens.insert(ident.to_string());
        }
        proc_macro::TokenTree::Punct(_) => (),
        proc_macro::TokenTree::Literal(_) => (),
    })
}

fn push_idents2(input: proc_macro2::TokenStream, user_tokens: &mut HashSet<String>) {
    input.into_iter().for_each(|token| match token {
        proc_macro2::TokenTree::Group(g) => {
            push_idents2(g.stream(), user_tokens);
        }
        proc_macro2::TokenTree::Ident(ident) => {
            user_tokens.insert(ident.to_string());
        }
        proc_macro2::TokenTree::Punct(_) => (),
        proc_macro2::TokenTree::Literal(_) => (),
    })
}

impl Hygiene {
    /// Generates an identifier similar to `text` but
    /// distinct from any identifiers that appear in the user's
    /// code.
    pub(crate) fn ident(&self, text: impl AsRef<str>) -> syn::Ident {
        // Make the default be `foo_` rather than `foo` -- this helps detect
        // cases where people wrote `foo` instead of `#foo` or `$foo` in the generated code.
        let mut buffer = format!("{}_", text.as_ref());

        while self.user_tokens.contains(&buffer) {
            buffer.push('_');
        }

        syn::Ident::new(&buffer, proc_macro2::Span::call_site())
    }

    /// Generates an identifier similar to `text` but distinct from any identifiers
    /// that appear in the user's code.
    ///
    /// The identifier must be unique relative to the `scope` identifier.
    pub(crate) fn scoped_ident(&self, scope: &syn::Ident, text: &str) -> syn::Ident {
        self.ident(format!("{scope}_{text}"))
    }
}
