use quote::quote;
use syn::spanned::Spanned;
use syn::{Error, ItemStruct};

pub(super) fn generate_newtype_index(item: ItemStruct) -> syn::Result<proc_macro2::TokenStream> {
    if !item.fields.is_empty() {
        return Err(Error::new(
            item.span(),
            "A new type index cannot have any fields.",
        ));
    }

    if !item.generics.params.is_empty() {
        return Err(Error::new(
            item.span(),
            "A new type index cannot be generic.",
        ));
    }

    let ItemStruct {
        attrs,
        vis,
        struct_token,
        ident,
        generics: _,
        fields: _,
        semi_token,
    } = item;

    let debug_name = ident.to_string();

    let semi_token = semi_token.unwrap_or_default();
    let output = quote! {
        #(#attrs)*
        #[derive(Copy, Clone, Eq, PartialEq, Hash)]
        #vis #struct_token #ident(std::num::NonZeroU32)#semi_token

        impl #ident {
            const MAX_VALUE: u32 = u32::MAX - 1;
            const MAX: Self = Self::from_u32(Self::MAX_VALUE);

            #vis const fn from_usize(value: usize) -> Self {
                assert!(value <= Self::MAX_VALUE as usize);

                // SAFETY:
                // * The `value < u32::MAX` guarantees that the add doesn't overflow.
                // * The `+ 1` guarantees that the index is not zero
                //
                // N.B. We have to use the unchecked variant here because we're
                // in a const context and Option::unwrap isn't const yet.
                #[allow(unsafe_code)]
                Self(unsafe { std::num::NonZeroU32::new_unchecked((value as u32) + 1) })
            }

            #vis const fn from_u32(value: u32) -> Self {
                assert!(value <= Self::MAX_VALUE);

                // SAFETY:
                // * The `value < u32::MAX` guarantees that the add doesn't overflow.
                // * The `+ 1` guarantees that the index is larger than zero.
                //
                // N.B. We have to use the unchecked variant here because we're
                // in a const context and Option::unwrap isn't const yet.
                #[allow(unsafe_code)]
                Self(unsafe { std::num::NonZeroU32::new_unchecked(value + 1) })
            }

            /// Returns the index as a `u32` value
            #[inline]
            #vis const fn as_u32(self) -> u32 {
                self.0.get() - 1
            }

            /// Returns the index as a `usize` value
            #[inline]
            #vis const fn as_usize(self) -> usize {
                self.as_u32() as usize
            }

            #[inline]
            #vis const fn index(self) -> usize {
                self.as_usize()
            }
        }

        impl std::ops::Add<usize> for #ident {
            type Output = #ident;

            fn add(self, rhs: usize) -> Self::Output {
                #ident::from_usize(self.index() + rhs)
            }
        }

        impl std::ops::Add for #ident {
            type Output = #ident;

            fn add(self, rhs: Self) -> Self::Output {
                #ident::from_usize(self.index() + rhs.index())
            }
        }

        impl std::fmt::Debug for #ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(#debug_name).field(&self.index()).finish()
            }
        }

        impl ruff_index::Idx for #ident {
            #[inline]
            fn new(value: usize) -> Self {
                #ident::from_usize(value)
            }

            #[inline]
            fn index(self) -> usize {
                self.index()
            }
        }

        impl From<usize> for #ident {
            fn from(value: usize) -> Self {
                #ident::from_usize(value)
            }
        }

        impl From<u32> for #ident {
            fn from(value: u32) -> Self {
                #ident::from_u32(value)
            }
        }

        impl From<#ident> for usize {
            fn from(value: #ident) -> Self {
                value.as_usize()
            }
        }

        impl From<#ident> for u32 {
            fn from(value: #ident) -> Self {
                value.as_u32()
            }
        }
    };

    Ok(output)
}
