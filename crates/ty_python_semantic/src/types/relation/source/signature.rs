//! Callable comparisons borrow the caller's checker and retain its existing child execution owner.

use std::borrow::Cow;
use std::ops::ControlFlow;
use std::slice::Iter;

use salsa::execution_probe::{BorrowOrCopy, ExecutionWork, RunError, RunResult};

use super::retained::PairChildren;
use super::{BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::Db;
use crate::types::callable::{CallableType, CallableTypes};
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::generics::GenericContext;
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::signatures::{ConcatenateTail, ParametersKind, SignatureRelationKey};
use crate::types::signatures::variadic::{VariadicNormalizationEffects, normalize_variadic_parameters_with};
use crate::types::type_alias::AliasResolutionStep;
use crate::types::signatures::effects::{
    ConstraintBound, SignatureEffects, SignatureVisit, sealed,
};
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{
    BoundTypeVarInstance, CallableSignature, KnownClass, Parameter, ParameterKind, Parameters,
    Signature, Type, UnionBuilder,
};

#[cfg(test)]
pub(in crate::types) mod observations;

pub(super) struct SourceSignatureEffects<'pairs, 'effects, 'run, 'db: 'run, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'c, E, P>
    SourceSignatureEffects<'pairs, 'effects, 'run, 'db, 'c, E, P>
{
    pub(super) fn new(pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>) -> Self {
        Self { pairs }
    }
}

impl<E, P> sealed::Sealed for SourceSignatureEffects<'_, '_, '_, '_, '_, E, P> {}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    pub(super) async fn compare_callables(
        &self,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        sources: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                checker
                    .check_callables_vs_callable_with(
                        self.db,
                        &SourceSignatureEffects::new(self),
                        sources,
                        target,
                    )
                    .await
            })
            .await)
    }
}

impl<
    'state,
    'run,
    'db: 'run,
    'c,
    E: RelationSourceEffects<'run, 'db>,
    P: PairChildren<'run, 'db, 'c>,
> SignatureEffects<'state, 'db, 'c> for SourceSignatureEffects<'_, '_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let work = work.and_then(|work| work.checked_add(3)).ok_or(RunError::Contract(
            "signature comparison work quotation overflow",
        ))?;
        let requested_bytes = requested_bytes
            .and_then(|bytes| bytes.checked_add(size_of_val(&action)))
            .and_then(|bytes| bytes.checked_add(size_of::<Option<RunResult<T>>>()))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>()))
            .ok_or(RunError::Contract("signature comparison byte quotation overflow"))?;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(work)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                #[cfg(test)]
                observations::admitted(self.pairs.db);
                self.pairs.endpoint.check_completion()?;
                Ok(action())
            })
            .await)
    }

    async fn callable_runtime_class(
        &self,
        _db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.pairs.callable_runtime_class(callable).await
    }

    async fn callable_signatures(
        &self,
        _db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        Ok(self
            .pairs
            .endpoint
            .read_field(
                callable
                    .field_requests(self.pairs.endpoint.field_request_context())
                    .signatures(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn signature_entry(
        &self,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: &CallableSignature<'db>,
        target: &CallableSignature<'db>,
    ) -> RunResult<()> {
        let gate = self
            .local(Some(8), Some(0), || {
                if checker.report_context().is_some() {
                    Some(RelationSourceOperation::SignatureContext)
                } else if source.overloads.len() != 1 || target.overloads.len() != 1 {
                    Some(RelationSourceOperation::SignatureOverloads)
                } else {
                    None
                }
            })
            .await?;
        if let Some(operation) = gate {
            return self.pairs.unavailable(operation).await;
        }
        let signatures = self
            .local(Some(2), Some(0), || source.overloads.iter().chain(&target.overloads))
            .await?;
        for signature in signatures {
            let (gate, count, standard) = self
                .local(Some(8), Some(0), || {
                    let gate = if signature.generic_context.is_some() {
                        Some(RelationSourceOperation::SignatureGeneric)
                    } else if signature.receiver_constraints().is_some() {
                        Some(RelationSourceOperation::SignatureReceiver)
                    } else {
                        match signature.parameters().kind() {
                            ParametersKind::Standard
                            | ParametersKind::Top
                            | ParametersKind::Gradual
                            | ParametersKind::Concatenate(ConcatenateTail::Gradual) => None,
                            ParametersKind::ParamSpec(_)
                            | ParametersKind::Concatenate(ConcatenateTail::ParamSpec(_)) => {
                                Some(RelationSourceOperation::SignatureVariadic)
                            }
                        }
                    };
                    (gate, signature.parameters().len(), signature.parameters().is_standard())
                })
                .await?;
            if let Some(operation) = gate {
                return self.pairs.unavailable(operation).await;
            }
            let gate = self
                .local(
                    count.checked_add(1).and_then(|count| count.checked_mul(8)),
                    Some(size_of::<Iter<'_, Parameter<'db>>>() + size_of::<bool>()),
                    || {
                        let mut variadic_seen = false;
                        signature.parameters().iter().find_map(|parameter| {
                            if parameter.has_starred_annotation() {
                                return Some(RelationSourceOperation::SignatureVariadic);
                            }
                            match parameter.kind() {
                                ParameterKind::KeywordOnly { .. } => {
                                    Some(RelationSourceOperation::SignatureKeywordParameters)
                                }
                                ParameterKind::Variadic { .. }
                                | ParameterKind::KeywordVariadic { .. } => {
                                    variadic_seen = true;
                                    standard.then_some(RelationSourceOperation::SignatureVariadic)
                                }
                                ParameterKind::PositionalOnly { .. }
                                | ParameterKind::PositionalOrKeyword { .. } => {
                                    variadic_seen.then_some(RelationSourceOperation::SignatureVariadic)
                                }
                            }
                        })
                    },
                )
                .await?;
            if let Some(operation) = gate {
                return self.pairs.unavailable(operation).await;
            }
        }
        Ok(())
    }

    async fn merge_typevars(
        &self,
        _db: &'db dyn Db,
        left: TypeVarSet<'db>,
        right: TypeVarSet<'db>,
    ) -> RunResult<TypeVarSet<'db>> {
        let value = self
            .local(Some(2), Some(size_of::<TypeVarSet<'db>>()), || match (left, right) {
                (TypeVarSet::None, other) | (other, TypeVarSet::None) => Some(other),
                _ => None,
            })
            .await?;
        match value {
            Some(value) => Ok(value),
            None => {
                self.pairs
                    .unavailable(RelationSourceOperation::SignatureTypevars)
                    .await
            }
        }
    }

    async fn signature_checker<'checker>(
        &self,
        checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
        inferable: TypeVarSet<'db>,
        has_receiver_constraints: bool,
    ) -> RunResult<Cow<'checker, TypeRelationChecker<'state, 'c, 'db>>> {
        if self
            .local(Some(2), Some(size_of::<Cow<'_, TypeRelationChecker<'state, 'c, 'db>>>()), || {
                inferable == checker.inferable && !has_receiver_constraints
            })
            .await?
        {
            Ok(Cow::Borrowed(checker))
        } else {
            self.pairs
                .unavailable(RelationSourceOperation::SignatureChecker)
                .await
        }
    }

    async fn parameter_exemption(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        target: Type<'db>,
        source: Type<'db>,
    ) -> RunResult<bool> {
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::BeforePrefix);
        if self
            .local(Some(2), Some(0), || {
                matches!((target, source), (Type::TypeVar(_), Type::TypeVar(_)))
            })
            .await?
        {
            self.pairs
                .unavailable(RelationSourceOperation::SignatureParameterTypevar)
                .await
        } else {
            Ok(false)
        }
    }

    async fn combine_constraints(
        &self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .combine_constraints(builder, kind, left, right)
            .await
    }

    async fn push_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        self.pairs.push_constraints(fold, next).await
    }

    async fn finish_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs.finish_constraints(fold).await
    }

    async fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> RunResult<SignatureVisit<'visit, 'db>> {
        let key = self
            .local(Some(8), Some(size_of::<SignatureVisit<'_, 'db>>()), || {
                SignatureRelationKey::from_signatures(
                    source,
                    target,
                    checker.relation,
                    checker.typevar_evaluation,
                )
            })
            .await?;
        if key.is_some() {
            self.pairs
                .unavailable(RelationSourceOperation::SignatureScope)
                .await
        } else {
            Ok(SignatureVisit::Untracked)
        }
    }

    async fn relate(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs.check_type_pair(checker, source, target).await
    }

    async fn disjoint(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .children
            .derived_disjoint_pair(self.pairs.db, self.pairs.effects, checker, source, target)
            .await
    }

    async fn is_never(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.pairs.satisfy(constraints, false).await
    }

    async fn is_always(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.pairs.satisfy(constraints, true).await
    }

    async fn constraint_bound(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _kind: ConstraintBound,
        _typevar: BoundTypeVarInstance<'db>,
        _bound: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureTypevars)
            .await
    }

    async fn receiver_constraints(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let value = self
            .local(Some(3), Some(size_of::<ConstraintSet<'db, 'c>>()), || {
                signature
                    .receiver_constraints()
                    .is_none()
                    .then(|| checker.always())
            })
            .await?;
        match value {
            Some(value) => Ok(value),
            None => {
                self.pairs
                    .unavailable(RelationSourceOperation::SignatureReceiver)
                    .await
            }
        }
    }

    async fn reduce_inferable(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let unchanged = self
            .local(Some(2), Some(size_of::<ConstraintSet<'db, 'c>>()), || {
                constraints.verify_builder(checker.constraints);
                inferable == TypeVarSet::None
            })
            .await?;
        if unchanged {
            Ok(constraints)
        } else {
            self.pairs
                .unavailable(RelationSourceOperation::SignatureTypevars)
                .await
        }
    }

    async fn max_freshness(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _signature: &Signature<'db>,
        _context: GenericContext<'db>,
    ) -> RunResult<Option<TypeVarNonce>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureGeneric)
            .await
    }

    async fn freshen_signature(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _signature: &Signature<'db>,
        _delta: u32,
    ) -> RunResult<Signature<'db>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureGeneric)
            .await
    }

    async fn signature_typevars(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> RunResult<TypeVarSet<'db>> {
        if self
            .local(Some(1), Some(size_of::<TypeVarSet<'db>>()), || signature.generic_context.is_none())
            .await?
        {
            Ok(TypeVarSet::None)
        } else {
            self.pairs
                .unavailable(RelationSourceOperation::SignatureGeneric)
                .await
        }
    }

    async fn aggregate_candidate(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureAggregate)
            .await
    }

    async fn union_add(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureAggregate)
            .await
    }

    async fn union_build(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _builder: UnionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureAggregate)
            .await
    }

    async fn resolve_alias(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.resolve_normalization_alias(ty).await
    }

    async fn parameter_contains_typevar(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _parameters: &Parameters<'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureTypevars)
            .await
    }

    async fn expand_parameters(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> RunResult<Parameters<'db>> {
        let count = self.local(Some(1), Some(0), || parameters.len()).await?;
        let gate = self
            .local(
                count.checked_add(1).and_then(|count| count.checked_mul(4)),
                Some(size_of::<Iter<'_, Parameter<'db>>>()),
                || parameters.iter().any(|parameter| {
                    parameter.is_variadic() && parameter.has_starred_annotation()
                }),
            )
            .await?;
        if gate {
            return self.pairs.unavailable(RelationSourceOperation::SignatureVariadic).await;
        }
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::BeforeClone);
        // The borrowed signatures retain the backing arrays until all comparison children drain.
        // This operation clones and later retires an Arc handle; it cannot destroy the array.
        let expanded = self.local(Some(4), Some(size_of::<RunResult<Parameters<'db>>>()), || parameters.clone()).await?;
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::AfterClone);
        Ok(expanded)
    }

    async fn normalize_variadic_parameters(
        &self,
        db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        mut source: Parameters<'db>,
        mut target: Parameters<'db>,
    ) -> RunResult<(Parameters<'db>, Parameters<'db>)> {
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::BeforeNormalize);
        self.local(Some(4), Some(size_of::<(Parameters<'db>, Parameters<'db>)>()), || ()).await?;
        normalize_variadic_parameters_with(db, &mut source, &mut target, &SourceVariadicNormalization { effects: self }).await?;
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::AfterNormalize);
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::BeforeTransfer);
        // Both owners stay in this frame until admission and the completion check succeed.
        self.local(Some(4), Some(size_of::<RunResult<(Parameters<'db>, Parameters<'db>)>>()), || ()).await?;
        #[cfg(test)]
        observations::boundary(self.pairs.db, observations::Stage::AfterTransfer);
        Ok((source, target))
    }

    async fn empty_tuple(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
    ) -> RunResult<Type<'db>> {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureTuple)
            .await
    }

    async fn tuple_from_parameters<'p>(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> RunResult<Type<'db>>
    where
        'db: 'p,
    {
        self.pairs
            .unavailable(RelationSourceOperation::SignatureTuple)
            .await
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    SourceSignatureEffects<'_, '_, 'run, 'db, 'c, E, P>
{
    /// Returns an already resolved annotation and refuses alias expansion as `SignatureAlias`.
    async fn resolve_normalization_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let step = SignatureEffects::local(self, Some(1), Some(size_of::<Type<'db>>()), || ty.alias_resolution_step()).await?;
        match step {
            AliasResolutionStep::Resolved(ty) => Ok(ty),
            AliasResolutionStep::Alias(_)
            | AliasResolutionStep::Recursive(_)
            | AliasResolutionStep::UnboundRecursiveVariable => self.pairs.unavailable(RelationSourceOperation::SignatureAlias).await,
        }
    }
}

/// Supplies the comparison's endpoint to the driver that borrows both parameter owners.
struct SourceVariadicNormalization<'signature, 'pairs, 'effects, 'run, 'db: 'run, 'c, E, P> {
    effects: &'signature SourceSignatureEffects<'pairs, 'effects, 'run, 'db, 'c, E, P>,
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    VariadicNormalizationEffects<'db> for SourceVariadicNormalization<'_, '_, '_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;

    async fn local<T>(&self, work: Option<usize>, requested_bytes: Option<usize>, action: impl FnOnce() -> T) -> RunResult<T> {
        SignatureEffects::local(self.effects, work, requested_bytes, action).await
    }

    async fn resolve_alias(&self, _db: &'db dyn Db, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.effects.resolve_normalization_alias(ty).await
    }

    async fn reorder_homogeneous_suffix(&self, _db: &'db dyn Db, _parameters: &mut Parameters<'db>, _variadic_index: usize, _suffix_len: usize) -> RunResult<()> {
        self.effects.pairs.unavailable(RelationSourceOperation::SignatureVariadic).await
    }
}
