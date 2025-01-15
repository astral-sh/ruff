use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Error, Fields, Ident, ItemEnum, Result, Type, Variant};

pub(crate) fn generate_ast_enum(input: ItemEnum) -> Result<TokenStream> {
    let ast_enum = AstEnum::new(input)?;
    let id_enum = generate_id_enum(&ast_enum);
    let node_enum = generate_node_enum(&ast_enum);
    let node_enum_node_method = generate_node_enum_node_method(&ast_enum);
    let node_enum_ranged_impl = generate_node_enum_ranged_impl(&ast_enum);
    let variant_ids = generate_variant_ids(&ast_enum);
    let storage = generate_storage(&ast_enum);
    Ok(quote! {
        #id_enum
        #node_enum
        #node_enum_node_method
        #node_enum_ranged_impl
        #variant_ids
        #storage
    })
}

fn snake_case(node_ident: &Ident) -> Ident {
    let node_string = node_ident.to_string().to_snake_case();
    Ident::new(&node_string, node_ident.span())
}

fn concat(prefix: &str, id: &Ident, suffix: &str) -> Ident {
    let mut id_string = id.to_string();
    id_string.insert_str(0, prefix);
    id_string.push_str(suffix);
    Ident::new(&id_string, id.span())
}

/// Describes one of the enums that holds syntax nodes (e.g. Mod, Stmt)
struct AstEnum {
    /// The base name of the enums (e.g. Mod, Stmt)
    base_enum_name: Ident,
    /// The syntax node variants for this enum
    variants: Vec<AstVariant>,
}

/// Describes one specific syntax node (e.g. ModExpression, StmtIf)
struct AstVariant {
    /// The name of the variant within its containing enum (e.g. Expression, If)
    variant_name: Ident,
    /// The struct type defining the contents of this syntax node (e.g. ModExpression, StmtIf)
    node_ty: Ident,
    /// All of the attributes attached to this variant
    attrs: Vec<Attribute>,
}

impl AstEnum {
    fn new(input: ItemEnum) -> Result<AstEnum> {
        let base_enum_name = input.ident;
        let variants: Result<Vec<_>> = input.variants.into_iter().map(AstVariant::new).collect();
        let variants = variants?;
        Ok(AstEnum {
            base_enum_name,
            variants,
        })
    }

    fn map_variants<'a, B, F>(&'a self, f: F) -> impl Iterator<Item = B> + 'a
    where
        F: FnMut(&AstVariant) -> B + 'a,
    {
        self.variants.iter().map(f)
    }

    /// The name of the enum containing syntax node IDs (e.g. ModId, StmtId)
    fn id_enum_ty(&self) -> Ident {
        concat("", &self.base_enum_name, "Id")
    }

    /// The name of the enum containing references to syntax nodes (e.g. ModRef, StmtRef)
    fn ref_enum_ty(&self) -> Ident {
        concat("", &self.base_enum_name, "Ref")
    }

    /// The name of the storage type for this enum (e.g. ModStorage)
    fn enum_storage_ty(&self) -> Ident {
        concat("", &self.base_enum_name, "Storage")
    }

    /// The name of the storage field in Ast (e.g. mod_storage)
    fn enum_storage_field(&self) -> Ident {
        snake_case(&self.enum_storage_ty())
    }
}

impl AstVariant {
    fn new(variant: Variant) -> Result<AstVariant> {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(Error::new(
                variant.fields.span(),
                "Each AstNode variant must have a single unnamed field",
            ));
        };
        let mut fields = fields.unnamed.iter();
        let field = fields.next().ok_or_else(|| {
            Error::new(
                variant.fields.span(),
                "Each AstNode variant must have a single unnamed field",
            )
        })?;
        if fields.next().is_some() {
            return Err(Error::new(
                variant.fields.span(),
                "Each AstNode variant must have a single unnamed field",
            ));
        }
        let Type::Path(field_ty) = &field.ty else {
            return Err(Error::new(
                field.ty.span(),
                "Each AstNode variant must wrap a simple Id type",
            ));
        };
        let node_ty = field_ty.path.require_ident()?.clone();
        Ok(AstVariant {
            variant_name: variant.ident,
            node_ty,
            attrs: variant.attrs,
        })
    }

    /// The name of the ID type for this variant's syntax node (e.g. ModExpressionId, StmtIfId)
    fn id_ty(&self) -> Ident {
        concat("", &self.node_ty, "Id")
    }

    /// The name of the storage field in the containing enum storage type (e.g.
    /// mod_expression_storage)
    fn variant_storage_field(&self) -> Ident {
        concat("", &snake_case(&self.node_ty), "_storage")
    }

    /// The name of the method that adds a new syntax node to an [Ast] (e.g. `add_mod_expression`)
}

/// Generates the enum containing syntax node IDs (e.g. ModId, StmtId)
fn generate_id_enum(ast_enum: &AstEnum) -> TokenStream {
    let id_enum_ty = ast_enum.id_enum_ty();
    let id_enum_variants = ast_enum.map_variants(|v| {
        let AstVariant {
            variant_name,
            attrs,
            ..
        } = v;
        let id_ty = v.id_ty();
        quote! {
            #( #attrs )*
            #variant_name(#id_ty)
        }
    });
    quote! {
        #[automatically_derived]
        #[derive(Copy, Clone, Debug, PartialEq, is_macro::Is)]
        pub enum #id_enum_ty {
            #( #id_enum_variants ),*
        }
    }
}

fn generate_ref_enum(ast_enum: &AstEnum) -> TokenStream {
    let ref_enum_ty = ast_enum.ref_enum_ty();
    let variants = ast_enum.map_variants(|v| {
        let AstVariant {
            attrs,
            variant_name,
            node_ty,
            ..
        } = v;
        quote! {
            #( #attrs )*
            #variant_name(crate::Node<'a, &'a #node_ty>)
        }
    });
    quote! {
        #[automatically_derived]
        #[derive(Copy, Clone, Debug, PartialEq, is_macro::Is)]
        pub enum #node_ident<'a> {
            #( #variants ),*
        }
    }
}

fn generate_node_enum_node_method(ast_enum: &AstEnum) -> TokenStream {
    let id_enum_ty = ast_enum.id_enum_ty();
    let ref_enum_ty = ast_enum.ref_enum_ty();
    let variants = ast_enum.map_variants(|v| {
        let AstVariant { variant_name, .. } = v;
        quote! { #id_enum_ty::#variant_name(id) => #ref_enum_ty::#variant_name(self.ast.wrap(&self.ast[id])) }
    });
    quote! {
        #[automatically_derived]
        impl<'a> crate::Node<'a, #id_enum_ty> {
            #[inline]
            pub fn node(&self) -> #ref_enum_ty<'a> {
                match self.node {
                    #( #variants ),*
                }
            }
        }
    }
}

fn generate_node_enum_ranged_impl(ast_enum: &AstEnum) -> TokenStream {
    let ref_enum_ty = ast_enum.ref_enum_ty();
    let variants = ast_enum.map_variants(|v| {
        let AstVariant { variant_name, .. } = v;
        quote! { #ref_enum_ty::#variant_name(node) => node.range() }
    });
    quote! {
        #[automatically_derived]
        impl ruff_text_size::Ranged for #ref_enum_ty<'_> {
            fn range(&self) -> ruff_text_size::TextRange {
                match self {
                    #( #variants ),*
                }
            }
        }
    }
}

/// Generates the ID type for each syntax node struct in this enum.
///
/// We also define:
///   - [Index] and [IndexMut] impls so that you can index into an [Ast] using the ID type
///   - a `node` method on e.g. `Node<StmtIfId>` that returns a `Node<&StmtIf>`
///   - [Ranged] impls for the `StmtIf` and `Node<&StmtIf>`
fn generate_ids(ast_enum: &AstEnum) -> TokenStream {
    let id_enum_ty = ast_enum.id_enum_ty();
    let enum_storage_field = ast_enum.enum_storage_field();
    let variants = ast_enum.map_variants(|v| {
        let AstVariant { node_ty, .. } = v;
        let id_ty = v.id_ty();
        let variant_storage_field = v.variant_storage_field();
        quote! {
            #[automatically_derived]
            #[ruff_index::newtype_index]
            pub struct #id_ty;

            #[automatically_derived]
            impl std::ops::Index<#id_ty> for crate::Ast {
                type Output = #node_ty;
                #[inline]
                fn index(&self, id: #id_ty) -> &#node_ty {
                    &self.#enum_storage_field.#variant_storage_field[id]
                }
            }

            #[automatically_derived]
            impl std::ops::IndexMut<#id_ty> for crate::Ast {
                #[inline]
                fn index_mut(&mut self, id: #id_ty) -> &mut #node_ty {
                    &mut self.#enum_storage_field.#variant_storage_field[id]
                }
            }

            #[automatically_derived]
            impl<'a> crate::Node<'a, #id_ty> {
                #[inline]
                pub fn node(&self) -> crate::Node<'a, &'a #node_ty> {
                    self.ast.wrap(&self.ast[self.node])
                }
            }

            #[automatically_derived]
            impl<'a> ruff_text_size::Ranged for #node_ty {
                fn range(&self) -> TextRange {
                    self.range
                }
            }

            #[automatically_derived]
            impl<'a> ruff_text_size::Ranged for crate::Node<'a, &'a #node_ty> {
                fn range(&self) -> TextRange {
                    self.as_ref().range()
                }
            }
        }
    });
    quote! { #( #variants )* }
}

fn generate_storage(ast_enum: &AstEnum) -> TokenStream {
    let id_enum_ty = ast_enum.id_enum_ty();
    let enum_storage_ty = ast_enum.enum_storage_ty();
    let enum_storage_field = ast_enum.enum_storage_field();
    let storage_fields = ast_enum.map_variants(|v| {
        let AstVariant { id_ty, node_ty, .. } = v;
        let variant_storage_field = v.variant_storage_field();
        quote! { #variant_storage_field: ruff_index::IndexVec<#id_ty, #node_ty> }
    });
    let add_methods = ast_enum.map_variants(|v| {
        let AstVariant { node_ty, .. } = v;
        let variant_storage_field = v.variant_storage_field();
        let method_name = concat("add_", vec_name, "");
        quote! {
            #[automatically_derived]
            impl crate::Ast {
                pub fn #method_name(&mut self, payload: #node_ty) -> #id_ident {
                    #id_ident::#variant_name(self.#storage_field.#vec_name.push(payload))
                }
            }
        }
    });
    quote! {
        #[automatically_derived]
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, Default, PartialEq)]
        pub(crate) struct #storage_ty {
            #( #storage_fields ),*
        }

        #( #add_methods )*
    }
}
