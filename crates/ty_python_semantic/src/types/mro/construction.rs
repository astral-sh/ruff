//! Shared static MRO construction with explicit declaration reads and semantic dependencies.

use crate::types::mro::field_reads::MroFieldReads;

use std::collections::VecDeque;
use std::convert::Infallible;

use ty_python_core::scope::ScopeId;

use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::base::{
    InlineBaseMroEffects, collect_base_mro_sync, collect_single_base_mro_sync,
};
use crate::types::mro::root::{InlineMroRootEffects, apply_optional_class_specialization_sync};
use crate::types::{
    ClassLiteral, ClassType, KnownInstanceType, SpecialFormType, StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

use super::{Mro, StaticMroError, StaticMroErrorKind, c3_merge};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum StaticMroWork {
    Begin,
    RootRequest,
    ExplicitBases,
    KnownClass,
    Pep695Classification,
    ObjectBase,
    RawBaseAdvance,
    ConvertBase { index: usize },
    GenericProtocolScan { len: usize },
    GenericAliasScan { len: usize },
    ResolvedBaseAppend { prefix_len: usize, capacity: usize },
    InvalidBaseAppend { prefix_len: usize, capacity: usize },
    InvalidBasesBox { len: usize, capacity: usize },
    BaseCycleDispatch,
    StaticCycleRequest,
    SingleBaseCollectionRequest,
    SequenceStart { bases: usize },
    ResolvedBaseAdvance,
    BaseCollectionRequest,
    SequenceAppend { prefix_len: usize, capacity: usize },
    DirectSequenceCapacity { len: usize },
    DirectBaseAdvance,
    DirectBaseSpecializationRequest,
    DirectBaseAppend { len: usize, capacity: usize },
    C3Request,
    ErrorRequest,
    ErrorDetailsRequest,
    FixedMro { entries: usize },
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// Construction reads declaration inputs only after admitting their work.
pub(in crate::types) trait StaticMroFacts<'db>: sealed::Sealed {
    type Error;
}

pub(in crate::types) trait StaticMroEffects<'db>: StaticMroFacts<'db> {
    async fn body_scope(&self, class: StaticClassLiteral<'db>)
    -> Result<ScopeId<'db>, Self::Error>;
    async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
    async fn explicit_bases<'call>(
        &'call self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'call [Type<'db>], Self::Error>
    where
        'db: 'call;
    async fn has_pep_695_type_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;
    async fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error>;

    async fn checkpoint(&self, work: StaticMroWork) -> Result<(), Self::Error>;
    async fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Self::Error>;
    async fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Self::Error>;
    async fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Self::Error>;
    async fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error>;
    async fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Self::Error>;
    async fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Self::Error>;
    async fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Self::Error>;
    async fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Self::Error>;
}

/// Ordinary construction invokes dependencies directly without constructing futures.
pub(in crate::types) trait SynchronousStaticMroEffects<'db>:
    StaticMroFacts<'db>
{
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Self::Error>;
    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&[Type<'db>], Self::Error>;
    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Self::Error>;

    fn checkpoint(&self, work: StaticMroWork) -> Result<(), Self::Error>;
    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Self::Error>;
    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Self::Error>;
    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Self::Error>;
    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error>;
    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Self::Error>;
    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Self::Error>;
    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Self::Error>;
    fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Self::Error>;
}

/// Builds the productive inheritance-cycle seed through the same fallible dependencies as
/// ordinary MRO construction. Operational refusal propagates before any semantic fallback exists.
#[ty_mapping_probe_macros::dual_static_mro]
pub(in crate::types) async fn static_mro_cycle_with<'db, E: StaticMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    class_literal: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<StaticMroError<'db>, E::Error> {
    let _ = fields;
    effects.checkpoint(StaticMroWork::Begin).await?;
    let env = ProgramEnvironment::from_scope(effects.body_scope(class_literal).await?);
    effects.checkpoint(StaticMroWork::RootRequest).await?;
    let class = effects.root_class(class_literal, specialization).await?;
    effects.checkpoint(StaticMroWork::ErrorRequest).await?;
    let error = effects
        .make_error(&env, class, StaticMroErrorKind::InheritanceCycle)
        .await?;
    effects.checkpoint(StaticMroWork::Publish).await?;
    Ok(error)
}

#[ty_mapping_probe_macros::dual_static_mro]
#[inline]
pub(in crate::types) async fn static_mro_with<'db, E: StaticMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    class_literal: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    effects: &E,
) -> Result<Result<Mro<'db>, StaticMroError<'db>>, E::Error> {
    effects.checkpoint(StaticMroWork::Begin).await?;
    let env = &ProgramEnvironment::from_scope(effects.body_scope(class_literal).await?);
    effects.checkpoint(StaticMroWork::RootRequest).await?;
    let class = effects.root_class(class_literal, specialization).await?;

    effects.checkpoint(StaticMroWork::ExplicitBases).await?;
    let original_bases = effects.explicit_bases(class_literal).await?;

    let result = match original_bases {
        // `builtins.object` is the special case:
        // the only class in Python that has an MRO with length <2
        [] if {
            effects.checkpoint(StaticMroWork::KnownClass).await?;
            effects.is_object(class).await?
        } =>
        {
            effects
                .checkpoint(StaticMroWork::FixedMro { entries: 1 })
                .await?;
            Ok(Mro::from([
                // object is not generic, so the default specialization should be a no-op
                ClassBase::Class(class),
            ]))
        }

        // All other classes in Python have an MRO with length >=2.
        // Even if a class has no explicit base classes,
        // it will implicitly inherit from `object` at runtime;
        // `object` will appear in the class's `__bases__` list and `__mro__`:
        //
        // ```pycon
        // >>> class Foo: ...
        // ...
        // >>> Foo.__bases__
        // (<class 'object'>,)
        // >>> Foo.__mro__
        // (<class '__main__.Foo'>, <class 'object'>)
        // ```
        [] => {
            effects
                .checkpoint(StaticMroWork::Pep695Classification)
                .await?;
            // e.g. `class Foo[T]: ...` implicitly has `Generic` inserted into its bases
            if effects.has_pep_695_type_params(class_literal).await? {
                effects.checkpoint(StaticMroWork::ObjectBase).await?;
                let object = effects.object_base(env).await?;
                effects
                    .checkpoint(StaticMroWork::FixedMro { entries: 3 })
                    .await?;
                Ok(Mro::from([
                    ClassBase::Class(class),
                    ClassBase::Generic,
                    object,
                ]))
            } else {
                effects.checkpoint(StaticMroWork::ObjectBase).await?;
                let object = effects.object_base(env).await?;
                effects
                    .checkpoint(StaticMroWork::FixedMro { entries: 2 })
                    .await?;
                Ok(Mro::from([ClassBase::Class(class), object]))
            }
        }

        // Fast path for a class that has only a single explicit base.
        //
        // This *could* theoretically be handled by the final branch below,
        // but it's a common case (i.e., worth optimizing for),
        // and the `c3_merge` function requires lots of allocations.
        [single_base]
            if {
                effects
                    .checkpoint(StaticMroWork::Pep695Classification)
                    .await?;
                !effects.has_pep_695_type_params(class_literal).await?
                    && !matches!(
                        single_base,
                        Type::GenericAlias(_)
                            | Type::KnownInstance(
                                KnownInstanceType::SubscriptedGeneric(_)
                                    | KnownInstanceType::SubscriptedProtocol(_)
                            )
                    )
            } =>
        {
            effects
                .checkpoint(StaticMroWork::ConvertBase { index: 0 })
                .await?;
            match effects
                .converted_explicit_base(env, class_literal, 0, *single_base)
                .await?
            {
                None => {
                    effects
                        .checkpoint(StaticMroWork::InvalidBasesBox { len: 1, capacity: 0 })
                        .await?;
                    let kind = StaticMroErrorKind::InvalidBases(Box::from([(0, *single_base)]));
                    effects.checkpoint(StaticMroWork::ErrorRequest).await?;
                    Err(effects.make_error(env, class, kind).await?)
                }
                Some(single_base) => {
                    if base_has_cyclic_mro_with(fields, single_base, effects).await? {
                        effects.checkpoint(StaticMroWork::ErrorRequest).await?;
                        Err(effects
                            .make_error(env, class, StaticMroErrorKind::InheritanceCycle)
                            .await?)
                    } else {
                        effects
                            .checkpoint(StaticMroWork::SingleBaseCollectionRequest)
                            .await?;
                        Ok(effects
                            .collect_single_base_mro(env, class, single_base, specialization)
                            .await?)
                    }
                }
            }
        }

        // The class has multiple explicit bases.
        //
        // We'll fallback to a full implementation of the C3-merge algorithm to determine
        // what MRO Python will give this class at runtime
        // (if an MRO is indeed resolvable at all!)
        _ => {
            let mut resolved_bases = Vec::new();
            let mut invalid_bases = Vec::new();

            let mut bases = original_bases.iter().enumerate();
            loop {
                effects.checkpoint(StaticMroWork::RawBaseAdvance).await?;
                let Some((i, base)) = bases.next() else {
                    break;
                };
                // Note that we emit a diagnostic for inheriting from bare (unsubscripted) `Generic` elsewhere
                // (see `infer::TypeInferenceBuilder::check_class_definitions`),
                // which is why we only care about `KnownInstanceType::Generic(Some(_))`,
                // not `KnownInstanceType::Generic(None)`.
                if let Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_)) = base {
                    maybe_add_generic_with(
                        &mut resolved_bases,
                        original_bases,
                        &original_bases[i + 1..],
                        effects,
                    )
                    .await?;
                } else {
                    effects
                        .checkpoint(StaticMroWork::ConvertBase { index: i })
                        .await?;
                    match effects
                        .converted_explicit_base(env, class_literal, i, *base)
                        .await?
                    {
                        Some(valid_base) => {
                            effects
                                .checkpoint(StaticMroWork::ResolvedBaseAppend {
                                    prefix_len: resolved_bases.len(),
                                    capacity: resolved_bases.capacity(),
                                })
                                .await?;
                            resolved_bases.push(valid_base);
                        }
                        None => {
                            effects
                                .checkpoint(StaticMroWork::InvalidBaseAppend {
                                    prefix_len: invalid_bases.len(),
                                    capacity: invalid_bases.capacity(),
                                })
                                .await?;
                            invalid_bases.push((i, *base));
                        }
                    }
                }
            }

            if !invalid_bases.is_empty() {
                effects
                    .checkpoint(StaticMroWork::InvalidBasesBox {
                        len: invalid_bases.len(),
                        capacity: invalid_bases.capacity(),
                    })
                    .await?;
                let kind = StaticMroErrorKind::InvalidBases(invalid_bases.into_boxed_slice());
                effects.checkpoint(StaticMroWork::ErrorRequest).await?;
                let error = effects.make_error(env, class, kind).await?;
                effects.checkpoint(StaticMroWork::Publish).await?;
                return Ok(Err(error));
            }

            // `Generic` is implicitly added to the bases list of a class that has PEP-695 type parameters
            // (documented at https://docs.python.org/3/reference/compound_stmts.html#generic-classes)
            effects
                .checkpoint(StaticMroWork::Pep695Classification)
                .await?;
            if effects.has_pep_695_type_params(class_literal).await? {
                maybe_add_generic_with(&mut resolved_bases, original_bases, &[], effects).await?;
            }

            effects
                .checkpoint(StaticMroWork::SequenceStart {
                    bases: resolved_bases.len(),
                })
                .await?;
            let mut seqs = Vec::with_capacity(resolved_bases.len() + 2);
            seqs.push(VecDeque::from([ClassBase::Class(class)]));
            let mut bases = resolved_bases.iter();
            loop {
                effects
                    .checkpoint(StaticMroWork::ResolvedBaseAdvance)
                    .await?;
                let Some(base) = bases.next() else {
                    break;
                };
                if base_has_cyclic_mro_with(fields, *base, effects).await? {
                    effects.checkpoint(StaticMroWork::ErrorRequest).await?;
                    let error = effects
                        .make_error(env, class, StaticMroErrorKind::InheritanceCycle)
                        .await?;
                    effects.checkpoint(StaticMroWork::Publish).await?;
                    return Ok(Err(error));
                }
                effects
                    .checkpoint(StaticMroWork::BaseCollectionRequest)
                    .await?;
                let sequence = effects.collect_base_mro(env, *base, specialization).await?;
                effects
                    .checkpoint(StaticMroWork::SequenceAppend {
                        prefix_len: seqs.len(),
                        capacity: seqs.capacity(),
                    })
                    .await?;
                seqs.push(sequence);
            }

            effects
                .checkpoint(StaticMroWork::DirectSequenceCapacity {
                    len: resolved_bases.len(),
                })
                .await?;
            let mut direct_bases = VecDeque::with_capacity(resolved_bases.len());
            let mut bases = resolved_bases.iter();
            loop {
                effects.checkpoint(StaticMroWork::DirectBaseAdvance).await?;
                let Some(base) = bases.next() else {
                    break;
                };
                effects
                    .checkpoint(StaticMroWork::DirectBaseSpecializationRequest)
                    .await?;
                let base = effects.specialize_base(*base, specialization).await?;
                effects
                    .checkpoint(StaticMroWork::DirectBaseAppend {
                        len: direct_bases.len(),
                        capacity: direct_bases.capacity(),
                    })
                    .await?;
                direct_bases.push_back(base);
            }
            effects
                .checkpoint(StaticMroWork::SequenceAppend {
                    prefix_len: seqs.len(),
                    capacity: seqs.capacity(),
                })
                .await?;
            seqs.push(direct_bases);

            effects.checkpoint(StaticMroWork::C3Request).await?;
            if let Some(mro) = effects.c3_merge(seqs).await? {
                Ok(mro)
            } else {
                effects
                    .checkpoint(StaticMroWork::ErrorDetailsRequest)
                    .await?;
                effects
                    .failed_c3(env, class_literal, class, original_bases, &resolved_bases)
                    .await?
            }
        }
    };
    effects.checkpoint(StaticMroWork::Publish).await?;
    Ok(result)
}

/// Possibly add `Generic` to the resolved bases list.
///
/// This function is called in two cases:
/// - If we encounter a subscripted `Generic` in the original bases list
///   (`Generic[T]` or similar)
/// - If the class has PEP-695 type parameters,
///   `Generic` is [implicitly appended] to the bases list at runtime
///
/// Whether or not `Generic` is added to the bases list depends on:
/// - Whether `Protocol` is present in the original bases list
/// - Whether any of the bases yet to be visited in the original bases list
///   is a generic alias (which would therefore have `Generic` in its MRO)
///
/// This function emulates the behavior of `typing._GenericAlias.__mro_entries__` at
/// <https://github.com/python/cpython/blob/ad42dc1909bdf8ec775b63fb22ed48ff42797a17/Lib/typing.py#L1487-L1500>.
///
/// [implicitly inherits]: https://docs.python.org/3/reference/compound_stmts.html#generic-classes
#[ty_mapping_probe_macros::dual_static_mro]
#[inline]
pub(in crate::types) async fn maybe_add_generic_with<'db, E: StaticMroEffects<'db>>(
    resolved_bases: &mut Vec<ClassBase<'db>>,
    original_bases: &[Type<'db>],
    remaining_bases: &[Type<'db>],
    effects: &E,
) -> Result<(), E::Error> {
    effects
        .checkpoint(StaticMroWork::GenericProtocolScan {
            len: original_bases.len(),
        })
        .await?;
    if original_bases.contains(&Type::SpecialForm(SpecialFormType::Protocol)) {
        return Ok(());
    }
    effects
        .checkpoint(StaticMroWork::GenericAliasScan {
            len: remaining_bases.len(),
        })
        .await?;
    if remaining_bases.iter().any(Type::is_generic_alias) {
        return Ok(());
    }
    effects
        .checkpoint(StaticMroWork::ResolvedBaseAppend {
            prefix_len: resolved_bases.len(),
            capacity: resolved_bases.capacity(),
        })
        .await?;
    resolved_bases.push(ClassBase::Generic);
    Ok(())
}

#[ty_mapping_probe_macros::dual_static_mro]
#[inline]
pub(in crate::types) async fn base_has_cyclic_mro_with<'db, E: StaticMroEffects<'db>>(
    fields: MroFieldReads<'db>,
    base: ClassBase<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    let _ = fields;
    effects.checkpoint(StaticMroWork::BaseCycleDispatch).await?;
    match base {
        ClassBase::Class(class) => {
            let Some((class_literal, specialization)) = effects.static_class_literal(class).await? else {
                // Dynamic classes can't have cyclic MRO since their bases must
                // already exist at creation time. Unlike statement classes, we do not
                // permit dynamic classes to have forward references in their
                // bases list.
                return Ok(false);
            };
            effects
                .checkpoint(StaticMroWork::StaticCycleRequest)
                .await?;
            effects
                .static_mro_is_cycle(class_literal, specialization)
                .await
        }
        ClassBase::Any
        | ClassBase::Dynamic(_)
        | ClassBase::Divergent(_)
        | ClassBase::Generic
        | ClassBase::Protocol
        | ClassBase::TypedDict(_) => Ok(false),
    }
}

pub(in crate::types) struct InlineStaticMroEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineStaticMroEffects<'db> {
    #[inline]
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineStaticMroEffects<'_> {}

impl<'db> StaticMroFacts<'db> for InlineStaticMroEffects<'db> {
    type Error = Infallible;
}

impl<'db> SynchronousStaticMroEffects<'db> for InlineStaticMroEffects<'db> {
    #[inline]
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Infallible> {
        Ok(class.body_scope(self.db))
    }

    #[inline]
    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(MroFieldReads::new(self.db).is_object(class))
    }

    #[inline]
    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Infallible> {
        Ok(MroFieldReads::new(self.db).static_class_literal(class))
    }

    #[inline]
    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&[Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    #[inline]
    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Infallible> {
        Ok(class.has_pep_695_type_params(self.db))
    }

    #[inline]
    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        _index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(ClassBase::try_from_explicit_base(
            self.db,
            env,
            ty,
            Some(ClassLiteral::Static(class)),
        ))
    }

    #[inline]
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Infallible> {
        Ok(ClassBase::object(self.db, env))
    }

    #[inline]
    fn checkpoint(&self, _work: StaticMroWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Infallible> {
        apply_optional_class_specialization_sync(
            self.db,
            class,
            specialization,
            &InlineMroRootEffects::new(self.db),
        )
    }

    #[inline]
    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Infallible> {
        Ok(class
            .try_mro(self.db, specialization)
            .is_err_and(StaticMroError::is_cycle))
    }

    #[inline]
    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Infallible> {
        collect_single_base_mro_sync(
            self.db,
            env,
            root,
            base,
            additional,
            &InlineBaseMroEffects::new(self.db),
        )
    }

    #[inline]
    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Infallible> {
        collect_base_mro_sync(
            self.db,
            env,
            base,
            additional,
            &InlineBaseMroEffects::new(self.db),
        )
    }

    #[inline]
    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Infallible> {
        Ok(base.apply_optional_specialization(self.db, specialization))
    }

    #[inline]
    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Infallible> {
        Ok(c3_merge(self.db, sequences))
    }

    #[inline]
    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Infallible> {
        Ok(kind.into_mro_error(self.db, env, class))
    }

    #[inline]
    fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Infallible> {
        Ok(Mro::static_error_details(
            self.db,
            env,
            class_literal,
            class,
            original_bases,
            resolved_bases,
        ))
    }
}
