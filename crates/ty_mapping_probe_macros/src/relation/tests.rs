use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::Result;

use super::expand;

fn original(body: &TokenStream) -> TokenStream {
    quote! {
        async fn check_type_pair_inner_with<E: PairEffects<'a, 'c, 'db>>(
            &self, source: Type<'db>, target: Type<'db>, effects: &E,
        ) -> Result<ConstraintSet<'db, 'c>, E::Error> { #body }
    }
}

fn lowers(body: &TokenStream, expected: &TokenStream) -> Result<()> {
    let original = original(body);
    let synchronous = quote! {
        fn check_type_pair_inner(
            &self, db: &'db dyn Db, source: Type<'db>, target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> {
            let fields = RelationFieldReads::new(db);
            #expected
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

fn lowers_disjointness(body: &TokenStream, expected: &TokenStream) -> Result<()> {
    let original = quote! {
        async fn check_type_pair_with<E: disjointness_effects::DisjointnessEffects<'a, 'c, 'db>>(
            &self, fields: RelationFieldReads<'db>, left: Type<'db>, right: Type<'db>, effects: &E,
        ) -> Result<ConstraintSet<'db, 'c>, E::Error> { #body }
    };
    let synchronous = quote! {
        fn check_type_pair(
            &self, db: &'db dyn Db, left: Type<'db>, right: Type<'db>,
        ) -> ConstraintSet<'db, 'c> {
            let fields = RelationFieldReads::new(db);
            #expected
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #synchronous))?,
    );
    Ok(())
}

#[test]
fn stored_field_getters_lower_to_private_readers() -> Result<()> {
    for name in [
        "type_form_argument",
        "field_default",
        "field_converter",
        "method_wrapper_kind",
        "method_wrapper_type",
        "partial_wrapped",
        "partial_callable",
        "interned_type",
        "type_is_argument",
        "type_guard_return",
    ] {
        let method = Ident::new(name, Span::call_site());
        lowers(
            &quote!(Ok(effects.#method(value()).await?)),
            &quote!(fields.#method(value())),
        )?;
    }
    Ok(())
}

#[test]
fn stored_field_manifest_checks_arity_and_provider() {
    for body in [
        quote!(Ok(effects.type_form_argument().await?)),
        quote!(Ok(effects.type_form_argument(self, source).await?)),
        quote!(Ok(other.type_form_argument(source).await?)),
        quote!(Ok(effects.type_form_argument::<Type>(source).await?)),
        quote!(Ok(effects.unlisted_field(source).await?)),
    ] {
        assert!(expand(TokenStream::new(), &original(&body)).is_err());
    }
    let input = quote! {
        async fn pair_with<E: DisjointnessEffects<'a, 'c, 'db>>(
            &self, fields: RelationFieldReads<'db>, source: Type<'db>, effects: &E,
        ) -> Result<Type<'db>, E::Error> {
            effects.type_form_argument(source).await
        }
    };
    assert!(expand(TokenStream::new(), &input).is_err());
}

#[test]
fn disjointness_still_requires_its_field_reader() {
    let input = quote! {
        async fn pair_with<E: DisjointnessEffects<'a, 'c, 'db>>(
            &self, source: Type<'db>, target: Type<'db>, effects: &E,
        ) -> Result<bool, E::Error> {
            Ok(true)
        }
    };
    assert_eq!(
        expand(TokenStream::new(), &input).unwrap_err().to_string(),
        "dual_relation requires fields: RelationFieldReads<'db> before its operands",
    );
}

#[test]
fn stored_wrapper_reads_preserve_guard_boundaries() -> Result<()> {
    lowers(
        &quote! {
            Ok(match (source, target) {
                (source, target) if effects.method_wrapper_kind(source).await?
                    == effects.method_wrapper_kind(target).await? =>
                {
                    effects.guard(self, source, target, || async {
                        effects.check_type_pair(
                            self,
                            effects.method_wrapper_type(source).await?,
                            effects.method_wrapper_type(target).await?,
                        ).await
                    }).await?
                }
                _ => self.never(),
            })
        },
        &quote! {
            match (source, target) {
                (source, target) if fields.method_wrapper_kind(source)
                    == fields.method_wrapper_kind(target) =>
                {
                    (self).with_recursion_guard(db, source, target, || {
                        (self).check_type_pair(
                            db,
                            fields.method_wrapper_type(source),
                            fields.method_wrapper_type(target)
                        )
                    })
                }
                _ => self.never(),
            }
        },
    )
}

#[test]
fn stored_default_and_converter_reads_preserve_short_circuiting() -> Result<()> {
    lowers(
        &quote! {
            (effects.field_default(field).await?).when_none_or_with(
                self.constraints,
                |default| async { effects.check_type_pair(self, default, target).await },
                effects,
            ).await?.and_with(self.constraints, || async {
                (effects.field_converter(field).await?).map(|(_, output)| output).when_none_or_with(
                    self.constraints,
                    |output| async { effects.check_type_pair(self, output, target).await },
                    effects,
                ).await
            }, effects).await
        },
        &quote! {
            (fields.field_default(field)).when_none_or(
                db,
                self.constraints,
                |default| { (self).check_type_pair(db, default, target) }
            ).and(db, self.constraints, || {
                (fields.field_converter(field)).map(|(_, output)| output).when_none_or(
                    db,
                    self.constraints,
                    |output| { (self).check_type_pair(db, output, target) }
                )
            })
        },
    )
}

#[test]
fn stored_partial_reads_preserve_source_target_and_lazy_callable_order() -> Result<()> {
    lowers(
        &quote! {
            effects.check_type_pair(
                self,
                effects.interned_type(effects.partial_wrapped(source).await?).await?,
                effects.interned_type(effects.partial_wrapped(target).await?).await?,
            ).await?.and_with(self.constraints, || async {
                effects.check_callable_pair(
                    self,
                    effects.partial_callable(source).await?,
                    effects.partial_callable(target).await?,
                ).await
            }, effects).await
        },
        &quote! {
            (self).check_type_pair(
                db,
                fields.interned_type(fields.partial_wrapped(source)),
                fields.interned_type(fields.partial_wrapped(target))
            ).and(db, self.constraints, || {
                (self).check_callable_pair(
                    db,
                    fields.partial_callable(source),
                    fields.partial_callable(target)
                )
            })
        },
    )
}

#[test]
fn disjointness_context_lifecycle_retains_completion_condition() -> Result<()> {
    lowers_disjointness(
        &quote! {
            effects.disjointness_clear_context(self).await?;
            let result = effects.check_type_pair_impl(self, left, right).await?;
            if effects.disjointness_has_context(self).await?
                && !effects.is_always_satisfied(self, result).await?
            {
                effects.disjointness_clear_context(self).await?;
            }
            Ok(result)
        },
        &quote! {
            (self).disjointness_clear_context(db,);
            let result = (self).check_type_pair_impl(db, left, right);
            if (self).disjointness_has_context(db,)
                && !(result).is_always_satisfied(db, (self).env)
            {
                (self).disjointness_clear_context(db,);
            }
            result
        },
    )
}

#[test]
fn disjointness_guard_reads_precede_expensive_checks() -> Result<()> {
    lowers_disjointness(
        &quote! {
            Ok(match (left, right) {
                (Type::Callable(callable), other)
                    if let Some(class) = effects.disjointness_callable_runtime_class(self, callable).await? =>
                {
                    if self.perform_expensive_checks {
                        effects.disjointness_callable_other(self, class, other).await?
                    } else {
                        effects.disjointness_boolean(self, false).await?
                    }
                }
                _ => effects.disjointness_boolean(self, false).await?,
            })
        },
        &quote! {
            match (left, right) {
                (Type::Callable(callable), other)
                    if let Some(class) = (self).disjointness_callable_runtime_class(db, callable) =>
                {
                    if self.perform_expensive_checks {
                        (self).disjointness_callable_other(db, class, other)
                    } else {
                        (self).disjointness_boolean(db, false)
                    }
                }
                _ => (self).disjointness_boolean(db, false),
            }
        },
    )
}

#[test]
fn disjointness_lazy_alternative_retains_expensive_gate() -> Result<()> {
    lowers_disjointness(
        &quote! {
            effects.or(self, initial, || async {
                Ok(if self.perform_expensive_checks {
                    effects.disjointness_alias_specializations(self, left_alias, right_alias).await?
                } else {
                    effects.disjointness_boolean(self, false).await?
                })
            }).await
        },
        &quote! {
            (initial).or(db, (self).constraints, || {
                if self.perform_expensive_checks {
                    (self).disjointness_alias_specializations(db, left_alias, right_alias)
                } else {
                    (self).disjointness_boolean(db, false)
                }
            })
        },
    )
}

#[test]
fn disjointness_manifest_is_separate_and_checks_arity() {
    for body in [
        quote!(effects.check_source_union(self, source, target).await),
        quote!(effects.disjointness_union(self, source).await),
        quote!(effects.disjointness_boolean(self, true, false).await),
    ] {
        let original = quote! {
            async fn pair_with<E: DisjointnessEffects<'a, 'c, 'db>>(
                &self, fields: RelationFieldReads<'db>, effects: &E,
            ) -> Result<ConstraintSet<'db, 'c>, E::Error> { #body }
        };
        assert!(expand(TokenStream::new(), &original).is_err());
    }
    assert!(
        expand(
            TokenStream::new(),
            &original(&quote!(
                effects.disjointness_union(self, source, target).await
            )),
        )
        .is_err()
    );
}

#[test]
fn child_effect_manifest_preserves_checker_and_operands() -> Result<()> {
    for name in [
        "check_type_pair",
        "check_typevar_subclass_relation_to_target",
        "check_newtype_pair",
        "check_source_union",
        "check_target_union",
        "check_target_intersection",
        "check_source_intersection",
        "check_source_typevar_bounds",
        "check_function_pair",
        "check_bound_method_pair",
        "check_known_bound_method_pair",
        "check_callable_pair",
        "check_callable_signature_pair",
        "check_callable_source",
        "check_type_satisfies_protocol",
        "check_meta_type_satisfies_protocol",
        "check_typeddict_pair",
        "check_class_pair",
        "check_subclassof_pair",
        "check_nominal_instance_pair",
        "check_property_instance_pair",
        "check_bound_super_pair",
        "check_typeddict_fallback",
        "when_recursive_types_relate_by_arguments",
    ] {
        let method = Ident::new(name, Span::call_site());
        lowers(
            &quote!(Ok(effects.#method(&self.as_equivalence_checker(), source, target).await?)),
            &quote!((&self.as_equivalence_checker()).#method(db, source, target)),
        )?;
    }
    Ok(())
}

#[test]
fn preparation_manifest_preserves_checker_and_operands() -> Result<()> {
    for name in [
        "protocol_is_equivalent_to_object",
        "unfold_recursive",
        "alias_value",
        "union_has_aliases",
        "expand_union_aliases",
        "subclass_instance",
        "class_default_specialization",
        "class_instance",
        "known_instance_type_form_argument",
        "special_form_type_form_argument",
        "enum_remaining_literals",
        "enum_intersection",
        "lookup_wrapped_function",
        "specialize_partial_instance",
        "union_contains_dynamic",
        "intersection_contains_nondivergent_dynamic",
        "intersection_contains_dynamic",
        "instance_approximation",
        "typevar_is_inferable",
        "typevar_is_typevartuple",
        "is_exact_tuple_instance",
        "is_variadic_exact_tuple_instance",
        "unpacked_typevartuple",
        "typevar_domain",
        "callable_is_gradual_paramspec_value",
        "callable_is_top_paramspec_value",
        "callable_is_bottom_paramspec_value",
        "typevar_constraints",
        "typevar_upper_bound",
        "typevar_bound_or_constraints",
        "newtype_concrete_base",
        "type_is_always_falsy",
        "type_is_always_truthy",
        "callable_signatures",
        "function_callable_signatures",
        "known_class_instance",
        "literal_fallback_instance",
        "callable_runtime_class",
        "subclass_inner_class",
        "class_literal_metaclass_instance",
        "class_metaclass_instance",
        "subclass_metaclass_instance",
        "special_form_instance_fallback",
        "known_instance_fallback",
        "property_instance_fallback",
    ] {
        let method = Ident::new(name, Span::call_site());
        lowers(
            &quote!(Ok(effects.#method(checker(), operand()).await?)),
            &quote!((checker()).#method(db, operand())),
        )?;
    }
    for name in [
        "implied_typevar_relation",
        "lazy_typevar_upper_constraint",
        "lazy_typevar_lower_constraint",
        "same_typevar_occurrence",
        "nominal_has_known_class",
        "same_sentinel",
        "wrapper_matches_nominal",
        "nominal_class_is_known",
        "union_contains_type",
        "intersection_positive_contains",
        "intersection_negative_contains",
        "check_string_literal_nominal",
        "check_bytes_literal_nominal",
        "check_enum_instance_literal",
    ] {
        let method = Ident::new(name, Span::call_site());
        lowers(
            &quote!(Ok(effects.#method(checker(), source(), target()).await?)),
            &quote!((checker()).#method(db, source(), target())),
        )?;
    }
    Ok(())
}

#[test]
fn object_equivalence_guard_stays_before_terminal_arms() -> Result<()> {
    lowers(
        &quote! {
            Ok(match (source, target) {
                (_, Type::ProtocolInstance(protocol))
                    if effects.protocol_is_equivalent_to_object(self, protocol).await? =>
                {
                    self.always()
                }
                (Type::Never, _) => self.always(),
                (Type::Dynamic(_), _) => self.never(),
                (Type::TypeForm(form), _) => {
                    let argument = effects.type_form_argument(form).await?;
                    effects.check_type_pair(self, argument, target).await?
                }
                _ => self.never(),
            })
        },
        &quote! {
            match (source, target) {
                (_, Type::ProtocolInstance(protocol))
                    if (self).protocol_is_equivalent_to_object(db, protocol) =>
                {
                    self.always()
                }
                (Type::Never, _) => self.always(),
                (Type::Dynamic(_), _) => self.never(),
                (Type::TypeForm(form), _) => {
                    let argument = fields.type_form_argument(form);
                    (self).check_type_pair(db, argument, target)
                }
                _ => self.never(),
            }
        },
    )
}

#[test]
fn borrowed_preparations_keep_database_lifetimes_and_order() -> Result<()> {
    lowers(
        &quote! {
            let bounds: Option<&'db [Type<'db>]> =
                effects.typevar_constraints(self, typevar).await?;
            let source: &'db CallableSignature<'db> =
                effects.callable_signatures(self, callable).await?;
            let target: &'db CallableSignature<'db> =
                effects.function_callable_signatures(self, function).await?;
            Ok(effects.check_callable_signature_pair(self, source, target).await?)
        },
        &quote! {
            let bounds: Option<&'db [Type<'db>]> = (self).typevar_constraints(db, typevar);
            let source: &'db CallableSignature<'db> = (self).callable_signatures(db, callable);
            let target: &'db CallableSignature<'db> =
                (self).function_callable_signatures(db, function);
            (self).check_callable_signature_pair(db, source, target)
        },
    )
}

#[test]
fn nested_preparations_stay_in_their_lazy_child_arguments() -> Result<()> {
    lowers(
        &quote! {
            Ok(effects.guard(self, source, target, || async {
                effects.check_type_pair(
                    self,
                    effects.class_instance(
                        self,
                        effects.class_default_specialization(self, source_class).await?,
                    ).await?,
                    target,
                ).await
            }).await?)
        },
        &quote! {
            (self).with_recursion_guard(db, source, target, || {
                (self).check_type_pair(
                    db,
                    (self).class_instance(db, (self).class_default_specialization(db, source_class)),
                    target
                )
            })
        },
    )
}

#[test]
fn guard_and_result_branches_preserve_lazy_return_scope() -> Result<()> {
    lowers(
        &quote! {
            if source == target { return Ok(self.always()); }
            Ok(effects.guard(self, source, target, || async {
                if source.is_never() { return Ok(self.never()); }
                Ok(effects.check_type_pair(self, target, source).await?)
            }).await?)
        },
        &quote! {
            if source == target { return self.always(); }
            (self).with_recursion_guard(db, source, target, || {
                if source.is_never() { return self.never(); }
                (self).check_type_pair(db, target, source)
            })
        },
    )
}

#[test]
fn effect_combinators_lower_callbacks_in_the_original_order() -> Result<()> {
    for name in [
        "and",
        "or",
        "when_some_and",
        "when_none_or",
        "when_all",
        "when_any",
    ] {
        let method = Ident::new(name, Span::call_site());
        let parameter = if name == "and" || name == "or" {
            quote!()
        } else {
            quote!(element)
        };
        lowers(
            &quote!(Ok(effects.#method(checker(), values(), |#parameter| async {
                before();
                Ok(effects.check_type_pair(self, source, target).await?)
            }).await?)),
            &quote!((values()).#method(db, (checker()).constraints, |#parameter| {
                before();
                (self).check_type_pair(db, source, target)
            })),
        )?;
    }
    Ok(())
}

#[test]
fn extension_combinators_lower_their_declared_async_callback() -> Result<()> {
    for name in [
        "and",
        "or",
        "when_some_and",
        "when_none_or",
        "when_all",
        "when_any",
    ] {
        let synchronous = Ident::new(name, Span::call_site());
        let asynchronous = Ident::new(&format!("{name}_with"), Span::call_site());
        let parameter = if name == "and" || name == "or" {
            quote!()
        } else {
            quote!(element)
        };
        lowers(
            &quote!(Ok(values().#asynchronous(self.constraints, |#parameter| async {
                Ok(effects.check_type_pair(self, source, target).await?)
            }, effects).await?)),
            &quote!(values().#synchronous(db, self.constraints, |#parameter| {
                (self).check_type_pair(db, source, target)
            })),
        )?;
    }
    Ok(())
}

#[test]
fn async_move_callbacks_keep_by_value_capture() -> Result<()> {
    lowers(
        &quote! {
            Ok(values.when_all_with(self.constraints, |element| async move {
                if element.is_never() { return Ok(self.always()); }
                Ok(effects.check_type_pair(self, element, target).await?)
            }, effects).await?)
        },
        &quote! {
            values.when_all(db, self.constraints, move |element| {
                if element.is_never() { return self.always(); }
                (self).check_type_pair(db, element, target)
            })
        },
    )?;
    lowers(
        &quote! {
            Ok(effects.guard(self, source, target, move || async move {
                consume(owned);
                Ok(effects.check_type_pair(self, source, target).await?)
            }).await?)
        },
        &quote! {
            (self).with_recursion_guard(db, source, target, move || {
                consume(owned);
                (self).check_type_pair(db, source, target)
            })
        },
    )
}

#[test]
fn satisfaction_helpers_use_the_original_environment() -> Result<()> {
    for name in ["is_never_satisfied", "is_always_satisfied"] {
        let method = Ident::new(name, Span::call_site());
        lowers(
            &quote!(Ok(effects.#method(self, constraints).await?)),
            &quote!((constraints).#method(db, (self).env)),
        )?;
    }
    Ok(())
}

#[test]
fn canonical_helper_restores_database_and_removes_provider_argument() -> Result<()> {
    lowers(
        &quote!(Ok(self
            .check_type_pair_inner_with(source, target, effects)
            .await?)),
        &quote!(self.check_type_pair_inner(db, source, target)),
    )
}

#[test]
fn nested_result_blocks_and_match_guard_effects_are_lowered() -> Result<()> {
    lowers(
        &quote! {
            Ok(match source {
                item if !effects.is_never_satisfied(self, constraints).await? => {
                    effects.guard(self, item, target, || async {
                        Ok({
                            let left = effects.check_type_pair(self, item, target).await?;
                            left.or_with(self.constraints, || async {
                                Ok({ effects.check_type_pair(self, target, item).await? })
                            }, effects).await?
                        })
                    }).await?
                }
                _ => self.never(),
            })
        },
        &quote! {
            match source {
                item if !(constraints).is_never_satisfied(db, (self).env) => {
                    (self).with_recursion_guard(db, item, target, || {
                        {
                            let left = (self).check_type_pair(db, item, target);
                            left.or(db, self.constraints, || {
                                { (self).check_type_pair(db, target, item) }
                            })
                        }
                    })
                }
                _ => self.never(),
            }
        },
    )
}

#[test]
fn dispatcher_qualified_bound_is_accepted() -> Result<()> {
    let input = quote! {
        async fn value_with<E: pair_effects::PairEffects<'a, 'c, 'db>>(
            effects: &E,
        ) -> Result<bool, E::Error> {
            Ok(true)
        }
    };
    let expected = quote! {
        fn value(db: &'db dyn Db,) -> bool {
            let fields = RelationFieldReads::new(db);
            true
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand(TokenStream::new(), &input)?)?,
        syn::parse2::<syn::File>(quote!(#input #expected))?,
    );
    Ok(())
}

#[test]
fn guarded_patterns_lower_awaited_let_chain_dependencies() -> Result<()> {
    lowers(
        &quote! {
            Ok(match source {
                item if let Some(constraints) = effects
                    .check_typevar_subclass_relation_to_target(self, item, target).await?
                    && !effects.is_never_satisfied(self, constraints).await? => constraints,
                _ => self.never(),
            })
        },
        &quote! {
            match source {
                item if let Some(constraints) = (self)
                    .check_typevar_subclass_relation_to_target(db, item, target)
                    && !(constraints).is_never_satisfied(db, (self).env) => constraints,
                _ => self.never(),
            }
        },
    )
}

#[test]
fn synchronous_step_keeps_its_return_and_question_mark_scope() -> Result<()> {
    lowers(
        &quote! {
            let option = effects.step(|| {
                if source.is_never() { return None; }
                Some(source.lookup()?.value)
            })?;
            Ok(option.unwrap_or(target))
        },
        &quote! {
            let option = (|| {
                if source.is_never() { return None; }
                Some(source.lookup()?.value)
            })();
            option.unwrap_or(target)
        },
    )
}

#[test]
fn unrelated_results_and_options_are_not_operation_wrappers() -> Result<()> {
    lowers(
        &quote! {
            let local = Ok::<_, LocalError>(source);
            let local_error = Err::<Type, _>(LocalError);
            let callback = || {
                if condition { return Ok(source); }
                let value = local?;
                Err(value)
            };
            fn nested() -> Result<Type, LocalError> { return Ok(source); }
            Ok(callback().unwrap_or(target))
        },
        &quote! {
            let local = Ok::<_, LocalError>(source);
            let local_error = Err::<Type, _>(LocalError);
            let callback = || {
                if condition { return Ok(source); }
                let value = local?;
                Err(value)
            };
            fn nested() -> Result<Type, LocalError> { return Ok(source); }
            callback().unwrap_or(target)
        },
    )
}

#[test]
fn branch_tail_results_and_explicit_returns_are_lowered() -> Result<()> {
    lowers(
        &quote! {
            match source {
                Type::Never => Ok(self.always()),
                Type::Object => { if condition { Ok(self.never()) } else { Ok(self.always()) } }
                _ => { return Ok(self.never()); }
            }
        },
        &quote! {
            match source {
                Type::Never => self.always(),
                Type::Object => { if condition { self.never() } else { self.always() } }
                _ => { return self.never(); }
            }
        },
    )
}

#[test]
fn bare_await_result_branches_and_returns_are_lowered() -> Result<()> {
    lowers(
        &quote! {
            if source == target {
                return effects.check_type_pair(self, target, source).await;
            }
            match source {
                Type::Never => effects.check_type_pair(self, source, target).await,
                _ => {
                    if condition {
                        (self.check_type_pair_inner_with(source, target, effects).await)
                    } else {
                        Ok(self.never())
                    }
                }
            }
        },
        &quote! {
            if source == target {
                return (self).check_type_pair(db, target, source);
            }
            match source {
                Type::Never => (self).check_type_pair(db, source, target),
                _ => {
                    if condition {
                        (self.check_type_pair_inner(db, source, target))
                    } else {
                        self.never()
                    }
                }
            }
        },
    )
}

#[test]
fn bare_await_callback_results_preserve_lazy_scope() -> Result<()> {
    lowers(
        &quote! {
            effects.guard(self, source, target, || async {
                if condition {
                    return effects.check_type_pair(self, source, target).await;
                }
                left.and_with(self.constraints, || async {
                    effects.check_type_pair(self, target, source).await
                }, effects).await
            }).await
        },
        &quote! {
            (self).with_recursion_guard(db, source, target, || {
                if condition {
                    return (self).check_type_pair(db, source, target);
                }
                left.and(db, self.constraints, || {
                    (self).check_type_pair(db, target, source)
                })
            })
        },
    )
}

#[test]
fn bare_awaits_outside_operation_results_are_rejected() {
    for body in [
        quote!(let result = effects.check_type_pair(self, source, target).await; Ok(result)),
        quote!(Ok(effects.check_type_pair(self, source, target).await)),
        quote!(effects.check_type_pair(self, source, target).await; Ok(source)),
        quote!(if effects.is_never_satisfied(self, source).await {
            Ok(source)
        } else {
            Ok(target)
        }),
        quote!(match source {
            item if effects.is_never_satisfied(self, item).await => Ok(source),
            _ => Ok(target),
        }),
        quote!(let callback = || { return effects.check_type_pair(self, source, target).await; }; Ok(source)),
        quote!(let callback = || effects.check_type_pair(self, source, target).await; Ok(source)),
        quote!(
            effects
                .guard(self, source, target, || async {
                    let result = effects.check_type_pair(self, source, target).await;
                    Ok(result)
                })
                .await
        ),
        quote!(effects.mystery(self, source).await),
        quote!(self.unlisted_with(db, effects).await),
    ] {
        assert!(
            expand(TokenStream::new(), &original(&body)).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn other_generics_and_effect_free_macros_are_preserved() -> Result<()> {
    let original = quote! {
        pub(super) async fn classify_with<'a, T: Clone, E: PairEffects<'a, 'c, 'db>>(
            &self, value: &'a T, effects: &E,
        ) -> Result<bool, E::Error> where T: Sized {
            assert!(matches!(value, _));
            Ok(true)
        }
    };
    let expected = quote! {
        pub(super) fn classify<'a, T: Clone>(
            &self, db: &'db dyn Db, value: &'a T,
        ) -> bool where T: Sized {
            let fields = RelationFieldReads::new(db);
            assert!(matches!(value, _));
            true
        }
    };
    assert_eq!(
        syn::parse2::<syn::File>(expand(TokenStream::new(), &original)?)?,
        syn::parse2::<syn::File>(quote!(#original #expected))?
    );
    Ok(())
}

#[test]
fn unsupported_effects_awaits_and_provider_escapes_are_rejected() {
    for body in [
        quote!(Ok(effects.mystery(self, source).await?)),
        quote!(Ok(effects
            .protocol_is_equivalent_to_object_extra(self, source)
            .await?)),
        quote!(Ok(effects
            .recursive_type_pair_fallback(self, source, target)
            .await?)),
        quote!(Ok(effects.protocol_is_equivalent_to_object(self).await?)),
        quote!(Ok(effects
            .protocol_is_equivalent_to_object(self, source, target)
            .await?)),
        quote!(Ok(effects.implied_typevar_relation(self, source).await?)),
        quote!(Ok(effects
            .check_type_pair(db, self, source, target)
            .await?)),
        quote!(Ok(effects
            .protocol_is_equivalent_to_object(db, source)
            .await?)),
        quote!(Ok(self
            .check_type_pair_inner_with(db, source, target, effects)
            .await?)),
        quote!(Ok(value
            .and_with(db, self.constraints, || async { Ok(source) }, effects)
            .await?)),
        quote!(Ok(effects.step(db, || source)?)),
        quote!(Ok(self.unlisted_with(db, effects).await?)),
        quote!(Ok(self
            .check_type_pair_inner_with(source, target, alias)
            .await?)),
        quote!(Ok(self
            .check_type_pair_inner_with(source, target, (effects))
            .await?)),
        quote!(Ok(effects.check_type_pair(self, source, target))),
        quote!(let provider = effects; Ok(source)),
        quote!(let effects = other; Ok(source)),
        quote!(let r#effects = other; Ok(source)),
        quote!(Ok(other().await?)),
        quote!(Ok(other()?)),
        quote!(Ok(effects
            .check_type_pair(not_db(), self, source, target)
            .await?)),
        quote!(Ok(effects
            .check_type_pair::<E>(self, source, target)
            .await?)),
        quote!(Ok(async { source })),
        quote!(Ok(async || source)),
        quote!(Ok(try { source })),
        quote!(Err(Failure)),
        quote!(return Err(Failure);),
        quote!(let callback = || effects.check_type_pair(self, source, target).await?; Ok(source)),
        quote!(let callback = || effects.step(|| source)?; Ok(source)),
        quote!(hidden!(effects); Ok(source)),
        quote!(hidden!(nested(async { source })); Ok(source)),
        quote!(let marker: E::Error = value; Ok(source)),
    ] {
        assert!(
            expand(TokenStream::new(), &original(&body)).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn unsupported_callback_shapes_are_rejected() {
    for body in [
        quote!(Ok(effects.guard(self, source, target, || source).await?)),
        quote!(Ok(effects
            .guard(self, source, target, |extra| async { Ok(source) })
            .await?)),
        quote!(Ok(effects
            .guard(self, source, target, async move || { Ok(source) })
            .await?)),
        quote!(Ok(effects
            .guard(self, source, target, |extra| async move { Ok(source) })
            .await?)),
        quote!(Ok(effects.and(self, value, async || Ok(source)).await?)),
        quote!(Ok(effects
            .when_all(self, values, || async { Ok(source) })
            .await?)),
        quote!(Ok(value
            .and_with(self.constraints, || source, effects)
            .await?)),
        quote!(Ok(value
            .when_all_with(self.constraints, || async { Ok(source) }, effects)
            .await?)),
        quote!(Ok(effects.step(async || source)?)),
        quote!(Ok(effects.step(|value| value)?)),
        quote!(Ok(effects.step(|| source).await?)),
    ] {
        assert!(
            expand(TokenStream::new(), &original(&body)).is_err(),
            "accepted {body}"
        );
    }
}

#[test]
fn malformed_signatures_and_macro_arguments_are_rejected() {
    for input in [
        quote!(
            fn plain<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: Other>(
                fields: RelationFieldReads<'db>,
                effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db> + Clone>(
                effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                provider: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                effects: E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
                other: bool,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
            ) -> bool {
                true
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
            ) -> Result<bool, Other> {
                Ok(true)
            }
        ),
        quote!(
            async fn plain_with<E: PairEffects<'a, 'c, 'db>>(
                effects: &E,
            ) -> Result<bool, E::Error>
            where
                E: Clone,
            {
                Ok(true)
            }
        ),
    ] {
        assert!(
            expand(TokenStream::new(), &input).is_err(),
            "accepted {input}"
        );
    }
    assert!(expand(quote!(unknown), &original(&quote!(Ok(true)))).is_err());
}

#[test]
fn pair_database_and_field_reader_parameters_are_rejected() {
    for parameters in [
        quote!(db: &'db dyn Db),
        quote!(fields: &'db dyn Db),
        quote!(reads: RelationFieldReads<'db>),
        quote!(fields: RelationFieldReads<'a>),
        quote!(fields: &RelationFieldReads<'db>),
        quote!(mut fields: RelationFieldReads<'db>),
        quote!(source: Type<'db>, fields: RelationFieldReads<'db>),
    ] {
        let input = quote! {
            async fn value_with<E: PairEffects<'a, 'c, 'db>>(
                &self, #parameters, effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        };
        let error = expand(TokenStream::new(), &input).unwrap_err();
        assert_eq!(
            error.to_string(),
            "pair relations cannot receive database or field-reader parameters",
            "wrong rejection for {input}"
        );
    }
}

#[test]
fn missing_or_reordered_effect_lifetimes_are_rejected() {
    for bound in [
        quote!(PairEffects<'db>),
        quote!(PairEffects<'a, 'db>),
        quote!(PairEffects<'c, 'a, 'db>),
        quote!(PairEffects<'a, 'c, 'other>),
        quote!(pair_effects::PairEffects<'db>),
        quote!(pair_effects::PairEffects<'c, 'a, 'db>),
    ] {
        let input = quote! {
            async fn value_with<E: #bound>(
                fields: RelationFieldReads<'db>, effects: &E,
            ) -> Result<bool, E::Error> {
                Ok(true)
            }
        };
        assert!(
            expand(TokenStream::new(), &input).is_err(),
            "accepted {input}"
        );
    }
}
