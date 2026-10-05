//! Common code for `#[salsa::interned]`, `#[salsa::input]`, and
//! `#[salsa::tracked]` decorators.
//!
//! Example of usage:
//!
//! ```rust,ignore
//! #[salsa::interned(jar = Jar0, data = TyData0)]
//! #[derive(Eq, PartialEq, Hash, Debug, Clone)]
//! struct Ty0 {
//!    field1: Type1,
//!    #[ref] field2: Type2,
//!    ...
//! }
//! ```
//! For an interned or entity struct `Foo`, we generate:
//!
//! * the actual struct: `struct Foo(Id);`
//! * constructor function: `impl Foo { fn new(db: &crate::Db, field1: Type1, ..., fieldN: TypeN) -> Self { ... } }
//! * field accessors: `impl Foo { fn field1(&self) -> Type1 { self.field1.clone() } }`
//!     * if the field is `ref`, we generate `fn field1(&self) -> &Type1`
//!
//! Only if there are no `ref` fields:
//!
//! * the data type: `struct FooData { field1: Type1, ... }` or `enum FooData { ... }`
//! * data method `impl Foo { fn data(&self, db: &dyn crate::Db) -> FooData { FooData { f: self.f(db), ... } } }`
//!     * this could be optimized, particularly for interned fields

use proc_macro2::{Ident, Literal, Span, TokenStream};
use syn::{ext::IdentExt, spanned::Spanned};

use crate::db_lifetime;
use crate::options::{AllowedOptions, Options};

pub(crate) struct SalsaStruct<'s, A: SalsaStructAllowedOptions> {
    struct_item: &'s syn::ItemStruct,
    args: &'s Options<A>,
    fields: Vec<SalsaField<'s>>,
}

pub(crate) trait SalsaStructAllowedOptions: AllowedOptions {
    /// The kind of struct (e.g., interned, input, tracked).
    const KIND: &'static str;

    /// Are `#[tracked]` fields allowed?
    const ALLOW_TRACKED: bool;

    /// Does this kind of struct have a `'db` lifetime?
    const HAS_LIFETIME: bool;

    /// Can this struct elide the `'db` lifetime?
    const ELIDABLE_LIFETIME: bool;

    /// Are `#[default]` fields allowed?
    const ALLOW_DEFAULT: bool;

    /// Are manual `#[salsa_value(...)]` retention proofs allowed on fields?
    const ALLOW_MANUAL_RETENTION_PROOF: bool;
}

pub(crate) struct SalsaField<'s> {
    pub(crate) field: &'s syn::Field,

    pub(crate) has_tracked_attr: bool,
    pub(crate) has_default_attr: bool,
    pub(crate) returns: syn::Ident,
    pub(crate) has_no_eq_attr: bool,
    pub(crate) manual_retention_proof: Option<crate::salsa_value::ManualRetentionProof>,
    get_name: syn::Ident,
    set_name: syn::Ident,
    unknown_attrs: Vec<&'s syn::Attribute>,
}

const BANNED_FIELD_NAMES: &[&str] = &["from", "new"];
const ALLOWED_RETURN_MODES: &[&str] = &["copy", "clone", "ref", "deref", "as_ref", "as_deref"];

#[allow(clippy::type_complexity)]
pub(crate) const FIELD_OPTION_ATTRIBUTES: &[(
    &str,
    fn(&syn::Attribute, &mut SalsaField) -> syn::Result<()>,
)] = &[
    ("tracked", |_, ef| {
        ef.has_tracked_attr = true;
        Ok(())
    }),
    ("default", |_, ef| {
        ef.has_default_attr = true;
        Ok(())
    }),
    ("returns", |attr, ef| {
        ef.returns = attr.parse_args_with(syn::Ident::parse_any)?;
        Ok(())
    }),
    ("no_eq", |_, ef| {
        ef.has_no_eq_attr = true;
        Ok(())
    }),
    ("salsa_value", |attr, ef| {
        if ef.manual_retention_proof.is_some() {
            return Err(syn::Error::new_spanned(
                ef.field,
                "multiple `#[salsa_value]` attributes on field",
            ));
        }
        ef.manual_retention_proof = Some(crate::salsa_value::parse_manual_retention_proof(attr)?);
        Ok(())
    }),
    ("get", |attr, ef| {
        ef.get_name = attr.parse_args()?;
        Ok(())
    }),
    ("set", |attr, ef| {
        ef.set_name = attr.parse_args()?;
        Ok(())
    }),
];

impl<'s, A> SalsaStruct<'s, A>
where
    A: SalsaStructAllowedOptions,
{
    pub fn new(struct_item: &'s syn::ItemStruct, args: &'s Options<A>) -> syn::Result<Self> {
        let syn::Fields::Named(n) = &struct_item.fields else {
            return Err(syn::Error::new_spanned(
                &struct_item.ident,
                "must have named fields for a struct",
            ));
        };

        let fields = n
            .named
            .iter()
            .map(SalsaField::new)
            .collect::<syn::Result<_>>()?;

        let this = Self {
            struct_item,
            args,
            fields,
        };

        this.maybe_disallow_tracked_fields()?;
        this.maybe_disallow_default_fields()?;
        this.maybe_disallow_manual_retention_proofs()?;

        this.check_generics()?;

        Ok(this)
    }

    /// Returns the `constructor_name` in `Options` if it is `Some`, else `new`
    pub(crate) fn constructor_name(&self) -> syn::Ident {
        match self.args.constructor_name.clone() {
            Some(name) => name,
            None => Ident::new("new", self.struct_item.ident.span()),
        }
    }

    /// Returns the `id` in `Options` if it is `Some`, else `salsa::Id`.
    pub(crate) fn id(&self) -> syn::Path {
        match &self.args.id {
            Some(id) => id.clone(),
            None => parse_quote!(salsa::Id),
        }
    }

    /// Returns the `revisions` in `Options` as an optional iterator.
    pub(crate) fn revisions(&self) -> impl Iterator<Item = &syn::Expr> + '_ {
        self.args.revisions.iter()
    }

    /// Disallow `#[tracked]` attributes on the fields of this struct.
    ///
    /// If an `#[tracked]` field is found, return an error.
    ///
    /// # Parameters
    ///
    /// * `kind`, the attribute name (e.g., `input` or `interned`)
    fn maybe_disallow_tracked_fields(&self) -> syn::Result<()> {
        if A::ALLOW_TRACKED {
            return Ok(());
        }

        // Check if any field has the `#[tracked]` attribute.
        for ef in &self.fields {
            if ef.has_tracked_attr {
                return Err(syn::Error::new_spanned(
                    ef.field,
                    format!("`#[tracked]` cannot be used with `#[salsa::{}]`", A::KIND),
                ));
            }
        }

        Ok(())
    }

    /// Disallow `#[default]` attributes on the fields of this struct.
    ///
    /// If an `#[default]` field is found, return an error.
    ///
    /// # Parameters
    ///
    /// * `kind`, the attribute name (e.g., `input` or `interned`)
    fn maybe_disallow_default_fields(&self) -> syn::Result<()> {
        if A::ALLOW_DEFAULT {
            return Ok(());
        }

        // Check if any field has the `#[default]` attribute.
        for ef in &self.fields {
            if ef.has_default_attr {
                return Err(syn::Error::new_spanned(
                    ef.field,
                    format!("`#[default]` cannot be used with `#[salsa::{}]`", A::KIND),
                ));
            }
        }

        Ok(())
    }

    fn maybe_disallow_manual_retention_proofs(&self) -> syn::Result<()> {
        if A::ALLOW_MANUAL_RETENTION_PROOF {
            return Ok(());
        }

        for field in &self.fields {
            if field.manual_retention_proof.is_some() {
                return Err(syn::Error::new_spanned(
                    field.field,
                    format!(
                        "`#[salsa_value(...)]` cannot be used with `#[salsa::{}]`",
                        A::KIND
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Check that the generic parameters look as expected for this kind of struct.
    fn check_generics(&self) -> syn::Result<()> {
        if A::HAS_LIFETIME {
            if !A::ELIDABLE_LIFETIME {
                db_lifetime::require_db_lifetime(&self.struct_item.generics)
            } else {
                Ok(())
            }
        } else {
            db_lifetime::require_no_generics(&self.struct_item.generics)
        }
    }

    pub(crate) fn field_ids(&self) -> Vec<&syn::Ident> {
        self.fields
            .iter()
            .map(|f| f.field.ident.as_ref().unwrap())
            .collect()
    }

    pub(crate) fn tracked_ids(&self) -> Vec<&syn::Ident> {
        self.tracked_fields_iter()
            .map(|(_, f)| f.field.ident.as_ref().unwrap())
            .collect()
    }

    pub(crate) fn field_indices(&self) -> Vec<Literal> {
        (0..self.fields.len())
            .map(Literal::usize_unsuffixed)
            .collect()
    }

    pub(crate) fn tracked_field_indices(&self) -> Vec<Literal> {
        self.tracked_fields_iter()
            .map(|(index, _)| Literal::usize_unsuffixed(index))
            .collect()
    }

    pub(crate) fn untracked_field_indices(&self) -> Vec<Literal> {
        self.untracked_fields_iter()
            .map(|(index, _)| Literal::usize_unsuffixed(index))
            .collect()
    }

    pub(crate) fn num_fields(&self) -> Literal {
        Literal::usize_unsuffixed(self.fields.len())
    }

    pub(crate) fn num_tracked_fields(&self) -> Literal {
        Literal::usize_unsuffixed(self.tracked_fields_iter().count())
    }

    pub(crate) fn required_fields(&self) -> Vec<TokenStream> {
        self.fields
            .iter()
            .filter_map(|f| {
                if f.has_default_attr {
                    None
                } else {
                    let ident = f.field.ident.as_ref().unwrap();
                    let ty = &f.field.ty;
                    Some(quote!(#ident #ty))
                }
            })
            .collect()
    }

    pub(crate) fn field_vis(&self) -> Vec<&syn::Visibility> {
        self.fields.iter().map(|f| &f.field.vis).collect()
    }

    pub(crate) fn tracked_vis(&self) -> Vec<&syn::Visibility> {
        self.tracked_fields_iter()
            .map(|(_, f)| &f.field.vis)
            .collect()
    }

    pub(crate) fn untracked_vis(&self) -> Vec<&syn::Visibility> {
        self.untracked_fields_iter()
            .map(|(_, f)| &f.field.vis)
            .collect()
    }

    pub(crate) fn field_getter_ids(&self) -> Vec<&syn::Ident> {
        self.fields.iter().map(|f| &f.get_name).collect()
    }

    pub(crate) fn tracked_getter_ids(&self) -> Vec<&syn::Ident> {
        self.tracked_fields_iter()
            .map(|(_, f)| &f.get_name)
            .collect()
    }

    pub(crate) fn untracked_getter_ids(&self) -> Vec<&syn::Ident> {
        self.untracked_fields_iter()
            .map(|(_, f)| &f.get_name)
            .collect()
    }

    pub(crate) fn field_setter_ids(&self) -> Vec<&syn::Ident> {
        self.fields.iter().map(|f| &f.set_name).collect()
    }

    pub(crate) fn field_durability_ids(&self) -> Vec<syn::Ident> {
        self.fields
            .iter()
            .map(|f| quote::format_ident!("{}_durability", f.field.ident.as_ref().unwrap()))
            .collect()
    }

    pub(crate) fn field_tys(&self) -> Vec<&syn::Type> {
        self.fields.iter().map(|f| &f.field.ty).collect()
    }

    pub(crate) fn field_manual_retention_proofs(
        &self,
    ) -> Vec<Option<&crate::salsa_value::ManualRetentionProof>> {
        self.fields
            .iter()
            .map(|field| field.manual_retention_proof.as_ref())
            .collect()
    }

    pub(crate) fn tracked_tys(&self) -> Vec<&syn::Type> {
        self.tracked_fields_iter()
            .map(|(_, f)| &f.field.ty)
            .collect()
    }

    pub(crate) fn untracked_tys(&self) -> Vec<&syn::Type> {
        self.untracked_fields_iter()
            .map(|(_, f)| &f.field.ty)
            .collect()
    }

    pub(crate) fn field_indexed_tys(&self) -> Vec<syn::Ident> {
        self.fields
            .iter()
            .enumerate()
            .map(|(i, _)| quote::format_ident!("T{i}"))
            .collect()
    }

    pub(crate) fn field_attrs(&self) -> Vec<&[&syn::Attribute]> {
        self.fields.iter().map(|f| &*f.unknown_attrs).collect()
    }

    pub(crate) fn tracked_field_attrs(&self) -> Vec<&[&syn::Attribute]> {
        self.tracked_fields_iter()
            .map(|f| &*f.1.unknown_attrs)
            .collect()
    }

    pub(crate) fn untracked_field_attrs(&self) -> Vec<&[&syn::Attribute]> {
        self.untracked_fields_iter()
            .map(|f| &*f.1.unknown_attrs)
            .collect()
    }

    pub(crate) fn field_options(&self) -> Vec<TokenStream> {
        self.fields.iter().map(SalsaField::options).collect()
    }

    pub(crate) fn tracked_options(&self) -> Vec<TokenStream> {
        self.tracked_fields_iter()
            .map(|(_, f)| f.options())
            .collect()
    }

    pub(crate) fn untracked_options(&self) -> Vec<TokenStream> {
        self.untracked_fields_iter()
            .map(|(_, f)| f.options())
            .collect()
    }

    pub fn generate_debug_impl(&self) -> bool {
        self.args.debug.is_some()
    }

    pub fn generate_lifetime(&self) -> bool {
        self.args.no_lifetime.is_none()
    }

    pub fn tracked_fields_iter(&self) -> impl Iterator<Item = (usize, &SalsaField<'s>)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.has_tracked_attr)
    }

    pub fn untracked_fields_iter(&self) -> impl Iterator<Item = (usize, &SalsaField<'s>)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.has_tracked_attr)
    }

    /// Returns the path to the `serialize` function as an optional iterator.
    ///
    /// This will be `None` if `persistable` returns `false`.
    pub(crate) fn serialize_fn(&self) -> impl Iterator<Item = syn::Path> + '_ {
        self.args
            .persist
            .clone()
            .map(|persist| {
                persist
                    .serialize_fn
                    .unwrap_or(parse_quote! { serde::Serialize::serialize })
            })
            .into_iter()
    }

    /// Returns the path to the `deserialize` function as an optional iterator.
    ///
    /// This will be `None` if `persistable` returns `false`.
    pub(crate) fn deserialize_fn(&self) -> impl Iterator<Item = syn::Path> + '_ {
        self.args
            .persist
            .clone()
            .map(|persist| {
                persist
                    .deserialize_fn
                    .unwrap_or(parse_quote! { serde::Deserialize::deserialize })
            })
            .into_iter()
    }
}

impl<'s> SalsaField<'s> {
    fn new(field: &'s syn::Field) -> syn::Result<Self> {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();
        if BANNED_FIELD_NAMES.iter().any(|n| *n == field_name_str) {
            return Err(syn::Error::new(
                field_name.span(),
                format!("the field name `{field_name_str}` is disallowed in salsa structs",),
            ));
        }

        let get_name = Ident::new(&field_name_str, field_name.span());
        let set_name = Ident::new(&format!("set_{field_name_str}",), field_name.span());
        let returns = Ident::new("ref", field.span());
        let mut result = SalsaField {
            field,
            has_tracked_attr: false,
            returns,
            has_default_attr: false,
            has_no_eq_attr: false,
            manual_retention_proof: None,
            get_name,
            set_name,
            unknown_attrs: Default::default(),
        };

        // Scan the attributes and look for the salsa attributes:
        for attr in &field.attrs {
            let mut handled = false;
            for (fa, func) in FIELD_OPTION_ATTRIBUTES {
                if attr.path().is_ident(fa) {
                    func(attr, &mut result)?;
                    handled = true;
                    break;
                }
            }
            if !handled {
                result.unknown_attrs.push(attr);
            }
        }

        // Validate return mode
        if !ALLOWED_RETURN_MODES
            .iter()
            .any(|mode| mode == &result.returns.to_string())
        {
            return Err(syn::Error::new(
                result.returns.span(),
                format!("Invalid return mode. Allowed modes are: {ALLOWED_RETURN_MODES:?}"),
            ));
        }

        Ok(result)
    }

    fn options(&self) -> TokenStream {
        let returns = &self.returns;

        let default_ident = if self.has_default_attr {
            syn::Ident::new("default", Span::call_site())
        } else {
            syn::Ident::new("required", Span::call_site())
        };

        quote!((#returns, #default_ident))
    }
}
