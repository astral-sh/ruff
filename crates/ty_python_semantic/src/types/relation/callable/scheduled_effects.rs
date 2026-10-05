//! Admitted signature effects for the queued relation experiment.
//!
//! Structural combinations and folds use the ordinary inline provider. This experiment does
//! not supervise those operations through its relation queue.

use std::future::{Future, ready};
use std::ops::ControlFlow;

use crate::types::callable::scheduled_probe::{Boundary, Router};
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::generics::GenericContext;
use crate::types::relation::TypeRelationChecker;
use crate::types::signatures::effects::{
    self, ConstraintBound, LegacyInlineEffects, SignatureEffect, SignatureEffects, SignatureVisit,
};
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{BoundTypeVarInstance, Parameter, Parameters, Signature, Type, UnionBuilder};
use crate::{Db, ProgramEnvironment};

use super::scheduled_requests::{RelationKey, RelationRequest};

pub(super) struct QueuedSignatureEffects<'eval, 'db, 'c> {
    pub(super) router: &'eval Router<'db, 'c>,
    pub(super) parent: RelationKey<'db>,
    pub(super) env: &'eval ProgramEnvironment<'db>,
}

impl effects::sealed::Sealed for QueuedSignatureEffects<'_, '_, '_> {}

impl<'state, 'db, 'c> SignatureEffects<'state, 'db, 'c> for QueuedSignatureEffects<'_, 'db, 'c> {
    type Error = Boundary;

    async fn combine_constraints(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Boundary> {
        LegacyInlineEffects
            .combine_constraints(db, builder, kind, left, right)
            .await
            .map_err(|never| match never {})
    }

    async fn push_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Boundary> {
        LegacyInlineEffects
            .push_constraints(db, fold, next)
            .await
            .map_err(|never| match never {})
    }

    async fn finish_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Boundary> {
        LegacyInlineEffects
            .finish_constraints(db, fold)
            .await
            .map_err(|never| match never {})
    }

    fn begin_signature_visit<'visit>(
        &self,
        _checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> impl Future<Output = Result<SignatureVisit<'visit, 'db>, Boundary>> {
        ready(
            if source.definition().is_none() && target.definition().is_none() {
                Ok(SignatureVisit::Untracked)
            } else {
                Err(Boundary::SignatureEffect(SignatureEffect::SignatureScope))
            },
        )
    }

    async fn relate(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Boundary> {
        if !std::ptr::eq(self.env, checker.env) {
            return Err(Boundary::RelationContext);
        }
        let mut request = RelationRequest::inherit(checker, source, target)?;
        let lease = checker
            .context_tree
            .as_ref()
            .map(|context| {
                let (seed, lease) = context.snapshot_with_lease();
                request = request.with_seed(self.router.intern_diagnostic_seed(seed)?);
                Ok::<_, Boundary>(lease)
            })
            .transpose()?;
        let answer = self.router.relation_demand(self.parent, request).await?;
        if let (Some(context), Some(lease)) = (checker.context_tree.as_ref(), lease) {
            let completed = answer.context.ok_or(Boundary::DiagnosticSeed)?;
            context
                .commit(&completed, &lease)
                .map_err(|_| Boundary::DiagnosticConflict)?;
        }
        Ok(answer.constraints)
    }

    fn disjoint(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(SignatureEffect::Disjoint)))
    }

    fn is_never(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Boundary>> {
        ready(
            if constraints.is_trivially_never_satisfied()
                || constraints.is_trivially_always_satisfied()
            {
                Ok(constraints.is_trivially_never_satisfied())
            } else {
                Err(Boundary::SignatureEffect(
                    SignatureEffect::ConstraintSatisfiability,
                ))
            },
        )
    }

    fn is_always(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Boundary>> {
        ready(
            if constraints.is_trivially_never_satisfied()
                || constraints.is_trivially_always_satisfied()
            {
                Ok(constraints.is_trivially_always_satisfied())
            } else {
                Err(Boundary::SignatureEffect(
                    SignatureEffect::ConstraintSatisfiability,
                ))
            },
        )
    }

    fn constraint_bound(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _kind: ConstraintBound,
        _typevar: BoundTypeVarInstance<'db>,
        _bound: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::ConstraintConstruction,
        )))
    }

    fn receiver_constraints(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Boundary>> {
        ready(if signature.receiver_constraints().is_none() {
            Ok(checker.always())
        } else {
            Err(Boundary::SignatureEffect(
                SignatureEffect::ReceiverConstraints,
            ))
        })
    }

    fn reduce_inferable(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Boundary>> {
        ready(if inferable == TypeVarSet::None {
            Ok(constraints)
        } else {
            Err(Boundary::SignatureEffect(
                SignatureEffect::ConstraintReduction,
            ))
        })
    }

    fn max_freshness(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _signature: &Signature<'db>,
        _context: GenericContext<'db>,
    ) -> impl Future<Output = Result<Option<TypeVarNonce>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::SignatureFreshness,
        )))
    }

    fn freshen_signature(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _signature: &Signature<'db>,
        _delta: u32,
    ) -> impl Future<Output = Result<Signature<'db>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::SignatureFreshening,
        )))
    }

    fn signature_typevars(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> impl Future<Output = Result<TypeVarSet<'db>, Boundary>> {
        ready(if signature.generic_context.is_none() {
            Ok(TypeVarSet::None)
        } else {
            Err(Boundary::SignatureEffect(
                SignatureEffect::SignatureTypevars,
            ))
        })
    }

    fn aggregate_candidate(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::AggregateTypeInspection,
        )))
    }

    fn union_add(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::UnionNormalization,
        )))
    }

    fn union_build(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::UnionNormalization,
        )))
    }

    fn resolve_alias(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(
            if matches!(
                ty,
                Type::TypeAlias(_) | Type::Recursive(_) | Type::RecursiveVar(_)
            ) {
                Err(Boundary::SignatureEffect(SignatureEffect::AliasResolution))
            } else {
                Ok(ty)
            },
        )
    }

    fn parameter_contains_typevar(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _parameters: &Parameters<'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::ParameterTypeInspection,
        )))
    }

    fn expand_parameters(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> impl Future<Output = Result<Parameters<'db>, Boundary>> {
        ready(
            if parameters
                .iter()
                .all(|parameter| !parameter.has_starred_annotation())
            {
                Ok(parameters.clone())
            } else {
                Err(Boundary::SignatureEffect(
                    SignatureEffect::ParameterExpansion,
                ))
            },
        )
    }

    fn normalize_variadic_parameters(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Parameters<'db>,
        target: Parameters<'db>,
    ) -> impl Future<Output = Result<(Parameters<'db>, Parameters<'db>), Boundary>> {
        ready(
            if source.variadic().is_none() || target.variadic().is_none() {
                Ok((source, target))
            } else {
                Err(Boundary::SignatureEffect(
                    SignatureEffect::VariadicNormalization,
                ))
            },
        )
    }

    fn empty_tuple(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::TupleNormalization,
        )))
    }

    fn tuple_from_parameters<'p>(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>>
    where
        'db: 'p,
    {
        ready(Err(Boundary::SignatureEffect(
            SignatureEffect::TupleNormalization,
        )))
    }
}
