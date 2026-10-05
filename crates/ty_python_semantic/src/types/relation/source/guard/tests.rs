use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::attempt_probe::Incomplete;
use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder};
use ty_python_core::ProgramFile;

use super::*;
use crate::Db;
use crate::db::tests::{TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::constructor::expansion_probe;
use crate::types::relation::source::disjoint_guard::tests::{
    Effects, Failure, make_admission, take_refused_operation,
};
use crate::types::relation::source::{RelationSourceOperation, UnavailablePairs};
use crate::types::relation::{HasRelationToVisitor, RelationOwners, TypeVarEvaluation};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, ErrorContextTree, SubclassOfInner, SubclassOfType, TypeVarVariance,
    todo_type,
};

fn pairs<'effects, 'run, 'db: 'run, 'c>(
    db: &'db dyn Db,
    constraints: &'c ConstraintSetBuilder<'db>,
    effects: &'effects Effects<'run, 'db>,
) -> BorrowedPairs<'effects, 'run, 'db, 'c, Effects<'run, 'db>> {
    BorrowedPairs {
        children: &UnavailablePairs,
        db,
        endpoint: &effects.endpoint,
        effects,
        constraints,
    }
}

#[test]
fn exact_cycles_cache_keys_and_context_recomputation_keep_the_original_owners() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let yes = ConstraintSet::from_bool(&builder, true);
    let no = ConstraintSet::from_bool(&builder, false);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let db = &db;
        let builder = &builder;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(db, builder, &effects);
                let first = pairs
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                        let cycle = pairs
                            .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                                Err(RunError::Contract("exact cycle evaluated its body"))
                            })
                            .await?;
                        assert!(cycle.has_same_identity(yes));
                        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                        Ok(no)
                    })
                    .await?;
                assert!(first.has_same_identity(no));
                for (relation, evaluation) in [
                    (TypeRelation::Subtyping, TypeVarEvaluation::Eager),
                    (TypeRelation::Assignability, TypeVarEvaluation::Lazy),
                ] {
                    let mut other = checker.clone();
                    other.relation = relation;
                    other.typevar_evaluation = evaluation;
                    let result = pairs
                        .guard(&other, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                            Ok(yes)
                        })
                        .await?;
                    assert!(result.has_same_identity(yes));
                }
                pairs
                    .guard(checker, Type::object(), Type::AlwaysFalsy, || async {
                        Ok(yes)
                    })
                    .await?;
                assert!(
                    checker
                        .relation_visitor
                        .ownership_probe_storage()
                        .cache_capacity
                        .is_some()
                );

                let mut context = checker.clone();
                context.context_tree = Some(ErrorContextTree::new(context.relation));
                let recomputed = pairs
                    .guard(&context, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        assert_eq!(context.relation_visitor.ownership_probe_counts(), (1, 4));
                        Ok(yes)
                    })
                    .await?;
                assert!(recomputed.has_same_identity(yes));
                let cached = pairs
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        Err(RunError::Contract(
                            "completed guard recomputed its cached result",
                        ))
                    })
                    .await?;
                assert!(cached.has_same_identity(no));
                Ok(())
            })
    })
    .0;
    assert_eq!(result, Ok(Ok(())));
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 4));
}

#[test]
fn cached_constraint_identity_and_satisfaction_refusal_precede_the_body() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let [left, right] = ["A", "B"].map(|name| {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(
            &db,
            &env,
            &builder,
            variable,
            Type::bool_literal(true),
        )
    });
    let expected = left.and(&db, &builder, || right);
    let reversed = right.and(&db, &builder, || left);
    assert!(!expected.has_same_identity(reversed));
    let result = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let db = &db;
        let builder = &builder;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(db, builder, &effects);
                pairs
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        Ok(expected)
                    })
                    .await?;
                let cached = pairs
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        Err(RunError::Contract("cached constraints evaluated the body"))
                    })
                    .await?;
                assert!(cached.has_same_identity(expected));
                assert!(!cached.has_same_identity(reversed));
                let mut context = checker.clone();
                context.context_tree = Some(ErrorContextTree::new(context.relation));
                take_refused_operation();
                let refused = pairs
                    .guard(&context, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        Err(RunError::Contract(
                            "unsatisfied cache decision evaluated the body",
                        ))
                    })
                    .await;
                assert!(matches!(
                    refused,
                    Err(RunError::Contract(
                        "guard test reached an unavailable operation"
                    ))
                ));
                assert_eq!(
                    take_refused_operation(),
                    Some(RelationSourceOperation::ConstraintSatisfaction)
                );
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
                Ok(())
            })
    })
    .0;
    assert_eq!(result, Ok(Ok(())));
}

struct PendingChild<'a, 'db, 'c> {
    visitor: &'a HasRelationToVisitor<'db, 'c>,
    active_at_drop: &'a Cell<Option<(usize, usize)>>,
}

impl Drop for PendingChild<'_, '_, '_> {
    fn drop(&mut self) {
        self.active_at_drop
            .set(Some(self.visitor.ownership_probe_counts()));
    }
}

#[derive(Clone, Copy)]
enum StopAt {
    Child,
    Preparation,
    Acceptance,
}

fn interrupted_guard(failure: Failure, stop: StopAt) {
    let db = setup_db();
    let retry_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, failure);
    let active_at_drop = Cell::new(None);
    let child_started = Cell::new(false);
    let body_completed = Cell::new(false);
    let expected = ConstraintSet::from_bool(&builder, true);
    observations::reset(false);
    if matches!(stop, StopAt::Acceptance) {
        observations::cancel_prepared();
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let checker = &checker;
            let db = &db;
            let builder = &builder;
            let admission = &admission;
            let active_at_drop = &active_at_drop;
            let child_started = &child_started;
            let body_completed = &body_completed;
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = pairs(db, builder, &effects);
                    pairs
                        .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                            if matches!(stop, StopAt::Child) {
                                effects
                                    .endpoint
                                    .local_call(|| {
                                        let child = PendingChild {
                                            visitor: checker.relation_visitor,
                                            active_at_drop,
                                        };
                                        let _reply = effects.endpoint.demand(move || {
                                            child_started.set(true);
                                            async move {
                                                let _child = child;
                                                Err::<(), _>(RunError::Contract(
                                                    "interrupted guard ran its queued child",
                                                ))
                                            }
                                        })?;
                                        admission.armed.set(true);
                                        effects.endpoint.admit_work(1)?;
                                        effects.endpoint.check_completion()?;
                                        Ok(())
                                    })
                                    .await;
                            }
                            if matches!(stop, StopAt::Preparation) {
                                admission.armed.set(true);
                            }
                            body_completed.set(true);
                            Ok(expected)
                        })
                        .await
                })
        })
        .0
    }));
    match failure {
        Failure::Refuse => assert!(matches!(
            outcome.expect("refusal does not unwind"),
            Err(expansion_probe::Incomplete::Allowance)
        )),
        Failure::Cancel => {
            let payload = outcome.expect_err("native cancellation unwinds");
            assert!(matches!(
                payload.downcast_ref::<salsa::Cancelled>(),
                Some(salsa::Cancelled::Local)
            ));
        }
    }
    match stop {
        StopAt::Child => {
            assert!(!body_completed.get());
            assert_eq!(active_at_drop.get(), Some((1, 0)));
            assert_eq!(observations::prepared_count(), 0);
        }
        StopAt::Preparation => {
            assert!(body_completed.get());
            assert_eq!(observations::progress().0, 1);
            assert_eq!(observations::prepared_count(), 0);
        }
        StopAt::Acceptance => {
            assert!(body_completed.get());
            assert_eq!(observations::prepared_count(), 1);
        }
    }
    assert!(!child_started.get());
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    assert!(!expansion_probe::active());
    let retry_db = match failure {
        Failure::Refuse => &db,
        Failure::Cancel => &retry_db,
    };
    let retry_admission = make_admission(retry_db, Failure::Refuse);
    observations::reset(false);
    let retried = Cell::new(false);
    let result = expansion_probe::run(retry_db, usize::MAX, || {
        let checker = &checker;
        let builder = &builder;
        let retried = &retried;
        RegistryBuilder::new(retry_db, &retry_admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                pairs(retry_db, builder, &effects)
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        retried.set(true);
                        Ok(expected)
                    })
                    .await
            })
    })
    .0;
    assert!(
        result
            .expect("completed retry attempt")
            .expect("completed retry guard")
            .has_same_identity(expected)
    );
    assert!(retried.get());
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
    assert!(!expansion_probe::active());
}

#[test]
fn refusal_drains_queued_children_before_retiring_the_guard() {
    interrupted_guard(Failure::Refuse, StopAt::Child);
}

#[test]
fn native_cancellation_drains_queued_children_before_retiring_the_guard() {
    interrupted_guard(Failure::Cancel, StopAt::Child);
}

#[test]
fn preparation_refusal_leaves_no_cached_result_and_retries() {
    interrupted_guard(Failure::Refuse, StopAt::Preparation);
}

#[test]
fn post_preparation_cancellation_does_not_commit_the_result() {
    interrupted_guard(Failure::Cancel, StopAt::Acceptance);
}

struct RefuseResource {
    armed: Cell<bool>,
    fired: Cell<bool>,
    candidate_carriers: Cell<usize>,
}

impl ExecutionAdmission for RefuseResource {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Resource { .. }) && self.armed.get() {
            let remaining = self.candidate_carriers.get();
            if remaining != 0 {
                self.candidate_carriers.set(remaining - 1);
                return Ok(());
            }
            self.armed.set(false);
            self.fired.set(true);
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

#[test]
fn active_and_cache_growth_refusals_preserve_prior_completed_entries() {
    for active_growth in [true, false] {
        let db = setup_db();
        let revision = salsa::plumbing::current_revision(&db);
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let admission = RefuseResource {
            armed: Cell::new(false),
            fired: Cell::new(false),
            // The nested guard first admits its candidate step and false source component.
            // Skip those fixed carriers so this control still denies the active backing growth.
            candidate_carriers: Cell::new(if active_growth { 2 } else { 0 }),
        };
        let result = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let checker = &checker;
            let builder = &builder;
            let admission = &admission;
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = pairs(db, builder, &effects);
                    let yes = ConstraintSet::from_bool(builder, true);
                    if active_growth {
                        pairs
                            .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                                admission.armed.set(true);
                                pairs
                                    .guard(checker, Type::object(), Type::AlwaysFalsy, || async {
                                        Ok(yes)
                                    })
                                    .await
                            })
                            .await?;
                    } else {
                        for source in [Type::AlwaysTruthy, Type::AlwaysFalsy] {
                            pairs
                                .guard(checker, source, Type::Never, || async { Ok(yes) })
                                .await?;
                        }
                        admission.armed.set(true);
                        pairs
                            .guard(checker, Type::object(), Type::Never, || async { Ok(yes) })
                            .await?;
                    }
                    Ok(())
                })
        })
        .0;
        assert!(admission.fired.get());
        assert!(matches!(
            result,
            Err(expansion_probe::Incomplete::Allowance)
        ));
        assert_eq!(
            checker.relation_visitor.ownership_probe_counts(),
            (0, if active_growth { 0 } else { 2 })
        );
        let retry = make_admission(&db, Failure::Refuse);
        let result = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let checker = &checker;
            let builder = &builder;
            RegistryBuilder::new(db, &retry)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = pairs(db, builder, &effects);
                    let yes = ConstraintSet::from_bool(builder, true);
                    pairs
                        .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                            pairs
                                .guard(checker, Type::object(), Type::Never, || async { Ok(yes) })
                                .await
                        })
                        .await?;
                    Ok(())
                })
        })
        .0;
        assert_eq!(result, Ok(Ok(())));
        assert_eq!(checker.relation_visitor.ownership_probe_counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert!(!expansion_probe::active());
    }
}

#[test]
fn stale_result_identity_is_rejected_before_cache_publication() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let db = &db;
        let builder = &builder;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(db, builder, &effects);
                let RelationGuardStep::Evaluate(scope) = RelationGuardEffects::start(
                    &pairs,
                    checker,
                    Type::AlwaysTruthy,
                    Type::AlwaysFalsy,
                )
                .await?
                else {
                    return Err(RunError::Contract("fresh guard did not evaluate"));
                };
                let yes = ConstraintSet::from_bool(builder, true);
                let prepared =
                    RelationGuardEffects::prepare_finish(&pairs, checker, &scope, yes).await?;
                RelationGuardEffects::commit_finish(
                    &pairs,
                    scope,
                    prepared,
                    ConstraintSet::from_bool(builder, false),
                )
                .await
            })
    })
    .0;
    assert!(matches!(
        result,
        Err(expansion_probe::Incomplete::Interrupted)
    ));
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
}

#[test]
fn fixed_source_comparison_precedes_target_identity_and_relation_modes() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_file("/src/guard.py", "def function(): ...\n")
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/guard.py")?,
        env.program(&db),
    );
    let Type::FunctionLiteral(function) = global_symbol(&db, file, "function").place.expect_type()
    else {
        anyhow::bail!("guard fixture must retain a function literal");
    };
    let plain = Type::FunctionLiteral(function);
    let wrapped = Type::FunctionLiteral(
        function.with_descriptor_kind(&db, CallableTypeKind::StaticMethodLike),
    );
    assert_ne!(plain, wrapped);
    for distinct_source in [true, false] {
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let admission = make_admission(&db, Failure::Refuse);
        let result = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let builder = &builder;
            let checker = &checker;
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = pairs(db, builder, &effects);
                    let yes = ConstraintSet::from_bool(builder, true);
                    let outer = if distinct_source {
                        (Type::int_literal(1), plain)
                    } else {
                        (plain, Type::Never)
                    };
                    pairs
                        .guard(checker, outer.0, outer.1, || async {
                            let mut different_modes = checker.clone();
                            different_modes.relation = TypeRelation::Subtyping;
                            different_modes.typevar_evaluation = TypeVarEvaluation::Lazy;
                            take_refused_operation();
                            if distinct_source {
                                let inner = pairs
                                    .guard(
                                        &different_modes,
                                        Type::int_literal(2),
                                        wrapped,
                                        || async { Ok(yes) },
                                    )
                                    .await?;
                                assert!(inner.has_same_identity(yes));
                                assert_eq!(take_refused_operation(), None);
                            } else {
                                let inner = pairs
                                    .guard(&different_modes, wrapped, Type::Never, || async {
                                        Err(RunError::Contract(
                                            "unadmitted identity evaluated its body",
                                        ))
                                    })
                                    .await;
                                assert!(matches!(
                                    inner,
                                    Err(RunError::Contract(
                                        "guard test reached an unavailable operation"
                                    ))
                                ));
                                assert_eq!(
                                    take_refused_operation(),
                                    Some(RelationSourceOperation::GuardIdentity)
                                );
                                assert_eq!(
                                    checker.relation_visitor.ownership_probe_counts(),
                                    (1, 0)
                                );
                            }
                            Ok(yes)
                        })
                        .await?;
                    Ok(())
                })
        })
        .0;
        assert_eq!(result, Ok(Ok(())));
        assert_eq!(
            checker.relation_visitor.ownership_probe_counts(),
            (0, if distinct_source { 2 } else { 1 })
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateCase {
    FunctionSource,
    ProtocolSource,
    FunctionTarget,
    EqualFunctionSource,
    SkippedTargetFields,
    FunctionSourceFields,
    FunctionTargetFields,
    ProtocolTargetFields,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateStop {
    Work,
    Bytes,
    Cancellation,
}

/// Interrupts the first reached component after its enclosing guard has been installed.
struct StopCandidate<'db> {
    db: &'db crate::db::tests::TestDb,
    stop: CandidateStop,
    armed: Cell<bool>,
    component: Cell<bool>,
    fired: Cell<bool>,
}

impl ExecutionAdmission for StopCandidate<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !self.armed.get() {
            return Ok(());
        }
        if work == (ExecutionWork::Work { units: 32 }) {
            self.component.set(true);
            if matches!(self.stop, CandidateStop::Work) {
                self.fired.set(true);
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        if self.component.get() && matches!(work, ExecutionWork::Resource { .. }) {
            self.fired.set(true);
            self.armed.set(false);
            match self.stop {
                CandidateStop::Work | CandidateStop::Bytes => {
                    return Err(RunError::Refused(Incomplete::Allowance));
                }
                CandidateStop::Cancellation => self.db.cancellation_token().cancel(),
            }
        }
        Ok(())
    }
}

// These controls distinguish logical work from carrier-byte denial at the actual component
// boundary. They observe an intact outer guard during unwind and a fresh same-revision retry.
#[test_case::test_case(CandidateStop::Work; "logical work")]
#[test_case::test_case(CandidateStop::Bytes; "carrier bytes")]
#[test_case::test_case(CandidateStop::Cancellation; "native cancellation")]
fn candidate_admission_preserves_outer_guard_and_retry(stop: CandidateStop) -> anyhow::Result<()> {
    let db = setup_db();
    let retry_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = StopCandidate {
        db: &db,
        stop,
        armed: Cell::new(false),
        component: Cell::new(false),
        fired: Cell::new(false),
    };
    let active_at_drop = Cell::new(None);
    let body_ran = Cell::new(false);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let (db, builder, checker) = (&db, &builder, &checker);
            let (admission, active_at_drop, body_ran) = (&admission, &active_at_drop, &body_ran);
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = pairs(db, builder, &effects);
                    pairs
                        .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                            let _retained = PendingChild {
                                visitor: checker.relation_visitor,
                                active_at_drop,
                            };
                            admission.armed.set(true);
                            pairs
                                .guard(checker, Type::object(), Type::Never, || async {
                                    body_ran.set(true);
                                    Ok(ConstraintSet::from_bool(builder, true))
                                })
                                .await
                        })
                        .await
                })
        })
        .0
    }));
    match stop {
        CandidateStop::Work | CandidateStop::Bytes => {
            assert!(matches!(
                outcome,
                Ok(Err(expansion_probe::Incomplete::Allowance))
            ));
        }
        CandidateStop::Cancellation => {
            let Err(payload) = outcome else {
                anyhow::bail!("native cancellation must unwind");
            };
            assert!(matches!(
                payload.downcast_ref::<salsa::Cancelled>(),
                Some(salsa::Cancelled::Local)
            ));
        }
    }
    assert!(admission.fired.get());
    assert!(!body_ran.get());
    assert_eq!(active_at_drop.get(), Some((1, 0)));
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    assert!(!expansion_probe::active());
    let retry_db = match stop {
        CandidateStop::Cancellation => &retry_db,
        CandidateStop::Work | CandidateStop::Bytes => &db,
    };
    let retry_admission = make_admission(retry_db, Failure::Refuse);
    let retried = expansion_probe::run(retry_db, usize::MAX, || {
        let (builder, checker, body_ran) = (&builder, &checker, &body_ran);
        RegistryBuilder::new(retry_db, &retry_admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(retry_db, builder, &effects);
                pairs
                    .guard(checker, Type::AlwaysTruthy, Type::AlwaysFalsy, || async {
                        pairs
                            .guard(checker, Type::object(), Type::Never, || async {
                                body_ran.set(true);
                                Ok(ConstraintSet::from_bool(builder, true))
                            })
                            .await
                    })
                    .await
            })
    })
    .0;
    assert!(matches!(retried, Ok(Ok(_))));
    assert!(body_ran.get());
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 2));
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
    assert!(!expansion_probe::active());
    Ok(())
}

// These controls observe candidate ordering, refused fields, and visitor ownership, which mdtests
// cannot observe. Cross-variant candidates run the real guard body; required fields still refuse.
#[test_case::test_case(CandidateCase::FunctionSource; "function source versus nominal")]
#[test_case::test_case(CandidateCase::ProtocolSource; "protocol source versus nominal")]
#[test_case::test_case(CandidateCase::FunctionTarget; "function target versus nominal")]
#[test_case::test_case(CandidateCase::EqualFunctionSource; "equal function source needs no fields")]
#[test_case::test_case(CandidateCase::SkippedTargetFields; "false source skips target fields")]
#[test_case::test_case(CandidateCase::FunctionSourceFields; "source fields precede modes")]
#[test_case::test_case(CandidateCase::FunctionTargetFields; "target fields precede modes")]
#[test_case::test_case(CandidateCase::ProtocolTargetFields; "protocol target fields remain unavailable")]
fn finite_guard_candidates_preserve_order_and_owners(case: CandidateCase) -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/candidates.py",
            "from typing import Protocol\ndef function(): ...\nclass P[T](Protocol):\n    value: T\np1: P[int]\np2: P[str]\n",
        )
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/candidates.py")?,
        env.program(&db),
    );
    let symbol = |name| global_symbol(&db, file, name).place.expect_type();
    let Type::FunctionLiteral(function) = symbol("function") else {
        anyhow::bail!("candidate fixture must retain a function literal");
    };
    let plain = Type::FunctionLiteral(function);
    let wrapped = Type::FunctionLiteral(
        function.with_descriptor_kind(&db, CallableTypeKind::StaticMethodLike),
    );
    let (p1, p2) = (symbol("p1"), symbol("p2"));
    assert!(p1.as_protocol_instance().is_some() && p2.as_protocol_instance().is_some());
    let one = Type::int_literal(1);
    let two = Type::int_literal(2);
    let (outer, inner, refused) = match case {
        CandidateCase::FunctionSource => ((Type::object(), one), (plain, two), false),
        CandidateCase::ProtocolSource => ((Type::object(), one), (p1, two), false),
        CandidateCase::FunctionTarget => ((one, Type::object()), (one, plain), false),
        CandidateCase::EqualFunctionSource => ((plain, one), (plain, two), false),
        CandidateCase::SkippedTargetFields => ((one, plain), (two, wrapped), false),
        CandidateCase::FunctionSourceFields => ((plain, one), (wrapped, two), true),
        CandidateCase::FunctionTargetFields => ((one, plain), (one, wrapped), true),
        CandidateCase::ProtocolTargetFields => ((one, p1), (one, p2), true),
    };
    assert_ne!(plain, wrapped);
    assert_ne!(p1, p2);
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let body_ran = Cell::new(false);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let (db, builder, checker, body_ran) = (&db, &builder, &checker, &body_ran);
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(db, builder, &effects);
                let yes = ConstraintSet::from_bool(builder, true);
                pairs
                    .guard(checker, outer.0, outer.1, || async {
                        let mut different_modes = checker.clone();
                        different_modes.relation = TypeRelation::Subtyping;
                        different_modes.typevar_evaluation = TypeVarEvaluation::Lazy;
                        take_refused_operation();
                        let result = pairs
                            .guard(&different_modes, inner.0, inner.1, || async {
                                body_ran.set(true);
                                assert_eq!(
                                    checker.relation_visitor.ownership_probe_counts(),
                                    (2, 0)
                                );
                                Ok(yes)
                            })
                            .await;
                        if refused {
                            assert!(matches!(
                                result,
                                Err(RunError::Contract(
                                    "guard test reached an unavailable operation"
                                ))
                            ));
                            assert_eq!(
                                take_refused_operation(),
                                Some(RelationSourceOperation::GuardIdentity)
                            );
                            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                        } else {
                            assert!(result?.has_same_identity(yes));
                            assert_eq!(take_refused_operation(), None);
                            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 1));
                        }
                        Ok(yes)
                    })
                    .await?;
                Ok(())
            })
    })
    .0;
    assert_eq!(result, Ok(Ok(())));
    assert_eq!(body_ran.get(), !refused);
    assert_eq!(
        checker.relation_visitor.ownership_probe_counts(),
        (0, if refused { 1 } else { 2 })
    );
    Ok(())
}

#[test]
fn unbounded_keys_and_recursive_fallback_keep_their_named_refusals() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let admission = make_admission(&db, Failure::Refuse);
    let todo = todo_type!("source guard key");
    let wrapped = SubclassOfType::from(&db, &env, SubclassOfInner::Dynamic(todo.expect_dynamic()));
    let result = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builder = &builder;
        let checker = &checker;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = pairs(db, builder, &effects);
                for source in [todo, wrapped] {
                    take_refused_operation();
                    let refused = pairs
                        .guard(checker, source, Type::Never, || async {
                            Err(RunError::Contract("unbounded key evaluated its body"))
                        })
                        .await;
                    assert!(matches!(
                        refused,
                        Err(RunError::Contract(
                            "guard test reached an unavailable operation"
                        ))
                    ));
                    assert_eq!(
                        take_refused_operation(),
                        Some(RelationSourceOperation::GuardKey)
                    );
                    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
                }
                let refused = RelationGuardEffects::recursive_fallback(
                    &pairs,
                    checker,
                    Type::AlwaysTruthy,
                    Type::AlwaysFalsy,
                )
                .await;
                assert!(matches!(
                    refused,
                    Err(RunError::Contract(
                        "guard test reached an unavailable operation"
                    ))
                ));
                assert_eq!(
                    take_refused_operation(),
                    Some(RelationSourceOperation::RecursiveFallback)
                );
                Ok(())
            })
    })
    .0;
    assert_eq!(result, Ok(Ok(())));
}
