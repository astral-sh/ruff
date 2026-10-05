//! Preparation and parameter contexts shared by ordinary and controlled argument inference.

use super::*;
use crate::{Db, ProgramEnvironment};

pub(super) trait ArgumentPreparationEffects<'db> {
    type Error;
    type Builder: std::borrow::Borrow<ConstraintSetBuilder<'db>>;
    async fn new_builder(&self) -> Result<Self::Builder, Self::Error>;
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
    async fn clone_arguments<'call>(
        &self,
        arguments: &CallArguments<'call, 'db>,
    ) -> Result<CallArguments<'call, 'db>, Self::Error>;
    async fn callables<'a>(
        &self,
        bindings: &'a Bindings<'db>,
    ) -> Result<Vec<&'a CallableBinding<'db>>, Self::Error>;
    async fn binding_work(&self, binding: &CallableBinding<'db>) -> Result<(), Self::Error>;
    async fn candidate_indices(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<SmallVec<[usize; 1]>, Self::Error>;
    async fn occurrence_count(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        overload: &Binding<'db>,
        binding: &CallableBinding<'db>,
        index: usize,
    ) -> Result<usize, Self::Error>;
    async fn parameter_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        overload: &Binding<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        index: usize,
        tcx: TypeContext<'db>,
        specialization: &OnceCell<Option<Specialization<'db>>>,
    ) -> Result<Option<ArgumentTypeContext<'db>>, Self::Error>;
}

impl<'db> ArgumentPreparationEffects<'db> for InlineArgumentEffects {
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;

    async fn new_builder(&self) -> Result<Self::Builder, Self::Error> {
        Ok(ConstraintSetBuilder::new())
    }

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }
    async fn clone_arguments<'call>(
        &self,
        arguments: &CallArguments<'call, 'db>,
    ) -> Result<CallArguments<'call, 'db>, Self::Error> {
        Ok(arguments.clone())
    }
    async fn callables<'a>(
        &self,
        bindings: &'a Bindings<'db>,
    ) -> Result<Vec<&'a CallableBinding<'db>>, Self::Error> {
        let mut callables = Vec::new();
        bindings.visit_type_context_callables(&mut |binding| callables.push(binding));
        Ok(callables)
    }
    async fn binding_work(&self, _binding: &CallableBinding<'db>) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn candidate_indices(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<SmallVec<[usize; 1]>, Self::Error> {
        Ok(binding.candidate_overload_indices(db, env, arguments))
    }
    async fn occurrence_count(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        overload: &Binding<'db>,
        binding: &CallableBinding<'db>,
        index: usize,
    ) -> Result<usize, Self::Error> {
        Ok(overload.typevar_occurrences_for_parameter(db, env, binding, index))
    }
    async fn parameter_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        overload: &Binding<'db>,
        binding: &CallableBinding<'db>,
        arguments: &CallArguments<'_, 'db>,
        index: usize,
        tcx: TypeContext<'db>,
        specialization: &OnceCell<Option<Specialization<'db>>>,
    ) -> Result<Option<ArgumentTypeContext<'db>>, Self::Error> {
        Ok(overload.argument_type_context(
            db,
            env,
            constraints,
            binding,
            arguments,
            index,
            tcx,
            || {
                *specialization.get_or_init(|| {
                    overload.argument_type_context_specialization(db, env, constraints, tcx)
                })
            },
        ))
    }
}

async fn push_small<'db, A: smallvec::Array, E: ArgumentPreparationEffects<'db>>(
    value: &mut SmallVec<A>,
    item: A::Item,
    effects: &E,
) -> Result<(), E::Error> {
    let grows = value.len() == value.capacity();
    let capacity = value
        .len()
        .checked_add(1)
        .and_then(usize::checked_next_power_of_two);
    let bytes = if grows {
        capacity.and_then(|n| n.checked_mul(size_of::<A::Item>()))
    } else {
        Some(0)
    };
    let mut item = Some(item);
    effects
        .local(value.len().checked_add(2), bytes, || {
            if grows && let Some(capacity) = capacity {
                value.reserve_exact(capacity - value.len());
            }
            value.extend(item.take());
        })
        .await
}

async fn push_vec<'db, T, E: ArgumentPreparationEffects<'db>>(
    value: &mut Vec<T>,
    item: T,
    effects: &E,
) -> Result<(), E::Error> {
    let grows = value.len() == value.capacity();
    let capacity = value
        .len()
        .checked_add(1)
        .and_then(usize::checked_next_power_of_two);
    let bytes = if grows {
        capacity.and_then(|n| n.checked_mul(size_of::<T>()))
    } else {
        Some(0)
    };
    let mut item = Some(item);
    effects
        .local(value.len().checked_add(2), bytes, || {
            if grows && let Some(capacity) = capacity {
                value.reserve_exact(capacity - value.len());
            }
            value.extend(item.take());
        })
        .await
}

pub(super) async fn prepare_with<
    'root,
    'db,
    'ast,
    'arg,
    'call,
    S: ArgumentStorage<'call, 'db>,
    E: ArgumentPreparationEffects<'db>,
>(
    input: Input<'db, 'arg, S>,
    builders: &mut BuilderStore<'root, 'db, 'ast>,
    effects: &E,
) -> Result<Context<'db, 'arg, 'call, S, E::Builder>, E::Error> {
    let builder = builders.builder(input.builder);
    let db = builder.db();
    let constraints = effects.new_builder().await?;
    #[cfg(test)]
    effects
        .local(Some(1), Some(0), || {
            crate::types::relation::source::resources::observations::observe_invocation(
                db,
                std::borrow::Borrow::borrow(&constraints),
                crate::types::relation::source::resources::observations::InvocationStage::Preparing,
            );
        })
        .await?;
    let (arguments, bindings) = input.storage.parts();
    let baseline = effects.clone_arguments(arguments).await?;
    let env = builder.program_environment();
    let mut generic_arguments = SmallVec::<[bool; 8]>::new();
    let bytes = if arguments.len() > 8 {
        Some(arguments.len())
    } else {
        Some(0)
    };
    effects
        .local(arguments.len().checked_add(1), bytes, || {
            generic_arguments.reserve_exact(arguments.len());
            generic_arguments.resize(arguments.len(), false);
        })
        .await?;
    let mut typevar_occurrences = 0;
    let mut has_generic_context = false;
    let mut candidates = OverloadSet::new();

    // Inferable typevar occurrences bound fixpoint iteration. The candidate set remains
    // unchanged across those iterations.
    let callables = effects.callables(bindings).await?;
    for binding in callables {
        let indices = effects
            .candidate_indices(db, env, binding, arguments)
            .await?;
        has_generic_context |= effects
            .local(indices.len().checked_add(1), Some(0), || {
                indices.iter().any(|&index| {
                    binding.overloads()[index]
                        .signature
                        .generic_context
                        .is_some()
                })
            })
            .await?;
        for index in &indices {
            let overload = &binding.overloads()[*index];
            if overload.signature.generic_context.is_none() {
                continue;
            }
            let mut occurrences = 0;
            for (argument_index, is_generic) in generic_arguments.iter_mut().enumerate() {
                if arguments.is_variadic(argument_index) {
                    continue;
                }
                let count = effects
                    .occurrence_count(db, env, overload, binding, argument_index)
                    .await?;
                *is_generic |= count > 0;
                occurrences += count;
            }
            typevar_occurrences = typevar_occurrences.max(occurrences);
        }
        push_small(&mut candidates, indices, effects).await?;
    }
    let mut generic_indices = SmallVec::new();
    for (index, is_generic) in generic_arguments.into_iter().enumerate() {
        if is_generic {
            push_small(&mut generic_indices, index, effects).await?;
        }
    }
    Ok(Context {
        input,
        baseline,
        constraints,
        candidates,
        generic_arguments: generic_indices,
        typevar_occurrences,
        has_generic_context,
        teardown_cache: false,
        active: Active::Root,
    })
}

pub(super) async fn collect_contexts_with<'db, E: ArgumentPreparationEffects<'db>>(
    builder: &TypeInferenceBuilder<'db, '_>,
    arguments: &CallArguments<'_, 'db>,
    bindings: &Bindings<'db>,
    candidates: Option<&OverloadSet>,
    constraints: &ConstraintSetBuilder<'db>,
    tcx: TypeContext<'db>,
    effects: &E,
) -> Result<Vec<Option<MatchingArgumentTypeContext<'db>>>, E::Error> {
    type Overloads<'a, 'db> = Vec<(
        &'a Binding<'db>,
        &'a CallableBinding<'db>,
        OnceCell<Option<Specialization<'db>>>,
    )>;
    let db = builder.db();
    let env = builder.program_environment();
    let mut overloads: Overloads = Vec::new();
    let callables = effects.callables(bindings).await?;
    let mut candidate_overloads = candidates.map(|candidates| candidates.iter());
    for binding in callables {
        effects.binding_work(binding).await?;
        if let Some(candidate_overloads) = &mut candidate_overloads {
            let indices = candidate_overloads
                .next()
                .expect("checked bindings are stable across fixpoint iterations");
            if indices.is_empty() {
                // A single non-matching overload still supplies context for better diagnostics.
                if let [overload] = binding.overloads() {
                    push_vec(
                        &mut overloads,
                        (overload, binding, OnceCell::new()),
                        effects,
                    )
                    .await?;
                }
            } else {
                for &index in indices {
                    push_vec(
                        &mut overloads,
                        (&binding.overloads()[index], binding, OnceCell::new()),
                        effects,
                    )
                    .await?;
                }
            }
        } else {
            let mut matching = binding.matching_overloads().peekable();
            if matching.peek().is_some() {
                for (_, overload) in matching {
                    push_vec(
                        &mut overloads,
                        (overload, binding, OnceCell::new()),
                        effects,
                    )
                    .await?;
                }
            } else if let Some(overload) = binding.best_failing_overload() {
                // A single failing overload still supplies context for better diagnostics.
                push_vec(
                    &mut overloads,
                    (overload, binding, OnceCell::new()),
                    effects,
                )
                .await?;
            }
        }
    }
    let mut contexts = Vec::new();
    for index in 0..arguments.len() {
        let context = if arguments.is_variadic(index) {
            None
        } else if let [(overload, binding, specialization)] = overloads.as_slice() {
            Some(MatchingArgumentTypeContext::Unique(
                effects
                    .parameter_context(
                        db,
                        env,
                        constraints,
                        overload,
                        binding,
                        arguments,
                        index,
                        tcx,
                        specialization,
                    )
                    .await?,
            ))
        } else {
            let mut many = Vec::new();
            for (overload, binding, specialization) in &overloads {
                let context = effects
                    .parameter_context(
                        db,
                        env,
                        constraints,
                        overload,
                        binding,
                        arguments,
                        index,
                        tcx,
                        specialization,
                    )
                    .await?;
                push_vec(&mut many, context, effects).await?;
            }
            Some(MatchingArgumentTypeContext::Many(many))
        };
        push_vec(&mut contexts, context, effects).await?;
    }
    Ok(contexts)
}
