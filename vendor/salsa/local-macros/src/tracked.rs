use syn::Item;
use syn::spanned::Spanned;

use crate::token_stream_with_error;

pub(crate) fn tracked(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as Item);
    let res = match item {
        syn::Item::Struct(item) => crate::tracked_struct::tracked_struct(args, item),
        syn::Item::Fn(item) => crate::tracked_fn::tracked_fn(args, item),
        syn::Item::Impl(item) => crate::tracked_impl::tracked_impl(args, item),
        _ => Err(syn::Error::new(
            item.span(),
            "tracked can only be applied to structs, functions, and impls",
        )),
    };
    match res {
        Ok(s) => s.into(),
        Err(err) => token_stream_with_error(input, err),
    }
}
