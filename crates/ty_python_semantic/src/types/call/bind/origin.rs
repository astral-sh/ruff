//! Descriptor provenance retains every candidate and the overloads selected by the call.

use std::convert::Infallible;

use salsa::execution_probe::FieldRequest;
use smallvec::SmallVec;

use super::{Binding, Bindings, CallableBinding, OverloadCallResult};
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::{
    DescriptorArgumentComparison, DescriptorDispatch, DescriptorDispatches, DescriptorOrigin, Type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(in crate::types) trait OriginEffects<'db> {
    type Error;

    async fn origin_local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn origin_field<R: FieldRequest<'db>>(
        &self,
        request: R,
    ) -> Result<R::Output, Self::Error>;

    async fn clone_signature(
        &self,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error>;

    async fn callable_origin(
        &self,
        db: &'db dyn Db,
        callable: &CallableBinding<'db>,
        arguments: &[Type<'db>],
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;

    async fn merge_origin(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;

    async fn dispatch(
        &self,
        db: &'db dyn Db,
        signatures: CallableSignature<'db>,
        arguments: Box<[Type<'db>]>,
        comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
        selected: Box<[usize]>,
        failed: bool,
    ) -> Result<DescriptorDispatch<'db>, Self::Error>;

    async fn dispatches(
        &self,
        db: &'db dyn Db,
        elements: Box<[DescriptorDispatch<'db>]>,
    ) -> Result<DescriptorDispatches<'db>, Self::Error>;

    async fn insert_dispatch(
        &self,
        elements: &mut FxOrderSet<DescriptorDispatch<'db>>,
        dispatch: DescriptorDispatch<'db>,
    ) -> Result<(), Self::Error>;

    async fn finish_dispatches(
        &self,
        elements: FxOrderSet<DescriptorDispatch<'db>>,
    ) -> Result<Box<[DescriptorDispatch<'db>]>, Self::Error>;

    async fn downstream_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;

    async fn origin_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn restrict_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        origin: DescriptorOrigin<'db>,
        return_type: Type<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;
}

pub(in crate::types) struct InlineOriginEffects;

impl<'db> OriginEffects<'db> for InlineOriginEffects {
    type Error = Infallible;

    async fn origin_local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn origin_field<R: FieldRequest<'db>>(
        &self,
        request: R,
    ) -> Result<R::Output, Infallible> {
        Ok(request.read_ordinary())
    }

    async fn clone_signature(
        &self,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Infallible> {
        Ok(signature.clone())
    }

    async fn callable_origin(
        &self,
        db: &'db dyn Db,
        callable: &CallableBinding<'db>,
        arguments: &[Type<'db>],
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        callable.descriptor_origin_with(db, arguments, self).await
    }

    async fn merge_origin(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        left.merge_with(db, right, self).await
    }

    async fn dispatch(
        &self,
        db: &'db dyn Db,
        signatures: CallableSignature<'db>,
        arguments: Box<[Type<'db>]>,
        comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
        selected: Box<[usize]>,
        failed: bool,
    ) -> Result<DescriptorDispatch<'db>, Infallible> {
        Ok(DescriptorDispatch::new(
            db,
            signatures,
            arguments,
            comparisons,
            selected,
            failed,
        ))
    }

    async fn dispatches(
        &self,
        db: &'db dyn Db,
        elements: Box<[DescriptorDispatch<'db>]>,
    ) -> Result<DescriptorDispatches<'db>, Infallible> {
        Ok(DescriptorDispatches::new(db, elements))
    }

    async fn insert_dispatch(
        &self,
        elements: &mut FxOrderSet<DescriptorDispatch<'db>>,
        dispatch: DescriptorDispatch<'db>,
    ) -> Result<(), Infallible> {
        elements.insert(dispatch);
        Ok(())
    }

    async fn finish_dispatches(
        &self,
        elements: FxOrderSet<DescriptorDispatch<'db>>,
    ) -> Result<Box<[DescriptorDispatch<'db>]>, Infallible> {
        Ok(elements.into_iter().collect())
    }

    async fn downstream_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>],
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        Ok(bindings.descriptor_origin(db, env, arguments))
    }

    async fn origin_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(bindings.return_type(db, env))
    }

    async fn restrict_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        origin: DescriptorOrigin<'db>,
        return_type: Type<'db>,
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        Ok(origin.restrict_to_return_type(db, env, return_type))
    }
}

// Collection may grow an intermediate vector before boxing it. Include that backing and
// the final allocation; all elements here are scalar handles or already-owned payloads.
// Geometric Vec/SmallVec growth requests at most `4 * count + 16` slots cumulatively,
// including minimum capacities; allow another `count` if boxing reallocates.
fn collected_bytes<T>(count: usize) -> Option<usize> {
    if count == 0 {
        Some(0)
    } else {
        count
            .checked_mul(5)?
            .checked_add(16)?
            .checked_mul(size_of::<T>())
    }
}

impl<'db> DescriptorOrigin<'db> {
    pub(in crate::types) async fn merge_with<E: OriginEffects<'db>>(
        self,
        db: &'db dyn Db,
        other: Self,
        effects: &E,
    ) -> Result<Self, E::Error> {
        effects.origin_local(Some(4), Some(0), || ()).await?;
        let dispatches = match (self.dispatches, other.dispatches) {
            (Some(left), Some(right)) if left != right => {
                let left = effects
                    .origin_field(left.field_requests(db).elements())
                    .await?;
                let right = effects
                    .origin_field(right.field_requests(db).elements())
                    .await?;
                let mut elements = effects
                    .origin_local(
                        Some(1),
                        Some(size_of::<FxOrderSet<DescriptorDispatch<'db>>>()),
                        FxOrderSet::default,
                    )
                    .await?;
                for dispatch in left.iter().chain(right.iter()).copied() {
                    effects.insert_dispatch(&mut elements, dispatch).await?;
                }
                let elements = effects.finish_dispatches(elements).await?;
                Some(effects.dispatches(db, elements).await?)
            }
            _ => self.dispatches.or(other.dispatches),
        };
        effects
            .origin_local(Some(3), Some(size_of::<Self>()), || Self {
                dispatches,
                incomplete: self.incomplete || other.incomplete,
                return_contains_recursive_recovery: self.return_contains_recursive_recovery
                    || other.return_contains_recursive_recovery,
            })
            .await
    }
}

impl<'db> Bindings<'db> {
    /// Attaches the descriptor dispatches that selected each callable before argument checking.
    /// The binding owner remains alive while an origin merge borrows the canonical interner.
    /// An error can leave earlier origins merged; callers must discard this unpublished result
    /// when they require all origins to be attached before publication.
    pub(in crate::types) async fn add_descriptor_origin_with<E: OriginEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        origin: DescriptorOrigin<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        if effects
            .origin_local(Some(4), Some(size_of::<bool>()), || {
                origin == DescriptorOrigin::default()
            })
            .await?
        {
            return Ok(());
        }
        let elements = effects
            .origin_local(Some(1), Some(size_of::<usize>()), || self.elements.len())
            .await?;
        for element in 0..elements {
            let items = effects
                .origin_local(Some(2), Some(size_of::<usize>()), || {
                    self.elements[element].items.len()
                })
                .await?;
            for item in 0..items {
                let previous = effects
                    .origin_local(Some(3), Some(size_of::<DescriptorOrigin<'db>>()), || {
                        self.elements[element].items[item]
                            .callable()
                            .descriptor_origin
                    })
                    .await?;
                let merged = effects.merge_origin(db, previous, origin).await?;
                effects
                    .origin_local(Some(3), Some(size_of::<DescriptorOrigin<'db>>()), || {
                        self.elements[element].items[item]
                            .callable_mut()
                            .descriptor_origin = merged;
                    })
                    .await?;
            }
        }
        Ok(())
    }

    pub(in crate::types) async fn descriptor_origin_with<E: OriginEffects<'db>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &[Type<'db>],
        effects: &E,
    ) -> Result<DescriptorOrigin<'db>, E::Error> {
        let mut origin = effects
            .origin_local(
                Some(1),
                Some(size_of::<DescriptorOrigin<'db>>()),
                DescriptorOrigin::default,
            )
            .await?;
        for element in &self.elements {
            effects.origin_local(Some(1), Some(0), || ()).await?;
            for item in &element.items {
                effects.origin_local(Some(4), Some(0), || ()).await?;
                let callable_origin = effects
                    .callable_origin(db, item.callable(), arguments)
                    .await?;
                origin = effects.merge_origin(db, origin, callable_origin).await?;
                if let Some(constructor) = item.as_constructor()
                    && let Some(downstream) = constructor.downstream_constructor()
                {
                    let downstream = effects
                        .downstream_origin(db, env, downstream, arguments)
                        .await?;
                    origin = effects.merge_origin(db, origin, downstream).await?;
                }
            }
        }
        if origin.return_contains_recursive_recovery {
            let return_type = effects.origin_return_type(db, env, self).await?;
            effects.restrict_origin(db, env, origin, return_type).await
        } else {
            Ok(origin)
        }
    }
}

impl<'db> CallableBinding<'db> {
    pub(in crate::types) async fn descriptor_origin_with<E: OriginEffects<'db>>(
        &self,
        db: &'db dyn Db,
        arguments: &[Type<'db>],
        effects: &E,
    ) -> Result<DescriptorOrigin<'db>, E::Error> {
        let count = self.overloads.len();
        let selection_work = effects
            .origin_local(count.checked_add(1), Some(0), || {
                let selection = match &self.overload_call_result {
                    Some(OverloadCallResult::ArgumentTypeExpansion(expanded)) => {
                        expanded.selected_overloads.len()
                    }
                    _ => 0,
                };
                self.overloads.iter().try_fold(1usize, |work, overload| {
                    work.checked_add(overload.errors.len())?
                        .checked_add(selection)?
                        .checked_add(4)
                })
            })
            .await?;
        let mut selected = effects
            .origin_local(
                selection_work
                    .and_then(|work| work.checked_add(count).and_then(|work| work.checked_add(4))),
                if count > 1 {
                    collected_bytes::<&Binding<'db>>(count)
                } else {
                    Some(0)
                }
                .and_then(|bytes| bytes.checked_add(size_of::<SmallVec<[&Binding<'db>; 1]>>())),
                || {
                    self.selected_overloads()
                        .map(|(_, overload)| overload)
                        .collect::<SmallVec<[_; 1]>>()
                },
            )
            .await?;
        // A single failing overload still supplies the recovery return type.
        effects
            .origin_local(Some(4), Some(0), || {
                if selected.is_empty()
                    && self.overload_call_result.is_none()
                    && let [overload] = self.overloads.as_slice()
                {
                    selected.push(overload);
                }
            })
            .await?;
        let arguments_len = arguments
            .len()
            .checked_add(usize::from(self.bound_type.is_some()));
        let arguments = effects
            .origin_local(
                arguments_len.and_then(|count| count.checked_mul(3)?.checked_add(4)),
                arguments_len
                    .and_then(collected_bytes::<Type<'db>>)
                    .and_then(|bytes| bytes.checked_add(size_of::<Box<[Type<'db>]>>())),
                || {
                    self.bound_type
                        .into_iter()
                        .chain(arguments.iter().copied())
                        .collect::<Box<[_]>>()
                },
            )
            .await?;
        let mut comparisons = effects
            .origin_local(
                count.checked_add(2),
                count
                    .checked_mul(size_of::<Box<[DescriptorArgumentComparison<'db>]>>())
                    .and_then(|bytes| {
                        bytes
                            .checked_add(size_of::<Vec<Box<[DescriptorArgumentComparison<'db>]>>>())
                    }),
                || Vec::with_capacity(count),
            )
            .await?;
        for binding in &self.overloads {
            let pairs = binding.argument_matches.len().min(arguments.len());
            let comparisons_len = effects
                .origin_local(pairs.checked_add(1), Some(0), || {
                    binding
                        .argument_matches
                        .iter()
                        .take(pairs)
                        .try_fold(0usize, |count, argument| {
                            count.checked_add(argument.parameters.len())
                        })
                })
                .await?;
            let row = effects
                .origin_local(
                    comparisons_len.and_then(|count| {
                        count
                            .checked_mul(16)?
                            .checked_add(pairs.checked_mul(4)?)?
                            .checked_add(4)
                    }),
                    comparisons_len
                        .and_then(collected_bytes::<DescriptorArgumentComparison<'db>>)
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<Box<[DescriptorArgumentComparison<'db>]>>())
                        }),
                    || {
                        binding
                            .argument_matches
                            .iter()
                            .zip(&arguments)
                            .enumerate()
                            .flat_map(|(index, (matched_argument, &argument_type))| {
                                matched_argument.iter().filter_map(move |parameter| {
                                    let relation = parameter.argument_relation(
                                        binding.signature.parameters(),
                                        index,
                                        None,
                                        |_| Some(argument_type),
                                    )?;
                                    Some(DescriptorArgumentComparison {
                                        argument_index: index,
                                        argument_type: relation.argument_type,
                                        parameter_type: relation.declared_type,
                                    })
                                })
                            })
                            .collect::<Box<[_]>>()
                    },
                )
                .await?;
            effects
                .origin_local(
                    Some(1),
                    Some(size_of::<Box<[DescriptorArgumentComparison<'db>]>>()),
                    || comparisons.push(row),
                )
                .await?;
        }
        let comparisons = effects
            .origin_local(
                Some(1),
                Some(size_of::<Box<[Box<[DescriptorArgumentComparison<'db>]>]>>()),
                || comparisons.into_boxed_slice(),
            )
            .await?;
        let mut signatures = effects
            .origin_local(
                count.checked_add(2),
                if count > 1 {
                    count.checked_mul(size_of::<Signature<'db>>())
                } else {
                    Some(0)
                }
                .and_then(|bytes| bytes.checked_add(size_of::<SmallVec<[Signature<'db>; 1]>>())),
                || SmallVec::<[Signature<'db>; 1]>::with_capacity(count),
            )
            .await?;
        for overload in &self.overloads {
            let signature = effects.clone_signature(&overload.signature).await?;
            effects
                .origin_local(Some(1), Some(size_of::<Signature<'db>>()), || {
                    signatures.push(signature)
                })
                .await?;
        }
        let selected_indexes = effects
            .origin_local(
                selected
                    .len()
                    .checked_mul(3)
                    .and_then(|work| work.checked_add(4)),
                collected_bytes::<usize>(selected.len())
                    .and_then(|bytes| bytes.checked_add(size_of::<Box<[usize]>>())),
                || {
                    selected
                        .iter()
                        .map(|overload| overload.source_overload_index())
                        .collect::<Box<[_]>>()
                },
            )
            .await?;
        let failed = effects
            .origin_local(selection_work, Some(0), || self.has_binding_errors())
            .await?;
        let signatures = effects
            .origin_local(Some(1), Some(size_of::<CallableSignature<'db>>()), || {
                CallableSignature {
                    overloads: signatures,
                }
            })
            .await?;
        let dispatch = effects
            .dispatch(
                db,
                signatures,
                arguments,
                comparisons,
                selected_indexes,
                failed,
            )
            .await?;
        let dispatches = effects
            .origin_local(
                Some(2),
                Some(
                    size_of::<DescriptorDispatch<'db>>()
                        + size_of::<Box<[DescriptorDispatch<'db>]>>(),
                ),
                || Box::from([dispatch]),
            )
            .await?;
        let dispatches = Some(effects.dispatches(db, dispatches).await?);
        let return_contains_recursive_recovery = effects
            .origin_local(
                selected
                    .len()
                    .checked_mul(3)
                    .and_then(|work| work.checked_add(1)),
                Some(0),
                || {
                    selected.iter().any(|overload| {
                        overload.signature.is_recursion_recovery()
                            || overload.return_origin.return_contains_recursive_recovery
                    })
                },
            )
            .await?;
        let dispatch_origin = effects
            .origin_local(Some(2), Some(size_of::<DescriptorOrigin<'db>>()), || {
                DescriptorOrigin {
                    dispatches,
                    incomplete: self.overloads.is_empty(),
                    return_contains_recursive_recovery: false,
                }
            })
            .await?;
        let mut origin = effects
            .merge_origin(db, self.descriptor_origin, dispatch_origin)
            .await?;
        for overload in selected {
            origin = effects
                .merge_origin(db, origin, overload.return_origin)
                .await?;
        }
        origin.return_contains_recursive_recovery = return_contains_recursive_recovery;
        Ok(origin)
    }
}
