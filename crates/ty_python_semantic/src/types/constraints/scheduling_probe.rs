use ruff_python_ast::name::Name;

use super::{ConstraintSet, ConstraintSetBuilder, NodeId, SolutionPaths, Solutions, SourceOrderId};
use crate::db::tests::setup_db;
use crate::types::typevar::{TypeVarNonceGenerator, TypeVarSet};
use crate::types::{
    BoundTypeVarInstance, KnownClass, Type, TypeVarBoundOrConstraints, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ConstraintHandleKey {
    node: NodeId,
    source_order: Option<SourceOrderId>,
}

impl<'db> ConstraintSet<'db, '_> {
    pub(crate) fn scheduling_key(
        self,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Option<ConstraintHandleKey> {
        std::ptr::eq(self.builder, builder).then_some(ConstraintHandleKey {
            node: self.node,
            source_order: self.source_order,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Meaning {
    never: bool,
    no_valid: bool,
    status: &'static str,
    paths: Vec<(bool, Vec<String>, Vec<String>)>,
}

fn meaning<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    set: ConstraintSet<'db, '_>,
    inferable: TypeVarSet<'db>,
) -> Meaning {
    let (status, paths) = match set.solutions(db, env, inferable).unwrap() {
        Solutions::Unconstrained => ("unconstrained", Vec::new()),
        Solutions::Constrained(SolutionPaths::Complete(paths)) => ("constrained", paths),
        Solutions::Unsatisfiable(SolutionPaths::Complete(paths)) => ("unsatisfiable", paths),
        result => panic!("small fixture exhausted its solver budget: {result:?}"),
    };
    Meaning {
        never: set.is_never_satisfied(db, env),
        no_valid: set.has_no_valid_solutions(db, env),
        status,
        paths: paths
            .into_iter()
            .map(|path| {
                let solutions = path
                    .solved_typevars
                    .iter()
                    .map(|binding| binding.solution.display(db, env).to_string())
                    .collect();
                let violations = path
                    .violations()
                    .iter()
                    .map(|violation| {
                        format!(
                            "{:?}: declared {}, argument {}",
                            violation.kind,
                            violation
                                .bound_typevar
                                .require_bound_or_constraints(db, env)
                                .as_type(db, env)
                                .display(db, env),
                            violation
                                .argument
                                .map(|ty| ty.display(db, env).to_string())
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                (path.is_valid(), solutions, violations)
            })
            .collect(),
    }
}

fn synthetic<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::synthetic(db, env, Name::new_static("T"), TypeVarVariance::Invariant)
}

#[test]
fn scheduling_probe_materialized_bound_order() {
    let db = setup_db();
    let env = db.program_environment();
    let original = synthetic(&db, &env).map_bound_or_constraints(&db, |_| {
        Some(TypeVarBoundOrConstraints::UpperBound(Type::any()))
    });
    let original_type = Type::TypeVar(original);
    let top = original_type
        .top_materialization(&db, &env)
        .as_typevar()
        .unwrap();
    let bottom = original_type
        .bottom_materialization(&db, &env)
        .as_typevar()
        .unwrap();
    assert_ne!(top, bottom);
    assert_eq!(top.identity(&db), bottom.identity(&db));
    assert_eq!(
        top.require_bound_or_constraints(&db, &env)
            .as_type(&db, &env),
        Type::object()
    );
    assert_eq!(
        bottom
            .require_bound_or_constraints(&db, &env)
            .as_type(&db, &env),
        Type::Never
    );
    let int = KnownClass::Int.to_instance(&db, &env);
    let orders = [[top, bottom], [bottom, top]];

    for via_relations in [false, true] {
        for subject in [original, top, bottom] {
            let run = |order: [BoundTypeVarInstance<'_>; 2]| {
                let builder = ConstraintSetBuilder::new();
                for item in order {
                    if via_relations {
                        int.when_constraint_set_assignable_to(
                            &db,
                            &env,
                            Type::TypeVar(item),
                            &builder,
                        );
                    } else {
                        builder.storage.borrow_mut().intern_typevar(&db, item);
                    }
                }
                let retained = builder
                    .storage
                    .borrow()
                    .typevars
                    .iter()
                    .next()
                    .copied()
                    .unwrap();
                assert_eq!(retained, order[0]);
                let relation = int.when_constraint_set_assignable_to(
                    &db,
                    &env,
                    Type::TypeVar(subject),
                    &builder,
                );
                let observed = meaning(
                    &db,
                    &env,
                    relation,
                    TypeVarSet::from_typevars(&db, [subject]),
                );
                let eager = int.when_assignable_to(
                    &db,
                    &env,
                    Type::TypeVar(subject),
                    &builder,
                    TypeVarSet::from_typevars(&db, [subject]),
                );
                (
                    observed,
                    meaning(&db, &env, eager, TypeVarSet::from_typevars(&db, [subject])),
                )
            };
            let first = run(orders[0]);
            let second = run(orders[1]);
            assert_eq!(first, second);
            assert_eq!(first.0.no_valid, subject == bottom);
            assert_eq!(first.1.no_valid, subject == bottom);
        }
    }
}

#[test]
fn scheduling_probe_support_membership_order() {
    let db = setup_db();
    let env = db.program_environment();
    let original = synthetic(&db, &env).map_bound_or_constraints(&db, |_| {
        Some(TypeVarBoundOrConstraints::UpperBound(Type::any()))
    });
    let top = Type::TypeVar(original)
        .top_materialization(&db, &env)
        .as_typevar()
        .unwrap();
    let bottom = Type::TypeVar(original)
        .bottom_materialization(&db, &env)
        .as_typevar()
        .unwrap();
    let int = KnownClass::Int.to_instance(&db, &env);
    let run = |warm| {
        let builder = ConstraintSetBuilder::new();
        let _ = int.when_constraint_set_assignable_to(&db, &env, Type::TypeVar(warm), &builder);
        let actual = int.when_constraint_set_assignable_to(&db, &env, Type::TypeVar(top), &builder);
        (
            actual.mentions_typevar(&db, top),
            actual.mentions_typevar(&db, bottom),
            meaning(&db, &env, actual, TypeVarSet::from_typevars(&db, [top])),
        )
    };
    let top_first = run(top);
    let bottom_first = run(bottom);
    assert!(top_first.0);
    assert!(top_first.1);
    assert_eq!(top_first, bottom_first);
}

#[test]
fn scheduling_probe_independent_nonce_domains() {
    let db = setup_db();
    let env = db.program_environment();
    let original = synthetic(&db, &env);
    let first_generator = TypeVarNonceGenerator::default();
    let second_generator = TypeVarNonceGenerator::default();
    let with_nonce = |nonce| {
        BoundTypeVarInstance::new(
            &db,
            original.typevar(&db),
            original.binding_context(&db),
            None,
            nonce,
        )
    };
    let first = with_nonce(first_generator.next());
    let second = with_nonce(second_generator.next());
    let distinct = with_nonce(first_generator.next());
    assert_eq!(first, second);
    assert_ne!(first.identity(&db), distinct.identity(&db));
    let int = KnownClass::Int.to_instance(&db, &env);
    let str = KnownClass::Str.to_instance(&db, &env);
    let request_a = (int, Type::TypeVar(first));
    let request_b = (int, Type::TypeVar(second));
    assert_eq!(request_a, request_b);
    assert_ne!((0_u8, request_a), (1_u8, request_b));

    let independent = |typevar, ty| {
        let builder = ConstraintSetBuilder::new();
        let set =
            ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, typevar, ty);
        let result = meaning(&db, &env, set, TypeVarSet::from_typevars(&db, [typevar]));
        assert!(!result.no_valid);
    };
    independent(first, int);
    independent(second, str);
    for second_typevar in [second, distinct] {
        let run = |reverse| {
            let builder = ConstraintSetBuilder::new();
            let make = |typevar, ty| {
                ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, typevar, ty)
            };
            let (left, right) = if reverse {
                let right = make(second_typevar, str);
                (make(first, int), right)
            } else {
                let left = make(first, int);
                (left, make(second_typevar, str))
            };
            let set = left.and(&db, &builder, || right);
            meaning(
                &db,
                &env,
                set,
                TypeVarSet::from_typevars(&db, [first, second_typevar]),
            )
        };
        let forward = run(false);
        let reverse = run(true);
        assert_eq!(forward, reverse);
        assert_eq!(forward.no_valid, second_typevar == second);
    }
}

#[test]
fn scheduling_probe_handle_identity() {
    let db = setup_db();
    let env = db.program_environment();
    let first = ConstraintSetBuilder::new();
    let second = ConstraintSetBuilder::new();
    let typevar = synthetic(&db, &env);
    let int = KnownClass::Int.to_instance(&db, &env);
    let str = KnownClass::Str.to_instance(&db, &env);
    let a = ConstraintSet::constrain_typevar_lower_bound(&db, &env, &first, typevar, int);
    let b = ConstraintSet::constrain_typevar_upper_bound(&db, &env, &first, typevar, str);
    let ab = a.and(&db, &first, || b);
    let ba = b.and(&db, &first, || a);
    assert_eq!(ab.node, ba.node);
    assert_ne!(ab.source_order, ba.source_order);
    assert_ne!(ab.scheduling_key(&first), ba.scheduling_key(&first));
    assert_eq!(ab.scheduling_key(&first), ab.scheduling_key(&first));
    assert_eq!(ab.scheduling_key(&second), None);
}
