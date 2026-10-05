use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use salsa::Database;
use salsa::execution_probe::RegistryBuilder;
use salsa::prepared_source_probe::assert_no_active_attempt;
use ty_python_core::scope::FileScopeId;

use super::*;
use crate::analysis::{
    AnalysisFailure, AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
    prepare_file, with_analysis_session,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::{Type, todo_type};

const INLINE_PAYLOAD: Type<'static> = todo_type!(
    "narrowing constraint with inline bytes retained in every disjunction and conjunction"
);

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = 1\n").unwrap();
    db
}

fn prepare(db: &TestDb) -> PreparedAnalysisFile<'_> {
    prepare_file(db, system_path_to_file(db, "src/main.py").unwrap()).unwrap()
}

fn combined(ty: Type<'_>, disjuncts: usize) -> NarrowingConstraint<'_> {
    let disjunction = || {
        (0..disjuncts)
            .map(|index| Conjunctions {
                conjuncts: (0..3)
                    .map(|operation| match (index + operation) % 3 {
                        0 => NarrowingOperation::Intersection(ty),
                        1 => NarrowingOperation::GenericFiltering(Type::AlwaysTruthy),
                        _ => NarrowingOperation::Intersection(Type::AlwaysFalsy),
                    })
                    .collect(),
            })
            .collect()
    };
    NarrowingConstraint(NarrowingConstraintKind::Combined(Box::new(
        CombinedNarrowingConstraint {
            intersection_disjuncts: disjunction(),
            replacement_disjuncts: disjunction(),
        },
    )))
}

fn assert_spilled(constraint: &NarrowingConstraint<'_>, count: usize) {
    let NarrowingConstraintKind::Combined(combined) = &constraint.0 else {
        panic!("expected a combined constraint");
    };
    for disjuncts in [
        &combined.intersection_disjuncts,
        &combined.replacement_disjuncts,
    ] {
        assert!(disjuncts.spilled());
        assert_eq!(disjuncts.len(), count);
        assert!(disjuncts.len() > 2);
        for conjunction in disjuncts {
            assert!(conjunction.conjuncts.spilled());
            assert_eq!(conjunction.conjuncts.len(), 3);
        }
    }
}

fn maps<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    disjuncts: usize,
) -> ExpressionNarrowingConstraints<'db> {
    let places = prepared.semantic_index().place_table(FileScopeId::global());
    let entries = || {
        ["left", "right"]
            .map(|name| {
                (
                    ScopedPlaceId::Symbol(places.symbol_id(name).unwrap()),
                    combined(ty, disjuncts),
                )
            })
            .into_iter()
            .collect()
    };
    ExpressionNarrowingConstraints {
        positive: Some(entries()),
        negative: Some(entries()),
    }
}

fn controlled_clone<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    constraint: &NarrowingConstraint<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<NarrowingConstraint<'db>>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move { clone_constraint(&endpoint, constraint).await })
    })
}

fn controlled_comparison<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    left: &ExpressionNarrowingConstraints<'db>,
    right: &ExpressionNarrowingConstraints<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move {
            let work = comparison_work(&endpoint, left, right).await?;
            let equal = endpoint
                .local_call(|| {
                    endpoint.admit_work(work)?;
                    endpoint.check_completion()?;
                    Ok(left == right)
                })
                .await;
            Ok((work, equal))
        })
    })
}

#[test]
fn spilled_constraint_clones_refuse_and_retry_in_the_same_revision() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let constraint = combined(INLINE_PAYLOAD, 3);
    assert_spilled(&constraint, 3);
    assert_eq!(
        INLINE_PAYLOAD.inline_payload_bytes() > 0,
        cfg!(debug_assertions)
    );

    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        assert_eq!(
            controlled_clone(&prepared, &constraint, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            }),
        );
        assert_no_active_attempt();
        let result = controlled_clone(&prepared, &constraint, &funded());
        let Ok(AnalysisOutcome::Complete(cloned)) = result else {
            panic!("{result:?}");
        };
        assert_eq!(cloned, constraint);
        assert_spilled(&cloned, 3);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn comparison_visits_both_maps_and_all_spilled_payloads() {
    let db = fixture();
    let prepared = prepare(&db);
    let small = maps(&prepared, Type::unknown(), 3);
    let larger = maps(&prepared, Type::unknown(), 5);
    let inline = maps(&prepared, INLINE_PAYLOAD, 3);
    let quote = |value| {
        let result = controlled_comparison(&prepared, value, value, &funded());
        let Ok(AnalysisOutcome::Complete((work, equal))) = result else {
            panic!("{result:?}");
        };
        assert!(equal);
        work
    };
    let small_work = quote(&small);
    assert!(quote(&larger) > small_work);
    if cfg!(debug_assertions) {
        // Both compared values have two maps, each with two entries. Every entry retains
        // the inline payload in three intersection and three replacement conjunctions.
        assert!(
            quote(&inline) >= small_work + 2 * 2 * 2 * 6 * INLINE_PAYLOAD.inline_payload_bytes()
        );
    }
    assert_eq!(
        controlled_comparison(
            &prepared,
            &inline,
            &inline,
            &AnalysisPolicy {
                semantic_work_limit: 0,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_no_active_attempt();

    for positive in [true, false] {
        let mut changed = maps(&prepared, Type::unknown(), 3);
        let selected = if positive {
            &mut changed.positive
        } else {
            &mut changed.negative
        };
        let map = selected.as_mut().unwrap();
        let (_, constraint) = map.iter_mut().last().unwrap();
        let NarrowingConstraintKind::Combined(combined) = &mut constraint.0 else {
            panic!("expected a combined constraint");
        };
        let conjunction = combined.replacement_disjuncts.last_mut().unwrap();
        *conjunction.conjuncts.last_mut().unwrap() = NarrowingOperation::Intersection(Type::Never);
        let result = controlled_comparison(&prepared, &small, &changed, &funded());
        assert!(
            matches!(result, Ok(AnalysisOutcome::Complete((_, false)))),
            "{result:?}"
        );
        assert_no_active_attempt();
    }
}

#[derive(Default)]
struct CloneProgress {
    completed: Cell<bool>,
    returned: Cell<bool>,
    retired: Cell<Option<[usize; 2]>>,
}

struct OwnedClone<'owner, 'db> {
    constraint: Option<NarrowingConstraint<'db>>,
    progress: &'owner CloneProgress,
}

impl Drop for OwnedClone<'_, '_> {
    fn drop(&mut self) {
        if let Some(constraint) = self.constraint.take() {
            let lengths = match &constraint.0 {
                NarrowingConstraintKind::Combined(combined) => [
                    combined.intersection_disjuncts.len(),
                    combined.replacement_disjuncts.len(),
                ],
                _ => [0, 0],
            };
            drop(constraint);
            self.progress.retired.set(Some(lengths));
        }
    }
}

#[test]
fn native_cancellation_retires_a_completed_spilled_clone() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let constraint = combined(INLINE_PAYLOAD, 3);
    assert_spilled(&constraint, 3);
    let progress = CloneProgress::default();
    let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        with_analysis_session(&prepared, &funded(), |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            let constraint = &constraint;
            let progress = &progress;
            run.run(|endpoint| async move {
                let owned = OwnedClone {
                    constraint: Some(clone_constraint(&endpoint, constraint).await?),
                    progress,
                };
                progress.completed.set(true);
                endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        endpoint.check_completion()
                    })
                    .await;
                progress.returned.set(true);
                drop(owned);
                Ok(())
            })
        })
    }));
    assert!(
        matches!(outcome, Err(salsa::Cancelled::Local)),
        "{outcome:?}"
    );
    assert!(progress.completed.get());
    assert!(!progress.returned.get());
    assert_eq!(progress.retired.get(), Some([3, 3]));
    assert_no_active_attempt();
    assert_eq!(
        controlled_clone(&prepared, &constraint, &funded()),
        Ok(AnalysisOutcome::Complete(constraint)),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
