use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use salsa::execution_probe::RegistryBuilder;
use salsa::prepared_source_probe::assert_no_active_attempt;

use super::*;
use crate::analysis::{
    AnalysisFailure, AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
    prepare_file, with_analysis_session,
};
use crate::db::tests::{TestDb, setup_db};
use crate::types::{Type, todo_type};

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "pass\n").unwrap();
    db
}

fn prepare(db: &TestDb) -> PreparedAnalysisFile<'_> {
    prepare_file(db, system_path_to_file(db, "src/main.py").unwrap()).unwrap()
}

fn disjunction<'db>(
    count: usize,
    width: usize,
    duplicates: bool,
) -> SmallVec<[Conjunctions<'db>; 1]> {
    (0..count)
        .map(|index| Conjunctions {
            conjuncts: (0..width)
                .map(|operation| {
                    let value = if duplicates {
                        operation
                    } else {
                        index * width + operation
                    };
                    let ty = Type::int_literal(value as i64);
                    if operation % 2 == 0 {
                        NarrowingOperation::Intersection(ty)
                    } else {
                        NarrowingOperation::GenericFiltering(ty)
                    }
                })
                .collect(),
        })
        .collect()
}

fn constraint<'db>(count: usize, width: usize, duplicates: bool) -> NarrowingConstraint<'db> {
    NarrowingConstraint::from_disjuncts(disjunction(count, width, duplicates), SmallVec::new())
}

fn shapes<'db>() -> Vec<NarrowingConstraint<'db>> {
    vec![
        NarrowingConstraint::default(),
        NarrowingConstraint::intersection(Type::AlwaysTruthy),
        NarrowingConstraint::replacement(Type::AlwaysFalsy),
        constraint(3, 3, false),
        NarrowingConstraint::from_disjuncts(disjunction(3, 3, true), disjunction(3, 3, false)),
        NarrowingConstraint::from_disjuncts(
            [Conjunctions {
                conjuncts: [
                    NarrowingOperation::Intersection(Type::Never),
                    NarrowingOperation::GenericFiltering(Type::AlwaysTruthy),
                ]
                .into_iter()
                .collect(),
            }]
            .into_iter()
            .collect(),
            SmallVec::new(),
        ),
    ]
}

fn controlled_and<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    left: &NarrowingConstraint<'db>,
    right: &NarrowingConstraint<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<NarrowingConstraint<'db>>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move {
            let right = super::super::clone_constraint(&endpoint, right).await?;
            merge_and(&endpoint, left, right).await
        })
    })
}

fn controlled_or<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    left: &mut NarrowingConstraint<'db>,
    right: &NarrowingConstraint<'db>,
    policy: &AnalysisPolicy,
    entered: &Cell<bool>,
) -> Result<AnalysisOutcome<()>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        let left = &mut *left;
        run.run(|endpoint| async move {
            let right = super::super::clone_constraint(&endpoint, right).await?;
            entered.set(true);
            merge_or(&endpoint, left, right).await
        })
    })
}

fn quote<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    left: &NarrowingConstraint<'db>,
    right: &NarrowingConstraint<'db>,
    and: bool,
) -> (usize, usize) {
    let result = with_analysis_session(prepared, &funded(), |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move {
            let left = measure(&endpoint, left).await?;
            let right = measure(&endpoint, right).await?;
            let quote = endpoint
                .local_call(|| {
                    admit(&endpoint, 256, 0)?;
                    if and {
                        and_quote(&left, &right)
                    } else {
                        or_quote(&left, &right)
                    }
                })
                .await;
            Ok((quote.work, quote.bytes))
        })
    });
    let Ok(AnalysisOutcome::Complete(quote)) = result else {
        panic!("{result:?}");
    };
    quote
}

#[test]
fn admitted_merges_preserve_all_constraint_shapes_and_operand_order() {
    let db = fixture();
    let prepared = prepare(&db);
    for left in shapes() {
        for right in shapes() {
            let expected = left.merge_constraint_and(right.clone());
            assert_eq!(
                controlled_and(&prepared, &left, &right, &funded()),
                Ok(AnalysisOutcome::Complete(expected))
            );
            let mut expected = left.clone();
            expected.merge_constraint_or(right.clone());
            let mut actual = left.clone();
            assert_eq!(
                controlled_or(&prepared, &mut actual, &right, &funded(), &Cell::new(false)),
                Ok(AnalysisOutcome::Complete(()))
            );
            assert_eq!(actual, expected);
            assert_no_active_attempt();
        }
    }
    let unused = constraint(4096, 64, false);
    for right in [
        NarrowingConstraint::default(),
        NarrowingConstraint::replacement(Type::AlwaysFalsy),
    ] {
        assert_eq!(
            controlled_and(
                &prepared,
                &unused,
                &right,
                &AnalysisPolicy {
                    semantic_work_limit: 10_000,
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Complete(right))
        );
        assert_no_active_attempt();
    }
}

#[test]
fn merge_bounds_charge_discarded_candidates_inline_payloads_and_retained_backing() {
    let db = fixture();
    let prepared = prepare(&db);
    let mut previous = (0, 0);
    for count in [1, 2, 4, 8, 16] {
        let repeated = constraint(count, 3, true);
        let current = quote(&prepared, &repeated, &repeated, true);
        assert!(current.0 > previous.0 && current.1 > previous.1);
        let expected = repeated.merge_constraint_and(repeated.clone());
        assert_eq!(expected, constraint(1, 3, true));
        let outcome = controlled_and(&prepared, &repeated, &repeated, &funded());
        if count <= 4 {
            assert_eq!(outcome, Ok(AnalysisOutcome::Complete(expected)));
        } else {
            assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        eprintln!(
            "duplicate merge: disjuncts={count}, work={}, requested_bytes={}, complete={}",
            current.0,
            current.1,
            matches!(outcome, Ok(AnalysisOutcome::Complete(_)))
        );
        previous = current;
    }
    for count in [1, 2, 4, 8, 16] {
        let distinct = constraint(count, 3, false);
        let current = quote(&prepared, &distinct, &distinct, true);
        let expected = distinct.merge_constraint_and(distinct.clone());
        let retained = expected.disjuncts(true).count();
        assert_eq!(retained, count * count);
        let outcome = controlled_and(&prepared, &distinct, &distinct, &funded());
        if count <= 4 {
            assert_eq!(outcome, Ok(AnalysisOutcome::Complete(expected)));
        } else {
            assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        eprintln!(
            "distinct merge: disjuncts={count}, retained={retained}, work={}, requested_bytes={}, complete={}",
            current.0,
            current.1,
            matches!(outcome, Ok(AnalysisOutcome::Complete(_)))
        );
    }
    let ordinary = constraint(3, 3, false);
    let mut inline = ordinary.clone();
    let NarrowingConstraintKind::Combined(combined) = &mut inline.0 else {
        panic!("combined fixture");
    };
    combined.intersection_disjuncts[0].conjuncts[0] = NarrowingOperation::Intersection(todo_type!(
        "inline comparison payload repeated by each candidate and deduplication comparison"
    ));
    if cfg!(debug_assertions) {
        assert!(
            quote(&prepared, &inline, &inline, true).0
                > quote(&prepared, &ordinary, &ordinary, true).0
        );
    }
    let mut spare = ordinary.clone();
    let NarrowingConstraintKind::Combined(combined) = &mut spare.0 else {
        panic!("combined fixture");
    };
    combined.intersection_disjuncts.reserve(64);
    assert!(
        quote(&prepared, &spare, &ordinary, false).0
            > quote(&prepared, &ordinary, &ordinary, false).0
    );
    assert_no_active_attempt();
}

#[test]
fn refused_merge_preserves_its_destination_and_retries_in_the_same_revision() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let left = constraint(32, 3, false);
    let right = constraint(1, 3, false);
    let mut expected = left.clone();
    expected.merge_constraint_or(right.clone());
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: 4_000,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: 4_096,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let mut actual = left.clone();
        let entered = Cell::new(false);
        assert_eq!(
            controlled_or(&prepared, &mut actual, &right, &policy, &entered),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert!(entered.get());
        assert_eq!(actual, left);
        assert_no_active_attempt();
        assert_eq!(
            controlled_or(&prepared, &mut actual, &right, &funded(), &Cell::new(false)),
            Ok(AnalysisOutcome::Complete(()))
        );
        assert_eq!(actual, expected);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

struct OwnedResult<'owner, 'db> {
    value: Option<NarrowingConstraint<'db>>,
    retired: &'owner Cell<bool>,
}

impl Drop for OwnedResult<'_, '_> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            drop(value);
            self.retired.set(true);
        }
    }
}

#[test]
fn cancellation_retires_an_allocated_merge_result_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let left = constraint(3, 3, false);
    let right = constraint(3, 3, true);
    let completed = Cell::new(false);
    let retired = Cell::new(false);
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        with_analysis_session(&prepared, &funded(), |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            let (left, right, completed, retired) = (&left, &right, &completed, &retired);
            run.run(|endpoint| async move {
                let right = super::super::clone_constraint(&endpoint, right).await?;
                let value = merge_and(&endpoint, left, right).await?;
                assert!(matches!(value.0, NarrowingConstraintKind::Combined(_)));
                let owned = OwnedResult {
                    value: Some(value),
                    retired,
                };
                completed.set(true);
                endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        endpoint.check_completion()
                    })
                    .await;
                drop(owned);
                Ok(())
            })
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(completed.get() && retired.get());
    assert_no_active_attempt();
    assert_eq!(
        controlled_and(&prepared, &left, &right, &funded()),
        Ok(AnalysisOutcome::Complete(left.merge_constraint_and(right)))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
