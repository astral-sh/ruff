use proc_macro2::TokenStream;
use quote::quote;
use syn::Result;

use super::expand;

fn assert_expansion(original: &TokenStream, synchronous: &TokenStream) -> Result<()> {
    let actual = expand(TokenStream::new(), original)?;
    assert_eq!(
        syn::parse2::<syn::File>(actual)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn mapping_function() -> Result<()> {
    assert_expansion(
        &quote! {
            pub(crate) async fn apply_type_mapping_with<E: MappingEffects<'db>>(
                self, effects: &E,
            ) -> Result<Self, E::Error> {
                effects.checkpoint(Work::Dispatch).await?;
                let ty = self.class(db).apply_type_mapping_with(db, effects).await?;
                let root = ty.specialization_start_with(db, effects).await?;
                if matches!(root, Some(_)) {
                    return effects.legacy(Operation::Legacy, || ty);
                }
                effects.map_type(db, ty).await
            }
        },
        &quote! {
            pub(crate) fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(
                self, effects: &E,
            ) -> Result<Self, E::Error> {
                effects.checkpoint(Work::Dispatch)?;
                let ty = self.class(db).apply_type_mapping_sync(db, effects)?;
                let root = ty.specialization_start_sync(db, effects)?;
                if matches!(root, Some(_)) {
                    return effects.legacy(Operation::Legacy, || ty);
                }
                effects.map_type(db, ty)
            }
        },
    )
}

#[test]
fn mapping_start_and_completion_preserve_early_returns() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn mapping_start_with<E: MappingStartEffects<'db>>(self, effects: &E) -> Result<Start, E::Error> {
                effects.checkpoint(Work::Dispatch).await?;
                if matches!(self, Type::Never) {
                    return Ok(Start::Complete(self));
                }
                let mapped = effects.legacy(Operation::Legacy, || self)?;
                Ok(Start::Complete(mapped.complete_mapping_with(effects).await?))
            }
        },
        &quote! {
            fn mapping_start_sync<E: crate::types::mapping::effects::SynchronousMappingStartEffects<'db>>(self, effects: &E) -> Result<Start, E::Error> {
                effects.checkpoint(Work::Dispatch)?;
                if matches!(self, Type::Never) {
                    return Ok(Start::Complete(self));
                }
                let mapped = effects.legacy(Operation::Legacy, || self)?;
                Ok(Start::Complete(mapped.complete_mapping_sync(effects)?))
            }
        },
    )?;
    assert_expansion(
        &quote! {
            async fn complete_mapping_with<E>(self, effects: &E) -> Result<Self, E::Error>
            where E: MappingStartEffects<'db> {
                effects.checkpoint(Work::Publication).await?;
                Ok(self)
            }
        },
        &quote! {
            fn complete_mapping_sync<E>(self, effects: &E) -> Result<Self, E::Error>
            where E: crate::types::mapping::effects::SynchronousMappingStartEffects<'db> {
                effects.checkpoint(Work::Publication)?;
                Ok(self)
            }
        },
    )
}

#[test]
fn mapping_continuation_preserves_full_effects_and_scope_completion() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn resume_mapping_with<E: MappingEffects<'db>>(self, effects: &E) -> Result<Type<'db>, E::Error> {
                let child = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                let mapped = effects.finish_transformation(self.scope, child)?;
                mapped.complete_mapping_with(effects).await
            }
        },
        &quote! {
            fn resume_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(self, effects: &E) -> Result<Type<'db>, E::Error> {
                let child = effects.map_type(db, ty, mapping, tcx, visitor)?;
                let mapped = effects.finish_transformation(self.scope, child)?;
                mapped.complete_mapping_sync(effects)
            }
        },
    )?;
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(self, effects: &E) -> Result<Type<'db>, E::Error> {
                match self.mapping_start_with(db, mapping, visitor, effects).await? {
                    Start::Complete(mapped) => Ok(mapped),
                    Start::Continue(continuation) => continuation.resume_mapping_with(db, mapping, visitor, effects).await,
                }
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(self, effects: &E) -> Result<Type<'db>, E::Error> {
                match self.mapping_start_sync(db, mapping, visitor, effects)? {
                    Start::Complete(mapped) => Ok(mapped),
                    Start::Continue(continuation) => continuation.resume_mapping_sync(db, mapping, visitor, effects),
                }
            }
        },
    )
}

#[test]
fn specialization_composition_preserves_both_mapping_stages() -> Result<()> {
    assert_expansion(
        &quote! {
            pub(super) async fn apply_specialization_with<E: MappingEffects<'db>>(
                self,
                db: &'db dyn Db,
                other: Specialization<'db>,
                visitor: &ApplyTypeMappingVisitor<'_, 'db>,
                effects: &E,
            ) -> Result<Self, E::Error> {
                let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(other));
                let specialization = self.apply_type_mapping_with(db, &mapping, &[], visitor, effects).await?;
                if let Some(kind) = other.materialization_kind(db) {
                    specialization.apply_type_mapping_with(db, &TypeMapping::Materialize(kind), &[], visitor, effects).await
                } else {
                    Ok(specialization)
                }
            }
        },
        &quote! {
            pub(super) fn apply_specialization_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(
                self,
                db: &'db dyn Db,
                other: Specialization<'db>,
                visitor: &ApplyTypeMappingVisitor<'_, 'db>,
                effects: &E,
            ) -> Result<Self, E::Error> {
                let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(other));
                let specialization = self.apply_type_mapping_sync(db, &mapping, &[], visitor, effects)?;
                if let Some(kind) = other.materialization_kind(db) {
                    specialization.apply_type_mapping_sync(db, &TypeMapping::Materialize(kind), &[], visitor, effects)
                } else {
                    Ok(specialization)
                }
            }
        },
    )
}

#[test]
fn optional_specialization_preserves_fresh_mapping_visitors() -> Result<()> {
    assert_expansion(
        &quote! {
            pub(crate) async fn apply_optional_specialization_with<E: MappingEffects<'db>>(
                self,
                db: &'db dyn Db,
                specialization: Option<Specialization<'db>>,
                effects: &E,
            ) -> Result<Self, E::Error> {
                let Some(specialization) = specialization else { return Ok(self); };
                let env = specialization.context(db).program(db).environment(db);
                let visitor = ApplyTypeMappingVisitor::new(env);
                let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
                let base = self.apply_type_mapping_with(db, &mapping, TypeContext::default(), &visitor, effects).await?;
                if let Some(kind) = specialization.materialization_kind(db) {
                    let visitor = ApplyTypeMappingVisitor::new(env);
                    base.apply_type_mapping_with(db, &TypeMapping::Materialize(kind), TypeContext::default(), &visitor, effects).await
                } else {
                    Ok(base)
                }
            }
        },
        &quote! {
            pub(crate) fn apply_optional_specialization_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(
                self,
                db: &'db dyn Db,
                specialization: Option<Specialization<'db>>,
                effects: &E,
            ) -> Result<Self, E::Error> {
                let Some(specialization) = specialization else { return Ok(self); };
                let env = specialization.context(db).program(db).environment(db);
                let visitor = ApplyTypeMappingVisitor::new(env);
                let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization));
                let base = self.apply_type_mapping_sync(db, &mapping, TypeContext::default(), &visitor, effects)?;
                if let Some(kind) = specialization.materialization_kind(db) {
                    let visitor = ApplyTypeMappingVisitor::new(env);
                    base.apply_type_mapping_sync(db, &TypeMapping::Materialize(kind), TypeContext::default(), &visitor, effects)
                } else {
                    Ok(base)
                }
            }
        },
    )
}

#[test]
fn specialization_method_calls_use_existing_mapping_lowering() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                let specialization = specialization.apply_specialization_with(db, other, visitor, effects).await?;
                self.apply_optional_specialization_with(db, Some(specialization), effects).await
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                let specialization = specialization.apply_specialization_sync(db, other, visitor, effects)?;
                self.apply_optional_specialization_sync(db, Some(specialization), effects)
            }
        },
    )
}

#[test]
fn mapping_methods_require_direct_await_and_bare_effects() {
    for (method, arguments) in [
        (
            quote!(apply_specialization_with),
            quote!(db, other, visitor),
        ),
        (
            quote!(apply_optional_specialization_with),
            quote!(db, specialization),
        ),
        (quote!(mapping_start_with), quote!(db, mapping, visitor)),
        (quote!(resume_mapping_with), quote!(db, mapping, visitor)),
        (quote!(complete_mapping_with), quote!(db)),
    ] {
        for body in [
            quote!(self.#method(#arguments, effects)),
            quote!(self.#method(#arguments).await),
            quote!(self.#method(#arguments, &effects).await),
            quote!(self.#method(#arguments, (effects)).await),
            quote!(self.#method(#arguments, r#effects).await),
            quote!(self.#method(#arguments, alias).await),
            quote!(self.#method(#arguments, effects, other).await),
            quote!(let deferred = || self.#method(#arguments, effects).await;),
            quote!(async { self.#method(#arguments, effects).await }),
            quote!(matches!(self.#method(#arguments, effects).await, _)),
            quote!(self.#method(effects, #arguments, effects).await),
        ] {
            let original = quote! {
                async fn apply_type_mapping_with<E: MappingEffects<'db>>(self, effects: &E) {
                    #body
                }
            };
            assert!(
                expand(TokenStream::new(), &original).is_err(),
                "accepted {body}"
            );
        }
    }
}

#[test]
fn declared_callback() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn map_types_with<E: MappingEffects<'db>>(
                self, mut map: impl AsyncFnMut(usize, Type) -> Result<Type, E::Error>, effects: &E,
            ) -> Result<Type, E::Error> {
                effects.checkpoint(Work::Advance).await?;
                map(0, ty).await
            }
        },
        &quote! {
            fn map_types_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(
                self, mut map: impl FnMut(usize, Type) -> Result<Type, E::Error>, effects: &E,
            ) -> Result<Type, E::Error> {
                effects.checkpoint(Work::Advance)?;
                map(0, ty)
            }
        },
    )
}

#[test]
fn async_argument_closure() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                self.map_types_with(db, async move |index, typevar, ty| {
                    effects.checkpoint(Work::Child).await?;
                    effects.map_type(db, ty).await
                }, effects).await
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                self.map_types_sync(db, move |index, typevar, ty| {
                    effects.checkpoint(Work::Child)?;
                    effects.map_type(db, ty)
                }, effects)
            }
        },
    )
}

#[test]
fn promotion_helper_and_stored_facts() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn promote_impl_with<E: MappingEffects<'db>>(
                self, db: &'db dyn Db, env: &Environment<'db>, effects: &E,
            ) -> Result<Type<'db>, E::Error> {
                effects.checkpoint(Work::ScalarFallback).await?;
                effects.scalar_fallback(db, env, class)
            }
        },
        &quote! {
            fn promote_impl_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(
                self, db: &'db dyn Db, env: &Environment<'db>, effects: &E,
            ) -> Result<Type<'db>, E::Error> {
                effects.checkpoint(Work::ScalarFallback)?;
                effects.scalar_fallback(db, env, class)
            }
        },
    )?;
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                let literal = self.promote_impl_with(db, env, effects).await?;
                self.map_types_with(db, async |index, typevar, ty| {
                    effects.checkpoint(Work::Variance).await?;
                    let variance = effects.variance(db, typevar)?;
                    effects.map_type(db, ty, variance).await
                }, effects).await
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(self, effects: &E) -> Result<Self, E::Error> {
                let literal = self.promote_impl_sync(db, env, effects)?;
                self.map_types_sync(db, |index, typevar, ty| {
                    effects.checkpoint(Work::Variance)?;
                    let variance = effects.variance(db, typevar)?;
                    effects.map_type(db, ty, variance)
                }, effects)
            }
        },
    )
}

#[test]
fn rejects_deferred_facts_and_provider_escapes() {
    let cases = [
        (
            quote! { effects.variance(db, variable).await; },
            "fact methods must be called without await",
        ),
        (
            quote! { effects.scalar_fallback(db, env, class).await; },
            "fact methods must be called without await",
        ),
        (
            quote! { let deferred = || effects.variance(db, variable); },
            "fact calls are only supported",
        ),
        (
            quote! { effects.legacy(Operation::Legacy, || effects.variance(db, variable)); },
            "fact calls are only supported",
        ),
        (
            quote! { fn nested() { effects.scalar_fallback(db, env, class); } },
            "fact calls are only supported",
        ),
        (
            quote! { const { effects.variance(db, variable) }; },
            "fact calls are only supported",
        ),
        (
            quote! { let value: [(); effects.variance(db, variable)] = []; },
            "fact calls are only supported",
        ),
        (
            quote! { helper::<{ effects.variance(db, variable) }>(); },
            "fact calls are only supported",
        ),
        (quote! { let alias = effects; }, "effects may only be used"),
        (quote! { let alias = &effects; }, "effects may only be used"),
        (quote! { accept(effects); }, "effects may only be used"),
        (
            quote! { MappingFacts::variance(effects, db, variable); },
            "effects may only be used",
        ),
        (
            quote! { <E as MappingFacts>::variance(effects, db, variable); },
            "effects may only be used",
        ),
        (
            quote! { effects.unknown_fact(); },
            "unawaited calls on effects require",
        ),
        (
            quote! { effects.checkpoint(Work::Begin); },
            "unawaited calls on effects require",
        ),
        (
            quote! { self.promote_impl_with(db, env, effects); },
            "mapping methods must be awaited directly",
        ),
        (
            quote! { self.promote_impl_with(db, env, other).await; },
            "effects as their final argument",
        ),
        (
            quote! { self.promote_impl_with().await; },
            "effects as their final argument",
        ),
    ];
    for (body, message) in cases {
        let input = quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) {
                #body
            }
        };
        let error =
            expand(TokenStream::new(), &input).expect_err("invalid fact access was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn legacy_boundaries_remain_usable_in_deferred_closures() -> Result<()> {
    let body = quote! {
        tuple.map(|tuple| effects.legacy(Operation::Tuple, || tuple.map(db)))
    };
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) {
                #body
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(effects: &E) {
                #body
            }
        },
    )
}

#[test]
fn unsupported_control_flow() {
    let cases = [
        (
            quote! { effects.unknown().await; },
            "await requires a declared mapping method",
        ),
        (
            quote! { other.checkpoint().await; },
            "await requires a declared mapping method",
        ),
        (
            quote! { other.map_type().await; },
            "await requires a declared mapping method",
        ),
        (
            quote! { map(ty).await; },
            "await is outside the dual_mapping manifest",
        ),
        (
            quote! { unknown().await; },
            "await is outside the dual_mapping manifest",
        ),
        (
            quote! { let future = async { ty }; },
            "async blocks are not supported",
        ),
        (
            quote! { let callback = async |ty| ty; },
            "async closures are only supported",
        ),
        (
            quote! { self.other(async |ty| ty); },
            "async closures are only supported",
        ),
        (
            quote! { self.map_types_with(db, callback, effects).await; },
            "requires an async closure",
        ),
        (
            quote! { let callback = || effects.checkpoint().await; },
            "await is only supported in the mapping body",
        ),
        (
            quote! { async fn nested() {} },
            "nested async items are not supported",
        ),
        (
            quote! { hidden!(); },
            "only checked matches! expressions are supported",
        ),
        (
            quote! { matches!(effects.checkpoint().await, _); },
            "await is only supported in the mapping body",
        ),
        (
            quote! { matches!(async { ty }, _); },
            "async blocks are not supported",
        ),
        (
            quote! { matches!(hidden!(), _); },
            "matches! cannot contain nested macros",
        ),
        (
            quote! { matches!(value, hidden!()); },
            "matches! cannot contain nested macros",
        ),
        (
            quote! { matches!(value, _ if hidden!()); },
            "matches! cannot contain nested macros",
        ),
        (
            quote! { matches!(value, _ if effects.checkpoint().await); },
            "await is only supported in the mapping body",
        ),
    ];
    for (body, message) in cases {
        let input = quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) {
                #body
            }
        };
        let error =
            expand(TokenStream::new(), &input).expect_err("unsupported syntax was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn matches_operators_and_patterns() -> Result<()> {
    let body = quote! {
        let comparison = matches!(index != 0, true);
        let negated_guard = matches!(value, _ if !condition);
        let alternatives = matches!(value, | Some(_) | None,);
        let grouped_guard = matches!(value, Some(inner) if !(inner == 0),);
        Ok(())
    };
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) {
                #body
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(effects: &E) {
                #body
            }
        },
    )
}

#[test]
fn rejects_shadowed_effect_and_callback_bindings() {
    let cases = [
        quote! { let effects = other; },
        quote! { let r#effects = other; },
        quote! { let map = other; },
        quote! { let (map, _) = other; },
        quote! { let callback = |effects| effects; },
        quote! { let callback = |map| map; },
        quote! { match value { Some(effects) => (), _ => () } },
        quote! { matches!(value, map if condition); },
    ];
    for body in cases {
        let input = quote! {
            async fn map_types_with<E: MappingEffects<'db>>(
                effects: &E, map: impl AsyncFnMut() -> Result<(), E::Error>,
            ) {
                #body
            }
        };
        let error = expand(TokenStream::new(), &input).expect_err("shadowed binding was accepted");
        assert!(
            error.to_string().contains("body bindings cannot shadow"),
            "{error}"
        );
    }
}

#[test]
fn rejects_hidden_signature_syntax() {
    let cases = [
        (
            quote! { hidden!() },
            "only checked matches! expressions are supported",
        ),
        (
            quote! { [(); { async fn nested() {} 1 }] },
            "nested async items are not supported",
        ),
        (
            quote! { [(); { let _ = async {}; 1 }] },
            "async blocks are not supported",
        ),
        (
            quote! { [(); { let _ = effects.checkpoint().await; 1 }] },
            "await is only supported in the mapping body",
        ),
    ];
    for (parameter_type, message) in cases {
        let input = quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(
                effects: &E, _: #parameter_type,
            ) {}
        };
        let error =
            expand(TokenStream::new(), &input).expect_err("hidden signature syntax was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn rejects_unknown_attribute_contracts() {
    let cases = [
        (
            quote! { option },
            quote! { async fn apply_type_mapping_with<E: MappingEffects<'db>>() {} },
            "takes no arguments",
        ),
        (
            quote! {},
            quote! { async fn unknown<E: MappingEffects<'db>>() {} },
            "not in the dual_mapping manifest",
        ),
        (
            quote! {},
            quote! { async fn unknown_with<E: MappingStartEffects<'db>>() {} },
            "not in the dual_mapping manifest",
        ),
        (
            quote! {},
            quote! { async fn mapping_start_with<E: UnknownEffects<'db>>() {} },
            "requires a MappingEffects or MappingStartEffects bound",
        ),
        (
            quote! {},
            quote! { fn apply_type_mapping_with<E: MappingEffects<'db>>() {} },
            "requires an async function",
        ),
        (
            quote! {},
            quote! { async fn map_types_with<E: MappingEffects<'db>>(map: impl FnMut()) {} },
            "requires a map parameter with an AsyncFnMut bound",
        ),
    ];
    for (arguments, input, message) in cases {
        let error = expand(arguments, &input).expect_err("unsupported contract was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn mapping_start_rejects_unknown_awaits_and_provider_escapes() {
    for body in [
        quote! { self.unknown_with(effects).await; },
        quote! { effects.unknown().await; },
        quote! { let alias = effects; },
        quote! { let effects = other; },
        quote! { async { effects.checkpoint(Work::Dispatch).await; }; },
        quote! { effects.begin_transformation(db, ty, mapping, visitor).await; },
    ] {
        let function = quote! {
            async fn mapping_start_with<E: MappingStartEffects<'db>>(self, effects: &E) {
                #body
            }
        };
        assert!(
            expand(TokenStream::new(), &function).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn mapping_self_decision_and_transformation_scopes() -> Result<()> {
    assert_expansion(
        &quote! {
            async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) -> Result<Type<'db>, E::Error> {
                if effects.should_bind_self(db, env, binding, variable).await? {
                    let scope = effects.begin_transformation(db, ty, mapping, visitor)?;
                    let child = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                    effects.finish_transformation(scope, child)
                } else { Ok(ty) }
            }
        },
        &quote! {
            fn apply_type_mapping_sync<E: crate::types::mapping::effects::SynchronousMappingEffects<'db>>(effects: &E) -> Result<Type<'db>, E::Error> {
                if effects.should_bind_self(db, env, binding, variable)? {
                    let scope = effects.begin_transformation(db, ty, mapping, visitor)?;
                    let child = effects.map_type(db, ty, mapping, tcx, visitor)?;
                    effects.finish_transformation(scope, child)
                } else { Ok(ty) }
            }
        },
    )
}

#[test]
fn rejects_deferred_or_awaited_transformation_facts() {
    for body in [
        quote! { effects.begin_transformation(db, ty, mapping, visitor).await?; },
        quote! { effects.finish_transformation(scope, ty).await?; },
        quote! { let deferred = || effects.begin_transformation(db, ty, mapping, visitor); },
        quote! { effects.should_bind_self(db, env, binding, variable); },
    ] {
        let function = quote! { async fn apply_type_mapping_with<E: MappingEffects<'db>>(effects: &E) { #body } };
        assert!(expand(TokenStream::new(), &function).is_err());
    }
}
