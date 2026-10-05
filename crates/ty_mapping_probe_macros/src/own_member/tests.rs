use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::Result;

use super::member_lookup::LookupManifest;
use super::{
    expand, expand_base_mro, expand_c3, expand_class_type, expand_constraint_type, expand_mro,
    expand_mro_iteration, expand_mro_root, expand_promotion, expand_protocol_interface,
    expand_protocol_members_defined, expand_protocol_object, expand_protocol_relation,
    expand_satisfaction, expand_static_mro, expand_synthesized,
};

fn promotion(name: &str, body: &TokenStream) -> TokenStream {
    let name = Ident::new(name, Span::call_site());
    quote! {
        pub(crate) async fn #name<E: PublicPromotionEffects<'db>>(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            effects: &E,
        ) -> Result<Type<'db>, E::Error> {
            #body
        }
    }
}

fn protocol_candidates(body: &TokenStream) -> TokenStream {
    quote! {
        async fn for_each_protocol_member_candidate_with<'db, C, E: ProtocolCandidateEffects<'db, C>>(
            class: ClassType<'db>,
            env: &ProgramEnvironment<'db>,
            consumer: &mut C,
            effects: &E,
        ) -> Result<(), E::Error> { #body }
    }
}

fn protocol_candidate(body: &TokenStream) -> TokenStream {
    quote! {
        async fn protocol_interface_candidate_with<'db, E: ProtocolInterfaceEffects<'db>>(
            env: &ProgramEnvironment<'db>,
            build: &mut ProtocolInterfaceBuild<'db>,
            name: &Name,
            candidate: ProtocolMemberCandidate<'db>,
            specialization: Option<Specialization<'db>>,
            effects: &E,
        ) -> Result<(), E::Error> { #body }
    }
}

fn protocol_build(body: &TokenStream) -> TokenStream {
    quote! {
        async fn protocol_interface_build_with<'db, E: ProtocolInterfaceEffects<'db>>(
            class: ClassType<'db>,
            effects: &E,
        ) -> Result<PreparedProtocolInterface<'db>, E::Error> { #body }
    }
}

fn protocol_normalize(body: &TokenStream) -> TokenStream {
    quote! {
        async fn protocol_interface_normalize_with<'db, E: ProtocolInterfaceNormalizationEffects<'db>>(
            env: &ProgramEnvironment<'db>,
            previous: &BTreeMap<Name, ProtocolMemberData<'db>>,
            current: &BTreeMap<Name, ProtocolMemberData<'db>>,
            cycle: &salsa::Cycle<'_>,
            effects: &E,
        ) -> Result<BTreeMap<Name, ProtocolMemberData<'db>>, E::Error> { #body }
    }
}

fn protocol_object(body: &TokenStream) -> TokenStream {
    quote! {
        async fn protocol_object_equivalence_with<'db, E: ProtocolObjectEffects<'db>>(
            fields: RelationFieldReads<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<bool, E::Error> { #body }
    }
}

#[test]
fn protocol_object_preserves_exclusions_before_comparison() -> Result<()> {
    let original = protocol_object(&quote! {
        effects.checkpoint(Entry).await?;
        let interface = effects.protocol_interface(protocol).await?;
        effects.checkpoint(HashName).await?;
        if fields.protocol_interface_includes_member(interface, "__hash__") {
            effects.checkpoint(Complete).await?;
            return Ok(false);
        }
        effects.checkpoint(DictName).await?;
        if fields.protocol_interface_includes_member(interface, "__dict__") {
            effects.checkpoint(Complete).await?;
            return Ok(false);
        }
        let program = fields.protocol_interface_program(interface);
        let result = effects.compare_object(program, protocol).await?;
        effects.checkpoint(Complete).await?;
        Ok(result)
    });
    let synchronous = quote! {
        fn protocol_object_equivalence_sync<'db, E: crate::types::instance::protocol_object::SyncProtocolObjectEffects<'db>>(
            fields: RelationFieldReads<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<bool, E::Error> {
            effects.checkpoint(Entry)?;
            let interface = effects.protocol_interface(protocol)?;
            effects.checkpoint(HashName)?;
            if fields.protocol_interface_includes_member(interface, "__hash__") {
                effects.checkpoint(Complete)?;
                return Ok(false);
            }
            effects.checkpoint(DictName)?;
            if fields.protocol_interface_includes_member(interface, "__dict__") {
                effects.checkpoint(Complete)?;
                return Ok(false);
            }
            let program = fields.protocol_interface_program(interface);
            let result = effects.compare_object(program, protocol)?;
            effects.checkpoint(Complete)?;
            Ok(result)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_object(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_object_rejects_deferred_effects_and_helper_escapes() {
    for body in [
        quote!(effects.checkpoint(work)),
        quote!(effects.unknown().await),
        quote!(other.checkpoint(work).await),
        quote!(E::checkpoint(effects, work).await),
        quote!(let alias = effects; alias.checkpoint(work).await),
        quote!(let effects = other;),
        quote!(let callback = || effects.compare_object(program, protocol).await;),
        quote!(async { effects.protocol_interface(protocol).await }),
        quote!(matches!(effects.checkpoint(work).await, _)),
        quote!(protocol_interface_build_with(class, effects).await),
        quote!(for_each_protocol_member_candidate_with(class, env, consumer, effects).await),
    ] {
        assert!(
            expand_protocol_object(TokenStream::new(), &protocol_object(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn protocol_object_signature_and_attribute_remain_closed() -> Result<()> {
    let original = syn::parse2::<syn::ItemFn>(protocol_object(&quote!()))?;
    let mut modified = original.clone();
    modified
        .sig
        .inputs
        .insert(0, syn::parse_quote!(db: &'db dyn Db));
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.asyncness = None;
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.ident = syn::parse_quote!(other_protocol_with);
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.inputs.pop();
    modified.sig.inputs.push(syn::parse_quote!(effects: &mut E));
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.output = syn::parse_quote!(-> Result<(), OtherError>);
    assert!(expand_protocol_object(TokenStream::new(), &quote!(#modified)).is_err());
    assert!(expand_protocol_object(quote!(option), &quote!(#original)).is_err());
    assert!(expand_protocol_interface(TokenStream::new(), &quote!(#original)).is_err());
    assert!(expand_protocol_object(TokenStream::new(), &protocol_build(&quote!())).is_err());
    Ok(())
}

#[test]
fn protocol_normalization_preserves_lookup_and_lazy_member_requests() -> Result<()> {
    let original = protocol_normalize(&quote! {
        let mut members = BTreeMap::new();
        for (name, member) in current {
            effects.checkpoint(work).await?;
            let member = if let Some(previous) = previous.get(name) {
                effects.normalize_member(env, member, previous, cycle).await?
            } else {
                member.clone()
            };
            members.insert(name.clone(), member);
        }
        Ok(members)
    });
    let synchronous = quote! {
        fn protocol_interface_normalize_sync<'db, E: crate::types::protocol_class::interface_build::SyncProtocolInterfaceNormalizationEffects<'db>>(
            env: &ProgramEnvironment<'db>,
            previous: &BTreeMap<Name, ProtocolMemberData<'db>>,
            current: &BTreeMap<Name, ProtocolMemberData<'db>>,
            cycle: &salsa::Cycle<'_>,
            effects: &E,
        ) -> Result<BTreeMap<Name, ProtocolMemberData<'db>>, E::Error> {
            let mut members = BTreeMap::new();
            for (name, member) in current {
                effects.checkpoint(work)?;
                let member = if let Some(previous) = previous.get(name) {
                    effects.normalize_member(env, member, previous, cycle)?
                } else {
                    member.clone()
                };
                members.insert(name.clone(), member);
            }
            Ok(members)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_interface(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_candidates_preserve_streaming_loops_and_native_merge_closures() -> Result<()> {
    let original = protocol_candidates(&quote! {
        let mut cursor = effects.mro_start(class).await?;
        while let Some(base) = effects.mro_next(&mut cursor).await? {
            if let Some(class) = base.into_class()
                && let Some((scope, specialization)) = effects.protocol_scope(class).await?
            {
                let use_def = effects.use_def_map(scope).await?;
                let places = effects.place_table(scope).await?;
                let mut direct = FxHashMap::default();
                for (symbol, bindings) in use_def.all_end_of_scope_symbol_bindings() {
                    effects.checkpoint(Symbol).await?;
                    let value = effects.binding_place(env, bindings).await?;
                    direct.insert(symbol, value);
                }
                for (symbol, declarations) in use_def.all_end_of_scope_symbol_declarations() {
                    let imported = use_def.end_of_scope_imported_final_candidates(symbol.into());
                    let value = effects.declaration_place(env, declarations, imported).await?;
                    direct.entry(symbol).and_modify(|candidate| candidate.ty = value.ty);
                }
                for (symbol, candidate) in direct {
                    let name = places.symbol(symbol).name();
                    if excluded_from_proto_members(name) { continue; }
                    effects.visit_candidate(env, consumer, name, candidate, specialization).await?;
                }
            }
        }
        Ok(())
    });
    let synchronous = quote! {
        fn for_each_protocol_member_candidate_sync<'db, C, E: crate::types::protocol_class::interface_build::SyncProtocolCandidateEffects<'db, C>>(
            class: ClassType<'db>,
            env: &ProgramEnvironment<'db>,
            consumer: &mut C,
            effects: &E,
        ) -> Result<(), E::Error> {
            let mut cursor = effects.mro_start(class)?;
            while let Some(base) = effects.mro_next(&mut cursor)? {
                if let Some(class) = base.into_class()
                    && let Some((scope, specialization)) = effects.protocol_scope(class)?
                {
                    let use_def = effects.use_def_map(scope)?;
                    let places = effects.place_table(scope)?;
                    let mut direct = FxHashMap::default();
                    for (symbol, bindings) in use_def.all_end_of_scope_symbol_bindings() {
                        effects.checkpoint(Symbol)?;
                        let value = effects.binding_place(env, bindings)?;
                        direct.insert(symbol, value);
                    }
                    for (symbol, declarations) in use_def.all_end_of_scope_symbol_declarations() {
                        let imported = use_def.end_of_scope_imported_final_candidates(symbol.into());
                        let value = effects.declaration_place(env, declarations, imported)?;
                        direct.entry(symbol).and_modify(|candidate| candidate.ty = value.ty);
                    }
                    for (symbol, candidate) in direct {
                        let name = places.symbol(symbol).name();
                        if excluded_from_proto_members(name) { continue; }
                        effects.visit_candidate(env, consumer, name, candidate, specialization)?;
                    }
                }
            }
            Ok(())
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_interface(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_candidate_preserves_shadowing_and_classification_guards() -> Result<()> {
    let original = protocol_candidate(&quote! {
        effects.checkpoint(NameLookup).await?;
        if build.members.contains_key(name) { return Ok(()); }
        let specialization = if let Some(specialization) = specialization {
            Some(effects.with_typevar_bounds(specialization).await?)
        } else { None };
        let ty = effects.specialize_candidate_type(candidate.ty, specialization).await?;
        let member = match ty {
            Type::PropertyInstance(property) => {
                let (getter, setter) = effects.property_accessors(property).await?;
                property_member(getter, setter)
            }
            Type::Callable(callable)
                if candidate.bound_on_class.is_yes() && effects.callable_is_method_like(callable).await? =>
            {
                effects.method_member(env, callable, candidate.definition).await?
            }
            Type::FunctionLiteral(function)
                if candidate.bound_on_class.is_yes()
                    || effects.function_is_staticmethod(function).await?
                    || effects.function_is_classmethod(function).await? =>
            {
                let callable = effects.function_callable(function).await?;
                effects.method_member(env, callable, candidate.definition).await?
            }
            _ => {
                if candidate.bound_on_class.is_yes()
                    && let Some(definition) = candidate.definition
                    && effects.definition_is_function(definition).await?
                    && let Some(member) = effects.descriptor_member(env, ty, build.class, candidate.definition).await?
                { member } else { attribute_member(candidate) }
            }
        };
        build.members.insert(name.clone(), member);
        Ok(())
    });
    let synchronous = quote! {
        fn protocol_interface_candidate_sync<'db, E: crate::types::protocol_class::interface_build::SyncProtocolInterfaceEffects<'db>>(
            env: &ProgramEnvironment<'db>,
            build: &mut ProtocolInterfaceBuild<'db>,
            name: &Name,
            candidate: ProtocolMemberCandidate<'db>,
            specialization: Option<Specialization<'db>>,
            effects: &E,
        ) -> Result<(), E::Error> {
            effects.checkpoint(NameLookup)?;
            if build.members.contains_key(name) { return Ok(()); }
            let specialization = if let Some(specialization) = specialization {
                Some(effects.with_typevar_bounds(specialization)?)
            } else { None };
            let ty = effects.specialize_candidate_type(candidate.ty, specialization)?;
            let member = match ty {
                Type::PropertyInstance(property) => {
                    let (getter, setter) = effects.property_accessors(property)?;
                    property_member(getter, setter)
                }
                Type::Callable(callable)
                    if candidate.bound_on_class.is_yes() && effects.callable_is_method_like(callable)? =>
                {
                    effects.method_member(env, callable, candidate.definition)?
                }
                Type::FunctionLiteral(function)
                    if candidate.bound_on_class.is_yes()
                        || effects.function_is_staticmethod(function)?
                        || effects.function_is_classmethod(function)? =>
                {
                    let callable = effects.function_callable(function)?;
                    effects.method_member(env, callable, candidate.definition)?
                }
                _ => {
                    if candidate.bound_on_class.is_yes()
                        && let Some(definition) = candidate.definition
                        && effects.definition_is_function(definition)?
                        && let Some(member) = effects.descriptor_member(env, ty, build.class, candidate.definition)?
                    { member } else { attribute_member(candidate) }
                }
            };
            build.members.insert(name.clone(), member);
            Ok(())
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_interface(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_build_lowers_the_declared_candidate_helper() -> Result<()> {
    let original = protocol_build(&quote! {
        let env = effects.environment(class).await?;
        effects.checkpoint(Build).await?;
        let mut build = ProtocolInterfaceBuild::new(class);
        for_each_protocol_member_candidate_with(class, &env, &mut build, effects).await?;
        Ok(PreparedProtocolInterface { env, members: build.members })
    });
    let synchronous = quote! {
        fn protocol_interface_build_sync<'db, E: crate::types::protocol_class::interface_build::SyncProtocolInterfaceEffects<'db>>(
            class: ClassType<'db>,
            effects: &E,
        ) -> Result<PreparedProtocolInterface<'db>, E::Error> {
            let env = effects.environment(class)?;
            effects.checkpoint(Build)?;
            let mut build = ProtocolInterfaceBuild::new(class);
            for_each_protocol_member_candidate_sync(class, &env, &mut build, effects)?;
            Ok(PreparedProtocolInterface { env, members: build.members })
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_interface(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_interface_rejects_deferred_effects_and_helper_escapes() {
    for make in [
        protocol_candidates,
        protocol_candidate,
        protocol_build,
        protocol_normalize,
    ] {
        for body in [
            quote!(effects.checkpoint(work)),
            quote!(effects.unknown().await),
            quote!(other.checkpoint(work).await),
            quote!(E::checkpoint(effects, work).await),
            quote!(let alias = effects; alias.checkpoint(work).await),
            quote!(let effects = other;),
            quote!(let callback = || effects.checkpoint(work).await;),
            quote!(let callback = async || effects.checkpoint(work).await;),
            quote!(async { effects.checkpoint(work).await }),
            quote!(matches!(effects.checkpoint(work).await, _)),
            quote!(
                effects
                    .checkpoint::<{
                        effects.checkpoint(work).await;
                        1
                    }>(work)
                    .await
            ),
            quote!(let alias = for_each_protocol_member_candidate_with;),
            quote!(let for_each_protocol_member_candidate_with = other;),
            quote!(
                module::for_each_protocol_member_candidate_with(class, env, consumer, effects)
                    .await
            ),
            quote!(for_each_protocol_member_candidate_with(
                class, env, consumer, effects
            )),
            quote!(for_each_protocol_member_candidate_with(class, env, effects).await),
            quote!(for_each_protocol_member_candidate_with(class, env, consumer, (effects)).await),
        ] {
            assert!(
                expand_protocol_interface(TokenStream::new(), &make(&body)).is_err(),
                "accepted {body}",
            );
        }
    }
    for original in [
        protocol_candidates(&quote!(effects.environment(class).await)),
        protocol_candidate(&quote!(effects.mro_next(cursor).await)),
        protocol_build(&quote!(
            effects
                .visit_candidate(env, consumer, name, candidate, specialization)
                .await
        )),
        protocol_candidate(&quote!(
            for_each_protocol_member_candidate_with(class, env, consumer, effects).await
        )),
        protocol_candidates(&quote!(
            for_each_protocol_member_candidate_with(class, env, consumer, effects).await
        )),
    ] {
        assert!(expand_protocol_interface(TokenStream::new(), &original).is_err());
    }
}

#[test]
fn protocol_interface_signatures_and_attribute_remain_closed() -> Result<()> {
    for make in [
        protocol_candidates,
        protocol_candidate,
        protocol_build,
        protocol_normalize,
    ] {
        let original = syn::parse2::<syn::ItemFn>(make(&quote!()))?;
        let mut modified = original.clone();
        modified
            .sig
            .inputs
            .insert(0, syn::parse_quote!(db: &'db dyn Db));
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.asyncness = None;
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.ident = syn::parse_quote!(other_protocol_with);
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.pop();
        modified.sig.inputs.push(syn::parse_quote!(effects: &mut E));
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.output = syn::parse_quote!(-> Result<(), OtherError>);
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
        assert!(expand_protocol_interface(quote!(option), &quote!(#original)).is_err());
        assert!(expand_mro(TokenStream::new(), &quote!(#original)).is_err());
    }
    let mut modified = syn::parse2::<syn::ItemFn>(protocol_candidates(&quote!()))?;
    modified.sig.generics = syn::parse_quote!(<'db, C: Send, E: ProtocolCandidateEffects<'db, C>>);
    assert!(expand_protocol_interface(TokenStream::new(), &quote!(#modified)).is_err());
    assert!(expand_protocol_interface(TokenStream::new(), &mro_member(&quote!())).is_err());
    Ok(())
}

#[test]
fn public_promotion_lowers_only_declared_dependencies() -> Result<()> {
    // Singleton classification is an awaited dependency whose synchronous form is generated.
    for (name, synchronous_name, body, synchronous_body) in [
        (
            "promote_public_with",
            "promote_public_sync",
            quote! {
                effects.checkpoint(Admission).await?;
                let mapped = effects.regular(db, env, self).await?;
                mapped.promote_singletons_impl_with(db, env, effects).await
            },
            quote! {
                effects.checkpoint(Admission)?;
                let mapped = effects.regular(db, env, self)?;
                mapped.promote_singletons_impl_sync(db, env, effects)
            },
        ),
        (
            "promote_singletons_impl_with",
            "promote_singletons_impl_sync",
            quote! {
                effects.checkpoint(SingletonDispatch).await?;
                let Type::NominalInstance(instance) = self else { return Ok(self); };
                effects.checkpoint(SingletonClassification).await?;
                if effects.is_singleton(db, instance).await? {
                    effects.union_two(db, env, self, Type::unknown()).await
                } else {
                    Ok(self)
                }
            },
            quote! {
                effects.checkpoint(SingletonDispatch)?;
                let Type::NominalInstance(instance) = self else { return Ok(self); };
                effects.checkpoint(SingletonClassification)?;
                if effects.is_singleton(db, instance)? {
                    effects.union_two(db, env, self, Type::unknown())
                } else {
                    Ok(self)
                }
            },
        ),
    ] {
        let original = promotion(name, &body);
        let synchronous_name = Ident::new(synchronous_name, Span::call_site());
        let synchronous = quote! {
            pub(crate) fn #synchronous_name<E: crate::types::promotion::SynchronousPublicPromotionEffects<'db>>(
                self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, effects: &E,
            ) -> Result<Type<'db>, E::Error> {
                #synchronous_body
            }
        };
        assert_eq!(
            syn::parse2::<syn::File>(expand_promotion(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(quote!(#original #synchronous))?,
        );
    }
    Ok(())
}

#[test]
fn promotion_helpers_forbid_deferred_calls_and_provider_escapes() {
    // Promotion dependencies cannot escape into deferred code or bypass their awaited effects.
    for (name, bodies) in [
        (
            "promote_public_with",
            vec![
                quote!(self.promote_singletons_impl_with(db, env, effects)),
                quote!(self.promote_singletons_impl_with(db, effects).await),
                quote!(self.promote_singletons_impl_with(db, env, (effects)).await),
                quote!(let alias = effects; self.promote_singletons_impl_with(db, env, alias).await),
                quote!(effects.promote_singletons_impl_with(db, env, effects).await),
                quote!(
                    self.promote_singletons_impl_with(effects, env, effects)
                        .await
                ),
                quote!(
                    self.promote_singletons_impl_with::<{
                        effects.regular(db, env, self);
                        1
                    }>(db, env, effects)
                        .await
                ),
                quote!(effects.regular(db, env, self)),
                quote!(effects.union_two(db, env, self, self).await),
                quote!(other(self, effects).await),
                quote!(async { self.promote_singletons_impl_with(db, env, effects).await }),
            ],
        ),
        (
            "promote_singletons_impl_with",
            vec![
                quote!(effects.is_singleton(db, instance)),
                quote!(async { effects.is_singleton(db, instance).await }),
                quote!(instance.is_singleton_with(db, effects)),
                quote!(instance.is_singleton_with(db, effects).await),
                quote!(instance.is_singleton_with(db, env, effects)),
                quote!(instance.is_singleton_with(db, &effects)),
                quote!(effects.is_singleton_with(db, effects)),
                quote!(instance.is_singleton_with(effects, effects)),
                quote!(let callback = || instance.is_singleton_with(db, effects);),
                quote!(const { instance.is_singleton_with(db, effects) }),
                quote!(let value: [(); { instance.is_singleton_with(db, effects); 1 }];),
                quote!(
                    fn deferred() {
                        instance.is_singleton_with(db, effects);
                    }
                ),
                quote!(let r#effects = other; instance.is_singleton_with(db, r#effects)),
                quote!(effects.enum_singleton(db, class)),
                quote!(effects.regular(db, env, self).await),
                quote!(effects.union_two(db, env, self, self)),
            ],
        ),
    ] {
        for body in bodies {
            assert!(
                expand_promotion(TokenStream::new(), &promotion(name, &body)).is_err(),
                "accepted {name}: {body}",
            );
        }
    }
}

#[test]
fn promotion_manifest_keeps_receiver_and_bounds_exact() -> Result<()> {
    let original = promotion("promote_public_with", &quote!());
    assert!(expand_promotion(quote!(option), &original).is_err());
    assert!(expand(TokenStream::new(), &original).is_err());
    assert!(expand_mro(TokenStream::new(), &original).is_err());
    assert!(expand_promotion(TokenStream::new(), &own_member(&quote!())).is_err());
    for receiver in [quote!(&self), quote!(mut self), quote!(this: Self)] {
        let mut modified = syn::parse2::<syn::ItemFn>(original.clone())?;
        modified.sig.inputs[0] = syn::parse2(receiver)?;
        assert!(expand_promotion(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for name in ["promote_impl_with", "is_singleton_with", "anything"] {
        assert!(expand_promotion(TokenStream::new(), &promotion(name, &quote!())).is_err());
    }
    Ok(())
}

const EFFECT_METHODS: [&str; 9] = [
    "checkpoint",
    "dataclass_fields",
    "named_tuple_field",
    "named_tuple_property",
    "dunder_paramspec",
    "constructor_context",
    "slot_descriptor",
    "synthesized_member",
    "nonmember_value",
];

const SOURCE_EFFECT_METHODS: [&str; 9] = [
    "code_generator",
    "raw_member",
    "slot_exists",
    "generated_slots",
    "explicit_slots",
    "implicit_member",
    "is_kw_only",
    "is_enum_member",
    "is_enum_class",
];

fn own_member(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn own_class_member_with<'a, 'db, E: OwnMemberEffects<'db>>(
            request: OwnMemberLookupRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Member<'db>, E::Error> {
            #body
        }
    }
}

fn assert_expansion(body: &TokenStream, synchronous_body: &TokenStream) -> Result<()> {
    let original = own_member(body);
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn own_class_member_sync<'a, 'db, E: crate::types::class::own_member::SynchronousOwnMemberEffects<'db>>(
            request: OwnMemberLookupRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Member<'db>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn declared_effect_methods() -> Result<()> {
    for method in EFFECT_METHODS {
        let method = Ident::new(method, Span::call_site());
        assert_expansion(
            &quote!(effects.#method(request).await),
            &quote!(effects.#method(request)),
        )?;
    }
    Ok(())
}

#[test]
fn source_dependencies_are_awaited_effects() -> Result<()> {
    for method in SOURCE_EFFECT_METHODS {
        let method = Ident::new(method, Span::call_site());
        assert_expansion(
            &quote!(effects.#method(request).await),
            &quote!(effects.#method(request)),
        )?;
    }
    Ok(())
}

#[test]
fn source_and_other_effects_require_await() {
    for method in EFFECT_METHODS.into_iter().chain(SOURCE_EFFECT_METHODS) {
        let method = Ident::new(method, Span::call_site());
        let input = own_member(&quote!(effects.#method(request)));
        let error =
            expand(TokenStream::new(), &input).expect_err("unawaited effect method was accepted");
        assert!(
            error.to_string().contains("must be awaited directly"),
            "{error}"
        );
    }
}

#[test]
fn preserves_branches_patterns_attributes_and_error_propagation() -> Result<()> {
    assert_expansion(
        &quote! {
            let OwnMemberLookupRequest { class, name, .. } = request;
            #[allow(unused)]
            effects.checkpoint().await?;
            let mut member = effects.raw_member(request).await?;
            if name == "__slots__"
                && {
                    effects.checkpoint().await?;
                    effects.generated_slots(class).await?
                }
                && !effects.explicit_slots(class).await?
            {
                return effects.implicit_member(request).await;
            }
            if let Some(value) = member.raw_type()
                && let Some(inner) = effects.nonmember_value(value).await?
                && effects.is_enum_class(class).await?
            {
                member = Member::declared(inner);
            }
            Ok(member)
        },
        &quote! {
            let OwnMemberLookupRequest { class, name, .. } = request;
            #[allow(unused)]
            effects.checkpoint()?;
            let mut member = effects.raw_member(request)?;
            if name == "__slots__"
                && {
                    effects.checkpoint()?;
                    effects.generated_slots(class)?
                }
                && !effects.explicit_slots(class)?
            {
                return effects.implicit_member(request);
            }
            if let Some(value) = member.raw_type()
                && let Some(inner) = effects.nonmember_value(value)?
                && effects.is_enum_class(class)?
            {
                member = Member::declared(inner);
            }
            Ok(member)
        },
    )
}

#[test]
fn nested_effect_calls_keep_argument_order() -> Result<()> {
    assert_expansion(
        &quote! {
            effects.constructor_context(
                before(),
                effects.dunder_paramspec(effects.raw_member(request).await?).await?,
                after(),
            ).await
        },
        &quote! {
            effects.constructor_context(
                before(),
                effects.dunder_paramspec(effects.raw_member(request)?)?,
                after(),
            )
        },
    )
}

#[test]
fn fact_arguments_preserve_awaits_order_and_attributes() -> Result<()> {
    assert_expansion(
        &quote! {
            #[allow(unused)]
            effects.raw_member(build_request(
                before(),
                effects.named_tuple_field(request).await?,
                after(),
            )).await?
        },
        &quote! {
            #[allow(unused)]
            effects.raw_member(build_request(
                before(),
                effects.named_tuple_field(request)?,
                after(),
            ))?
        },
    )
}

#[test]
fn rejects_helper_from_another_manifest() {
    let error = expand_mro(
        TokenStream::new(),
        &mro_member(&quote!(
            adjust_own_member_with(request, member, effects).await
        )),
    )
    .expect_err("a helper from another manifest was accepted");
    assert!(
        error.to_string().contains("declared MRO member effect"),
        "{error}"
    );
}

#[test]
fn checked_matches_and_pure_closures_are_preserved() -> Result<()> {
    let body = quote! {
        let callback = |value| value + 1;
        let comparison = matches!(index != 0, true);
        let alternatives = matches!(value, | Some(_) | None,);
        let guarded = matches!(value, Some(inner) if !(inner == 0),);
        effects.raw_member(request).await
    };
    let synchronous_body = quote! {
        let callback = |value| value + 1;
        let comparison = matches!(index != 0, true);
        let alternatives = matches!(value, | Some(_) | None,);
        let guarded = matches!(value, Some(inner) if !(inner == 0),);
        effects.raw_member(request)
    };
    assert_expansion(&body, &synchronous_body)
}

#[test]
fn rejects_unrecognized_or_deferred_effects() {
    let cases = [
        (
            quote!(effects.unknown().await),
            "declared own-member effect",
        ),
        (
            quote!(other.raw_member().await),
            "declared own-member effect",
        ),
        (
            quote!((effects).raw_member().await),
            "declared own-member effect",
        ),
        (
            quote!(holder.effects.raw_member().await),
            "declared own-member effect",
        ),
        (
            quote!(effects.map_type().await),
            "declared own-member effect",
        ),
        (
            quote!(self.map_types_with().await),
            "declared own-member effect",
        ),
        (quote!(unknown().await), "declared own-member effect"),
        (quote!(map(request).await), "declared own-member effect"),
        (
            quote!(let future = effects.dunder_paramspec(ty);),
            "must be awaited directly",
        ),
        (
            quote!(let alias = effects;),
            "effects may only be the receiver",
        ),
        (
            quote!(let alias = &effects;),
            "effects may only be the receiver",
        ),
        (
            quote!(OwnMemberFacts::raw_member(effects, request)),
            "effects may only be the receiver",
        ),
        (
            quote!(E::raw_member(effects, request)),
            "effects may only be the receiver",
        ),
        (
            quote!(<E as OwnMemberFacts<'db>>::raw_member(effects, request)),
            "effects may only be the receiver",
        ),
        (quote!(consume(effects)), "effects may only be the receiver"),
        (
            quote!(let callback = || effects.checkpoint().await;),
            "only supported directly",
        ),
        (
            quote!(const { effects.checkpoint().await };),
            "only supported directly",
        ),
        (
            quote!(let value: [(); { effects.checkpoint().await; 1 }];),
            "only supported directly",
        ),
        (
            quote!(helper::<
                {
                    effects.checkpoint().await;
                    1
                },
            >()),
            "only supported directly",
        ),
        (
            quote!(let future = async { request };),
            "async blocks are not supported",
        ),
        (
            quote!(let callback = async |value| value;),
            "async closures are not supported",
        ),
        (
            quote!(
                fn deferred() {
                    effects.checkpoint().await;
                }
            ),
            "nested items are not supported",
        ),
        (
            quote!(
                async fn deferred() {}
            ),
            "nested items are not supported",
        ),
        (
            quote!(
                use other as effects;
            ),
            "nested items are not supported",
        ),
        (
            quote!(effects.raw_member(hidden!()).await),
            "only checked matches!",
        ),
        (
            quote!(hidden!(effects.checkpoint().await);),
            "only checked matches!",
        ),
        (
            quote!(matches!(effects.checkpoint().await, _)),
            "only supported directly",
        ),
        (
            quote!(matches!(request, _ if effects.checkpoint().await)),
            "only supported directly",
        ),
        (
            quote!(matches!(hidden!(), _)),
            "matches! cannot contain nested macros",
        ),
        (
            quote!(matches!(request, hidden!())),
            "matches! cannot contain nested macros",
        ),
        (
            quote!(matches!(request, _ if hidden!())),
            "matches! cannot contain nested macros",
        ),
        (
            quote!(matches!(async {}, _)),
            "async blocks are not supported",
        ),
    ];
    for (body, message) in cases {
        let error = expand(TokenStream::new(), &own_member(&body))
            .expect_err("unsupported own-member body was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn rejects_effects_shadowing() {
    for body in [
        quote!(let effects = other;),
        quote!(let r#effects = other;),
        quote!(let (effects, _) = other;),
        quote!(let callback = |effects| value;),
        quote!(match value {
            Some(effects) => (),
            _ => (),
        }),
        quote!(for effects in values {}),
        quote!(matches!(value, Some(effects) if condition)),
    ] {
        let error = expand(TokenStream::new(), &own_member(&body))
            .expect_err("shadowed effects was accepted");
        assert!(
            error
                .to_string()
                .contains("body bindings cannot shadow effects"),
            "{error}"
        );
    }
}

#[test]
fn rejects_other_function_or_trait_contracts() {
    let mut cases = vec![
        (quote!(option), own_member(&quote!()), "takes no arguments"),
        (
            quote!(),
            quote!(
                async fn another_member<E: OwnMemberEffects<'db>>() {}
            ),
            "not in the dual_own_member manifest",
        ),
        (
            quote!(),
            quote! {
                async fn adjust_own_member_with<'a, 'db, E: OwnMemberEffects<'db>>(
                    request: OwnMemberLookupRequest<'a, 'db>, member: Member<'db>, effects: &E,
                ) -> Result<Member<'db>, E::Error> {}
            },
            "not in the dual_own_member manifest",
        ),
        (
            quote!(),
            quote!(
                fn own_class_member_with<E: OwnMemberEffects<'db>>() {}
            ),
            "requires an async function",
        ),
    ];
    for parameters in [
        quote!('a, 'db, E: MappingEffects<'db>),
        quote!('a, 'db, E: OwnMemberFacts<'db>),
        quote!('a, 'db, E: other::OwnMemberEffects<'db>),
        quote!('a, 'db, E: OwnMemberEffects<'a>),
        quote!('a, 'db, E: OwnMemberEffects<'db> + Send),
        quote!('a, 'db, E: OwnMemberEffects<'db>, F: AsyncFnMut()),
    ] {
        cases.push((
            quote!(),
            quote! {
                async fn own_class_member_with<#parameters>(
                    request: OwnMemberLookupRequest<'a, 'db>, effects: &E,
                ) -> Result<Member<'db>, E::Error> {}
            },
            "requires the declared OwnMemberEffects signature",
        ));
    }
    for parameters in [
        quote!(request: OwnMemberLookupRequest<'a, 'db>, effects: &mut E),
        quote!(request: OwnMemberLookupRequest<'a, 'db>, other: &E),
        quote!(request: OtherRequest<'a, 'db>, effects: &E),
        quote!(request: OwnMemberLookupRequest<'a, 'db>, effects: &E, db: &dyn Db),
        quote!(self, request: OwnMemberLookupRequest<'a, 'db>, effects: &E),
    ] {
        cases.push((
            quote!(),
            quote! {
                async fn own_class_member_with<'a, 'db, E: OwnMemberEffects<'db>>(#parameters)
                    -> Result<Member<'db>, E::Error> {}
            },
            "requires the declared OwnMemberEffects signature",
        ));
    }
    for output in [
        quote!(Member<'db>),
        quote!(Result<Member<'db>, OtherError>),
        quote!(Result<Type<'db>, E::Error>),
    ] {
        cases.push((
            quote!(),
            quote! {
                async fn own_class_member_with<'a, 'db, E: OwnMemberEffects<'db>>(
                    request: OwnMemberLookupRequest<'a, 'db>, effects: &E,
                ) -> #output {}
            },
            "requires the declared OwnMemberEffects signature",
        ));
    }
    for (arguments, input, message) in cases {
        let error = expand(arguments, &input).expect_err("unsupported signature was accepted");
        assert!(error.to_string().contains(message), "{error}");
    }
}

fn mro_member(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn mro_class_member_with<'a, 'db, C, E: MroMemberEffects<'db, C>>(
            request: MroClassMemberRequest<'a, 'db>, mut cursor: C, effects: &E,
        ) -> Result<ClassMemberResult<'db>, E::Error> {
            #body
        }
    }
}

fn finalize_member(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn finalize_class_member_with<'db, E: MemberFinalizationEffects<'db>>(
            result: CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
            #body
        }
    }
}

fn assert_mro_expansion(body: &TokenStream, synchronous_body: &TokenStream) -> Result<()> {
    let original = mro_member(body);
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn mro_class_member_sync<'a, 'db, C, E: crate::types::class::member_lookup::SynchronousMroMemberEffects<'db, C>>(
            request: MroClassMemberRequest<'a, 'db>, mut cursor: C, effects: &E,
        ) -> Result<ClassMemberResult<'db>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_mro(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

fn assert_finalization_expansion(body: &TokenStream, synchronous_body: &TokenStream) -> Result<()> {
    let original = finalize_member(body);
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn finalize_class_member_sync<'db, E: crate::types::class::member_lookup::SynchronousMemberFinalizationEffects<'db>>(
            result: CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_mro(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn mro_manifest_fact_and_effect_methods() -> Result<()> {
    for method in [
        "known_class",
        "implicit_attribute",
        "checkpoint",
        "advance",
        "own_member",
        "push_pending",
        "clear_pending",
        "finish_pending",
        "infer_augmented",
        "union_augmented",
        "fall_back_to",
    ] {
        let method = Ident::new(method, Span::call_site());
        assert_mro_expansion(
            &quote!(effects.#method(value).await?),
            &quote!(effects.#method(value)?),
        )?;
        let error = expand_mro(
            TokenStream::new(),
            &mro_member(&quote!(effects.#method(value))),
        )
        .expect_err("unawaited MRO effect was accepted");
        assert!(
            error.to_string().contains("must be awaited directly"),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn mro_loop_keeps_cursor_borrows_fact_order_and_pending_slices() -> Result<()> {
    assert_mro_expansion(
        &quote! {
            let mut pending = Vec::new();
            loop {
                effects.checkpoint(MroMemberWork::Advance).await?;
                let Some(class) = effects.advance(&mut cursor).await? else { break; };
                effects.checkpoint(MroMemberWork::KnownClass).await?;
                let known = effects.known_class(class).await?;
                if matches!(known, Some(KnownClass::Generic)) { continue; }
                effects.checkpoint(MroMemberWork::OwnMember).await?;
                let member = effects.own_member(class, request.name, context).await?;
                effects.checkpoint(MroMemberWork::ImplicitAttribute).await?;
                if let Some(implicit) = effects.implicit_attribute(class, request.name).await?
                    && implicit.is_defined() && member.is_defined()
                {
                    effects.checkpoint(MroMemberWork::PushAugmented { prefix_len: pending.len() }).await?;
                    effects.push_pending(&mut pending, class, bindings).await?;
                }
                #[allow(unused)]
                let inferred = effects.infer_augmented(MroPendingBindings::new(&pending)).await?;
                return effects.fall_back_to(prior, member).await;
            }
            Ok(result)
        },
        &quote! {
            let mut pending = Vec::new();
            loop {
                effects.checkpoint(MroMemberWork::Advance)?;
                let Some(class) = effects.advance(&mut cursor)? else { break; };
                effects.checkpoint(MroMemberWork::KnownClass)?;
                let known = effects.known_class(class)?;
                if matches!(known, Some(KnownClass::Generic)) { continue; }
                effects.checkpoint(MroMemberWork::OwnMember)?;
                let member = effects.own_member(class, request.name, context)?;
                effects.checkpoint(MroMemberWork::ImplicitAttribute)?;
                if let Some(implicit) = effects.implicit_attribute(class, request.name)?
                    && implicit.is_defined() && member.is_defined()
                {
                    effects.checkpoint(MroMemberWork::PushAugmented { prefix_len: pending.len() })?;
                    effects.push_pending(&mut pending, class, bindings)?;
                }
                #[allow(unused)]
                let inferred = effects.infer_augmented(MroPendingBindings::new(&pending))?;
                return effects.fall_back_to(prior, member);
            }
            Ok(result)
        },
    )
}

#[test]
fn mro_pending_mutations_require_effects() {
    for body in [
        quote!(pending_augmented_bindings.push((class, bindings)); Ok(result)),
        quote!(pending_augmented_bindings.clear(); Ok(result)),
        quote!(effects.reserve_pending(&mut pending_augmented_bindings).await?; Ok(result)),
    ] {
        assert!(expand_mro(TokenStream::new(), &mro_member(&body)).is_err());
        assert!(super::expand_instance_mro(TokenStream::new(), &instance_mro(&body)).is_err());
    }
}

#[test]
fn finalization_manifest_and_argument_order() -> Result<()> {
    for method in ["checkpoint", "intersect_dynamic"] {
        let method = Ident::new(method, Span::call_site());
        assert_finalization_expansion(
            &quote!(effects.#method(value).await?),
            &quote!(effects.#method(value)?),
        )?;
        let error = expand_mro(
            TokenStream::new(),
            &finalize_member(&quote!(effects.#method(value))),
        )
        .expect_err("unawaited finalization effect was accepted");
        assert!(
            error.to_string().contains("must be awaited directly"),
            "{error}"
        );
    }
    assert_finalization_expansion(
        &quote! {
            effects.checkpoint(MemberFinalizationWork::Begin).await?;
            let intersection = effects.intersect_dynamic(before(), after()).await?;
            effects.checkpoint(MemberFinalizationWork::Publish).await?;
            Ok(Place::bound(intersection).with_qualifiers(result.qualifiers))
        },
        &quote! {
            effects.checkpoint(MemberFinalizationWork::Begin)?;
            let intersection = effects.intersect_dynamic(before(), after())?;
            effects.checkpoint(MemberFinalizationWork::Publish)?;
            Ok(Place::bound(intersection).with_qualifiers(result.qualifiers))
        },
    )
}

#[test]
fn member_manifests_do_not_accept_each_others_methods() {
    for body in [
        quote!(effects.known_class(class)),
        quote!(effects.advance(&mut cursor).await),
    ] {
        assert!(expand(TokenStream::new(), &own_member(&body)).is_err());
        assert!(expand_mro(TokenStream::new(), &finalize_member(&body)).is_err());
    }
    for body in [
        quote!(effects.raw_member(request)),
        quote!(effects.dunder_paramspec(ty).await),
    ] {
        assert!(expand_mro(TokenStream::new(), &mro_member(&body)).is_err());
        assert!(expand_mro(TokenStream::new(), &finalize_member(&body)).is_err());
    }
    assert!(
        expand_mro(
            TokenStream::new(),
            &mro_member(&quote!(effects.intersect_dynamic(ty, dynamic).await))
        )
        .is_err()
    );
    for method in ["known_class", "implicit_attribute"] {
        let method = Ident::new(method, Span::call_site());
        assert!(
            expand_mro(
                TokenStream::new(),
                &finalize_member(&quote!(effects.#method(class)))
            )
            .is_err()
        );
        assert!(
            expand_mro(
                TokenStream::new(),
                &finalize_member(&quote!(effects.#method(class).await))
            )
            .is_err()
        );
    }
}

#[test]
fn mro_and_finalization_reject_aliases_shadowing_and_deferred_calls() {
    for wrap in [
        mro_member as fn(&TokenStream) -> TokenStream,
        finalize_member,
    ] {
        for (body, message) in [
            (quote!(effects.unknown().await), "await requires a declared"),
            (quote!(effects.unknown()), "require a declared"),
            (
                quote!(other.checkpoint(work).await),
                "await requires a declared",
            ),
            (
                quote!((effects).checkpoint(work).await),
                "await requires a declared",
            ),
            (quote!(unknown().await), "await requires a declared"),
            (
                quote!(let alias = effects;),
                "effects may only be the receiver",
            ),
            (
                quote!(let alias = &effects;),
                "effects may only be the receiver",
            ),
            (
                quote!(E::checkpoint(effects, work)),
                "effects may only be the receiver",
            ),
            (quote!(let effects = other;), "cannot shadow effects"),
            (quote!(let r#effects = other;), "cannot shadow effects"),
            (quote!(let (_, effects) = pair;), "cannot shadow effects"),
            (quote!(let f = |effects| value;), "cannot shadow effects"),
            (
                quote!(let f = || effects.checkpoint(work).await;),
                "only supported directly",
            ),
            (
                quote!(const { effects.checkpoint(work).await }),
                "only supported directly",
            ),
            (
                quote!(let f = async || value;),
                "async closures are not supported",
            ),
            (
                quote!(let f = async { value };),
                "async blocks are not supported",
            ),
            (
                quote!(
                    fn helper() {}
                ),
                "nested items are not supported",
            ),
            (
                quote!(hidden!(effects.checkpoint(work).await)),
                "only checked matches!",
            ),
            (
                quote!(effects.checkpoint(hidden!()).await),
                "only checked matches!",
            ),
            (
                quote!(matches!(effects.checkpoint(work).await, _)),
                "only supported directly",
            ),
        ] {
            let error = expand_mro(TokenStream::new(), &wrap(&body))
                .expect_err("unsupported MRO/finalization syntax was accepted");
            assert!(error.to_string().contains(message), "{error}");
        }
    }
    for body in [
        quote!(let f = || effects.known_class(class);),
        quote!(const { effects.known_class(class) }),
        quote!(effects.known_class(effects)),
        quote!(effects.known_class(hidden!())),
        quote!(effects.known_class::<{
            effects.implicit_attribute(class, name);
            1
        }>(class)),
        quote!(MroMemberFacts::known_class(effects, class)),
    ] {
        assert!(expand_mro(TokenStream::new(), &mro_member(&body)).is_err());
    }
}

#[test]
fn mro_signature_requires_the_exact_cursor_and_generic_contract() {
    for parameters in [
        quote!('a, 'db, C: Clone, E: MroMemberEffects<'db, C>),
        quote!('a, 'db, C: Iterator<Item = ClassBase<'db>>, E: MroMemberEffects<'db, C>),
        quote!('a, 'db, C: Send + Unpin, E: MroMemberEffects<'db, C>),
        quote!('a, 'db, C, E: MroMemberEffects<'db>),
        quote!('a, 'db, C, E: MroMemberEffects<'a, C>),
        quote!('a, 'db, C, E: MroMemberEffects<'db, Other>),
        quote!('a, 'db, C, E: OwnMemberEffects<'db>),
        quote!('a, 'db, C, E: MroMemberFacts<'db>),
        quote!('a, 'db, C, E: MroMemberEffects<'db, C> + Send),
        quote!('a, 'db, C, E: other::MroMemberEffects<'db, C>),
    ] {
        let input = quote! {
            async fn mro_class_member_with<#parameters>(
                request: MroClassMemberRequest<'a, 'db>, mut cursor: C, effects: &E,
            ) -> Result<ClassMemberResult<'db>, E::Error> {}
        };
        let error = expand_mro(TokenStream::new(), &input)
            .expect_err("incompatible MRO generics were accepted");
        assert!(
            error
                .to_string()
                .contains("declared MroMemberEffects signature"),
            "{error}"
        );
    }
    for parameters in [
        quote!(request: MroClassMemberRequest<'a, 'db>, cursor: C, effects: &E),
        quote!(request: MroClassMemberRequest<'a, 'db>, mut cursor: &mut C, effects: &E),
        quote!(request: MroClassMemberRequest<'a, 'db>, mut cursor: Box<C>, effects: &E),
        quote!(request: MroClassMemberRequest<'a, 'db>, mut other: C, effects: &E),
        quote!(request: OtherRequest<'a, 'db>, mut cursor: C, effects: &E),
        quote!(request: MroClassMemberRequest<'a, 'db>, mut cursor: C, effects: &mut E),
        quote!(request: MroClassMemberRequest<'a, 'db>, mut cursor: C, other: &E),
    ] {
        let input = quote! {
            async fn mro_class_member_with<'a, 'db, C, E: MroMemberEffects<'db, C>>(#parameters)
                -> Result<ClassMemberResult<'db>, E::Error> {}
        };
        assert!(expand_mro(TokenStream::new(), &input).is_err());
    }
    let where_clause = quote! {
        async fn mro_class_member_with<'a, 'db, C, E: MroMemberEffects<'db, C>>(
            request: MroClassMemberRequest<'a, 'db>, mut cursor: C, effects: &E,
        ) -> Result<ClassMemberResult<'db>, E::Error> where C: Clone {}
    };
    assert!(expand_mro(TokenStream::new(), &where_clause).is_err());
}

#[test]
fn mro_attribute_and_finalization_signature_stay_closed() {
    assert!(expand_mro(quote!(option), &mro_member(&quote!())).is_err());
    assert!(expand_mro(TokenStream::new(), &own_member(&quote!())).is_err());
    assert!(expand(TokenStream::new(), &mro_member(&quote!())).is_err());
    assert!(expand(TokenStream::new(), &finalize_member(&quote!())).is_err());
    for signature in [
        quote!(fn mro_class_member_with()),
        quote!(async fn other_member()),
        quote!(async fn finalize_class_member_with<'db, E: MroMemberEffects<'db>>(
            result: CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error>),
        quote!(async fn finalize_class_member_with<'db, C, E: MemberFinalizationEffects<'db>>(
            result: CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error>),
        quote!(async fn finalize_class_member_with<'db, E: MemberFinalizationEffects<'db>>(
            result: &CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error>),
        quote!(async fn finalize_class_member_with<'db, E: MemberFinalizationEffects<'db>>(
            result: CompletedMemberLookup<'db>, effects: &E,
        ) -> Result<Member<'db>, E::Error>),
    ] {
        assert!(expand_mro(TokenStream::new(), &quote!(#signature {})).is_err());
    }
}

const MRO_ROOT_SIGNATURES: [(&str, &str, &str, &str); 3] = [
    (
        "apply_optional_class_specialization_with",
        "apply_optional_class_specialization_sync",
        "StaticClassLiteral",
        "ClassType",
    ),
    (
        "mro_first_with",
        "mro_first_sync",
        "ClassLiteral",
        "ClassBase",
    ),
    (
        "mro_tail_request_with",
        "mro_tail_request_sync",
        "ClassLiteral",
        "MroTailRequest",
    ),
];

fn expected_mro_expansion(original: &TokenStream, synchronous: &TokenStream) -> Result<syn::File> {
    let mut synchronous: syn::ItemFn = syn::parse2(synchronous.clone())?;
    if synchronous.sig.ident != "maybe_add_generic_sync" {
        synchronous
            .block
            .stmts
            .insert(0, syn::parse_quote!(let fields = MroFieldReads::new(db);));
        synchronous
            .block
            .stmts
            .insert(1, syn::parse_quote!(let _ = fields;));
    }
    syn::parse2(quote!(#original #synchronous))
}

fn mro_root(name: &str, class: &str, output: &str, body: &TokenStream) -> TokenStream {
    let name = Ident::new(name, Span::call_site());
    let class = Ident::new(class, Span::call_site());
    let output = Ident::new(output, Span::call_site());
    quote! {
        #[inline]
        pub(in crate::types) async fn #name<'db, E: MroRootEffects<'db>>(
            fields: MroFieldReads<'db>,
            class: #class<'db>,
            specialization: Option<Specialization<'db>>,
            effects: &E,
        ) -> Result<#output<'db>, E::Error> {
            #body
        }
    }
}

fn mro_first(body: &TokenStream) -> TokenStream {
    mro_root("mro_first_with", "ClassLiteral", "ClassBase", body)
}

fn mro_iteration(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn mro_next_with<'db, E: MroIterationEffects<'db>>(
            fields: MroFieldReads<'db>,
            cursor: &mut MroCursor<'db>,
            direction: MroDirection,
            effects: &E,
        ) -> Result<Option<ClassBase<'db>>, E::Error> {
            #body
        }
    }
}

#[test]
fn mro_iteration_lowers_only_declared_dependencies() -> Result<()> {
    let original = mro_iteration(&quote! {
        effects.iteration_checkpoint(MroIterationWork::Advance).await?;
        if direction == MroDirection::Forward {
            return Ok(Some(mro_first_with(fields, cursor.class, cursor.specialization, effects).await?));
        }
        let request = mro_tail_request_with(fields, cursor.class, cursor.specialization, effects).await?;
        effects.full_mro(request).await?;
        Ok(None)
    });
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn mro_next_sync<'db, E: crate::types::mro::iteration::SynchronousMroIterationEffects<'db>>(
            db: &'db dyn Db,
            cursor: &mut MroCursor<'db>,
            direction: MroDirection,
            effects: &E,
        ) -> Result<Option<ClassBase<'db>>, E::Error> {
            effects.iteration_checkpoint(MroIterationWork::Advance)?;
            if direction == MroDirection::Forward {
                return Ok(Some(mro_first_sync(db, cursor.class, cursor.specialization, effects)?));
            }
            let request = mro_tail_request_sync(db, cursor.class, cursor.specialization, effects)?;
            effects.full_mro(request)?;
            Ok(None)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_mro_iteration(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

#[test]
fn mro_iteration_rejects_deferred_and_undeclared_dependencies() {
    for body in [
        quote!(effects.full_mro(request)),
        quote!(effects.iteration_checkpoint(Advance)),
        quote!(effects.default_class_specialization(context, None).await),
        quote!(mro_first_with(fields, class, specialization, effects)),
        quote!(mro_tail_request_with(
            fields,
            class,
            specialization,
            effects
        )),
        quote!(mro_first_with(fields, class, effects).await),
        quote!(mro_tail_request_with(fields, class, specialization, (effects)).await),
        quote!(let alias = effects; mro_first_with(fields, class, specialization, alias).await),
        quote!(other(db, effects).await),
        quote!(async { effects.full_mro(request).await }),
        quote!(let deferred = || effects.full_mro(request);),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, effects).await
        ),
    ] {
        assert!(
            expand_mro_iteration(TokenStream::new(), &mro_iteration(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn mro_iteration_requires_its_exact_manifest() -> Result<()> {
    let original = mro_iteration(&quote!());
    assert!(expand_mro_iteration(quote!(option), &original).is_err());
    assert!(expand_mro_root(TokenStream::new(), &original).is_err());
    assert!(expand_mro_iteration(TokenStream::new(), &mro_first(&quote!())).is_err());
    for parameter in [
        quote!(cursor: &MroCursor<'db>),
        quote!(cursor: MroCursor<'db>),
        quote!(cursor: &mut MroCursor<'other>),
    ] {
        let mut modified = syn::parse2::<syn::ItemFn>(original.clone())?;
        modified.sig.inputs[1] = syn::parse2(parameter)?;
        assert!(expand_mro_iteration(TokenStream::new(), &quote!(#modified)).is_err());
    }
    let mut modified = syn::parse2::<syn::ItemFn>(original)?;
    modified.sig.ident = Ident::new("mro_other_with", Span::call_site());
    assert!(expand_mro_iteration(TokenStream::new(), &quote!(#modified)).is_err());
    Ok(())
}

fn assert_mro_root_expansion(
    signature: (&str, &str, &str, &str),
    body: &TokenStream,
    synchronous_body: &TokenStream,
) -> Result<()> {
    let (name, synchronous_name, class, output) = signature;
    let original = mro_root(name, class, output, body);
    let synchronous_name = Ident::new(synchronous_name, Span::call_site());
    let class = Ident::new(class, Span::call_site());
    let output = Ident::new(output, Span::call_site());
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn #synchronous_name<'db, E: crate::types::mro::root::SynchronousMroRootEffects<'db>>(
            db: &'db dyn Db,
            class: #class<'db>,
            specialization: Option<Specialization<'db>>,
            effects: &E,
        ) -> Result<#output<'db>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_mro_root(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

fn class_type_member(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn class_type_own_member_with<'a, 'db, E: ClassTypeOwnMemberEffects<'db>>(
            request: ClassTypeOwnMemberRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Member<'db>, E::Error> {
            #body
        }
    }
}

fn assert_class_type_expansion(body: &TokenStream, synchronous_body: &TokenStream) -> Result<()> {
    let original = class_type_member(body);
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn class_type_own_member_sync<'a, 'db, E: crate::types::class::own_member::SynchronousClassTypeOwnMemberEffects<'db>>(
            request: ClassTypeOwnMemberRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Member<'db>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_class_type(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn mro_root_manifest_requires_awaited_source_and_constructor_requests() -> Result<()> {
    for signature in MRO_ROOT_SIGNATURES {
        let (name, _, class, output) = signature;
        for method in [
            "checkpoint",
            "generic_context",
            "generic_alias",
            "default_class_specialization",
            "tuple_runtime_specialization",
        ] {
            let method = Ident::new(method, Span::call_site());
            assert_mro_root_expansion(
                signature,
                &quote!(effects.#method(value).await?),
                &quote!(effects.#method(value)?),
            )?;
            assert!(
                expand_mro_root(
                    TokenStream::new(),
                    &mro_root(name, class, output, &quote!(effects.#method(value)))
                )
                .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn mro_root_bodies_keep_dispatch_order_and_optional_specialization() -> Result<()> {
    assert_mro_root_expansion(
        MRO_ROOT_SIGNATURES[0],
        &quote! {
            let Some(specialization) = specialization else {
                effects.checkpoint(MroRootWork::DefaultSpecialization).await?;
                return effects.default_class_specialization(class).await;
            };
            effects.checkpoint(MroRootWork::Context).await?;
            let Some(_) = effects.generic_context(class).await? else {
                return Ok(ClassType::NonGeneric(class.into()));
            };
            effects.checkpoint(MroRootWork::GenericAlias).await?;
            effects.generic_alias(class, specialization).await
        },
        &quote! {
            let Some(specialization) = specialization else {
                effects.checkpoint(MroRootWork::DefaultSpecialization)?;
                return effects.default_class_specialization(class);
            };
            effects.checkpoint(MroRootWork::Context)?;
            let Some(_) = effects.generic_context(class)? else {
                return Ok(ClassType::NonGeneric(class.into()));
            };
            effects.checkpoint(MroRootWork::GenericAlias)?;
            effects.generic_alias(class, specialization)
        },
    )?;
    assert_mro_root_expansion(
        MRO_ROOT_SIGNATURES[1],
        &quote! {
            match class {
                ClassLiteral::Static(class) => {
                    let class = apply_optional_class_specialization_with(fields, class, specialization, effects).await?;
                    Ok(ClassBase::Class(class))
                }
                other => Ok(ClassBase::Class(ClassType::NonGeneric(other))),
            }
        },
        &quote! {
            match class {
                ClassLiteral::Static(class) => {
                    let class = apply_optional_class_specialization_sync(db, class, specialization, effects)?;
                    Ok(ClassBase::Class(class))
                }
                other => Ok(ClassBase::Class(ClassType::NonGeneric(other))),
            }
        },
    )?;
    assert_mro_root_expansion(
        MRO_ROOT_SIGNATURES[2],
        &quote! {
            match class {
                ClassLiteral::Static(class) => {
                    let specialization = if let Some(specialization) = specialization {
                        effects.checkpoint(MroRootWork::TupleRuntimeSpecialization).await?;
                        Some(effects.tuple_runtime_specialization(specialization).await?)
                    } else { None };
                    Ok(MroTailRequest::Static(class, specialization))
                }
                ClassLiteral::Dynamic(class) => Ok(MroTailRequest::Dynamic(class)),
            }
        },
        &quote! {
            match class {
                ClassLiteral::Static(class) => {
                    let specialization = if let Some(specialization) = specialization {
                        effects.checkpoint(MroRootWork::TupleRuntimeSpecialization)?;
                        Some(effects.tuple_runtime_specialization(specialization)?)
                    } else { None };
                    Ok(MroTailRequest::Static(class, specialization))
                }
                ClassLiteral::Dynamic(class) => Ok(MroTailRequest::Dynamic(class)),
            }
        },
    )
}

#[test]
fn mro_free_helper_preserves_generic_arguments_and_nested_argument_order() -> Result<()> {
    assert_mro_root_expansion(
        MRO_ROOT_SIGNATURES[1],
        &quote! {
            #[allow(unused)]
            before();
            let class = apply_optional_class_specialization_with::<'db, E, { 1 + 2 }>(
                fields,
                class_from_default(effects.default_class_specialization(class).await?),
                specialization,
                effects,
            ).await?;
            after(class)
        },
        &quote! {
            #[allow(unused)]
            before();
            let class = apply_optional_class_specialization_sync::<'db, E, { 1 + 2 }>(
                db,
                class_from_default(effects.default_class_specialization(class)?),
                specialization,
                effects,
            )?;
            after(class)
        },
    )
}

#[test]
fn mro_free_helper_rejects_unapproved_calls_and_escapes() {
    for body in [
        quote!(apply_optional_class_specialization_with(
            fields,
            class,
            specialization,
            effects
        )),
        quote!(
            crate::types::mro::apply_optional_class_specialization_with(
                fields,
                class,
                specialization,
                effects
            )
            .await
        ),
        quote!(
            ::apply_optional_class_specialization_with(fields, class, specialization, effects)
                .await
        ),
        quote!(
            r#apply_optional_class_specialization_with(fields, class, specialization, effects)
                .await
        ),
        quote!(
            (apply_optional_class_specialization_with)(db, class, specialization, effects).await
        ),
        quote!(let helper = apply_optional_class_specialization_with;),
        quote!(let helper = crate::types::mro::apply_optional_class_specialization_with;),
        quote!(let helper = &apply_optional_class_specialization_with;),
        quote!(return apply_optional_class_specialization_with;),
        quote!(consume(apply_optional_class_specialization_with)),
        quote!(let helper = apply_optional_class_specialization_with; helper(db, class, specialization, effects).await),
        quote!(
            other
                .apply_optional_class_specialization_with(fields, class, specialization, effects)
                .await
        ),
        quote!(apply_optional_class_specialization_with(fields, class, effects).await),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, effects, extra)
                .await
        ),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, (effects))
                .await
        ),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, &effects).await
        ),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, alias).await
        ),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, r#effects)
                .await
        ),
        quote!(let helper = || apply_optional_class_specialization_with(fields, class, specialization, effects).await;),
        quote!(
            const {
                apply_optional_class_specialization_with(fields, class, specialization, effects)
                    .await
            }
        ),
        quote!(async {
            apply_optional_class_specialization_with(fields, class, specialization, effects).await
        }),
        quote!(matches!(
            apply_optional_class_specialization_with(fields, class, specialization, effects).await,
            _
        )),
        quote!(unknown(db, class, specialization, effects).await),
        quote!(unknown().await),
        quote!(other.checkpoint(work).await),
    ] {
        assert!(
            expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn mro_free_helper_recursively_validates_arguments() {
    for position in 0..3 {
        for escaped in [
            quote!(effects),
            quote!(&effects),
            quote!(consume(effects)),
            quote!({
                let alias = effects;
                value
            }),
            quote!(effects.default_class_specialization(context, known)),
            quote!(|| effects.generic_context(class)),
            quote!(hidden!()),
            quote!(apply_optional_class_specialization_with),
            quote!(other::apply_optional_class_specialization_with),
            quote!({
                let helper = apply_optional_class_specialization_with;
                value
            }),
            quote!(consume(other::apply_optional_class_specialization_with)),
            quote!(unknown().await),
        ] {
            let mut arguments = [quote!(db), quote!(class), quote!(specialization)];
            arguments[position] = escaped;
            let body =
                quote!(apply_optional_class_specialization_with(#(#arguments,)* effects).await);
            assert!(
                expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err(),
                "accepted argument {position}: {body}",
            );
        }
    }
    for arguments in [
        quote!(<{ effects; 1 }>),
        quote!(<{ effects.generic_context(class); 1 }>),
        quote!(<{ effects.checkpoint(work).await; 1 }>),
        quote!(<{ apply_optional_class_specialization_with; 1 }>),
        quote!(<{ apply_optional_class_specialization_with(fields, class, specialization, effects).await; 1 }>),
        quote!(<[(); { effects.generic_context(class); 1 }]>),
        quote!(<{ hidden!() }>),
    ] {
        let body = quote!(apply_optional_class_specialization_with::#arguments(db, class, specialization, effects).await);
        assert!(
            expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err(),
            "accepted generic arguments: {body}",
        );
    }
}

#[test]
fn mro_free_helper_and_effects_cannot_be_shadowed() {
    for name in [
        "apply_optional_class_specialization_with",
        "r#apply_optional_class_specialization_with",
        "effects",
        "r#effects",
    ] {
        let name = syn::parse_str::<Ident>(name).expect("valid identifier");
        for body in [
            quote!(let #name = other;),
            quote!(let (_, #name) = pair;),
            quote!(let callback = |#name| value;),
            quote!(if let Some(#name) = value {}),
            quote!(match value { Some(#name) => (), _ => () }),
            quote!(for #name in values {}),
            quote!(matches!(value, Some(#name))),
        ] {
            assert!(
                expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err(),
                "accepted shadow binding: {body}",
            );
        }
    }
}

const CLASS_TYPE_EFFECT_METHODS: [&str; 15] = [
    "alias_origin",
    "alias_specialization",
    "is_tuple",
    "specialization_tuple",
    "checkpoint",
    "dynamic_member",
    "named_tuple_member",
    "typed_dict_member",
    "enum_member",
    "tuple_len",
    "tuple_getitem",
    "tuple_new",
    "tuple_runtime_specialization",
    "static_own_member",
    "owner_specialize",
];

#[test]
fn class_type_own_member_manifest_effect_methods() -> Result<()> {
    for method in CLASS_TYPE_EFFECT_METHODS {
        let method = Ident::new(method, Span::call_site());
        assert_class_type_expansion(
            &quote!(effects.#method(value).await?),
            &quote!(effects.#method(value)?),
        )?;
        assert!(
            expand_class_type(
                TokenStream::new(),
                &class_type_member(&quote!(effects.#method(value)))
            )
            .is_err()
        );
    }
    assert_class_type_expansion(
        &quote! {
            effects.checkpoint(ClassTypeOwnMemberWork::Begin).await?;
            let specialization = effects.tuple_runtime_specialization(specialization).await?;
            let mut member = effects.static_own_member(OwnMemberLookupRequest {
                class, specialization: Some(specialization), ..request
            }).await?;
            if let Some(place) = member.place.as_defined_mut() {
                place.ty = effects.owner_specialize(place.ty, specialization).await?;
            }
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            Ok(member)
        },
        &quote! {
            effects.checkpoint(ClassTypeOwnMemberWork::Begin)?;
            let specialization = effects.tuple_runtime_specialization(specialization)?;
            let mut member = effects.static_own_member(OwnMemberLookupRequest {
                class, specialization: Some(specialization), ..request
            })?;
            if let Some(place) = member.place.as_defined_mut() {
                place.ty = effects.owner_specialize(place.ty, specialization)?;
            }
            effects.checkpoint(ClassTypeOwnMemberWork::Publish)?;
            Ok(member)
        },
    )
}

#[test]
fn new_member_manifests_reject_unlisted_methods() {
    for body in [
        quote!(effects.raw_member(request)),
        quote!(effects.known_class(class)),
        quote!(effects.advance(cursor).await),
        quote!(effects.intersect_dynamic(ty, dynamic).await),
        quote!(effects.regular(db, env, ty).await),
        quote!(effects.unknown().await),
        quote!(effects.unknown()),
    ] {
        assert!(expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err());
        assert!(expand_class_type(TokenStream::new(), &class_type_member(&body)).is_err());
    }
    for method in CLASS_TYPE_EFFECT_METHODS {
        if matches!(method, "checkpoint" | "tuple_runtime_specialization") {
            continue;
        }
        let method = Ident::new(method, Span::call_site());
        let body = quote!(effects.#method(value).await);
        assert!(expand_mro_root(TokenStream::new(), &mro_first(&body)).is_err());
    }
    for body in [
        quote!(effects.generic_context(class)),
        quote!(effects.generic_context(class).await),
        quote!(effects.default_class_specialization(context, known).await),
    ] {
        assert!(expand_class_type(TokenStream::new(), &class_type_member(&body)).is_err());
    }
}

#[test]
fn class_type_member_rejects_escaped_effects_and_deferred_calls() {
    for body in [
        quote!(let alias = effects;),
        quote!(let alias = &effects;),
        quote!(let effects = other;),
        quote!(let r#effects = other;),
        quote!(let (_, effects) = pair;),
        quote!(let callback = |effects| value;),
        quote!(generic.origin(db)),
        quote!(generic.specialization(db)),
        quote!(class_literal.is_tuple(db)),
        quote!(specialization.tuple(db)),
        quote!(let callback = || effects.static_own_member(request).await;),
        quote!(const { effects.static_own_member(request).await }),
        quote!(async { effects.static_own_member(request).await }),
        quote!(let callback = async || value;),
        quote!(
            fn helper() {}
        ),
        quote!(matches!(effects.static_own_member(request).await, _)),
        quote!(hidden!(effects.static_own_member(request).await)),
        quote!(effects.static_own_member(hidden!()).await),
        quote!(effects.static_own_member(effects).await),
        quote!(
            effects
                .static_own_member::<{
                    effects;
                    1
                }>(request)
                .await
        ),
        quote!(E::static_own_member(effects, request)),
        quote!((effects).checkpoint(work).await),
        quote!(other.checkpoint(work).await),
        quote!(unknown().await),
    ] {
        assert!(
            expand_class_type(TokenStream::new(), &class_type_member(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn free_helper_is_only_accepted_by_mro_first() {
    let body = quote!(
        apply_optional_class_specialization_with(fields, class, specialization, effects).await
    );
    for (name, _, class, output) in [MRO_ROOT_SIGNATURES[0], MRO_ROOT_SIGNATURES[2]] {
        assert!(
            expand_mro_root(TokenStream::new(), &mro_root(name, class, output, &body)).is_err()
        );
    }
    assert!(expand_class_type(TokenStream::new(), &class_type_member(&body)).is_err());
    assert!(expand(TokenStream::new(), &own_member(&body)).is_err());
    assert!(expand_mro(TokenStream::new(), &mro_member(&body)).is_err());
    assert!(expand_mro(TokenStream::new(), &finalize_member(&body)).is_err());
    assert!(
        expand_promotion(TokenStream::new(), &promotion("promote_public_with", &body)).is_err()
    );
}

#[test]
fn mro_root_signatures_remain_exact() -> Result<()> {
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        let original = syn::parse2::<syn::ItemFn>(mro_root(name, class, output, &quote!()))?;
        for generics in [
            quote!(<'db, E: OwnMemberEffects<'db>>),
            quote!(<'db, E: MroRootFacts<'db>>),
            quote!(<'db, E: other::MroRootEffects<'db>>),
            quote!(<'db, E: MroRootEffects<'db> + Send>),
            quote!(<'a, 'db, E: MroRootEffects<'db>>),
            quote!(<'db, E: MroRootEffects<'a>>),
            quote!(<'db, E: MroRootEffects<'db>, F>),
            quote!(<'db, E: MroRootEffects<'db> = Provider>),
        ] {
            let mut modified = original.clone();
            modified.sig.generics = syn::parse2(generics)?;
            assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
        }
        for (position, replacement) in [
            (0, quote!(db: &dyn Db)),
            (0, quote!(db: &'db mut dyn Db)),
            (0, quote!(db: &'db dyn OtherDb)),
            (0, quote!(other: &'db dyn Db)),
            (1, quote!(class: OtherClass<'db>)),
            (1, quote!(mut class: ClassLiteral<'db>)),
            (2, quote!(specialization: Specialization<'db>)),
            (2, quote!(specialization: Option<Specialization<'a>>)),
            (3, quote!(effects: &mut E)),
            (3, quote!(effects: &'db E)),
            (3, quote!(other: &E)),
        ] {
            let mut modified = original.clone();
            modified.sig.inputs[position] = syn::parse2(replacement)?;
            assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
        }
        for output in [
            quote!(-> ClassBase<'db>),
            quote!(-> Result<OtherOutput<'db>, E::Error>),
            quote!(-> Result<ClassBase<'db>, OtherError>),
        ] {
            let mut modified = original.clone();
            modified.sig.output = syn::parse2(output)?;
            assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
        }
        let mut modified = original.clone();
        modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
        assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.push(syn::parse_quote!(extra: usize));
        assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original;
        modified.sig.inputs.pop();
        assert!(expand_mro_root(TokenStream::new(), &quote!(#modified)).is_err());
    }
    Ok(())
}

#[test]
fn class_type_member_signature_remains_exact() -> Result<()> {
    let original = syn::parse2::<syn::ItemFn>(class_type_member(&quote!()))?;
    for generics in [
        quote!(<'a, 'db, E: OwnMemberEffects<'db>>),
        quote!(<'a, 'db, E: ClassTypeOwnMemberFacts<'db>>),
        quote!(<'a, 'db, E: other::ClassTypeOwnMemberEffects<'db>>),
        quote!(<'a, 'db, E: ClassTypeOwnMemberEffects<'a>>),
        quote!(<'a, 'db, E: ClassTypeOwnMemberEffects<'db> + Send>),
        quote!(<'db, 'a, E: ClassTypeOwnMemberEffects<'db>>),
        quote!(<'a, 'db, E: ClassTypeOwnMemberEffects<'db>, F>),
    ] {
        let mut modified = original.clone();
        modified.sig.generics = syn::parse2(generics)?;
        assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for (position, replacement) in [
        (0, quote!(db: &dyn Db)),
        (0, quote!(db: &'db mut dyn Db)),
        (0, quote!(db: &'db dyn OtherDb)),
        (0, quote!(other: &'db dyn Db)),
        (0, quote!(request: OwnMemberLookupRequest<'a, 'db>)),
        (0, quote!(request: ClassTypeOwnMemberRequest<'db, 'a>)),
        (0, quote!(request: &ClassTypeOwnMemberRequest<'a, 'db>)),
        (0, quote!(mut request: ClassTypeOwnMemberRequest<'a, 'db>)),
        (1, quote!(effects: &mut E)),
        (1, quote!(effects: &'db E)),
        (1, quote!(other: &E)),
    ] {
        let mut modified = original.clone();
        modified.sig.inputs[position] = syn::parse2(replacement)?;
        assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for output in [
        quote!(-> Member<'db>),
        quote!(-> Result<Type<'db>, E::Error>),
        quote!(-> Result<Member<'db>, OtherError>),
    ] {
        let mut modified = original.clone();
        modified.sig.output = syn::parse2(output)?;
        assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    }
    let mut modified = original.clone();
    modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
    assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.inputs.push(syn::parse_quote!(extra: usize));
    assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original;
    modified.sig.inputs.pop();
    assert!(expand_class_type(TokenStream::new(), &quote!(#modified)).is_err());
    Ok(())
}

#[test]
fn new_attributes_do_not_admit_other_manifests() -> Result<()> {
    for original in [
        own_member(&quote!()),
        mro_member(&quote!()),
        finalize_member(&quote!()),
        promotion("promote_public_with", &quote!()),
        promotion("promote_singletons_impl_with", &quote!()),
    ] {
        assert!(expand_mro_root(TokenStream::new(), &original).is_err());
        assert!(expand_class_type(TokenStream::new(), &original).is_err());
    }
    let class_type = class_type_member(&quote!());
    assert!(expand_mro_root(TokenStream::new(), &class_type).is_err());
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        let original = mro_root(name, class, output, &quote!());
        assert!(expand_class_type(TokenStream::new(), &original).is_err());
    }
    for (expand_new, original) in [
        (
            expand_mro_root as fn(TokenStream, &TokenStream) -> Result<TokenStream>,
            mro_first(&quote!()),
        ),
        (expand_class_type, class_type),
    ] {
        assert!(expand_new(quote!(option), &original).is_err());
        assert!(expand(TokenStream::new(), &original).is_err());
        assert!(expand_mro(TokenStream::new(), &original).is_err());
        assert!(expand_promotion(TokenStream::new(), &original).is_err());
        let original = syn::parse2::<syn::ItemFn>(original)?;
        let mut modified = original.clone();
        modified.sig.asyncness = None;
        assert!(expand_new(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.ident = Ident::new("unknown_with", Span::call_site());
        assert!(expand_new(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.safety = syn::parse_quote!(unsafe);
        assert!(expand_new(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.abi = Some(syn::parse_quote!(extern "C"));
        assert!(expand_new(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original;
        modified.sig.constness = Some(syn::token::Const::default());
        assert!(expand_new(TokenStream::new(), &quote!(#modified)).is_err());
    }
    Ok(())
}

fn synthesized_member(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn own_synthesized_member_with<'a, 'db, E: SynthesizedMemberEffects<'db>>(
            request: OwnMemberLookupRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Option<Type<'db>>, E::Error> {
            #body
        }
    }
}

fn assert_synthesized_expansion(body: &TokenStream, synchronous_body: &TokenStream) -> Result<()> {
    let original = synthesized_member(body);
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn own_synthesized_member_sync<'a, 'db, E: crate::types::class::synthesized_member::SynchronousSynthesizedMemberEffects<'db>>(
            request: OwnMemberLookupRequest<'a, 'db>,
            effects: &E,
        ) -> Result<Option<Type<'db>>, E::Error> {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_synthesized(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn synthesized_member_preserves_producer_priority_and_publication() -> Result<()> {
    assert_synthesized_expansion(
        &quote! {
            effects.checkpoint(SynthesizedMemberWork::Admission { name_bytes: request.name.len() }).await?;
            if effects.total_ordering(request.class).await?
                && matches!(request.name, "__lt__" | "__le__" | "__gt__" | "__ge__")
            {
                effects.checkpoint(SynthesizedMemberWork::OrderingRequest).await?;
                if let Some(member) = effects.total_ordering_member(request).await? {
                    effects.checkpoint(SynthesizedMemberWork::Publish).await?;
                    return Ok(Some(member));
                }
            }
            if let Some(method) = FrozenDataclassMethod::from_name(request.name) {
                effects.checkpoint(SynthesizedMemberWork::FrozenRequest).await?;
                if let Some(member) = effects.frozen_subclass_member(request, method).await? {
                    effects.checkpoint(SynthesizedMemberWork::Publish).await?;
                    return Ok(Some(member));
                }
            }
            effects.checkpoint(SynthesizedMemberWork::CodeGenerator).await?;
            let Some(field_policy) = effects.code_generator(request.class).await? else {
                effects.checkpoint(SynthesizedMemberWork::Publish).await?;
                return Ok(None);
            };
            effects.checkpoint(SynthesizedMemberWork::GeneratedRequest).await?;
            let member = effects.generated_member(request, field_policy).await?;
            effects.checkpoint(SynthesizedMemberWork::Publish).await?;
            Ok(member)
        },
        &quote! {
            effects.checkpoint(SynthesizedMemberWork::Admission { name_bytes: request.name.len() })?;
            if effects.total_ordering(request.class)?
                && matches!(request.name, "__lt__" | "__le__" | "__gt__" | "__ge__")
            {
                effects.checkpoint(SynthesizedMemberWork::OrderingRequest)?;
                if let Some(member) = effects.total_ordering_member(request)? {
                    effects.checkpoint(SynthesizedMemberWork::Publish)?;
                    return Ok(Some(member));
                }
            }
            if let Some(method) = FrozenDataclassMethod::from_name(request.name) {
                effects.checkpoint(SynthesizedMemberWork::FrozenRequest)?;
                if let Some(member) = effects.frozen_subclass_member(request, method)? {
                    effects.checkpoint(SynthesizedMemberWork::Publish)?;
                    return Ok(Some(member));
                }
            }
            effects.checkpoint(SynthesizedMemberWork::CodeGenerator)?;
            let Some(field_policy) = effects.code_generator(request.class)? else {
                effects.checkpoint(SynthesizedMemberWork::Publish)?;
                return Ok(None);
            };
            effects.checkpoint(SynthesizedMemberWork::GeneratedRequest)?;
            let member = effects.generated_member(request, field_policy)?;
            effects.checkpoint(SynthesizedMemberWork::Publish)?;
            Ok(member)
        },
    )
}

#[test]
fn synthesized_member_manifest_effect_methods() -> Result<()> {
    for method in [
        "total_ordering",
        "code_generator",
        "checkpoint",
        "total_ordering_member",
        "frozen_subclass_member",
        "generated_member",
    ] {
        let method = Ident::new(method, Span::call_site());
        assert_synthesized_expansion(
            &quote!(effects.#method(value).await?),
            &quote!(effects.#method(value)?),
        )?;
        let body = quote!(effects.#method(value));
        assert!(
            expand_synthesized(TokenStream::new(), &synthesized_member(&body)).is_err(),
            "accepted unawaited effect: {body}",
        );
    }
    assert_synthesized_expansion(
        &quote! {
            #[allow(unused)]
            effects.generated_member(before(), effects.code_generator(after()).await?).await?
        },
        &quote! {
            #[allow(unused)]
            effects.generated_member(before(), effects.code_generator(after())?)?
        },
    )
}

#[test]
fn synthesized_member_rejects_escapes_deferred_calls_and_unknown_operations() {
    for body in [
        quote!(fields.total_ordering(request.class)),
        quote!(request.class.total_ordering(db)),
        quote!(let alias = effects;),
        quote!(let alias = &effects;),
        quote!(consume(effects)),
        quote!(return effects;),
        quote!(let effects = other;),
        quote!(let r#effects = other;),
        quote!(let (_, effects) = pair;),
        quote!(let callback = |effects| value;),
        quote!(matches!(value, Some(effects))),
        quote!(let callback = || effects.code_generator(class);),
        quote!(let callback = || effects.generated_member(request, policy).await;),
        quote!(const { effects.code_generator(class) }),
        quote!(const { effects.generated_member(request, policy).await }),
        quote!(async { effects.generated_member(request, policy).await }),
        quote!(let callback = async || value;),
        quote!(
            fn helper() {}
        ),
        quote!(let value: [(); { effects.code_generator(class); 1 }];),
        quote!(effects.code_generator(effects)),
        quote!(effects.generated_member(request, effects).await),
        quote!(effects.code_generator::<{
            effects.checkpoint(work).await;
            1
        }>(class)),
        quote!(
            effects
                .generated_member::<{
                    effects.code_generator(class);
                    1
                }>(request, policy)
                .await
        ),
        quote!(
            effects
                .generated_member(effects.total_ordering_member(request), policy)
                .await
        ),
        quote!(E::code_generator(effects, class)),
        quote!(E::generated_member(effects, request, policy).await),
        quote!((effects).checkpoint(work).await),
        quote!(other.checkpoint(work).await),
        quote!(unknown().await),
        quote!(effects.unknown()),
        quote!(effects.unknown().await),
        quote!(effects.raw_member(request)),
        quote!(effects.generic_context(class)),
        quote!(effects.synthesized_member(request).await),
        quote!(effects.static_own_member(request).await),
        quote!(effects.dataclass_fields(class).await),
        quote!(effects.advance(cursor).await),
        quote!(effects.regular(db, env, ty).await),
        quote!(hidden!(effects.code_generator(class))),
        quote!(effects.code_generator(hidden!())),
        quote!(effects.generated_member(hidden!(), policy).await),
        quote!(matches!(effects.generated_member(request, policy).await, _)),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, effects).await
        ),
        quote!(ty.promote_singletons_impl_with(db, env, effects).await),
        quote!(instance.is_singleton_with(db, effects)),
    ] {
        assert!(
            expand_synthesized(TokenStream::new(), &synthesized_member(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn synthesized_member_signature_remains_exact() -> Result<()> {
    let original = syn::parse2::<syn::ItemFn>(synthesized_member(&quote!()))?;
    for generics in [
        quote!(<'a, 'db, E: OwnMemberEffects<'db>>),
        quote!(<'a, 'db, E: SynthesizedMemberFacts<'db>>),
        quote!(<'a, 'db, E: other::SynthesizedMemberEffects<'db>>),
        quote!(<'a, 'db, E: SynthesizedMemberEffects<'a>>),
        quote!(<'a, 'db, E: SynthesizedMemberEffects<'db> + Send>),
        quote!(<'db, 'a, E: SynthesizedMemberEffects<'db>>),
        quote!(<'a, 'db, E: SynthesizedMemberEffects<'db>, F>),
        quote!(<'a, 'db, E: SynthesizedMemberEffects<'db> = Provider>),
    ] {
        let mut modified = original.clone();
        modified.sig.generics = syn::parse2(generics)?;
        assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for (position, replacement) in [
        (0, quote!(db: &dyn Db)),
        (0, quote!(db: &'db mut dyn Db)),
        (0, quote!(db: &'db dyn OtherDb)),
        (0, quote!(other: &'db dyn Db)),
        (0, quote!(request: OtherRequest<'a, 'db>)),
        (0, quote!(request: OwnMemberLookupRequest<'db, 'a>)),
        (0, quote!(request: &OwnMemberLookupRequest<'a, 'db>)),
        (0, quote!(mut request: OwnMemberLookupRequest<'a, 'db>)),
        (1, quote!(effects: &mut E)),
        (1, quote!(effects: &'db E)),
        (1, quote!(other: &E)),
    ] {
        let mut modified = original.clone();
        modified.sig.inputs[position] = syn::parse2(replacement)?;
        assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for output in [
        quote!(-> Option<Type<'db>>),
        quote!(-> Result<Type<'db>, E::Error>),
        quote!(-> Result<Option<Member<'db>>, E::Error>),
        quote!(-> Result<Option<Type<'a>>, E::Error>),
        quote!(-> Result<Option<Type<'db>>, OtherError>),
    ] {
        let mut modified = original.clone();
        modified.sig.output = syn::parse2(output)?;
        assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    }
    let mut modified = original.clone();
    modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.inputs.push(syn::parse_quote!(extra: usize));
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.inputs.pop();
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.asyncness = None;
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.safety = syn::parse_quote!(unsafe);
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.abi = Some(syn::parse_quote!(extern "C"));
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original;
    modified.sig.constness = Some(syn::token::Const::default());
    assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    Ok(())
}

#[test]
fn synthesized_member_attribute_has_one_manifest() -> Result<()> {
    let original = synthesized_member(&quote!());
    assert!(expand_synthesized(quote!(option), &original).is_err());
    for other in [
        own_member(&quote!()),
        class_type_member(&quote!()),
        mro_member(&quote!()),
        finalize_member(&quote!()),
        promotion("promote_public_with", &quote!()),
        promotion("promote_singletons_impl_with", &quote!()),
    ] {
        assert!(expand_synthesized(TokenStream::new(), &other).is_err());
    }
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        assert!(
            expand_synthesized(
                TokenStream::new(),
                &mro_root(name, class, output, &quote!())
            )
            .is_err()
        );
    }
    for expand_other in [
        expand,
        expand_class_type,
        expand_mro,
        expand_mro_root,
        expand_promotion,
    ] {
        assert!(expand_other(TokenStream::new(), &original).is_err());
    }
    for name in [
        "another_synthesized_member_with",
        "own_total_ordering_member_with",
        "own_frozen_dataclass_subclass_method",
        "own_generated_member_with",
    ] {
        let mut modified = syn::parse2::<syn::ItemFn>(original.clone())?;
        modified.sig.ident = Ident::new(name, Span::call_site());
        assert!(expand_synthesized(TokenStream::new(), &quote!(#modified)).is_err());
    }
    Ok(())
}

const STATIC_MRO_SIGNATURES: [(&str, &str, &str, &str); 3] = [
    (
        "static_mro_with",
        "static_mro_sync",
        "db: &'db dyn Db, class_literal: StaticClassLiteral<'db>, specialization: Option<Specialization<'db>>, effects: &E",
        "Result<Result<Mro<'db>, StaticMroError<'db>>, E::Error>",
    ),
    (
        "maybe_add_generic_with",
        "maybe_add_generic_sync",
        "resolved_bases: &mut Vec<ClassBase<'db>>, original_bases: &[Type<'db>], remaining_bases: &[Type<'db>], effects: &E",
        "Result<(), E::Error>",
    ),
    (
        "base_has_cyclic_mro_with",
        "base_has_cyclic_mro_sync",
        "db: &'db dyn Db, base: ClassBase<'db>, effects: &E",
        "Result<bool, E::Error>",
    ),
];

fn static_mro_function(index: usize, body: &TokenStream) -> Result<TokenStream> {
    let (name, _, inputs, output) = STATIC_MRO_SIGNATURES[index];
    let name = Ident::new(name, Span::call_site());
    let inputs = syn::parse_str::<TokenStream>(
        &inputs.replace("db: &'db dyn Db", "fields: MroFieldReads<'db>"),
    )?;
    let output = syn::parse_str::<syn::Type>(output)?;
    Ok(quote! {
        #[inline]
        pub(in crate::types) async fn #name<'db, E: StaticMroEffects<'db>>(
            #inputs
        ) -> #output {
            #body
        }
    })
}

fn assert_static_mro_expansion(
    index: usize,
    body: &TokenStream,
    synchronous_body: &TokenStream,
) -> Result<()> {
    let original = static_mro_function(index, body)?;
    let (_, synchronous_name, inputs, output) = STATIC_MRO_SIGNATURES[index];
    let synchronous_name = Ident::new(synchronous_name, Span::call_site());
    let inputs = syn::parse_str::<TokenStream>(inputs)?;
    let output = syn::parse_str::<syn::Type>(output)?;
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn #synchronous_name<'db, E: crate::types::mro::construction::SynchronousStaticMroEffects<'db>>(
            #inputs
        ) -> #output {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_static_mro(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

#[test]
fn static_mro_manifests_require_awaited_source_requests() -> Result<()> {
    for index in 0..STATIC_MRO_SIGNATURES.len() {
        assert_static_mro_expansion(index, &quote!(), &quote!())?;
        for method in [
            "explicit_bases",
            "has_pep_695_type_params",
            "converted_explicit_base",
            "object_base",
            "checkpoint",
            "root_class",
            "collect_single_base_mro",
            "collect_base_mro",
            "specialize_base",
            "c3_merge",
            "make_error",
            "failed_c3",
            "static_mro_is_cycle",
        ] {
            let allowed = method == "checkpoint"
                || (index == 0 && method != "static_mro_is_cycle")
                || (index == 2 && method == "static_mro_is_cycle");
            let method = Ident::new(method, Span::call_site());
            let body = quote!(effects.#method(value).await?);
            if allowed {
                assert_static_mro_expansion(index, &body, &quote!(effects.#method(value)?))?;
            } else {
                assert!(
                    expand_static_mro(TokenStream::new(), &static_mro_function(index, &body)?)
                        .is_err()
                );
            }
            assert!(
                expand_static_mro(
                    TokenStream::new(),
                    &static_mro_function(index, &quote!(effects.#method(value)))?
                )
                .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn static_mro_preserves_guards_helpers_collection_and_c3_order() -> Result<()> {
    assert_static_mro_expansion(
        0,
        &quote! {
            effects.checkpoint(StaticMroWork::Begin).await?;
            let class = effects.root_class(class_literal, specialization).await?;
            let original_bases = effects.explicit_bases(class_literal).await?;
            match original_bases.as_ref() {
                [] if { effects.checkpoint(StaticMroWork::KnownClass).await?; class.is_object() } => {
                    return Ok(Ok(fixed(class)));
                }
                [base] if { effects.checkpoint(StaticMroWork::Pep695Classification).await?;
                    !effects.has_pep_695_type_params(class_literal).await? }
                    && !matches!(base, Type::GenericAlias(_)) => {
                    let base = effects.converted_explicit_base(class_literal, 0).await?;
                    if base_has_cyclic_mro_with(fields, base, effects).await? {
                        return Ok(Err(effects.make_error(class_literal, cycle()).await?));
                    }
                    return Ok(Ok(effects.collect_single_base_mro(class, base, specialization).await?));
                }
                _ => {}
            }
            maybe_add_generic_with(&mut resolved_bases, original_bases, remaining_bases, effects).await?;
            for base in resolved_bases.iter().copied() {
                if base_has_cyclic_mro_with(fields, base, effects).await? {
                    return Ok(Err(effects.make_error(class_literal, cycle()).await?));
                }
                sequences.push(effects.collect_base_mro(base, specialization).await?);
            }
            for base in resolved_bases {
                direct.push(effects.specialize_base(base, specialization).await?);
            }
            sequences.push(direct);
            let mro = match effects.c3_merge(sequences).await? {
                Some(mro) => Ok(mro),
                None => effects.failed_c3(class_literal, original_bases).await?,
            };
            effects.checkpoint(StaticMroWork::Publish).await?;
            Ok(mro)
        },
        &quote! {
            effects.checkpoint(StaticMroWork::Begin)?;
            let class = effects.root_class(class_literal, specialization)?;
            let original_bases = effects.explicit_bases(class_literal)?;
            match original_bases.as_ref() {
                [] if { effects.checkpoint(StaticMroWork::KnownClass)?; class.is_object() } => {
                    return Ok(Ok(fixed(class)));
                }
                [base] if { effects.checkpoint(StaticMroWork::Pep695Classification)?;
                    !effects.has_pep_695_type_params(class_literal)? }
                    && !matches!(base, Type::GenericAlias(_)) => {
                    let base = effects.converted_explicit_base(class_literal, 0)?;
                    if base_has_cyclic_mro_sync(db, base, effects)? {
                        return Ok(Err(effects.make_error(class_literal, cycle())?));
                    }
                    return Ok(Ok(effects.collect_single_base_mro(class, base, specialization)?));
                }
                _ => {}
            }
            maybe_add_generic_sync(&mut resolved_bases, original_bases, remaining_bases, effects)?;
            for base in resolved_bases.iter().copied() {
                if base_has_cyclic_mro_sync(db, base, effects)? {
                    return Ok(Err(effects.make_error(class_literal, cycle())?));
                }
                sequences.push(effects.collect_base_mro(base, specialization)?);
            }
            for base in resolved_bases {
                direct.push(effects.specialize_base(base, specialization)?);
            }
            sequences.push(direct);
            let mro = match effects.c3_merge(sequences)? {
                Some(mro) => Ok(mro),
                None => effects.failed_c3(class_literal, original_bases)?,
            };
            effects.checkpoint(StaticMroWork::Publish)?;
            Ok(mro)
        },
    )
}

#[test]
fn static_mro_helpers_preserve_scan_short_circuit_and_cycle_dispatch() -> Result<()> {
    assert_static_mro_expansion(
        1,
        &quote! {
            effects.checkpoint(StaticMroWork::GenericProtocolScan { len: original_bases.len() }).await?;
            if original_bases.iter().any(|base| matches!(base, Type::KnownInstance(KnownInstanceType::Protocol))) {
                return Ok(());
            }
            effects.checkpoint(StaticMroWork::GenericAliasScan { len: remaining_bases.len() }).await?;
            if remaining_bases.iter().any(|base| matches!(base, Type::GenericAlias(_))) {
                return Ok(());
            }
            effects.checkpoint(StaticMroWork::ResolvedBaseAppend { prefix_len: resolved_bases.len(), capacity: resolved_bases.capacity() }).await?;
            resolved_bases.push(ClassBase::Generic);
            Ok(())
        },
        &quote! {
            effects.checkpoint(StaticMroWork::GenericProtocolScan { len: original_bases.len() })?;
            if original_bases.iter().any(|base| matches!(base, Type::KnownInstance(KnownInstanceType::Protocol))) {
                return Ok(());
            }
            effects.checkpoint(StaticMroWork::GenericAliasScan { len: remaining_bases.len() })?;
            if remaining_bases.iter().any(|base| matches!(base, Type::GenericAlias(_))) {
                return Ok(());
            }
            effects.checkpoint(StaticMroWork::ResolvedBaseAppend { prefix_len: resolved_bases.len(), capacity: resolved_bases.capacity() })?;
            resolved_bases.push(ClassBase::Generic);
            Ok(())
        },
    )?;
    assert_static_mro_expansion(
        2,
        &quote! {
            effects.checkpoint(StaticMroWork::BaseCycleDispatch).await?;
            match base {
                ClassBase::Class(class) => {
                    let Some(literal) = class.class_literal(db).static_class_literal() else {
                        return Ok(false);
                    };
                    effects.checkpoint(StaticMroWork::StaticCycleRequest).await?;
                    effects.static_mro_is_cycle(literal, class.specialization()).await
                }
                _ => Ok(false),
            }
        },
        &quote! {
            effects.checkpoint(StaticMroWork::BaseCycleDispatch)?;
            match base {
                ClassBase::Class(class) => {
                    let Some(literal) = class.class_literal(db).static_class_literal() else {
                        return Ok(false);
                    };
                    effects.checkpoint(StaticMroWork::StaticCycleRequest)?;
                    effects.static_mro_is_cycle(literal, class.specialization())
                }
                _ => Ok(false),
            }
        },
    )
}

#[test]
fn static_mro_free_helpers_lower_arguments_recursively() -> Result<()> {
    assert_static_mro_expansion(
        0,
        &quote! {
            #[allow(unused)]
            maybe_add_generic_with::<E>(
                &mut resolved_bases,
                effects.explicit_bases(class_literal).await?,
                after(effects.root_class(class_literal, specialization).await?),
                effects,
            ).await?;
            base_has_cyclic_mro_with::<E>(
                fields,
                effects.specialize_base(effects.object_base(env).await?, specialization).await?,
                effects,
            ).await
        },
        &quote! {
            #[allow(unused)]
            maybe_add_generic_sync::<E>(
                &mut resolved_bases,
                effects.explicit_bases(class_literal)?,
                after(effects.root_class(class_literal, specialization)?),
                effects,
            )?;
            base_has_cyclic_mro_sync::<E>(
                db,
                effects.specialize_base(effects.object_base(env)?, specialization)?,
                effects,
            )
        },
    )
}

#[test]
fn static_mro_rejects_escapes_deferred_calls_and_unlisted_operations() -> Result<()> {
    for (index, signature) in STATIC_MRO_SIGNATURES.iter().enumerate() {
        for body in [
            quote!(let alias = effects;),
            quote!(consume(&effects)),
            quote!(return effects;),
            quote!(let effects = other;),
            quote!(let r#effects = other;),
            quote!(let (_, effects) = pair;),
            quote!(let callback = |effects| value;),
            quote!(matches!(value, Some(effects))),
            quote!(effects.checkpoint(effects).await),
            quote!(effects.checkpoint(consume(effects)).await),
            quote!(let callback = || effects.checkpoint(work).await;),
            quote!(const { effects.checkpoint(work).await }),
            quote!(async { effects.checkpoint(work).await }),
            quote!(let callback = async || value;),
            quote!(
                fn helper() {}
            ),
            quote!(let value: [(); { effects.checkpoint(work).await; 1 }];),
            quote!(
                effects
                    .checkpoint::<{
                        effects.checkpoint(work).await;
                        1
                    }>(work)
                    .await
            ),
            quote!(effects.checkpoint(effects.checkpoint(work)).await),
            quote!(E::checkpoint(effects, work).await),
            quote!((effects).checkpoint(work).await),
            quote!(other.checkpoint(work).await),
            quote!(unknown().await),
            quote!(effects.unknown()),
            quote!(effects.unknown().await),
            quote!(effects.generic_context(class)),
            quote!(effects.code_generator(class)),
            quote!(effects.raw_member(request)),
            quote!(effects.default_class_specialization(class).await),
            quote!(effects.static_own_member(request).await),
            quote!(effects.generated_member(request).await),
            quote!(effects.advance(cursor).await),
            quote!(effects.regular(db, env, ty).await),
            quote!(hidden!(effects.checkpoint(work).await)),
            quote!(effects.checkpoint(hidden!()).await),
            quote!(matches!(effects.checkpoint(work).await, _)),
            quote!(
                apply_optional_class_specialization_with(fields, class, specialization, effects)
                    .await
            ),
            quote!(ty.promote_singletons_impl_with(db, env, effects).await),
            quote!(instance.is_singleton_with(db, effects)),
        ] {
            assert!(
                expand_static_mro(TokenStream::new(), &static_mro_function(index, &body)?).is_err(),
                "accepted body in {}: {body}",
                signature.0,
            );
        }
    }
    for body in [
        quote!(let callback = || effects.explicit_bases(class);),
        quote!(const { effects.object_base(env) }),
        quote!(
            effects
                .root_class::<{
                    effects.has_pep_695_type_params(class)?;
                    1
                }>(class, spec)
                .await
        ),
        quote!(let value: [(); { effects.converted_explicit_base(class, 0)?; 1 }];),
        quote!(effects.explicit_bases(effects)),
        quote!(effects.object_base(hidden!())),
        quote!(E::explicit_bases(effects, class)),
    ] {
        assert!(
            expand_static_mro(TokenStream::new(), &static_mro_function(0, &body)?).is_err(),
            "accepted {body}",
        );
    }
    Ok(())
}

#[test]
fn static_mro_free_helpers_require_exact_direct_calls() -> Result<()> {
    for (helper, arguments) in [
        (
            "maybe_add_generic_with",
            quote!(&mut resolved_bases, original_bases, remaining_bases),
        ),
        ("base_has_cyclic_mro_with", quote!(db, base)),
    ] {
        let raw_helper = Ident::new_raw(helper, Span::call_site());
        let helper = Ident::new(helper, Span::call_site());
        for body in [
            quote!(#helper(#arguments, effects)),
            quote!(let helper = #helper;),
            quote!(consume(#helper)),
            quote!(let #helper = other; #helper(#arguments, effects).await),
            quote!(let #raw_helper = other;),
            quote!(let (_, #helper) = pair;),
            quote!(let callback = |#helper| value;),
            quote!(matches!(value, Some(#helper))),
            quote!(#helper(#arguments).await),
            quote!(#helper(#arguments, extra, effects).await),
            quote!(#helper(#arguments, effects, extra).await),
            quote!(#helper(#arguments, &effects).await),
            quote!(#helper(#arguments, (effects)).await),
            quote!(#helper(#arguments, r#effects).await),
            quote!(#helper(#arguments, alias).await),
            quote!(other::#helper(#arguments, effects).await),
            quote!(::#helper(#arguments, effects).await),
            quote!(<#helper as Trait>::call(#arguments, effects).await),
            quote!(#raw_helper(#arguments, effects).await),
            quote!((#helper)(#arguments, effects).await),
            quote!(value.#helper(#arguments, effects).await),
            quote!(value.#helper(#arguments, effects)),
            quote!(#helper::<{ effects.checkpoint(work).await; 1 }>(#arguments, effects).await),
            quote!(#helper::<{ effects.object_base(env)?; 1 }>(#arguments, effects).await),
            quote!(let callback = || #helper(#arguments, effects).await;),
            quote!(async { #helper(#arguments, effects).await }),
            quote!(const { #helper(#arguments, effects).await }),
            quote!(let value: [(); { #helper(#arguments, effects).await; 1 }];),
            quote!(matches!(#helper(#arguments, effects).await, _)),
        ] {
            assert!(
                expand_static_mro(TokenStream::new(), &static_mro_function(0, &body)?).is_err(),
                "accepted {body}",
            );
        }
        let call = quote!(#helper(#arguments, effects).await);
        for index in [1, 2] {
            assert!(
                expand_static_mro(TokenStream::new(), &static_mro_function(index, &call)?).is_err()
            );
        }
        for (name, _, class, output) in MRO_ROOT_SIGNATURES {
            assert!(
                expand_mro_root(TokenStream::new(), &mro_root(name, class, output, &call)).is_err()
            );
        }
        for (expand_other, original) in [
            (
                expand as fn(TokenStream, &TokenStream) -> Result<TokenStream>,
                own_member(&call),
            ),
            (expand_class_type, class_type_member(&call)),
            (expand_mro, mro_member(&call)),
            (expand_mro, finalize_member(&call)),
            (expand_synthesized, synthesized_member(&call)),
            (expand_promotion, promotion("promote_public_with", &call)),
            (
                expand_promotion,
                promotion("promote_singletons_impl_with", &call),
            ),
        ] {
            assert!(expand_other(TokenStream::new(), &original).is_err());
        }
    }
    for body in [
        quote!(
            maybe_add_generic_with(&mut resolved_bases, effects, remaining_bases, effects).await
        ),
        quote!(
            maybe_add_generic_with(&mut resolved_bases, hidden!(), remaining_bases, effects).await
        ),
        quote!(
            maybe_add_generic_with(
                &mut resolved_bases,
                original_bases,
                || effects.explicit_bases(class),
                effects
            )
            .await
        ),
        quote!(
            maybe_add_generic_with(
                &mut resolved_bases,
                original_bases,
                effects.root_class(class, spec),
                effects
            )
            .await
        ),
        quote!(base_has_cyclic_mro_with(effects, base, effects).await),
        quote!(base_has_cyclic_mro_with(fields, hidden!(), effects).await),
        quote!(base_has_cyclic_mro_with(fields, || effects.object_base(env), effects).await),
        quote!(base_has_cyclic_mro_with(fields, unknown().await, effects).await),
        quote!(base_has_cyclic_mro_with(fields, effects.root_class(class, spec), effects).await),
    ] {
        assert!(
            expand_static_mro(TokenStream::new(), &static_mro_function(0, &body)?).is_err(),
            "accepted {body}",
        );
    }
    Ok(())
}

#[test]
fn static_mro_signatures_remain_exact() -> Result<()> {
    for index in 0..STATIC_MRO_SIGNATURES.len() {
        let original = syn::parse2::<syn::ItemFn>(static_mro_function(index, &quote!())?)?;
        for generics in [
            quote!(<'db, E: MroRootEffects<'db>>),
            quote!(<'db, E: StaticMroFacts<'db>>),
            quote!(<'db, E: other::StaticMroEffects<'db>>),
            quote!(<'db, E: StaticMroEffects<'static>>),
            quote!(<'db, E: StaticMroEffects<'db> + Send>),
            quote!(<'a, 'db, E: StaticMroEffects<'db>>),
            quote!(<'db, E: StaticMroEffects<'db>, F>),
            quote!(<'db, E: StaticMroEffects<'db> = Provider>),
        ] {
            let mut modified = original.clone();
            modified.sig.generics = syn::parse2(generics)?;
            assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
        let effects_index = original.sig.inputs.len() - 1;
        for argument in [
            quote!(effects: &mut E),
            quote!(effects: &'db E),
            quote!(other: &E),
        ] {
            let mut modified = original.clone();
            modified.sig.inputs[effects_index] = syn::parse2(argument)?;
            assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
        let arguments = match index {
            0 => vec![
                (0, quote!(db: &dyn Db)),
                (0, quote!(db: &'db mut dyn Db)),
                (1, quote!(class: StaticClassLiteral<'db>)),
                (1, quote!(class_literal: ClassLiteral<'db>)),
                (2, quote!(specialization: Specialization<'db>)),
                (2, quote!(specialization: Option<Specialization<'static>>)),
            ],
            1 => vec![
                (0, quote!(resolved_bases: &Vec<ClassBase<'db>>)),
                (0, quote!(resolved_bases: &mut [ClassBase<'db>])),
                (0, quote!(mut resolved_bases: &mut Vec<ClassBase<'db>>)),
                (1, quote!(original_bases: &Vec<Type<'db>>)),
                (1, quote!(original_bases: &[ClassBase<'db>])),
                (1, quote!(remaining_bases: &[Type<'db>])),
                (2, quote!(remaining_bases: &mut [Type<'db>])),
                (2, quote!(remaining_bases: &'db [Type<'db>])),
            ],
            _ => vec![
                (0, quote!(db: &dyn Db)),
                (0, quote!(db: &'db dyn OtherDb)),
                (1, quote!(base: &ClassBase<'db>)),
                (1, quote!(base: ClassType<'db>)),
                (1, quote!(mut base: ClassBase<'db>)),
            ],
        };
        for (position, argument) in arguments {
            let mut modified = original.clone();
            modified.sig.inputs[position] = syn::parse2(argument)?;
            assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
        for output in [
            quote!(-> Result<Type<'db>, E::Error>),
            quote!(-> Result<Result<Mro<'db>, StaticMroError<'db>>, OtherError>),
            quote!(-> Result<Mro<'db>, E::Error>),
            quote!(-> Result<Result<Mro<'db>, E::Error>, StaticMroError<'db>>),
            quote!(-> bool),
            quote!(-> ()),
        ] {
            let mut modified = original.clone();
            modified.sig.output = syn::parse2(output)?;
            assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
        let mut modified = original.clone();
        modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.push(syn::parse_quote!(extra: usize));
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.pop();
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs[0] = original.sig.inputs[1].clone();
        modified.sig.inputs[1] = original.sig.inputs[0].clone();
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.asyncness = None;
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.safety = syn::parse_quote!(unsafe);
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.abi = Some(syn::parse_quote!(extern "C"));
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original;
        modified.sig.constness = Some(syn::token::Const::default());
        assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
    }
    Ok(())
}

#[test]
fn static_mro_attribute_accepts_only_three_manifests() -> Result<()> {
    for index in 0..STATIC_MRO_SIGNATURES.len() {
        let original = static_mro_function(index, &quote!())?;
        assert!(expand_static_mro(quote!(option), &original).is_err());
        for expand_other in [
            expand,
            expand_class_type,
            expand_mro,
            expand_mro_root,
            expand_promotion,
            expand_synthesized,
        ] {
            assert!(expand_other(TokenStream::new(), &original).is_err());
        }
        for name in [
            "another_static_mro_with",
            "collect_base_mro_with",
            "specialize_base_with",
            "c3_merge_with",
            "make_error_with",
            "failed_c3_with",
        ] {
            let mut modified = syn::parse2::<syn::ItemFn>(original.clone())?;
            modified.sig.ident = Ident::new(name, Span::call_site());
            assert!(expand_static_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
    }
    for other in [
        own_member(&quote!()),
        class_type_member(&quote!()),
        synthesized_member(&quote!()),
        mro_member(&quote!()),
        finalize_member(&quote!()),
        promotion("promote_public_with", &quote!()),
        promotion("promote_singletons_impl_with", &quote!()),
    ] {
        assert!(expand_static_mro(TokenStream::new(), &other).is_err());
    }
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        assert!(
            expand_static_mro(
                TokenStream::new(),
                &mro_root(name, class, output, &quote!())
            )
            .is_err()
        );
    }
    Ok(())
}

const BASE_MRO_SIGNATURES: [(&str, &str, &str, &str); 4] = [
    (
        "class_mro_start_with",
        "class_mro_start_sync",
        "db: &'db dyn Db, class: ClassType<'db>, additional: Option<Specialization<'db>>, effects: &E",
        "Result<ClassMroStart<'db>, E::Error>",
    ),
    (
        "base_mro_start_with",
        "base_mro_start_sync",
        "db: &'db dyn Db, env: &ProgramEnvironment<'db>, base: ClassBase<'db>, additional: Option<Specialization<'db>>, effects: &E",
        "Result<BaseMroStart<'db>, E::Error>",
    ),
    (
        "collect_base_mro_with",
        "collect_base_mro_sync",
        "db: &'db dyn Db, env: &ProgramEnvironment<'db>, base: ClassBase<'db>, additional: Option<Specialization<'db>>, effects: &E",
        "Result<VecDeque<ClassBase<'db>>, E::Error>",
    ),
    (
        "collect_single_base_mro_with",
        "collect_single_base_mro_sync",
        "db: &'db dyn Db, env: &ProgramEnvironment<'db>, root: ClassType<'db>, base: ClassBase<'db>, additional: Option<Specialization<'db>>, effects: &E",
        "Result<Mro<'db>, E::Error>",
    ),
];

fn base_mro_function(index: usize, body: &TokenStream) -> Result<TokenStream> {
    let (name, _, inputs, output) = BASE_MRO_SIGNATURES[index];
    let name = Ident::new(name, Span::call_site());
    let inputs = syn::parse_str::<TokenStream>(
        &inputs.replace("db: &'db dyn Db", "fields: MroFieldReads<'db>"),
    )?;
    let output = syn::parse_str::<syn::Type>(output)?;
    Ok(quote! {
        #[inline]
        pub(in crate::types) async fn #name<'db, E: BaseMroEffects<'db>>(
            #inputs
        ) -> #output {
            #body
        }
    })
}

fn assert_base_mro_expansion(
    index: usize,
    body: &TokenStream,
    synchronous_body: &TokenStream,
) -> Result<()> {
    let original = base_mro_function(index, body)?;
    let (_, synchronous_name, inputs, output) = BASE_MRO_SIGNATURES[index];
    let synchronous_name = Ident::new(synchronous_name, Span::call_site());
    let inputs = syn::parse_str::<TokenStream>(inputs)?;
    let output = syn::parse_str::<syn::Type>(output)?;
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn #synchronous_name<'db, E: crate::types::mro::base::SynchronousBaseMroEffects<'db>>(
            #inputs
        ) -> #output {
            #synchronous_body
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_base_mro(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

#[test]
fn base_mro_manifests_have_separate_fact_and_effect_allowlists() -> Result<()> {
    for index in 0..BASE_MRO_SIGNATURES.len() {
        assert_base_mro_expansion(index, &quote!(), &quote!())?;
        for method in [
            "checkpoint",
            "compose_specialization",
            "collect_start",
            "collect_start_with_root",
            "object_base",
            "root_class",
            "static_mro_is_cycle",
            "map_type",
            "unknown",
        ] {
            let allowed = method == "checkpoint"
                || (index == 1 && method == "object_base")
                || (index == 0 && method == "compose_specialization")
                || (index == 2 && method == "collect_start")
                || (index == 3 && method == "collect_start_with_root");
            let method = Ident::new(method, Span::call_site());
            let body = quote!(effects.#method(value).await?);
            if allowed {
                assert_base_mro_expansion(index, &body, &quote!(effects.#method(value)?))?;
            } else {
                assert!(
                    expand_base_mro(TokenStream::new(), &base_mro_function(index, &body)?).is_err()
                );
            }
            {
                let body = quote!(effects.#method(value));
                assert!(
                    expand_base_mro(TokenStream::new(), &base_mro_function(index, &body)?).is_err()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn base_mro_preserves_specialization_and_collection_order() -> Result<()> {
    assert_base_mro_expansion(
        0,
        &quote! {
            effects.checkpoint(BaseMroWork::ClassDispatch).await?;
            let start = match class {
                ClassType::NonGeneric(class) => ClassMroStart { class, specialization: None },
                ClassType::Generic(alias) => {
                    let specialization = alias.specialization(db);
                    let specialization = if let Some(additional) = additional {
                        effects.checkpoint(BaseMroWork::CompositionRequest).await?;
                        effects.compose_specialization(specialization, additional).await?
                    } else {
                        specialization
                    };
                    ClassMroStart { class: alias.origin(db).into(), specialization: Some(specialization) }
                }
            };
            effects.checkpoint(BaseMroWork::Publish).await?;
            Ok(start)
        },
        &quote! {
            effects.checkpoint(BaseMroWork::ClassDispatch)?;
            let start = match class {
                ClassType::NonGeneric(class) => ClassMroStart { class, specialization: None },
                ClassType::Generic(alias) => {
                    let specialization = alias.specialization(db);
                    let specialization = if let Some(additional) = additional {
                        effects.checkpoint(BaseMroWork::CompositionRequest)?;
                        effects.compose_specialization(specialization, additional)?
                    } else {
                        specialization
                    };
                    ClassMroStart { class: alias.origin(db).into(), specialization: Some(specialization) }
                }
            };
            effects.checkpoint(BaseMroWork::Publish)?;
            Ok(start)
        },
    )?;
    assert_base_mro_expansion(
        1,
        &quote! {
            effects.checkpoint(BaseMroWork::BaseDispatch).await?;
            let start = match base {
                ClassBase::Class(class) => BaseMroStart::Class(class_mro_start_with(fields, class, additional, effects).await?),
                ClassBase::Protocol => {
                    effects.checkpoint(BaseMroWork::ObjectBase).await?;
                    BaseMroStart::Length3([base, ClassBase::Generic, effects.object_base(env).await?])
                }
                _ => {
                    effects.checkpoint(BaseMroWork::ObjectBase).await?;
                    BaseMroStart::Length2([base, effects.object_base(env).await?])
                }
            };
            effects.checkpoint(BaseMroWork::Publish).await?;
            Ok(start)
        },
        &quote! {
            effects.checkpoint(BaseMroWork::BaseDispatch)?;
            let start = match base {
                ClassBase::Class(class) => BaseMroStart::Class(class_mro_start_sync(db, class, additional, effects)?),
                ClassBase::Protocol => {
                    effects.checkpoint(BaseMroWork::ObjectBase)?;
                    BaseMroStart::Length3([base, ClassBase::Generic, effects.object_base(env)?])
                }
                _ => {
                    effects.checkpoint(BaseMroWork::ObjectBase)?;
                    BaseMroStart::Length2([base, effects.object_base(env)?])
                }
            };
            effects.checkpoint(BaseMroWork::Publish)?;
            Ok(start)
        },
    )?;
    assert_base_mro_expansion(
        2,
        &quote! {
            let start = base_mro_start_with(fields, env, base, additional, effects).await?;
            effects.checkpoint(BaseMroWork::CollectionRequest).await?;
            effects.collect_start(start).await
        },
        &quote! {
            let start = base_mro_start_sync(db, env, base, additional, effects)?;
            effects.checkpoint(BaseMroWork::CollectionRequest)?;
            effects.collect_start(start)
        },
    )?;
    assert_base_mro_expansion(
        3,
        &quote! {
            let start = base_mro_start_with::<E>(fields, env, base, additional, effects).await?;
            effects.checkpoint(BaseMroWork::SingleCollectionRequest).await?;
            effects.collect_start_with_root(root, start).await
        },
        &quote! {
            let start = base_mro_start_sync::<E>(db, env, base, additional, effects)?;
            effects.checkpoint(BaseMroWork::SingleCollectionRequest)?;
            effects.collect_start_with_root(root, start)
        },
    )
}

#[test]
fn base_mro_helpers_require_exact_direct_calls() -> Result<()> {
    for (name, arguments, permitted) in [
        (
            "class_mro_start_with",
            quote!(db, class, additional),
            &[1][..],
        ),
        (
            "base_mro_start_with",
            quote!(db, env, base, additional),
            &[2, 3][..],
        ),
    ] {
        let helper = Ident::new(name, Span::call_site());
        let raw_helper = Ident::new_raw(name, Span::call_site());
        for index in 0..BASE_MRO_SIGNATURES.len() {
            if !permitted.contains(&index) {
                let body = quote!(#helper(#arguments, effects).await);
                assert!(
                    expand_base_mro(TokenStream::new(), &base_mro_function(index, &body)?).is_err()
                );
            }
            for body in [
                quote!(#helper(#arguments, effects)),
                quote!(#helper(#arguments).await),
                quote!(#helper(#arguments, extra, effects).await),
                quote!(#helper(#arguments, effects, extra).await),
                quote!(#helper(#arguments, &effects).await),
                quote!(#helper(#arguments, (effects)).await),
                quote!(#helper(#arguments, r#effects).await),
                quote!(#helper(#arguments, alias).await),
                quote!(other::#helper(#arguments, effects).await),
                quote!(::#helper(#arguments, effects).await),
                quote!(#raw_helper(#arguments, effects).await),
                quote!((#helper)(#arguments, effects).await),
                quote!(value.#helper(#arguments, effects).await),
                quote!(value.#helper(#arguments, effects)),
                quote!(let alias = #helper;),
                quote!(let alias = &other::#helper;),
                quote!(let #helper = callback;),
                quote!(let #raw_helper = callback;),
                quote!(#helper::<{ effects.checkpoint(work).await; 1 }>(#arguments, effects).await),
                quote!(let deferred = || #helper(#arguments, effects).await;),
                quote!(async { #helper(#arguments, effects).await }),
                quote!(const { #helper(#arguments, effects).await }),
                quote!(matches!(#helper(#arguments, effects).await, _)),
            ] {
                assert!(
                    expand_base_mro(TokenStream::new(), &base_mro_function(index, &body)?).is_err(),
                    "accepted {body}",
                );
            }
        }
        let body = quote!(#helper(#arguments, effects).await);
        for index in 0..STATIC_MRO_SIGNATURES.len() {
            assert!(
                expand_static_mro(TokenStream::new(), &static_mro_function(index, &body)?).is_err()
            );
        }
        for (name, _, class, output) in MRO_ROOT_SIGNATURES {
            assert!(
                expand_mro_root(TokenStream::new(), &mro_root(name, class, output, &body)).is_err()
            );
        }
    }
    assert_base_mro_expansion(
        1,
        &quote! {
            class_mro_start_with::<E>(
                fields,
                after(effects.object_base(env).await?),
                { effects.checkpoint(BaseMroWork::CompositionRequest).await?; additional },
                effects,
            ).await
        },
        &quote! {
            class_mro_start_sync::<E>(
                db,
                after(effects.object_base(env)?),
                { effects.checkpoint(BaseMroWork::CompositionRequest)?; additional },
                effects,
            )
        },
    )?;
    for body in [
        quote!(class_mro_start_with(effects, class, additional, effects).await),
        quote!(class_mro_start_with(fields, hidden!(), additional, effects).await),
        quote!(
            class_mro_start_with(fields, || effects.object_base(env), additional, effects).await
        ),
        quote!(class_mro_start_with(fields, unknown().await, additional, effects).await),
    ] {
        assert!(expand_base_mro(TokenStream::new(), &base_mro_function(1, &body)?).is_err());
    }
    Ok(())
}

#[test]
fn base_mro_rejects_deferred_operations_and_provider_escapes() -> Result<()> {
    for index in 0..BASE_MRO_SIGNATURES.len() {
        for body in [
            quote!(let alias = effects;),
            quote!(let alias = &effects;),
            quote!(helper(effects)),
            quote!(let effects = other;),
            quote!(let r#effects = other;),
            quote!((&effects).checkpoint(work).await),
            quote!((effects).checkpoint(work).await),
            quote!(r#effects.checkpoint(work).await),
            quote!(other.checkpoint(work).await),
            quote!(BaseMroEffects::checkpoint(effects, work).await),
            quote!(effects.checkpoint(effects).await),
            quote!(let callback = || effects.checkpoint(work).await;),
            quote!(let callback = || effects.object_base(env);),
            quote!(async { effects.checkpoint(work).await }),
            quote!(const { effects.object_base(env) }),
            quote!(let value: [(); { effects.checkpoint(work).await; 1 }];),
            quote!(helper::<
                {
                    effects.object_base(env)?;
                    1
                },
            >()),
            quote!(matches!(effects.checkpoint(work).await, _)),
            quote!(hidden!()),
            quote!(
                fn nested() {}
            ),
            quote!(effects.legacy(operation, || value)),
            quote!(
                apply_optional_class_specialization_with(fields, class, additional, effects).await
            ),
            quote!(base_has_cyclic_mro_with(fields, base, effects).await),
        ] {
            assert!(
                expand_base_mro(TokenStream::new(), &base_mro_function(index, &body)?).is_err(),
                "accepted {body}",
            );
        }
    }
    Ok(())
}

#[test]
fn base_mro_signatures_remain_exact() -> Result<()> {
    for index in 0..BASE_MRO_SIGNATURES.len() {
        let original = syn::parse2::<syn::ItemFn>(base_mro_function(index, &quote!())?)?;
        for generics in [
            quote!(<'db, E: BaseMroFacts<'db>>),
            quote!(<'db, E: StaticMroEffects<'db>>),
            quote!(<'db, E: other::BaseMroEffects<'db>>),
            quote!(<'db, E: BaseMroEffects<'static>>),
            quote!(<'db, E: BaseMroEffects<'db> + Send>),
            quote!(<'a, 'db, E: BaseMroEffects<'db>>),
            quote!(<'db, E: BaseMroEffects<'db>, F>),
            quote!(<'db, E: BaseMroEffects<'db> = Provider>),
        ] {
            let mut modified = original.clone();
            modified.sig.generics = syn::parse2(generics)?;
            assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
        for (position, _) in original.sig.inputs.iter().enumerate() {
            for argument in [quote!(other: Type<'db>), quote!(effects: &mut E)] {
                let mut modified = original.clone();
                modified.sig.inputs[position] = syn::parse2(argument)?;
                assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
            }
        }
        for (_, _, _, output) in BASE_MRO_SIGNATURES {
            let output = syn::parse_str::<syn::Type>(output)?;
            let output = syn::parse2(quote!(-> #output))?;
            if original.sig.output != output {
                let mut modified = original.clone();
                modified.sig.output = output;
                assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
            }
        }
        let mut modified = original.clone();
        modified.sig.output = syn::parse_quote!(-> Result<Type<'db>, OtherError>);
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.push(syn::parse_quote!(extra: usize));
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs.pop();
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.inputs[0] = original.sig.inputs[1].clone();
        modified.sig.inputs[1] = original.sig.inputs[0].clone();
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.asyncness = None;
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.safety = syn::parse_quote!(unsafe);
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original.clone();
        modified.sig.abi = Some(syn::parse_quote!(extern "C"));
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        let mut modified = original;
        modified.sig.constness = Some(syn::token::Const::default());
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
    }
    Ok(())
}

#[test]
fn base_mro_attribute_accepts_only_four_manifests() -> Result<()> {
    for index in 0..BASE_MRO_SIGNATURES.len() {
        let original = base_mro_function(index, &quote!())?;
        assert!(expand_base_mro(quote!(option), &original).is_err());
        for expand_other in [
            expand,
            expand_class_type,
            expand_mro,
            expand_mro_root,
            expand_promotion,
            expand_synthesized,
            expand_static_mro,
            expand_c3,
        ] {
            assert!(expand_other(TokenStream::new(), &original).is_err());
        }
        for name in [
            "other_with",
            "compose_specialization_with",
            "collect_start_with",
        ] {
            let mut modified = syn::parse2::<syn::ItemFn>(original.clone())?;
            modified.sig.ident = Ident::new(name, Span::call_site());
            assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
        }
    }
    for other in [
        own_member(&quote!()),
        class_type_member(&quote!()),
        synthesized_member(&quote!()),
        mro_member(&quote!()),
        finalize_member(&quote!()),
        promotion("promote_public_with", &quote!()),
        promotion("promote_singletons_impl_with", &quote!()),
        c3_merge(&quote!()),
    ] {
        assert!(expand_base_mro(TokenStream::new(), &other).is_err());
    }
    for index in 0..STATIC_MRO_SIGNATURES.len() {
        assert!(
            expand_base_mro(TokenStream::new(), &static_mro_function(index, &quote!())?).is_err()
        );
    }
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        assert!(
            expand_base_mro(
                TokenStream::new(),
                &mro_root(name, class, output, &quote!())
            )
            .is_err()
        );
    }
    Ok(())
}

fn c3_merge(body: &TokenStream) -> TokenStream {
    quote! {
        #[inline]
        pub(in crate::types) async fn c3_merge_with<'db, E: C3Effects<'db>>(
            fields: MroFieldReads<'db>,
            mut sequences: Vec<VecDeque<ClassBase<'db>>>,
            effects: &E,
        ) -> Result<Option<Mro<'db>>, E::Error> {
            #body
        }
    }
}

#[test]
fn c3_merge_preserves_ordered_collection_and_identity_operations() -> Result<()> {
    let original = c3_merge(&quote! {
        let occurrences: Option<C3Occurrences> = effects.start_occurrences(sequences.len()).await?;
        effects.checkpoint(C3Work::OutputCapacity { entries: 8 }).await?;
        let mut mro = Vec::with_capacity(8);
        effects.checkpoint(C3Work::RetainSequences { len: sequences.len() }).await?;
        sequences.retain(|sequence| !sequence.is_empty());
        let mut candidates = sequences.iter();
        loop {
            effects.checkpoint(C3Work::CandidateAdvance).await?;
            let Some(sequence) = candidates.next() else { break };
            let selected = sequence[0];
            let identity = effects.mro_identity(fields, selected).await?;
            for base in sequence.iter().skip(1) {
                let tail_identity = effects.mro_identity(fields, *base).await?;
                effects.checkpoint(C3Work::IdentityComparison {
                    todo_bytes: comparison_label_bytes(identity, tail_identity),
                }).await?;
                if identity == tail_identity {
                    effects.checkpoint(C3Work::Publish).await?;
                    return Ok(None);
                }
            }
            effects.checkpoint(C3Work::OutputAppend { prefix_len: mro.len(), capacity: mro.capacity() }).await?;
            mro.push(selected);
        }
        effects.checkpoint(C3Work::BoxOutput { len: mro.len(), capacity: mro.capacity() }).await?;
        let mro = Mro::from(mro);
        effects.checkpoint(C3Work::Publish).await?;
        effects.publish_occurrences(occurrences).await?;
        Ok(Some(mro))
    });
    let synchronous = quote! {
        #[inline]
        pub(in crate::types) fn c3_merge_sync<'db, E: crate::types::mro::c3::SynchronousC3Effects>(
            db: &'db dyn Db,
            mut sequences: Vec<VecDeque<ClassBase<'db>>>,
            effects: &E,
        ) -> Result<Option<Mro<'db>>, E::Error> {
            let occurrences: Option<C3Occurrences> = effects.start_occurrences(sequences.len())?;
            effects.checkpoint(C3Work::OutputCapacity { entries: 8 })?;
            let mut mro = Vec::with_capacity(8);
            effects.checkpoint(C3Work::RetainSequences { len: sequences.len() })?;
            sequences.retain(|sequence| !sequence.is_empty());
            let mut candidates = sequences.iter();
            loop {
                effects.checkpoint(C3Work::CandidateAdvance)?;
                let Some(sequence) = candidates.next() else { break };
                let selected = sequence[0];
                let identity = effects.mro_identity(fields, selected)?;
                for base in sequence.iter().skip(1) {
                    let tail_identity = effects.mro_identity(fields, *base)?;
                    effects.checkpoint(C3Work::IdentityComparison {
                        todo_bytes: comparison_label_bytes(identity, tail_identity),
                    })?;
                    if identity == tail_identity {
                        effects.checkpoint(C3Work::Publish)?;
                        return Ok(None);
                    }
                }
                effects.checkpoint(C3Work::OutputAppend { prefix_len: mro.len(), capacity: mro.capacity() })?;
                mro.push(selected);
            }
            effects.checkpoint(C3Work::BoxOutput { len: mro.len(), capacity: mro.capacity() })?;
            let mro = Mro::from(mro);
            effects.checkpoint(C3Work::Publish)?;
            effects.publish_occurrences(occurrences)?;
            Ok(Some(mro))
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_c3(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

#[test]
fn c3_merge_accepts_only_direct_declared_effect_awaits() {
    for body in [
        quote!(effects.checkpoint(work)),
        quote!(effects.mro_identity(fields, base)),
        quote!(effects.start_occurrences(sequences.len())),
        quote!(effects.publish_occurrences(occurrences)),
        quote!(effects.checkpoint(effects).await),
        quote!(effects.mro_identity(fields, effects).await),
        quote!(effects.start_occurrences(effects).await),
        quote!(effects.publish_occurrences(effects).await),
        quote!((effects).checkpoint(work).await),
        quote!((effects).mro_identity(fields, base).await),
        quote!((effects).start_occurrences(sequences.len()).await),
        quote!((effects).publish_occurrences(occurrences).await),
        quote!(other.checkpoint(work).await),
        quote!(E::checkpoint(effects, work).await),
        quote!(effects.explicit_bases(class)),
        quote!(effects.raw_member(request)),
        quote!(effects.root_class(class, specialization).await),
        quote!(effects.c3_merge(sequences).await),
        quote!(effects.static_mro_is_cycle(class, specialization).await),
        quote!(effects.unknown()),
        quote!(effects.unknown().await),
        quote!(unknown().await),
        quote!(c3_merge_with(fields, sequences, effects).await),
        quote!(
            apply_optional_class_specialization_with(fields, class, specialization, effects).await
        ),
        quote!(maybe_add_generic_with(&mut bases, original, remaining, effects).await),
        quote!(base_has_cyclic_mro_with(fields, base, effects).await),
        quote!(let alias = effects;),
        quote!(consume(&effects)),
        quote!(let effects = other;),
        quote!(let r#effects = other;),
        quote!(let callback = || effects.checkpoint(work).await;),
        quote!(const { effects.checkpoint(work).await }),
        quote!(async { effects.checkpoint(work).await }),
        quote!(
            effects
                .checkpoint::<{
                    effects.checkpoint(work).await;
                    1
                }>(work)
                .await
        ),
        quote!(matches!(effects.checkpoint(work).await, _)),
        quote!(vec![base]),
        quote!(effects.checkpoint(hidden!()).await),
    ] {
        assert!(
            expand_c3(TokenStream::new(), &c3_merge(&body)).is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn c3_merge_signature_and_attribute_remain_exact() -> Result<()> {
    let original = syn::parse2::<syn::ItemFn>(c3_merge(&quote!()))?;
    for generics in [
        quote!(<'db, E: C3Effects>),
        quote!(<'db, E: C3Effects<'static>>),
        quote!(<'db, E: other::C3Effects<'db>>),
        quote!(<'db, E: C3Effects<'db> + Send>),
        quote!(<'db, E: StaticMroEffects<'db>>),
        quote!(<'a, 'db, E: C3Effects<'db>>),
    ] {
        let mut modified = original.clone();
        modified.sig.generics = syn::parse2(generics)?;
        assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for (position, argument) in [
        (0, quote!(db: &dyn Db)),
        (1, quote!(sequences: Vec<VecDeque<ClassBase<'db>>>)),
        (1, quote!(mut sequences: &mut Vec<VecDeque<ClassBase<'db>>>)),
        (1, quote!(mut sequences: Vec<Vec<ClassBase<'db>>>)),
        (2, quote!(effects: &mut E)),
    ] {
        let mut modified = original.clone();
        modified.sig.inputs[position] = syn::parse2(argument)?;
        assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for output in [
        quote!(-> Result<Mro<'db>, E::Error>),
        quote!(-> Result<Option<Mro<'db>>, OtherError>),
        quote!(-> Option<Mro<'db>>),
    ] {
        let mut modified = original.clone();
        modified.sig.output = syn::parse2(output)?;
        assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    }
    let mut modified = original.clone();
    modified.sig.asyncness = None;
    assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
    assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original.clone();
    modified.sig.ident = syn::parse_quote!(other_merge_with);
    assert!(expand_c3(TokenStream::new(), &quote!(#modified)).is_err());
    assert!(expand_c3(quote!(option), &quote!(#original)).is_err());
    for expand_other in [
        expand,
        expand_class_type,
        expand_mro,
        expand_mro_root,
        expand_promotion,
        expand_static_mro,
        expand_synthesized,
    ] {
        assert!(expand_other(TokenStream::new(), &quote!(#original)).is_err());
    }
    let mut other_manifests = vec![
        own_member(&quote!()),
        class_type_member(&quote!()),
        mro_member(&quote!()),
        finalize_member(&quote!()),
        promotion("promote_public_with", &quote!()),
        promotion("promote_singletons_impl_with", &quote!()),
        synthesized_member(&quote!()),
    ];
    for (name, _, class, output) in MRO_ROOT_SIGNATURES {
        other_manifests.push(mro_root(name, class, output, &quote!()));
    }
    for index in 0..STATIC_MRO_SIGNATURES.len() {
        other_manifests.push(static_mro_function(index, &quote!())?);
    }
    for other in other_manifests {
        assert!(expand_c3(TokenStream::new(), &other).is_err());
    }
    Ok(())
}

#[test]
fn mro_cycle_seed_and_base_collections_lower_their_exact_requests() -> Result<()> {
    let cycle = quote! {
        async fn static_mro_cycle_with<'db, E: StaticMroEffects<'db>>(
            fields: MroFieldReads<'db>, class_literal: StaticClassLiteral<'db>,
            specialization: Option<Specialization<'db>>, effects: &E,
        ) -> Result<StaticMroError<'db>, E::Error> {
            effects.checkpoint(Begin).await?;
            let env = ProgramEnvironment::from_scope(fields.body_scope(class_literal));
            let class = effects.root_class(class_literal, specialization).await?;
            effects.make_error(&env, class, InheritanceCycle).await
        }
    };
    let synchronous = quote! {
        fn static_mro_cycle_sync<'db, E: crate::types::mro::construction::SynchronousStaticMroEffects<'db>>(
            db: &'db dyn Db, class_literal: StaticClassLiteral<'db>,
            specialization: Option<Specialization<'db>>, effects: &E,
        ) -> Result<StaticMroError<'db>, E::Error> {
            effects.checkpoint(Begin)?;
            let env = ProgramEnvironment::from_scope(fields.body_scope(class_literal));
            let class = effects.root_class(class_literal, specialization)?;
            effects.make_error(&env, class, InheritanceCycle)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_static_mro(TokenStream::new(), &cycle)?)?,
        expected_mro_expansion(&cycle, &synchronous)?
    );

    let cursor = quote! {
        async fn base_cursor_next_with<'db, E: MroIterationEffects<'db>>(
            fields: MroFieldReads<'db>, cursor: &mut BaseCursor<'db>, effects: &E,
        ) -> Result<Option<ClassBase<'db>>, E::Error> {
            match cursor {
                BaseCursor::Length2(entries) => {
                    effects.iteration_checkpoint(Advance).await?;
                    Ok(entries.next())
                }
                BaseCursor::Class(cursor) => mro_next_with(fields, cursor, Forward, effects).await,
            }
        }
    };
    let synchronous = quote! {
        fn base_cursor_next_sync<'db, E: crate::types::mro::iteration::SynchronousMroIterationEffects<'db>>(
            db: &'db dyn Db, cursor: &mut BaseCursor<'db>, effects: &E,
        ) -> Result<Option<ClassBase<'db>>, E::Error> {
            match cursor {
                BaseCursor::Length2(entries) => {
                    effects.iteration_checkpoint(Advance)?;
                    Ok(entries.next())
                }
                BaseCursor::Class(cursor) => mro_next_sync(db, cursor, Forward, effects),
            }
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_base_mro(TokenStream::new(), &cursor)?)?,
        expected_mro_expansion(&cursor, &synchronous)?
    );

    for (name, sync_name, operands, sync_operands, output) in [
        (
            "collect_start_with",
            "collect_start_sync",
            quote!(fields: MroFieldReads<'db>, start: BaseMroStart<'db>, effects: &E),
            quote!(db: &'db dyn Db, start: BaseMroStart<'db>, effects: &E),
            quote!(VecDeque<ClassBase<'db>>),
        ),
        (
            "collect_start_with_root_with",
            "collect_start_with_root_sync",
            quote!(fields: MroFieldReads<'db>, root: ClassType<'db>, start: BaseMroStart<'db>, effects: &E),
            quote!(db: &'db dyn Db, root: ClassType<'db>, start: BaseMroStart<'db>, effects: &E),
            quote!(Mro<'db>),
        ),
    ] {
        let name = Ident::new(name, Span::call_site());
        let sync_name = Ident::new(sync_name, Span::call_site());
        let original = quote! {
            async fn #name<'db, E: MroCollectionEffects<'db>>(#operands) -> Result<#output, E::Error> {
                let mut cursor = BaseCursor::new(start);
                while let Some(base) = base_cursor_next_with(fields, &mut cursor, effects).await? {
                    effects.collection_checkpoint(Append).await?;
                    output.push(base);
                }
                effects.collection_checkpoint(Publish).await?;
                Ok(output)
            }
        };
        let synchronous = quote! {
            fn #sync_name<'db, E: crate::types::mro::collection::SynchronousMroCollectionEffects<'db>>(#sync_operands) -> Result<#output, E::Error> {
                let mut cursor = BaseCursor::new(start);
                while let Some(base) = base_cursor_next_sync(db, &mut cursor, effects)? {
                    effects.collection_checkpoint(Append)?;
                    output.push(base);
                }
                effects.collection_checkpoint(Publish)?;
                Ok(output)
            }
        };
        assert_eq!(
            syn::parse2::<syn::File>(expand_base_mro(TokenStream::new(), &original)?)?,
            expected_mro_expansion(&original, &synchronous)?
        );
        let mut rejected: syn::ItemFn = syn::parse2(original)?;
        rejected.block = Box::new(syn::parse_quote!({
            base_cursor_next_with(db, cursor, effects).await
        }));
        assert!(expand_base_mro(TokenStream::new(), &quote!(#rejected)).is_err());
        rejected.block = Box::new(syn::parse_quote!({
            effects.collection_checkpoint(Publish)
        }));
        assert!(expand_base_mro(TokenStream::new(), &quote!(#rejected)).is_err());
    }
    Ok(())
}

fn class_literal_collection(body: &TokenStream) -> TokenStream {
    quote! {
        async fn collect_class_literals_with<'db, E: ClassLiteralCollectionEffects<'db>>(
            fields: MroFieldReads<'db>, class: ClassLiteral<'db>, effects: &E,
        ) -> Result<Box<[ClassLiteral<'db>]>, E::Error> {
            #body
        }
    }
}

/// Verifies that synchronous expansion preserves the order of literal-collection checkpoints and class-literal conversion.
#[test]
fn class_literal_collection_preserves_admission_and_conversion_order() -> Result<()> {
    let original = class_literal_collection(&quote! {
        effects.literal_collection_checkpoint(MroCollectionWork::Begin).await?;
        let mut cursor = MroCursor::new(class, None);
        let mut output = Vec::new();
        while let Some(base) = mro_next_with(fields, &mut cursor, MroDirection::Forward, effects).await? {
            effects.literal_collection_checkpoint(MroCollectionWork::Classify).await?;
            if let ClassBase::Class(class) = base {
                effects.literal_collection_checkpoint(MroCollectionWork::Append {
                    len: output.len(), capacity: output.capacity(),
                }).await?;
                output.push(effects.class_literal(fields, class).await?);
            }
        }
        effects.literal_collection_checkpoint(MroCollectionWork::BoxOutput {
            len: output.len(), capacity: output.capacity(),
        }).await?;
        let output = output.into_boxed_slice();
        effects.literal_collection_checkpoint(MroCollectionWork::Publish).await?;
        Ok(output)
    });
    let synchronous = quote! {
        fn collect_class_literals_sync<'db, E: crate::types::mro::collection::SynchronousClassLiteralCollectionEffects<'db>>(
            db: &'db dyn Db, class: ClassLiteral<'db>, effects: &E,
        ) -> Result<Box<[ClassLiteral<'db>]>, E::Error> {
            effects.literal_collection_checkpoint(MroCollectionWork::Begin)?;
            let mut cursor = MroCursor::new(class, None);
            let mut output = Vec::new();
            while let Some(base) = mro_next_sync(db, &mut cursor, MroDirection::Forward, effects)? {
                effects.literal_collection_checkpoint(MroCollectionWork::Classify)?;
                if let ClassBase::Class(class) = base {
                    effects.literal_collection_checkpoint(MroCollectionWork::Append {
                        len: output.len(), capacity: output.capacity(),
                    })?;
                    output.push(effects.class_literal(fields, class)?);
                }
            }
            effects.literal_collection_checkpoint(MroCollectionWork::BoxOutput {
                len: output.len(), capacity: output.capacity(),
            })?;
            let output = output.into_boxed_slice();
            effects.literal_collection_checkpoint(MroCollectionWork::Publish)?;
            Ok(output)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_base_mro(TokenStream::new(), &original)?)?,
        expected_mro_expansion(&original, &synchronous)?,
    );
    Ok(())
}

/// Verifies that macro expansion rejects unsupported dependencies and calls outside the supported direct, awaited forms in the collector body.
#[test]
fn class_literal_collection_rejects_undeclared_and_indirect_dependencies() {
    for body in [
        quote!(effects.collection_checkpoint(work).await),
        quote!(effects.iteration_checkpoint(work).await),
        quote!(effects.full_mro(request).await),
        quote!(effects.literal_collection_checkpoint(work)),
        quote!(effects.class_literal(fields, class)),
        quote!(class.class_literal(db)),
        quote!(fields.class_literal(class)),
        quote!(mro_next_with(fields, &mut cursor, MroDirection::Forward, effects)),
        quote!(mro_next_with(db, &mut cursor, MroDirection::Forward, effects).await),
        quote!(mro_next_with(fields, &mut cursor, effects).await),
        quote!(mro_next_with(fields, &mut cursor, MroDirection::Forward, &effects).await),
        quote!(other::mro_next_with(fields, &mut cursor, MroDirection::Forward, effects).await),
        quote!(base_cursor_next_with(fields, &mut cursor, effects).await),
        quote!(let next = mro_next_with;),
        quote!(let provider = effects;),
        quote!(let deferred = || effects.class_literal(fields, class).await;),
        quote!(async { effects.literal_collection_checkpoint(work).await }),
        quote!(matches!(effects.class_literal(fields, class).await, _)),
    ] {
        assert!(
            expand_base_mro(TokenStream::new(), &class_literal_collection(&body)).is_err(),
            "accepted {body}",
        );
    }
}

/// Verifies that macro expansion accepts only the collector's declared signature and macro entry point.
#[test]
fn class_literal_collection_requires_its_exact_signature() -> Result<()> {
    let original = class_literal_collection(&quote!());
    assert!(expand_base_mro(TokenStream::new(), &original).is_ok());
    assert!(expand_base_mro(quote!(option), &original).is_err());
    assert!(expand_mro_iteration(TokenStream::new(), &original).is_err());
    let original = syn::parse2::<syn::ItemFn>(original)?;
    for generics in [
        quote!(<'db, E: MroCollectionEffects<'db>>),
        quote!(<'db, E: SynchronousClassLiteralCollectionEffects<'db>>),
        quote!(<'db, E: other::ClassLiteralCollectionEffects<'db>>),
        quote!(<'db, E: ClassLiteralCollectionEffects<'static>>),
        quote!(<'db, E: ClassLiteralCollectionEffects<'db> + Send>),
    ] {
        let mut modified = original.clone();
        modified.sig.generics = syn::parse2(generics)?;
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
    }
    for (position, argument) in [
        (0, quote!(db: &'db dyn Db)),
        (1, quote!(class: ClassType<'db>)),
        (2, quote!(effects: &mut E)),
    ] {
        let mut modified = original.clone();
        modified.sig.inputs[position] = syn::parse2(argument)?;
        assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
    }
    let mut modified = original.clone();
    modified.sig.output = syn::parse_quote!(-> Result<Box<[ClassBase<'db>]>, E::Error>);
    assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
    let mut modified = original;
    modified.sig.asyncness = None;
    assert!(expand_base_mro(TokenStream::new(), &quote!(#modified)).is_err());
    Ok(())
}

#[test]
fn protocol_relation_entry_lowers_only_its_declared_children() -> Result<()> {
    let original = quote! { async fn check_type_satisfies_protocol_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> { effects.checkpoint(Entry).await?;
    let step = protocol_relation_start_with(fields, checker, ty, protocol, effects).await?;
    protocol_relation_run_with(fields, step, effects).await } };
    let synchronous = quote! { fn check_type_satisfies_protocol_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> { effects.checkpoint(Entry)?;
    let step = protocol_relation_start_sync(fields, checker, ty, protocol, effects)?;
    protocol_relation_run_sync(fields, step, effects) } };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_relation_resume_keeps_the_owned_fold_and_continuation() -> Result<()> {
    let original = quote! { async fn protocol_relation_member_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        pending: PendingProtocolMember<'checker, 'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { effects.checkpoint(Transition).await?;
    match pending.continuation {
        MemberContinuation::Structural(mut members) => {
            match effects.push_constraints(&mut members.fold, result).await? {
                ControlFlow::Break(result) => protocol_relation_finish_with(fields, members.input, members.nominal_result, result, effects).await,
                ControlFlow::Continue(()) => protocol_relation_structural_next_with(fields, members, effects).await,
            }
        }
    } } };
    let synchronous = quote! { fn protocol_relation_member_resume_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        pending: PendingProtocolMember<'checker, 'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { effects.checkpoint(Transition)?;
    match pending.continuation {
        MemberContinuation::Structural(mut members) => {
            match effects.push_constraints(&mut members.fold, result)? {
                ControlFlow::Break(result) => protocol_relation_finish_sync(fields, members.input, members.nominal_result, result, effects),
                ControlFlow::Continue(()) => protocol_relation_structural_next_sync(fields, members, effects),
            }
        }
    } } };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_relation_bindings_resume_keeps_its_short_borrow() -> Result<()> {
    let original = quote! { async fn protocol_relation_meta_bindings_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        pending: PendingMetaBindings<'checker, 'a, 'c, 'db>,
        bindings: &Bindings<'db>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { let _ = fields;
    effects.checkpoint(Transition).await?;
    let instance_ty = effects.bindings_return_type(pending.checker, bindings).await?;
    Ok(ProtocolRelationStep::Relate(PendingProtocolPair { checker: pending.checker, source: instance_ty, target: Type::ProtocolInstance(pending.protocol), continuation: PairContinuation::Meta { meta_ty: pending.meta_ty, protocol: pending.protocol } })) } };
    let synchronous = quote! { fn protocol_relation_meta_bindings_resume_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        pending: PendingMetaBindings<'checker, 'a, 'c, 'db>,
        bindings: &Bindings<'db>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { let _ = fields;
    effects.checkpoint(Transition)?;
    let instance_ty = effects.bindings_return_type(pending.checker, bindings)?;
    Ok(ProtocolRelationStep::Relate(PendingProtocolPair { checker: pending.checker, source: instance_ty, target: Type::ProtocolInstance(pending.protocol), continuation: PairContinuation::Meta { meta_ty: pending.meta_ty, protocol: pending.protocol } })) } };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_relation_cursor_and_fold_effects_keep_mutable_borrows() -> Result<()> {
    let original = quote! { async fn protocol_relation_structural_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        members: StructuralMembers<'checker, 'a, 'c, 'db>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { effects.checkpoint(Transition).await?;
    let mut members = members;
    if let Some(member) = effects.next_interface_member(&mut members.members).await? {
        return Ok(ProtocolRelationStep::Member(PendingProtocolMember { checker: members.input.checker, ty: members.input.ty, member, continuation: MemberContinuation::Structural(members) }));
    }
    let result = effects.finish_constraints(&mut members.fold).await?;
    protocol_relation_finish_with(fields, members.input, members.nominal_result, result, effects).await } };
    let synchronous = quote! { fn protocol_relation_structural_next_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
        fields: RelationFieldReads<'db>,
        members: StructuralMembers<'checker, 'a, 'c, 'db>,
        effects: &E,
    ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> { effects.checkpoint(Transition)?;
    let mut members = members;
    if let Some(member) = effects.next_interface_member(&mut members.members)? {
        return Ok(ProtocolRelationStep::Member(PendingProtocolMember { checker: members.input.checker, ty: members.input.ty, member, continuation: MemberContinuation::Structural(members) }));
    }
    let result = effects.finish_constraints(&mut members.fold)?;
    protocol_relation_finish_sync(fields, members.input, members.nominal_result, result, effects) } };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_object_comparison_preserves_direct_then_solver_order() -> Result<()> {
    let original = quote! { async fn protocol_object_compare_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> { let constraints = effects.check_type_satisfies_protocol(checker, Type::object(), protocol).await?;
    effects.is_always_satisfied(checker, constraints).await } };
    let synchronous = quote! { fn protocol_object_compare_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> { let constraints = effects.check_type_satisfies_protocol(checker, Type::object(), protocol)?;
    effects.is_always_satisfied(checker, constraints) } };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

fn protocol_relation_signatures() -> Vec<TokenStream> {
    vec![
        quote! { async fn check_type_satisfies_protocol_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ConstraintSet<'db, 'c>, E::Error> {} },
        quote! { async fn check_meta_type_satisfies_protocol_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            meta_ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ConstraintSet<'db, 'c>, E::Error> {} },
        quote! { async fn protocol_relation_run_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            step: ProtocolRelationStep<'checker, 'a, 'c, 'db>,
            effects: &E,
        ) -> Result<ConstraintSet<'db, 'c>, E::Error> {} },
        quote! { async fn protocol_relation_start_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_start_meta_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            meta_ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_pair_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            pending: PendingProtocolPair<'checker, 'a, 'c, 'db>,
            result: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_after_nominal_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            input: ProtocolInput<'checker, 'a, 'c, 'db>,
            source_nominal: Option<NominalInstanceType<'db>>,
            target_nominal: NominalInstanceType<'db>,
            nominally_satisfied: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_non_recursive_interface_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            input: ProtocolInput<'checker, 'a, 'c, 'db>,
            source_nominal: Option<NominalInstanceType<'db>>,
            target_nominal: NominalInstanceType<'db>,
            effects: &E,
        ) -> Result<Option<FiniteInterface<'db>>, E::Error> {} },
        quote! { async fn protocol_relation_structural_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            input: ProtocolInput<'checker, 'a, 'c, 'db>,
            nominal_result: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_nominal_recursive_members_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            input: ProtocolInput<'checker, 'a, 'c, 'db>,
            nominally_satisfied: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<Option<NominalRecursiveMembers<'checker, 'a, 'c, 'db>>, E::Error> {} },
        quote! { async fn protocol_relation_finish_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            input: ProtocolInput<'checker, 'a, 'c, 'db>,
            nominal_result: ConstraintSet<'db, 'c>,
            structural: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_interface_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            pending: PendingProtocolInterface<'checker, 'a, 'c, 'db>,
            structural: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_structural_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            members: StructuralMembers<'checker, 'a, 'c, 'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_nominal_finite_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_nominal_recursive_next_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            members: NominalRecursiveMembers<'checker, 'a, 'c, 'db>,
            structural: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_member_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            pending: PendingProtocolMember<'checker, 'a, 'c, 'db>,
            result: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_meta_bindings_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            pending: PendingMetaBindings<'checker, 'a, 'c, 'db>,
            bindings: &Bindings<'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_relation_meta_members_resume_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            pending: PendingMetaMembers<'checker, 'a, 'c, 'db>,
            result: ConstraintSet<'db, 'c>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {} },
        quote! { async fn protocol_object_compare_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<bool, E::Error> {} },
    ]
}

#[test]
fn protocol_relation_signatures_and_attribute_are_closed() -> Result<()> {
    for original in protocol_relation_signatures() {
        expand_protocol_relation(TokenStream::new(), &original)?;
        let original: syn::ItemFn = syn::parse2(original)?;
        for generics in [
            quote!(<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'static>>),
            quote!(<'checker, 'a, 'c, 'db, E: other::ProtocolRelationEffects<'a, 'c, 'db>>),
            quote!(<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db> + Send>),
            quote!(<'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>),
        ] {
            let mut changed = original.clone();
            changed.sig.generics = syn::parse2(generics)?;
            assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        }
        for first in [quote!(self), quote!(db: &'db dyn Db)] {
            let mut changed = original.clone();
            changed.sig.inputs[0] = syn::parse2(first)?;
            assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        }
        let mut changed = original.clone();
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &mut E));
        assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed = original.clone();
        changed.sig.output = syn::parse_quote!(-> Result<(), OtherError>);
        assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed = original.clone();
        changed.sig.asyncness = None;
        assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed = original.clone();
        changed.sig.generics.where_clause = Some(syn::parse_quote!(where E: Send));
        assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed = original.clone();
        changed.sig.ident = syn::parse_quote!(other_protocol_relation_with);
        assert!(expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err());
        assert!(expand_protocol_relation(quote!(option), &quote!(#original)).is_err());
        assert!(expand_protocol_object(TokenStream::new(), &quote!(#original)).is_err());
        assert!(expand_protocol_interface(TokenStream::new(), &quote!(#original)).is_err());
    }
    assert!(expand_protocol_relation(TokenStream::new(), &protocol_object(&quote!())).is_err());
    Ok(())
}

#[test]
fn protocol_relation_rejects_unlisted_effects_and_helper_escapes() -> Result<()> {
    let original: syn::ItemFn = syn::parse2(protocol_relation_signatures().remove(0))?;
    for body in [
        quote!({ effects.checkpoint(work) }),
        quote!({ effects.protocol_interface(protocol).await }),
        quote!({ effects.unknown().await }),
        quote!({ other.checkpoint(work).await }),
        quote!({ E::checkpoint(effects, work).await }),
        quote!({
            let alias = effects;
            alias.checkpoint(work).await
        }),
        quote!({
            let effects = other;
        }),
        quote!({
            let callback = || effects.checkpoint(work).await;
        }),
        quote!({ async { effects.checkpoint(work).await } }),
        quote!({ matches!(effects.checkpoint(work).await, _) }),
        quote!({ protocol_relation_start_with(fields, checker, ty, protocol, effects) }),
        quote!({
            other::protocol_relation_start_with(fields, checker, ty, protocol, effects).await
        }),
        quote!({ protocol_relation_start_with(fields, checker, protocol, effects).await }),
        quote!({ protocol_relation_start_with(fields, checker, ty, protocol, &effects).await }),
        quote!({ protocol_relation_start_with(fields, checker, ty, protocol, other).await }),
        quote!({ protocol_relation_start_meta_with(fields, checker, ty, protocol, effects).await }),
        quote!({ protocol_relation_finish_with(fields, input, left, right, effects).await }),
        quote!({
            let protocol_relation_start_with = other;
        }),
        quote!({
            let r#protocol_relation_start_with = other;
        }),
        quote!({
            let helper = protocol_relation_start_with;
        }),
        quote!({
            let helper = r#protocol_relation_start_with;
        }),
        quote!({ protocol_interface_build_with(class, effects).await }),
    ] {
        let mut changed = original.clone();
        changed.block = Box::new(syn::parse2(body.clone())?);
        assert!(
            expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err(),
            "accepted {body}"
        );
    }
    let mut comparison: syn::ItemFn = syn::parse2(protocol_relation_signatures().remove(18))?;
    comparison.block = Box::new(syn::parse_quote!({ effects.checkpoint(work).await }));
    assert!(expand_protocol_relation(TokenStream::new(), &quote!(#comparison)).is_err());
    Ok(())
}

#[test]
fn protocol_meta_entry_keeps_its_debug_invariant() -> Result<()> {
    let original = quote! {
        async fn protocol_relation_start_meta_with<'checker, 'a, 'c, 'db, E: ProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            meta_ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
            let _ = fields;
            effects.checkpoint(Transition).await?;
            debug_assert!(matches!(
                meta_ty,
                Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_)
            ));
            let constructor_ty = effects.to_class_type(meta_ty).await?.map_or(meta_ty, Type::from);
            Ok(ProtocolRelationStep::MetaBindings(PendingMetaBindings { checker, constructor_ty, meta_ty, protocol }))
        }
    };
    let synchronous = quote! {
        fn protocol_relation_start_meta_sync<'checker, 'a, 'c, 'db, E: crate::types::instance::protocol_relation::SyncProtocolRelationEffects<'a, 'c, 'db>>(
            fields: RelationFieldReads<'db>,
            checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
            meta_ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<ProtocolRelationStep<'checker, 'a, 'c, 'db>, E::Error> {
            let _ = fields;
            effects.checkpoint(Transition)?;
            debug_assert!(matches!(
                meta_ty,
                Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_)
            ));
            let constructor_ty = effects.to_class_type(meta_ty)?.map_or(meta_ty, Type::from);
            Ok(ProtocolRelationStep::MetaBindings(PendingMetaBindings { checker, constructor_ty, meta_ty, protocol }))
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_relation(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_meta_debug_assertion_remains_a_checked_match() -> Result<()> {
    let original: syn::ItemFn = syn::parse2(protocol_relation_signatures().remove(4))?;
    for body in [
        quote!({
            debug_assert!(condition);
        }),
        quote!({
            debug_assert!(matches!(meta_ty, Type::ClassLiteral(_)), "message");
        }),
        quote!({
            std::debug_assert!(matches!(meta_ty, Type::ClassLiteral(_)));
        }),
        quote!({
            debug_assert!(std::matches!(meta_ty, Type::ClassLiteral(_)));
        }),
        quote!({
            debug_assert!(matches!(hidden!(), _));
        }),
        quote!({
            debug_assert!(matches!(debug_assert!(matches!(meta_ty, _)), _));
        }),
        quote!({
            debug_assert!(matches!(effects.to_class_type(meta_ty).await, _));
        }),
        quote!({
            debug_assert!(matches!(effects.to_class_type(meta_ty), _));
        }),
        quote!({
            debug_assert!(matches!(effects, _));
        }),
        quote!({
            debug_assert!(matches!(meta_ty, _ if effects.checkpoint(work).await));
        }),
        quote!({
            let alias = effects;
            debug_assert!(matches!(alias, _));
        }),
        quote!({
            let deferred = || debug_assert!(matches!(meta_ty, _));
        }),
        quote!({
            matches!(debug_assert!(matches!(meta_ty, _)), _);
        }),
    ] {
        let mut changed = original.clone();
        changed.block = Box::new(syn::parse2(body.clone())?);
        assert!(
            expand_protocol_relation(TokenStream::new(), &quote!(#changed)).is_err(),
            "accepted {body}"
        );
    }
    let mut other: syn::ItemFn = syn::parse2(protocol_relation_signatures().remove(0))?;
    other.block = Box::new(syn::parse_quote!({
        debug_assert!(matches!(ty, Type::ClassLiteral(_)));
    }));
    assert!(expand_protocol_relation(TokenStream::new(), &quote!(#other)).is_err());
    Ok(())
}

#[test]
fn satisfaction_signatures_preserve_mutable_resource_types() -> Result<()> {
    let signatures = [
        quote! { async fn node_satisfaction_with<'db, E: SatisfactionEffects<'db>>(
            node: NodeId,
            source_order: Option<SourceOrderId>,
            kind: SatisfactionKind,
            effects: &mut E,
        ) -> Result<bool, E::Error> },
        quote! { async fn simple_conjunction_satisfiable_with<'db, E: SatisfactionEffects<'db>>(
            node: NodeId,
            effects: &mut E,
        ) -> Result<bool, E::Error> },
        quote! { async fn path_assignments_with<'db, E: SatisfactionEffects<'db>>(
            interior: InteriorNode,
            source_order: Option<SourceOrderId>,
            effects: &mut E,
        ) -> Result<PathAssignments, E::Error> },
        quote! { async fn path_visit_owned_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
            path: PathAssignments,
            node: NodeId,
            visitor: &mut V,
            negated: bool,
            effects: &mut E,
        ) -> Result<PathVisitCompletion<V>, E::Error> },
        quote! { async fn path_visit_body_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
            state: &mut PathVisitState,
            visitor: &mut V,
            effects: &mut E,
        ) -> Result<ControlFlow<V::Break, V::Result>, E::Error> },
        quote! { async fn path_enter_edge_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            assignment: ConstraintAssignment,
            effects: &mut E,
        ) -> Result<EdgeOutcome, E::Error> },
        quote! { async fn path_drain_assignments_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            source_constraint: ConstraintId,
            effects: &mut E,
        ) -> Result<Result<(), PathAssignmentConflict>, E::Error> },
        quote! { async fn path_add_assignment_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            assignment: ConstraintAssignment,
            source_constraint: ConstraintId,
            fuel: AssignmentFuel,
            effects: &mut E,
        ) -> Result<Result<(), PathAssignmentConflict>, E::Error> },
        quote! { async fn path_discover_constraint_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            constraint: ConstraintId,
            effects: &mut E,
        ) -> Result<(), E::Error> },
        quote! { async fn path_import_sequents_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            map: &SequentMap<'db>,
            effects: &mut E,
        ) -> Result<Range<usize>, E::Error> },
        quote! { async fn path_import_slice_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            sequents: &[Sequent<Constraint<'db>>],
            effects: &mut E,
        ) -> Result<(), E::Error> },
        quote! { async fn path_import_sequent_with<'db, E: PathEffects<'db>>(
            sequent: Sequent<Constraint<'db>>,
            effects: &mut E,
        ) -> Result<Sequent<ConstraintId, u16>, E::Error> },
        quote! { async fn path_check_sequent_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            sequent: Sequent<ConstraintId, u16>,
            effects: &mut E,
        ) -> Result<Result<(), PathAssignmentConflict>, E::Error> },
        quote! { async fn path_check_pair_implication_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            ante1: ConstraintId,
            ante2: ConstraintId,
            post: ConstraintId,
            fuel_cost: u16,
            effects: &mut E,
        ) -> Result<(), E::Error> },
        quote! { async fn path_check_single_implication_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            ante: ConstraintId,
            post: ConstraintId,
            fuel_cost: u16,
            effects: &mut E,
        ) -> Result<(), E::Error> },
    ];
    for signature in signatures {
        let original = quote! { #signature { loop {} } };
        let result = syn::parse2::<syn::File>(expand_satisfaction(TokenStream::new(), &original)?)?;
        assert_eq!(result.items.len(), 2);
        let syn::Item::Fn(original_fn) = &result.items[0] else {
            panic!("expected authored function");
        };
        let syn::Item::Fn(synchronous) = &result.items[1] else {
            panic!("expected synchronous function");
        };
        assert_eq!(original_fn.sig.inputs, synchronous.sig.inputs);
        assert_eq!(original_fn.sig.output, synchronous.sig.output);
        assert_eq!(original_fn.block, synchronous.block);
        assert!(synchronous.sig.asyncness.is_none());
        let expected_name = original_fn.sig.ident.to_string().replace("_with", "_sync");
        assert_eq!(synchronous.sig.ident.to_string(), expected_name);

        let mut wrong: syn::ItemFn = syn::parse2(original)?;
        let Some(syn::FnArg::Typed(last)) = wrong.sig.inputs.last_mut() else {
            panic!("expected effects argument");
        };
        let syn::Type::Reference(reference) = last.ty.as_mut() else {
            panic!("expected mutable effects reference");
        };
        reference.mutability = None;
        assert!(expand_satisfaction(TokenStream::new(), &quote!(#wrong)).is_err());
    }
    Ok(())
}

#[test]
fn satisfaction_root_keeps_cache_and_child_order() -> Result<()> {
    let original = quote! {
            async fn node_satisfaction_with<'db, E: SatisfactionEffects<'db>>(
        node: NodeId,
        source_order: Option<SourceOrderId>,
        kind: SatisfactionKind,
        effects: &mut E,
    ) -> Result<bool, E::Error> {
                effects.checkpoint(Entry).await?;
                if let Some(result) = effects.never_cache_get(node).await? {
                    return Ok(result);
                }
                let result = if simple_conjunction_satisfiable_with(node, effects).await? {
                    false
                } else {
                    let path = path_assignments_with(interior, source_order, effects).await?;
                    path_visit_owned_with(path, node, &mut visitor, false, effects).await?.flow.is_continue()
                };
                effects.never_cache_insert(node, result).await?;
                Ok(result)
            }
        };
    let synchronous = quote! {
        fn node_satisfaction_sync<'db, E: crate::types::constraints::satisfaction::SyncSatisfactionEffects<'db>>(
            node: NodeId, source_order: Option<SourceOrderId>, kind: SatisfactionKind,
            effects: &mut E,
        ) -> Result<bool, E::Error> {
            effects.checkpoint(Entry)?;
            if let Some(result) = effects.never_cache_get(node)? {
                return Ok(result);
            }
            let result = if simple_conjunction_satisfiable_sync(node, effects)? {
                false
            } else {
                let path = path_assignments_sync(interior, source_order, effects)?;
                path_visit_owned_sync(path, node, &mut visitor, false, effects)?.flow.is_continue()
            };
            effects.never_cache_insert(node, result)?;
            Ok(result)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_satisfaction(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn satisfaction_traversal_moves_generic_visitor_values_without_cloning() -> Result<()> {
    let original = quote! {
            async fn path_visit_body_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
        state: &mut PathVisitState,
        visitor: &mut V,
        effects: &mut E,
    ) -> Result<ControlFlow<V::Break, V::Result>, E::Error> {
                let value: V::Interior = effects.enter_interior(visitor, interior).await?.continue_value();
                let child: V::Result = effects.visit_satisfied(visitor, &state.path).await?.continue_value();
                let result = effects.visit_edge(visitor, &value, child, &state.path, range).await?;
                state.path.restore_edge(&checkpoint);
                Ok(result)
            }
        };
    let synchronous = quote! {
        fn path_visit_body_sync<'db, V: PathVisitor, E: crate::types::constraints::paths::SyncPathVisitEffects<'db, V>>(
            state: &mut PathVisitState, visitor: &mut V, effects: &mut E,
        ) -> Result<ControlFlow<V::Break, V::Result>, E::Error> {
            let value: V::Interior = effects.enter_interior(visitor, interior)?.continue_value();
            let child: V::Result = effects.visit_satisfied(visitor, &state.path)?.continue_value();
            let result = effects.visit_edge(visitor, &value, child, &state.path, range)?;
            state.path.restore_edge(&checkpoint);
            Ok(result)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_satisfaction(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn satisfaction_discovery_keeps_queries_imports_and_publication_separate() -> Result<()> {
    let original = quote! {
            async fn path_discover_constraint_with<'db, E: PathEffects<'db>>(
        path: &mut PathAssignments,
        constraint: ConstraintId,
        effects: &mut E,
    ) -> Result<(), E::Error> {
                let map = effects.single_sequents(constraint_data).await?;
                let added = path_import_sequents_with(path, map, effects).await?;
                effects.reserve_replay(&mut consequents).await?;
                path.single_replay_consequents.insert(constraint, consequents);
                if !effects.pair_cannot_produce(existing, constraint_data).await? {
                    let map = effects.pair_sequents(existing, constraint_data).await?;
                    path_import_sequents_with(path, map, effects).await?;
                }
                Ok(())
            }
        };
    let synchronous = quote! {
        fn path_discover_constraint_sync<'db, E: crate::types::constraints::paths::SyncPathEffects<'db>>(
            path: &mut PathAssignments, constraint: ConstraintId, effects: &mut E,
        ) -> Result<(), E::Error> {
            let map = effects.single_sequents(constraint_data)?;
            let added = path_import_sequents_sync(path, map, effects)?;
            effects.reserve_replay(&mut consequents)?;
            path.single_replay_consequents.insert(constraint, consequents);
            if !effects.pair_cannot_produce(existing, constraint_data)? {
                let map = effects.pair_sequents(existing, constraint_data)?;
                path_import_sequents_sync(path, map, effects)?;
            }
            Ok(())
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_satisfaction(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn satisfaction_edge_preserves_debug_assertion_and_conflict_result() -> Result<()> {
    let original = quote! {
            async fn path_enter_edge_with<'db, E: PathEffects<'db>>(
        path: &mut PathAssignments,
        assignment: ConstraintAssignment,
        effects: &mut E,
    ) -> Result<EdgeOutcome, E::Error> {
                debug_assert!(path.assignment_queue.is_empty());
                effects.reserve_path(path, PathReserve::Queue { additional: 1 }).await?;
                let conflict = path_drain_assignments_with(path, assignment.constraint(), effects).await?.is_err();
                Ok(EdgeOutcome { checkpoint, new_range, found_conflict: conflict })
            }
        };
    let synchronous = quote! {
        fn path_enter_edge_sync<'db, E: crate::types::constraints::paths::SyncPathEffects<'db>>(
            path: &mut PathAssignments, assignment: ConstraintAssignment, effects: &mut E,
        ) -> Result<EdgeOutcome, E::Error> {
            debug_assert!(path.assignment_queue.is_empty());
            effects.reserve_path(path, PathReserve::Queue { additional: 1 })?;
            let conflict = path_drain_assignments_sync(path, assignment.constraint(), effects)?.is_err();
            Ok(EdgeOutcome { checkpoint, new_range, found_conflict: conflict })
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_satisfaction(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn satisfaction_closed_calls_reject_effect_and_helper_escape() {
    for body in [
        quote!(effects.single_sequents(data).await?;),
        quote!(let alias = effects;),
        quote!(let future = effects.never_cache_get(node);),
        quote!(let deferred = || effects.never_cache_get(node);),
        quote!(let deferred = async { effects.never_cache_get(node).await };),
        quote!(effects.never_cache_get(node);),
        quote!(path_import_sequents_with(path, map, effects).await?;),
        quote!(nested::simple_conjunction_satisfiable_with(node, effects).await?;),
        quote!(simple_conjunction_satisfiable_with(node, &mut effects).await?;),
        quote!(simple_conjunction_satisfiable_with(node, extra, effects).await?;),
        quote!(let alias = simple_conjunction_satisfiable_with;),
        quote!(let simple_conjunction_satisfiable_with = value;),
        quote!(let effects = other;),
    ] {
        let original = quote! { async fn node_satisfaction_with<'db, E: SatisfactionEffects<'db>>(
            node: NodeId,
            source_order: Option<SourceOrderId>,
            kind: SatisfactionKind,
            effects: &mut E,
        ) -> Result<bool, E::Error> { #body loop {} } };
        assert!(
            expand_satisfaction(TokenStream::new(), &original).is_err(),
            "{body}"
        );
    }
}

#[test]
fn satisfaction_debug_assertion_is_exact_and_local_to_enter_edge() {
    for assertion in [
        quote!(debug_assert!(other.assignment_queue.is_empty());),
        quote!(debug_assert!(path.assignment_queue.is_empty(), "message");),
        quote!(std::debug_assert!(path.assignment_queue.is_empty());),
        quote!(debug_assert!(effects.checkpoint(Entry).await);),
        quote!(debug_assert!(matches!(path, _));),
        quote!(debug_assert!(path.assignment_queue.is_empty() && flag);),
    ] {
        let original = quote! { async fn path_enter_edge_with<'db, E: PathEffects<'db>>(
            path: &mut PathAssignments,
            assignment: ConstraintAssignment,
            effects: &mut E,
        ) -> Result<EdgeOutcome, E::Error> { #assertion loop {} } };
        assert!(
            expand_satisfaction(TokenStream::new(), &original).is_err(),
            "{assertion}"
        );
    }
    let wrong_body = quote! {
            async fn path_drain_assignments_with<'db, E: PathEffects<'db>>(
        path: &mut PathAssignments,
        source_constraint: ConstraintId,
        effects: &mut E,
    ) -> Result<Result<(), PathAssignmentConflict>, E::Error> {
                debug_assert!(path.assignment_queue.is_empty());
                loop {}
            }
        };
    assert!(expand_satisfaction(TokenStream::new(), &wrong_body).is_err());
}

fn protocol_members_defined(body: &TokenStream) -> TokenStream {
    quote! {
        async fn protocol_members_defined_with<'db, E: ProtocolMembersDefinedEffects<'db>>(
            fields: RelationFieldReads<'db>,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<bool, E::Error> { #body }
    }
}

#[test]
fn protocol_preflight_lowers_exact_effects_without_changing_lazy_branches() -> Result<()> {
    let original = protocol_members_defined(&quote! {
        effects.checkpoint(Entry).await?;
        let target = effects.protocol_interface(protocol).await?;
        let result = match ty {
            Type::ProtocolInstance(source_protocol) => {
                let source = effects.protocol_interface(source_protocol).await?;
                if fields.protocol_interface_member_count(source) >= fields.protocol_interface_member_count(target)
                    || fields.protocol_interface_member_count(source) >= effects.non_object_member_count(target.base()).await?
                {
                    let mut members = InterfaceMembers::with_fields(fields, target);
                    loop {
                        let Some(member) = effects.next_interface_member(&mut members).await? else { break true; };
                        if !effects.includes_member_or_object_fallback(source, env, member.name()).await? { break false; }
                    }
                } else { false }
            }
            _ => {
                let mut members = InterfaceMembers::with_fields(fields, target);
                loop {
                    let Some(member) = effects.next_interface_member(&mut members).await? else { break true; };
                    if !effects.restricted_member(ty, env, member.name()).await?.place.is_definitely_bound()
                        && !effects.member(ty, env, member.name()).await?.place.is_definitely_bound()
                    { break false; }
                }
            }
        };
        effects.checkpoint(Complete).await?;
        Ok(result)
    });
    let synchronous = quote! {
        fn protocol_members_defined_sync<'db, E: crate::types::protocol_class::member_presence::SyncProtocolMembersDefinedEffects<'db>>(
            fields: RelationFieldReads<'db>,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            effects: &E,
        ) -> Result<bool, E::Error> {
            effects.checkpoint(Entry)?;
            let target = effects.protocol_interface(protocol)?;
            let result = match ty {
                Type::ProtocolInstance(source_protocol) => {
                    let source = effects.protocol_interface(source_protocol)?;
                    if fields.protocol_interface_member_count(source) >= fields.protocol_interface_member_count(target)
                        || fields.protocol_interface_member_count(source) >= effects.non_object_member_count(target.base())?
                    {
                        let mut members = InterfaceMembers::with_fields(fields, target);
                        loop {
                            let Some(member) = effects.next_interface_member(&mut members)? else { break true; };
                            if !effects.includes_member_or_object_fallback(source, env, member.name())? { break false; }
                        }
                    } else { false }
                }
                _ => {
                    let mut members = InterfaceMembers::with_fields(fields, target);
                    loop {
                        let Some(member) = effects.next_interface_member(&mut members)? else { break true; };
                        if !effects.restricted_member(ty, env, member.name())?.place.is_definitely_bound()
                            && !effects.member(ty, env, member.name())?.place.is_definitely_bound()
                        { break false; }
                    }
                }
            };
            effects.checkpoint(Complete)?;
            Ok(result)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand_protocol_members_defined(
            TokenStream::new(),
            &original
        )?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn protocol_preflight_rejects_changed_signature_or_attribute() -> Result<()> {
    let original = protocol_members_defined(&quote! { Ok(true) });
    assert!(expand_protocol_members_defined(quote!(option), &original).is_err());
    for other in [
        expand_protocol_object,
        expand_protocol_relation,
        expand_protocol_interface,
        expand_satisfaction,
    ] {
        assert!(other(TokenStream::new(), &original).is_err());
    }
    for name in [
        "other_with",
        "protocol_members_defined_sync",
        "protocol_object_equivalence_with",
    ] {
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.ident = Ident::new(name, Span::call_site());
        assert!(expand_protocol_members_defined(TokenStream::new(), &quote!(#changed)).is_err());
    }
    for (index, argument) in [
        (0, quote!(db: &'db dyn Db)),
        (1, quote!(env: &'db ProgramEnvironment<'db>)),
        (2, quote!(mut ty: Type<'db>)),
        (4, quote!(effects: &mut E)),
    ] {
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs[index] = syn::parse2(argument)?;
        assert!(expand_protocol_members_defined(TokenStream::new(), &quote!(#changed)).is_err());
    }
    let mut changed: syn::ItemFn = syn::parse2(original)?;
    changed.sig.output = syn::parse_quote!(-> Result<Type<'db>, E::Error>);
    assert!(expand_protocol_members_defined(TokenStream::new(), &quote!(#changed)).is_err());
    Ok(())
}

#[test]
fn protocol_preflight_rejects_undeclared_children_and_effect_escape() {
    for body in [
        quote! { effects.other(protocol).await },
        quote! { effects.protocol_interface(protocol) },
        quote! { let alias = effects; Ok(true) },
        quote! { let child = effects.protocol_interface(protocol); child.await },
        quote! { let deferred = || effects.protocol_interface(protocol); Ok(true) },
        quote! { let deferred = async { effects.protocol_interface(protocol).await }; Ok(true) },
        quote! { other(protocol).await },
        quote! { protocol_interface_build_with(class, effects).await },
    ] {
        assert!(
            expand_protocol_members_defined(TokenStream::new(), &protocol_members_defined(&body))
                .is_err(),
            "accepted {body}",
        );
    }
}

#[test]
fn type_analysis_lowering_preserves_its_three_signatures() -> Result<()> {
    {
        let original = quote! { async fn constraint_as_concrete_with<'db, E: ConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> { effects.checkpoint(Entry).await?; effects.search_bound(bound, search).await?; effects.materialize_bound(bound, kind).await } };
        let synchronous = quote! { fn constraint_as_concrete_sync<'db, E: crate::types::constraints::type_analysis::SyncConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> { effects.checkpoint(Entry)?; effects.search_bound(bound, search)?; effects.materialize_bound(bound, kind) } };
        assert_eq!(
            syn::parse2::<syn::File>(expand_constraint_type(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(quote!(#original #synchronous))?
        );
        assert!(expand_constraint_type(quote!(option), &original).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &E));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.generics.params.pop();
        changed
            .sig
            .generics
            .params
            .push(syn::parse_quote!(E: OtherEffects<'db>));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
    }
    {
        let original = quote! { async fn constraint_bound_depth_with<'db, E: ConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<(u16, u16), E::Error> { effects.checkpoint(Entry).await?; effects.type_depth(bound).await } };
        let synchronous = quote! { fn constraint_bound_depth_sync<'db, E: crate::types::constraints::type_analysis::SyncConstraintTypeEffects<'db>>(constraint: Constraint<'db>, effects: &mut E) -> Result<(u16, u16), E::Error> { effects.checkpoint(Entry)?; effects.type_depth(bound) } };
        assert_eq!(
            syn::parse2::<syn::File>(expand_constraint_type(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(quote!(#original #synchronous))?
        );
        assert!(expand_constraint_type(quote!(option), &original).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &E));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.generics.params.pop();
        changed
            .sig
            .generics
            .params
            .push(syn::parse_quote!(E: OtherEffects<'db>));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
    }
    {
        let original = quote! { async fn cached_constraint_bound_depth_with<'db, E: ConstraintDepthCacheEffects<'db>>(id: ConstraintId, effects: &mut E) -> Result<(u16, u16), E::Error> { effects.depth_cache_get(id).await?; let constraint = effects.depth_constraint(id).await?; let depth = constraint_bound_depth_with(constraint, effects).await?; effects.depth_cache_publish(id, depth).await } };
        let synchronous = quote! { fn cached_constraint_bound_depth_sync<'db, E: crate::types::constraints::type_analysis::SyncConstraintDepthCacheEffects<'db>>(id: ConstraintId, effects: &mut E) -> Result<(u16, u16), E::Error> { effects.depth_cache_get(id)?; let constraint = effects.depth_constraint(id)?; let depth = constraint_bound_depth_sync(constraint, effects)?; effects.depth_cache_publish(id, depth) } };
        assert_eq!(
            syn::parse2::<syn::File>(expand_constraint_type(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(quote!(#original #synchronous))?
        );
        assert!(expand_constraint_type(quote!(option), &original).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &E));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.generics.params.pop();
        changed
            .sig
            .generics
            .params
            .push(syn::parse_quote!(E: OtherEffects<'db>));
        assert!(expand_constraint_type(TokenStream::new(), &quote!(#changed)).is_err());
    }
    Ok(())
}

#[test]
fn sequent_rules_lower_only_declared_requests_and_ordered_helpers() -> Result<()> {
    let cases = [
        (
            quote! { async fn single_sequents_with<'db,E:SequentEffects<'db>>(constraint:Constraint<'db>,effects:&mut E)->Result<SequentMap<'db>,E::Error> {
                effects.checkpoint(Entry).await?;
                let mut map = SequentMap::default();
                constraint_sequents_with(constraint,&mut map,effects).await?;
                finish_sequents_with(&mut map,effects).await?;
                Ok(map)
            } },
            quote! { fn single_sequents_sync<'db,E:crate::types::constraints::sequents::effects::SyncSequentEffects<'db>>(constraint:Constraint<'db>,effects:&mut E)->Result<SequentMap<'db>,E::Error> {
                effects.checkpoint(Entry)?;
                let mut map = SequentMap::default();
                constraint_sequents_sync(constraint,&mut map,effects)?;
                finish_sequents_sync(&mut map,effects)?;
                Ok(map)
            } },
        ),
        (
            quote! { async fn constraint_pair_sequents_with<'db,E:SequentEffects<'db>>(left:Constraint<'db>,map:&mut SequentMap<'db>,right:Constraint<'db>,effects:&mut E)->Result<(),E::Error> {
                match (left,right) {
                    (Constraint::ConcreteUpper(other),Constraint::ConcreteLower(this)) => lower_pair_upper_with(this,map,other,true,effects).await?,
                    _ => {}
                }
                Ok(())
            } },
            quote! { fn constraint_pair_sequents_sync<'db,E:crate::types::constraints::sequents::effects::SyncSequentEffects<'db>>(left:Constraint<'db>,map:&mut SequentMap<'db>,right:Constraint<'db>,effects:&mut E)->Result<(),E::Error> {
                match (left,right) {
                    (Constraint::ConcreteUpper(other),Constraint::ConcreteLower(this)) => lower_pair_upper_sync(this,map,other,true,effects)?,
                    _ => {}
                }
                Ok(())
            } },
        ),
        (
            quote! { async fn add_sequents_for_range_with<'db,E:SequentEffects<'db>>(map:&mut SequentMap<'db>,lower:impl ProvidesConcreteLowerBound<'db>,upper:impl ProvidesConcreteUpperBound<'db>,effects:&mut E)->Result<(),E::Error> {
                let when = effects.owned_assignable(lower.bound(),upper.bound()).await?;
                add_constraint_set_implication_with(map,lower.into(),upper.into(),when.as_ref(),effects).await
            } },
            quote! { fn add_sequents_for_range_sync<'db,E:crate::types::constraints::sequents::effects::SyncSequentEffects<'db>>(map:&mut SequentMap<'db>,lower:impl ProvidesConcreteLowerBound<'db>,upper:impl ProvidesConcreteUpperBound<'db>,effects:&mut E)->Result<(),E::Error> {
                let when = effects.owned_assignable(lower.bound(),upper.bound())?;
                add_constraint_set_implication_sync(map,lower.into(),upper.into(),when.as_ref(),effects)
            } },
        ),
        (
            quote! { async fn lower_pair_upper_with<'db,E:SequentEffects<'db>>(this:ConcreteLowerBound<'db>,map:&mut SequentMap<'db>,other:ConcreteUpperBound<'db>,_reversed:bool,effects:&mut E)->Result<(),E::Error> {
                if effects.fields().is_paramspec(this.typevar) && effects.equivalent(effects.materialize(this.bound,Bottom).await?,other.bound).await? { return Ok(()); }
                add_sequents_for_range_with(map,this,other,effects).await
            } },
            quote! { fn lower_pair_upper_sync<'db,E:crate::types::constraints::sequents::effects::SyncSequentEffects<'db>>(this:ConcreteLowerBound<'db>,map:&mut SequentMap<'db>,other:ConcreteUpperBound<'db>,_reversed:bool,effects:&mut E)->Result<(),E::Error> {
                if effects.fields().is_paramspec(this.typevar) && effects.equivalent(effects.materialize(this.bound,Bottom)?,other.bound)? { return Ok(()); }
                add_sequents_for_range_sync(map,this,other,effects)
            } },
        ),
        (
            quote! { async fn derive_group_direction_with<'db,E:SequentEffects<'db>>(source:GroupedSequentSource<'db>,direction:TypeVarEquivalenceDirectedView<'db>,map:&mut SequentMap<'db>,effects:&mut E)->Result<(),E::Error> {
                match source { GroupedSequentSource::Lower(this) => {
                    add_covariant_lower_weakened_sequent_with(map,this,direction,effects).await?;
                    add_contravariant_lower_weakened_sequent_with(map,this,direction.reverse(),effects).await?;
                    add_invariant_weakened_sequent_with(map,this,direction.reverse(),effects).await?;
                }, _ => {} }
                Ok(())
            } },
            quote! { fn derive_group_direction_sync<'db,E:crate::types::constraints::sequents::effects::SyncSequentEffects<'db>>(source:GroupedSequentSource<'db>,direction:TypeVarEquivalenceDirectedView<'db>,map:&mut SequentMap<'db>,effects:&mut E)->Result<(),E::Error> {
                match source { GroupedSequentSource::Lower(this) => {
                    add_covariant_lower_weakened_sequent_sync(map,this,direction,effects)?;
                    add_contravariant_lower_weakened_sequent_sync(map,this,direction.reverse(),effects)?;
                    add_invariant_weakened_sequent_sync(map,this,direction.reverse(),effects)?;
                }, _ => {} }
                Ok(())
            } },
        ),
    ];
    for (original, synchronous) in cases {
        assert_eq!(
            syn::parse2::<syn::File>(super::expand_sequent(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(quote!(#original #synchronous))?
        );
        assert!(super::expand_sequent(quote!(option), &original).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects:&E));
        assert!(super::expand_sequent(TokenStream::new(), &quote!(#changed)).is_err());
    }
    Ok(())
}

#[test]
fn sequent_manifest_rejects_undeclared_effects_and_helper_edges() {
    for body in [
        quote!(effects.assignable(left, right).await),
        quote!(effects.checkpoint(Entry)),
        quote!(effects.fields().await),
        quote!(let alias=effects; Ok(map)),
        quote!(let callback=||effects.checkpoint(Entry); Ok(map)),
        quote!(async { effects.checkpoint(Entry).await }),
        quote!(pair_sequents_with(left, right, effects).await),
        quote!(constraint_sequents_with(constraint, effects).await),
        quote!(constraint_sequents_with(constraint, &mut map, effects)),
        quote!(super::constraint_sequents_with(constraint, &mut map, effects).await),
        quote!(let helper=constraint_sequents_with; Ok(map)),
        quote!(r#constraint_sequents_with(constraint, &mut map, effects).await),
    ] {
        let original = quote! { async fn single_sequents_with<'db,E:SequentEffects<'db>>(constraint:Constraint<'db>,effects:&mut E)->Result<SequentMap<'db>,E::Error> { #body } };
        assert!(
            super::expand_sequent(TokenStream::new(), &original).is_err(),
            "accepted {body}"
        );
    }
}

fn namespace_lookup(body: &TokenStream) -> TokenStream {
    quote! {
        async fn namespace_lookup_with<'a, 'db, E: NamespaceLookupEffects<'db>>(
            lookup_ty: Type<'db>, request: NamespaceLookupRequest<'a, 'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error> { #body }
    }
}

fn instance_mro(body: &TokenStream) -> TokenStream {
    quote! {
        async fn mro_instance_member_with<'a, 'db, C, E: InstanceMroEffects<'db, C>>(
            name: &'a str, mut cursor: C, effects: &E,
        ) -> Result<InstanceMemberResult<'db>, E::Error> { #body }
    }
}

#[test]
fn lookup_manifests_lower_exact_signatures_and_native_operations() -> Result<()> {
    let original = namespace_lookup(&quote! {
        let NamespaceLookupRequest { class, name, policy } = request;
        effects.checkpoint(NamespaceLookupWork::Begin).await?;
        let class_attr = effects.find_in_mro(lookup_ty, name, policy).await?
            .expect("The meta-type of an instance-like type should always have an MRO");
        Ok(class_attr)
    });
    let synchronous = quote! {
        fn namespace_lookup_sync<'a, 'db, E: crate::types::class::namespace::SynchronousNamespaceLookupEffects<'db>>(
            lookup_ty: Type<'db>, request: NamespaceLookupRequest<'a, 'db>, effects: &E,
        ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
            let NamespaceLookupRequest { class, name, policy } = request;
            effects.checkpoint(NamespaceLookupWork::Begin)?;
            let class_attr = effects.find_in_mro(lookup_ty, name, policy)?
                .expect("The meta-type of an instance-like type should always have an MRO");
            Ok(class_attr)
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(super::expand_namespace_lookup(
            TokenStream::new(),
            &original
        )?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?
    );
    let original = instance_mro(&quote! {
        effects.checkpoint(InstanceMroWork::Begin).await?;
        let mut union = effects.new_union().await?;
        let member = effects.own_instance_member(class, name).await?;
        let implicit = effects.implicit_attribute(class, name).await?;
        let absent = member.is_undefined() == implicit.member.is_undefined();
        let mut pending_augmented_bindings = Vec::new();
        effects.push_pending(&mut pending_augmented_bindings, class, bindings).await?;
        let count = pending_augmented_bindings.len();
        effects.infer_augmented(MroPendingBindings(&pending_augmented_bindings)).await?;
        effects.clear_pending(&mut pending_augmented_bindings).await?;
        effects.finish_pending(pending_augmented_bindings).await?;
        Ok(InstanceMemberResult::Done(member.inner))
    });
    let synchronous = quote! {
        fn mro_instance_member_sync<'a, 'db, C, E: crate::types::class::member_lookup::SynchronousInstanceMroEffects<'db, C>>(
            name: &'a str, mut cursor: C, effects: &E,
        ) -> Result<InstanceMemberResult<'db>, E::Error> {
            effects.checkpoint(InstanceMroWork::Begin)?;
            let mut union = effects.new_union()?;
            let member = effects.own_instance_member(class, name)?;
            let implicit = effects.implicit_attribute(class, name)?;
            let absent = member.is_undefined() == implicit.member.is_undefined();
            let mut pending_augmented_bindings = Vec::new();
            effects.push_pending(&mut pending_augmented_bindings, class, bindings)?;
            let count = pending_augmented_bindings.len();
            effects.infer_augmented(MroPendingBindings(&pending_augmented_bindings))?;
            effects.clear_pending(&mut pending_augmented_bindings)?;
            effects.finish_pending(pending_augmented_bindings)?;
            Ok(InstanceMemberResult::Done(member.inner))
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(super::expand_instance_mro(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?
    );
    Ok(())
}

#[test]
fn lookup_manifests_accept_the_complete_shared_source_bodies() -> Result<()> {
    for (source, name, expand) in [
        (
            include_str!("../../../ty_python_semantic/src/types/class/namespace.rs"),
            "namespace_lookup_with",
            super::expand_namespace_lookup as fn(TokenStream, &TokenStream) -> Result<TokenStream>,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class/member_lookup.rs"),
            "mro_instance_member_with",
            super::expand_instance_mro,
        ),
    ] {
        let parsed = syn::parse_file(source)?;
        let mut bodies = 0;
        for item in parsed.items {
            if let syn::Item::Fn(mut function) = item
                && function.sig.ident == name
            {
                function.attrs.clear();
                expand(TokenStream::new(), &quote!(#function))?;
                bodies += 1;
            }
        }
        assert_eq!(bodies, 1, "missing canonical {name}");
    }
    Ok(())
}

#[test]
fn lookup_manifests_reject_signature_changes_and_semantic_escapes() -> Result<()> {
    for (original, expand) in [
        (
            namespace_lookup(&quote!(Ok(member))),
            super::expand_namespace_lookup as fn(TokenStream, &TokenStream) -> Result<TokenStream>,
        ),
        (
            instance_mro(&quote!(Ok(InstanceMemberResult::TypedDict))),
            super::expand_instance_mro,
        ),
    ] {
        assert!(expand(quote!(option), &original).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &mut E));
        assert!(expand(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.generics.where_clause = Some(syn::parse_quote!(where E: Copy));
        assert!(expand(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.output = syn::parse_quote!(-> Result<Type<'db>, E::Error>);
        assert!(expand(TokenStream::new(), &quote!(#changed)).is_err());
    }
    for body in [
        quote!(Ok(name.contains("scan"))),
        quote!(Ok(name.is_undefined())),
        quote!(Ok(name.with_qualifiers(qualifiers))),
        quote!(Ok(name.into())),
        quote!(Ok(cursor.next())),
        quote!(Ok(ty.member(db, env, name))),
        quote!(effects.unknown().await),
        quote!(effects.own_member(request)),
        quote!(let escaped = effects; Ok(member)),
        quote!(let value = OwnMemberFacts::raw_member(effects, request); Ok(member)),
        quote!(let work = || effects.checkpoint(work).await; Ok(member)),
        quote!(let work = || name.len(); Ok(member)),
        quote!(let work = async { effects.checkpoint(work).await }; Ok(member)),
        quote!(let values = vec![]; Ok(member)),
        quote!(fn helper() {}, Ok(member)),
        quote!(for item in values { } Ok(member)),
    ] {
        assert!(
            super::expand_namespace_lookup(TokenStream::new(), &namespace_lookup(&body)).is_err(),
            "namespace accepted {body}"
        );
        assert!(
            super::expand_instance_mro(TokenStream::new(), &instance_mro(&body)).is_err(),
            "instance MRO accepted {body}"
        );
    }
    for body in [
        quote!(Ok(member.expect("arbitrary"))),
        quote!(Ok(effects
            .find_in_mro(lookup_ty, name, policy)
            .await?
            .expect("changed invariant"))),
        quote!(Ok(other
            .find_in_mro(lookup_ty, name, policy)
            .await?
            .expect(
                "The meta-type of an instance-like type should always have an MRO"
            ))),
    ] {
        assert!(
            super::expand_namespace_lookup(TokenStream::new(), &namespace_lookup(&body)).is_err()
        );
    }
    Ok(())
}

#[test]
fn member_source_manifests_accept_complete_shared_bodies() -> Result<()> {
    for (source, names, expand) in [
        (
            include_str!("../../../ty_python_semantic/src/types/class/member_source.rs"),
            &[
                "static_own_instance_member_with",
                "runtime_binding_absent_with",
            ][..],
            super::expand_member_source as fn(TokenStream, &TokenStream) -> Result<TokenStream>,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class/implicit_attributes.rs"),
            &["implicit_attribute_bindings_with"][..],
            super::expand_member_source,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class.rs"),
            &["static_code_generator_with"][..],
            super::expand_member_source,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class/own_member.rs"),
            &["own_class_member_with"][..],
            super::expand,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class/member_lookup.rs"),
            &["mro_class_member_with"][..],
            super::expand_mro,
        ),
        (
            include_str!("../../../ty_python_semantic/src/types/class/synthesized_member.rs"),
            &["own_synthesized_member_with"][..],
            super::expand_synthesized,
        ),
    ] {
        let parsed = syn::parse_file(source)?;
        let mut found = 0;
        for item in parsed.items {
            if let syn::Item::Fn(mut function) = item
                && names.iter().any(|name| function.sig.ident == *name)
            {
                function.attrs.clear();
                expand(TokenStream::new(), &quote!(#function))?;
                found += 1;
            }
        }
        assert_eq!(found, names.len());
    }
    Ok(())
}

#[test]
fn member_source_signatures_and_effects_lower_without_other_changes() -> Result<()> {
    for (signature, synchronous_signature, effects) in [
        (
            quote!(async fn static_own_instance_member_with<'a, 'db, E: StaticInstanceMemberEffects<'db>>( env: &ProgramEnvironment<'db>, class: StaticClassLiteral<'db>, name: &'a str, effects: &E) -> Result<Member<'db>, E::Error>),
            quote!(fn static_own_instance_member_sync<'a, 'db, E: crate::types::class::member_source::SynchronousStaticInstanceMemberEffects<'db>>( env: &ProgramEnvironment<'db>, class: StaticClassLiteral<'db>, name: &'a str, effects: &E) -> Result<Member<'db>, E::Error>),
            &[
                "checkpoint",
                "place_table",
                "use_def_map",
                "symbol_id",
                "binding_place",
                "body_scope",
                "code_generator",
                "has_own_named_tuple_field",
                "declaration_place",
                "imported_final",
                "implicit_member",
                "is_kw_only",
                "is_stub",
                "has_instance_slot",
                "is_own_dataclass_instance_field",
                "getter_member",
                "union_two",
            ][..],
        ),
        (
            quote!(async fn runtime_binding_absent_with<'a, 'db, E: MemberSourceEffects<'db>>(env: &ProgramEnvironment<'db>, scope: ScopeId<'db>, name: &'a str, effects: &E) -> Result<bool, E::Error>),
            quote!(fn runtime_binding_absent_sync<'a, 'db, E: crate::types::class::member_source::SynchronousMemberSourceEffects<'db>>(env: &ProgramEnvironment<'db>, scope: ScopeId<'db>, name: &'a str, effects: &E) -> Result<bool, E::Error>),
            &[
                "checkpoint",
                "place_table",
                "use_def_map",
                "symbol_id",
                "binding_place",
            ][..],
        ),
        (
            quote!(async fn implicit_attribute_bindings_with<'a, 'db, E: ImplicitAttributeEffects<'db>>( class: StaticClassLiteral<'db>, name: &'a str, target: MethodDecorator, effects: &E) -> Result<ImplicitAttribute<'db>, E::Error>),
            quote!(fn implicit_attribute_bindings_sync<'a, 'db, E: crate::types::class::member_source::SynchronousImplicitAttributeEffects<'db>>( class: StaticClassLiteral<'db>, name: &'a str, target: MethodDecorator, effects: &E) -> Result<ImplicitAttribute<'db>, E::Error>),
            &["body_scope", "checkpoint", "names", "find_name", "infer_named_attribute"][..],
        ),
        (
            quote!(async fn static_code_generator_with<'db, E: StaticCodeGeneratorEffects<'db>>(class: StaticClassLiteral<'db>, effects: &E) -> Result<Option<CodeGeneratorKind<'db>>, E::Error>),
            quote!(fn static_code_generator_sync<'db, E: crate::types::class::member_source::SynchronousStaticCodeGeneratorEffects<'db>>(class: StaticClassLiteral<'db>, effects: &E) -> Result<Option<CodeGeneratorKind<'db>>, E::Error>),
            &[
                "checkpoint",
                "dataclass_params",
                "known",
                "has_explicit_bases",
                "has_explicit_metaclass",
                "code_generator_query",
            ][..],
        ),
    ] {
        for effect in effects {
            let effect = Ident::new(effect, Span::call_site());
            let original = quote!(#signature { effects.#effect(value).await });
            let expected = quote!(#original #synchronous_signature { effects.#effect(value) });
            assert_eq!(
                syn::parse2::<syn::File>(super::expand_member_source(
                    TokenStream::new(),
                    &original
                )?)?,
                syn::parse2::<syn::File>(expected)?,
            );
            assert!(
                super::expand_member_source(
                    TokenStream::new(),
                    &quote!(#signature { effects.#effect(value) })
                )
                .is_err()
            );
        }
        let original = quote!(#signature { Ok(value) });
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.inputs.pop();
        changed.sig.inputs.push(syn::parse_quote!(effects: &mut E));
        assert!(super::expand_member_source(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original.clone())?;
        changed.sig.generics.where_clause = Some(syn::parse_quote!(where E: Copy));
        assert!(super::expand_member_source(TokenStream::new(), &quote!(#changed)).is_err());
        let mut changed: syn::ItemFn = syn::parse2(original)?;
        changed.sig.output = syn::parse_quote!(-> Result<usize, E::Error>);
        assert!(super::expand_member_source(TokenStream::new(), &quote!(#changed)).is_err());
        for body in [
            quote!(Ok(name.contains("hidden scan"))),
            quote!(Ok(name.into())),
            quote!(Ok(name.is_undefined())),
            quote!(Ok(name.with_qualifiers(qualifiers))),
            quote!(Ok(fields.body_scope(class))),
            quote!(Ok(class.own_instance_member(db, env, name))),
            quote!(Ok(db)),
            quote!(Ok(bindings.next())),
            quote!(Ok(place_table(db, scope))),
            quote!(effects.unknown().await),
            quote!(let owner = effects; Ok(value)),
            quote!(let run = || effects.checkpoint(value).await; Ok(value)),
            quote!(let run = async { effects.checkpoint(value).await }; Ok(value)),
            quote!(let values = Vec::new(); Ok(value)),
            quote!(let values = vec![]; Ok(value)),
            quote!(fn helper() {}, Ok(value)),
            quote!(for item in values {} Ok(value)),
            quote!(MemberSourceEffects::place_table(effects, scope).await),
        ] {
            assert!(
                super::expand_member_source(TokenStream::new(), &quote!(#signature { #body }))
                    .is_err(),
                "accepted {body}"
            );
        }
    }
    Ok(())
}

fn member_source_input(manifest: LookupManifest, body: &TokenStream) -> TokenStream {
    let signature = manifest.expected_signature();
    quote!(#signature { #body })
}

#[test]
fn member_source_helpers_reject_aliases_and_wrong_consumers() {
    for manifest in [
        LookupManifest::StaticInstance,
        LookupManifest::RuntimeBinding,
        LookupManifest::ImplicitAttribute,
        LookupManifest::StaticCodeGenerator,
    ] {
        for (setup, operation) in [
            (quote!(), quote!(name.contains("hidden scan"))),
            (
                quote!(let qualifiers = name;),
                quote!(qualifiers.contains("hidden scan")),
            ),
            (quote!(let symbol_id = name;), quote!(symbol_id.into())),
            (
                quote!(let symbol_id = name;),
                quote!({
                    let _: String = symbol_id.into();
                    false
                }),
            ),
            (quote!(), quote!(unapproved())),
            (quote!(), quote!(bindings.next())),
            (quote!(), quote!(db)),
            (quote!(), quote!(qualifiers.contains(name))),
            (quote!(), quote!(qualifiers.contains())),
            (
                quote!(),
                quote!(qualifiers.contains(TypeQualifiers::CLASS_VAR, TypeQualifiers::INIT_VAR)),
            ),
            (
                quote!(),
                quote!(qualifiers.contains::<str>(TypeQualifiers::CLASS_VAR)),
            ),
            (
                quote!(),
                quote!(other.end_of_scope_imported_final_candidates(symbol_id.into())),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates(other.into())),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates(symbol_id.into(), extra)),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates(symbol_id.into(extra))),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates::<T>(symbol_id.into())),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates(symbol_id.into::<T>())),
            ),
            (
                quote!(),
                quote!(use_def.end_of_scope_imported_final_candidates(symbol_id)),
            ),
        ] {
            for body in [
                quote!(#setup #operation; Ok(value)),
                quote!(#setup Ok(matches!(#operation, _))),
                quote!(#setup Ok(matches!(true, _ if { #operation; false }))),
            ] {
                assert!(
                    super::expand_member_source(
                        TokenStream::new(),
                        &member_source_input(manifest, &body)
                    )
                    .is_err(),
                    "{} accepted {body}",
                    manifest.body_name(),
                );
            }
        }
        if !matches!(manifest, LookupManifest::StaticInstance) {
            for operation in [
                quote!(qualifiers.contains(TypeQualifiers::CLASS_VAR)),
                quote!(qualifiers.contains(TypeQualifiers::INIT_VAR)),
                quote!(use_def.end_of_scope_imported_final_candidates(symbol_id.into())),
            ] {
                for body in [
                    quote!(#operation; Ok(value)),
                    quote!(Ok(matches!(#operation, _))),
                    quote!(Ok(matches!(true, _ if { #operation; false }))),
                ] {
                    assert!(
                        super::expand_member_source(
                            TokenStream::new(),
                            &member_source_input(manifest, &body)
                        )
                        .is_err(),
                        "{} accepted {body}",
                        manifest.body_name(),
                    );
                }
            }
        }
    }
}

#[test]
fn member_source_finite_helpers_and_checked_matches_keep_exact_lowering() -> Result<()> {
    let signature = LookupManifest::StaticInstance.expected_signature();
    let synchronous_signature = quote! {
        fn static_own_instance_member_sync<'a, 'db, E: crate::types::class::member_source::SynchronousStaticInstanceMemberEffects<'db>>( env: &ProgramEnvironment<'db>, class: StaticClassLiteral<'db>,
            name: &'a str, effects: &E,
        ) -> Result<Member<'db>, E::Error>
    };
    for operation in [
        quote!(qualifiers.contains(TypeQualifiers::CLASS_VAR)),
        quote!(qualifiers.contains(TypeQualifiers::INIT_VAR)),
        quote!(use_def.end_of_scope_imported_final_candidates(symbol_id.into())),
        quote!(matches!(value, |Some(_)| None,)),
        quote!(matches!(value, Some(inner) if inner == 0,)),
        quote!(matches!(
            qualifiers.contains(TypeQualifiers::CLASS_VAR),
            true
        )),
        quote!(matches!(
            qualifiers.contains(TypeQualifiers::INIT_VAR),
            true
        )),
        quote!(matches!(value, _ if qualifiers.contains(TypeQualifiers::CLASS_VAR))),
        quote!(matches!(value, _ if qualifiers.contains(TypeQualifiers::INIT_VAR))),
        quote!(matches!(
            use_def.end_of_scope_imported_final_candidates(symbol_id.into()),
            _
        )),
    ] {
        let original = quote!(#signature {
            let value = #operation;
            effects.checkpoint(work).await?;
            Ok(value)
        });
        let expected = quote!(#original #synchronous_signature {
            let value = #operation;
            effects.checkpoint(work)?;
            Ok(value)
        });
        assert_eq!(
            syn::parse2::<syn::File>(super::expand_member_source(TokenStream::new(), &original)?)?,
            syn::parse2::<syn::File>(expected)?,
        );
    }
    Ok(())
}

#[test]
fn lookup_checked_matches_reject_undeclared_helpers() {
    for operation in [
        quote!(name.contains("hidden scan")),
        quote!(unapproved()),
        quote!(bindings.next()),
        quote!(db),
        quote!(env),
    ] {
        for body in [
            quote!(Ok(matches!(#operation, _))),
            quote!(Ok(matches!(value, _ if { #operation; false }))),
        ] {
            assert!(
                super::expand_namespace_lookup(TokenStream::new(), &namespace_lookup(&body))
                    .is_err(),
                "namespace accepted {body}",
            );
            assert!(
                super::expand_instance_mro(TokenStream::new(), &instance_mro(&body)).is_err(),
                "instance MRO accepted {body}",
            );
        }
    }
}

fn storage_manifests() -> [LookupManifest; 14] {
    [
        LookupManifest::ClassStorage,
        LookupManifest::ClassOwnStorage,
        LookupManifest::StaticStorage,
        LookupManifest::TypedDictClassification,
        LookupManifest::OwnClassBinding,
        LookupManifest::GeneratedSlots,
        LookupManifest::NamedTupleSlots,
        LookupManifest::SlotNames,
        LookupManifest::InstanceSlot,
        LookupManifest::InstanceDictionary,
        LookupManifest::LacksInstanceStorage,
        LookupManifest::OwnSlotDescriptor,
        LookupManifest::SlotNameContains,
        LookupManifest::SlotNamedTupleBase,
    ]
}
fn expand_storage_body(manifest: LookupManifest, body: &TokenStream) -> Result<TokenStream> {
    let signature = manifest.expected_signature();
    super::expand_member(
        TokenStream::new(),
        &quote!(#signature { #body }),
        manifest.attribute(),
    )
}

#[test]
fn storage_manifests_keep_all_complete_bodies_and_capabilities() -> Result<()> {
    use syn::visit_mut::VisitMut;
    struct ExpectedBody;
    impl VisitMut for ExpectedBody {
        fn visit_expr_mut(&mut self, expression: &mut syn::Expr) {
            if let syn::Expr::Await(awaited) = expression {
                *expression = *awaited.base.clone();
            }
            syn::visit_mut::visit_expr_mut(self, expression);
        }
        fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
            if let syn::Expr::Path(path) = &mut *call.func
                && let Some(name) = path.path.get_ident()
                && let Some(manifest) = LookupManifest::storage_from_name(name, false)
            {
                path.path = syn::parse_str(manifest.synchronous_name()).expect("fixed helper name");
            }
            syn::visit_mut::visit_expr_call_mut(self, call);
        }
    }
    let mut count = 0;
    for source in [
        include_str!("../../../ty_python_semantic/src/types/class/instance_storage.rs"),
        include_str!("../../../ty_python_semantic/src/types/class/slots.rs"),
    ] {
        for item in syn::parse_file(source)?.items {
            let syn::Item::Fn(mut original) = item else {
                continue;
            };
            let Some(manifest) = LookupManifest::storage_from_name(&original.sig.ident, true)
                .or_else(|| LookupManifest::storage_from_name(&original.sig.ident, false))
            else {
                continue;
            };
            original.attrs.clear();
            let output =
                super::expand_member(TokenStream::new(), &quote!(#original), manifest.attribute())?;
            let mut expected = original.clone();
            expected.sig.asyncness = None;
            expected.sig.ident = Ident::new(manifest.synchronous_name(), Span::call_site());
            for param in expected.sig.generics.type_params_mut() {
                if param.ident == "E" {
                    let path = manifest.synchronous_bound(Span::call_site());
                    param.bounds = syn::parse_quote!(#path);
                }
            }
            ExpectedBody.visit_block_mut(&mut expected.block);
            assert_eq!(
                syn::parse2::<syn::File>(output)?,
                syn::parse2::<syn::File>(quote!(#original #expected))?,
                "{}",
                manifest.body_name()
            );
            assert_eq!(original.sig.inputs, expected.sig.inputs);
            count += 1;
        }
    }
    assert_eq!(count, 14);
    Ok(())
}

#[test]
fn storage_signatures_reject_extra_or_missing_capabilities() {
    for manifest in storage_manifests() {
        let mut signature = manifest.expected_signature();
        signature
            .inputs
            .insert(0, syn::parse_quote!(db: &'db dyn Db));
        assert!(
            super::expand_member(
                TokenStream::new(),
                &quote!(#signature { Ok(value) }),
                manifest.attribute()
            )
            .is_err()
        );
        let mut signature = manifest.expected_signature();
        signature.inputs.pop();
        assert!(
            super::expand_member(
                TokenStream::new(),
                &quote!(#signature { Ok(value) }),
                manifest.attribute()
            )
            .is_err()
        );
    }
    for manifest in [LookupManifest::StaticStorage, LookupManifest::InstanceSlot] {
        let mut signature = manifest.expected_signature();
        signature
            .inputs
            .insert(0, syn::parse_quote!(fields: MroFieldReads<'db>));
        assert!(
            super::expand_member(
                TokenStream::new(),
                &quote!(#signature { Ok(value) }),
                manifest.attribute()
            )
            .is_err()
        );
    }
}

#[test]
fn storage_effects_are_manifest_local_and_awaited() -> Result<()> {
    for manifest in storage_manifests() {
        for effect in manifest.effect_methods() {
            let effect = Ident::new(effect, Span::call_site());
            expand_storage_body(manifest, &quote!(effects.#effect(value).await?; Ok(value)))?;
            assert!(
                expand_storage_body(manifest, &quote!(effects.#effect(value)?; Ok(value))).is_err()
            );
            assert!(
                expand_storage_body(manifest, &quote!(other.#effect(value).await?; Ok(value)))
                    .is_err()
            );
        }
        for effect in [
            "binding_place",
            "union_build",
            "unknown_storage",
            "specialize_place",
            "source_python_version",
            "instance_layout",
            "instance_flags",
            "dynamic_own_instance_member",
        ] {
            if manifest.effect_methods().contains(&effect) {
                continue;
            }
            let effect = Ident::new(effect, Span::call_site());
            assert!(
                expand_storage_body(manifest, &quote!(effects.#effect(value).await?; Ok(value)))
                    .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn storage_helper_graph_has_exact_arity_await_and_rewrites() -> Result<()> {
    for (manifest, call, synchronous) in [
        (
            LookupManifest::GeneratedSlots,
            quote!(named_tuple_slots_with(class, effects)),
            quote!(named_tuple_slots_sync(class, effects)),
        ),
        (
            LookupManifest::NamedTupleSlots,
            quote!(slot_named_tuple_base_with(bases, effects)),
            quote!(slot_named_tuple_base_sync(bases, effects)),
        ),
        (
            LookupManifest::SlotNames,
            quote!(own_class_binding_with(class, "__slots__", effects)),
            quote!(own_class_binding_sync(class, "__slots__", effects)),
        ),
        (
            LookupManifest::SlotNames,
            quote!(generated_slots_with(class, effects)),
            quote!(generated_slots_sync(class, effects)),
        ),
        (
            LookupManifest::InstanceSlot,
            quote!(slot_name_contains_with(slots, name, effects)),
            quote!(slot_name_contains_sync(slots, name, effects)),
        ),
        (
            LookupManifest::InstanceDictionary,
            quote!(own_class_binding_with(class, "__slots__", effects)),
            quote!(own_class_binding_sync(class, "__slots__", effects)),
        ),
        (
            LookupManifest::InstanceDictionary,
            quote!(generated_slots_with(class, effects)),
            quote!(generated_slots_sync(class, effects)),
        ),
        (
            LookupManifest::LacksInstanceStorage,
            quote!(slot_names_with(class, effects)),
            quote!(slot_names_sync(class, effects)),
        ),
        (
            LookupManifest::LacksInstanceStorage,
            quote!(instance_slot_with(class, name, effects)),
            quote!(instance_slot_sync(class, name, effects)),
        ),
        (
            LookupManifest::LacksInstanceStorage,
            quote!(instance_dictionary_with(class, effects)),
            quote!(instance_dictionary_sync(class, effects)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(slot_names_with(class, effects)),
            quote!(slot_names_sync(class, effects)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(slot_name_contains_with(slots, name, effects)),
            quote!(slot_name_contains_sync(slots, name, effects)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(generated_slots_with(class, effects)),
            quote!(generated_slots_sync(class, effects)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(own_class_binding_with(class, name, effects)),
            quote!(own_class_binding_sync(class, name, effects)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(instance_slot_with(class, name, effects)),
            quote!(instance_slot_sync(class, name, effects)),
        ),
    ] {
        let output = syn::parse2::<syn::File>(expand_storage_body(
            manifest,
            &quote!(let value = #call.await?; Ok(value)),
        )?)?;
        let syn::Item::Fn(sync) = &output.items[1] else {
            panic!("missing synchronous body");
        };
        assert_eq!(
            sync.block,
            syn::parse_quote!({ let value = #synchronous?; Ok(value) })
        );
        assert!(expand_storage_body(manifest, &quote!(let value = #call?; Ok(value))).is_err());
        let mut wrong: syn::ExprCall = syn::parse2(call.clone())?;
        wrong.args.pop();
        assert!(
            expand_storage_body(manifest, &quote!(let value = #wrong.await?; Ok(value))).is_err()
        );
        let mut qualified: syn::ExprCall = syn::parse2(call.clone())?;
        let syn::Expr::Path(path) = &mut *qualified.func else {
            panic!("fixed helper call");
        };
        let name = path.path.clone();
        path.path = syn::parse_quote!(module::#name);
        assert!(expand_storage_body(manifest, &quote!(#qualified.await?; Ok(value))).is_err());
        assert!(
            expand_storage_body(
                manifest,
                &quote!(let alias = #name; alias().await?; Ok(value))
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn storage_rejects_hidden_operations_shadowing_and_unknown_work() {
    for manifest in storage_manifests() {
        for operation in [
            quote!(db),
            quote!(fields),
            quote!(&fields),
            quote!(name.contains("scan")),
            quote!(bindings.next()),
            quote!(names.get(0)),
            quote!(Vec::new()),
            quote!(Box::new(value)),
            quote!(ordinary()),
            quote!(other.into()),
            quote!(Place::Undefined.into(value)),
            quote!(effects.unknown().await?),
            quote!({
                let effects = other;
                true
            }),
            quote!({
                let fields = other;
                true
            }),
            quote!({
                let (fields, value) = pair;
                true
            }),
            quote!({
                let Name = other;
                true
            }),
            quote!({
                let str = other;
                true
            }),
            quote!({
                let ClassInstanceFlags = other;
                true
            }),
            quote!({
                let SlotSelectorWork = other;
                true
            }),
            quote!({
                let slot_name_equal = other;
                true
            }),
            quote!({
                fn slot_name_equal() {}
                true
            }),
            quote!({
                use module::slot_name_equal;
                true
            }),
            quote!({
                type Name = Other;
                true
            }),
            quote!(|value| value),
            quote!(async { ordinary() }),
            quote!(hidden!()),
            quote!(InstanceStorageWork::Unknown),
            quote!(SlotSelectorWork::Unknown),
            quote!(SlotSelectorWork::Unknown { bytes: 4 }),
            quote!(ClassInstanceFlags::contains(&flags, OtherFlags::TYPED_DICT)),
            quote!(OtherFlags::contains(&flags, ClassInstanceFlags::TYPED_DICT)),
            quote!(DataclassFlags::contains(&flags, OtherFlags::SLOTS)),
            quote!(fields.alias_origin(class)),
            quote!(fields.alias_specialization(class)),
            quote!(fields.unknown(class)),
            quote!(fields.body_scope(class, extra)),
        ] {
            for body in [
                quote!(#operation; Ok(value)),
                quote!(Ok(matches!(#operation, _))),
                quote!(Ok(matches!(value, _ if { #operation; true }))),
                quote!(Ok(Some(#operation))),
            ] {
                assert!(
                    expand_storage_body(manifest, &body).is_err(),
                    "{} accepted {body}",
                    manifest.body_name()
                );
            }
        }
    }
}

#[test]
fn storage_finite_native_calls_are_consumer_local() -> Result<()> {
    for (manifest, operation) in [
        (
            LookupManifest::ClassStorage,
            quote!(PlaceAndQualifiers::default()),
        ),
        (LookupManifest::ClassOwnStorage, quote!(Member::default())),
        (
            LookupManifest::TypedDictClassification,
            quote!(ClassInstanceFlags::contains(
                &flags,
                ClassInstanceFlags::TYPED_DICT
            )),
        ),
        (
            LookupManifest::OwnClassBinding,
            quote!(slot_bindings(use_def, symbol)),
        ),
        (
            LookupManifest::GeneratedSlots,
            quote!(DataclassFlags::contains(&flags, DataclassFlags::SLOTS)),
        ),
        (
            LookupManifest::SlotNames,
            quote!(slot_definition_names(definition)),
        ),
        (
            LookupManifest::InstanceSlot,
            quote!(slot_layout_names(layout)),
        ),
        (
            LookupManifest::InstanceDictionary,
            quote!(slot_layout_has_dictionary(layout)),
        ),
        (
            LookupManifest::OwnSlotDescriptor,
            quote!(slot_is_dictionary_name(name)),
        ),
        (
            LookupManifest::SlotNameContains,
            quote!(slot_name_equal(candidate, name)),
        ),
        (
            LookupManifest::SlotNamedTupleBase,
            quote!(slot_is_named_tuple_base(base)),
        ),
    ] {
        expand_storage_body(manifest, &quote!(let result = #operation; Ok(result)))?;
        expand_storage_body(manifest, &quote!(Ok(matches!(#operation, _))))?;
        let other = if matches!(manifest, LookupManifest::StaticStorage) {
            LookupManifest::SlotNames
        } else {
            LookupManifest::StaticStorage
        };
        assert!(expand_storage_body(other, &quote!(#operation; Ok(value))).is_err());
    }
    Ok(())
}

#[test]
fn storage_iteration_and_native_helper_arguments_remain_closed() {
    for manifest in storage_manifests() {
        for body in [
            quote!(for value in values { effects.slot_checkpoint(value).await?; } Ok(value)),
            quote!(while flag { flag = false; } Ok(value)),
            quote!(slot_bindings(use_def); Ok(value)),
            quote!(slot_name_equal(candidate, name, extra); Ok(value)),
            quote!(Type::inline_payload_bytes(base, extra); Ok(value)),
            quote!(effects.slot_checkpoint(SlotSelectorWork::NameCompare { candidate_bytes: ordinary(), requested_bytes: 0 }).await?; Ok(value)),
        ] {
            assert!(
                expand_storage_body(manifest, &body).is_err(),
                "{} accepted {body}",
                manifest.body_name()
            );
        }
        if !matches!(
            manifest,
            LookupManifest::OwnClassBinding
                | LookupManifest::SlotNameContains
                | LookupManifest::SlotNamedTupleBase
        ) {
            assert!(expand_storage_body(manifest, &quote!(loop { break; } Ok(value))).is_err());
        }
    }
}

#[test]
fn storage_none_variant_patterns_preserve_exact_lowering() -> Result<()> {
    for (manifest, body, synchronous_body) in [
        (
            LookupManifest::OwnClassBinding,
            quote! {
                match effects.next_binding_has_definition(&mut bindings).await? {
                    None => {
                        effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
                        Ok(false)
                    }
                    Some(value) => Ok(value),
                }
            },
            quote! {
                match effects.next_binding_has_definition(&mut bindings)? {
                    None => {
                        effects.slot_checkpoint(SlotSelectorWork::Publish)?;
                        Ok(false)
                    }
                    Some(value) => Ok(value),
                }
            },
        ),
        (
            LookupManifest::InstanceDictionary,
            quote! {
                if let None = effects.known(class).await? {
                    effects.slot_checkpoint(SlotSelectorWork::Publish).await?;
                    return Ok(true);
                }
                Ok(false)
            },
            quote! {
                if let None = effects.known(class)? {
                    effects.slot_checkpoint(SlotSelectorWork::Publish)?;
                    return Ok(true);
                }
                Ok(false)
            },
        ),
        (
            LookupManifest::OwnClassBinding,
            quote!(Ok(match value {
                None => false,
                Some(_) => true,
            })),
            quote!(Ok(match value {
                None => false,
                Some(_) => true,
            })),
        ),
        (
            LookupManifest::InstanceDictionary,
            quote!(let None = value else { return Ok(false); }; Ok(true)),
            quote!(let None = value else { return Ok(false); }; Ok(true)),
        ),
        (
            LookupManifest::OwnClassBinding,
            quote!(Ok(matches!(value, None))),
            quote!(Ok(matches!(value, None))),
        ),
    ] {
        let signature = manifest.expected_signature();
        let synchronous_signature = if matches!(manifest, LookupManifest::OwnClassBinding) {
            quote! {
                fn own_class_binding_sync<'a, 'db, E: crate::types::class::SynchronousSlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, name: &'a str, effects: &E,
                ) -> Result<bool, E::Error>
            }
        } else {
            quote! {
                fn instance_dictionary_sync<'db, E: crate::types::class::SynchronousSlotSelectorEffects<'db>>(
                    class: StaticClassLiteral<'db>, effects: &E,
                ) -> Result<bool, E::Error>
            }
        };
        assert_eq!(
            syn::parse2::<syn::File>(expand_storage_body(manifest, &body)?)?,
            syn::parse2::<syn::File>(quote! {
                #signature { #body }
                #synchronous_signature { #synchronous_body }
            })?,
        );
    }
    Ok(())
}

#[test]
fn storage_none_exception_keeps_binding_and_guard_validation() {
    for manifest in storage_manifests() {
        for pattern in [
            quote!(mut None),
            quote!(ref None),
            quote!(ref mut None),
            quote!(None @ _),
            quote!(r#None),
            quote!(None @ fields),
            quote!(value @ fields),
            quote!(Some(fields)),
            quote!(Some),
            quote!(Ok),
            quote!(fields),
            quote!(effects),
            quote!(Name),
            quote!(ClassInstanceFlags),
            quote!(slot_name_equal),
            quote!(slot_names_with),
        ] {
            for body in [
                quote!(let #pattern = value; Ok(false)),
                quote!(Ok(matches!(value, #pattern))),
            ] {
                assert!(
                    expand_storage_body(manifest, &body).is_err(),
                    "{} accepted {body}",
                    manifest.body_name(),
                );
            }
        }
        for operation in [
            quote!(ordinary()),
            quote!(bindings.next()),
            quote!(name.contains("hidden scan")),
            quote!({
                let fields = other;
                true
            }),
        ] {
            let body = quote!(Ok(matches!(value, None if { #operation; true })));
            assert!(
                expand_storage_body(manifest, &body).is_err(),
                "{} accepted {body}",
                manifest.body_name(),
            );
        }
    }
}
