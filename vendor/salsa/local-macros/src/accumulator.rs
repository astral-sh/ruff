use proc_macro2::TokenStream;

use crate::hygiene::Hygiene;
use crate::options::{AllowedOptions, AllowedPersistOptions, Options};
use crate::token_stream_with_error;

// #[salsa::accumulator(jar = Jar0)]
// struct Accumulator(DataType);

pub(crate) fn accumulator(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let hygiene = Hygiene::from1(&input);
    let args = syn::parse_macro_input!(args as Options<Accumulator>);
    let struct_item = parse_macro_input!(input as syn::ItemStruct);
    let ident = struct_item.ident.clone();
    let m = StructMacro {
        hygiene,
        _args: args,
        struct_item,
    };
    match m.try_expand() {
        Ok(v) => crate::debug::dump_tokens(ident, v).into(),
        Err(e) => token_stream_with_error(input, e),
    }
}

struct Accumulator;

impl AllowedOptions for Accumulator {
    const RETURNS: bool = false;
    const SPECIFY: bool = false;
    const NO_EQ: bool = false;
    const DEBUG: bool = false;
    const NON_SALSA_VALUES: bool = false;
    const NO_LIFETIME: bool = false;
    const SINGLETON: bool = false;
    const DATA: bool = false;
    const DB: bool = false;
    const CYCLE_FN: bool = false;
    const CYCLE_INITIAL: bool = false;
    const CYCLE_RESULT: bool = false;
    const LRU: bool = false;
    const CONSTRUCTOR_NAME: bool = false;
    const ID: bool = false;
    const REVISIONS: bool = false;
    const HEAP_SIZE: bool = false;
    const SELF_TY: bool = false;
    // TODO: Support serializing accumulators.
    const PERSIST: AllowedPersistOptions = AllowedPersistOptions::Invalid;
}

struct StructMacro {
    hygiene: Hygiene,
    _args: Options<Accumulator>,
    struct_item: syn::ItemStruct,
}

#[allow(non_snake_case)]
impl StructMacro {
    fn try_expand(self) -> syn::Result<TokenStream> {
        let ident = self.struct_item.ident.clone();

        let zalsa = self.hygiene.ident("zalsa");
        let zalsa_struct = self.hygiene.ident("zalsa_struct");
        let CACHE = self.hygiene.ident("CACHE");
        let ingredient = self.hygiene.ident("ingredient");

        let struct_item = self.struct_item;

        Ok(quote! {
            #struct_item

            salsa::plumbing::setup_accumulator_impl! {
                Struct: #ident,
                unused_names: [
                    #zalsa,
                    #zalsa_struct,
                    #CACHE,
                    #ingredient,
                ]
            }
        })
    }
}
