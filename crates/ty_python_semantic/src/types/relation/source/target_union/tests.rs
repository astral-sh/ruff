use salsa::execution_probe::RegistryBuilder;

use super::*;
use crate::db::tests::setup_db;
use crate::types::ErrorContextTree;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::constructor::expansion_probe;
use crate::types::relation::RelationOwners;
use crate::types::relation::source::UnavailablePairs;
use crate::types::relation::source::disjoint_guard::tests::{
    Effects, Failure, make_admission, take_refused_operation,
};
use crate::types::relation::target_union::check_target_union_with;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::typevar::TypeVarSet;

#[test]
fn enabled_context_refuses_before_union_fields_or_child_comparisons() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let mut checker = owners.assignability(TypeVarSet::None);
    checker.context_tree = Some(ErrorContextTree::new(checker.relation));
    let target = UnionType::new(
        &db,
        vec![Type::AlwaysTruthy, Type::AlwaysFalsy].into_boxed_slice(),
        RecursivelyDefined::No,
    );
    let admission = make_admission(&db, Failure::Refuse);
    take_refused_operation();
    let result = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builder = &builder;
        let checker = &checker;
        let result = RegistryBuilder::new(db, &admission)?
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
                check_target_union_with(
                    Type::bool_literal(true),
                    target,
                    &BorrowedTargetUnion::new(&pairs, checker),
                )
                .await
            });
        assert!(matches!(
            result,
            Err(RunError::Contract(
                "guard test reached an unavailable operation"
            ))
        ));
        result
    })
    .0;
    assert!(matches!(
        result,
        Err(expansion_probe::Incomplete::Interrupted)
    ));
    assert_eq!(
        take_refused_operation(),
        Some(RelationSourceOperation::TargetUnionContext)
    );
    assert!(
        checker
            .report_context()
            .is_some_and(ErrorContextTree::is_empty)
    );
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
}
