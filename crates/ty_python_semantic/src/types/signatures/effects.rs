//! Explicit semantic dependencies of callable-signature comparison.
//!
//! The inline provider retains the synchronous checker behavior. Queued providers must admit
//! each operation explicitly: an unsupported dependency cannot become an ordinary constraint.

use std::borrow::Cow;
use std::convert::Infallible;
use std::future::{Future, ready};
use std::ops::ControlFlow;
use std::task::{Context, Poll, Waker};

use itertools::Itertools;

use super::variadic::{
    InlineVariadicNormalizationEffects, normalize_variadic_parameters_with,
};
use super::{CallableSignature, Parameter, Parameters, Signature, SignatureRelationKey};
use crate::Db;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::cyclic::ActiveRecursionGuard;
use crate::types::generics::GenericContext;
use crate::types::relation::{TypeRelationChecker, TypeVarEvaluation};
use crate::types::tuple::{TupleType, VariableSegment};
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{
    BoundTypeVarInstance, CallableType, KnownClass, Type, UnionBuilder, any_over_type,
};

pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

#[cfg(test)]
mod ownership_probe;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(crate) enum SignatureEffect {
    Relation,
    EntryShape,
    CheckerMode,
    CompactedBuilder,
    Disjoint,
    ConstraintSatisfiability,
    ConstraintConstruction,
    ReceiverConstraints,
    ConstraintReduction,
    SignatureFreshness,
    SignatureFreshening,
    SignatureTypevars,
    AggregateTypeInspection,
    UnionNormalization,
    AliasResolution,
    ParameterTypeInspection,
    ParameterExpansion,
    VariadicNormalization,
    TupleNormalization,
    SignatureScope,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConstraintBound {
    Lower,
    Upper,
    Equivalent,
}

pub(crate) type SignatureResult<'db, 'c, E> = Result<ConstraintSet<'db, 'c>, E>;

/// A live-checker provider keeps this declaration visit active until completion or cancellation.
/// Providers that reconstruct checker state need a separate graph admission policy.
pub(crate) enum SignatureVisit<'visit, 'db> {
    Untracked,
    Active {
        _guard: ActiveRecursionGuard<'visit, SignatureRelationKey<'db>>,
    },
    Cycle,
}

/// `'state` names the checker's borrowed resources, independently of a temporary checker view.
/// An effect can retain an owned clone of that view without borrowing the local checker variable.
pub(in crate::types) trait SignatureEffects<'state, 'db, 'c>:
    sealed::Sealed
{
    type Error;

    /// Runs a local action after the provider admits its work and requested storage.
    ///
    /// Controlled providers quote the closure and staged/returned result representations.
    /// Callers quote additional input, iterator, or output state in `requested_bytes`; `None`
    /// means quotation overflow and must refuse before the action. Ordinary execution ignores
    /// these quotations and performs the action synchronously.
    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn callable_runtime_class(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(callable.runtime_class(db))
    }

    async fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        Ok(callable.signatures(db))
    }

    async fn signature_entry(
        &self,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _source: &CallableSignature<'db>,
        _target: &CallableSignature<'db>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn merge_typevars(
        &self,
        db: &'db dyn Db,
        left: TypeVarSet<'db>,
        right: TypeVarSet<'db>,
    ) -> Result<TypeVarSet<'db>, Self::Error> {
        Ok(left.merge(db, right))
    }

    async fn signature_checker<'checker>(
        &self,
        checker: &'checker TypeRelationChecker<'state, 'c, 'db>,
        inferable: TypeVarSet<'db>,
        has_receiver_constraints: bool,
    ) -> Result<Cow<'checker, TypeRelationChecker<'state, 'c, 'db>>, Self::Error> {
        let mut derived = checker.with_inferable_typevars(inferable);
        if has_receiver_constraints {
            derived.typevar_evaluation = TypeVarEvaluation::Lazy;
        }
        Ok(Cow::Owned(derived))
    }

    async fn parameter_exemption(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        target: Type<'db>,
        source: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(
            matches!((target, source), (Type::TypeVar(first), Type::TypeVar(second))
            if first.paramspec_attr(db).is_some()
                && first.paramspec_attr(db) == second.paramspec_attr(db)
                && first.without_paramspec_attr(db).is_inferable(db, checker.inferable)
                && second.without_paramspec_attr(db).is_inferable(db, checker.inferable)),
        )
    }

    async fn combine_constraints(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn push_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;

    async fn finish_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> Result<SignatureVisit<'visit, 'db>, Self::Error>;

    async fn relate(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjoint(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn is_never(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn is_always(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn constraint_bound(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        kind: ConstraintBound,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn receiver_constraints(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn reduce_inferable(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn max_freshness(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
    ) -> Result<Option<TypeVarNonce>, Self::Error>;

    async fn freshen_signature(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        delta: u32,
    ) -> Result<Signature<'db>, Self::Error>;

    async fn signature_typevars(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> Result<TypeVarSet<'db>, Self::Error>;

    async fn aggregate_candidate(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn union_add(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;

    async fn union_build(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn resolve_alias(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn parameter_contains_typevar(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn expand_parameters(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> Result<Parameters<'db>, Self::Error>;

    async fn normalize_variadic_parameters(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Parameters<'db>,
        target: Parameters<'db>,
    ) -> Result<(Parameters<'db>, Parameters<'db>), Self::Error>;

    async fn empty_tuple(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn tuple_from_parameters<'p>(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> Result<Type<'db>, Self::Error>
    where
        'db: 'p;
}

pub(crate) struct LegacyInlineEffects;
impl sealed::Sealed for LegacyInlineEffects {}

/// Moves matching variadic suffixes into prefixes when doing so preserves the comparison.
pub(in crate::types) fn normalize_variadic_parameters<'db>(
    db: &'db dyn Db,
    mut source: Parameters<'db>,
    mut target: Parameters<'db>,
) -> (Parameters<'db>, Parameters<'db>) {
    legacy_inline(normalize_variadic_parameters_with(
        db,
        &mut source,
        &mut target,
        &InlineVariadicNormalizationEffects,
    ));
    (source, target)
}

impl<'state, 'db, 'c> SignatureEffects<'state, 'db, 'c> for LegacyInlineEffects {
    type Error = Infallible;

    fn combine_constraints(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        mut left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        left.verify_builder(builder);
        right.verify_builder(builder);
        match kind {
            ConstraintFoldKind::All => left.intersect(db, builder, right),
            ConstraintFoldKind::Any => left.union(db, builder, right),
        };
        ready(Ok(left))
    }

    fn push_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>> {
        ready(Ok(fold.push(next)))
    }

    fn finish_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        ready(Ok(fold.finish_borrowed()))
    }

    fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> impl Future<Output = Result<SignatureVisit<'visit, 'db>, Self::Error>> {
        ready(Ok(
            match SignatureRelationKey::from_signatures(
                source,
                target,
                checker.relation,
                checker.typevar_evaluation,
            ) {
                // Recursive protocols can revisit the same declarations under changing
                // specializations. The inline checker retains its active coinductive assumption:
                // the revisit contributes `always`, finite mismatches still propagate, and no
                // completed result is memoized. A queued provider can share this admission when
                // it retains the original checker owners and guard across suspension.
                Some(key) => match checker.signature_relation_visitor.begin(key) {
                    Some(guard) => SignatureVisit::Active { _guard: guard },
                    None => SignatureVisit::Cycle,
                },
                None => SignatureVisit::Untracked,
            },
        ))
    }

    fn relate(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(checker.check_type_pair(db, source, target)))
    }

    fn disjoint(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(checker
            .as_disjointness_checker()
            .check_type_pair(db, source, target)))
    }

    fn is_never(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(constraints.is_never_satisfied(db, checker.env)))
    }

    fn is_always(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(constraints.is_always_satisfied(db, checker.env)))
    }

    fn constraint_bound(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        kind: ConstraintBound,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(match kind {
            ConstraintBound::Lower => ConstraintSet::constrain_typevar_lower_bound(
                db,
                checker.env,
                checker.constraints,
                typevar,
                bound,
            ),
            ConstraintBound::Upper => ConstraintSet::constrain_typevar_upper_bound(
                db,
                checker.env,
                checker.constraints,
                typevar,
                bound,
            ),
            ConstraintBound::Equivalent => ConstraintSet::constrain_typevar_equivalence_bound(
                db,
                checker.env,
                checker.constraints,
                typevar,
                bound,
            ),
        }))
    }

    fn receiver_constraints(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(
            signature.receiver_constraints_when_satisfied(db, checker)
        ))
    }

    fn reduce_inferable(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(constraints.reduce_inferable(
            db,
            checker.env,
            checker.constraints,
            inferable,
        )))
    }

    fn max_freshness(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
    ) -> impl Future<Output = Result<Option<TypeVarNonce>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(
            signature.max_typevar_freshness_matching_generic_context(db, context)
        ))
    }

    fn freshen_signature(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        delta: u32,
    ) -> impl Future<Output = Result<Signature<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(signature.freshen_bound_typevars(db, checker.env, delta)))
    }

    fn signature_typevars(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> impl Future<Output = Result<TypeVarSet<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(signature
            .generic_context
            .map_or(TypeVarSet::None, |context| {
                context.inferable_typevars(db)
            })))
    }

    fn aggregate_candidate(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(!ty.has_dynamic(db, checker.env)
            && !ty.has_typevar_or_typevar_instance(db, checker.env)))
    }

    fn union_add(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(builder.add(ty)))
    }

    fn union_build(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(builder.build()))
    }

    fn resolve_alias(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(ty.resolve_type_alias(db)))
    }

    fn parameter_contains_typevar(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(parameters.iter().any(|parameter| {
            any_over_type(
                db,
                checker.env,
                parameter.annotated_type(),
                false,
                |ty| matches!(ty, Type::TypeVar(other) if other.is_same_typevar_as(db, typevar)),
            )
        })))
    }

    fn expand_parameters(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> impl Future<Output = Result<Parameters<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(parameters.expand_starred_variadic_annotations(db)))
    }

    fn normalize_variadic_parameters(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Parameters<'db>,
        target: Parameters<'db>,
    ) -> impl Future<Output = Result<(Parameters<'db>, Parameters<'db>), Self::Error>> {
        let _ = (db, checker);
        ready(Ok(normalize_variadic_parameters(db, source, target)))
    }

    fn empty_tuple(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        let _ = (db, checker);
        ready(Ok(Type::empty_tuple(db, checker.env)))
    }

    fn tuple_from_parameters<'p>(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>
    where
        'db: 'p,
    {
        ready(Ok(
            if let Some((index, variadic)) = parameters
                .clone()
                .find_position(|parameter| parameter.is_variadic())
            {
                let variable = match variadic.annotated_type() {
                    Type::TypeVar(typevartuple) if typevartuple.is_typevartuple(db) => {
                        VariableSegment::TypeVarTuple(typevartuple)
                    }
                    element => VariableSegment::Homogeneous(element),
                };
                Type::tuple(TupleType::mixed_with_segment(
                    db,
                    checker.env,
                    parameters
                        .clone()
                        .take(index)
                        .map(Parameter::annotated_type),
                    variable,
                    parameters.skip(index + 1).map(Parameter::annotated_type),
                ))
            } else {
                Type::heterogeneous_tuple(
                    db,
                    checker.env,
                    parameters.map(Parameter::annotated_type),
                )
            },
        ))
    }
}

/// Returns after one poll, preserving suspension as a separate outcome.
pub(crate) fn try_poll_immediate<F: Future>(future: F) -> Poll<F::Output> {
    std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// Every await reachable with the sealed inline provider completes immediately. The synchronous
/// API cannot represent suspension, so violating that provider invariant is an internal error.
pub(crate) fn legacy_inline<T>(future: impl Future<Output = Result<T, Infallible>>) -> T {
    match try_poll_immediate(future) {
        Poll::Ready(Ok(value)) => value,
        Poll::Ready(Err(never)) => match never {},
        Poll::Pending => panic!("the sealed inline semantic provider must complete in one poll"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::constraints::ConstraintSetBuilder;
    use crate::types::cyclic::ActiveRecursionDetector;
    use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor};
    use crate::types::signatures::SignatureRelationVisitor;
    use crate::types::{ApplyTypeMappingVisitor, CallableType};

    #[test]
    fn scheduled_immediate_poll_preserves_pending_and_error() {
        assert!(matches!(
            try_poll_immediate(std::future::pending::<Result<(), &str>>()),
            Poll::Pending
        ));
        assert_eq!(
            try_poll_immediate(async { Err::<(), _>("unsupported effect") }),
            Poll::Ready(Err("unsupported effect"))
        );
        assert_eq!(
            try_poll_immediate(async { Ok::<_, Infallible>(7) }),
            Poll::Ready(Ok(7))
        );
    }

    #[test]
    fn scheduled_visit_scope_cleans_up_on_cancellation() {
        let visitor = ActiveRecursionDetector::<u8>::default();
        let mut future = Box::pin(async {
            let guard = visitor.begin(7);
            assert!(guard.is_some());
            assert!(visitor.begin(7).is_none());
            std::future::pending::<()>().await;
            drop(guard);
        });
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        assert!(!visitor.is_empty());
        drop(future);
        assert!(visitor.is_empty());
    }

    #[test]
    fn scheduled_inline_signature_future_completes_in_one_poll() {
        let db = setup_db();
        let env = db.program_environment();
        let constraints = ConstraintSetBuilder::new();
        let relation_visitor = HasRelationToVisitor::default(&constraints);
        let disjointness_visitor = IsDisjointVisitor::default(&constraints);
        let signature_visitor = SignatureRelationVisitor::default();
        let mapping_visitor = ApplyTypeMappingVisitor::new(&env);
        let checker = TypeRelationChecker::assignability_with_context(
            &env,
            &constraints,
            &relation_visitor,
            &disjointness_visitor,
            &signature_visitor,
            &mapping_visitor,
        );
        let signature = Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::object())
            ]),
            Type::object(),
        );
        let callable = CallableType::single(&db, signature.clone());
        let future =
            checker.check_callable_pair_with(&db, &LegacyInlineEffects, callable, callable);
        assert!(
            matches!(try_poll_immediate(future), Poll::Ready(Ok(result)) if result.is_trivially_always_satisfied())
        );
        let inner = checker.check_signature_pair_inner_with(
            &db,
            &LegacyInlineEffects,
            &signature,
            &signature,
        );
        assert!(
            matches!(try_poll_immediate(inner), Poll::Ready(Ok(result)) if result.is_trivially_always_satisfied())
        );
    }
}
