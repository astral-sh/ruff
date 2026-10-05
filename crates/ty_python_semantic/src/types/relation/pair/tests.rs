use std::cell::{Cell, RefCell};
use std::future::ready;
use std::task::Poll;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use super::PairEvaluation;
use crate::Db;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::relation::dependencies::{OrdinaryDependencies, RelationDependencies};
use crate::types::relation::guard::RelationGuardStep;
use crate::types::relation::pair_effects::{InlinePairEffects, PairEffects};
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, RelationObservationSite, RelationObservations,
    TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::signatures::effects::{legacy_inline, try_poll_immediate};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, Type, TypeVarVariance,
};

fn with_checker(
    check: impl for<'a, 'c, 'db> FnOnce(
        &'db TestDb,
        TypeRelationChecker<'a, 'c, 'db>,
    ) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relation = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    check(
        &db,
        TypeRelationChecker::assignability_with_context(
            &env,
            &constraints,
            &relation,
            &disjointness,
            &signatures,
            &mapping,
        ),
    )
}

struct RefuseAfter(Cell<usize>);

impl RelationDependencies for RefuseAfter {
    type Error = anyhow::Error;

    fn run<T>(&self, _db: &dyn Db, operation: impl FnOnce() -> T) -> anyhow::Result<T> {
        anyhow::ensure!(self.0.get() != 0, "refused");
        self.0.set(self.0.get() - 1);
        Ok(operation())
    }
}

#[test]
fn incomplete_pair_does_not_publish_observations() -> anyhow::Result<()> {
    with_checker(|db, mut checker| {
        let int = KnownClass::Int.to_instance(db, checker.env);
        let typevar = BoundTypeVarInstance::synthetic(
            db,
            checker.env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let target = Type::TypeVar(typevar);
        let patterns = FxHashSet::from_iter([int, target]);
        let observations = RelationObservations {
            patterns: &patterns,
            results: RefCell::default(),
            site: Cell::new(RelationObservationSite::Argument(0)),
            inferred: RefCell::default(),
        };
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        checker.inferable = TypeVarSet::from_typevars(db, [typevar]);
        let result = checker.check_type_pair(db, int, target);
        anyhow::ensure!(!result.is_trivially_always_satisfied());
        anyhow::ensure!(!result.is_trivially_never_satisfied());
        let checker = TypeRelationChecker {
            observations: Some(&observations),
            ..checker
        };

        // Both directions match a pattern. Refuse each preparation step and the final commit.
        for allowed in 0..11 {
            let pending = PairEvaluation::start(db, &checker, int, target, &OrdinaryDependencies)?;
            assert!(
                pending
                    .finish(db, result, &RefuseAfter(Cell::new(allowed)))
                    .is_err()
            );
            assert!(observations.results.borrow().is_empty());
            assert!(observations.inferred.borrow().is_empty());
        }
        for _ in 0..2 {
            let pending = PairEvaluation::start(db, &checker, int, target, &OrdinaryDependencies)?;
            let completed = pending.finish(db, result, &RefuseAfter(Cell::new(11)))?;
            assert!(completed.ownership_probe_same_set(result));
            assert_eq!(observations.results.borrow().len(), 2);
            assert_eq!(observations.inferred.borrow().len(), 1);
            assert!(observations.inferred.borrow()[&typevar.identity(db)].contains(&int));
        }
        Ok(())
    })
}

#[test]
fn cache_decision_can_suspend_and_recompute_without_replacing_result() -> anyhow::Result<()> {
    with_checker(|db, checker| {
        let source = KnownClass::Int.to_instance(db, checker.env);
        let target = KnownClass::Str.to_instance(db, checker.env);
        let RelationGuardStep::Evaluate(scope) =
            RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
        else {
            anyhow::bail!("first visit must evaluate");
        };
        let original = scope.finish(checker.never());
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));

        for _ in 0..2 {
            let RelationGuardStep::CheckCached(pending) =
                RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
            else {
                anyhow::bail!("context collection requires a cache check");
            };
            assert!(pending.constraints().ownership_probe_same_set(original));
            // These inspect the detector while the decision is pending. No RefCell borrow or
            // active scope may survive in the cached-entry token.
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
            let RelationGuardStep::Complete(cached) = RelationGuardStep::start(
                db,
                &checker.with_context_collection_disabled(),
                source,
                target,
                &OrdinaryDependencies,
            )?
            else {
                anyhow::bail!("quiet comparison should reuse the cached answer");
            };
            assert!(cached.ownership_probe_same_set(original));
            let RelationGuardStep::Evaluate(scope) =
                pending.resume(db, true, &OrdinaryDependencies)?
            else {
                anyhow::bail!("unsatisfied cache entry must recompute its context");
            };
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 1));
            assert!(
                scope
                    .finish(checker.always())
                    .is_trivially_always_satisfied()
            );
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
        }
        Ok(())
    })
}

#[test]
fn incomplete_guard_leaves_no_active_visit_or_completed_result() -> anyhow::Result<()> {
    with_checker(|db, checker| {
        let source = KnownClass::Int.to_instance(db, checker.env);
        let target = KnownClass::Str.to_instance(db, checker.env);
        for _ in 0..2 {
            let RelationGuardStep::Evaluate(scope) =
                RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
            else {
                anyhow::bail!("retry must start a fresh visit");
            };
            let RelationGuardStep::Complete(cycle) =
                RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
            else {
                anyhow::bail!("exact recursive edge uses the existing cycle value");
            };
            assert!(cycle.is_trivially_always_satisfied());
            drop(scope);
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
        }
        let RelationGuardStep::Evaluate(scope) =
            RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
        else {
            anyhow::bail!("completed retry must evaluate");
        };
        scope.finish(checker.never());
        let RelationGuardStep::CheckCached(pending) =
            RelationGuardStep::start(db, &checker, source, target, &OrdinaryDependencies)?
        else {
            anyhow::bail!("context requires checking the completed result");
        };
        assert!(
            pending
                .resume(db, true, &RefuseAfter(Cell::new(0)))
                .is_err()
        );
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
        Ok(())
    })
}

#[test]
fn shared_guard_keeps_cached_and_exact_cycle_bodies_lazy() -> anyhow::Result<()> {
    with_checker(|db, checker| {
        let checker = checker.with_context_collection_disabled();
        let effects = InlinePairEffects {
            db,
            dependencies: OrdinaryDependencies,
        };
        let source = Type::bool_literal(true);
        let target = Type::bool_literal(false);
        let bodies = Cell::new(0);
        let skipped = Cell::new(0);
        let original = checker.with_recursion_guard(db, source, target, || {
            bodies.set(bodies.get() + 1);
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
            let exact = legacy_inline(effects.guard(&checker, source, target, || {
                skipped.set(skipped.get() + 1);
                ready(Ok(checker.never()))
            }));
            assert!(exact.has_same_identity(checker.always()));
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
            checker.never()
        });
        let cached = legacy_inline(effects.guard(&checker, source, target, || {
            skipped.set(skipped.get() + 1);
            ready(Ok(checker.always()))
        }));
        assert!(cached.has_same_identity(original));
        assert_eq!((bodies.get(), skipped.get()), (1, 0));
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
        Ok(())
    })
}

#[test]
fn shared_guard_recomputes_context_without_replacing_cached_answer() -> anyhow::Result<()> {
    with_checker(|db, checker| {
        let quiet = checker.with_context_collection_disabled();
        let source = Type::bool_literal(true);
        let target = Type::bool_literal(false);
        let original = quiet.with_recursion_guard(db, source, target, || checker.never());
        let effects = InlinePairEffects {
            db,
            dependencies: OrdinaryDependencies,
        };
        let bodies = Cell::new(0);
        let skipped = Cell::new(0);
        for _ in 0..2 {
            let recomputed = legacy_inline(effects.guard(&checker, source, target, || {
                bodies.set(bodies.get() + 1);
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 1));
                // A different child answer exposes replacement of the original cached result.
                ready(Ok(checker.always()))
            }));
            assert!(recomputed.has_same_identity(checker.always()));
            let cached = quiet.with_recursion_guard(db, source, target, || {
                skipped.set(skipped.get() + 1);
                checker.always()
            });
            assert!(cached.has_same_identity(original));
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
        }
        assert!(checker.is_context_collection_enabled());
        assert_eq!((bodies.get(), skipped.get()), (2, 0));
        Ok(())
    })
}

#[test]
fn shared_guard_errors_retire_the_active_scope_before_retry() -> anyhow::Result<()> {
    with_checker(|db, checker| {
        let source = Type::bool_literal(true);
        let target = Type::bool_literal(false);
        for (allowed, refuse_child) in [(0, false), (1, true), (1, false)] {
            let effects = InlinePairEffects {
                db,
                dependencies: RefuseAfter(Cell::new(allowed)),
            };
            let body_ran = Cell::new(false);
            let result = try_poll_immediate(effects.guard(&checker, source, target, || {
                body_ran.set(true);
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                ready(if refuse_child {
                    Err(anyhow::anyhow!("child refused"))
                } else {
                    Ok(checker.never())
                })
            }));
            assert!(matches!(result, Poll::Ready(Err(_))));
            assert_eq!(body_ran.get(), allowed != 0);
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
        }
        let result = checker.with_recursion_guard(db, source, target, || checker.never());
        assert!(result.has_same_identity(checker.never()));
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
        Ok(())
    })
}
