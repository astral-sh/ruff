use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, Item, Result};

use super::expand;

fn family(body: TokenStream) -> TokenStream {
    quote! {
        #[synchronous(SynchronousBase)]
        trait Base<'db> {
            type Error;
            #[operation(source)]
            async fn read<'a>(&self, input: &'a u8) -> Result<&'a u8, Self::Error>;
        }
        #[synchronous(SynchronousEffects)]
        trait Effects<'db>: Base<'db> {
            #[operation(checkpoint)]
            async fn checkpoint(&self) -> Result<(), Self::Error>;
        }
        #[finite_capability]
        impl Facts {
            fn value(&self, input: u8) -> u8 { input }
            fn present(&self, input: Option<u8>) -> bool { input.is_some() }
        }
        #[synchronous(reduce_sync)]
        #[capabilities(effects = Effects, facts = Facts)]
        #[passive_values()]
        async fn reduce<'db, 'a, E: Effects<'db>>(
            input: &'a u8,
            facts: Facts,
            effects: &E,
        ) -> Result<&'a u8, E::Error> {
            #body
        }
    }
}

fn shared_function(file: &mut syn::File) -> Result<&mut syn::ItemFn> {
    file.items
        .iter_mut()
        .find_map(|item| {
            if let Item::Fn(function) = item {
                Some(function)
            } else {
                None
            }
        })
        .ok_or_else(|| syn::Error::new(Span::call_site(), "missing shared function"))
}

#[test]
fn declarations_generate_inherited_interfaces_and_preserve_lifetimes() -> Result<()> {
    let actual = syn::parse2::<syn::File>(expand(family(quote! {
        effects.checkpoint().await?;
        effects.read(input).await
    }))?)?;
    let expected: syn::File = syn::parse_quote! {
        trait Base<'db> {
            type Error;
            async fn read<'a>(&self, input: &'a u8) -> Result<&'a u8, Self::Error>;
        }
        trait SynchronousBase<'db> {
            type Error;
            fn read<'a>(&self, input: &'a u8) -> Result<&'a u8, Self::Error>;
        }
        trait Effects<'db>: Base<'db> {
            async fn checkpoint(&self) -> Result<(), Self::Error>;
        }
        trait SynchronousEffects<'db>: SynchronousBase<'db> {
            fn checkpoint(&self) -> Result<(), Self::Error>;
        }
        impl Facts {
            fn value(&self, input: u8) -> u8 { input }
            fn present(&self, input: Option<u8>) -> bool { input.is_some() }
        }
        async fn reduce<'db, 'a, E: Effects<'db>>(
            input: &'a u8,
            facts: Facts,
            effects: &E,
        ) -> Result<&'a u8, E::Error> {
            effects.checkpoint().await?;
            effects.read(input).await
        }
        fn reduce_sync<'db, 'a, E: SynchronousEffects<'db>>(
            input: &'a u8,
            facts: Facts,
            effects: &E,
        ) -> Result<&'a u8, E::Error> {
            effects.checkpoint()?;
            effects.read(input)
        }
    };
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn local_names_and_new_declared_operations_need_no_application_manifest() -> Result<()> {
    for local in [quote!(original), quote!(renamed)] {
        expand(family(quote! {
            let #local = input;
            effects.checkpoint().await?;
            effects.read(#local).await
        }))?;
    }
    let mut input = syn::parse2::<syn::File>(family(quote! {
        effects.another(input).await
    }))?;
    for item in &mut input.items {
        if let Item::Trait(declaration) = item
            && declaration.ident == "Effects"
        {
            declaration.items.push(syn::parse_quote! {
                #[operation(child)]
                async fn another<'a>(&self, value: &'a u8) -> Result<&'a u8, Self::Error>;
            });
        }
    }
    expand(quote!(#input))?;
    Ok(())
}

#[test]
fn escapes_are_rejected_in_expressions_and_matches_guards() {
    for operation in [
        quote!(db),
        quote!(place_table(db, scope)),
        quote!(unapproved()),
        quote!(effects.unknown().await),
        quote!(effects.checkpoint()),
        quote!(Effects::checkpoint(effects).await),
        quote!(effects),
        quote!(facts),
        quote!(input.contains("scan")),
        quote!(bindings.next()),
        quote!(Vec::new()),
        quote!(vec![]),
        quote!(facts.value(0).await),
        quote!(facts.value::<u8>(0)),
        quote!(async { effects.checkpoint().await }),
        quote!(|| effects.checkpoint()),
    ] {
        for body in [
            quote!(#operation; Ok(input)),
            quote!(matches!(#operation, _); Ok(input)),
            quote!(matches!(true, _ if { #operation; true }); Ok(input)),
        ] {
            assert!(expand(family(body.clone())).is_err(), "accepted {body}");
        }
    }
    for body in [
        quote!(let effects = input; Ok(input)),
        quote!(let facts = input; Ok(input)),
        quote!(let r#facts = input; Ok(input)),
        quote!(let r#effects = input; Ok(input)),
        quote!(fn nested() {}, Ok(input)),
        quote!(for value in input {} Ok(input)),
        quote!(while true {} Ok(input)),
        quote!(loop {}),
    ] {
        assert!(expand(family(body.clone())).is_err(), "accepted {body}");
    }
}

#[test]
fn matches_inputs_and_guards_are_lowered() -> Result<()> {
    let expanded = syn::parse2::<syn::File>(expand(family(quote! {
        let found = matches!(effects.read(input).await?, |Some(value)| None,);
        let guarded = matches!(true, _ if {
            effects.checkpoint().await?;
            facts.present(Some(1))
        });
        Ok(input)
    }))?)?;
    let Some(Item::Fn(synchronous)) = expanded.items.last() else {
        return Err(syn::Error::new_spanned(
            expanded,
            "missing synchronous body",
        ));
    };
    let expected: syn::Block = syn::parse_quote! {{
        let found = matches!(effects.read(input)?, |Some(value)| None);
        let guarded = matches!(true, _ if {
            effects.checkpoint()?;
            facts.present(Some(1))
        });
        Ok(input)
    }};
    assert_eq!(*synchronous.block, expected);
    Ok(())
}

#[test]
fn invalid_declarations_and_capability_substitution_are_rejected() -> Result<()> {
    let original = family(quote!(effects.read(input).await));
    for (from, to) in [
        ("operation (source)", "operation (unclassified)"),
        ("facts : Facts", "facts : OtherFacts"),
        ("effects : & E", "effects : & ConcreteProvider"),
        ("effects = Effects", "effects = OtherEffects"),
        ("async fn read", "fn read"),
    ] {
        let source = original.to_string();
        assert!(source.contains(from), "missing mutation anchor {from}");
        let changed: TokenStream = source
            .replace(from, to)
            .parse::<TokenStream>()
            .map_err(|error| syn::Error::new(Span::call_site(), error))?;
        assert!(expand(changed).is_err(), "accepted {to}");
    }
    let cyclic = quote! {
        #[synchronous(SyncA)]
        trait A: B {}
        #[synchronous(SyncB)]
        trait B: A {}
        #[synchronous(cycle_sync)]
        #[capabilities(effects = A)]
        #[passive_values()]
        async fn cycle<E: A>(effects: &E) { effects.any().await; }
    };
    assert!(expand(cyclic).is_err());
    Ok(())
}

#[test]
fn none_unit_patterns_do_not_admit_modified_bindings() -> Result<()> {
    expand(family(quote! {
        let absent = matches!(Some(1), Some(_) | None);
        Ok(input)
    }))?;
    for pattern in [
        quote!(mut None),
        quote!(ref None),
        quote!(ref mut None),
        quote!(None @ _),
        quote!(r#None),
        quote!(
            #[allow(unused_variables)]
            None
        ),
    ] {
        let body = quote! {
            let absent = matches!(Some(1), #pattern);
            Ok(input)
        };
        assert!(expand(family(body)).is_err(), "accepted {pattern}");
    }
    Ok(())
}

#[test]
fn generic_parameters_cannot_substitute_for_the_finite_type() -> Result<()> {
    let body = quote! {
        let value = facts.value(1);
        Ok(input)
    };
    for parameter in [
        syn::parse_quote!(Facts: Unreviewed),
        syn::parse_quote!(r#Facts: Unreviewed),
        syn::parse_quote!(const Facts: usize),
    ] {
        let mut input = syn::parse2::<syn::File>(family(body.clone()))?;
        shared_function(&mut input)?
            .sig
            .generics
            .params
            .push(parameter);
        assert_eq!(
            expand(quote!(#input)).err().map(|error| error.to_string()),
            Some(
                "generic parameters cannot replace finite types or passive constructors".to_owned()
            ),
        );
    }
    let mut input = syn::parse2::<syn::File>(family(body))?;
    shared_function(&mut input)?
        .sig
        .generics
        .params
        .push(syn::parse_quote!(Other));
    expand(quote!(#input))?;
    Ok(())
}

#[test]
fn formal_parameters_cannot_replace_passive_constructors_with_callbacks() -> Result<()> {
    for name in ["Ok", "r#Ok", "Some", "None", "Wrapper"] {
        let name: syn::Ident = syn::parse_str(name)?;
        let mut input = syn::parse2::<syn::File>(family(quote!(Ok(input))))?;
        let function = shared_function(&mut input)?;
        function.sig.inputs.push(syn::parse_quote!(
            #name: fn(&'a u8) -> Result<&'a u8, E::Error>
        ));
        if name == "Wrapper" {
            for attribute in &mut function.attrs {
                if attribute.path().is_ident("passive_values") {
                    *attribute = syn::parse_quote!(#[passive_values(Wrapper)]);
                }
            }
            function.block = Box::new(syn::parse_quote!({ Wrapper(input) }));
        }
        assert_eq!(
            expand(quote!(#input)).err().map(|error| error.to_string()),
            Some("formal parameters cannot shadow passive constructors".to_owned()),
        );
    }
    let mut input = syn::parse2::<syn::File>(family(quote!(Ok(input))))?;
    shared_function(&mut input)?
        .sig
        .inputs
        .push(syn::parse_quote!(
            callback: fn(&'a u8) -> Result<&'a u8, E::Error>
        ));
    expand(quote!(#input))?;
    shared_function(&mut input)?.block = Box::new(syn::parse_quote!({ callback(input) }));
    assert!(expand(quote!(#input)).is_err());

    let mut input = syn::parse2::<syn::File>(family(quote!(Ok(input))))?;
    shared_function(&mut input)?
        .sig
        .inputs
        .push(syn::parse_quote!(
            callback @ Ok: fn(&'a u8) -> Result<&'a u8, E::Error>
        ));
    assert_eq!(
        expand(quote!(#input)).err().map(|error| error.to_string()),
        Some("shared parameters cannot introduce subpatterns".to_owned()),
    );
    Ok(())
}

#[test]
fn synchronous_lowering_preserves_await_attributes() -> Result<()> {
    let actual = syn::parse2::<syn::File>(expand(family(quote! {
        #[cfg(any())]
        effects.checkpoint().await;
        effects.read(input).await
    }))?)?;
    let expected: syn::File = syn::parse_quote! {
        trait Base<'db> {
            type Error;
            async fn read<'a>(&self, input: &'a u8) -> Result<&'a u8, Self::Error>;
        }
        trait SynchronousBase<'db> {
            type Error;
            fn read<'a>(&self, input: &'a u8) -> Result<&'a u8, Self::Error>;
        }
        trait Effects<'db>: Base<'db> {
            async fn checkpoint(&self) -> Result<(), Self::Error>;
        }
        trait SynchronousEffects<'db>: SynchronousBase<'db> {
            fn checkpoint(&self) -> Result<(), Self::Error>;
        }
        impl Facts {
            fn value(&self, input: u8) -> u8 { input }
            fn present(&self, input: Option<u8>) -> bool { input.is_some() }
        }
        async fn reduce<'db, 'a, E: Effects<'db>>(
            input: &'a u8,
            facts: Facts,
            effects: &E,
        ) -> Result<&'a u8, E::Error> {
            #[cfg(any())]
            effects.checkpoint().await;
            effects.read(input).await
        }
        fn reduce_sync<'db, 'a, E: SynchronousEffects<'db>>(
            input: &'a u8,
            facts: Facts,
            effects: &E,
        ) -> Result<&'a u8, E::Error> {
            #[cfg(any())]
            effects.checkpoint();
            effects.read(input)
        }
    };
    assert_eq!(actual, expected);
    Ok(())
}

struct OriginalFiniteOperations;

impl VisitMut for OriginalFiniteOperations {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        visit_mut::visit_expr_mut(self, expression);
        let Expr::MethodCall(call) = expression else {
            return;
        };
        if !matches!(&*call.receiver, Expr::Path(path) if path.path.is_ident("facts")) {
            return;
        }
        let arguments: Vec<_> = call.args.iter().cloned().collect();
        let replacement = match (call.method.to_string().as_str(), arguments.as_slice()) {
            ("unbound", []) => syn::parse_quote!(Member::unbound()),
            ("environment", [scope]) => syn::parse_quote!(ProgramEnvironment::from_scope(#scope)),
            ("bindings", [receiver, symbol]) => {
                let mut call: syn::ExprMethodCall =
                    syn::parse_quote!(value.end_of_scope_symbol_bindings(#symbol));
                call.receiver = Box::new(receiver.clone());
                Expr::MethodCall(call)
            }
            ("provenance", [receiver, other]) | ("with_qualifiers", [receiver, other]) => {
                let method = if call.method == "provenance" {
                    quote!(or)
                } else {
                    quote!(with_qualifiers)
                };
                let mut call: syn::ExprMethodCall = syn::parse_quote!(value.#method(#other));
                call.receiver = Box::new(receiver.clone());
                Expr::MethodCall(call)
            }
            ("is_undefined" | "is_init_var", [receiver]) => {
                let method = &call.method;
                let mut call: syn::ExprMethodCall = syn::parse_quote!(value.#method());
                call.receiver = Box::new(receiver.clone());
                Expr::MethodCall(call)
            }
            _ => return,
        };
        *expression = replacement;
    }
}

#[test]
fn actual_raw_class_body_keeps_the_original_synchronous_decisions() -> Result<()> {
    let source = syn::parse_file(include_str!(
        "../../../ty_python_semantic/src/types/class/member_source.rs"
    ))?;
    let Some(invocation) = source.items.iter().find_map(|item| {
        if let Item::Macro(invocation) = item
            && invocation
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "shared_semantic_family")
        {
            Some(invocation)
        } else {
            None
        }
    }) else {
        return Err(syn::Error::new_spanned(
            source,
            "missing shared source family",
        ));
    };
    let expanded = syn::parse2::<syn::File>(expand(invocation.mac.tokens.clone())?)?;
    let Some(mut actual) = expanded.items.into_iter().find_map(|item| {
        if let Item::Fn(function) = item
            && function.sig.ident == "raw_class_member_sync"
        {
            Some(function.block)
        } else {
            None
        }
    }) else {
        return Err(syn::Error::new_spanned(
            invocation,
            "missing synchronous raw-class body",
        ));
    };
    OriginalFiniteOperations.visit_block_mut(&mut actual);
    let expected: syn::Block = syn::parse_quote! {
    {
        effects.checkpoint(MemberSourceWork::Begin)?;
        effects.checkpoint(MemberSourceWork::PlaceTable)?;
        let table = effects.place_table(scope)?;
        effects.checkpoint(MemberSourceWork::Symbol)?;
        let Some(symbol_id) = effects.symbol_id(table, name)? else {
            effects.checkpoint(MemberSourceWork::Publish)?;
            return Ok(Member::unbound());
        };
        effects.checkpoint(MemberSourceWork::Declarations)?;
        let place_and_quals = effects.public_class_place(scope, symbol_id)?;
        effects.checkpoint(MemberSourceWork::Classify)?;
        if !place_and_quals.is_undefined() && !place_and_quals.is_init_var() {
            // Trust the declared type if we see a class-level declaration
            effects.checkpoint(MemberSourceWork::Publish)?;
            return Ok(Member {
                inner: place_and_quals,
            });
        }

        let member = if let PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    ty,
                    provenance: declared_provenance,
                    ..
                }),
            qualifiers,
        } = place_and_quals
        {
            // Otherwise, we need to check if the symbol has bindings
            effects.checkpoint(MemberSourceWork::UseDef)?;
            let use_def = effects.use_def_map(scope)?;
            let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
            let env = ProgramEnvironment::from_scope(scope);
            effects.checkpoint(MemberSourceWork::Bindings)?;
            let inferred = effects.binding_place(&env, bindings)?.place;

            // TODO: we should not need to calculate inferred type second time. This is a temporary
            // solution until the notion of Boundness and Declaredness is split. See #16036, #16264
            Member {
                inner: match inferred {
                    Place::Undefined => Place::Undefined.with_qualifiers(qualifiers),
                    Place::Defined(place) => Place::Defined(DefinedPlace {
                        ty,
                        provenance: place.provenance.or(declared_provenance),
                        ..place
                    })
                    .with_qualifiers(qualifiers),
                },
            }
        } else {
            Member::unbound()
        };
        effects.checkpoint(MemberSourceWork::Publish)?;
        Ok(member)
    }
        };
    assert_eq!(*actual, expected);
    Ok(())
}

fn cursor_family(body: TokenStream) -> TokenStream {
    quote! {
        #[synchronous(SyncCursorEffects)]
        trait CursorEffects<'db> {
            type Error;
            #[operation(local)]
            #[progress]
            async fn advance<'cursor>(&mut self, cursor: &'cursor mut Cursor)
                -> Result<Option<u8>, Self::Error>;
            #[operation(child)]
            async fn child(&mut self, item: u8) -> Result<u8, Self::Error>;
        }
        #[synchronous(SyncEffects)]
        trait Effects<'db>: CursorEffects<'db> {}
        #[finite_capability]
        impl Facts {
            fn combine(&self, left: u8, right: u8) -> u8 { left.saturating_add(right) }
            fn stop(&self, value: u8) -> bool { value == 9 }
            fn skip(&self, value: u8) -> bool { value == 0 }
        }
        #[synchronous(reduce_sync)]
        #[capabilities(effects = Effects, facts = Facts)]
        #[passive_values()]
        async fn reduce<'db, E: Effects<'db>>(
            mut cursor: Cursor, seed: u8, facts: Facts, effects: &mut E,
        ) -> Result<u8, E::Error> {
            #body
        }
    }
}

fn rejects(input: TokenStream, expected: &str) {
    assert_eq!(
        expand(input.clone())
            .err()
            .map(|error| error.to_string())
            .as_deref(),
        Some(expected),
        "unexpected diagnostic for {input}",
    );
}

#[test]
fn cursor_lowering_preserves_control_flow_and_normalizes_option() -> Result<()> {
    let expanded = syn::parse2::<syn::File>(expand(cursor_family(quote! {
        #[passive_state]
        let mut total: u8 = seed;
        #[cursor_loop]
        while let Some(item) = effects.advance(&mut cursor).await? {
            if facts.skip(item) { continue; }
            if facts.stop(item) { break; }
            let value = effects.child(item).await?;
            total = facts.combine(total, value);
        }
        Ok(total)
    }))?)?;
    let expected: syn::Block = syn::parse_quote! {{
        let mut total: u8 = seed;
        {
            fn __shared_semantic_require_copy<T: ::core::marker::Copy>(_: &T) {}
            __shared_semantic_require_copy(&total);
        }
        while let ::core::option::Option::Some(item) = effects.advance(&mut cursor)? {
            if facts.skip(item) { continue; }
            if facts.stop(item) { break; }
            let value = effects.child(item)?;
            total = facts.combine(total, value);
        }
        Ok(total)
    }};
    let Some(Item::Fn(synchronous)) = expanded.items.last() else {
        return Err(syn::Error::new_spanned(
            &expanded,
            "missing synchronous reducer",
        ));
    };
    assert_eq!(*synchronous.block, expected);
    let rendered = quote!(#expanded).to_string();
    for marker in ["cursor_loop", "passive_state", "progress"] {
        assert!(!rendered.contains(marker), "unlowered {marker}");
    }
    let Some(asynchronous) = expanded.items.iter().find_map(|item| {
        if let Item::Fn(function) = item
            && function.sig.ident == "reduce"
        {
            Some(function)
        } else {
            None
        }
    }) else {
        return Err(syn::Error::new_spanned(
            &expanded,
            "missing asynchronous reducer",
        ));
    };
    let mut lowered = asynchronous.block.clone();
    super::Lowerer { synchronous: true }.visit_block_mut(&mut lowered);
    assert_eq!(lowered, synchronous.block);
    assert!(
        quote!(#asynchronous)
            .to_string()
            .contains("effects : & mut E")
    );
    assert!(rendered.contains("cursor : & 'cursor mut Cursor"));
    Ok(())
}

#[test]
fn cursor_names_receivers_and_progress_boundaries_are_generic() -> Result<()> {
    let source = cursor_family(quote! {
        #[passive_state]
        let mut total = seed;
        #[cursor_loop]
        while let Some(item) = effects.advance(&mut cursor).await? {
            #[cursor_loop]
            while let Some(inner) = effects.advance(&mut cursor).await? {
                total = effects.child(inner).await?;
                break;
            }
            if facts.stop(item) { return Ok(total); }
        }
        Ok(total)
    })
    .to_string();
    for (receiver, operation, local, reference, boundary) in [
        ("effects", "advance", "total", "& mut E", "local"),
        ("provider", "take_item", "answer", "& E", "child"),
    ] {
        let mut renamed = source
            .replace("effects", receiver)
            .replace("advance", operation)
            .replace("total", local)
            .replace("& mut E", reference)
            .replace("operation (local)", &format!("operation ({boundary})"));
        if reference == "& E" {
            renamed = renamed.replace("& mut self", "& self");
        }
        expand(
            renamed
                .parse()
                .map_err(|error| syn::Error::new(Span::call_site(), error))?,
        )?;
    }
    expand(
        family(quote!(effects.read(input).await))
            .to_string()
            .replace("effects : & E", "effects : & mut E")
            .parse()
            .map_err(|error| syn::Error::new(Span::call_site(), error))?,
    )?;
    Ok(())
}

#[test]
fn progress_declarations_validate_boundary_result_and_inheritance() -> Result<()> {
    let source = cursor_family(quote!(Ok(seed))).to_string();
    for (from, to, diagnostic) in [
        (
            "# [progress]",
            "# [progress] # [progress]",
            "duplicate family attribute",
        ),
        (
            "# [progress]",
            "# [progress(value)]",
            "expected #[progress]",
        ),
        (
            "operation (local)",
            "operation (source)",
            "progress requires a local or child operation",
        ),
        (
            "operation (local)",
            "operation (checkpoint)",
            "progress requires a local or child operation",
        ),
        (
            "Result < Option < u8 > , Self :: Error >",
            "Result<u8, Self::Error>",
            "progress requires Result<Option<T>, Self::Error>",
        ),
        (
            "Self :: Error",
            "Failure",
            "progress requires Result<Option<T>, Self::Error>",
        ),
        (
            "# [progress]",
            "# [progress] # [cfg(any())]",
            "progress declarations cannot be conditional",
        ),
        (
            "trait CursorEffects",
            "#[cfg_attr(any(), allow(dead_code))] trait CursorEffects",
            "progress declarations cannot be conditional",
        ),
    ] {
        assert!(source.contains(from), "missing mutation anchor {from}");
        rejects(
            source
                .replace(from, to)
                .parse()
                .map_err(|error| syn::Error::new(Span::call_site(), error))?,
            diagnostic,
        );
    }
    let mut conflicting = syn::parse2::<syn::File>(cursor_family(quote!(Ok(seed))))?;
    for item in &mut conflicting.items {
        if let Item::Trait(declaration) = item
            && declaration.ident == "Effects"
        {
            declaration.items.push(syn::parse_quote! {
                #[operation(child)]
                async fn advance<'cursor>(&mut self, cursor: &'cursor mut Cursor)
                    -> Result<Option<u8>, Self::Error>;
            });
        }
    }
    rejects(
        quote!(#conflicting),
        "conflicting inherited effect operation",
    );
    let qualified = source
        .replace(
            "Result < Option < u8 > , Self :: Error >",
            "::core::result::Result<::core::option::Option<u8>, Self::Failure>",
        )
        .replace("type Error", "type Failure")
        .replace("Self :: Error", "Self::Failure")
        .replace("E :: Error", "E::Failure");
    expand(
        qualified
            .parse()
            .map_err(|error| syn::Error::new(Span::call_site(), error))?,
    )?;
    Ok(())
}

#[test]
fn cursor_headers_require_an_unwrapped_progress_call() {
    for condition in [
        quote!(let Some(item) = effects.child(seed).await?),
        quote!(let Some(item) = facts.stop(seed)),
        quote!(let Some(item) = effects.unknown(&mut cursor).await?),
        quote!(let Some(item) = effects.advance(&mut cursor)),
        quote!(let Some(item) = effects.advance(&mut cursor).await),
        quote!(let Some(item) = (effects.advance(&mut cursor).await?)),
        quote!(let Some(item) = { effects.advance(&mut cursor).await? }),
        quote!(let Some(item) = effects.advance::<u8>(&mut cursor).await?),
        quote!(let Some(item) = (effects).advance(&mut cursor).await?),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while #condition {} Ok(seed)
            }),
            "cursor loop requires a declared progress operation followed by .await?",
        );
    }
    for condition in [
        quote!(true),
        quote!(let Some(item) = effects.advance(&mut cursor).await? && true),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while #condition {} Ok(seed)
            }),
            "cursor loop requires let Some(name) = capability.operation(...).await?",
        );
    }
    for pattern in [
        quote!(item),
        quote!(Some(_)),
        quote!(Some(mut item)),
        quote!(Some(ref item)),
        quote!(Some(item @ _)),
        quote!(Some((item, other))),
        quote!(Some(item, other)),
        quote!(Option::Some(item)),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while let #pattern = effects.advance(&mut cursor).await? {} Ok(seed)
            }),
            "cursor loop requires an unmodified Some(name) pattern",
        );
    }
}

#[test]
fn cursor_loops_do_not_admit_unstructured_control_flow() {
    for body in [
        quote!(loop {}),
        quote!(while true {}),
        quote!(for item in cursor {}),
        quote!(while let Some(item) = effects.advance(&mut cursor).await? {}),
    ] {
        rejects(
            cursor_family(body),
            "expression is outside the shared capability grammar",
        );
    }
    rejects(
        cursor_family(quote! {
            #[cursor_loop] 'outer: while let Some(item) = effects.advance(&mut cursor).await? {}
        }),
        "cursor loops cannot have labels",
    );
    for (exit, diagnostic) in [
        (
            quote!(break),
            "only bare break inside a cursor loop is supported",
        ),
        (
            quote!(continue),
            "only bare continue inside a cursor loop is supported",
        ),
    ] {
        rejects(cursor_family(quote!(#exit; Ok(seed))), diagnostic);
    }
    for (exit, diagnostic) in [
        (
            quote!(break seed),
            "only bare break inside a cursor loop is supported",
        ),
        (
            quote!(break 'outer),
            "only bare break inside a cursor loop is supported",
        ),
        (
            quote!(continue 'outer),
            "only bare continue inside a cursor loop is supported",
        ),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while let Some(item) = effects.advance(&mut cursor).await? { #exit; }
            }),
            diagnostic,
        );
    }
    for argument in [
        quote!({
            continue;
            &mut cursor
        }),
        quote!({
            break;
            &mut cursor
        }),
        quote!({
            #[cursor_loop]
            while let Some(inner) = effects.advance(&mut cursor).await? {}
            &mut cursor
        }),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while let Some(item) = effects.advance(#argument).await? {}
            }),
            "cursor loop headers cannot contain loops or loop exits",
        );
    }
}

#[test]
fn passive_assignment_requires_a_standalone_write_to_its_binding() {
    for target in [
        quote!(seed),
        quote!(cursor),
        quote!(effects),
        quote!(ordinary),
        quote!(total.field),
        quote!(total[0]),
        quote!(*total),
        quote!((total, ordinary)),
    ] {
        rejects(
            cursor_family(quote! {
                #[passive_state] let mut total = seed;
                let mut ordinary = seed;
                #target = seed;
                Ok(total)
            }),
            "assignment requires a declared passive-state local",
        );
    }
    for expression in [
        quote!(total += seed),
        quote!(total -= seed),
        quote!(total |= seed),
    ] {
        rejects(
            cursor_family(quote! {
                #[passive_state] let mut total = seed;
                #expression;
                Ok(total)
            }),
            "expression is outside the shared capability grammar",
        );
    }
    for body in [
        quote!(let assigned = total = seed;),
        quote!(facts.stop(total = seed);),
        quote!(total = seed),
    ] {
        rejects(
            cursor_family(quote! {
                #[passive_state] let mut total = seed;
                #body
            }),
            "passive-state assignment must be a standalone statement",
        );
    }
    for declaration in [
        quote!(let total = seed;),
        quote!(let mut total;),
        quote!(let (mut total, other) = (seed, seed);),
        quote!(let ref mut total = seed;),
        quote!(let mut total @ _ = seed;),
        quote!(let mut total = Some(seed) else { return Ok(seed); };),
    ] {
        rejects(
            cursor_family(quote!(#[passive_state] #declaration Ok(seed))),
            "passive state requires an initialized mutable local binding",
        );
    }
}

#[test]
fn assignment_permissions_follow_lexical_scopes_and_raw_identifiers() -> Result<()> {
    for body in [
        quote!({
            let mut total = seed;
            total = seed;
        }),
        quote!(let mut r#total = seed; total = seed;),
        quote!(if let Some(total) = Some(seed) {
            total = seed;
        }),
        quote!(match Some(seed) {
            Some(total) => {
                total = seed;
            }
            None => {}
        }),
        quote!(let _ = matches!(Some(seed), Some(total) if { total = seed; true });),
        quote!(
            #[cursor_loop]
            while let Some(total) = effects.advance(&mut cursor).await? {
                total = seed;
            }
        ),
    ] {
        rejects(
            cursor_family(quote! {
                #[passive_state] let mut total = seed;
                #body
                Ok(total)
            }),
            "assignment requires a declared passive-state local",
        );
    }
    expand(cursor_family(quote! {
        #[passive_state] let mut total = seed;
        { let total = seed; }
        if let Some(total) = Some(seed) {} else { total = seed; }
        match Some(seed) { Some(total) => {}, None => { total = seed; } }
        let _ = matches!(Some(seed), Some(total));
        r#total = seed;
        Ok(total)
    }))?;
    rejects(
        cursor_family(quote! {
            { #[passive_state] let mut total = seed; }
            total = seed;
        }),
        "assignment requires a declared passive-state local",
    );
    for name in [
        quote!(effects),
        quote!(r#effects),
        quote!(facts),
        quote!(Some),
        quote!(None),
        quote!(r#Ok),
    ] {
        rejects(
            cursor_family(quote! {
                #[cursor_loop] while let Some(#name) = effects.advance(&mut cursor).await? {}
            }),
            "capability and passive constructor names cannot be shadowed",
        );
    }
    Ok(())
}

#[test]
fn new_markers_reject_duplicates_misplacement_and_conditional_use() -> Result<()> {
    for (body, diagnostic) in [
        (
            quote!(
                #[cursor_loop]
                #[cursor_loop]
                while let Some(item) = effects.advance(&mut cursor).await? {}
            ),
            "duplicate family attribute",
        ),
        (
            quote!(
                #[cursor_loop(value)]
                while let Some(item) = effects.advance(&mut cursor).await? {}
            ),
            "expected #[cursor_loop]",
        ),
        (
            quote!(#[passive_state] #[passive_state] let mut total = seed;),
            "duplicate family attribute",
        ),
        (
            quote!(#[passive_state(value)] let mut total = seed;),
            "expected #[passive_state]",
        ),
        (
            quote!(#[progress] let mut total = seed;),
            "family marker is not valid here",
        ),
        (
            quote!(#[passive_state] effects.child(seed).await?;),
            "family marker is not valid here",
        ),
        (
            quote!(#[cursor_loop] effects.child(seed).await?;),
            "family marker is not valid here",
        ),
        (
            quote!(
                #[cursor_loop]
                #[cfg(any())]
                while let Some(item) = effects.advance(&mut cursor).await? {}
            ),
            "cursor loop headers cannot be conditional",
        ),
        (
            quote!(#[passive_state] #[cfg_attr(any(), allow(unused_mut))] let mut total = seed;),
            "passive-state declarations cannot be conditional",
        ),
        (
            quote!(#[passive_state] let mut total = seed; #[cfg(any())] total = seed;),
            "passive-state assignments cannot be conditional",
        ),
        (
            quote!(
                #[cursor_loop]
                while let Some(item) = effects
                    .advance({
                        #[cfg(any())]
                        effects.child(seed).await?;
                        &mut cursor
                    })
                    .await?
                {}
            ),
            "cursor loop headers cannot be conditional",
        ),
    ] {
        rejects(cursor_family(body), diagnostic);
    }
    for (from, to) in [
        ("trait Effects", "#[progress] trait Effects"),
        ("type Error", "#[cursor_loop] type Error"),
        ("impl Facts", "#[passive_state] impl Facts"),
        ("fn combine", "#[progress] fn combine"),
        ("async fn reduce", "#[cursor_loop] async fn reduce"),
    ] {
        let source = cursor_family(quote!(Ok(seed))).to_string();
        assert!(source.contains(from));
        rejects(
            source
                .replace(from, to)
                .parse()
                .map_err(|error| syn::Error::new(Span::call_site(), error))?,
            "family marker is not valid here",
        );
    }
    Ok(())
}

#[test]
fn header_rhs_and_guard_expressions_keep_the_capability_rules() {
    for (escape, diagnostic) in [
        (
            quote!(effects),
            "value is not a local or declared passive value; capabilities cannot escape",
        ),
        (
            quote!(r#effects),
            "value is not a local or declared passive value; capabilities cannot escape",
        ),
        (
            quote!(unapproved()),
            "only passive constructors may be called directly",
        ),
        (
            quote!(cursor.advance()),
            "operation must use a declared method and its declared await boundary",
        ),
        (
            quote!(async { seed }),
            "expression is outside the shared capability grammar",
        ),
        (
            quote!(|| seed),
            "expression is outside the shared capability grammar",
        ),
    ] {
        for body in [
            quote!(#[passive_state] let mut total = seed; total = #escape;),
            quote!(#[cursor_loop] while let Some(item) = effects.advance(#escape).await? {}),
            quote!(#[cursor_loop] while let Some(item) = effects.advance(&mut cursor).await? { match item { value if { #escape; true } => {}, _ => {} } }),
        ] {
            rejects(cursor_family(body), diagnostic);
        }
    }
}

#[test]
fn copy_checks_accept_generic_state_and_cannot_shadow_the_checked_binding() -> Result<()> {
    let expanded = syn::parse2::<syn::File>(expand(quote! {
        #[synchronous(identity_sync)]
        #[capabilities()]
        #[passive_values()]
        async fn identity<T: Copy>(seed: T) -> T {
            #[passive_state]
            let mut __shared_semantic_require_copy = seed;
            __shared_semantic_require_copy = seed;
            __shared_semantic_require_copy
        }
    })?)?;
    let Some(Item::Fn(synchronous)) = expanded.items.last() else {
        return Err(syn::Error::new_spanned(
            &expanded,
            "missing synchronous identity",
        ));
    };
    let expected: syn::Block = syn::parse_quote! {{
        let mut __shared_semantic_require_copy = seed;
        {
            fn __shared_semantic_require_copy_state<T: ::core::marker::Copy>(_: &T) {}
            __shared_semantic_require_copy_state(&__shared_semantic_require_copy);
        }
        __shared_semantic_require_copy = seed;
        __shared_semantic_require_copy
    }};
    assert_eq!(*synchronous.block, expected);
    Ok(())
}
