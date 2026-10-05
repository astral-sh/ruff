//! Argument matching shares source-order decisions while exposing storage and semantic effects.

use std::alloc::Layout;

use super::constructor_matching::{
    ConstructorMatchingEffects, freshen_constructor_with, match_constructor_with,
};
#[cfg(test)]
use super::constructor_matching::{ConstructorMatchingOperation, ConstructorMatchingPhase};
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum MatchingDependency {
    GenericFreshening,
    Variadic,
    Keywords,
    UnpackedVariadic,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct MatchingQuote {
    pub work: usize,
    pub requested_bytes: usize,
}

impl MatchingQuote {
    fn scan(items: usize) -> Option<Self> {
        Some(Self {
            work: items.checked_add(1)?,
            requested_bytes: 0,
        })
    }
}

pub(in crate::types) trait ParameterMatchingEffects<'db>:
    ConstructorMatchingEffects<'db>
{
    async fn matching_local<T>(
        &self,
        quote: Option<MatchingQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn dependency<T>(
        &self,
        dependency: MatchingDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn bound_arguments(
        &self,
        arguments: &CallArguments<'_, 'db>,
        bound_type: Option<Type<'db>>,
        action: impl FnOnce(),
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub(super) struct InlineParameterMatching;

impl<'db> ParameterMatchingEffects<'db> for InlineParameterMatching {
    async fn matching_local<T>(
        &self,
        _quote: Option<MatchingQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn dependency<T>(
        &self,
        _dependency: MatchingDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn bound_arguments(
        &self,
        _arguments: &CallArguments<'_, 'db>,
        _bound_type: Option<Type<'db>>,
        action: impl FnOnce(),
    ) -> Result<(), Self::Error> {
        action();
        Ok(())
    }
}

impl<'db> Bindings<'db> {
    pub(in crate::types) async fn match_parameters_with<E: ParameterMatchingEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let enclosing = self
            .enclosing_binding_contexts
            .as_deref()
            .unwrap_or_default();
        let nonce_generator =
            TypeVarNonceGenerator::new_for_matching_with(enclosing, effects).await?;
        effects
            .matching_local(MatchingQuote::scan(self.elements.len()), || ())
            .await?;
        for element in &mut self.elements {
            effects
                .matching_local(MatchingQuote::scan(element.items.len()), || ())
                .await?;
            for item in &mut element.items {
                match item {
                    CallableItem::Regular(binding) => {
                        let generic = effects
                            .matching_local(MatchingQuote::scan(binding.overloads.len()), || {
                                binding
                                    .overloads
                                    .iter()
                                    .any(|overload| overload.signature.generic_context.is_some())
                            })
                            .await?;
                        if generic {
                            effects
                                .dependency(MatchingDependency::GenericFreshening, || {
                                    binding.freshen_generic_contexts_in_place(
                                        db,
                                        env,
                                        &nonce_generator,
                                    );
                                })
                                .await?;
                        }
                    }
                    CallableItem::Constructor(binding) => {
                        freshen_constructor_with(db, env, binding, &nonce_generator, effects)
                            .await?;
                    }
                }
            }
        }
        effects
            .matching_local(MatchingQuote::scan(self.elements.len()), || ())
            .await?;
        for element in &mut self.elements {
            effects
                .matching_local(MatchingQuote::scan(element.items.len()), || ())
                .await?;
            for item in &mut element.items {
                match item {
                    CallableItem::Regular(binding) => {
                        binding
                            .match_parameters_with(db, env, arguments, effects)
                            .await?;
                    }
                    CallableItem::Constructor(binding) => {
                        match_constructor_with(db, env, binding, arguments, effects).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'db> CallableBinding<'db> {
    pub(super) async fn match_parameters_with<E: ParameterMatchingEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        #[cfg(test)]
        effects.matching_entry(
            ConstructorMatchingPhase::Matching,
            std::ptr::from_ref(self).addr(),
        );
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let mut bound_arguments = None;
        #[cfg(test)]
        effects.before_matching(ConstructorMatchingOperation::BoundArguments);
        effects
            .bound_arguments(arguments, self.bound_type, || {
                bound_arguments = Some(arguments.with_self(self.bound_type));
                #[cfg(test)]
                effects.after_matching(ConstructorMatchingOperation::BoundArguments);
            })
            .await?;
        if let Some(bound_arguments) = &bound_arguments {
            effects
                .matching_local(MatchingQuote::scan(self.overloads.len()), || ())
                .await?;
            for overload in &mut self.overloads {
                overload
                    .match_parameters_with(db, env, bound_arguments, effects)
                    .await?;
            }
        }
        Ok(())
    }
}

impl<'db> Binding<'db> {
    pub(super) async fn match_parameters_with<E: ParameterMatchingEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let parameters = self.signature.parameters();
        let metadata = arguments
            .len()
            .checked_add(parameters.len())
            .and_then(|count| count.checked_add(self.argument_matches.len()));
        let scan = metadata.and_then(|count| {
            Some(MatchingQuote {
                work: count.checked_mul(12)?.checked_add(64)?,
                requested_bytes: 0,
            })
        });
        let quote = effects
            .matching_local(scan, || {
                matcher_storage_quote(
                    arguments,
                    parameters,
                    &self.errors,
                    &self.argument_matches,
                    self.parameter_tys.len(),
                )
            })
            .await?;
        let mut matcher = None;
        let mut keywords_arguments = Vec::new();
        let errors = &mut self.errors;
        // These owners live outside protected callbacks, including when a callback queues work
        // and the driver must drain that work before returning an interruption.
        #[cfg(test)]
        effects.before_matching(ConstructorMatchingOperation::MatcherAllocation);
        effects
            .matching_local(quote, || {
                matcher = Some(ArgumentMatcher::new(arguments, parameters, errors));
                #[cfg(test)]
                effects.after_matching(ConstructorMatchingOperation::MatcherAllocation);
            })
            .await?;
        if let Some(matcher) = matcher.as_mut() {
            for (argument_index, (argument, argument_types)) in arguments.iter().enumerate() {
                match argument {
                    Argument::Positional | Argument::Synthetic => {
                        effects
                            .matching_local(MatchingQuote::scan(1), || {
                                let _ =
                                    matcher.match_positional(argument_index, argument, None, false);
                            })
                            .await?;
                    }
                    Argument::Keyword(name) => {
                        effects
                            .matching_local(MatchingQuote::scan(1), || {
                                let _ = matcher.match_keyword(argument_index, argument, None, name);
                            })
                            .await?;
                    }
                    Argument::Variadic => {
                        effects
                            .dependency(MatchingDependency::Variadic, || {
                                let _ = matcher.match_variadic(
                                    db,
                                    env,
                                    argument_index,
                                    argument,
                                    argument_types.get_default(),
                                );
                            })
                            .await?;
                    }
                    Argument::Keywords => {
                        effects
                            .matching_local(MatchingQuote::scan(1), || {
                                keywords_arguments.push((argument_index, argument_types))
                            })
                            .await?;
                    }
                }
            }
            for &(keywords_index, keywords_type) in &keywords_arguments {
                effects
                    .dependency(MatchingDependency::Keywords, || {
                        matcher.match_keyword_variadic(
                            db,
                            env,
                            keywords_index,
                            keywords_type.get_default(),
                        );
                    })
                    .await?;
            }
            let mut missing = Vec::new();
            let mut paramspec = None;
            effects
                .matching_local(MatchingQuote::scan(1), || {
                    (missing, paramspec) = matcher.missing_parameters();
                })
                .await?;
            let unpacked = effects
                .matching_local(MatchingQuote::scan(1), || {
                    parameters
                        .variadic()
                        .is_some_and(|(_, parameter)| parameter.has_starred_annotation())
                })
                .await?;
            if unpacked {
                effects
                    .dependency(MatchingDependency::UnpackedVariadic, || {
                        matcher.match_unpacked_variadic(db, env, &mut missing);
                    })
                    .await?;
            }
            effects
                .matching_local(MatchingQuote::scan(1), || {
                    if !missing.is_empty() {
                        matcher.errors.push(BindingError::MissingArguments {
                            parameters: ParameterContexts(std::mem::take(&mut missing)),
                            paramspec,
                        });
                    }
                    self.parameter_tys = vec![None; parameters.len()].into_boxed_slice();
                    self.variadic_argument_matched_to_variadic_parameter =
                        matcher.variadic_argument_matched_to_variadic_parameter;
                    self.argument_matches =
                        std::mem::take(&mut matcher.argument_matches).into_boxed_slice();
                })
                .await?;
        }
        Ok(())
    }
}

fn matcher_storage_quote(
    arguments: &CallArguments<'_, '_>,
    parameters: &Parameters<'_>,
    errors: &Vec<BindingError<'_>>,
    old_matches: &[MatchedArgument<'_>],
    old_parameter_count: usize,
) -> Option<MatchingQuote> {
    let a = arguments.len();
    let p = parameters.len();
    let names = arguments
        .iter()
        .try_fold(0usize, |total, (argument, _)| {
            total.checked_add(match argument {
                Argument::Keyword(name) => name.len(),
                _ => 0,
            })
        })?
        .checked_add(parameters.iter().try_fold(0usize, |total, parameter| {
            total.checked_add(parameter.name().map(|name| name.len()).unwrap_or(0))
        })?)?;
    let slots = a.checked_add(1)?.checked_mul(4)?.checked_add(32)?;
    let old_match_items = old_matches.iter().try_fold(0usize, |total, argument| {
        total
            .checked_add(argument.parameters.len().checked_add(3)?)?
            .checked_add(usize::from(argument.parameters.spilled()))
    })?;
    // Keyword lookup and duplicate detection compare source names. Include worst-case table
    // probes, all linear scans, and eventual retirement of the newly allocated flat owners.
    let scans = a.checked_add(p)?.checked_add(4)?;
    let work = scans
        .checked_mul(scans)?
        .checked_mul(names.checked_add(16)?)?
        .checked_add(a.checked_mul(slots)?)?
        .checked_add(old_match_items)?
        .checked_add(old_parameter_count)?
        .checked_add(usize::from(!old_matches.is_empty()))?
        .checked_add(usize::from(old_parameter_count != 0))?
        .checked_add(errors.capacity())?;
    let hash_bytes = slots
        .checked_mul(size_of::<usize>().checked_add(1)?)?
        .checked_mul(4)?;
    Layout::from_size_align(hash_bytes, align_of::<usize>()).ok()?;
    let deferred_capacity = a.checked_add(4)?.checked_mul(4)?;
    let error_capacity = errors
        .capacity()
        .checked_add(a)?
        .checked_add(4)?
        .checked_mul(4)?;
    let missing_capacity = p.checked_add(4)?.checked_mul(4)?;
    let requested_bytes = matching_array_bytes::<MatchedArgument<'_>>(a)?
        .checked_add(matching_array_bytes::<ParameterInfo>(p)?)?
        .checked_add(matching_array_bytes::<(usize, &CallArgumentTypes<'_>)>(
            deferred_capacity,
        )?)?
        .checked_add(hash_bytes)?
        .checked_add(matching_array_bytes::<BindingError<'_>>(error_capacity)?)?
        .checked_add(matching_array_bytes::<ParameterContext>(missing_capacity)?)?
        .checked_add(matching_array_bytes::<Option<Type<'_>>>(p)?)?
        .checked_add(matching_array_bytes::<MatchedArgument<'_>>(a)?)?
        .checked_add(names)?
        .checked_add(a.checked_mul(4 * size_of::<usize>())?)?;
    Some(MatchingQuote {
        work,
        requested_bytes,
    })
}

/// Validates one conservative array-allocation bound before matching mutates its owners.
fn matching_array_bytes<T>(len: usize) -> Option<usize> {
    Some(Layout::array::<T>(len).ok()?.size())
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;

    #[test]
    fn nonsplat_matching_preserves_parameter_indexes_and_shape_errors() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/matching.py",
            "def choose(first, /, second, *, third): pass\n",
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/matching.py")?;
        let file = ProgramFile::new(&db, file, env.program(&db));
        let function = global_symbol(&db, file, "choose").place.expect_type();
        let cases: &[(&[Argument<'_>], &[Option<usize>])] = &[
            (
                &[
                    Argument::Positional,
                    Argument::Keyword("second"),
                    Argument::Keyword("third"),
                ],
                &[Some(0), Some(1), Some(2)],
            ),
            (
                &[
                    Argument::Positional,
                    Argument::Positional,
                    Argument::Keyword("second"),
                    Argument::Keyword("third"),
                ],
                &[Some(0), Some(1), Some(1), Some(2)],
            ),
            (
                &[Argument::Keyword("second"), Argument::Keyword("third")],
                &[Some(1), Some(2)],
            ),
            (
                &[
                    Argument::Positional,
                    Argument::Keyword("second"),
                    Argument::Keyword("third"),
                    Argument::Keyword("extra"),
                ],
                &[Some(0), Some(1), Some(2), None],
            ),
            (
                &[
                    Argument::Keyword("first"),
                    Argument::Keyword("second"),
                    Argument::Keyword("third"),
                ],
                &[None, Some(1), Some(2)],
            ),
            (
                &[
                    Argument::Positional,
                    Argument::Positional,
                    Argument::Positional,
                    Argument::Keyword("third"),
                ],
                &[Some(0), Some(1), None, Some(2)],
            ),
        ];
        for (case, (source_arguments, expected_matches)) in cases.iter().enumerate() {
            let mut arguments = CallArguments::default();
            for argument in *source_arguments {
                arguments.push_argument(*argument, None);
            }
            let bindings = function
                .bindings(&db, &env)
                .match_parameters(&db, &env, &arguments);
            let binding = bindings
                .iter_flat()
                .next()
                .and_then(|callable| callable.overloads.first())
                .ok_or_else(|| anyhow::anyhow!("missing function binding"))?;
            let parameters = binding.signature.parameters();
            let expected_errors = match case {
                0 => vec![],
                1 => vec![BindingError::ParameterAlreadyAssigned {
                    argument_index: Some(2),
                    parameter: ParameterContext::new(&parameters[1], 1, false),
                }],
                2 => vec![BindingError::MissingArguments {
                    parameters: ParameterContexts(vec![ParameterContext::new(
                        &parameters[0],
                        0,
                        false,
                    )]),
                    paramspec: None,
                }],
                3 => vec![BindingError::UnknownArgument {
                    argument_name: Name::new("extra"),
                    argument_index: Some(3),
                }],
                4 => vec![BindingError::PositionalOnlyParameterAsKwarg {
                    argument_index: Some(0),
                    parameter: ParameterContext::new(&parameters[0], 0, true),
                }],
                _ => vec![BindingError::TooManyPositionalArguments {
                    first_excess_argument_index: Some(2),
                    expected_positional_count: 2,
                    provided_positional_count: 3,
                }],
            };
            assert_eq!(binding.errors, expected_errors, "case {case}");
            assert_eq!(&*binding.parameter_tys, &[None; 3]);
            assert!(!binding.variadic_argument_matched_to_variadic_parameter);
            assert_eq!(binding.argument_matches.len(), expected_matches.len());
            for (matched, expected) in binding.argument_matches.iter().zip(*expected_matches) {
                assert_eq!(matched.matched, expected.is_some());
                assert_eq!(matched.parameters.len(), usize::from(expected.is_some()));
                if let (Some(matched), Some(expected)) = (matched.parameters.first(), expected) {
                    assert_eq!(matched.index, *expected);
                    assert_eq!(matched.argument_type, None);
                    assert_eq!(matched.expected_type, None);
                    assert!(matches!(
                        matched.provenance,
                        InvalidArgumentTypeProvenance::Argument
                    ));
                }
            }
        }
        Ok(())
    }
}
