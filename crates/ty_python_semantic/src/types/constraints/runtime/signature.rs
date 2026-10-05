//! A finite ParamSpec consumer of admitted, borrowed constraint operations.

use std::cell::Cell;
use std::ops::ControlFlow;

use salsa::execution_probe::{
    ExecutionAdmission as RuntimeAdmission, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};

use super::super::control::attempt::ExecutionControl;
use super::super::typevar_equivalence::PendingTypevarEquivalence;
use super::super::{ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder};
use super::tests::ObservedCursor;
use super::{EndpointAdmission, RuntimeStructural};
use crate::Db;
use crate::types::callable::{CallableType, CallableTypeKind, CallableTypes};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::GenericContext;
use crate::types::relation::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::signatures::effects::{
    self, ConstraintBound, SignatureEffect, SignatureEffects, SignatureVisit, sealed,
};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{BoundTypeVarInstance, Type, UnionBuilder};

mod tests;
use tests::{Observations, PushBorrowGuard};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignatureSite {
    EntryMode,
    TargetCapability,
    SourceCapability(usize),
    ExpandParameters,
    NormalizeParameters,
    Equivalence,
    TrivialPredicate,
    AliasIdentity,
    Combine,
    Push,
    Finish,
    Unsupported(SignatureEffect),
}

#[derive(Clone, Copy, Debug)]
enum SignatureBoundary<'db> {
    Request {
        site: SignatureSite,
        operands: [Option<Type<'db>>; 2],
        builder: *const (),
        checker: Option<*const ()>,
    },
    AfterEquivalenceAdvanceReturned {
        outer_complete: bool,
    },
    BeforeEquivalenceRetirementAcceptance,
}

#[derive(Clone, Copy, Debug)]
enum SignatureWork {
    EntryMode,
    CallableShape,
    ExpandParameters,
    NormalizeParameters,
    ConstraintBound,
    TrivialPredicate,
    AliasIdentity,
    Unsupported,
}

impl SignatureWork {
    fn units(self) -> usize {
        match self {
            Self::EntryMode | Self::ConstraintBound => 16,
            Self::CallableShape | Self::ExpandParameters => 64,
            Self::NormalizeParameters => 128,
            Self::TrivialPredicate | Self::AliasIdentity => 8,
            Self::Unsupported => 1,
        }
    }
}

struct SiteScope<'a> {
    site: Option<&'a Cell<Option<SignatureSite>>>,
    previous: Option<SignatureSite>,
}

impl<'a> SiteScope<'a> {
    fn new(observations: Option<&'a Observations<'_, '_>>, site: SignatureSite) -> Self {
        let previous = observations.and_then(|observations| {
            observations
                .occurrence
                .set(observations.occurrence.get() + 1);
            observations.current_site.replace(Some(site))
        });
        Self {
            site: observations.map(|observations| &observations.current_site),
            previous,
        }
    }
}

impl Drop for SiteScope<'_> {
    fn drop(&mut self) {
        if let Some(site) = self.site {
            site.set(self.previous);
        }
    }
}

fn unsupported(db: &dyn Db, effect: SignatureEffect) -> RunError {
    let reason = expansion_probe::refuse(db, Incomplete::UnsupportedSignatureOperation(effect));
    RunError::Refused(match reason {
        Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
        Incomplete::RequestedAllocation => salsa::attempt_probe::Incomplete::RequestedAllocation,
        _ => salsa::attempt_probe::Incomplete::Interrupted,
    })
}

fn observe<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    observations: Option<&Observations<'run, 'db>>,
    boundary: SignatureBoundary<'db>,
) -> RunResult<()> {
    if let Some(observer) = observations.and_then(|observations| observations.observer) {
        observer(endpoint, boundary)?;
    }
    Ok(())
}

fn run<'run, 'state, 'db: 'run, 'c>(
    db: &'db dyn Db,
    admission: &'run dyn RuntimeAdmission,
    checker: &'run TypeRelationChecker<'state, 'c, 'db>,
    sources: &'run CallableTypes<'db>,
    target: CallableType<'db>,
    observations: Option<&'run Observations<'run, 'db>>,
) -> RunResult<ConstraintSet<'db, 'c>>
where
    'state: 'run,
    'c: 'run,
{
    RegistryBuilder::new(db, admission)?
        .seal()?
        .run(move |endpoint| {
            compare_registered(db, endpoint, checker, sources, target, observations)
        })
}

async fn compare_registered<'run, 'state, 'db: 'run, 'c>(
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    checker: &TypeRelationChecker<'state, 'c, 'db>,
    sources: &CallableTypes<'db>,
    target: CallableType<'db>,
    observations: Option<&'run Observations<'run, 'db>>,
) -> RunResult<ConstraintSet<'db, 'c>> {
    let effects = admit_provider(db, endpoint, checker, sources, target, observations).await?;
    checker
        .check_callables_vs_callable_with(db, &effects, sources, target)
        .await
}

async fn admit_provider<'run, 'state, 'db: 'run, 'c>(
    db: &'db dyn Db,
    endpoint: TaskEndpoint<'run, 'db>,
    checker: &TypeRelationChecker<'state, 'c, 'db>,
    sources: &CallableTypes<'db>,
    target: CallableType<'db>,
    observations: Option<&'run Observations<'run, 'db>>,
) -> RunResult<RuntimeSignature<'run, 'db, 'c>> {
    {
        let _site = SiteScope::new(observations, SignatureSite::EntryMode);
        endpoint
            .local_call(|| {
                endpoint.admit_work(SignatureWork::EntryMode.units())?;
                observe(
                    &endpoint,
                    observations,
                    SignatureBoundary::Request {
                        site: SignatureSite::EntryMode,
                        operands: [None, None],
                        builder: std::ptr::from_ref(checker.constraints).cast(),
                        checker: Some(std::ptr::from_ref(checker).cast()),
                    },
                )?;
                if checker.relation != TypeRelation::Assignability
                    || checker.typevar_evaluation != TypeVarEvaluation::Lazy
                    || checker.is_context_collection_enabled()
                {
                    return Err(unsupported(db, SignatureEffect::CheckerMode));
                }
                if checker.constraints.storage.borrow().compacted.is_some() {
                    return Err(unsupported(db, SignatureEffect::CompactedBuilder));
                }
                Ok(())
            })
            .await;
    }
    for (site, callable) in std::iter::once((SignatureSite::TargetCapability, target)).chain(
        sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| (SignatureSite::SourceCapability(index), *source)),
    ) {
        let _site = SiteScope::new(observations, site);
        endpoint
            .local_call(|| {
                endpoint.admit_work(SignatureWork::CallableShape.units())?;
                observe(
                    &endpoint,
                    observations,
                    SignatureBoundary::Request {
                        site,
                        operands: [Some(Type::Callable(callable)), None],
                        builder: std::ptr::from_ref(checker.constraints).cast(),
                        checker: Some(std::ptr::from_ref(checker).cast()),
                    },
                )?;
                if callable.kind(db) != CallableTypeKind::ParamSpecValue
                    || callable.signatures(db).fixed_paramspec_value(db).is_none()
                {
                    return Err(unsupported(db, SignatureEffect::EntryShape));
                }
                Ok(())
            })
            .await;
    }
    Ok(RuntimeSignature::new(
        db,
        endpoint,
        checker.constraints,
        observations,
    ))
}

struct RuntimeSignature<'run, 'db: 'run, 'c> {
    structural: RuntimeStructural<'run, 'db>,
    builder: &'c ConstraintSetBuilder<'db>,
    observations: Option<&'run Observations<'run, 'db>>,
}

impl<'run, 'db: 'run, 'c> RuntimeSignature<'run, 'db, 'c> {
    fn new(
        db: &'db dyn Db,
        endpoint: TaskEndpoint<'run, 'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        observations: Option<&'run Observations<'run, 'db>>,
    ) -> Self {
        let mut structural = RuntimeStructural::new(db, endpoint);
        structural.observer =
            observations.and_then(|observations| observations.structural_observer);
        structural.cursor_lifetime =
            observations.and_then(|observations| observations.structural_lifetime);
        Self {
            structural,
            builder,
            observations,
        }
    }

    fn verify_builder(&self, builder: &ConstraintSetBuilder<'db>) -> RunResult<()> {
        if std::ptr::eq(builder, self.builder) {
            Ok(())
        } else {
            Err(RunError::Contract(
                "signature effect received a foreign constraint builder",
            ))
        }
    }

    fn request(
        &self,
        site: SignatureSite,
        checker: Option<*const ()>,
        operands: [Option<Type<'db>>; 2],
    ) -> RunResult<()> {
        observe(
            &self.structural.endpoint,
            self.observations,
            SignatureBoundary::Request {
                site,
                operands,
                builder: std::ptr::from_ref(self.builder).cast(),
                checker,
            },
        )
    }

    async fn fixed<T>(
        &self,
        work: SignatureWork,
        site: SignatureSite,
        checker: *const (),
        operands: [Option<Type<'db>>; 2],
        action: impl FnOnce() -> RunResult<T>,
    ) -> RunResult<T> {
        let _site = SiteScope::new(self.observations, site);
        let value = self
            .structural
            .endpoint
            .local_call(|| {
                self.structural.endpoint.admit_work(work.units())?;
                self.request(site, Some(checker), operands)?;
                action()
            })
            .await;
        Ok(value)
    }

    async fn reject<T, H>(
        &self,
        effect: SignatureEffect,
        checker: *const (),
        held: H,
    ) -> RunResult<T> {
        let result = self
            .fixed(
                SignatureWork::Unsupported,
                SignatureSite::Unsupported(effect),
                checker,
                [None, None],
                || Err(unsupported(self.structural.db, effect)),
            )
            .await;
        // The caller owns semantic inputs until the protected error has drained child tasks.
        drop(held);
        result
    }

    async fn terminal<'state>(
        &self,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        always: bool,
    ) -> RunResult<bool> {
        self.fixed(
            SignatureWork::TrivialPredicate,
            SignatureSite::TrivialPredicate,
            std::ptr::from_ref(checker).cast(),
            [None, None],
            || {
                self.verify_builder(checker.constraints)?;
                self.verify_builder(constraints.builder)?;
                let is_never = constraints.is_trivially_never_satisfied();
                let is_always = constraints.is_trivially_always_satisfied();
                if !is_never && !is_always {
                    return Err(unsupported(
                        self.structural.db,
                        SignatureEffect::ConstraintSatisfiability,
                    ));
                }
                Ok(if always { is_always } else { is_never })
            },
        )
        .await
    }
}

impl sealed::Sealed for RuntimeSignature<'_, '_, '_> {}

impl<'run, 'state, 'db: 'run, 'c> SignatureEffects<'state, 'db, 'c>
    for RuntimeSignature<'run, 'db, 'c>
{
    type Error = RunError;

    async fn combine_constraints(
        &self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let _site = SiteScope::new(self.observations, SignatureSite::Combine);
        self.structural
            .combine_with_entry(builder, kind, left, right, || {
                self.verify_builder(builder)?;
                self.verify_builder(left.builder)?;
                self.verify_builder(right.builder)?;
                self.request(SignatureSite::Combine, None, [None, None])
            })
            .await
    }

    async fn push_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        let _site = SiteScope::new(self.observations, SignatureSite::Push);
        let mut fold = PushBorrowGuard::new(
            fold,
            self.observations
                .and_then(|observations| observations.push_lifetime),
        );
        let result = self
            .structural
            .push_with_entry(&mut fold, next, |fold| {
                self.verify_builder(fold.builder)?;
                self.verify_builder(next.builder)?;
                self.request(SignatureSite::Push, None, [None, None])
            })
            .await;
        if result.is_ok()
            && let Some(observations) = self.observations
        {
            observations
                .accepted_pushes
                .set(observations.accepted_pushes.get() + 1);
        }
        result
    }

    async fn finish_constraints(
        &self,
        _db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let _site = SiteScope::new(self.observations, SignatureSite::Finish);
        self.structural
            .finish_with_entry(fold, |fold| {
                self.verify_builder(fold.builder)?;
                self.request(SignatureSite::Finish, None, [None, None])
            })
            .await
    }

    async fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> RunResult<SignatureVisit<'visit, 'db>> {
        self.reject(
            SignatureEffect::SignatureScope,
            std::ptr::from_ref(checker).cast(),
            (source, target),
        )
        .await
    }

    async fn relate(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.reject(
            SignatureEffect::Relation,
            std::ptr::from_ref(checker).cast(),
            (source, target),
        )
        .await
    }

    async fn disjoint(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.reject(
            SignatureEffect::Disjoint,
            std::ptr::from_ref(checker).cast(),
            (source, target),
        )
        .await
    }

    async fn is_never(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.terminal(checker, constraints, false).await
    }

    async fn is_always(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.terminal(checker, constraints, true).await
    }

    async fn constraint_bound(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        kind: ConstraintBound,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let _site = SiteScope::new(self.observations, SignatureSite::Equivalence);
        let db = self.structural.db;
        let admission = EndpointAdmission(&self.structural.endpoint);
        let mut control = ExecutionControl::new(&admission);
        let cursor = self
            .structural
            .endpoint
            .local_call(|| {
                self.structural
                    .endpoint
                    .admit_work(SignatureWork::ConstraintBound.units())?;
                self.request(
                    SignatureSite::Equivalence,
                    Some(std::ptr::from_ref(checker).cast()),
                    [Some(Type::TypeVar(typevar)), Some(bound)],
                )?;
                self.verify_builder(checker.constraints)?;
                let Type::TypeVar(bound) = bound else {
                    return Err(unsupported(db, SignatureEffect::ConstraintConstruction));
                };
                if !matches!(kind, ConstraintBound::Equivalent)
                    || !typevar.is_paramspec(db)
                    || typevar.paramspec_attr(db).is_some()
                    || !bound.is_paramspec(db)
                    || bound.paramspec_attr(db).is_some()
                {
                    return Err(unsupported(db, SignatureEffect::ConstraintConstruction));
                }
                PendingTypevarEquivalence::new(db, self.builder, typevar, bound)
                    .map_err(|_| unsupported(db, SignatureEffect::CompactedBuilder))
            })
            .await;
        let mut cursor = Some(ObservedCursor::new(
            cursor,
            self.observations
                .and_then(|observations| observations.equivalence_lifetime),
        ));
        loop {
            let progress = self
                .structural
                .endpoint
                .local_call(|| {
                    let progress = cursor
                        .as_mut()
                        .ok_or(RunError::Contract("equivalence cursor already retired"))?
                        .advance_with(&mut control)
                        .map_err(|error| self.structural.error(error))?;
                    observe(
                        &self.structural.endpoint,
                        self.observations,
                        SignatureBoundary::AfterEquivalenceAdvanceReturned {
                            outer_complete: progress.is_break(),
                        },
                    )?;
                    Ok(progress)
                })
                .await;
            match progress {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(result) => {
                    self.structural
                        .endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            observe(
                                &self.structural.endpoint,
                                self.observations,
                                SignatureBoundary::BeforeEquivalenceRetirementAcceptance,
                            )
                        })
                        .await;
                    return Ok(result);
                }
            }
        }
    }

    async fn receiver_constraints(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.reject(
            SignatureEffect::ReceiverConstraints,
            std::ptr::from_ref(checker).cast(),
            signature,
        )
        .await
    }

    async fn reduce_inferable(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.reject(
            SignatureEffect::ConstraintReduction,
            std::ptr::from_ref(checker).cast(),
            (constraints, inferable),
        )
        .await
    }

    async fn max_freshness(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<Option<TypeVarNonce>> {
        self.reject(
            SignatureEffect::SignatureFreshness,
            std::ptr::from_ref(checker).cast(),
            (signature, context),
        )
        .await
    }

    async fn freshen_signature(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        delta: u32,
    ) -> RunResult<Signature<'db>> {
        self.reject(
            SignatureEffect::SignatureFreshening,
            std::ptr::from_ref(checker).cast(),
            (signature, delta),
        )
        .await
    }

    async fn signature_typevars(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> RunResult<TypeVarSet<'db>> {
        self.reject(
            SignatureEffect::SignatureTypevars,
            std::ptr::from_ref(checker).cast(),
            signature,
        )
        .await
    }

    async fn aggregate_candidate(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.reject(
            SignatureEffect::AggregateTypeInspection,
            std::ptr::from_ref(checker).cast(),
            ty,
        )
        .await
    }

    async fn union_add(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        self.reject(
            SignatureEffect::UnionNormalization,
            std::ptr::from_ref(checker).cast(),
            (builder, ty),
        )
        .await
    }

    async fn union_build(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        builder: UnionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.reject(
            SignatureEffect::UnionNormalization,
            std::ptr::from_ref(checker).cast(),
            builder,
        )
        .await
    }

    async fn resolve_alias(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.fixed(
            SignatureWork::AliasIdentity,
            SignatureSite::AliasIdentity,
            std::ptr::from_ref(checker).cast(),
            [Some(ty), None],
            || {
                self.verify_builder(checker.constraints)?;
                if !matches!(ty, Type::TypeVar(_)) {
                    return Err(unsupported(
                        self.structural.db,
                        SignatureEffect::AliasResolution,
                    ));
                }
                Ok(ty)
            },
        )
        .await
    }

    async fn parameter_contains_typevar(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.reject(
            SignatureEffect::ParameterTypeInspection,
            std::ptr::from_ref(checker).cast(),
            (parameters, typevar),
        )
        .await
    }

    async fn expand_parameters(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> RunResult<Parameters<'db>> {
        let db = self.structural.db;
        self.fixed(
            SignatureWork::ExpandParameters,
            SignatureSite::ExpandParameters,
            std::ptr::from_ref(checker).cast(),
            [None, None],
            || {
                self.verify_builder(checker.constraints)?;
                if parameters.fixed_paramspec(db).is_none() {
                    return Err(unsupported(db, SignatureEffect::ParameterExpansion));
                }
                Ok(parameters.expand_starred_variadic_annotations(db))
            },
        )
        .await
    }

    async fn normalize_variadic_parameters(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Parameters<'db>,
        target: Parameters<'db>,
    ) -> RunResult<(Parameters<'db>, Parameters<'db>)> {
        let db = self.structural.db;
        self.fixed(
            SignatureWork::NormalizeParameters,
            SignatureSite::NormalizeParameters,
            std::ptr::from_ref(checker).cast(),
            [None, None],
            || {
                self.verify_builder(checker.constraints)?;
                if source.fixed_paramspec(db).is_none() || target.fixed_paramspec(db).is_none() {
                    return Err(unsupported(db, SignatureEffect::VariadicNormalization));
                }
                Ok(effects::normalize_variadic_parameters(
                    db,
                    source.clone(),
                    target.clone(),
                ))
            },
        )
        .await
    }

    async fn empty_tuple(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
    ) -> RunResult<Type<'db>> {
        self.reject(
            SignatureEffect::TupleNormalization,
            std::ptr::from_ref(checker).cast(),
            (),
        )
        .await
    }

    async fn tuple_from_parameters<'p>(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> RunResult<Type<'db>>
    where
        'db: 'p,
    {
        self.reject(
            SignatureEffect::TupleNormalization,
            std::ptr::from_ref(checker).cast(),
            parameters,
        )
        .await
    }
}
