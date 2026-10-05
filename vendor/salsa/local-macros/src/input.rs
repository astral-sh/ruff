use proc_macro2::TokenStream;
use syn::ext::IdentExt;

use crate::hygiene::Hygiene;
use crate::options::{AllowedOptions, AllowedPersistOptions, Options};
use crate::salsa_struct::{SalsaStruct, SalsaStructAllowedOptions};
use crate::token_stream_with_error;

/// For an entity struct `Foo` with fields `f1: T1, ..., fN: TN`, we generate...
///
/// * the "id struct" `struct Foo(salsa::Id)`
/// * the entity ingredient, which maps the id fields to the `Id`
/// * for each value field, a function ingredient
pub(crate) fn input(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let args = syn::parse_macro_input!(args as InputArgs);
    let hygiene = Hygiene::from1(&input);
    let struct_item = parse_macro_input!(input as syn::ItemStruct);
    let m = Macro {
        hygiene,
        args,
        struct_item,
    };
    match m.try_macro() {
        Ok(v) => v.into(),
        Err(e) => token_stream_with_error(input, e),
    }
}

type InputArgs = Options<InputStruct>;

struct InputStruct;

impl AllowedOptions for InputStruct {
    const RETURNS: bool = false;

    const SPECIFY: bool = false;

    const NO_EQ: bool = false;

    const DEBUG: bool = true;

    const NO_LIFETIME: bool = false;

    const NON_SALSA_VALUES: bool = false;

    const SINGLETON: bool = true;

    const DATA: bool = true;

    const FIELD_REQUESTS: bool = true;

    const DB: bool = false;

    const CYCLE_FN: bool = false;

    const CYCLE_INITIAL: bool = false;

    const CYCLE_RESULT: bool = false;

    const LRU: bool = false;

    const CONSTRUCTOR_NAME: bool = true;

    const ID: bool = false;

    const REVISIONS: bool = false;

    const HEAP_SIZE: bool = true;

    const SELF_TY: bool = false;

    const PERSIST: AllowedPersistOptions = AllowedPersistOptions::AllowedValue;
}

impl SalsaStructAllowedOptions for InputStruct {
    const KIND: &'static str = "input";

    const ALLOW_TRACKED: bool = false;

    const HAS_LIFETIME: bool = false;

    const ELIDABLE_LIFETIME: bool = false;

    const ALLOW_DEFAULT: bool = true;

    const ALLOW_MANUAL_RETENTION_PROOF: bool = false;
}

struct Macro {
    hygiene: Hygiene,
    args: InputArgs,
    struct_item: syn::ItemStruct,
}

impl Macro {
    #[allow(non_snake_case)]
    fn try_macro(&self) -> syn::Result<TokenStream> {
        let salsa_struct = SalsaStruct::new(&self.struct_item, &self.args)?;

        let attrs = &self.struct_item.attrs;
        let vis = &self.struct_item.vis;
        let struct_ident = &self.struct_item.ident;
        let new_fn = salsa_struct.constructor_name();
        let field_ids = salsa_struct.field_ids();
        let field_indices = salsa_struct.field_indices();
        let num_fields = salsa_struct.num_fields();
        let field_vis = salsa_struct.field_vis();
        let field_getter_ids = salsa_struct.field_getter_ids();
        let field_setter_ids = salsa_struct.field_setter_ids();
        let field_requests_fn = &self.args.field_requests;
        if let Some(field_requests_fn) = field_requests_fn {
            let name = field_requests_fn.unraw().to_string();
            if name == new_fn.unraw().to_string()
                || field_getter_ids
                    .iter()
                    .chain(&field_setter_ids)
                    .any(|method| method.unraw().to_string() == name)
                || matches!(
                    name.as_str(),
                    "ingredient"
                        | "ingredient_"
                        | "ingredient_mut"
                        | "builder"
                        | "default_debug_fmt"
                )
                || (self.args.singleton.is_some() && matches!(name.as_str(), "get" | "try_get"))
            {
                return Err(syn::Error::new_spanned(
                    field_requests_fn,
                    "field requests method conflicts with another generated method",
                ));
            }
        }
        let required_fields = salsa_struct.required_fields();
        let field_options = salsa_struct.field_options();
        let field_tys = salsa_struct.field_tys();
        let field_durability_ids = salsa_struct.field_durability_ids();
        let field_attrs = salsa_struct.field_attrs();
        let is_singleton = self.args.singleton.is_some();
        let generate_debug_impl = salsa_struct.generate_debug_impl();
        let heap_size_fn = self.args.heap_size_fn.iter();

        let persist = self.args.persist();
        let serialize_fn = salsa_struct.serialize_fn();
        let deserialize_fn = salsa_struct.deserialize_fn();

        let zalsa = self.hygiene.ident("zalsa");
        let zalsa_struct = self.hygiene.ident("zalsa_struct");
        let Configuration = self.hygiene.ident("Configuration");
        let Builder = self.hygiene.ident("Builder");
        let CACHE = self.hygiene.ident("CACHE");
        let Db = self.hygiene.ident("Db");
        let FieldRequests = self
            .hygiene
            .scoped_ident(&struct_ident.unraw(), "FieldRequests");
        let request_zalsas = self.hygiene.ident("request_zalsas");
        let request_value = self.hygiene.ident("request_value");

        Ok(crate::debug::dump_tokens(
            struct_ident,
            quote! {
                salsa::plumbing::setup_input_struct!(
                    attrs: [#(#attrs),*],
                    vis: #vis,
                    Struct: #struct_ident,
                    new_fn: #new_fn,
                    field_requests_fn: #field_requests_fn,
                    field_options: [#(#field_options),*],
                    field_ids: [#(#field_ids),*],
                    field_getters: [#(#field_vis #field_getter_ids),*],
                    field_setters: [#(#field_vis #field_setter_ids),*],
                    field_tys: [#(#field_tys),*],
                    field_indices: [#(#field_indices),*],
                    field_attrs: [#([#(#field_attrs),*]),*],
                    required_fields: [#(#required_fields),*],
                    field_durability_ids: [#(#field_durability_ids),*],
                    num_fields: #num_fields,
                    is_singleton: #is_singleton,
                    generate_debug_impl: #generate_debug_impl,
                    heap_size_fn: #(#heap_size_fn)*,
                    persist: #persist,
                    serialize_fn: #(#serialize_fn)*,
                    deserialize_fn: #(#deserialize_fn)*,
                    unused_names: [
                        #zalsa,
                        #zalsa_struct,
                        #Configuration,
                        #Builder,
                        #CACHE,
                        #Db,
                        #FieldRequests,
                        #request_zalsas,
                        #request_value,
                    ]
                );
            },
        ))
    }
}
