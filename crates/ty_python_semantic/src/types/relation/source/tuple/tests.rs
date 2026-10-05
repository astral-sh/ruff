//! Controlled tuple tests observe named refusals and cleanup through the production adapter.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ruff_python_ast::name::Name;
use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder};

use super::*;
use crate::Db;
use crate::db::tests::setup_db;
use crate::types::ErrorContextTree;
use crate::types::TypeVarVariance;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::constructor::expansion_probe;
use crate::types::relation::source::UnavailablePairs;
use crate::types::relation::source::disjoint_guard::tests::{
    Admission, Effects, Failure, make_admission, take_refused_operation,
};
use crate::types::relation::{DisjointnessChecker, HasRelationToVisitor, RelationOwners};
use crate::types::tuple::relation::next_normalized_with;
use crate::types::typevar::TypeVarSet;

/// Selects an admission charge after the second element comparison has begun.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Charge {
    Work,
    Bytes,
}

/// Forwards the existing refusal/cancellation harness only the selected armed charge.
struct SelectedAdmission<'db> {
    inner: Admission<'db>,
    charge: Charge,
}

impl ExecutionAdmission for SelectedAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let selected = match (self.charge, work) {
            (Charge::Work, ExecutionWork::Work { .. })
            | (Charge::Bytes, ExecutionWork::Resource { .. }) => true,
            (
                Charge::Work,
                ExecutionWork::Task { .. } | ExecutionWork::Resource { .. } | ExecutionWork::Poll,
            )
            | (
                Charge::Bytes,
                ExecutionWork::Task { .. } | ExecutionWork::Work { .. } | ExecutionWork::Poll,
            ) => false,
        };
        if self.inner.armed.get() && !selected {
            return Ok(());
        }
        self.inner.admit(work)
    }
}

/// Observes child retirement while the enclosing tuple relation guard still owns its active entry.
struct ChildLifetime<'a, 'db, 'c> {
    visitor: &'a HasRelationToVisitor<'db, 'c>,
    live: &'a Cell<usize>,
    retired: &'a Cell<usize>,
    guard_at_retirement: &'a Cell<Option<(usize, usize)>>,
}

impl Drop for ChildLifetime<'_, '_, '_> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
        self.retired.set(self.retired.get() + 1);
        self.guard_at_retirement
            .set(Some(self.visitor.ownership_probe_counts()));
    }
}

/// Supplies child constraints and can interrupt the second comparison.
struct Children<'state, 'db, 'c> {
    builder: &'c ConstraintSetBuilder<'db>,
    results: [ConstraintSet<'db, 'c>; 2],
    arm_second: Option<&'state Cell<bool>>,
    started: Cell<usize>,
    completed: Cell<usize>,
    live: Cell<usize>,
    retired: Cell<usize>,
    guard_at_retirement: Cell<Option<(usize, usize)>>,
}

impl<'state, 'db, 'c> Children<'state, 'db, 'c> {
    fn new(
        builder: &'c ConstraintSetBuilder<'db>,
        results: [ConstraintSet<'db, 'c>; 2],
        arm_second: Option<&'state Cell<bool>>,
    ) -> Self {
        Self {
            builder,
            results,
            arm_second,
            started: Cell::new(0),
            completed: Cell::new(0),
            live: Cell::new(0),
            retired: Cell::new(0),
            guard_at_retirement: Cell::new(None),
        }
    }
}

impl<'run, 'db: 'run + 'c, 'c> PairChildren<'run, 'db, 'c> for Children<'_, 'db, 'c> {
    async fn pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        assert!(std::ptr::eq(checker.constraints, self.builder));
        let index = self.started.replace(self.started.get() + 1);
        let expected_pair = match index {
            0 => (Type::int_literal(1), Type::int_literal(3)),
            1 => (Type::int_literal(2), Type::int_literal(4)),
            _ => return Err(RunError::Contract("tuple replayed an element comparison")),
        };
        assert_eq!((source, target), expected_pair);
        let Some(result) = self.results.get(index).copied() else {
            return Err(RunError::Contract("tuple requested an extra child result"));
        };
        self.live.set(self.live.get() + 1);
        let _lifetime = ChildLifetime {
            visitor: checker.relation_visitor,
            live: &self.live,
            retired: &self.retired,
            guard_at_retirement: &self.guard_at_retirement,
        };
        let endpoint = effects.endpoint();
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                if index == 1
                    && let Some(armed) = self.arm_second
                {
                    armed.set(true);
                }
                Ok(())
            })
            .await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes: 1 })?;
                endpoint.check_completion()
            })
            .await;
        self.completed.set(self.completed.get() + 1);
        Ok(result)
    }

    async fn disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &DisjointnessChecker<'_, 'c, 'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }

    async fn derived_disjoint_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &TypeRelationChecker<'_, 'c, 'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }

    async fn derived_subtyping_pair<E: RelationSourceEffects<'run, 'db>>(
        &self,
        _db: &'db dyn Db,
        effects: &E,
        _checker: &DisjointnessChecker<'_, 'c, 'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        effects
            .unavailable(RelationSourceOperation::RecursivePair)
            .await
    }
}

/// Verifies work/byte refusal and native cancellation drop the active tuple work before a clean retry.
#[test_case::test_case(Failure::Refuse, Charge::Work; "work refusal")]
#[test_case::test_case(Failure::Refuse, Charge::Bytes; "byte refusal")]
#[test_case::test_case(Failure::Cancel, Charge::Work; "work cancellation")]
#[test_case::test_case(Failure::Cancel, Charge::Bytes; "byte cancellation")]
fn interrupted_second_element_releases_the_guard_and_original_builder(
    failure: Failure,
    charge: Charge,
) -> anyhow::Result<()> {
    let db = setup_db();
    let retry_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let storage_constraints = ["T", "U"].map(|name| {
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
    let expected = checker.always();
    let results = [expected; 2];
    let source_elements = [Type::int_literal(1), Type::int_literal(2)];
    let target_spec = TupleSpec::heterogeneous([Type::int_literal(3), Type::int_literal(4)]);
    let source = Type::heterogeneous_tuple(&db, &env, source_elements);
    let target = Type::tuple(TupleType::new(&db, &env, &target_spec));
    let admission = SelectedAdmission {
        inner: make_admission(&db, failure),
        charge,
    };
    let children = Children::new(&builder, results, Some(&admission.inner.armed));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let builder = &builder;
            let checker = &checker;
            let children = &children;
            let source_elements = &source_elements;
            let target_spec = &target_spec;
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(|endpoint| async move {
                    let effects = Effects { endpoint };
                    let pairs = BorrowedPairs {
                        children,
                        db,
                        endpoint: &effects.endpoint,
                        effects: &effects,
                        constraints: builder,
                    };
                    pairs
                        .guard(checker, source, target, || async {
                            check_fixed_pair_with(
                                source_elements,
                                target_spec,
                                &BorrowedTuplePairs {
                                    pairs: &pairs,
                                    checker,
                                },
                                TupleRelationFacts,
                            )
                            .await
                        })
                        .await
                })
        })
        .0
    }));
    assert!(admission.inner.fired.get());
    match failure {
        Failure::Refuse => {
            let Ok(result) = outcome else {
                anyhow::bail!("allowance refusal must not unwind");
            };
            assert!(matches!(
                result,
                Err(expansion_probe::Incomplete::Allowance)
            ));
        }
        Failure::Cancel => {
            let Err(payload) = outcome else {
                anyhow::bail!("native cancellation must unwind");
            };
            let Some(salsa::Cancelled::Local) = payload.downcast_ref::<salsa::Cancelled>() else {
                anyhow::bail!("unexpected cancellation payload");
            };
        }
    }
    assert_eq!(children.started.get(), 2);
    assert_eq!(children.completed.get(), 1);
    assert_eq!(children.live.get(), 0);
    assert_eq!(children.retired.get(), 2);
    assert_eq!(children.guard_at_retirement.get(), Some((1, 0)));
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    let retry_db = match failure {
        Failure::Refuse => &db,
        Failure::Cancel => &retry_db,
    };
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
    // A new combination needs mutable access to the same builder after interrupted fold cleanup.
    //
    assert!(
        !storage_constraints[0]
            .or(retry_db, &builder, || storage_constraints[1])
            .is_trivially_always_satisfied()
    );
    let children = Children::new(&builder, results, None);
    let admission = make_admission(retry_db, Failure::Refuse);
    let retried = expansion_probe::run(retry_db, usize::MAX, || {
        let builder = &builder;
        let checker = &checker;
        let children = &children;
        let source_elements = &source_elements;
        let target_spec = &target_spec;
        RegistryBuilder::new(retry_db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = BorrowedPairs {
                    children,
                    db: retry_db,
                    endpoint: &effects.endpoint,
                    effects: &effects,
                    constraints: builder,
                };
                pairs
                    .guard(checker, source, target, || async {
                        check_fixed_pair_with(
                            source_elements,
                            target_spec,
                            &BorrowedTuplePairs {
                                pairs: &pairs,
                                checker,
                            },
                            TupleRelationFacts,
                        )
                        .await
                    })
                    .await
            })
    })
    .0;
    let Ok(Ok(actual)) = retried else {
        anyhow::bail!("retry failed: {retried:?}");
    };
    assert!(actual.ownership_probe_same_set(expected));
    assert_eq!(children.started.get(), 2);
    assert_eq!(children.completed.get(), 2);
    assert_eq!(children.live.get(), 0);
    assert_eq!(children.retired.get(), 2);
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
    assert_eq!(salsa::plumbing::current_revision(retry_db), revision);
    Ok(())
}

/// Selects controlled operations that are deliberately unavailable at their semantic boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    LengthContext,
    ElementContext,
    SuppressedContext,
    GradualArity,
    Prenormalization,
    EmptyProtocol,
    ConstraintFold,
}

impl Boundary {
    const fn operation(self) -> RelationSourceOperation {
        match self {
            Self::LengthContext | Self::ElementContext | Self::SuppressedContext => {
                RelationSourceOperation::TupleContext
            }
            Self::GradualArity => RelationSourceOperation::TupleGradualArity,
            Self::Prenormalization => RelationSourceOperation::TuplePrenormalization,
            Self::EmptyProtocol => RelationSourceOperation::TupleProtocol,
            Self::ConstraintFold => RelationSourceOperation::ConstraintFold,
        }
    }
}

/// Verifies unavailable tuple dependencies refuse with their exact operation name before returning a value.
#[test_case::test_case(Boundary::LengthContext; "length context")]
#[test_case::test_case(Boundary::ElementContext; "element context")]
#[test_case::test_case(Boundary::SuppressedContext; "suppressed context")]
#[test_case::test_case(Boundary::GradualArity; "gradual arity")]
#[test_case::test_case(Boundary::Prenormalization; "prenormalization")]
#[test_case::test_case(Boundary::EmptyProtocol; "empty protocol")]
#[test_case::test_case(Boundary::ConstraintFold; "nonterminal constraint fold")]
fn unavailable_tuple_dependency_keeps_its_named_boundary(boundary: Boundary) -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let mut checker = owners.assignability(TypeVarSet::None);
    checker.context_tree = Some(ErrorContextTree::new(checker.relation));
    let admission = make_admission(&db, Failure::Refuse);
    let variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let nonterminal = ConstraintSet::constrain_typevar_equivalence_bound(
        &db,
        &env,
        &builder,
        variable,
        Type::bool_literal(true),
    );
    assert!(!nonterminal.is_source_free_terminal());
    let target_spec = TupleSpec::heterogeneous([Type::int_literal(1)]);
    let TupleSpec::Variable(source) = VariableLengthTuple::mixed(
        [],
        VariableSegment::Homogeneous(Type::any()),
        [Type::int_literal(2)],
    ) else {
        anyhow::bail!("the variable tuple fixture has no variable segment");
    };
    take_refused_operation();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builder = &builder;
        let checker = &checker;
        let target_spec = &target_spec;
        let source = &source;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                let effects = Effects { endpoint };
                let pairs = BorrowedPairs {
                    children: &UnavailablePairs,
                    db,
                    endpoint: &effects.endpoint,
                    effects: &effects,
                    constraints: builder,
                };
                let tuple = BorrowedTuplePairs {
                    pairs: &pairs,
                    checker,
                };
                let result: RunResult<()> = match boundary {
                    Boundary::LengthContext => {
                        check_fixed_pair_with(&[], target_spec, &tuple, TupleRelationFacts)
                            .await
                            .map(|_| ())
                    }
                    Boundary::ElementContext => {
                        tuple
                            .report_element(Type::int_literal(1), Type::int_literal(2), 1, 1)
                            .await
                    }
                    Boundary::SuppressedContext => tuple
                        .pair_without_context(Type::object(), Type::int_literal(1))
                        .await
                        .map(|_| ()),
                    Boundary::GradualArity => {
                        check_variable_pair_with(source, target_spec, &tuple, TupleRelationFacts)
                            .await
                            .map(|_| ())
                    }
                    Boundary::Prenormalization => {
                        let mut cursor = tuple
                            .normalized_start(source, None, NormalizedPart::Prefix)
                            .await?;
                        next_normalized_with(&mut cursor, &tuple).await.map(|_| ())
                    }
                    Boundary::EmptyProtocol => tuple.empty_protocol().await.map(|_| ()),
                    Boundary::ConstraintFold => {
                        let mut fold = tuple.fold_start().await?;
                        tuple.fold_push(&mut fold, nonterminal).await.map(|_| ())
                    }
                };
                assert!(matches!(
                    result,
                    Err(RunError::Contract(
                        "guard test reached an unavailable operation"
                    ))
                ));
                result
            })
    })
    .0;
    assert!(matches!(result, Err(expansion_probe::Incomplete::Interrupted)));
    assert_eq!(take_refused_operation(), Some(boundary.operation()));
    assert!(
        checker
            .report_context()
            .is_some_and(ErrorContextTree::is_empty)
    );
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    Ok(())
}
